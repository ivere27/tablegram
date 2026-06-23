//! Minimal static-file server for the local WASM demo.
//!
//! It exists to serve checked demo assets during development without adding a
//! web framework dependency to the crate.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::thread;

const MAX_SERVE_FILE_BYTES: u64 = 16 * 1024 * 1024;

fn main() -> std::io::Result<()> {
    let args = parse_args();
    let addr = format!("{}:{}", args.host, args.port);
    let listener = TcpListener::bind(&addr)?;
    println!(
        "tablegram-web-server serving {} at http://{}:{}",
        args.root.display(),
        args.host,
        args.port
    );

    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                let root = args.root.clone();
                thread::spawn(move || {
                    handle_client(stream, root);
                });
            }
            Err(err) => {
                eprintln!("incoming connection error: {err}");
            }
        }
    }
    Ok(())
}

#[derive(Clone)]
struct Args {
    root: PathBuf,
    host: String,
    port: u16,
}

fn parse_args() -> Args {
    let mut root = PathBuf::from("web");
    let mut host = String::from("127.0.0.1");
    let mut port = 4173u16;

    let mut iter = env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--root" => {
                if let Some(value) = iter.next() {
                    root = PathBuf::from(value);
                }
            }
            "--host" => {
                if let Some(value) = iter.next() {
                    host = value;
                }
            }
            "--port" => {
                if let Some(value) = iter.next() {
                    if let Ok(parsed) = value.parse::<u16>() {
                        port = parsed;
                    } else {
                        eprintln!("invalid --port value: {value}");
                    }
                }
            }
            _ => {}
        }
    }

    Args { root, host, port }
}

fn handle_client(mut stream: TcpStream, root: PathBuf) {
    let mut buffer = [0u8; 16 * 1024];
    let len = match stream.read(&mut buffer) {
        Ok(0) | Err(_) => return,
        Ok(n) => n,
    };

    let request_line = String::from_utf8_lossy(&buffer[..len]);
    let mut line_iter = request_line.lines();
    let first_line = match line_iter.next() {
        Some(line) => line,
        None => return,
    };

    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let raw_path = parts.next().unwrap_or("/");
    let version = parts.next().unwrap_or("");

    if version.is_empty() {
        send_response(
            &mut stream,
            400,
            "Bad Request",
            "text/plain; charset=utf-8",
            b"malformed request",
            true,
        );
        return;
    }

    match method {
        "GET" | "HEAD" => {
            let head_only = method == "HEAD";
            serve_path(&mut stream, &root, raw_path, head_only);
        }
        _ => {
            send_response(
                &mut stream,
                405,
                "Method Not Allowed",
                "text/plain; charset=utf-8",
                b"only GET/HEAD supported",
                true,
            );
        }
    }
}

fn serve_path(stream: &mut TcpStream, root: &Path, raw_path: &str, head_only: bool) {
    let root = match root.canonicalize() {
        Ok(root) => root,
        Err(_) => {
            send_response(
                stream,
                500,
                "Internal Server Error",
                "text/plain; charset=utf-8",
                b"web root is not readable",
                !head_only,
            );
            return;
        }
    };

    let relative = match request_relative_path(raw_path) {
        Ok(path) => path,
        Err(RequestPathError::BadEncoding) => {
            send_response(
                stream,
                400,
                "Bad Request",
                "text/plain; charset=utf-8",
                b"invalid path encoding",
                !head_only,
            );
            return;
        }
        Err(RequestPathError::Forbidden) => {
            send_response(
                stream,
                403,
                "Forbidden",
                "text/plain; charset=utf-8",
                b"invalid path",
                !head_only,
            );
            return;
        }
    };

    let path = match root.join(relative).canonicalize() {
        Ok(path) => path,
        Err(_) => {
            send_response(
                stream,
                404,
                "Not Found",
                "text/plain; charset=utf-8",
                b"not found",
                !head_only,
            );
            return;
        }
    };

    if !path.starts_with(&root) {
        send_response(
            stream,
            403,
            "Forbidden",
            "text/plain; charset=utf-8",
            b"invalid path",
            !head_only,
        );
        return;
    }

    if path.is_dir() {
        let index = path.join("index.html");
        if index.exists() && index.is_file() {
            serve_file(stream, &index, "text/html; charset=utf-8", head_only);
        } else {
            list_directory(stream, &path, &root, head_only);
        }
        return;
    }

    if !path.exists() || !path.is_file() {
        send_response(
            stream,
            404,
            "Not Found",
            "text/plain; charset=utf-8",
            b"not found",
            !head_only,
        );
        return;
    }

    let mime = content_type_for(&path);
    serve_file(stream, &path, mime, head_only);
}

