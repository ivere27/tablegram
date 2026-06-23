use std::fs;
use std::path::Path;
use tablegram::adtg::parse_adtg_bytes;
use tablegram::compat::{
    materialize_default_view, materialize_pending_view, MaterializedField, MaterializedRow,
};
use tablegram::model::{RecordStatusFlag, Recordset, Value};
use tablegram::xml::parse_ado_xml_bytes;

#[test]
fn native_adtg_matches_xml_for_flat_integer_states() {
    assert_native_matches_xml(
        include_bytes!("../corpus/exhaustive/flat_Integer_states.adtg"),
        include_bytes!("../corpus/exhaustive/flat_Integer_states.xml"),
    );
}

#[test]
fn native_adtg_matches_xml_for_flat_integer_null_states() {
    assert_native_matches_xml(
        include_bytes!("../corpus/exhaustive/flat_Integer_null_states.adtg"),
        include_bytes!("../corpus/exhaustive/flat_Integer_null_states.xml"),
    );
}

#[test]
fn native_adtg_matches_xml_for_flat_var_wchar_states() {
    assert_native_matches_xml(
        include_bytes!("../corpus/exhaustive/flat_VarWChar_states.adtg"),
        include_bytes!("../corpus/exhaustive/flat_VarWChar_states.xml"),
    );
}

#[test]
fn native_adtg_matches_xml_for_flat_binary_boundaries() {
    assert_native_matches_xml(
        include_bytes!("../corpus/exhaustive/flat_Binary_boundaries.adtg"),
        include_bytes!("../corpus/exhaustive/flat_Binary_boundaries.xml"),
    );
}

#[test]
fn native_adtg_matches_xml_for_flat_datetime_boundaries() {
    assert_native_matches_xml(
        include_bytes!("../corpus/exhaustive/flat_Date_boundaries.adtg"),
        include_bytes!("../corpus/exhaustive/flat_Date_boundaries.xml"),
    );
}

#[test]
fn corrupted_adtg_dbdate_returns_error_instead_of_invalid_date_string() {
    let mut adtg = include_bytes!("../corpus/exhaustive/flat_DBDate_boundaries.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[0x6b, 0x07, 0x0c, 0x00, 0x1e, 0x00],
        &[0x6b, 0x07, 0x0d, 0x00, 0x1e, 0x00],
    );

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("corrupted adDBDate month should be rejected");
    assert!(
        format!("{err:#}").contains("invalid ADTG DBDATE month 13"),
        "{err:#}"
    );
}

#[test]
fn corrupted_adtg_filetime_returns_error_instead_of_out_of_range_year() {
    let mut adtg = include_bytes!("../corpus/exhaustive/flat_FileTime_boundaries.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[0x80, 0xe9, 0xa5, 0xd4, 0x1e, 0xfd, 0xe9, 0x01],
        &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
    );

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("corrupted adFileTime year should be rejected");
    assert!(
        format!("{err:#}").contains("ADTG FILETIME year 60056 is out of range"),
        "{err:#}"
    );
}

#[test]
fn corrupted_adtg_dbtimestamp_rejects_out_of_range_fraction() {
    let mut adtg = include_bytes!("../corpus/fuzz/fractional_timestamp.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[
            0x07, 0xFF, 0x02, 0x00, 0x00, 0x00, 0xEA, 0x07, 0x02, 0x00, 0x03, 0x00, 0x04, 0x00,
            0x05, 0x00, 0x06, 0x00, 0x00, 0x65, 0xCD, 0x1D,
        ],
        &[
            0x07, 0xFF, 0x02, 0x00, 0x00, 0x00, 0xEA, 0x07, 0x02, 0x00, 0x03, 0x00, 0x04, 0x00,
            0x05, 0x00, 0x06, 0x00, 0x00, 0xCA, 0x9A, 0x3B,
        ],
    );

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("out-of-range DBTIMESTAMP fraction should be rejected");
    assert!(
        format!("{err:#}").contains("invalid ADTG DBTIMESTAMP fraction 1000000000"),
        "{err:#}"
    );
}

#[test]
fn corrupted_flat_adtg_rejects_trailing_bytes_after_terminator() {
    let mut adtg = include_bytes!("../corpus/exhaustive/flat_Integer_states.adtg").to_vec();
    adtg.push(0xaa);

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("flat ADTG trailing bytes should be rejected");
    assert!(
        format!("{err:#}").contains("unexpected trailing bytes in flat ADTG"),
        "{err:#}"
    );
}

#[test]
fn corrupted_adtg_temporal_widths_return_errors_instead_of_reading_next_value() {
    for (label, bytes, type_code, from_width, to_width, expected_error) in [
        (
            "adDBDate",
            include_bytes!("../corpus/exhaustive/flat_DBDate_boundaries.adtg").as_slice(),
            133,
            6,
            4,
            "unsupported ADTG adDBDate width 4",
        ),
        (
            "adDBTime",
            include_bytes!("../corpus/exhaustive/flat_DBTime_boundaries.adtg").as_slice(),
            134,
            6,
            4,
            "unsupported ADTG adDBTime width 4",
        ),
        (
            "adDBTimeStamp",
            include_bytes!("../corpus/exhaustive/flat_DBTimeStamp_boundaries.adtg").as_slice(),
            135,
            16,
            8,
            "unsupported ADTG adDBTimeStamp width 8",
        ),
        (
            "adFileTime",
            include_bytes!("../corpus/exhaustive/flat_FileTime_boundaries.adtg").as_slice(),
            64,
            8,
            4,
            "unsupported ADTG adFileTime width 4",
        ),
    ] {
        let mut adtg = bytes.to_vec();
        replace_value_field_descriptor_width(&mut adtg, type_code, from_width, to_width);

        let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
        assert!(result.is_ok(), "{label}: native ADTG parser panicked");

        let err = result.unwrap().expect_err(&format!(
            "{label}: corrupted temporal width should be rejected"
        ));
        assert!(
            format!("{err:#}").contains(expected_error),
            "{label}: {err:#}"
        );
    }
}

#[test]
fn corrupted_adtg_scalar_widths_return_errors_instead_of_reading_next_value() {
    for (label, bytes, type_code, from_width, to_width, expected_error) in [
        (
            "adSmallInt",
            include_bytes!("../corpus/exhaustive/flat_SmallInt_states.adtg").as_slice(),
            2,
            2,
            1,
            "unsupported ADTG adSmallInt width 1",
        ),
        (
            "adInteger",
            include_bytes!("../corpus/exhaustive/flat_Integer_states.adtg").as_slice(),
            3,
            4,
            2,
            "unsupported ADTG adInteger width 2",
        ),
        (
            "adCurrency",
            include_bytes!("../corpus/exhaustive/flat_Currency_states.adtg").as_slice(),
            6,
            8,
            4,
            "unsupported ADTG adCurrency width 4",
        ),
        (
            "adDate",
            include_bytes!("../corpus/exhaustive/flat_Date_states.adtg").as_slice(),
            7,
            8,
            4,
            "unsupported ADTG adDate width 4",
        ),
        (
            "adBoolean",
            include_bytes!("../corpus/exhaustive/flat_Boolean_states.adtg").as_slice(),
            11,
            2,
            1,
            "unsupported ADTG adBoolean width 1",
        ),
        (
            "adTinyInt",
            include_bytes!("../corpus/exhaustive/flat_TinyInt_states.adtg").as_slice(),
            16,
            1,
            2,
            "unsupported ADTG adTinyInt width 2",
        ),
        (
            "adUnsignedTinyInt",
            include_bytes!("../corpus/exhaustive/flat_UnsignedTinyInt_states.adtg").as_slice(),
            17,
            1,
            2,
            "unsupported ADTG adUnsignedTinyInt width 2",
        ),
        (
            "adUnsignedSmallInt",
            include_bytes!("../corpus/exhaustive/flat_UnsignedSmallInt_states.adtg").as_slice(),
            18,
            2,
            1,
            "unsupported ADTG adUnsignedSmallInt width 1",
        ),
        (
            "adUnsignedInt",
            include_bytes!("../corpus/exhaustive/flat_UnsignedInt_states.adtg").as_slice(),
            19,
            4,
            2,
            "unsupported ADTG adUnsignedInt width 2",
        ),
        (
            "adBigInt",
            include_bytes!("../corpus/exhaustive/flat_BigInt_states.adtg").as_slice(),
            20,
            8,
            4,
            "unsupported ADTG adBigInt width 4",
        ),
        (
            "adUnsignedBigInt",
            include_bytes!("../corpus/exhaustive/flat_UnsignedBigInt_states.adtg").as_slice(),
            21,
            8,
            4,
            "unsupported ADTG adUnsignedBigInt width 4",
        ),
        (
            "adGUID",
            include_bytes!("../corpus/exhaustive/flat_GUID_states.adtg").as_slice(),
            72,
            16,
            8,
            "unsupported ADTG adGUID width 8",
        ),
    ] {
        let mut adtg = bytes.to_vec();
        replace_value_field_descriptor_width(&mut adtg, type_code, from_width, to_width);

        let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
        assert!(result.is_ok(), "{label}: native ADTG parser panicked");

        let err = result.unwrap().expect_err(&format!(
            "{label}: corrupted scalar width should be rejected"
        ));
        assert!(
            format!("{err:#}").contains(expected_error),
            "{label}: {err:#}"
        );
    }
}

