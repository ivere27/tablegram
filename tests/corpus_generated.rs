use std::panic;

use tablegram::adtg::{inspect_adtg, parse_adtg_bytes};
use tablegram::detect::{detect_format, RecordsetFormat};
use tablegram::model::{RecordStatusFlag, RowChangeKind, RowState, Value};
use tablegram::parse_recordset_bytes;
use tablegram::xml::parse_ado_xml_bytes;

const XML_CORPUS: &[(&str, &[u8], usize, usize)] = &[
    (
        "binary",
        include_bytes!("../corpus/generated/binary.xml"),
        3,
        1,
    ),
    (
        "empty",
        include_bytes!("../corpus/generated/empty.xml"),
        2,
        0,
    ),
    (
        "long_text",
        include_bytes!("../corpus/generated/long_text.xml"),
        2,
        1,
    ),
    (
        "nulls",
        include_bytes!("../corpus/generated/nulls.xml"),
        3,
        2,
    ),
    (
        "strings_ascii",
        include_bytes!("../corpus/generated/strings_ascii.xml"),
        3,
        3,
    ),
    (
        "strings_korean_ansi",
        include_bytes!("../corpus/generated/strings_korean_ansi.xml"),
        3,
        3,
    ),
    (
        "strings_korean_unicode",
        include_bytes!("../corpus/generated/strings_korean_unicode.xml"),
        3,
        3,
    ),
    (
        "types_basic",
        include_bytes!("../corpus/generated/types_basic.xml"),
        5,
        2,
    ),
];

const ADTG_CORPUS: &[(&str, &[u8])] = &[
    ("binary", include_bytes!("../corpus/generated/binary.adtg")),
    ("empty", include_bytes!("../corpus/generated/empty.adtg")),
    (
        "strings_korean_unicode",
        include_bytes!("../corpus/generated/strings_korean_unicode.adtg"),
    ),
    (
        "types_basic",
        include_bytes!("../corpus/generated/types_basic.adtg"),
    ),
];

#[test]
fn parses_generated_xml_corpus_counts() {
    for (name, bytes, expected_fields, expected_rows) in XML_CORPUS {
        let recordset = parse_ado_xml_bytes(bytes)
            .unwrap_or_else(|err| panic!("failed to parse generated XML corpus {name}: {err:#}"));

        assert_eq!(recordset.fields.len(), *expected_fields, "{name} fields");
        assert_eq!(recordset.rows.len(), *expected_rows, "{name} rows");
    }
}

#[test]
fn parses_korean_unicode_values_from_mdac_xml() {
    let recordset = parse_ado_xml_bytes(include_bytes!(
        "../corpus/generated/strings_korean_unicode.xml"
    ))
    .unwrap();

    assert_eq!(recordset.rows[0].state, RowState::Inserted);
    assert_eq!(recordset.rows[0].status_flags, vec![RecordStatusFlag::New]);
    assert_eq!(recordset.changes.len(), 1);
    assert_eq!(recordset.changes[0].kind, RowChangeKind::Insert);
    assert_eq!(recordset.changes[0].row_indices, vec![0, 1, 2]);
    assert_eq!(recordset.rows[0].values[1], Value::String("가".to_string()));
    assert_eq!(recordset.rows[0].values[2], Value::String("각".to_string()));
    assert_eq!(
        recordset.rows[1].values[1],
        Value::String("한글".to_string())
    );
    assert_eq!(
        recordset.rows[1].values[2],
        Value::String("마지막".to_string())
    );
    assert_eq!(
        recordset.rows[2].values[1],
        Value::String("홍길동".to_string())
    );
    assert_eq!(
        recordset.rows[2].values[2],
        Value::String("끝값".to_string())
    );
}

#[test]
fn parses_null_empty_and_binary_values_from_mdac_xml() {
    let nulls = parse_ado_xml_bytes(include_bytes!("../corpus/generated/nulls.xml")).unwrap();
    assert_eq!(nulls.rows[0].values[1], Value::Null);
    assert_eq!(nulls.rows[0].values[2], Value::Null);
    assert_eq!(nulls.rows[1].values[1], Value::String(String::new()));
    assert_eq!(nulls.rows[1].values[2], Value::Integer(0));

    let binary = parse_ado_xml_bytes(include_bytes!("../corpus/generated/binary.xml")).unwrap();
    assert_eq!(
        binary.rows[0].values[1],
        Value::BinaryHex("000102FF".to_string())
    );
    assert_eq!(
        binary.rows[0].values[2],
        Value::BinaryHex("DEADBEEF00102030".to_string())
    );
}

#[test]
fn parses_basic_mdac_data_types() {
    let recordset =
        parse_ado_xml_bytes(include_bytes!("../corpus/generated/types_basic.xml")).unwrap();

    assert_eq!(recordset.fields[2].ado_type.unwrap().name, "adCurrency");
    assert_eq!(recordset.fields[4].ado_type.unwrap().name, "adDate");
    assert_eq!(recordset.rows[0].values[0], Value::Integer(1));
    assert_eq!(recordset.rows[0].values[1], Value::Boolean(true));
    assert_eq!(
        recordset.rows[0].values[2],
        Value::Decimal("1234.56".to_string())
    );
    assert_eq!(
        recordset.rows[0].values[3],
        Value::Float("3.14159".parse().unwrap())
    );
    assert_eq!(
        recordset.rows[0].values[4],
        Value::DateTime("2001-02-03T04:05:06".to_string())
    );

    assert_eq!(recordset.rows[1].values[0], Value::Integer(-2));
    assert_eq!(recordset.rows[1].values[1], Value::Boolean(false));
    assert_eq!(
        recordset.rows[1].values[2],
        Value::Decimal("-7.89".to_string())
    );
    assert_eq!(recordset.rows[1].values[3], Value::Float(-0.25));
    assert_eq!(
        recordset.rows[1].values[4],
        Value::DateTime("1999-12-31T23:59:58".to_string())
    );
}

