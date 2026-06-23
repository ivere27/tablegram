//! Lightweight persistence-format detection.
//!
//! ADO XML can start with UTF BOMs or whitespace before the first tag; anything
//! else is treated as ADTG so binary inputs take the native parser path.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordsetFormat {
    Xml,
    Adtg,
}

pub fn detect_format(bytes: &[u8]) -> RecordsetFormat {
    if starts_with_utf16_xml(bytes) {
        return RecordsetFormat::Xml;
    }

    let bytes = strip_utf8_bom(bytes);
    let first = bytes
        .iter()
        .copied()
        .find(|b| !matches!(b, b' ' | b'\t' | b'\r' | b'\n'));

    match first {
        Some(b'<') => RecordsetFormat::Xml,
        _ => RecordsetFormat::Adtg,
    }
}

pub fn strip_utf8_bom(bytes: &[u8]) -> &[u8] {
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        &bytes[3..]
    } else {
        bytes
    }
}

fn starts_with_utf16_xml(bytes: &[u8]) -> bool {
    if let Some(rest) = bytes.strip_prefix(&[0xff, 0xfe]) {
        return starts_with_utf16_xml_body(rest, true);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xfe, 0xff]) {
        return starts_with_utf16_xml_body(rest, false);
    }

    starts_with_utf16_xml_body(bytes, true) || starts_with_utf16_xml_body(bytes, false)
}

fn starts_with_utf16_xml_body(mut bytes: &[u8], little_endian: bool) -> bool {
    while bytes.len() >= 2 {
        let unit = if little_endian {
            u16::from_le_bytes([bytes[0], bytes[1]])
        } else {
            u16::from_be_bytes([bytes[0], bytes[1]])
        };
        match unit {
            0x09 | 0x0A | 0x0D | 0x20 => bytes = &bytes[2..],
            value => return value == b'<' as u16,
        }
    }
    false
}