#[test]
fn corrupted_empty_rowset_adtg_rejects_fixed_width_descriptor_without_rows() {
    for (label, from, to, expected_error) in [
        (
            "adInteger",
            &[0x49, 0x00, 0x44, 0x00, 0x03, 0x00, 0x04, 0x00, 0x00, 0x00][..],
            &[0x49, 0x00, 0x44, 0x00, 0x03, 0x00, 0x02, 0x00, 0x00, 0x00][..],
            "unsupported ADTG adInteger width 2",
        ),
        (
            "adDBTimeStamp",
            &[
                0x45, 0x00, 0x4D, 0x00, 0x50, 0x00, 0x54, 0x00, 0x59, 0x00, 0x5F, 0x00, 0x54, 0x00,
                0x53, 0x00, 0x87, 0x00, 0x10, 0x00, 0x00, 0x00,
            ][..],
            &[
                0x45, 0x00, 0x4D, 0x00, 0x50, 0x00, 0x54, 0x00, 0x59, 0x00, 0x5F, 0x00, 0x54, 0x00,
                0x53, 0x00, 0x87, 0x00, 0x08, 0x00, 0x00, 0x00,
            ][..],
            "unsupported ADTG adDBTimeStamp width 8",
        ),
        (
            "adVarNumeric",
            &[
                0x45, 0x00, 0x4D, 0x00, 0x50, 0x00, 0x54, 0x00, 0x59, 0x00, 0x5F, 0x00, 0x42, 0x00,
                0x49, 0x00, 0x4E, 0x00, 0x80, 0x00, 0x10, 0x00, 0x00, 0x00,
            ][..],
            &[
                0x45, 0x00, 0x4D, 0x00, 0x50, 0x00, 0x54, 0x00, 0x59, 0x00, 0x5F, 0x00, 0x42, 0x00,
                0x49, 0x00, 0x4E, 0x00, 0x8B, 0x00, 0x02, 0x00, 0x00, 0x00,
            ][..],
            "unsupported ADTG adVarNumeric width 2",
        ),
    ] {
        let mut adtg = include_bytes!("../corpus/fuzz/empty_rowset.adtg").to_vec();
        replace_once(&mut adtg, from, to);

        let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
        assert!(result.is_ok(), "{label}: native ADTG parser panicked");

        let err = result.unwrap().expect_err(&format!(
            "{label}: corrupted empty-rowset descriptor should be rejected"
        ));
        assert!(
            format!("{err:#}").contains(expected_error),
            "{label}: {err:#}"
        );
    }
}

#[test]
fn native_adtg_accepts_varnumeric_descriptor_width_larger_than_payload() {
    let mut adtg = include_bytes!("../corpus/fuzz/doc_number_varnumeric.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[
            0x8B, 0x00, 0x13, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x00,
        ],
        &[
            0x8B, 0x00, 0xFF, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x00,
        ],
    );

    let recordset = parse_adtg_bytes(&adtg).expect("wide adVarNumeric descriptor should parse");
    let view = materialize_default_view(&recordset);

    assert_eq!(
        field_identities(&view.fields),
        vec![
            ("ID", Some(3), 255, 0, 0x50),
            ("NUMBER_LEN8", Some(139), 255, 255, 0x40),
            ("NUMBER_LEN16", Some(139), 255, 255, 0x40),
            ("NUMBER_LEN19", Some(139), 255, 255, 0x40),
        ]
    );
    assert_eq!(
        view.rows[0].values,
        vec![
            Value::Integer(1),
            Value::Decimal("1234.5".to_string()),
            Value::Decimal("6000.75".to_string()),
            Value::Decimal("123456.789".to_string()),
        ]
    );
}

#[test]
fn native_adtg_reads_varnumeric_u32_length_prefix_for_wide_descriptor() {
    let mut adtg = include_bytes!("../corpus/fuzz/doc_number_varnumeric.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[
            0x8B, 0x00, 0x08, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x00,
        ],
        &[
            0x8B, 0x00, 0x00, 0x01, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x00,
        ],
    );
    replace_once(
        &mut adtg,
        &[
            0x07, 0xFF, 0x01, 0x00, 0x00, 0x00, 0x05, 0x05, 0x01, 0x01, 0x39, 0x30, 0x06, 0x06,
            0x02, 0x01,
        ],
        &[
            0x07, 0xFF, 0x01, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x05, 0x01, 0x01, 0x39,
            0x30, 0x06, 0x06, 0x02, 0x01,
        ],
    );

    let recordset =
        parse_adtg_bytes(&adtg).expect("wide adVarNumeric u32 length prefix should parse");
    let view = materialize_default_view(&recordset);

    assert_eq!(recordset.fields[1].max_length, Some(256));
    assert_eq!(
        view.rows[0].values,
        vec![
            Value::Integer(1),
            Value::Decimal("1234.5".to_string()),
            Value::Decimal("6000.75".to_string()),
            Value::Decimal("123456.789".to_string()),
        ]
    );
}

#[test]
fn corrupted_adtg_variant_width_returns_error_instead_of_ignoring_extra_bytes() {
    let mut adtg = include_bytes!("../corpus/variant/variant_integer.adtg").to_vec();
    replace_value_field_descriptor_width(&mut adtg, 12, 16, 18);

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("corrupted adVariant width should be rejected");
    assert!(
        format!("{err:#}").contains("unsupported ADTG adVariant width 18"),
        "{err:#}"
    );
}

#[test]
fn corrupted_adtg_boolean_rejects_non_com_boolean_word() {
    let mut adtg = include_bytes!("../corpus/exhaustive/flat_Boolean_states.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[0x07, 0xFF, 0x02, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0x0C],
        &[0x07, 0xFF, 0x02, 0x00, 0x00, 0x00, 0x01, 0x00, 0x0C],
    );

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("non-COM adBoolean word should be rejected");
    assert!(
        format!("{err:#}").contains("invalid ADTG adBoolean value 1"),
        "{err:#}"
    );
}

#[test]
fn corrupted_adtg_variant_bool_rejects_non_com_boolean_word() {
    let mut adtg = include_bytes!("../corpus/variant/variant_boolean.adtg").to_vec();
    replace_variant_payload_value(
        &mut adtg,
        &[0x07, 0xFF, 0x02, 0x00, 0x00, 0x00, 0x0B, 0x00],
        &[0x01, 0x00],
    );

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("non-COM variant bool word should be rejected");
    assert!(
        format!("{err:#}").contains("invalid ADTG variant VT_BOOL value 1"),
        "{err:#}"
    );
}

#[test]
fn corrupted_adtg_variant_error_subtype_is_rejected() {
    let mut adtg = include_bytes!("../corpus/variant/variant_integer.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[0x07, 0xFF, 0x03, 0x00, 0x00, 0x00, 0x03, 0x00],
        &[0x07, 0xFF, 0x03, 0x00, 0x00, 0x00, 0x0A, 0x00],
    );

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("VT_ERROR variant subtype should be rejected");
    assert!(
        format!("{err:#}").contains("unsupported native ADTG variant subtype 10"),
        "{err:#}"
    );
}

#[test]
fn corrupted_adtg_update_masks_reject_conflicting_null_and_non_null_bits() {
    let mut adtg = include_bytes!("../corpus/exhaustive/flat_Integer_states.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[
            0x07, 0xFF, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x80, 0x0A, 0x40, 0x00, 0x01,
            0x00, 0x00, 0x00,
        ],
        &[
            0x07, 0xFF, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x80, 0x0A, 0x40, 0x40, 0x01,
            0x00, 0x00, 0x00,
        ],
    );

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("conflicting update masks should be rejected");
    assert!(
        format!("{err:#}").contains("conflicting ADTG update masks for field VALUE_FIELD"),
        "{err:#}"
    );
}

#[test]
fn corrupted_adtg_insert_masks_reject_conflicting_null_and_non_null_bits() {
    let mut adtg = include_bytes!("../corpus/exhaustive/flat_Integer_states.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[
            0x0D, 0xC0, 0x00, 0x04, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x7F,
        ],
        &[
            0x0D, 0xC0, 0x40, 0x04, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x7F,
        ],
    );

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("conflicting insert masks should be rejected");
    assert!(
        format!("{err:#}").contains("conflicting ADTG insert masks for field VALUE_FIELD"),
        "{err:#}"
    );
}