#[test]
fn inspects_generated_adtg_corpus() {
    for (name, bytes) in ADTG_CORPUS {
        assert_eq!(detect_format(bytes), RecordsetFormat::Adtg, "{name}");

        let document = inspect_adtg(bytes).unwrap_or_else(|err| {
            panic!("failed to inspect generated ADTG corpus {name}: {err:#}")
        });
        assert_eq!(document.length, bytes.len(), "{name}");
        assert!(!document.header_hex.is_empty(), "{name}");
    }
}

#[test]
fn mutated_corpus_inputs_do_not_panic() {
    let seeds: &[(&str, &[u8])] = &[
        (
            "strings_korean_unicode.xml",
            include_bytes!("../corpus/generated/strings_korean_unicode.xml"),
        ),
        (
            "types_basic.xml",
            include_bytes!("../corpus/generated/types_basic.xml"),
        ),
        (
            "strings_korean_unicode.adtg",
            include_bytes!("../corpus/generated/strings_korean_unicode.adtg"),
        ),
        (
            "types_basic.adtg",
            include_bytes!("../corpus/generated/types_basic.adtg"),
        ),
        (
            "binary.adtg",
            include_bytes!("../corpus/generated/binary.adtg"),
        ),
        (
            "nulls.adtg",
            include_bytes!("../corpus/generated/nulls.adtg"),
        ),
        (
            "long_text.adtg",
            include_bytes!("../corpus/generated/long_text.adtg"),
        ),
        ("empty", b""),
        (
            "truncated_xml",
            b"<?xml version=\"1.0\"?><xml><rs:data><z:row",
        ),
        ("random_binary", &[0, 1, 2, 3, 0xff, 0xfe, 0x80, 0x81]),
    ];

    for (name, seed) in seeds {
        for case in 0..96 {
            let data = mutate(seed, case);
            let result = panic::catch_unwind(|| {
                let _ = parse_recordset_bytes(&data);
                let _ = parse_ado_xml_bytes(&data);
                if detect_format(&data) == RecordsetFormat::Adtg {
                    let _ = inspect_adtg(&data);
                    let _ = parse_adtg_bytes(&data);
                }
            });

            assert!(
                result.is_ok(),
                "parser panicked for seed {name}, case {case}"
            );
        }
    }
}

#[test]
fn corrupted_adtg_ole_date_returns_error_instead_of_panicking() {
    let mut data = include_bytes!("../corpus/generated/types_basic.adtg").to_vec();
    replace_once(
        &mut data,
        &[0x26, 0xbf, 0x58, 0x72, 0xa5, 0x07, 0xe2, 0x40],
        &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x7f],
    );

    let result = panic::catch_unwind(|| parse_recordset_bytes(&data));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("corrupted non-finite OLE date should be rejected");
    assert!(
        format!("{err:#}").contains("non-finite ADTG OLE date"),
        "{err:#}"
    );
}

#[test]
fn corrupted_adtg_double_returns_error_instead_of_non_finite_float() {
    let mut data = include_bytes!("../corpus/generated/types_basic.adtg").to_vec();
    replace_once(
        &mut data,
        &[0x6e, 0x86, 0x1b, 0xf0, 0xf9, 0x21, 0x09, 0x40],
        &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf8, 0x7f],
    );

    let result = panic::catch_unwind(|| parse_recordset_bytes(&data));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("corrupted non-finite adDouble should be rejected");
    assert!(
        format!("{err:#}").contains("non-finite ADTG adDouble value"),
        "{err:#}"
    );
}

#[test]
fn adtg_ole_date_rounding_carries_to_next_day() {
    let mut data = include_bytes!("../corpus/generated/types_basic.adtg").to_vec();
    replace_once(
        &mut data,
        &[0x26, 0xbf, 0x58, 0x72, 0xa5, 0x07, 0xe2, 0x40],
        &[0xc9, 0xf9, 0xff, 0xff, 0xbf, 0x07, 0xe2, 0x40],
    );

    let recordset =
        parse_recordset_bytes(&data).expect("ADTG with near-midnight adDate should parse");
    assert_eq!(
        recordset.rows[0].values[4],
        Value::DateTime("2001-02-04T00:00:00".to_string())
    );
}

fn mutate(seed: &[u8], case: usize) -> Vec<u8> {
    let mut data = seed.to_vec();
    if data.is_empty() {
        data.resize((case % 17) + 1, 0);
    }

    for round in 0..4 {
        let index = (case.wrapping_mul(131) + round * 17) % data.len();
        let mask = ((case >> (round % 8)) as u8).wrapping_add((round * 31) as u8);
        data[index] ^= mask;
    }

    if case.is_multiple_of(5) {
        let new_len = (case * 29) % data.len().max(1);
        data.truncate(new_len);
    }

    if case.is_multiple_of(7) {
        data.extend_from_slice(&[b'<', b'z', b':', b'r', b'o', b'w', 0, 0xff]);
    }

    data
}

fn replace_once(data: &mut [u8], from: &[u8], to: &[u8]) {
    assert_eq!(from.len(), to.len(), "replacement length mismatch");
    let index = data
        .windows(from.len())
        .position(|window| window == from)
        .unwrap_or_else(|| panic!("pattern not found: {}", hex::encode_upper(from)));
    data[index..index + to.len()].copy_from_slice(to);
    assert!(
        !data[index + 1..]
            .windows(from.len())
            .any(|window| window == from),
        "pattern was not unique: {}",
        hex::encode_upper(from)
    );
}
