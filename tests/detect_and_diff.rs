use tablegram::detect::{detect_format, RecordsetFormat};
use tablegram::hexdiff::{hexdiff, HexDiffOptions};

#[test]
fn detects_xml_with_whitespace() {
    assert_eq!(detect_format(b"\r\n\t <xml/>"), RecordsetFormat::Xml);
}

#[test]
fn detects_utf8_bom_xml_with_whitespace() {
    assert_eq!(
        detect_format(b"\xEF\xBB\xBF\r\n\t <xml/>"),
        RecordsetFormat::Xml
    );
}

#[test]
fn detects_utf16_xml_with_bom_and_whitespace() {
    assert_eq!(
        detect_format(&utf16_bytes("\r\n\t <xml/>", true)),
        RecordsetFormat::Xml
    );
    assert_eq!(
        detect_format(&utf16_bytes("\r\n\t <xml/>", false)),
        RecordsetFormat::Xml
    );
}

#[test]
fn detects_utf16_xml_without_bom_and_with_whitespace() {
    assert_eq!(
        detect_format(&utf16_bytes_without_bom("\r\n\t <xml/>", true)),
        RecordsetFormat::Xml
    );
    assert_eq!(
        detect_format(&utf16_bytes_without_bom("\r\n\t <xml/>", false)),
        RecordsetFormat::Xml
    );
}

#[test]
fn treats_utf16_bom_without_xml_as_adtg() {
    assert_eq!(
        detect_format(&utf16_bytes("\r\n\t not xml", true)),
        RecordsetFormat::Adtg
    );
    assert_eq!(
        detect_format(&utf16_bytes("\r\n\t not xml", false)),
        RecordsetFormat::Adtg
    );
}

#[test]
fn treats_utf16_without_bom_and_without_xml_as_adtg() {
    assert_eq!(
        detect_format(&utf16_bytes_without_bom("\r\n\t not xml", true)),
        RecordsetFormat::Adtg
    );
    assert_eq!(
        detect_format(&utf16_bytes_without_bom("\r\n\t not xml", false)),
        RecordsetFormat::Adtg
    );
}

#[test]
fn treats_binary_as_adtg() {
    assert_eq!(detect_format(&[0, 1, 2, 3]), RecordsetFormat::Adtg);
}

#[test]
fn hexdiff_reports_changed_offsets() {
    let diff = hexdiff(
        b"abcdef",
        b"abcxef",
        HexDiffOptions {
            width: 4,
            max_lines: 10,
        },
    );
    assert!(diff.contains("00000000"));
    assert!(diff.contains("^"));
}

fn utf16_bytes(text: &str, little_endian: bool) -> Vec<u8> {
    let mut out = if little_endian {
        vec![0xff, 0xfe]
    } else {
        vec![0xfe, 0xff]
    };
    for unit in text.encode_utf16() {
        let bytes = if little_endian {
            unit.to_le_bytes()
        } else {
            unit.to_be_bytes()
        };
        out.extend_from_slice(&bytes);
    }
    out
}

fn utf16_bytes_without_bom(text: &str, little_endian: bool) -> Vec<u8> {
    let mut out = Vec::new();
    for unit in text.encode_utf16() {
        let bytes = if little_endian {
            unit.to_le_bytes()
        } else {
            unit.to_be_bytes()
        };
        out.extend_from_slice(&bytes);
    }
    out
}