#[test]
fn corrupted_adtg_masks_reject_null_bits_for_non_nullable_fields() {
    for (label, from, to, expected) in [
        (
            "update",
            &[
                0x07, 0xFF, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x80, 0x0A, 0x40, 0x00, 0x01,
                0x00, 0x00, 0x00,
            ][..],
            &[
                0x07, 0xFF, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x80, 0x0A, 0x40, 0x80, 0x01,
                0x00, 0x00, 0x00,
            ][..],
            "null ADTG update mask for non-nullable field ID",
        ),
        (
            "insert",
            &[
                0x0D, 0xC0, 0x00, 0x04, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x7F,
            ][..],
            &[
                0x0D, 0x40, 0x80, 0x04, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x7F,
            ][..],
            "null ADTG insert mask for non-nullable field ID",
        ),
    ] {
        let mut adtg = include_bytes!("../corpus/exhaustive/flat_Integer_states.adtg").to_vec();
        replace_once(&mut adtg, from, to);

        let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
        assert!(result.is_ok(), "{label}: native ADTG parser panicked");

        let err = result.unwrap().expect_err(&format!(
            "{label}: non-nullable null mask should be rejected"
        ));
        assert!(format!("{err:#}").contains(expected), "{label}: {err:#}");
    }
}

#[test]
fn corrupted_adtg_insert_masks_reject_missing_non_nullable_values() {
    let mut adtg = include_bytes!("../corpus/exhaustive/flat_Integer_states.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[
            0x0D, 0xC0, 0x00, 0x04, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x7F,
        ],
        &[
            0x0D, 0x40, 0x00, 0x04, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x7F,
        ],
    );

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("missing non-nullable insert value should be rejected");
    assert!(
        format!("{err:#}").contains("missing ADTG insert value for non-nullable field ID"),
        "{err:#}"
    );
}

#[test]
fn corrupted_adtg_update_masks_reject_unused_padding_bits() {
    let mut adtg = include_bytes!("../corpus/exhaustive/flat_Integer_states.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[
            0x07, 0xFF, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x80, 0x0A, 0x40, 0x00, 0x01,
            0x00, 0x00, 0x00,
        ],
        &[
            0x07, 0xFF, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x80, 0x0A, 0x41, 0x00, 0x01,
            0x00, 0x00, 0x00,
        ],
    );

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("unused update mask bits should be rejected");
    assert!(
        format!("{err:#}").contains("unused ADTG update non-null mask bits set for 2 fields"),
        "{err:#}"
    );
}

#[test]
fn corrupted_adtg_insert_masks_reject_unused_padding_bits() {
    let mut adtg = include_bytes!("../corpus/exhaustive/flat_Integer_states.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[
            0x0D, 0xC0, 0x00, 0x04, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x7F,
        ],
        &[
            0x0D, 0xC1, 0x00, 0x04, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x7F,
        ],
    );

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("unused insert mask bits should be rejected");
    assert!(
        format!("{err:#}").contains("unused ADTG insert non-null mask bits set for 2 fields"),
        "{err:#}"
    );
}

#[test]
fn corrupted_adtg_row_presence_mask_rejects_unset_unused_padding_bits() {
    let mut adtg = include_bytes!("../corpus/exhaustive/flat_Integer_states.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[0x07, 0xFF, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x80],
        &[0x07, 0x80, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x80],
    );

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("unset row-presence padding bits should be rejected");
    assert!(
        format!("{err:#}").contains("unset unused ADTG row presence mask bits for 1 fields"),
        "{err:#}"
    );
}

#[test]
fn corrupted_chaptered_adtg_row_presence_mask_rejects_unset_unused_padding_bits() {
    let mut adtg = include_bytes!("fixtures/shape/orders_lines_shape.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[0x07, 0x7F, 0xA1, 0x86, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00],
        &[0x07, 0x70, 0xA1, 0x86, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00],
    );

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("unset chapter row-presence padding bits should be rejected");
    assert!(
        format!("{err:#}").contains("unset unused ADTG chapter row presence mask bits"),
        "{err:#}"
    );
}

#[test]
fn corrupted_chaptered_adtg_rejects_parent_relation_to_chapter_field() {
    let mut adtg = include_bytes!("fixtures/shape/orders_lines_shape.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[
            0x0C, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ],
        &[
            0x0C, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ],
    );

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("chapter relation to parent chapter field should be rejected");
    assert!(
        format!("{err:#}")
            .contains("ADTG chapter parent relation ordinal 4 points to chapter field Lines"),
        "{err:#}"
    );
}

#[test]
fn corrupted_chaptered_adtg_rejects_child_relation_to_chapter_field() {
    let mut adtg = include_bytes!("fixtures/shape/orders_lines_product_shape.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[
            0x0C, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ],
        &[
            0x0C, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ],
    );

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("chapter relation to child chapter field should be rejected");
    assert!(
        format!("{err:#}")
            .contains("ADTG chapter child relation ordinal 6 points to chapter field Product"),
        "{err:#}"
    );
}

#[test]
fn corrupted_empty_parent_chaptered_adtg_validates_relation_schema_without_rows() {
    let mut adtg = include_bytes!("fixtures/shape/orders_empty_parent_lines_shape.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[
            0x0C, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ],
        &[
            0x0C, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ],
    );

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("empty-parent chapter relation should be schema-validated");
    assert!(
        format!("{err:#}")
            .contains("ADTG chapter parent relation ordinal 4 points to chapter field Lines"),
        "{err:#}"
    );
}

#[test]
fn corrupted_chaptered_adtg_rejects_short_relation_pair_record() {
    let mut adtg = include_bytes!("fixtures/shape/orders_lines_shape.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[
            0x0C, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ],
        &[
            0x08, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ],
    );

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("short relation pair record should be rejected");
    assert!(
        format!("{err:#}").contains("chaptered ADTG field had no relation metadata"),
        "{err:#}"
    );
}

#[test]
fn corrupted_composite_chaptered_adtg_rejects_partial_relation_pair_record() {
    let mut adtg = include_bytes!("fixtures/shape/orders_lines_composite_shape.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[
            0x18, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ],
        &[
            0x10, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ],
    );

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("partial relation pair record should be rejected");
    assert!(
        format!("{err:#}").contains("chaptered ADTG field had no relation metadata"),
        "{err:#}"
    );
}

#[test]
fn corrupted_composite_chaptered_adtg_rejects_repeated_parent_relation_ordinal() {
    let mut adtg = include_bytes!("fixtures/shape/orders_lines_composite_shape.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[
            0x18, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ],
        &[
            0x18, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ],
    );

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("duplicate parent relation ordinal should be rejected");
    assert!(
        format!("{err:#}").contains("ADTG chapter relation repeated parent ordinal 1"),
        "{err:#}"
    );
}

#[test]
fn corrupted_composite_chaptered_adtg_rejects_repeated_child_relation_ordinal() {
    let mut adtg = include_bytes!("fixtures/shape/orders_lines_composite_shape.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[
            0x18, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ],
        &[
            0x18, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ],
    );

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("duplicate child relation ordinal should be rejected");
    assert!(
        format!("{err:#}").contains("ADTG chapter relation repeated child ordinal 1"),
        "{err:#}"
    );
}

#[test]
fn corrupted_adtg_varlen_value_rejects_length_beyond_defined_size() {
    let mut adtg = include_bytes!("../corpus/exhaustive/flat_VarWChar_states.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[
            0x07, 0xFF, 0x02, 0x00, 0x00, 0x00, 0x16, 0x70, 0x00, 0x6C, 0x00, 0x61, 0x00,
        ],
        &[
            0x07, 0xFF, 0x02, 0x00, 0x00, 0x00, 0xF2, 0x70, 0x00, 0x6C, 0x00, 0x61, 0x00,
        ],
    );

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("oversized variable value should be rejected");
    assert!(
        format!("{err:#}").contains(
            "ADTG variable value length 242 exceeds defined byte length 240 for field VALUE_FIELD"
        ),
        "{err:#}"
    );
}

#[test]
fn corrupted_adtg_single_width_returns_error_instead_of_reading_next_value() {
    let mut adtg = include_bytes!("../corpus/exhaustive/flat_Single_states.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[
            0x06, 0x33, 0x00, 0x80, 0x01, 0x00, 0x02, 0x00, 0x0B, 0x00, 0x56, 0x00, 0x41, 0x00,
            0x4C, 0x00, 0x55, 0x00, 0x45, 0x00, 0x5F, 0x00, 0x46, 0x00, 0x49, 0x00, 0x45, 0x00,
            0x4C, 0x00, 0x44, 0x00, 0x04, 0x00, 0x04, 0x00,
        ],
        &[
            0x06, 0x33, 0x00, 0x80, 0x01, 0x00, 0x02, 0x00, 0x0B, 0x00, 0x56, 0x00, 0x41, 0x00,
            0x4C, 0x00, 0x55, 0x00, 0x45, 0x00, 0x5F, 0x00, 0x46, 0x00, 0x49, 0x00, 0x45, 0x00,
            0x4C, 0x00, 0x44, 0x00, 0x04, 0x00, 0x02, 0x00,
        ],
    );

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("corrupted narrow adSingle width should be rejected");
    assert!(
        format!("{err:#}").contains("unsupported ADTG adSingle width 2"),
        "{err:#}"
    );
}