fn serve_file(stream: &mut TcpStream, path: &Path, mime: &str, head_only: bool) {
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(_) => {
            send_response(
                stream,
                404,
                "Not Found",
                "text/plain; charset=utf-8",
                b"not found",
                !head_only,
            );
            return;
        }
    };

    let content_len = match file.metadata() {
        Ok(metadata) => metadata.len(),
        Err(_) => 0,
    };
    if content_len > MAX_SERVE_FILE_BYTES {
        send_response(
            stream,
            413,
            "Payload Too Large",
            "text/plain; charset=utf-8",
            b"file too large",
            !head_only,
        );
        return;
    }

    let mut body = Vec::new();
    if !head_only {
        let mut limited = std::io::Read::by_ref(&mut file).take(MAX_SERVE_FILE_BYTES + 1);
        if limited.read_to_end(&mut body).is_err() {
            send_response(
                stream,
                500,
                "Internal Server Error",
                "text/plain; charset=utf-8",
                b"failed reading file",
                !head_only,
            );
            return;
        }
        if body.len() as u64 > MAX_SERVE_FILE_BYTES {
            send_response(
                stream,
                413,
                "Payload Too Large",
                "text/plain; charset=utf-8",
                b"file too large",
                !head_only,
            );
            return;
        }
    }

    let mut headers = String::new();
    let _ = write!(
        headers,
        "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        if head_only { content_len } else { body.len() as u64 }
    );
    if stream.write_all(headers.as_bytes()).is_err() {
        return;
    }
    if !head_only {
        let _ = stream.write_all(&body);
    }
}

fn list_directory(stream: &mut TcpStream, path: &Path, root: &Path, head_only: bool) {
    let mut entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(_) => {
            send_response(
                stream,
                404,
                "Not Found",
                "text/plain; charset=utf-8",
                b"not found",
                !head_only,
            );
            return;
        }
    };

    let mut items = Vec::new();
    let prefix = match path.strip_prefix(root) {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(_) => String::new(),
    };
    let base = if prefix.is_empty() || prefix == "." {
        String::from("/")
    } else {
        format!("/{}", prefix)
    };

    while let Some(Ok(entry)) = entries.next() {
        if let Ok(name) = entry.file_name().into_string() {
            let full = match entry.file_type() {
                Ok(ft) if ft.is_dir() => format!("{}/", name),
                _ => name,
            };
            let separator = if base.ends_with('/') { "" } else { "/" };
            let href = html_escape(&format!("{base}{separator}{full}"));
            let label = html_escape(&full);
            items.push(format!("<li><a href=\"{href}\">{label}</a></li>"));
        }
    }

    let body = format!(
        "<!doctype html><html><body><h1>Index</h1><ul>{}</ul></body></html>",
        items.join("")
    );
    if head_only {
        let mut headers = String::new();
        let _ = write!(
            headers,
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(headers.as_bytes());
    } else {
        let mut headers = String::new();
        let _ = write!(
            headers,
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(headers.as_bytes());
    }
}

fn send_response(
    stream: &mut TcpStream,
    status: u16,
    phrase: &str,
    mime: &str,
    body: &[u8],
    include_body: bool,
) {
    let mut response = String::new();
    let _ = write!(
        response,
        "HTTP/1.1 {status} {phrase}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    if stream.write_all(response.as_bytes()).is_err() {
        return;
    }
    if include_body {
        let _ = stream.write_all(body);
    }
}

fn content_type_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
    {
        "html" => "text/html; charset=utf-8",
        "js" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        "txt" => "text/plain; charset=utf-8",
        "xml" => "application/xml; charset=utf-8",
        "adtg" => "application/octet-stream",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequestPathError {
    BadEncoding,
    Forbidden,
}

fn request_relative_path(input: &str) -> Result<PathBuf, RequestPathError> {
    let path_only = input
        .split(['?', '#'])
        .next()
        .unwrap_or(input)
        .trim_start_matches('/');
    let decoded = percent_decode(path_only)?;
    if decoded.contains('\0') || decoded.contains('\\') {
        return Err(RequestPathError::Forbidden);
    }
    if decoded.is_empty() {
        return Ok(PathBuf::from("index.html"));
    }

    let mut out = PathBuf::new();
    for component in Path::new(&decoded).components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(RequestPathError::Forbidden);
            }
        }
    }

    if out.as_os_str().is_empty() {
        Ok(PathBuf::from("index.html"))
    } else {
        Ok(out)
    }
}

fn percent_decode(input: &str) -> Result<String, RequestPathError> {
    let mut output = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut idx = 0;

    while idx < bytes.len() {
        if bytes[idx] == b'%' {
            if idx + 2 >= bytes.len() {
                return Err(RequestPathError::BadEncoding);
            }
            let hi = bytes[idx + 1];
            let lo = bytes[idx + 2];
            let value = from_hex(hi).and_then(|h| from_hex(lo).map(|l| (h << 4) | l));
            if let Some(byte) = value {
                output.push(byte);
                idx += 3;
                continue;
            }
            return Err(RequestPathError::BadEncoding);
        }

        output.push(input.as_bytes()[idx]);
        idx += 1;
    }

    String::from_utf8(output).map_err(|_| RequestPathError::BadEncoding)
}

fn from_hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

fn html_escape(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            _ => output.push(ch),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_path_maps_to_index() {
        assert_eq!(
            request_relative_path("/").unwrap(),
            PathBuf::from("index.html")
        );
    }

    #[test]
    fn query_string_is_not_part_of_file_path() {
        assert_eq!(
            request_relative_path("/index.html?cache=1").unwrap(),
            PathBuf::from("index.html")
        );
    }

    #[test]
    fn percent_encoded_parent_dir_is_forbidden() {
        assert_eq!(
            request_relative_path("/%2e%2e/Cargo.toml"),
            Err(RequestPathError::Forbidden)
        );
    }

    #[test]
    fn malformed_percent_encoding_is_bad_request() {
        assert_eq!(
            request_relative_path("/bad%zz"),
            Err(RequestPathError::BadEncoding)
        );
        assert_eq!(
            request_relative_path("/bad%"),
            Err(RequestPathError::BadEncoding)
        );
    }
}
