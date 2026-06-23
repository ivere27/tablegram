//! Small byte-oriented diff formatter for ADTG diagnostics.
//!
//! The output keeps hex and ASCII views side by side so corpus and oracle
//! failures can be inspected without requiring an external diff tool.

use std::fmt::Write;

#[derive(Debug, Clone, Copy)]
pub struct HexDiffOptions {
    pub width: usize,
    pub max_lines: usize,
}

impl Default for HexDiffOptions {
    fn default() -> Self {
        Self {
            width: 16,
            max_lines: 0,
        }
    }
}

pub fn hexdiff(left: &[u8], right: &[u8], options: HexDiffOptions) -> String {
    let width = options.width.clamp(4, 32);
    let max_len = left.len().max(right.len());
    let mut output = String::new();
    let mut emitted = 0usize;

    writeln!(
        output,
        "offset    left{pad_left} | right{pad_right} | ascii",
        pad_left = " ".repeat((width * 3).saturating_sub(4)),
        pad_right = " ".repeat((width * 3).saturating_sub(5))
    )
    .expect("write to string");

    for offset in (0..max_len).step_by(width) {
        if options.max_lines > 0 && emitted >= options.max_lines {
            writeln!(output, "... truncated at {emitted} lines").expect("write to string");
            break;
        }

        let end = (offset + width).min(max_len);
        let left_chunk = left.get(offset..left.len().min(end)).unwrap_or(&[]);
        let right_chunk = right.get(offset..right.len().min(end)).unwrap_or(&[]);
        let differs = (offset..end).any(|i| left.get(i) != right.get(i));

        if !differs {
            continue;
        }

        emitted += 1;
        writeln!(
            output,
            "{offset:08X}  {} | {} | {}",
            format_chunk(left_chunk, width),
            format_chunk(right_chunk, width),
            format_ascii(left, right, offset, end)
        )
        .expect("write to string");
    }

    if emitted == 0 {
        writeln!(output, "no byte differences").expect("write to string");
    }

    output
}

fn format_chunk(chunk: &[u8], width: usize) -> String {
    let mut out = String::new();
    for index in 0..width {
        if let Some(byte) = chunk.get(index) {
            write!(out, "{byte:02X} ").expect("write to string");
        } else {
            out.push_str("   ");
        }
    }
    out
}

fn format_ascii(left: &[u8], right: &[u8], offset: usize, end: usize) -> String {
    let mut out = String::new();
    for index in offset..end {
        match (left.get(index), right.get(index)) {
            (Some(a), Some(b)) if a == b => out.push(printable(*a)),
            (Some(_), Some(_)) => out.push('^'),
            (Some(_), None) => out.push('-'),
            (None, Some(_)) => out.push('+'),
            (None, None) => out.push(' '),
        }
    }
    out
}

fn printable(byte: u8) -> char {
    if byte.is_ascii_graphic() || byte == b' ' {
        byte as char
    } else {
        '.'
    }
}