#[test]
fn corrupted_adtg_decimal_width_returns_error_instead_of_reading_next_value() {
    let mut adtg = include_bytes!("../corpus/exhaustive/flat_Decimal_states.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[
            0x06, 0x33, 0x00, 0x80, 0x01, 0x00, 0x02, 0x00, 0x0B, 0x00, 0x56, 0x00, 0x41, 0x00,
            0x4C, 0x00, 0x55, 0x00, 0x45, 0x00, 0x5F, 0x00, 0x46, 0x00, 0x49, 0x00, 0x45, 0x00,
            0x4C, 0x00, 0x44, 0x00, 0x0E, 0x00, 0x10, 0x00,
        ],
        &[
            0x06, 0x33, 0x00, 0x80, 0x01, 0x00, 0x02, 0x00, 0x0B, 0x00, 0x56, 0x00, 0x41, 0x00,
            0x4C, 0x00, 0x55, 0x00, 0x45, 0x00, 0x5F, 0x00, 0x46, 0x00, 0x49, 0x00, 0x45, 0x00,
            0x4C, 0x00, 0x44, 0x00, 0x0E, 0x00, 0x08, 0x00,
        ],
    );

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("corrupted adDecimal width should be rejected");
    assert!(
        format!("{err:#}").contains("unsupported ADTG adDecimal width 8"),
        "{err:#}"
    );
}

#[test]
fn corrupted_adtg_numeric_width_returns_error_instead_of_reading_next_value() {
    let mut adtg = include_bytes!("../corpus/exhaustive/flat_Numeric_states.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[
            0x06, 0x33, 0x00, 0x80, 0x01, 0x00, 0x02, 0x00, 0x0B, 0x00, 0x56, 0x00, 0x41, 0x00,
            0x4C, 0x00, 0x55, 0x00, 0x45, 0x00, 0x5F, 0x00, 0x46, 0x00, 0x49, 0x00, 0x45, 0x00,
            0x4C, 0x00, 0x44, 0x00, 0x83, 0x00, 0x13, 0x00,
        ],
        &[
            0x06, 0x33, 0x00, 0x80, 0x01, 0x00, 0x02, 0x00, 0x0B, 0x00, 0x56, 0x00, 0x41, 0x00,
            0x4C, 0x00, 0x55, 0x00, 0x45, 0x00, 0x5F, 0x00, 0x46, 0x00, 0x49, 0x00, 0x45, 0x00,
            0x4C, 0x00, 0x44, 0x00, 0x83, 0x00, 0x08, 0x00,
        ],
    );

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("corrupted adNumeric width should be rejected");
    assert!(
        format!("{err:#}").contains("unsupported ADTG adNumeric width 8"),
        "{err:#}"
    );
}

#[test]
fn corrupted_adtg_numeric_descriptor_rejects_invalid_precision_scale() {
    let from = [
        0x83, 0x00, 0x13, 0x00, 0x00, 0x00, 0x12, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x34,
        0x00, 0x00, 0x00,
    ];
    for (label, to, expected) in [
        (
            "precision",
            [
                0x83, 0x00, 0x13, 0x00, 0x00, 0x00, 0x27, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00,
                0x34, 0x00, 0x00, 0x00,
            ],
            "invalid ADTG adNumeric descriptor precision 39 for field VALUE_FIELD",
        ),
        (
            "scale",
            [
                0x83, 0x00, 0x13, 0x00, 0x00, 0x00, 0x12, 0x00, 0x00, 0x00, 0x27, 0x00, 0x00, 0x00,
                0x34, 0x00, 0x00, 0x00,
            ],
            "invalid ADTG adNumeric descriptor scale 39 for field VALUE_FIELD",
        ),
        (
            "scale exceeds precision",
            [
                0x83, 0x00, 0x13, 0x00, 0x00, 0x00, 0x12, 0x00, 0x00, 0x00, 0x13, 0x00, 0x00, 0x00,
                0x34, 0x00, 0x00, 0x00,
            ],
            "invalid ADTG adNumeric descriptor scale 19 exceeds precision 18 for field VALUE_FIELD",
        ),
    ] {
        let mut adtg = include_bytes!("../corpus/exhaustive/flat_Numeric_states.adtg").to_vec();
        replace_once(&mut adtg, &from, &to);

        let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
        assert!(result.is_ok(), "{label}: native ADTG parser panicked");

        let err = result.unwrap().expect_err(&format!(
            "{label}: corrupted numeric descriptor should be rejected"
        ));
        assert!(format!("{err:#}").contains(expected), "{label}: {err:#}");
    }
}

#[test]
fn corrupted_adtg_decimal_descriptor_rejects_invalid_precision_scale() {
    let from = [
        0x0E, 0x00, 0x10, 0x00, 0x00, 0x00, 0x12, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x34,
        0x00, 0x00, 0x00,
    ];
    for (label, to, expected) in [
        (
            "precision",
            [
                0x0E, 0x00, 0x10, 0x00, 0x00, 0x00, 0x1D, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00,
                0x34, 0x00, 0x00, 0x00,
            ],
            "invalid ADTG adDecimal descriptor precision 29 for field VALUE_FIELD",
        ),
        (
            "scale",
            [
                0x0E, 0x00, 0x10, 0x00, 0x00, 0x00, 0x12, 0x00, 0x00, 0x00, 0x1D, 0x00, 0x00, 0x00,
                0x34, 0x00, 0x00, 0x00,
            ],
            "invalid ADTG adDecimal descriptor scale 29 for field VALUE_FIELD",
        ),
        (
            "scale exceeds precision",
            [
                0x0E, 0x00, 0x10, 0x00, 0x00, 0x00, 0x12, 0x00, 0x00, 0x00, 0x13, 0x00, 0x00, 0x00,
                0x34, 0x00, 0x00, 0x00,
            ],
            "invalid ADTG adDecimal descriptor scale 19 exceeds precision 18 for field VALUE_FIELD",
        ),
        (
            "partial xml-reader sentinel",
            [
                0x0E, 0x00, 0x10, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00,
                0x34, 0x00, 0x00, 0x00,
            ],
            "invalid ADTG adDecimal descriptor precision 255 for field VALUE_FIELD",
        ),
    ] {
        let mut adtg = include_bytes!("../corpus/exhaustive/flat_Decimal_states.adtg").to_vec();
        replace_once(&mut adtg, &from, &to);

        let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
        assert!(result.is_ok(), "{label}: native ADTG parser panicked");

        let err = result.unwrap().expect_err(&format!(
            "{label}: corrupted decimal descriptor should be rejected"
        ));
        assert!(format!("{err:#}").contains(expected), "{label}: {err:#}");
    }
}

#[test]
fn corrupted_adtg_numeric_rejects_invalid_precision_scale_and_sign_byte() {
    for (label, from, to, expected) in [
        (
            "precision",
            &[
                0x07, 0xFF, 0x02, 0x00, 0x00, 0x00, 0x12, 0x04, 0x00, 0x39, 0x30,
            ][..],
            &[
                0x07, 0xFF, 0x02, 0x00, 0x00, 0x00, 0x27, 0x04, 0x00, 0x39, 0x30,
            ][..],
            "invalid ADTG adNumeric value precision 39",
        ),
        (
            "scale",
            &[
                0x07, 0xFF, 0x02, 0x00, 0x00, 0x00, 0x12, 0x04, 0x00, 0x39, 0x30,
            ][..],
            &[
                0x07, 0xFF, 0x02, 0x00, 0x00, 0x00, 0x12, 0x27, 0x00, 0x39, 0x30,
            ][..],
            "invalid ADTG adNumeric value scale 39",
        ),
        (
            "scale exceeds precision",
            &[
                0x07, 0xFF, 0x02, 0x00, 0x00, 0x00, 0x12, 0x04, 0x00, 0x39, 0x30,
            ][..],
            &[
                0x07, 0xFF, 0x02, 0x00, 0x00, 0x00, 0x12, 0x13, 0x00, 0x39, 0x30,
            ][..],
            "invalid ADTG adNumeric value scale 19 exceeds precision 18",
        ),
        (
            "magnitude exceeds precision",
            &[
                0x07, 0xFF, 0x02, 0x00, 0x00, 0x00, 0x12, 0x04, 0x00, 0x39, 0x30,
            ][..],
            &[
                0x07, 0xFF, 0x02, 0x00, 0x00, 0x00, 0x04, 0x04, 0x00, 0x39, 0x30,
            ][..],
            "invalid ADTG adNumeric value magnitude exceeds precision 4",
        ),
        (
            "sign",
            &[
                0x07, 0xFF, 0x02, 0x00, 0x00, 0x00, 0x12, 0x04, 0x00, 0x39, 0x30,
            ][..],
            &[
                0x07, 0xFF, 0x02, 0x00, 0x00, 0x00, 0x12, 0x04, 0x02, 0x39, 0x30,
            ][..],
            "invalid ADTG adNumeric value sign byte 0x02",
        ),
    ] {
        let mut adtg = include_bytes!("../corpus/exhaustive/flat_Numeric_states.adtg").to_vec();
        replace_once(&mut adtg, from, to);

        let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
        assert!(result.is_ok(), "{label}: native ADTG parser panicked");

        let err = result
            .unwrap()
            .expect_err(&format!("{label}: corrupted numeric should be rejected"));
        assert!(format!("{err:#}").contains(expected), "{label}: {err:#}");
    }
}

#[test]
fn corrupted_adtg_varnumeric_rejects_invalid_precision_and_sign_byte() {
    for (label, from, to, expected) in [
        (
            "truncated payload",
            &[
                0x07, 0xFF, 0x01, 0x00, 0x00, 0x00, 0x05, 0x05, 0x01, 0x01, 0x39, 0x30,
            ][..],
            &[0x07, 0xFF, 0x01, 0x00, 0x00, 0x00, 0x03, 0x05, 0x01, 0x01][..],
            "truncated ADTG varnumeric value",
        ),
        (
            "precision",
            &[
                0x07, 0xFF, 0x01, 0x00, 0x00, 0x00, 0x05, 0x05, 0x01, 0x01, 0x39, 0x30,
            ][..],
            &[
                0x07, 0xFF, 0x01, 0x00, 0x00, 0x00, 0x05, 0x27, 0x01, 0x01, 0x39, 0x30,
            ][..],
            "invalid ADTG varnumeric value precision 39",
        ),
        (
            "scale exceeds precision",
            &[
                0x07, 0xFF, 0x01, 0x00, 0x00, 0x00, 0x05, 0x05, 0x01, 0x01, 0x39, 0x30,
            ][..],
            &[
                0x07, 0xFF, 0x01, 0x00, 0x00, 0x00, 0x05, 0x05, 0x06, 0x01, 0x39, 0x30,
            ][..],
            "invalid ADTG varnumeric value scale 6 exceeds precision 5",
        ),
        (
            "magnitude exceeds precision",
            &[
                0x07, 0xFF, 0x01, 0x00, 0x00, 0x00, 0x05, 0x05, 0x01, 0x01, 0x39, 0x30,
            ][..],
            &[
                0x07, 0xFF, 0x01, 0x00, 0x00, 0x00, 0x05, 0x04, 0x01, 0x01, 0x39, 0x30,
            ][..],
            "invalid ADTG varnumeric value magnitude exceeds precision 4",
        ),
        (
            "sign",
            &[
                0x07, 0xFF, 0x01, 0x00, 0x00, 0x00, 0x05, 0x05, 0x01, 0x01, 0x39, 0x30,
            ][..],
            &[
                0x07, 0xFF, 0x01, 0x00, 0x00, 0x00, 0x05, 0x05, 0x01, 0x02, 0x39, 0x30,
            ][..],
            "invalid ADTG varnumeric value sign byte 0x02",
        ),
    ] {
        let mut adtg = include_bytes!("../corpus/fuzz/doc_number_varnumeric.adtg").to_vec();
        replace_once(&mut adtg, from, to);

        let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
        assert!(result.is_ok(), "{label}: native ADTG parser panicked");

        let err = result
            .unwrap()
            .expect_err(&format!("{label}: corrupted varnumeric should be rejected"));
        assert!(format!("{err:#}").contains(expected), "{label}: {err:#}");
    }
}

#[test]
fn corrupted_adtg_decimal_rejects_invalid_reserved_and_sign_bytes() {
    for (label, from, to, expected) in [
        (
            "magnitude exceeds descriptor precision",
            &[
                0x0E, 0x00, 0x10, 0x00, 0x00, 0x00, 0x12, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00,
                0x34, 0x00, 0x00, 0x00,
            ][..],
            &[
                0x0E, 0x00, 0x10, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00,
                0x34, 0x00, 0x00, 0x00,
            ][..],
            "invalid ADTG adDecimal value magnitude exceeds precision 4",
        ),
        (
            "scale mismatch",
            &[
                0x07, 0xFF, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x80, 0x00, 0x00,
            ][..],
            &[
                0x07, 0xFF, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x80, 0x00, 0x00,
            ][..],
            "invalid ADTG adDecimal value scale 3 does not match descriptor scale 4",
        ),
        (
            "reserved word",
            &[
                0x07, 0xFF, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x80, 0x00, 0x00,
            ][..],
            &[
                0x07, 0xFF, 0x02, 0x00, 0x00, 0x00, 0x01, 0x00, 0x04, 0x80, 0x00, 0x00,
            ][..],
            "invalid ADTG adDecimal value reserved word 1",
        ),
        (
            "sign byte",
            &[
                0x07, 0xFF, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x80, 0x00, 0x00,
            ][..],
            &[
                0x07, 0xFF, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x40, 0x00, 0x00,
            ][..],
            "invalid ADTG adDecimal value sign byte 0x40",
        ),
    ] {
        let mut adtg = include_bytes!("../corpus/exhaustive/flat_Decimal_states.adtg").to_vec();
        replace_once(&mut adtg, from, to);

        let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
        assert!(result.is_ok(), "{label}: native ADTG parser panicked");

        let err = result
            .unwrap()
            .expect_err(&format!("{label}: corrupted decimal should be rejected"));
        assert!(format!("{err:#}").contains(expected), "{label}: {err:#}");
    }
}

#[test]
fn native_adtg_accepts_mdac_xml_resaved_decimal_variant_marker() {
    let mut adtg = include_bytes!("../corpus/fuzz/rowid_negative_scale.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[
            0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0xE2, 0x01, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ],
        &[
            0x0E, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0xE2, 0x01, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ],
    );

    let recordset = parse_adtg_bytes(&adtg).expect("MDAC XML-resaved adDecimal should parse");
    let view = materialize_default_view(&recordset);
    assert_eq!(
        view.rows[0].values[1],
        Value::Decimal("1234.56".to_string())
    );
}

#[test]
fn native_adtg_accepts_mdac_xml_resaved_scaled_decimal_zero() {
    let mut adtg = include_bytes!("../corpus/exhaustive/flat_Decimal_states.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[
            0x07, 0xFF, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ],
        &[
            0x07, 0xFF, 0x03, 0x00, 0x00, 0x00, 0x0E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ],
    );

    let recordset =
        parse_adtg_bytes(&adtg).expect("MDAC XML-resaved scaled adDecimal zero should parse");
    let view = materialize_default_view(&recordset);
    assert!(view
        .rows
        .iter()
        .any(|row| row.values[1] == Value::Decimal("0".to_string())));
}

#[test]
fn corrupted_adtg_variant_decimal_rejects_invalid_sign_byte() {
    let mut adtg = include_bytes!("../corpus/variant/variant_decimal.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[
            0x07, 0xFF, 0x02, 0x00, 0x00, 0x00, 0x0E, 0x00, 0x04, 0x80, 0x00, 0x00,
        ],
        &[
            0x07, 0xFF, 0x02, 0x00, 0x00, 0x00, 0x0E, 0x00, 0x04, 0x40, 0x00, 0x00,
        ],
    );

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("corrupted variant decimal should be rejected");
    assert!(
        format!("{err:#}").contains("invalid ADTG variant DECIMAL value sign byte 0x40"),
        "{err:#}"
    );
}

#[test]
fn unsupported_adtg_descriptor_type_returns_specific_error() {
    let mut adtg = include_bytes!("../corpus/exhaustive/flat_Integer_states.adtg").to_vec();
    replace_once(
        &mut adtg,
        &[
            0x06, 0x21, 0x00, 0x80, 0x01, 0x00, 0x01, 0x00, 0x02, 0x00, 0x49, 0x00, 0x44, 0x00,
            0x03, 0x00, 0x04, 0x00,
        ],
        &[
            0x06, 0x21, 0x00, 0x80, 0x01, 0x00, 0x01, 0x00, 0x02, 0x00, 0x49, 0x00, 0x44, 0x00,
            0x84, 0x00, 0x04, 0x00,
        ],
    );

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("unsupported ADTG descriptor type should be rejected");
    assert!(
        format!("{err:#}").contains("unsupported native ADTG descriptor type 132 for field ID"),
        "{err:#}"
    );
}

#[test]
fn corrupted_adtg_descriptor_ordinals_reject_gap() {
    let mut adtg = include_bytes!("../corpus/exhaustive/flat_Integer_states.adtg").to_vec();
    replace_value_field_descriptor_ordinal(&mut adtg, 3, 4, 2, 3);

    let result = std::panic::catch_unwind(|| parse_adtg_bytes(&adtg));
    assert!(result.is_ok(), "native ADTG parser panicked");

    let err = result
        .unwrap()
        .expect_err("descriptor ordinal gap should be rejected");
    assert!(
        format!("{err:#}")
            .contains("unexpected ADTG field descriptor ordinal 3 for field VALUE_FIELD"),
        "{err:#}"
    );
}

#[test]
fn native_adtg_matches_exhaustive_flat_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/exhaustive");
    if !dir.exists() {
        return;
    }

    let mut checked = 0usize;
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("adtg") {
            continue;
        }

        let xml_path = path.with_extension("xml");
        if !xml_path.exists() {
            continue;
        }

        let native = parse_adtg_bytes(&fs::read(&path).unwrap()).unwrap_or_else(|err| {
            panic!("failed to parse native ADTG {}: {err:#}", path.display())
        });
        let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
            .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
        assert_materialized_matches(&native, &expected, path.display().to_string());
        checked += 1;
    }

    assert_eq!(checked, 104);
}

#[test]
fn native_adtg_matches_generated_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/generated");
    if !dir.exists() {
        return;
    }

    let mut checked = 0usize;
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("adtg") {
            continue;
        }

        let xml_path = path.with_extension("xml");
        let native = parse_adtg_bytes(&fs::read(&path).unwrap()).unwrap_or_else(|err| {
            panic!("failed to parse native ADTG {}: {err:#}", path.display())
        });
        let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
            .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
        assert_materialized_matches(&native, &expected, path.display().to_string());
        checked += 1;
    }

    assert_eq!(checked, 8);
}

#[test]
fn native_adtg_decodes_korean_ansi_cp949_bytes() {
    let mut adtg = include_bytes!("../corpus/generated/strings_korean_ansi.adtg").to_vec();
    let patched_utf8 = try_replace_once(&mut adtg, &[0x03, 0xEA, 0xB0, 0x80], &[0x02, 0xB0, 0xA1]);
    if patched_utf8 {
        replace_once(&mut adtg, &[0x03, 0xEA, 0xB0, 0x81], &[0x02, 0xB0, 0xA2]);
        replace_once(
            &mut adtg,
            &[0x06, 0xED, 0x95, 0x9C, 0xEA, 0xB8, 0x80],
            &[0x04, 0xC7, 0xD1, 0xB1, 0xDB],
        );
        replace_once(
            &mut adtg,
            &[0x09, 0xEB, 0xA7, 0x88, 0xEC, 0xA7, 0x80, 0xEB, 0xA7, 0x89],
            &[0x06, 0xB8, 0xB6, 0xC1, 0xF6, 0xB8, 0xB7],
        );
        replace_once(
            &mut adtg,
            &[0x09, 0xED, 0x99, 0x8D, 0xEA, 0xB8, 0xB8, 0xEB, 0x8F, 0x99],
            &[0x06, 0xC8, 0xAB, 0xB1, 0xE6, 0xB5, 0xBF],
        );
        replace_once(
            &mut adtg,
            &[0x06, 0xEB, 0x81, 0x9D, 0xEA, 0xB0, 0x92],
            &[0x04, 0xB3, 0xA1, 0xB0, 0xAA],
        );
    } else {
        assert_contains(&adtg, &[0x02, 0xB0, 0xA1]);
        assert_contains(&adtg, &[0x04, 0xC7, 0xD1, 0xB1, 0xDB]);
    }

    let native = parse_adtg_bytes(&adtg).unwrap();
    let expected = parse_ado_xml_bytes(include_bytes!(
        "../corpus/generated/strings_korean_ansi.xml"
    ))
    .unwrap();
    assert_materialized_matches(&native, &expected, "cp949 ansi fixture".to_string());
}

#[test]
fn native_adtg_decodes_xml_reader_varnumeric_fixture_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("doc_number_varnumeric.adtg");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let view = materialize_default_view(&native);

    assert_eq!(
        field_identities(&view.fields),
        vec![
            ("ID", Some(3), 255, 0, 0x50),
            ("NUMBER_LEN8", Some(139), 255, 255, 0x40),
            ("NUMBER_LEN16", Some(139), 255, 255, 0x40),
            ("NUMBER_LEN19", Some(139), 255, 255, 0x40),
        ],
        "{} fields",
        adtg_path.display()
    );
    assert_eq!(view.rows.len(), 2, "{} rows", adtg_path.display());
    assert_eq!(view.rows[0].status, RecordStatusFlag::Unmodified);
    assert_eq!(
        view.rows[0].values,
        vec![
            Value::Integer(1),
            Value::Decimal("1234.5".to_string()),
            Value::Decimal("6000.75".to_string()),
            Value::Decimal("123456.789".to_string()),
        ],
        "{} row 0",
        adtg_path.display()
    );
    assert_eq!(
        view.rows[1].values,
        vec![Value::Integer(2), Value::Null, Value::Null, Value::Null],
        "{} row 1",
        adtg_path.display()
    );
}

#[test]
fn native_adtg_matches_random_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let mut checked = 0usize;
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !file_name.starts_with("random_")
            || path.extension().and_then(|ext| ext.to_str()) != Some("adtg")
        {
            continue;
        }

        let xml_path = path.with_extension("xml");
        let native = parse_adtg_bytes(&fs::read(&path).unwrap()).unwrap_or_else(|err| {
            panic!("failed to parse native ADTG {}: {err:#}", path.display())
        });
        let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
            .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
        assert_materialized_matches(&native, &expected, path.display().to_string());
        checked += 1;
    }

    assert!(checked >= 100, "expected at least 100 random ADTG files");
}

#[test]
fn native_adtg_matches_wide_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("wide_0016.adtg");
    let xml_path = dir.join("wide_0016.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 16, "{}", adtg_path.display());
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_wide_0048_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("wide_0048.adtg");
    let xml_path = dir.join("wide_0048.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 48, "{}", adtg_path.display());
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_wide_0065_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("wide_0065.adtg");
    let xml_path = dir.join("wide_0065.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 65, "{}", adtg_path.display());
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_wide_0129_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("wide_0129.adtg");
    let xml_path = dir.join("wide_0129.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 129, "{}", adtg_path.display());
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_all_supported_types_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("all_supported_types.adtg");
    let xml_path = dir.join("all_supported_types.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 31, "{}", adtg_path.display());
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_empty_rowset_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("empty_rowset.adtg");
    let xml_path = dir.join("empty_rowset.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 4, "{}", adtg_path.display());
    assert!(native.rows.is_empty(), "{} rows", adtg_path.display());
    assert!(native.changes.is_empty(), "{} changes", adtg_path.display());
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_multi_change_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("multi_changes.adtg");
    let xml_path = dir.join("multi_changes.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 7, "{}", adtg_path.display());
    assert_eq!(native.changes.len(), 8, "{}", adtg_path.display());
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_binary_c1_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("binary_c1.adtg");
    let xml_path = dir.join("binary_c1.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 4, "{}", adtg_path.display());
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_binary_full_range_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("binary_full_range.adtg");
    let xml_path = dir.join("binary_full_range.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 4, "{}", adtg_path.display());
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_binary_zero_length_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("binary_zero_length.adtg");
    let xml_path = dir.join("binary_zero_length.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 3, "{}", adtg_path.display());
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_large_varlen_fields_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("large_varlen_fields.adtg");
    let xml_path = dir.join("large_varlen_fields.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(
        field_shapes(&native),
        vec![
            ("ID".to_string(), Some(3), Some(4), false),
            ("VC300".to_string(), Some(200), Some(300), false),
            ("VWC300".to_string(), Some(202), Some(300), false),
            ("VB300".to_string(), Some(204), Some(300), false),
        ],
        "{} field shapes",
        adtg_path.display()
    );
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_large_fixed_fields_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("large_fixed_fields.adtg");
    let xml_path = dir.join("large_fixed_fields.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(
        field_shapes(&native),
        vec![
            ("ID".to_string(), Some(3), Some(4), false),
            ("FC300".to_string(), Some(129), Some(300), false),
            ("FWC300".to_string(), Some(130), Some(300), false),
            ("FB300".to_string(), Some(128), Some(300), false),
        ],
        "{} field shapes",
        adtg_path.display()
    );
    assert!(
        native.fields[1..].iter().all(|field| field.fixed_length),
        "{} fixed-length metadata",
        adtg_path.display()
    );
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_long_flag_fields_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("long_flag_fields.adtg");
    let xml_path = dir.join("long_flag_fields.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(
        field_shapes(&native),
        vec![
            ("ID".to_string(), Some(3), Some(4), false),
            ("LONG_FLAG_VARWCHAR".to_string(), Some(203), Some(120), true),
            ("LONG_FLAG_VARBINARY".to_string(), Some(205), Some(16), true),
        ],
        "{} field shapes",
        adtg_path.display()
    );
    assert_eq!(
        field_flag_pairs(&materialize_default_view(&native).fields),
        vec![
            ("ID".to_string(), 0x10 | 0x04),
            ("LONG_FLAG_VARWCHAR".to_string(), 0x80 | 0x20 | 0x04),
            ("LONG_FLAG_VARBINARY".to_string(), 0x80 | 0x20 | 0x04),
        ],
        "{} ADTG flags",
        adtg_path.display()
    );
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_float_extremes_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("float_extremes.adtg");
    let xml_path = dir.join("float_extremes.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 3, "{}", adtg_path.display());
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
    let default_view = materialize_default_view(&native);
    let negative_zero = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(6)))
        .expect("float_extremes native negative zero row missing");
    assert_negative_zero(negative_zero, 1);
    assert_negative_zero(negative_zero, 2);
}

#[test]
fn native_adtg_matches_utf16_xml_stream_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("utf16_xml_stream.adtg");
    let xml_path = dir.join("utf16_xml_stream.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 3, "{}", adtg_path.display());
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_required_fields_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("required_fields.adtg");
    let xml_path = dir.join("required_fields.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 5, "{}", adtg_path.display());
    assert!(
        native.fields.iter().all(|field| !field.nullable),
        "{} should have only non-nullable fields",
        adtg_path.display()
    );
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_field_attributes_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("field_attributes.adtg");
    let xml_path = dir.join("field_attributes.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 6, "{}", adtg_path.display());
    let native_default = materialize_default_view(&native);
    let expected_default = materialize_default_view(&expected);
    assert_eq!(
        field_flag_pairs(&native_default.fields),
        field_attributes_adtg_expected_flags(),
        "{} ADTG flags",
        adtg_path.display()
    );
    assert_eq!(
        field_flag_pairs(&expected_default.fields),
        field_attributes_xml_expected_flags(),
        "{} XML flags",
        xml_path.display()
    );
    assert_ordered_rows_eq(
        native_default.rows,
        expected_default.rows,
        &adtg_path.display().to_string(),
    );
    assert_rows_unordered_eq(
        materialize_pending_view(&native).rows,
        materialize_pending_view(&expected).rows,
        &adtg_path.display().to_string(),
    );
}

#[test]
fn native_adtg_matches_rowid_negative_scale_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("rowid_negative_scale.adtg");
    let xml_path = dir.join("rowid_negative_scale.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    let native_default = materialize_default_view(&native);
    let expected_default = materialize_default_view(&expected);
    assert_eq!(
        field_flag_pairs(&native_default.fields),
        rowid_negative_scale_adtg_expected_flags(),
        "{} ADTG flags",
        adtg_path.display()
    );
    assert_eq!(
        field_flag_pairs(&expected_default.fields),
        rowid_negative_scale_xml_expected_flags(),
        "{} XML flags",
        xml_path.display()
    );
    assert_ordered_rows_eq(
        native_default.rows,
        expected_default.rows,
        &adtg_path.display().to_string(),
    );
}

#[test]
fn native_adtg_matches_fractional_timestamp_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("fractional_timestamp.adtg");
    let xml_path = dir.join("fractional_timestamp.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 2, "{}", adtg_path.display());
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_filetime_fraction_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("filetime_fraction.adtg");
    let xml_path = dir.join("filetime_fraction.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 2, "{}", adtg_path.display());
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_pre_epoch_date_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("pre_epoch_date.adtg");
    let xml_path = dir.join("pre_epoch_date.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 2, "{}", adtg_path.display());
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_temporal_extremes_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("temporal_extremes.adtg");
    let xml_path = dir.join("temporal_extremes.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 6, "{}", adtg_path.display());
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_name_mapping_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("name_mapping.adtg");
    let xml_path = dir.join("name_mapping.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 5, "{}", adtg_path.display());
    assert_eq!(field_names(&native), name_mapping_expected_names());
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_special_field_names_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("special_field_names.adtg");
    let xml_path = dir.join("special_field_names.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 6, "{}", adtg_path.display());
    assert_eq!(field_names(&native), special_field_names_expected_names());
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_whitespace_field_names_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("whitespace_field_names.adtg");
    let xml_path = dir.join("whitespace_field_names.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 6, "{}", adtg_path.display());
    assert_eq!(
        field_names(&native),
        whitespace_field_names_expected_names()
    );
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_text_escapes_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("text_escapes.adtg");
    let xml_path = dir.join("text_escapes.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 4, "{}", adtg_path.display());
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_text_controls_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("text_controls.adtg");
    let xml_path = dir.join("text_controls.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 7, "{}", adtg_path.display());
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_reserved_row_attrs_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("reserved_row_attrs.adtg");
    let xml_path = dir.join("reserved_row_attrs.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 3, "{}", adtg_path.display());
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_text_korean_ansi_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("text_korean_ansi.adtg");
    let xml_path = dir.join("text_korean_ansi.xml");
    if !adtg_path.exists() || !xml_path.exists() {
        return;
    }

    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 4, "{}", adtg_path.display());
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_text_spaces_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("text_spaces.adtg");
    let xml_path = dir.join("text_spaces.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 6, "{}", adtg_path.display());
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_text_empty_strings_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("text_empty_strings.adtg");
    let xml_path = dir.join("text_empty_strings.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 7, "{}", adtg_path.display());
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_unicode_supplementary_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let adtg_path = dir.join("unicode_supplementary.adtg");
    let xml_path = dir.join("unicode_supplementary.xml");
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));

    assert_eq!(native.fields.len(), 4, "{}", adtg_path.display());
    assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
}

#[test]
fn native_adtg_matches_type_matrix_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let mut checked = 0usize;
    for row in read_csv_rows(&dir.join("type_matrix.csv")) {
        if row.get(2).map(String::as_str) != Some("ok") {
            continue;
        }

        let type_name = &row[0];
        let adtg_path = dir.join(format!("type_{type_name}.adtg"));
        let xml_path = dir.join(format!("type_{type_name}.xml"));

        let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
            panic!(
                "failed to parse native type-matrix ADTG {}: {err:#}",
                adtg_path.display()
            )
        });
        let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap()).unwrap_or_else(|err| {
            panic!(
                "failed to parse type-matrix XML {}: {err:#}",
                xml_path.display()
            )
        });
        assert_materialized_matches(&native, &expected, adtg_path.display().to_string());
        checked += 1;
    }

    assert_eq!(checked, 30, "supported type-matrix ADTG files");
}

fn assert_native_matches_xml(adtg: &[u8], xml: &[u8]) {
    let native = parse_adtg_bytes(adtg).unwrap();
    let expected = parse_ado_xml_bytes(xml).unwrap();

    assert_materialized_matches(&native, &expected, "inline fixture".to_string());
}

fn assert_materialized_matches(native: &Recordset, expected: &Recordset, label: String) {
    assert_eq!(native.fields.len(), expected.fields.len(), "{label}");
    let native_default = materialize_default_view(native);
    let expected_default = materialize_default_view(expected);
    assert_fields_match(&native_default.fields, &expected_default.fields, &label);
    assert_ordered_rows_eq(native_default.rows, expected_default.rows, &label);
    let native_pending = materialize_pending_view(native);
    let expected_pending = materialize_pending_view(expected);
    assert_fields_match(&native_pending.fields, &expected_pending.fields, &label);
    assert_rows_unordered_eq(native_pending.rows, expected_pending.rows, &label);
}

fn assert_fields_match(left: &[MaterializedField], right: &[MaterializedField], label: &str) {
    assert_eq!(
        field_identities(left),
        field_identities(right),
        "{label}: fields"
    );
}

fn field_identities(fields: &[MaterializedField]) -> Vec<(&str, Option<u16>, usize, i32, u32)> {
    fields
        .iter()
        .map(|field| {
            (
                field.name.as_str(),
                field.ado_type_code,
                field.precision.unwrap_or(0),
                field.scale.unwrap_or(0),
                field.attribute_flags,
            )
        })
        .collect()
}

fn field_names(recordset: &Recordset) -> Vec<String> {
    recordset
        .fields
        .iter()
        .map(|field| field.name.clone())
        .collect()
}

fn field_shapes(recordset: &Recordset) -> Vec<(String, Option<u16>, Option<usize>, bool)> {
    recordset
        .fields
        .iter()
        .map(|field| {
            (
                field.name.clone(),
                field.ado_type.map(|ty| ty.code),
                field.max_length,
                field.long,
            )
        })
        .collect()
}

fn field_flag_pairs(fields: &[MaterializedField]) -> Vec<(String, u32)> {
    fields
        .iter()
        .map(|field| (field.name.clone(), field.attribute_flags))
        .collect()
}

fn field_attributes_xml_expected_flags() -> Vec<(String, u32)> {
    vec![
        ("ID_KEY".to_string(), 0x10 | 0x04),
        ("MAY_DEFER_TEXT".to_string(), 0x20 | 0x02 | 0x04),
        ("MAYBENULL_TEXT".to_string(), 0x40 | 0x04),
        ("UNKNOWN_TEXT".to_string(), 0x20 | 0x08 | 0x04),
        ("ROW_VERSION_TS".to_string(), 0x200 | 0x10 | 0x04),
        ("CACHE_TEXT".to_string(), 0x1000 | 0x20 | 0x04),
    ]
}

fn field_attributes_adtg_expected_flags() -> Vec<(String, u32)> {
    let mut flags = field_attributes_xml_expected_flags();
    flags[0].1 |= 0x8000;
    flags
}

fn rowid_negative_scale_xml_expected_flags() -> Vec<(String, u32)> {
    vec![
        ("ROW_ID_INT".to_string(), 0x100 | 0x10 | 0x04),
        ("NEG_SCALE_DEC".to_string(), 0x10 | 0x20 | 0x04),
    ]
}

fn rowid_negative_scale_adtg_expected_flags() -> Vec<(String, u32)> {
    vec![
        ("ROW_ID_INT".to_string(), 0x100 | 0x10 | 0x04),
        ("NEG_SCALE_DEC".to_string(), 0x4000 | 0x10 | 0x20 | 0x04),
    ]
}

fn assert_negative_zero(row: &MaterializedRow, field_index: usize) {
    match row.values.get(field_index) {
        Some(Value::Float(actual)) => assert!(
            actual.to_bits() == (-0.0f64).to_bits(),
            "field {field_index}: expected negative zero bits, got {actual:?}"
        ),
        other => panic!("field {field_index}: expected negative zero, got {other:?}"),
    }
}

fn name_mapping_expected_names() -> Vec<String> {
    vec![
        "ID".to_string(),
        "Field Space Text".to_string(),
        "1LeadingInteger".to_string(),
        "Name-With-Dash".to_string(),
        "\u{d55c}\u{ae00} \u{d544}\u{b4dc}".to_string(),
    ]
}

fn special_field_names_expected_names() -> Vec<String> {
    vec![
        "ID".to_string(),
        "Amp & Field".to_string(),
        "Quote \" Field".to_string(),
        "Apostrophe ' Field".to_string(),
        "Less < Field".to_string(),
        "Greater > Field".to_string(),
    ]
}

fn whitespace_field_names_expected_names() -> Vec<String> {
    vec![
        "ID".to_string(),
        " ".to_string(),
        "  Edge Name  ".to_string(),
        "Tab\tField".to_string(),
        "Lf\nField".to_string(),
        "Cr\rField".to_string(),
    ]
}

fn assert_ordered_rows_eq(left: Vec<MaterializedRow>, right: Vec<MaterializedRow>, label: &str) {
    assert_eq!(left.len(), right.len(), "{label}: row count");
    for (index, (left, right)) in left.iter().zip(right.iter()).enumerate() {
        assert_rows_match(left, right, &format!("{label}: row {index}"));
    }
}

fn assert_rows_unordered_eq(left: Vec<MaterializedRow>, right: Vec<MaterializedRow>, label: &str) {
    let mut unmatched = right;
    for row in left {
        let index = unmatched
            .iter()
            .position(|candidate| rows_match(candidate, &row))
            .unwrap_or_else(|| panic!("{label}: pending row not found: {row:?}"));
        unmatched.remove(index);
    }
    assert!(
        unmatched.is_empty(),
        "{label}: unmatched pending rows: {unmatched:?}"
    );
}

fn assert_rows_match(left: &MaterializedRow, right: &MaterializedRow, label: &str) {
    assert!(
        rows_match(left, right),
        "{label}: left={left:?} right={right:?}"
    );
}

fn rows_match(left: &MaterializedRow, right: &MaterializedRow) -> bool {
    left.status == right.status
        && left.values.len() == right.values.len()
        && left
            .values
            .iter()
            .zip(right.values.iter())
            .all(|(left, right)| values_match(left, right))
}

fn values_match(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Float(left), Value::Float(right)) => float_values_match(*left, *right),
        (Value::Decimal(left), Value::Decimal(right)) => numeric_text_matches(left, right),
        (Value::DateTime(left), Value::DateTime(right)) => {
            canonical_datetime_text(left) == canonical_datetime_text(right)
        }
        _ => left == right,
    }
}

fn float_values_match(left: f64, right: f64) -> bool {
    if left == right {
        return true;
    }
    let scale = left.abs().max(right.abs()).max(1.0);
    (left - right).abs() <= scale * 0.000001
}

fn numeric_text_matches(left: &str, right: &str) -> bool {
    canonical_decimal_text(left) == canonical_decimal_text(right)
}

fn canonical_datetime_text(raw: &str) -> String {
    let Some((head, fraction)) = raw.split_once('.') else {
        return raw.to_string();
    };
    let fraction = fraction.trim_end_matches('0');
    if fraction.is_empty() {
        head.to_string()
    } else {
        format!("{head}.{fraction}")
    }
}

fn canonical_decimal_text(raw: &str) -> String {
    let trimmed = raw.trim();
    let (negative, body) = trimmed
        .strip_prefix('-')
        .map(|body| (true, body))
        .unwrap_or((false, trimmed));
    let (whole, fraction) = body
        .split_once('.')
        .map(|(whole, fraction)| (whole, Some(fraction)))
        .unwrap_or((body, None));
    let whole = whole.trim_start_matches('0');
    let whole = if whole.is_empty() { "0" } else { whole };
    let fraction = fraction.map(|value| value.trim_end_matches('0'));
    match fraction {
        Some(fraction) if !fraction.is_empty() && negative => format!("-{whole}.{fraction}"),
        Some(fraction) if !fraction.is_empty() => format!("{whole}.{fraction}"),
        _ if negative && whole != "0" => format!("-{whole}"),
        _ => whole.to_string(),
    }
}

fn replace_once(data: &mut Vec<u8>, from: &[u8], to: &[u8]) {
    let Some(index) = data.windows(from.len()).position(|window| window == from) else {
        panic!("pattern not found: {}", hex_bytes(from));
    };
    data.splice(index..index + from.len(), to.iter().copied());
}

fn try_replace_once(data: &mut Vec<u8>, from: &[u8], to: &[u8]) -> bool {
    let Some(index) = data.windows(from.len()).position(|window| window == from) else {
        return false;
    };
    data.splice(index..index + from.len(), to.iter().copied());
    true
}

fn replace_variant_payload_value(data: &mut [u8], prefix: &[u8], to: &[u8]) {
    let Some(index) = data
        .windows(prefix.len())
        .position(|window| window == prefix)
    else {
        panic!("pattern not found: {}", hex_bytes(prefix));
    };
    let value_index = index + prefix.len() + 6;
    data[value_index..value_index + to.len()].copy_from_slice(to);
}

fn assert_contains(data: &[u8], pattern: &[u8]) {
    assert!(
        data.windows(pattern.len()).any(|window| window == pattern),
        "pattern not found: {}",
        hex_bytes(pattern)
    );
}

fn replace_value_field_descriptor_width(
    data: &mut Vec<u8>,
    type_code: u16,
    from_width: u16,
    to_width: u16,
) {
    let from = value_field_descriptor_type_width(type_code, from_width);
    let to = value_field_descriptor_type_width(type_code, to_width);
    replace_once(data, &from, &to);
}

fn replace_value_field_descriptor_ordinal(
    data: &mut Vec<u8>,
    type_code: u16,
    width: u16,
    from_ordinal: u16,
    to_ordinal: u16,
) {
    let from = value_field_descriptor_type_width_with_ordinal(type_code, width, from_ordinal);
    let to = value_field_descriptor_type_width_with_ordinal(type_code, width, to_ordinal);
    replace_once(data, &from, &to);
}

fn value_field_descriptor_type_width(type_code: u16, width: u16) -> Vec<u8> {
    value_field_descriptor_type_width_with_ordinal(type_code, width, 2)
}

fn value_field_descriptor_type_width_with_ordinal(
    type_code: u16,
    width: u16,
    ordinal: u16,
) -> Vec<u8> {
    let mut bytes = vec![0x06, 0x33, 0x00, 0x80, 0x01, 0x00];
    bytes.extend_from_slice(&ordinal.to_le_bytes());
    bytes.extend_from_slice(&[
        0x0B, 0x00, 0x56, 0x00, 0x41, 0x00, 0x4C, 0x00, 0x55, 0x00, 0x45, 0x00, 0x5F, 0x00, 0x46,
        0x00, 0x49, 0x00, 0x45, 0x00, 0x4C, 0x00, 0x44, 0x00,
    ]);
    bytes.extend_from_slice(&type_code.to_le_bytes());
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn read_csv_rows(path: &Path) -> Vec<Vec<String>> {
    let text = fs::read_to_string(path).unwrap();
    text.lines().skip(1).map(parse_csv_line).collect()
}

fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;

    while let Some(ch) = chars.next() {
        match ch {
            '"' if in_quotes && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                fields.push(field);
                field = String::new();
            }
            _ => field.push(ch),
        }
    }
    fields.push(field);
    fields
}
