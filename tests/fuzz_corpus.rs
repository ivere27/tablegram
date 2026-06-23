use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use tablegram::adtg::{inspect_adtg, parse_adtg_bytes};
use tablegram::compat::{materialize_default_view, materialize_pending_view, MaterializedRow};
use tablegram::detect::{detect_format, RecordsetFormat};
use tablegram::model::{RecordStatusFlag, Value};
use tablegram::parse_recordset_bytes;
use tablegram::xml::parse_ado_xml_bytes;

const TYPE_MATRIX_SUPPORTED: &[(&str, &str)] = &[
    ("TinyInt", "16"),
    ("UnsignedTinyInt", "17"),
    ("SmallInt", "2"),
    ("UnsignedSmallInt", "18"),
    ("Integer", "3"),
    ("UnsignedInt", "19"),
    ("BigInt", "20"),
    ("UnsignedBigInt", "21"),
    ("Single", "4"),
    ("Double", "5"),
    ("Currency", "6"),
    ("Boolean", "11"),
    ("Date", "7"),
    ("DBDate", "133"),
    ("DBTime", "134"),
    ("DBTimeStamp", "135"),
    ("FileTime", "64"),
    ("GUID", "72"),
    ("BSTR", "8"),
    ("Char", "129"),
    ("WChar", "130"),
    ("VarChar", "200"),
    ("VarWChar", "202"),
    ("LongVarChar", "201"),
    ("LongVarWChar", "203"),
    ("Binary", "128"),
    ("VarBinary", "204"),
    ("LongVarBinary", "205"),
    ("Numeric", "131"),
    ("Decimal", "14"),
];

const TYPE_MATRIX_UNSUPPORTED: &[(&str, &str)] = &[
    ("Empty", "0"),
    ("VarNumeric", "139"),
    ("Error", "10"),
    ("Variant", "12"),
    ("IDispatch", "9"),
    ("IUnknown", "13"),
    ("Chapter", "136"),
    ("PropVariant", "138"),
    ("UserDefined", "132"),
    ("ArrayInteger", "8195"),
];

const FIELD_ATTRIBUTE_MATRIX_UNSUPPORTED: &[(&str, &str, &str)] = &[
    ("IsRowURL", "adVarWChar", "65568"),
    ("IsDefaultStream", "adLongVarWChar", "131232"),
    ("IsCollection", "adVarWChar", "262176"),
    ("IsChapter", "adInteger", "8192"),
];

const SCHEMA_SHAPE_MATRIX_UNSUPPORTED: &[(&str, &str, &str)] = &[
    ("empty_field_name", "append", "3001"),
    ("duplicate_field_name", "append", "3367"),
    ("zero_fields", "open", "3709"),
];

const XML_READER_MATRIX_UNSUPPORTED: &[(&str, &str, &str)] = &[
    ("missing_required_current", "default_view", "-2147467259"),
    ("invalid_int_value", "default_view", "-2147467259"),
    ("invalid_boolean_value", "default_view", "-2147467259"),
    ("number_without_dbtype", "default_view", "-2147467259"),
    ("number_len1_without_dbtype", "default_view", "-2147467259"),
    ("number_len2_without_dbtype", "default_view", "-2147467259"),
    (
        "number_len4_overflow_without_dbtype",
        "default_view",
        "-2147467259",
    ),
    ("unbraced_uuid_value", "default_view", "-2147467259"),
    ("invalid_uuid_value", "default_view", "-2147467259"),
];

const XML_READER_MATRIX_ACCEPTED: &[(&str, &str)] = &[
    ("invalid_bin_hex_value", "default_view"),
    ("empty_type", "default_view"),
    ("error_type", "default_view"),
    ("variant_type", "default_view"),
];

const STREAM_ENCODING_MATRIX_ACCEPTED: &[(&str, &str, &str)] = &[
    ("unicode_text_stream", "unicode", "reopen"),
    ("unicodefffe_text_stream", "unicodeFFFE", "reopen"),
    ("utf16_text_stream", "utf-16", "reopen"),
    ("utf16be_text_stream", "utf-16BE", "reopen"),
];

const STREAM_ENCODING_MATRIX_UNSUPPORTED: &[(&str, &str, &str, &str)] =
    &[("utf8_text_stream", "utf-8", "reopen", "-2147467259")];

const FLOAT_SPECIAL_MATRIX_UNSUPPORTED: &[(&str, &str, &str, &str, &str)] = &[
    ("Single", "4", "nan", "save_xml", "-2147217887"),
    (
        "Single",
        "4",
        "positive_infinity",
        "save_xml",
        "-2147217887",
    ),
    (
        "Single",
        "4",
        "negative_infinity",
        "save_xml",
        "-2147217887",
    ),
    ("Double", "5", "nan", "save_xml", "-2147467262"),
    (
        "Double",
        "5",
        "positive_infinity",
        "save_xml",
        "-2147467262",
    ),
    (
        "Double",
        "5",
        "negative_infinity",
        "save_xml",
        "-2147467262",
    ),
];

const FILTER_SAVE_MATRIX_ACCEPTED: &[(&str, &str, &str)] = &[
    ("filter_save_none", "none", "0"),
    ("filter_save_pending", "pending", "1"),
    ("filter_save_affected", "affected", "2"),
    ("filter_save_fetched", "fetched", "3"),
    ("filter_save_conflicting", "conflicting", "5"),
];

const FILTER_SAVE_MATRIX_UNSUPPORTED: &[(&str, &str, &str, &str, &str)] = &[
    (
        "filter_save_criteria_id_1",
        "criteria_id_1",
        "ID = 1",
        "set_filter",
        "-2147467262",
    ),
    (
        "filter_save_criteria_num_ge_30",
        "criteria_num_ge_30",
        "NUM >= 30",
        "set_filter",
        "-2147467262",
    ),
    (
        "filter_save_criteria_txt_inserted",
        "criteria_txt_inserted",
        "TXT = 'inserted'",
        "set_filter",
        "-2147467262",
    ),
];

#[test]
fn fuzz_artifact_inventory_matches_metadata_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let mut expected = BTreeSet::new();
    let mut missing = Vec::new();

    for row in read_csv_rows(&dir.join("manifest.csv")) {
        assert_eq!(row.len(), 7, "fuzz manifest columns: {row:?}");
        for artifact in &row[4..7] {
            if artifact.is_empty() {
                continue;
            }
            add_expected_artifact(&dir, artifact, &mut expected, &mut missing);
        }
    }

    for row in read_csv_rows(&dir.join("type_matrix.csv")) {
        assert_eq!(row.len(), 7, "type_matrix columns: {row:?}");
        let case_name = format!("type_{}", row[0]);
        if row[2] == "ok" {
            for file_name in [
                format!("{case_name}.xml"),
                format!("{case_name}.adtg"),
                format!("{case_name}.roundtrip.xml"),
            ] {
                add_expected_file_name(&dir, &file_name, &mut expected, &mut missing);
            }
        } else {
            assert_absent(&dir.join(format!("{case_name}.xml")));
            assert_absent(&dir.join(format!("{case_name}.adtg")));
            assert_absent(&dir.join(format!("{case_name}.roundtrip.xml")));
        }
    }

    for row in read_csv_rows(&dir.join("stream_encoding_matrix.csv")) {
        assert_eq!(row.len(), 6, "stream_encoding_matrix columns: {row:?}");
        if row[0] == "unicodefffe_text_stream" && row[2] == "ok" {
            add_expected_file_name(&dir, "utf16be_xml_stream.xml", &mut expected, &mut missing);
        }
    }

    assert!(
        missing.is_empty(),
        "fuzz metadata references missing artifacts: {missing:?}"
    );

    let actual = corpus_artifacts(&dir);
    assert_eq!(actual, expected, "fuzz metadata artifact inventory");
}

#[test]
fn parse_any_fuzz_seed_corpus_exercises_public_parser_without_panics() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fuzz/corpus/parse_any");
    let seeds = corpus_artifacts(&dir);

    assert_eq!(
        seeds.len(),
        692,
        "parse_any fuzz seed count should cover generated, variant, fuzz, and shape ADTG/XML corpora"
    );
    assert!(
        seeds.iter().any(|name| name.starts_with("shape__")),
        "parse_any fuzz seeds should include shaped/chaptered inputs"
    );

    for name in seeds {
        let path = dir.join(&name);
        let bytes = fs::read(&path)
            .unwrap_or_else(|err| panic!("failed to read fuzz seed {}: {err:#}", path.display()));
        let result = std::panic::catch_unwind(|| {
            let _ = parse_recordset_bytes(&bytes);
            let _ = parse_ado_xml_bytes(&bytes);
            if detect_format(&bytes) == RecordsetFormat::Adtg {
                let _ = inspect_adtg(&bytes);
                let _ = parse_adtg_bytes(&bytes);
            }
        });
        assert!(
            result.is_ok(),
            "parse_any fuzz seed panicked: {}",
            path.display()
        );
    }
}

#[test]
fn parses_com_generated_fuzz_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let mut xml_count = 0usize;
    let mut adtg_count = 0usize;
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        match extension(&path).as_deref() {
            Some("xml") => {
                let bytes = fs::read(&path).unwrap();
                let recordset = parse_ado_xml_bytes(&bytes)
                    .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", path.display()));
                assert!(!recordset.fields.is_empty(), "{}", path.display());
                xml_count += 1;
            }
            Some("adtg") => {
                let bytes = fs::read(&path).unwrap();
                let document = inspect_adtg(&bytes)
                    .unwrap_or_else(|err| panic!("failed to inspect {}: {err:#}", path.display()));
                assert_eq!(document.length, bytes.len(), "{}", path.display());
                adtg_count += 1;
            }
            _ => {}
        }
    }

    assert!(xml_count >= 100, "expected at least 100 fuzz XML files");
    assert!(adtg_count >= 100, "expected at least 100 fuzz ADTG files");
}

#[test]
fn fuzz_type_matrix_documents_supported_and_unsupported_mdac_types() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("type_matrix.csv"));
    assert_eq!(
        rows.len(),
        TYPE_MATRIX_SUPPORTED.len() + TYPE_MATRIX_UNSUPPORTED.len(),
        "type_matrix.csv should contain every supported and unsupported probe"
    );

    let mut seen = HashMap::new();
    for row in rows {
        let type_name = row[0].clone();
        assert!(
            seen.insert(type_name.clone(), row).is_none(),
            "duplicate type_matrix row for {type_name}"
        );
    }

    for (type_name, type_code) in TYPE_MATRIX_SUPPORTED {
        let row = seen
            .get(*type_name)
            .unwrap_or_else(|| panic!("missing supported type_matrix row for {type_name}"));
        assert_eq!(row[1], *type_code, "{type_name} code");
        assert_eq!(row[2], "ok", "{type_name} result");
        assert!(
            Path::new(&row[3]).exists(),
            "{type_name} XML probe missing: {}",
            row[3]
        );
        assert!(
            Path::new(&row[4]).exists(),
            "{type_name} ADTG probe missing: {}",
            row[4]
        );
    }

    for (type_name, type_code) in TYPE_MATRIX_UNSUPPORTED {
        let row = seen
            .get(*type_name)
            .unwrap_or_else(|| panic!("missing unsupported type_matrix row for {type_name}"));
        assert_eq!(row[1], *type_code, "{type_name} code");
        assert_eq!(row[2], "fail", "{type_name} result");
        assert!(row[3].is_empty(), "{type_name} XML should not be generated");
        assert!(
            row[4].is_empty(),
            "{type_name} ADTG should not be generated"
        );
        assert!(
            !row[5].is_empty(),
            "{type_name} MDAC failure should record an error number"
        );
        assert_absent(&dir.join(format!("type_{type_name}.xml")));
        assert_absent(&dir.join(format!("type_{type_name}.adtg")));
        assert_absent(&dir.join(format!("type_{type_name}.roundtrip.xml")));
    }
}

#[test]
fn fuzz_field_attribute_matrix_documents_mdac_append_failures() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("field_attribute_matrix.csv"));
    assert_eq!(
        rows.len(),
        FIELD_ATTRIBUTE_MATRIX_UNSUPPORTED.len(),
        "field_attribute_matrix.csv should contain every unsupported client-side flag probe"
    );

    let mut seen = HashMap::new();
    for row in rows {
        let attribute_name = row[0].clone();
        assert!(
            seen.insert(attribute_name.clone(), row).is_none(),
            "duplicate field_attribute_matrix row for {attribute_name}"
        );
    }

    for (attribute_name, field_type, attribute_flags) in FIELD_ATTRIBUTE_MATRIX_UNSUPPORTED {
        let row = seen
            .get(*attribute_name)
            .unwrap_or_else(|| panic!("missing field_attribute_matrix row for {attribute_name}"));
        assert_eq!(row[1], *field_type, "{attribute_name} field type");
        assert_eq!(row[2], *attribute_flags, "{attribute_name} flags");
        assert_eq!(row[3], "fail", "{attribute_name} result");
        assert_eq!(row[4], "3001", "{attribute_name} MDAC error number");
        assert!(
            !row[5].is_empty(),
            "{attribute_name} MDAC failure should record an error description"
        );
    }
}

#[test]
fn fuzz_schema_shape_matrix_documents_mdac_failures() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("schema_shape_matrix.csv"));
    assert_eq!(
        rows.len(),
        SCHEMA_SHAPE_MATRIX_UNSUPPORTED.len(),
        "schema_shape_matrix.csv should contain every unsupported schema-shape probe"
    );

    let mut seen = HashMap::new();
    for row in rows {
        let case_name = row[0].clone();
        assert!(
            seen.insert(case_name.clone(), row).is_none(),
            "duplicate schema_shape_matrix row for {case_name}"
        );
    }

    for (case_name, stage, error_number) in SCHEMA_SHAPE_MATRIX_UNSUPPORTED {
        let row = seen
            .get(*case_name)
            .unwrap_or_else(|| panic!("missing schema_shape_matrix row for {case_name}"));
        assert_eq!(row[1], *stage, "{case_name} stage");
        assert_eq!(row[2], "fail", "{case_name} result");
        assert_eq!(row[3], *error_number, "{case_name} MDAC error number");
        assert!(
            !row[4].is_empty(),
            "{case_name} MDAC failure should record an error description"
        );
    }
}

#[test]
fn fuzz_xml_reader_matrix_documents_mdac_reopen_behavior() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("xml_reader_matrix.csv"));
    assert_eq!(
        rows.len(),
        XML_READER_MATRIX_ACCEPTED.len() + XML_READER_MATRIX_UNSUPPORTED.len(),
        "xml_reader_matrix.csv should contain every XML reader probe"
    );

    let mut seen = HashMap::new();
    for row in rows {
        let case_name = row[0].clone();
        assert!(
            seen.insert(case_name.clone(), row).is_none(),
            "duplicate xml_reader_matrix row for {case_name}"
        );
    }

    for (case_name, stage, error_number) in XML_READER_MATRIX_UNSUPPORTED {
        let row = seen
            .get(*case_name)
            .unwrap_or_else(|| panic!("missing xml_reader_matrix row for {case_name}"));
        assert_eq!(row[1], *stage, "{case_name} stage");
        assert_eq!(row[2], "fail", "{case_name} result");
        assert_eq!(row[3], *error_number, "{case_name} MDAC error number");
        assert!(
            !row[4].is_empty(),
            "{case_name} MDAC failure should record an error description"
        );
        assert_absent(&dir.join(format!("_xml_reader_{case_name}.xml")));
    }

    for (case_name, stage) in XML_READER_MATRIX_ACCEPTED {
        let row = seen
            .get(*case_name)
            .unwrap_or_else(|| panic!("missing xml_reader_matrix row for {case_name}"));
        assert_eq!(row[1], *stage, "{case_name} stage");
        assert_eq!(row[2], "ok", "{case_name} result");
        assert!(row[3].is_empty(), "{case_name} error number");
        assert!(row[4].is_empty(), "{case_name} error description");
        assert_absent(&dir.join(format!("_xml_reader_{case_name}.xml")));
    }
}

#[test]
fn fuzz_stream_encoding_matrix_documents_mdac_reopen_behavior() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("stream_encoding_matrix.csv"));
    assert_eq!(
        rows.len(),
        STREAM_ENCODING_MATRIX_ACCEPTED.len() + STREAM_ENCODING_MATRIX_UNSUPPORTED.len(),
        "stream_encoding_matrix.csv should contain every stream encoding probe"
    );

    let mut seen = HashMap::new();
    for row in rows {
        let case_name = row[0].clone();
        assert!(
            seen.insert(case_name.clone(), row).is_none(),
            "duplicate stream_encoding_matrix row for {case_name}"
        );
    }

    for (case_name, charset, stage) in STREAM_ENCODING_MATRIX_ACCEPTED {
        let row = seen
            .get(*case_name)
            .unwrap_or_else(|| panic!("missing stream_encoding_matrix row for {case_name}"));
        assert_eq!(row[1], *charset, "{case_name} charset");
        assert_eq!(row[2], "ok", "{case_name} result");
        assert_eq!(row[3], *stage, "{case_name} stage");
        assert!(row[4].is_empty(), "{case_name} error number");
        assert!(row[5].is_empty(), "{case_name} error description");
    }

    for (case_name, charset, stage, error_number) in STREAM_ENCODING_MATRIX_UNSUPPORTED {
        let row = seen
            .get(*case_name)
            .unwrap_or_else(|| panic!("missing stream_encoding_matrix row for {case_name}"));
        assert_eq!(row[1], *charset, "{case_name} charset");
        assert_eq!(row[2], "fail", "{case_name} result");
        assert_eq!(row[3], *stage, "{case_name} stage");
        assert_eq!(row[4], *error_number, "{case_name} MDAC error number");
        assert!(
            !row[5].is_empty(),
            "{case_name} MDAC failure should record an error description"
        );
        assert_absent(&dir.join(format!("_{case_name}.xml")));
    }
}

#[test]
fn fuzz_float_special_matrix_documents_mdac_failures() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("float_special_matrix.csv"));
    assert_eq!(
        rows.len(),
        FLOAT_SPECIAL_MATRIX_UNSUPPORTED.len(),
        "float_special_matrix.csv should contain every unsupported float special probe"
    );

    let mut seen = HashMap::new();
    for row in rows {
        let key = format!("{}:{}", row[0], row[2]);
        assert!(
            seen.insert(key.clone(), row).is_none(),
            "duplicate float_special_matrix row for {key}"
        );
    }

    for (type_name, type_code, value_name, stage, error_number) in FLOAT_SPECIAL_MATRIX_UNSUPPORTED
    {
        let key = format!("{type_name}:{value_name}");
        let row = seen
            .get(&key)
            .unwrap_or_else(|| panic!("missing float_special_matrix row for {key}"));
        assert_eq!(row[0], *type_name, "{key} type name");
        assert_eq!(row[1], *type_code, "{key} type code");
        assert_eq!(row[2], *value_name, "{key} value name");
        assert_eq!(row[3], "fail", "{key} result");
        assert_eq!(row[4], *stage, "{key} stage");
        assert_eq!(row[5], *error_number, "{key} MDAC error number");
        assert!(
            !row[6].is_empty(),
            "{key} MDAC failure should record an error description"
        );
        assert_absent(&dir.join(format!("float_special_{type_name}_{value_name}.xml")));
        assert_absent(&dir.join(format!("float_special_{type_name}_{value_name}.adtg")));
    }
}

#[test]
fn fuzz_filter_save_matrix_documents_mdac_filter_save_behavior() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("filter_save_matrix.csv"));
    assert_eq!(
        rows.len(),
        FILTER_SAVE_MATRIX_ACCEPTED.len() + FILTER_SAVE_MATRIX_UNSUPPORTED.len(),
        "filter_save_matrix.csv should contain every filter save probe"
    );

    let mut seen = HashMap::new();
    for row in rows {
        let case_name = row[0].clone();
        assert!(
            seen.insert(case_name.clone(), row).is_none(),
            "duplicate filter_save_matrix row for {case_name}"
        );
    }

    for (case_name, filter_name, filter_value) in FILTER_SAVE_MATRIX_ACCEPTED {
        let row = seen
            .get(*case_name)
            .unwrap_or_else(|| panic!("missing filter_save_matrix row for {case_name}"));
        assert_eq!(row[1], *filter_name, "{case_name} filter name");
        assert_eq!(row[2], *filter_value, "{case_name} filter value");
        assert_eq!(row[3], "ok", "{case_name} result");
        assert!(row[4].is_empty(), "{case_name} stage");
        assert!(row[5].is_empty(), "{case_name} error number");
        assert!(row[6].is_empty(), "{case_name} error description");
        assert_eq!(row[7], "1:2|3:8|4:1", "{case_name} default view");
        assert_eq!(row[8], "1:2|4:1|:4", "{case_name} pending view");
        assert!(row[9].is_empty(), "{case_name} affected view");
        assert!(row[10].is_empty(), "{case_name} conflicting view");

        let xml_path = dir.join(format!("{case_name}.xml"));
        let adtg_path = dir.join(format!("{case_name}.adtg"));
        let roundtrip_path = dir.join(format!("{case_name}.roundtrip.xml"));
        assert!(xml_path.exists(), "{case_name} XML missing");
        assert!(adtg_path.exists(), "{case_name} ADTG missing");
        assert!(roundtrip_path.exists(), "{case_name} roundtrip XML missing");

        let xml = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
            .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
        let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
            panic!(
                "failed to parse native ADTG {}: {err:#}",
                adtg_path.display()
            )
        });
        let roundtrip =
            parse_ado_xml_bytes(&fs::read(&roundtrip_path).unwrap()).unwrap_or_else(|err| {
                panic!(
                    "failed to parse roundtrip XML {}: {err:#}",
                    roundtrip_path.display()
                )
            });

        assert_materialized_views_match(&xml, &native, &adtg_path.display().to_string());
        assert_materialized_views_match(&xml, &roundtrip, &roundtrip_path.display().to_string());
    }

    for (case_name, filter_name, filter_value, stage, error_number) in
        FILTER_SAVE_MATRIX_UNSUPPORTED
    {
        let row = seen
            .get(*case_name)
            .unwrap_or_else(|| panic!("missing filter_save_matrix row for {case_name}"));
        assert_eq!(row[1], *filter_name, "{case_name} filter name");
        assert_eq!(row[2], *filter_value, "{case_name} filter value");
        assert_eq!(row[3], "fail", "{case_name} result");
        assert_eq!(row[4], *stage, "{case_name} stage");
        assert_eq!(row[5], *error_number, "{case_name} MDAC error number");
        assert!(
            !row[6].is_empty(),
            "{case_name} MDAC failure should record an error description"
        );
        assert!(row[7].is_empty(), "{case_name} default view");
        assert!(row[8].is_empty(), "{case_name} pending view");
        assert!(row[9].is_empty(), "{case_name} affected view");
        assert!(row[10].is_empty(), "{case_name} conflicting view");
        assert_absent(&dir.join(format!("{case_name}.xml")));
        assert_absent(&dir.join(format!("{case_name}.adtg")));
        assert_absent(&dir.join(format!("{case_name}.roundtrip.xml")));
    }
}

#[test]
fn bstr_type_matrix_persists_as_long_unicode_text() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("type_BSTR.xml");
    let adtg_path = dir.join("type_BSTR.adtg");
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    for recordset in [&expected, &native] {
        let field = recordset
            .fields
            .iter()
            .find(|field| field.name == "VALUE_FIELD")
            .expect("VALUE_FIELD missing");
        assert_eq!(
            field.ado_type.map(|data_type| data_type.code),
            Some(203),
            "MDAC persists accepted adBSTR fields as adLongVarWChar"
        );
        assert_eq!(field.max_length, None, "persisted BSTR max length");
        assert!(field.long, "persisted BSTR long flag");
    }

    let default_view = materialize_default_view(&expected);
    assert_eq!(default_view.rows.len(), 1, "type_BSTR row count");
    assert_eq!(
        default_view.rows[0].values.get(1),
        Some(&Value::String("r1_p0_\u{d55c}\u{ae00}_<&'>".to_string())),
        "type_BSTR value"
    );
}

#[test]
fn fuzz_manifest_contains_wide_multibyte_mask_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "wide_0016")
        .expect("wide_0016 manifest row missing");
    assert_eq!(row[1], "wide_update_delete_insert");
    assert_eq!(row[2], "16", "wide_0016 should span multi-byte ADTG masks");
    assert_eq!(row[3], "4", "wide_0016 source row count");
    assert!(Path::new(&row[4]).exists(), "wide_0016 XML missing");
    assert!(Path::new(&row[5]).exists(), "wide_0016 ADTG missing");
    assert!(
        Path::new(&row[6]).exists(),
        "wide_0016 ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_wide_0048_mask_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "wide_0048")
        .expect("wide_0048 manifest row missing");
    assert_eq!(row[1], "wide_48_update_delete_insert");
    assert_eq!(row[2], "48", "wide_0048 should span 48 fields");
    assert_eq!(row[3], "5", "wide_0048 source row count");
    assert!(Path::new(&row[4]).exists(), "wide_0048 XML missing");
    assert!(Path::new(&row[5]).exists(), "wide_0048 ADTG missing");
    assert!(
        Path::new(&row[6]).exists(),
        "wide_0048 ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_wide_0065_mask_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "wide_0065")
        .expect("wide_0065 manifest row missing");
    assert_eq!(row[1], "wide_65_update_delete_insert");
    assert_eq!(row[2], "65", "wide_0065 should span 65 fields");
    assert_eq!(row[3], "6", "wide_0065 source row count");
    assert!(Path::new(&row[4]).exists(), "wide_0065 XML missing");
    assert!(Path::new(&row[5]).exists(), "wide_0065 ADTG missing");
    assert!(
        Path::new(&row[6]).exists(),
        "wide_0065 ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_wide_0129_mask_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "wide_0129")
        .expect("wide_0129 manifest row missing");
    assert_eq!(row[1], "wide_129_update_delete_insert");
    assert_eq!(row[2], "129", "wide_0129 should span 129 fields");
    assert_eq!(row[3], "7", "wide_0129 source row count");
    assert!(Path::new(&row[4]).exists(), "wide_0129 XML missing");
    assert!(Path::new(&row[5]).exists(), "wide_0129 ADTG missing");
    assert!(
        Path::new(&row[6]).exists(),
        "wide_0129 ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_all_supported_types_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "all_supported_types")
        .expect("all_supported_types manifest row missing");
    assert_eq!(row[1], "all_supported_types_update_delete_insert");
    assert_eq!(
        row[2], "31",
        "all_supported_types should include ID plus every supported flat type"
    );
    assert_eq!(row[3], "4", "all_supported_types source row count");
    assert!(
        Path::new(&row[4]).exists(),
        "all_supported_types XML missing"
    );
    assert!(
        Path::new(&row[5]).exists(),
        "all_supported_types ADTG missing"
    );
    assert!(
        Path::new(&row[6]).exists(),
        "all_supported_types ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_empty_rowset_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "empty_rowset")
        .expect("empty_rowset manifest row missing");
    assert_eq!(row[1], "empty_rowset");
    assert_eq!(row[2], "4", "empty_rowset field count");
    assert_eq!(row[3], "0", "empty_rowset source row count");
    assert!(Path::new(&row[4]).exists(), "empty_rowset XML missing");
    assert!(Path::new(&row[5]).exists(), "empty_rowset ADTG missing");
    assert!(
        Path::new(&row[6]).exists(),
        "empty_rowset ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_multi_change_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "multi_changes")
        .expect("multi_changes manifest row missing");
    assert_eq!(row[1], "multi_update_delete_insert");
    assert_eq!(row[2], "7", "multi_changes field count");
    assert_eq!(row[3], "6", "multi_changes accepted source row count");
    assert!(Path::new(&row[4]).exists(), "multi_changes XML missing");
    assert!(Path::new(&row[5]).exists(), "multi_changes ADTG missing");
    assert!(
        Path::new(&row[6]).exists(),
        "multi_changes ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_binary_c1_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "binary_c1")
        .expect("binary_c1 manifest row missing");
    assert_eq!(row[1], "binary_c1");
    assert_eq!(row[2], "4", "binary_c1 field count");
    assert_eq!(row[3], "3", "binary_c1 source row count");
    assert!(Path::new(&row[4]).exists(), "binary_c1 XML missing");
    assert!(Path::new(&row[5]).exists(), "binary_c1 ADTG missing");
    assert!(
        Path::new(&row[6]).exists(),
        "binary_c1 ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_binary_full_range_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "binary_full_range")
        .expect("binary_full_range manifest row missing");
    assert_eq!(row[1], "binary_full_range");
    assert_eq!(row[2], "4", "binary_full_range field count");
    assert_eq!(row[3], "3", "binary_full_range source row count");
    assert!(Path::new(&row[4]).exists(), "binary_full_range XML missing");
    assert!(
        Path::new(&row[5]).exists(),
        "binary_full_range ADTG missing"
    );
    assert!(
        Path::new(&row[6]).exists(),
        "binary_full_range ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_binary_zero_length_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "binary_zero_length")
        .expect("binary_zero_length manifest row missing");
    assert_eq!(row[1], "binary_zero_length");
    assert_eq!(row[2], "3", "binary_zero_length field count");
    assert_eq!(row[3], "3", "binary_zero_length source row count");
    assert!(
        Path::new(&row[4]).exists(),
        "binary_zero_length XML missing"
    );
    assert!(
        Path::new(&row[5]).exists(),
        "binary_zero_length ADTG missing"
    );
    assert!(
        Path::new(&row[6]).exists(),
        "binary_zero_length ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_large_varlen_fields_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "large_varlen_fields")
        .expect("large_varlen_fields manifest row missing");
    assert_eq!(row[1], "large_varlen_fields");
    assert_eq!(row[2], "4", "large_varlen_fields field count");
    assert_eq!(row[3], "2", "large_varlen_fields source row count");
    assert!(
        Path::new(&row[4]).exists(),
        "large_varlen_fields XML missing"
    );
    assert!(
        Path::new(&row[5]).exists(),
        "large_varlen_fields ADTG missing"
    );
    assert!(
        Path::new(&row[6]).exists(),
        "large_varlen_fields ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_large_fixed_fields_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "large_fixed_fields")
        .expect("large_fixed_fields manifest row missing");
    assert_eq!(row[1], "large_fixed_fields");
    assert_eq!(row[2], "4", "large_fixed_fields field count");
    assert_eq!(row[3], "3", "large_fixed_fields source row count");
    assert!(
        Path::new(&row[4]).exists(),
        "large_fixed_fields XML missing"
    );
    assert!(
        Path::new(&row[5]).exists(),
        "large_fixed_fields ADTG missing"
    );
    assert!(
        Path::new(&row[6]).exists(),
        "large_fixed_fields ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_long_flag_fields_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "long_flag_fields")
        .expect("long_flag_fields manifest row missing");
    assert_eq!(row[1], "long_flag_fields");
    assert_eq!(row[2], "3", "long_flag_fields field count");
    assert_eq!(row[3], "3", "long_flag_fields source row count");
    assert!(Path::new(&row[4]).exists(), "long_flag_fields XML missing");
    assert!(Path::new(&row[5]).exists(), "long_flag_fields ADTG missing");
    assert!(
        Path::new(&row[6]).exists(),
        "long_flag_fields ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_float_extremes_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "float_extremes")
        .expect("float_extremes manifest row missing");
    assert_eq!(row[1], "float_extremes");
    assert_eq!(row[2], "3", "float_extremes field count");
    assert_eq!(row[3], "4", "float_extremes source row count");
    assert!(Path::new(&row[4]).exists(), "float_extremes XML missing");
    assert!(Path::new(&row[5]).exists(), "float_extremes ADTG missing");
    assert!(
        Path::new(&row[6]).exists(),
        "float_extremes ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_utf16_xml_stream_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "utf16_xml_stream")
        .expect("utf16_xml_stream manifest row missing");
    assert_eq!(row[1], "utf16_xml_stream");
    assert_eq!(row[2], "3", "utf16_xml_stream field count");
    assert_eq!(row[3], "3", "utf16_xml_stream source row count");
    assert!(Path::new(&row[4]).exists(), "utf16_xml_stream XML missing");
    assert!(Path::new(&row[5]).exists(), "utf16_xml_stream ADTG missing");
    assert!(
        Path::new(&row[6]).exists(),
        "utf16_xml_stream ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_required_fields_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "required_fields")
        .expect("required_fields manifest row missing");
    assert_eq!(row[1], "required_fields");
    assert_eq!(row[2], "5", "required_fields field count");
    assert_eq!(row[3], "3", "required_fields source row count");
    assert!(Path::new(&row[4]).exists(), "required_fields XML missing");
    assert!(Path::new(&row[5]).exists(), "required_fields ADTG missing");
    assert!(
        Path::new(&row[6]).exists(),
        "required_fields ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_field_attributes_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "field_attributes")
        .expect("field_attributes manifest row missing");
    assert_eq!(row[1], "field_attributes");
    assert_eq!(row[2], "6", "field_attributes field count");
    assert_eq!(row[3], "1", "field_attributes source row count");
    assert!(Path::new(&row[4]).exists(), "field_attributes XML missing");
    assert!(Path::new(&row[5]).exists(), "field_attributes ADTG missing");
    assert!(
        Path::new(&row[6]).exists(),
        "field_attributes ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_rowid_negative_scale_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "rowid_negative_scale")
        .expect("rowid_negative_scale manifest row missing");
    assert_eq!(row[1], "rowid_negative_scale");
    assert_eq!(row[2], "2", "rowid_negative_scale field count");
    assert_eq!(row[3], "1", "rowid_negative_scale source row count");
    assert!(
        Path::new(&row[4]).exists(),
        "rowid_negative_scale XML missing"
    );
    assert!(
        Path::new(&row[5]).exists(),
        "rowid_negative_scale ADTG missing"
    );
    assert!(
        Path::new(&row[6]).exists(),
        "rowid_negative_scale ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_fractional_timestamp_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "fractional_timestamp")
        .expect("fractional_timestamp manifest row missing");
    assert_eq!(row[1], "fractional_timestamp");
    assert_eq!(row[2], "2", "fractional_timestamp field count");
    assert_eq!(row[3], "3", "fractional_timestamp source row count");
    assert!(
        Path::new(&row[4]).exists(),
        "fractional_timestamp XML missing"
    );
    assert!(
        Path::new(&row[5]).exists(),
        "fractional_timestamp ADTG missing"
    );
    assert!(
        Path::new(&row[6]).exists(),
        "fractional_timestamp ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_filetime_fraction_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "filetime_fraction")
        .expect("filetime_fraction manifest row missing");
    assert_eq!(row[1], "filetime_fraction");
    assert_eq!(row[2], "2", "filetime_fraction field count");
    assert_eq!(row[3], "3", "filetime_fraction source row count");
    assert!(Path::new(&row[4]).exists(), "filetime_fraction XML missing");
    assert!(
        Path::new(&row[5]).exists(),
        "filetime_fraction ADTG missing"
    );
    assert!(
        Path::new(&row[6]).exists(),
        "filetime_fraction ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_pre_epoch_date_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "pre_epoch_date")
        .expect("pre_epoch_date manifest row missing");
    assert_eq!(row[1], "pre_epoch_date");
    assert_eq!(row[2], "2", "pre_epoch_date field count");
    assert_eq!(row[3], "3", "pre_epoch_date source row count");
    assert!(Path::new(&row[4]).exists(), "pre_epoch_date XML missing");
    assert!(Path::new(&row[5]).exists(), "pre_epoch_date ADTG missing");
    assert!(
        Path::new(&row[6]).exists(),
        "pre_epoch_date ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_temporal_extremes_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "temporal_extremes")
        .expect("temporal_extremes manifest row missing");
    assert_eq!(row[1], "temporal_extremes");
    assert_eq!(row[2], "6", "temporal_extremes field count");
    assert_eq!(row[3], "3", "temporal_extremes source row count");
    assert!(Path::new(&row[4]).exists(), "temporal_extremes XML missing");
    assert!(
        Path::new(&row[5]).exists(),
        "temporal_extremes ADTG missing"
    );
    assert!(
        Path::new(&row[6]).exists(),
        "temporal_extremes ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_name_mapping_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "name_mapping")
        .expect("name_mapping manifest row missing");
    assert_eq!(row[1], "xml_name_mapping");
    assert_eq!(row[2], "5", "name_mapping field count");
    assert_eq!(row[3], "3", "name_mapping source row count");
    assert!(Path::new(&row[4]).exists(), "name_mapping XML missing");
    assert!(Path::new(&row[5]).exists(), "name_mapping ADTG missing");
    assert!(
        Path::new(&row[6]).exists(),
        "name_mapping ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_special_field_names_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "special_field_names")
        .expect("special_field_names manifest row missing");
    assert_eq!(row[1], "xml_special_field_names");
    assert_eq!(row[2], "6", "special_field_names field count");
    assert_eq!(row[3], "3", "special_field_names source row count");
    assert!(
        Path::new(&row[4]).exists(),
        "special_field_names XML missing"
    );
    assert!(
        Path::new(&row[5]).exists(),
        "special_field_names ADTG missing"
    );
    assert!(
        Path::new(&row[6]).exists(),
        "special_field_names ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_whitespace_field_names_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "whitespace_field_names")
        .expect("whitespace_field_names manifest row missing");
    assert_eq!(row[1], "xml_whitespace_field_names");
    assert_eq!(row[2], "6", "whitespace_field_names field count");
    assert_eq!(row[3], "3", "whitespace_field_names source row count");
    assert!(
        Path::new(&row[4]).exists(),
        "whitespace_field_names XML missing"
    );
    assert!(
        Path::new(&row[5]).exists(),
        "whitespace_field_names ADTG missing"
    );
    assert!(
        Path::new(&row[6]).exists(),
        "whitespace_field_names ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_text_escapes_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "text_escapes")
        .expect("text_escapes manifest row missing");
    assert_eq!(row[1], "text_escaping_control_chars");
    assert_eq!(row[2], "4", "text_escapes field count");
    assert_eq!(row[3], "3", "text_escapes source row count");
    assert!(Path::new(&row[4]).exists(), "text_escapes XML missing");
    assert!(Path::new(&row[5]).exists(), "text_escapes ADTG missing");
    assert!(
        Path::new(&row[6]).exists(),
        "text_escapes ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_text_controls_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "text_controls")
        .expect("text_controls manifest row missing");
    assert_eq!(row[1], "text_literal_xml_illegal_controls");
    assert_eq!(row[2], "7", "text_controls field count");
    assert_eq!(row[3], "3", "text_controls source row count");
    assert!(Path::new(&row[4]).exists(), "text_controls XML missing");
    assert!(Path::new(&row[5]).exists(), "text_controls ADTG missing");
    assert!(
        Path::new(&row[6]).exists(),
        "text_controls ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_reserved_row_attrs_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "reserved_row_attrs")
        .expect("reserved_row_attrs manifest row missing");
    assert_eq!(row[1], "reserved_row_attribute_names");
    assert_eq!(row[2], "3", "reserved_row_attrs field count");
    assert_eq!(row[3], "2", "reserved_row_attrs source row count");
    assert!(
        Path::new(&row[4]).exists(),
        "reserved_row_attrs XML missing"
    );
    assert!(
        Path::new(&row[5]).exists(),
        "reserved_row_attrs ADTG missing"
    );
    assert!(
        Path::new(&row[6]).exists(),
        "reserved_row_attrs ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_doc_minimal_schema_xml_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "doc_minimal_schema")
        .expect("doc_minimal_schema manifest row missing");
    assert_eq!(row[1], "documented_minimal_schema_xml");
    assert_eq!(row[2], "4", "doc_minimal_schema field count");
    assert_eq!(row[3], "2", "doc_minimal_schema source row count");
    assert!(
        Path::new(&row[4]).exists(),
        "doc_minimal_schema XML missing"
    );
    assert!(
        Path::new(&row[5]).exists(),
        "doc_minimal_schema ADTG missing"
    );
    assert!(
        Path::new(&row[6]).exists(),
        "doc_minimal_schema ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_doc_schema_attribute_refs_xml_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "doc_schema_attribute_refs")
        .expect("doc_schema_attribute_refs manifest row missing");
    assert_eq!(row[1], "documented_schema_attribute_refs_xml");
    assert_eq!(row[2], "3", "doc_schema_attribute_refs field count");
    assert_eq!(row[3], "2", "doc_schema_attribute_refs source row count");
    assert!(
        Path::new(&row[4]).exists(),
        "doc_schema_attribute_refs XML missing"
    );
    assert!(
        Path::new(&row[5]).exists(),
        "doc_schema_attribute_refs ADTG missing"
    );
    assert!(
        Path::new(&row[6]).exists(),
        "doc_schema_attribute_refs ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_doc_base64_type_fallback_xml_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "doc_base64_type_fallback")
        .expect("doc_base64_type_fallback manifest row missing");
    assert_eq!(row[1], "documented_base64_type_fallback_xml");
    assert_eq!(row[2], "5", "doc_base64_type_fallback field count");
    assert_eq!(row[3], "2", "doc_base64_type_fallback source row count");
    assert!(
        Path::new(&row[4]).exists(),
        "doc_base64_type_fallback XML missing"
    );
    assert!(
        Path::new(&row[5]).exists(),
        "doc_base64_type_fallback ADTG missing"
    );
    assert!(
        Path::new(&row[6]).exists(),
        "doc_base64_type_fallback ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_doc_datetime_tz_fallback_xml_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "doc_datetime_tz_fallback")
        .expect("doc_datetime_tz_fallback manifest row missing");
    assert_eq!(row[1], "documented_datetime_tz_fallback_xml");
    assert_eq!(row[2], "5", "doc_datetime_tz_fallback field count");
    assert_eq!(row[3], "2", "doc_datetime_tz_fallback source row count");
    assert!(
        Path::new(&row[4]).exists(),
        "doc_datetime_tz_fallback XML missing"
    );
    assert!(
        Path::new(&row[5]).exists(),
        "doc_datetime_tz_fallback ADTG missing"
    );
    assert!(
        Path::new(&row[6]).exists(),
        "doc_datetime_tz_fallback ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_doc_empty_error_variant_types_xml_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "doc_empty_error_variant_types")
        .expect("doc_empty_error_variant_types manifest row missing");
    assert_eq!(row[1], "documented_empty_error_variant_types_xml");
    assert_eq!(row[2], "4", "doc_empty_error_variant_types field count");
    assert_eq!(
        row[3], "2",
        "doc_empty_error_variant_types source row count"
    );
    assert!(
        Path::new(&row[4]).exists(),
        "doc_empty_error_variant_types XML missing"
    );
    assert_eq!(row[5], "", "doc_empty_error_variant_types has no ADTG");
    assert_eq!(
        row[6], "",
        "doc_empty_error_variant_types has no ADTG roundtrip"
    );
}

#[test]
fn fuzz_manifest_contains_doc_float_type_aliases_xml_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "doc_float_type_aliases")
        .expect("doc_float_type_aliases manifest row missing");
    assert_eq!(row[1], "documented_float_type_aliases_xml");
    assert_eq!(row[2], "8", "doc_float_type_aliases field count");
    assert_eq!(row[3], "2", "doc_float_type_aliases source row count");
    assert!(
        Path::new(&row[4]).exists(),
        "doc_float_type_aliases XML missing"
    );
    if row[5].is_empty() {
        assert_absent(&dir.join("doc_float_type_aliases.adtg"));
        assert_absent(&dir.join("doc_float_type_aliases.roundtrip.xml"));
    } else {
        assert!(
            Path::new(&row[5]).exists(),
            "doc_float_type_aliases ADTG missing"
        );
        assert!(
            Path::new(&row[6]).exists(),
            "doc_float_type_aliases ADTG-to-XML roundtrip missing"
        );
    }
}

#[test]
fn fuzz_manifest_contains_doc_numeric_type_aliases_xml_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "doc_numeric_type_aliases")
        .expect("doc_numeric_type_aliases manifest row missing");
    assert_eq!(row[1], "documented_numeric_type_aliases_xml");
    assert_eq!(row[2], "7", "doc_numeric_type_aliases field count");
    assert_eq!(row[3], "2", "doc_numeric_type_aliases source row count");
    assert!(
        Path::new(&row[4]).exists(),
        "doc_numeric_type_aliases XML missing"
    );
    assert!(
        Path::new(&row[5]).exists(),
        "doc_numeric_type_aliases ADTG missing"
    );
    assert!(
        Path::new(&row[6]).exists(),
        "doc_numeric_type_aliases ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_doc_number_varnumeric_xml_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "doc_number_varnumeric")
        .expect("doc_number_varnumeric manifest row missing");
    assert_eq!(row[1], "documented_number_varnumeric_xml");
    assert_eq!(row[2], "4", "doc_number_varnumeric field count");
    assert_eq!(row[3], "2", "doc_number_varnumeric source row count");
    assert!(
        Path::new(&row[4]).exists(),
        "doc_number_varnumeric XML missing"
    );
    assert!(
        Path::new(&row[5]).exists(),
        "doc_number_varnumeric ADTG missing"
    );
    assert!(
        Path::new(&row[6]).exists(),
        "doc_number_varnumeric ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_doc_number_varnumeric_small_width_xml_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "doc_number_varnumeric_small_width")
        .expect("doc_number_varnumeric_small_width manifest row missing");
    assert_eq!(row[1], "documented_number_varnumeric_small_width_xml");
    assert_eq!(row[2], "7", "doc_number_varnumeric_small_width field count");
    assert_eq!(
        row[3], "2",
        "doc_number_varnumeric_small_width source row count"
    );
    assert!(
        Path::new(&row[4]).exists(),
        "doc_number_varnumeric_small_width XML missing"
    );
    assert_eq!(row[5], "", "doc_number_varnumeric_small_width is XML-only");
    assert_eq!(
        row[6], "",
        "doc_number_varnumeric_small_width has no ADTG roundtrip"
    );
}

#[test]
fn fuzz_manifest_contains_doc_nullable_attr_matrix_xml_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "doc_nullable_attr_matrix")
        .expect("doc_nullable_attr_matrix manifest row missing");
    assert_eq!(row[1], "documented_nullable_attribute_matrix_xml");
    assert_eq!(row[2], "9", "doc_nullable_attr_matrix field count");
    assert_eq!(row[3], "2", "doc_nullable_attr_matrix source row count");
    assert!(
        Path::new(&row[4]).exists(),
        "doc_nullable_attr_matrix XML missing"
    );
    assert!(
        Path::new(&row[5]).exists(),
        "doc_nullable_attr_matrix ADTG missing"
    );
    assert!(
        Path::new(&row[6]).exists(),
        "doc_nullable_attr_matrix ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_text_korean_ansi_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let Some(row) = rows.iter().find(|row| row[0] == "text_korean_ansi") else {
        assert_text_korean_ansi_failure_recorded(&dir);
        return;
    };
    assert_eq!(row[1], "text_korean_ansi");
    assert_eq!(row[2], "4", "text_korean_ansi field count");
    assert_eq!(row[3], "3", "text_korean_ansi source row count");
    assert!(Path::new(&row[4]).exists(), "text_korean_ansi XML missing");
    assert!(Path::new(&row[5]).exists(), "text_korean_ansi ADTG missing");
    assert!(
        Path::new(&row[6]).exists(),
        "text_korean_ansi ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_text_spaces_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "text_spaces")
        .expect("text_spaces manifest row missing");
    assert_eq!(row[1], "text_leading_repeated_trailing_spaces");
    assert_eq!(row[2], "6", "text_spaces field count");
    assert_eq!(row[3], "3", "text_spaces source row count");
    assert!(Path::new(&row[4]).exists(), "text_spaces XML missing");
    assert!(Path::new(&row[5]).exists(), "text_spaces ADTG missing");
    assert!(
        Path::new(&row[6]).exists(),
        "text_spaces ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_text_empty_strings_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "text_empty_strings")
        .expect("text_empty_strings manifest row missing");
    assert_eq!(row[1], "text_empty_strings");
    assert_eq!(row[2], "7", "text_empty_strings field count");
    assert_eq!(row[3], "3", "text_empty_strings source row count");
    assert!(
        Path::new(&row[4]).exists(),
        "text_empty_strings XML missing"
    );
    assert!(
        Path::new(&row[5]).exists(),
        "text_empty_strings ADTG missing"
    );
    assert!(
        Path::new(&row[6]).exists(),
        "text_empty_strings ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn fuzz_manifest_contains_unicode_supplementary_case() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "unicode_supplementary")
        .expect("unicode_supplementary manifest row missing");
    assert_eq!(row[1], "unicode_supplementary_plane");
    assert_eq!(row[2], "4", "unicode_supplementary field count");
    assert_eq!(row[3], "3", "unicode_supplementary source row count");
    assert!(
        Path::new(&row[4]).exists(),
        "unicode_supplementary XML missing"
    );
    assert!(
        Path::new(&row[5]).exists(),
        "unicode_supplementary ADTG missing"
    );
    assert!(
        Path::new(&row[6]).exists(),
        "unicode_supplementary ADTG-to-XML roundtrip missing"
    );
}

#[test]
fn doc_minimal_schema_xml_uses_mdac_reader_defaults() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("doc_minimal_schema.xml");
    let recordset = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let view = materialize_default_view(&recordset);

    assert_eq!(view.fields.len(), 4, "doc_minimal_schema field count");
    assert_eq!(view.fields[0].name, "ID");
    assert_eq!(view.fields[0].ado_type_code, Some(3));
    assert_eq!(view.fields[0].attribute_flags, 0x50);
    assert_eq!(view.fields[1].name, "Friendly Name");
    assert_eq!(view.fields[1].ado_type_code, Some(203));
    assert_eq!(view.fields[1].attribute_flags, 0xc0);
    assert_eq!(view.fields[2].name, "Direct Int");
    assert_eq!(view.fields[2].ado_type_code, Some(3));
    assert_eq!(view.fields[2].attribute_flags, 0x50);
    assert_eq!(view.fields[3].name, "Direct Binary");
    assert_eq!(view.fields[3].ado_type_code, Some(205));
    assert_eq!(view.fields[3].attribute_flags, 0xc0);

    assert_eq!(view.rows.len(), 2, "doc_minimal_schema row count");
    assert_eq!(view.rows[0].status, RecordStatusFlag::Unmodified);
    assert_eq!(view.rows[0].values[0], Value::Integer(1));
    assert_eq!(view.rows[0].values[1], Value::String("alpha".to_string()));
    assert_eq!(view.rows[0].values[2], Value::Integer(42));
    assert_eq!(
        view.rows[0].values[3],
        Value::BinaryHex("000102".to_string())
    );
    assert_eq!(view.rows[1].values[0], Value::Integer(2));
    assert_eq!(view.rows[1].values[1], Value::String(String::new()));
    assert_eq!(view.rows[1].values[2], Value::Null);
    assert_eq!(view.rows[1].values[3], Value::Null);
}

#[test]
fn doc_schema_attribute_refs_xml_uses_rowset_field_membership_and_order() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("doc_schema_attribute_refs.xml");
    let recordset = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let view = materialize_default_view(&recordset);

    assert_eq!(
        view.fields.len(),
        3,
        "unreferenced AttributeType should be ignored"
    );
    assert_eq!(view.fields[0].name, "Text First");
    assert_eq!(view.fields[0].ado_type_code, Some(203));
    assert_eq!(view.fields[1].name, "Number After Text");
    assert_eq!(view.fields[1].ado_type_code, Some(3));
    assert_eq!(view.fields[2].name, "Binary Third");
    assert_eq!(view.fields[2].ado_type_code, Some(205));

    assert_eq!(view.rows.len(), 2, "doc_schema_attribute_refs row count");
    assert_eq!(view.rows[0].status, RecordStatusFlag::Unmodified);
    assert_eq!(view.rows[0].values[0], Value::String("alpha".to_string()));
    assert_eq!(view.rows[0].values[1], Value::Integer(10));
    assert_eq!(view.rows[0].values[2], Value::BinaryHex("0A0B".to_string()));
    assert_eq!(view.rows[1].values[0], Value::String("beta".to_string()));
    assert_eq!(view.rows[1].values[1], Value::Integer(20));
    assert_eq!(view.rows[1].values[2], Value::Null);
}

#[test]
fn doc_base64_type_fallback_xml_uses_mdac_text_coercion() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("doc_base64_type_fallback.xml");
    let recordset = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let view = materialize_default_view(&recordset);

    assert_eq!(view.fields.len(), 5, "doc_base64_type_fallback field count");
    assert_eq!(
        view.fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "ID",
            "BASE64_LONG",
            "BASE64_VAR",
            "BASE64_FIXED",
            "BASE64_CHILD"
        ]
    );
    assert_eq!(view.fields[1].ado_type_code, Some(203));
    assert_eq!(view.fields[1].max_length, None);
    assert_eq!(view.fields[1].attribute_flags, 0xc0);
    assert_eq!(view.fields[2].ado_type_code, Some(202));
    assert_eq!(view.fields[2].max_length, Some(12));
    assert_eq!(view.fields[2].attribute_flags, 0x40);
    assert_eq!(view.fields[3].ado_type_code, Some(130));
    assert_eq!(view.fields[3].max_length, Some(12));
    assert_eq!(view.fields[3].attribute_flags, 0x50);
    assert_eq!(view.fields[4].ado_type_code, Some(203));
    assert_eq!(view.fields[4].max_length, None);
    assert_eq!(view.fields[4].attribute_flags, 0xc0);

    assert_eq!(view.rows.len(), 2, "doc_base64_type_fallback row count");
    assert_eq!(view.rows[0].status, RecordStatusFlag::Unmodified);
    assert_eq!(view.rows[0].values[0], Value::Integer(1));
    assert_eq!(
        view.rows[0].values[1],
        Value::String("AAECAwQF+v8=".to_string())
    );
    assert_eq!(view.rows[0].values[2], Value::String("YWJj".to_string()));
    assert_eq!(
        view.rows[0].values[3],
        Value::String("MTIzNA==".to_string())
    );
    assert_eq!(
        view.rows[0].values[4],
        Value::String("ZmllbGQ=".to_string())
    );
    assert_eq!(view.rows[1].values[0], Value::Integer(2));
    assert_eq!(view.rows[1].values[1], Value::Null);
    assert_eq!(view.rows[1].values[2], Value::Null);
    assert_eq!(view.rows[1].values[3], Value::Null);
    assert_eq!(view.rows[1].values[4], Value::Null);
}

#[test]
fn doc_datetime_tz_fallback_xml_uses_mdac_text_coercion() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("doc_datetime_tz_fallback.xml");
    let recordset = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let view = materialize_default_view(&recordset);

    assert_eq!(view.fields.len(), 5, "doc_datetime_tz_fallback field count");
    assert_eq!(
        view.fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "ID",
            "DT_TZ_LONG",
            "DT_TZ_VAR",
            "DT_TZ_FIXED",
            "DT_TZ_CHILD"
        ]
    );
    assert_eq!(view.fields[1].ado_type_code, Some(203));
    assert_eq!(view.fields[1].max_length, None);
    assert_eq!(view.fields[1].attribute_flags, 0xc0);
    assert_eq!(view.fields[2].ado_type_code, Some(202));
    assert_eq!(view.fields[2].max_length, Some(32));
    assert_eq!(view.fields[2].attribute_flags, 0x40);
    assert_eq!(view.fields[3].ado_type_code, Some(130));
    assert_eq!(view.fields[3].max_length, Some(32));
    assert_eq!(view.fields[3].attribute_flags, 0x50);
    assert_eq!(view.fields[4].ado_type_code, Some(203));
    assert_eq!(view.fields[4].max_length, None);
    assert_eq!(view.fields[4].attribute_flags, 0xc0);

    assert_eq!(view.rows.len(), 2, "doc_datetime_tz_fallback row count");
    assert_eq!(view.rows[0].status, RecordStatusFlag::Unmodified);
    assert_eq!(
        view.rows[0].values,
        vec![
            Value::Integer(1),
            Value::String("2026-06-12T01:02:03Z".to_string()),
            Value::String("2026-06-12T01:02:03+09:30".to_string()),
            Value::String("2026-06-12T01:02:03-04:00".to_string()),
            Value::String("2026-06-12T01:02:03Z".to_string()),
        ]
    );
    assert_eq!(view.rows[1].values[0], Value::Integer(2));
    assert!(view.rows[1]
        .values
        .iter()
        .skip(1)
        .all(|value| matches!(value, Value::Null)));
}

#[test]
fn doc_datetime_tz_roundtrip_xml_keeps_mdac_saved_fixed_text_bytes() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("doc_datetime_tz_fallback.roundtrip.xml");
    let recordset = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let view = materialize_default_view(&recordset);

    assert_eq!(view.rows.len(), 2, "ADO-saved roundtrip row count");
    assert_eq!(view.rows[0].values[0], Value::Integer(1));
    let Value::String(fixed_text) = &view.rows[0].values[3] else {
        panic!("fixed dateTime.tz roundtrip value should be text");
    };
    assert!(
        fixed_text.starts_with("2026-06-12T01:02:03-04:00"),
        "fixed dateTime.tz text prefix"
    );
    assert!(
        fixed_text.contains('\0'),
        "fixed dateTime.tz roundtrip should preserve MDAC control bytes"
    );
    assert_eq!(view.rows[1].values[0], Value::Integer(2));
    assert!(view.rows[1]
        .values
        .iter()
        .skip(1)
        .all(|value| matches!(value, Value::Null)));
}

#[test]
fn doc_empty_error_variant_types_xml_uses_mdac_reader_type_rules() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("doc_empty_error_variant_types.xml");
    let recordset = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let view = materialize_default_view(&recordset);

    assert_eq!(
        view.fields.len(),
        4,
        "doc_empty_error_variant_types field count"
    );
    assert_eq!(
        view.fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["ID", "EMPTY_FIELD", "ERROR_FIELD", "VARIANT_FIELD"]
    );
    assert_eq!(view.fields[1].ado_type_code, Some(203));
    assert_eq!(view.fields[1].max_length, None);
    assert_eq!(view.fields[1].attribute_flags, 0xc0);
    assert_eq!(view.fields[2].ado_type_code, Some(203));
    assert_eq!(view.fields[2].max_length, None);
    assert_eq!(view.fields[2].attribute_flags, 0xc0);
    assert_eq!(view.fields[3].ado_type_code, Some(12));
    assert_eq!(view.fields[3].max_length, Some(16));
    assert_eq!(view.fields[3].attribute_flags, 0x50);

    assert_eq!(
        view.rows.len(),
        2,
        "doc_empty_error_variant_types row count"
    );
    assert_eq!(
        view.rows[0].values,
        vec![
            Value::Integer(1),
            Value::String("anything".to_string()),
            Value::String("5".to_string()),
            Value::String("plain text".to_string()),
        ]
    );
    assert_eq!(view.rows[1].values[0], Value::Integer(2));
    assert!(view.rows[1]
        .values
        .iter()
        .skip(1)
        .all(|value| matches!(value, Value::Null)));
}

#[test]
fn doc_float_type_aliases_xml_uses_mdac_reader_type_rules() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("doc_float_type_aliases.xml");
    let recordset = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let view = materialize_default_view(&recordset);

    assert_eq!(view.fields.len(), 8, "doc_float_type_aliases field count");
    assert_eq!(
        view.fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "ID",
            "FLOAT_NO_LEN",
            "FLOAT_LEN4",
            "FLOAT_LEN8",
            "R4_NO_LEN",
            "R4_LEN8",
            "R8_NO_LEN",
            "R8_LEN4"
        ]
    );
    assert_eq!(
        view.fields
            .iter()
            .map(|field| field.ado_type_code)
            .collect::<Vec<_>>(),
        vec![
            Some(3),
            Some(5),
            Some(5),
            Some(5),
            Some(4),
            Some(4),
            Some(5),
            Some(5)
        ]
    );
    assert_eq!(view.fields[2].max_length, Some(4));
    assert_eq!(view.fields[3].max_length, Some(8));
    assert_eq!(view.fields[5].max_length, Some(8));
    assert_eq!(view.fields[7].max_length, Some(4));
    assert!(
        view.fields
            .iter()
            .skip(1)
            .all(|field| field.attribute_flags == 0x50),
        "float alias fields should reopen as fixed MayBeNull values"
    );

    assert_eq!(view.rows.len(), 2, "doc_float_type_aliases row count");
    assert_eq!(view.rows[0].status, RecordStatusFlag::Unmodified);
    assert_eq!(
        view.rows[0].values,
        vec![
            Value::Integer(1),
            Value::Float(1.25),
            Value::Float(0.0),
            Value::Float(3.75),
            Value::Float(4.5),
            Value::Float(5.25),
            Value::Float(6.125),
            Value::Float(0.0),
        ]
    );
    assert_eq!(view.rows[1].values[0], Value::Integer(2));
    assert!(view.rows[1]
        .values
        .iter()
        .skip(1)
        .all(|value| matches!(value, Value::Null)));
}

#[test]
fn doc_numeric_type_aliases_xml_uses_mdac_reader_type_rules() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("doc_numeric_type_aliases.xml");
    let recordset = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let view = materialize_default_view(&recordset);

    assert_eq!(view.fields.len(), 7, "doc_numeric_type_aliases field count");
    assert_eq!(
        view.fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "ID",
            "FIXED_14_4",
            "CURRENCY_ALIAS",
            "DECIMAL_ALIAS",
            "NUMBER_DB_CURRENCY",
            "NUMBER_DB_DECIMAL",
            "NUMBER_DB_NUMERIC"
        ]
    );
    assert_eq!(
        view.fields
            .iter()
            .map(|field| field.ado_type_code)
            .collect::<Vec<_>>(),
        vec![
            Some(3),
            Some(203),
            Some(6),
            Some(14),
            Some(6),
            Some(14),
            Some(131)
        ]
    );
    assert_eq!(
        view.fields
            .iter()
            .map(|field| field.attribute_flags)
            .collect::<Vec<_>>(),
        vec![0x50, 0xc0, 0x50, 0x50, 0x50, 0x50, 0x50]
    );
    assert_eq!(view.fields[6].precision, Some(18));
    assert_eq!(view.fields[6].scale, Some(4));

    assert_eq!(view.rows.len(), 2, "doc_numeric_type_aliases row count");
    assert_eq!(view.rows[0].status, RecordStatusFlag::Unmodified);
    assert_eq!(
        view.rows[0].values,
        vec![
            Value::Integer(1),
            Value::String("1000.1234".to_string()),
            Value::Decimal("2000.5678".to_string()),
            Value::Decimal("3000.25".to_string()),
            Value::Decimal("4000.125".to_string()),
            Value::Decimal("5000.5".to_string()),
            Value::Decimal("6000.75".to_string()),
        ]
    );
    assert_eq!(view.rows[1].values[0], Value::Integer(2));
    assert!(view.rows[1]
        .values
        .iter()
        .skip(1)
        .all(|value| matches!(value, Value::Null)));
}

#[test]
fn doc_number_varnumeric_xml_uses_mdac_reader_type_rules() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("doc_number_varnumeric.xml");
    let recordset = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let view = materialize_default_view(&recordset);

    assert_eq!(view.fields.len(), 4, "doc_number_varnumeric field count");
    assert_eq!(
        view.fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["ID", "NUMBER_LEN8", "NUMBER_LEN16", "NUMBER_LEN19"]
    );
    assert_eq!(
        view.fields
            .iter()
            .map(|field| field.ado_type_code)
            .collect::<Vec<_>>(),
        vec![Some(3), Some(139), Some(139), Some(139)]
    );
    assert_eq!(
        view.fields
            .iter()
            .map(|field| field.max_length)
            .collect::<Vec<_>>(),
        vec![None, Some(8), Some(16), Some(19)]
    );
    assert_eq!(
        view.fields
            .iter()
            .map(|field| field.attribute_flags)
            .collect::<Vec<_>>(),
        vec![0x50, 0x40, 0x40, 0x40]
    );

    assert_eq!(view.rows.len(), 2, "doc_number_varnumeric row count");
    assert_eq!(view.rows[0].status, RecordStatusFlag::Unmodified);
    assert_eq!(
        view.rows[0].values,
        vec![
            Value::Integer(1),
            Value::Decimal("1234.5".to_string()),
            Value::Decimal("6000.75".to_string()),
            Value::Decimal("123456.789".to_string()),
        ]
    );
    assert_eq!(view.rows[1].values[0], Value::Integer(2));
    assert!(view.rows[1]
        .values
        .iter()
        .skip(1)
        .all(|value| matches!(value, Value::Null)));
}

#[test]
fn doc_number_varnumeric_small_width_xml_uses_mdac_truncation_rules() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("doc_number_varnumeric_small_width.xml");
    let recordset = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let view = materialize_default_view(&recordset);

    assert_eq!(
        view.fields.len(),
        7,
        "doc_number_varnumeric_small_width field count"
    );
    assert_eq!(
        view.fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "ID",
            "NUMBER_LEN3_DECIMAL",
            "NUMBER_LEN3_TRAILING_ZERO",
            "NUMBER_LEN4_DECIMAL",
            "NUMBER_LEN4_INTEGER",
            "NUMBER_LEN5_DECIMAL",
            "NUMBER_LEN6_DECIMAL",
        ]
    );
    assert_eq!(
        view.fields
            .iter()
            .map(|field| field.ado_type_code)
            .collect::<Vec<_>>(),
        vec![
            Some(3),
            Some(139),
            Some(139),
            Some(139),
            Some(139),
            Some(139),
            Some(139)
        ]
    );
    assert_eq!(
        view.fields
            .iter()
            .map(|field| field.max_length)
            .collect::<Vec<_>>(),
        vec![None, Some(3), Some(3), Some(4), Some(4), Some(5), Some(6)]
    );
    assert_eq!(
        view.fields
            .iter()
            .map(|field| field.attribute_flags)
            .collect::<Vec<_>>(),
        vec![0x50, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40]
    );

    assert_eq!(
        view.rows.len(),
        2,
        "doc_number_varnumeric_small_width row count"
    );
    assert_eq!(view.rows[0].status, RecordStatusFlag::Unmodified);
    assert_eq!(
        view.rows[0].values,
        vec![
            Value::Integer(1),
            Value::Decimal("0.4".to_string()),
            Value::Decimal("400".to_string()),
            Value::Decimal("14.81".to_string()),
            Value::Decimal("1490".to_string()),
            Value::Decimal("4034.67".to_string()),
            Value::Decimal("6000.75".to_string()),
        ]
    );
    assert_eq!(view.rows[1].values[0], Value::Integer(2));
    assert!(view.rows[1]
        .values
        .iter()
        .skip(1)
        .all(|value| matches!(value, Value::Null)));
}

#[test]
fn doc_nullable_attr_matrix_xml_uses_mdac_field_flag_rules() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("doc_nullable_attr_matrix.xml");
    let recordset = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let view = materialize_default_view(&recordset);

    assert_eq!(view.fields.len(), 9, "doc_nullable_attr_matrix field count");
    assert_eq!(
        view.fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "ID",
            "INT_DEFAULT",
            "INT_MAYBE_TRUE",
            "INT_MAYBE_FALSE",
            "INT_NULLABLE_TRUE",
            "INT_NULLABLE_TRUE_MAYBE_FALSE",
            "INT_NULLABLE_FALSE_MAYBE_TRUE",
            "TEXT_DEFAULT",
            "TEXT_MAYBE_FALSE"
        ]
    );
    assert_eq!(
        view.fields
            .iter()
            .map(|field| field.ado_type_code)
            .collect::<Vec<_>>(),
        vec![
            Some(3),
            Some(3),
            Some(3),
            Some(3),
            Some(3),
            Some(3),
            Some(3),
            Some(203),
            Some(203)
        ]
    );
    assert_eq!(
        view.fields
            .iter()
            .map(|field| field.attribute_flags)
            .collect::<Vec<_>>(),
        vec![0x50, 0x50, 0x50, 0x10, 0x70, 0x30, 0x50, 0xc0, 0x80]
    );

    assert_eq!(view.rows.len(), 2, "doc_nullable_attr_matrix row count");
    assert_eq!(view.rows[0].status, RecordStatusFlag::Unmodified);
    assert_eq!(
        view.rows[0].values,
        vec![
            Value::Integer(1),
            Value::Integer(10),
            Value::Integer(11),
            Value::Integer(12),
            Value::Integer(13),
            Value::Integer(14),
            Value::Integer(15),
            Value::String("alpha".to_string()),
            Value::String("beta".to_string()),
        ]
    );
    assert_eq!(
        view.rows[1].values,
        vec![
            Value::Integer(2),
            Value::Integer(20),
            Value::Integer(21),
            Value::Integer(22),
            Value::Integer(23),
            Value::Integer(24),
            Value::Integer(25),
            Value::String("gamma".to_string()),
            Value::String("delta".to_string()),
        ]
    );
}

#[test]
fn reserved_row_attrs_case_keeps_data_forcenull_distinct_from_rs_forcenull() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("reserved_row_attrs.xml");
    let adtg_path = dir.join("reserved_row_attrs.adtg");
    let recordset = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&recordset, &native, &adtg_path.display().to_string());

    let default = materialize_default_view(&recordset);
    assert_eq!(
        default
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["ID", "forcenull", "VALUE_FIELD"]
    );
    assert_eq!(default.rows.len(), 2, "reserved_row_attrs default rows");
    assert_eq!(default.rows[0].status, RecordStatusFlag::Modified);
    assert_eq!(default.rows[0].values[0], Value::Integer(1));
    assert_eq!(
        default.rows[0].values[1],
        Value::String("field-value-1-updated".to_string())
    );
    assert_eq!(default.rows[0].values[2], Value::Null);
    assert_eq!(default.rows[1].status, RecordStatusFlag::Unmodified);
    assert_eq!(default.rows[1].values[0], Value::Integer(2));
    assert_eq!(default.rows[1].values[1], Value::Null);
    assert_eq!(
        default.rows[1].values[2],
        Value::String("value-2".to_string())
    );

    let pending = materialize_pending_view(&recordset);
    assert_eq!(pending.rows.len(), 1, "reserved_row_attrs pending rows");
    assert_eq!(pending.rows[0].status, RecordStatusFlag::Modified);
    assert_eq!(pending.rows[0].values[0], Value::Integer(1));
    assert_eq!(
        pending.rows[0].values[1],
        Value::String("field-value-1-updated".to_string())
    );
    assert_eq!(pending.rows[0].values[2], Value::Null);
}

#[test]
fn name_mapping_case_preserves_real_field_names() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("name_mapping.xml");
    let adtg_path = dir.join("name_mapping.adtg");
    let expected_names = name_mapping_expected_names();
    let recordset = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&recordset, &native, &adtg_path.display().to_string());
    assert_eq!(
        materialize_default_view(&recordset)
            .fields
            .iter()
            .map(|field| field.name.clone())
            .collect::<Vec<_>>(),
        expected_names,
        "real ADO field names"
    );

    assert_xml_name_is_mapped(&recordset, "Field Space Text");
    assert_xml_name_is_mapped(&recordset, "1LeadingInteger");
    assert_xml_name_is_mapped(&recordset, korean_field_name().as_str());
}

#[test]
fn special_field_names_case_preserves_xml_sensitive_real_names() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("special_field_names.xml");
    let adtg_path = dir.join("special_field_names.adtg");
    let expected_names = special_field_names_expected_names();
    let recordset = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&recordset, &native, &adtg_path.display().to_string());
    assert_eq!(
        materialize_default_view(&recordset)
            .fields
            .iter()
            .map(|field| field.name.clone())
            .collect::<Vec<_>>(),
        expected_names,
        "real ADO field names"
    );
    assert_eq!(
        recordset
            .fields
            .iter()
            .map(|field| field.xml_name.clone())
            .collect::<Vec<_>>(),
        vec![
            "ID".to_string(),
            "c1".to_string(),
            "c2".to_string(),
            "c3".to_string(),
            "c4".to_string(),
            "c5".to_string(),
        ],
        "XML-safe field names"
    );
    for field_name in special_field_names_expected_names().into_iter().skip(1) {
        assert_xml_name_is_mapped(&recordset, &field_name);
    }

    let default_view = materialize_default_view(&recordset);
    assert_status_counts(
        &default_view.rows,
        &[
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::Unmodified, 1),
            (RecordStatusFlag::New, 1),
        ],
        "special_field_names default view",
    );
    let updated = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(1)))
        .expect("special_field_names updated row missing");
    assert_special_field_names_row(updated, 1, 1);
    let unchanged = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(3)))
        .expect("special_field_names unchanged row missing");
    assert_special_field_names_row(unchanged, 3, 0);
    let inserted = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(4)))
        .expect("special_field_names inserted row missing");
    assert_special_field_names_row(inserted, 4, 2);

    let deleted = materialize_pending_view(&recordset)
        .rows
        .into_iter()
        .find(|row| row.values.first() == Some(&Value::Integer(2)))
        .expect("special_field_names deleted row missing");
    assert_special_field_names_row(&deleted, 2, 0);
}

#[test]
fn whitespace_field_names_case_preserves_control_char_real_names() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("whitespace_field_names.xml");
    let adtg_path = dir.join("whitespace_field_names.adtg");
    let expected_names = whitespace_field_names_expected_names();
    let recordset = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&recordset, &native, &adtg_path.display().to_string());
    assert_eq!(
        materialize_default_view(&recordset)
            .fields
            .iter()
            .map(|field| field.name.clone())
            .collect::<Vec<_>>(),
        expected_names,
        "real ADO field names"
    );
    assert_eq!(
        materialize_default_view(&native)
            .fields
            .iter()
            .map(|field| field.name.clone())
            .collect::<Vec<_>>(),
        whitespace_field_names_expected_names(),
        "native ADTG field names"
    );
    assert_eq!(
        recordset
            .fields
            .iter()
            .map(|field| field.xml_name.clone())
            .collect::<Vec<_>>(),
        vec![
            "ID".to_string(),
            "c1".to_string(),
            "c2".to_string(),
            "c3".to_string(),
            "c4".to_string(),
            "c5".to_string(),
        ],
        "XML-safe field names"
    );
    for field_name in whitespace_field_names_expected_names().into_iter().skip(1) {
        assert_xml_name_is_mapped(&recordset, &field_name);
    }

    let default_view = materialize_default_view(&recordset);
    assert_status_counts(
        &default_view.rows,
        &[
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::Unmodified, 1),
            (RecordStatusFlag::New, 1),
        ],
        "whitespace_field_names default view",
    );
    let updated = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(1)))
        .expect("whitespace_field_names updated row missing");
    assert_whitespace_field_names_row(updated, 1, 1);
    let unchanged = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(3)))
        .expect("whitespace_field_names unchanged row missing");
    assert_whitespace_field_names_row(unchanged, 3, 0);
    let inserted = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(4)))
        .expect("whitespace_field_names inserted row missing");
    assert_whitespace_field_names_row(inserted, 4, 2);

    let deleted = materialize_pending_view(&recordset)
        .rows
        .into_iter()
        .find(|row| row.values.first() == Some(&Value::Integer(2)))
        .expect("whitespace_field_names deleted row missing");
    assert_whitespace_field_names_row(&deleted, 2, 0);
}

#[test]
fn text_escapes_case_preserves_raw_control_chars_and_entities() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("text_escapes.xml");
    let adtg_path = dir.join("text_escapes.adtg");
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    let default_view = materialize_default_view(&expected);
    assert_status_counts(
        &default_view.rows,
        &[
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::Unmodified, 1),
            (RecordStatusFlag::New, 1),
        ],
        "text_escapes default view",
    );

    let updated = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(1)))
        .expect("text_escapes updated row missing");
    assert_text_escape_row(updated, 1, 1);

    let null_row = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(3)))
        .expect("text_escapes null row missing");
    assert_eq!(null_row.values.get(1), Some(&Value::Null));
    assert_eq!(null_row.values.get(2), Some(&Value::Null));
    assert_eq!(null_row.values.get(3), Some(&Value::Null));

    let inserted = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(4)))
        .expect("text_escapes inserted row missing");
    assert_text_escape_row(inserted, 4, 2);

    let pending = materialize_pending_view(&expected);
    assert_status_counts(
        &pending.rows,
        &[
            (RecordStatusFlag::Deleted, 1),
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::New, 1),
        ],
        "text_escapes pending view",
    );
    let deleted = pending
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(2)))
        .expect("text_escapes deleted row missing");
    assert_text_escape_row(deleted, 2, 0);
}

#[test]
fn text_controls_case_preserves_literal_xml_illegal_control_chars() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("text_controls.xml");
    let adtg_path = dir.join("text_controls.adtg");
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    let default_view = materialize_default_view(&expected);
    assert_status_counts(
        &default_view.rows,
        &[
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::Unmodified, 1),
            (RecordStatusFlag::New, 1),
        ],
        "text_controls default view",
    );

    let updated = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(1)))
        .expect("text_controls updated row missing");
    assert_text_controls_row(updated, 1, 1);

    let null_row = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(3)))
        .expect("text_controls null row missing");
    for index in 1..=6 {
        assert_eq!(null_row.values.get(index), Some(&Value::Null));
    }

    let inserted = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(4)))
        .expect("text_controls inserted row missing");
    assert_text_controls_row(inserted, 4, 2);

    let pending = materialize_pending_view(&expected);
    assert_status_counts(
        &pending.rows,
        &[
            (RecordStatusFlag::Deleted, 1),
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::New, 1),
        ],
        "text_controls pending view",
    );
    let deleted = pending
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(2)))
        .expect("text_controls deleted row missing");
    assert_text_controls_row(deleted, 2, 0);
}

#[test]
fn text_korean_ansi_case_preserves_exact_multibyte_boundaries() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("text_korean_ansi.xml");
    let adtg_path = dir.join("text_korean_ansi.adtg");
    if !xml_path.exists() || !adtg_path.exists() {
        assert_text_korean_ansi_failure_recorded(&dir);
        return;
    }

    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    let field_shapes = expected
        .fields
        .iter()
        .map(|field| {
            (
                field.name.as_str(),
                field.ado_type.map(|ty| ty.code),
                field.max_length,
                field.fixed_length,
                field.long,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(field_shapes[0], ("ID", Some(3), Some(4), true, false));
    assert_eq!(
        field_shapes[1],
        ("FIXED_ANSI_KR", Some(129), Some(10), true, false)
    );
    assert_eq!(
        field_shapes[2],
        ("VAR_ANSI_KR", Some(200), Some(120), false, false)
    );
    assert_eq!(field_shapes[3].0, "LONG_ANSI_KR");
    assert_eq!(field_shapes[3].1, Some(201));
    assert!(
        matches!(field_shapes[3].2, None | Some(4000)),
        "text_korean_ansi LONG_ANSI_KR max length: {:?}",
        field_shapes[3].2
    );
    assert!(!field_shapes[3].3);
    assert!(field_shapes[3].4);

    let default_view = materialize_default_view(&expected);
    assert_status_counts(
        &default_view.rows,
        &[
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::Unmodified, 1),
            (RecordStatusFlag::New, 1),
        ],
        "text_korean_ansi default view",
    );

    let updated = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(1)))
        .expect("text_korean_ansi updated row missing");
    assert_korean_ansi_row(updated, 1, 1);

    let null_row = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(3)))
        .expect("text_korean_ansi null row missing");
    assert_eq!(null_row.values.get(1), Some(&Value::Null));
    assert_eq!(null_row.values.get(2), Some(&Value::Null));
    assert_eq!(null_row.values.get(3), Some(&Value::Null));

    let inserted = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(4)))
        .expect("text_korean_ansi inserted row missing");
    assert_korean_ansi_row(inserted, 4, 2);

    let pending = materialize_pending_view(&expected);
    assert_status_counts(
        &pending.rows,
        &[
            (RecordStatusFlag::Deleted, 1),
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::New, 1),
        ],
        "text_korean_ansi pending view",
    );
    let deleted = pending
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(2)))
        .expect("text_korean_ansi deleted row missing");
    assert_korean_ansi_row(deleted, 2, 0);
}

#[test]
fn text_spaces_case_preserves_leading_repeated_and_trailing_spaces() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("text_spaces.xml");
    let adtg_path = dir.join("text_spaces.adtg");
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    let default_view = materialize_default_view(&expected);
    assert_status_counts(
        &default_view.rows,
        &[
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::Unmodified, 1),
            (RecordStatusFlag::New, 1),
        ],
        "text_spaces default view",
    );

    let updated = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(1)))
        .expect("text_spaces updated row missing");
    assert_text_space_row(updated, 1, 1);

    let null_row = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(3)))
        .expect("text_spaces null row missing");
    for index in 1..=5 {
        assert_eq!(null_row.values.get(index), Some(&Value::Null));
    }

    let inserted = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(4)))
        .expect("text_spaces inserted row missing");
    assert_text_space_row(inserted, 4, 2);

    let pending = materialize_pending_view(&expected);
    assert_status_counts(
        &pending.rows,
        &[
            (RecordStatusFlag::Deleted, 1),
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::New, 1),
        ],
        "text_spaces pending view",
    );
    let deleted = pending
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(2)))
        .expect("text_spaces deleted row missing");
    assert_text_space_row(deleted, 2, 0);
}

#[test]
fn text_empty_strings_case_preserves_empty_and_null_text_distinction() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("text_empty_strings.xml");
    let adtg_path = dir.join("text_empty_strings.adtg");
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    let field_shapes = expected
        .fields
        .iter()
        .map(|field| {
            (
                field.name.as_str(),
                field.ado_type.map(|ty| ty.code),
                field.max_length,
                field.fixed_length,
                field.long,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        field_shapes,
        vec![
            ("ID", Some(3), Some(4), true, false),
            ("FIXED_ASCII_EMPTY", Some(129), Some(4), true, false),
            ("FIXED_WIDE_EMPTY", Some(130), Some(4), true, false),
            ("VAR_ASCII_EMPTY", Some(200), Some(16), false, false),
            ("VAR_WIDE_EMPTY", Some(202), Some(16), false, false),
            ("LONG_ASCII_EMPTY", Some(201), Some(4000), false, true),
            ("LONG_WIDE_EMPTY", Some(203), Some(4000), false, true),
        ],
        "text_empty_strings metadata"
    );

    let default_view = materialize_default_view(&expected);
    assert_status_counts(
        &default_view.rows,
        &[
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::Unmodified, 1),
            (RecordStatusFlag::New, 1),
        ],
        "text_empty_strings default view",
    );

    let updated = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(1)))
        .expect("text_empty_strings updated row missing");
    assert_empty_text_row(updated, 1);

    let null_row = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(3)))
        .expect("text_empty_strings null row missing");
    for index in 1..=6 {
        assert_eq!(
            null_row.values.get(index),
            Some(&Value::Null),
            "text_empty_strings null field {index}"
        );
    }

    let inserted = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(4)))
        .expect("text_empty_strings inserted row missing");
    assert_empty_text_row(inserted, 4);

    let pending = materialize_pending_view(&expected);
    assert_status_counts(
        &pending.rows,
        &[
            (RecordStatusFlag::Deleted, 1),
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::New, 1),
        ],
        "text_empty_strings pending view",
    );
    let deleted = pending
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(2)))
        .expect("text_empty_strings deleted row missing");
    assert_empty_text_row(deleted, 2);
}

#[test]
fn unicode_supplementary_case_preserves_surrogate_pair_text() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("unicode_supplementary.xml");
    let adtg_path = dir.join("unicode_supplementary.adtg");
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    let default_view = materialize_default_view(&expected);
    assert_status_counts(
        &default_view.rows,
        &[
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::Unmodified, 1),
            (RecordStatusFlag::New, 1),
        ],
        "unicode_supplementary default view",
    );

    let updated = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(1)))
        .expect("unicode_supplementary updated row missing");
    assert_supplementary_unicode_row(updated, 1, 1);

    let null_row = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(3)))
        .expect("unicode_supplementary null row missing");
    assert_eq!(null_row.values.get(1), Some(&Value::Null));
    assert_eq!(null_row.values.get(2), Some(&Value::Null));
    assert_eq!(null_row.values.get(3), Some(&Value::Null));

    let inserted = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(4)))
        .expect("unicode_supplementary inserted row missing");
    assert_supplementary_unicode_row(inserted, 4, 2);

    let pending = materialize_pending_view(&expected);
    assert_status_counts(
        &pending.rows,
        &[
            (RecordStatusFlag::Deleted, 1),
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::New, 1),
        ],
        "unicode_supplementary pending view",
    );
    let deleted = pending
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(2)))
        .expect("unicode_supplementary deleted row missing");
    assert_supplementary_unicode_row(deleted, 2, 0);
}

#[test]
fn empty_rowset_case_preserves_schema_without_rows() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("empty_rowset.xml");
    let adtg_path = dir.join("empty_rowset.adtg");
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    let default_view = materialize_default_view(&expected);
    let pending_view = materialize_pending_view(&expected);
    assert_eq!(
        field_names(&default_view.fields),
        vec!["ID", "EMPTY_TEXT", "EMPTY_BIN", "EMPTY_TS"],
        "empty_rowset schema"
    );
    assert!(default_view.rows.is_empty(), "empty_rowset default rows");
    assert!(pending_view.rows.is_empty(), "empty_rowset pending rows");
}

#[test]
fn wide_0048_case_exercises_field_masks_beyond_32_columns() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("wide_0048.xml");
    let adtg_path = dir.join("wide_0048.adtg");
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    assert_eq!(expected.fields.len(), 48, "wide_0048 XML fields");
    assert_eq!(native.fields.len(), 48, "wide_0048 ADTG fields");
    assert_status_counts(
        &materialize_default_view(&expected).rows,
        &[
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::Unmodified, 3),
            (RecordStatusFlag::New, 1),
        ],
        "wide_0048 default view",
    );
    assert_status_counts(
        &materialize_pending_view(&expected).rows,
        &[
            (RecordStatusFlag::Deleted, 1),
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::New, 1),
        ],
        "wide_0048 pending view",
    );
}

#[test]
fn wide_0065_case_exercises_field_masks_beyond_64_columns() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("wide_0065.xml");
    let adtg_path = dir.join("wide_0065.adtg");
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    assert_eq!(expected.fields.len(), 65, "wide_0065 XML fields");
    assert_eq!(native.fields.len(), 65, "wide_0065 ADTG fields");
    assert_status_counts(
        &materialize_default_view(&expected).rows,
        &[
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::Unmodified, 4),
            (RecordStatusFlag::New, 1),
        ],
        "wide_0065 default view",
    );
    assert_status_counts(
        &materialize_pending_view(&expected).rows,
        &[
            (RecordStatusFlag::Deleted, 1),
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::New, 1),
        ],
        "wide_0065 pending view",
    );
}

#[test]
fn wide_0129_case_exercises_field_masks_beyond_128_columns() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("wide_0129.xml");
    let adtg_path = dir.join("wide_0129.adtg");
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    assert_eq!(expected.fields.len(), 129, "wide_0129 XML fields");
    assert_eq!(native.fields.len(), 129, "wide_0129 ADTG fields");
    assert_status_counts(
        &materialize_default_view(&expected).rows,
        &[
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::Unmodified, 5),
            (RecordStatusFlag::New, 1),
        ],
        "wide_0129 default view",
    );
    assert_status_counts(
        &materialize_pending_view(&expected).rows,
        &[
            (RecordStatusFlag::Deleted, 1),
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::New, 1),
        ],
        "wide_0129 pending view",
    );
}

#[test]
fn all_supported_types_case_mixes_every_supported_flat_type_in_one_updategram() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("all_supported_types.xml");
    let adtg_path = dir.join("all_supported_types.adtg");
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    let default_view = materialize_default_view(&expected);
    assert_eq!(default_view.fields.len(), 31, "all_supported_types fields");
    assert_eq!(
        field_type_codes(&default_view.fields),
        vec![
            Some(3),
            Some(16),
            Some(17),
            Some(2),
            Some(18),
            Some(3),
            Some(19),
            Some(20),
            Some(21),
            Some(4),
            Some(5),
            Some(6),
            Some(11),
            Some(7),
            Some(133),
            Some(134),
            Some(135),
            Some(64),
            Some(72),
            Some(203),
            Some(129),
            Some(130),
            Some(200),
            Some(202),
            Some(201),
            Some(203),
            Some(128),
            Some(204),
            Some(205),
            Some(131),
            Some(14),
        ],
        "all_supported_types ADO type codes"
    );
    assert_status_counts(
        &default_view.rows,
        &[
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::Unmodified, 2),
            (RecordStatusFlag::New, 1),
        ],
        "all_supported_types default view",
    );
    assert_status_counts(
        &materialize_pending_view(&expected).rows,
        &[
            (RecordStatusFlag::Deleted, 1),
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::New, 1),
        ],
        "all_supported_types pending view",
    );
}

#[test]
fn binary_zero_length_case_reopens_empty_binary_as_null() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("binary_zero_length.xml");
    let adtg_path = dir.join("binary_zero_length.adtg");
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    let default_view = materialize_default_view(&expected);
    assert_eq!(default_view.rows.len(), 3, "binary_zero_length row count");
    assert_eq!(
        default_view.rows[0].values.get(1),
        Some(&Value::Null),
        "row 1 variable binary should reopen as null"
    );
    assert_eq!(
        default_view.rows[0].values.get(2),
        Some(&Value::Null),
        "row 1 long binary should reopen as null"
    );
    assert_binary_hex(&default_view.rows[1], 1, "000102");
    assert_binary_hex(&default_view.rows[1], 2, "DEADBEEF");
    assert_eq!(
        default_view.rows[2].values.get(1),
        Some(&Value::Null),
        "row 3 variable binary should be null"
    );
    assert_eq!(
        default_view.rows[2].values.get(2),
        Some(&Value::Null),
        "row 3 long binary should reopen as null"
    );
}

#[test]
fn large_varlen_fields_case_keeps_non_long_32bit_length_values() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("large_varlen_fields.xml");
    let adtg_path = dir.join("large_varlen_fields.adtg");
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    let field_shapes = expected
        .fields
        .iter()
        .map(|field| {
            (
                field.name.as_str(),
                field.ado_type.map(|ty| ty.code),
                field.max_length,
                field.long,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        field_shapes,
        vec![
            ("ID", Some(3), Some(4), false),
            ("VC300", Some(200), Some(300), false),
            ("VWC300", Some(202), Some(300), false),
            ("VB300", Some(204), Some(300), false),
        ],
        "large_varlen_fields metadata"
    );

    let default_view = materialize_default_view(&expected);
    assert_eq!(default_view.rows.len(), 2, "large_varlen_fields row count");
    let row = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(1)))
        .expect("large_varlen_fields value row missing");
    assert_string_chars(row, 1, 'A', 260);
    assert_string_chars(row, 2, '\u{d55c}', 260);
    assert_binary_hex(row, 3, &large_varbinary_expected_hex());

    let null_row = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(2)))
        .expect("large_varlen_fields null row missing");
    assert_eq!(null_row.values.get(1), Some(&Value::Null));
    assert_eq!(null_row.values.get(2), Some(&Value::Null));
    assert_eq!(null_row.values.get(3), Some(&Value::Null));
}

#[test]
fn large_fixed_fields_case_keeps_fixed_32bit_length_values_and_statuses() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("large_fixed_fields.xml");
    let adtg_path = dir.join("large_fixed_fields.adtg");
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    let field_shapes = expected
        .fields
        .iter()
        .map(|field| {
            (
                field.name.as_str(),
                field.ado_type.map(|ty| ty.code),
                field.max_length,
                field.fixed_length,
                field.long,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        field_shapes,
        vec![
            ("ID", Some(3), Some(4), true, false),
            ("FC300", Some(129), Some(300), true, false),
            ("FWC300", Some(130), Some(300), true, false),
            ("FB300", Some(128), Some(300), true, false),
        ],
        "large_fixed_fields metadata"
    );

    let default_view = materialize_default_view(&expected);
    assert_status_counts(
        &default_view.rows,
        &[
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::Unmodified, 1),
            (RecordStatusFlag::New, 1),
        ],
        "large_fixed_fields default view",
    );

    let updated = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(1)))
        .expect("large_fixed_fields updated row missing");
    assert_string_chars(updated, 1, 'U', 300);
    assert_string_chars(updated, 2, '\u{b098}', 300);
    let updated_hex = byte_cycle_hex(0x31, 300);
    assert_binary_hex(updated, 3, &updated_hex);

    let null_row = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(3)))
        .expect("large_fixed_fields null row missing");
    assert_eq!(null_row.values.get(1), Some(&Value::Null));
    assert_eq!(null_row.values.get(2), Some(&Value::Null));
    assert_eq!(null_row.values.get(3), Some(&Value::Null));

    let inserted = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(4)))
        .expect("large_fixed_fields inserted row missing");
    assert_string_chars(inserted, 1, 'I', 300);
    assert_string_chars(inserted, 2, '\u{20ac}', 300);
    let inserted_hex = byte_cycle_hex(0x51, 300);
    assert_binary_hex(inserted, 3, &inserted_hex);

    let pending = materialize_pending_view(&expected);
    assert_status_counts(
        &pending.rows,
        &[
            (RecordStatusFlag::Deleted, 1),
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::New, 1),
        ],
        "large_fixed_fields pending view",
    );
    let deleted = pending
        .rows
        .iter()
        .find(|row| row.status == RecordStatusFlag::Deleted)
        .expect("large_fixed_fields deleted row missing");
    assert_eq!(deleted.values.first(), Some(&Value::Integer(2)));
    assert_string_chars(deleted, 1, 'D', 300);
    assert_string_chars(deleted, 2, '\u{ac12}', 300);
    let deleted_hex = byte_cycle_hex(0x41, 300);
    assert_binary_hex(deleted, 3, &deleted_hex);
}

#[test]
fn long_flag_fields_case_reopens_nonlong_types_as_long_types() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("long_flag_fields.xml");
    let adtg_path = dir.join("long_flag_fields.adtg");
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    for recordset in [&expected, &native] {
        let field_shapes = recordset
            .fields
            .iter()
            .map(|field| {
                (
                    field.name.as_str(),
                    field.ado_type.map(|ty| ty.code),
                    field.max_length,
                    field.long,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            field_shapes,
            vec![
                ("ID", Some(3), Some(4), false),
                ("LONG_FLAG_VARWCHAR", Some(203), Some(120), true),
                ("LONG_FLAG_VARBINARY", Some(205), Some(16), true),
            ],
            "long_flag_fields metadata"
        );
    }

    let expected_flags = vec![
        ("ID".to_string(), 0x10 | 0x04),
        ("LONG_FLAG_VARWCHAR".to_string(), 0x80 | 0x20 | 0x04),
        ("LONG_FLAG_VARBINARY".to_string(), 0x80 | 0x20 | 0x04),
    ];
    for recordset in [&expected, &native] {
        assert_eq!(
            field_flag_pairs(&materialize_default_view(recordset).fields),
            expected_flags,
            "long_flag_fields flags"
        );
    }

    let default_view = materialize_default_view(&expected);
    assert_status_counts(
        &default_view.rows,
        &[
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::Unmodified, 1),
            (RecordStatusFlag::New, 1),
        ],
        "long_flag_fields default view",
    );

    let updated = default_view
        .rows
        .iter()
        .find(|row| row.status == RecordStatusFlag::Modified)
        .expect("long_flag_fields modified row missing");
    assert_long_flag_row(updated, 1, 1);

    let null_row = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(3)))
        .expect("long_flag_fields null row missing");
    assert_eq!(null_row.values.get(1), Some(&Value::Null));
    assert_eq!(null_row.values.get(2), Some(&Value::Null));

    let inserted = default_view
        .rows
        .iter()
        .find(|row| row.status == RecordStatusFlag::New)
        .expect("long_flag_fields inserted row missing");
    assert_long_flag_row(inserted, 4, 2);

    let pending = materialize_pending_view(&expected);
    assert_status_counts(
        &pending.rows,
        &[
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::New, 1),
            (RecordStatusFlag::Deleted, 1),
        ],
        "long_flag_fields pending view",
    );
    let deleted = pending
        .rows
        .iter()
        .find(|row| row.status == RecordStatusFlag::Deleted)
        .expect("long_flag_fields deleted row missing");
    assert_long_flag_row(deleted, 2, 0);
}

#[test]
fn float_extremes_case_preserves_finite_extremes_and_negative_zero() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("float_extremes.xml");
    let adtg_path = dir.join("float_extremes.adtg");
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    let default_view = materialize_default_view(&expected);
    assert_status_counts(
        &default_view.rows,
        &[
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::Unmodified, 2),
            (RecordStatusFlag::New, 2),
        ],
        "float_extremes default view",
    );

    let updated = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(1)))
        .expect("float_extremes updated row missing");
    assert_float_row(updated, 1, 3.4028231e38, 1.7976931348623157e308);

    let null_row = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(4)))
        .expect("float_extremes null row missing");
    assert_eq!(null_row.values.get(1), Some(&Value::Null));
    assert_eq!(null_row.values.get(2), Some(&Value::Null));

    let positive_min = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(3)))
        .expect("float_extremes positive min row missing");
    assert_float_row(positive_min, 3, 1.401298464e-45, f64::from_bits(1));

    let negative_min = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(5)))
        .expect("float_extremes negative min row missing");
    assert_float_row(negative_min, 5, -1.401298464e-45, -f64::from_bits(1));

    let negative_zero = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(6)))
        .expect("float_extremes negative zero row missing");
    assert_float_row(negative_zero, 6, -0.0, -0.0);

    let pending = materialize_pending_view(&expected);
    assert_status_counts(
        &pending.rows,
        &[
            (RecordStatusFlag::Deleted, 1),
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::New, 2),
        ],
        "float_extremes pending view",
    );
    let deleted = pending
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(2)))
        .expect("float_extremes deleted row missing");
    assert_float_row(deleted, 2, -3.4028231e38, -1.7976931348623157e308);
}

#[test]
fn required_fields_case_keeps_required_metadata_and_sparse_updates() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("required_fields.xml");
    let adtg_path = dir.join("required_fields.adtg");
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    for recordset in [&expected, &native] {
        assert!(
            recordset.fields.iter().all(|field| !field.nullable),
            "required_fields should persist non-nullable metadata: {:?}",
            recordset.fields
        );
    }

    let default_view = materialize_default_view(&expected);
    assert_status_counts(
        &default_view.rows,
        &[
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::Unmodified, 1),
            (RecordStatusFlag::New, 1),
        ],
        "required_fields default view",
    );

    let updated = default_view
        .rows
        .iter()
        .find(|row| row.status == RecordStatusFlag::Modified)
        .expect("required_fields modified row missing");
    assert_eq!(updated.values.first(), Some(&Value::Integer(1)));
    assert_eq!(
        updated.values.get(1),
        Some(&Value::String("alpha-updated".to_string()))
    );
    assert_eq!(updated.values.get(2), Some(&Value::Integer(10)));
    assert_binary_hex(updated, 3, "AABBCC");
    assert_eq!(
        updated.values.get(4),
        Some(&Value::DateTime("2026-01-02T03:04:05".to_string()))
    );

    assert_status_counts(
        &materialize_pending_view(&expected).rows,
        &[
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::New, 1),
            (RecordStatusFlag::Deleted, 1),
        ],
        "required_fields pending view",
    );
}

#[test]
fn utf16_xml_stream_case_parses_mdac_unicode_stream_output() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("utf16_xml_stream.xml");
    let adtg_path = dir.join("utf16_xml_stream.adtg");
    let xml_bytes = fs::read(&xml_path).unwrap();
    assert!(
        xml_bytes.starts_with(&[0xff, 0xfe, b'<', 0x00]),
        "utf16_xml_stream XML should be MDAC stream-saved UTF-16LE"
    );

    let expected = parse_ado_xml_bytes(&xml_bytes)
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    assert_eq!(
        expected
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        ["ID", "TXT", "LONG_TXT"]
    );

    let default_view = materialize_default_view(&expected);
    assert_status_counts(
        &default_view.rows,
        &[(RecordStatusFlag::Modified, 1), (RecordStatusFlag::New, 1)],
        "utf16_xml_stream default view",
    );

    let updated = default_view
        .rows
        .iter()
        .find(|row| row.status == RecordStatusFlag::Modified)
        .expect("utf16_xml_stream modified row missing");
    assert_eq!(updated.values.first(), Some(&Value::Integer(1)));
    assert_eq!(
        updated.values.get(1),
        Some(&Value::String("€ updated".to_string()))
    );
    assert!(
        matches!(updated.values.get(2), Some(Value::String(value)) if value.starts_with("updated|한글|")),
        "utf16_xml_stream long updated text: {:?}",
        updated.values.get(2)
    );

    let pending = materialize_pending_view(&expected);
    assert_status_counts(
        &pending.rows,
        &[
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::New, 1),
            (RecordStatusFlag::Deleted, 1),
        ],
        "utf16_xml_stream pending view",
    );
    let deleted = pending
        .rows
        .iter()
        .find(|row| row.status == RecordStatusFlag::Deleted)
        .expect("utf16_xml_stream deleted row missing");
    assert_eq!(deleted.values.first(), Some(&Value::Integer(2)));
}

#[test]
fn utf16be_xml_stream_case_parses_big_endian_xml_accepted_by_mdac() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let le_xml_path = dir.join("utf16_xml_stream.xml");
    let be_xml_path = dir.join("utf16be_xml_stream.xml");
    let adtg_path = dir.join("utf16_xml_stream.adtg");
    let be_xml_bytes = fs::read(&be_xml_path).unwrap();
    assert!(
        be_xml_bytes.starts_with(&[0xfe, 0xff, 0x00, b'<']),
        "utf16be_xml_stream XML should be UTF-16BE with BOM"
    );

    let le_expected = parse_ado_xml_bytes(&fs::read(&le_xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", le_xml_path.display()));
    let be_expected = parse_ado_xml_bytes(&be_xml_bytes)
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", be_xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(
        &le_expected,
        &be_expected,
        &be_xml_path.display().to_string(),
    );
    assert_materialized_views_match(&be_expected, &native, &adtg_path.display().to_string());
}

#[test]
fn field_attributes_case_preserves_mdac_field_flags() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("field_attributes.xml");
    let adtg_path = dir.join("field_attributes.adtg");
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    let default_view = materialize_default_view(&expected);
    assert_eq!(
        field_flag_pairs(&default_view.fields),
        field_attributes_xml_expected_flags(),
        "field_attributes XML flags"
    );
    assert_eq!(default_view.rows.len(), 1, "field_attributes row count");
    assert_eq!(
        default_view.rows[0].values,
        vec![
            Value::Integer(1),
            Value::String("defer".to_string()),
            Value::String("maybe".to_string()),
            Value::String("unknown".to_string()),
            Value::DateTime("2026-06-12T01:02:03".to_string()),
            Value::String("cache".to_string()),
        ],
        "field_attributes values"
    );

    assert_eq!(
        field_flag_pairs(&materialize_default_view(&native).fields),
        field_attributes_adtg_expected_flags(),
        "field_attributes ADTG flags"
    );
}

#[test]
fn rowid_negative_scale_case_pins_format_specific_field_flags() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("rowid_negative_scale.xml");
    let adtg_path = dir.join("rowid_negative_scale.adtg");
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    let default_view = materialize_default_view(&expected);
    assert_eq!(
        field_flag_pairs(&default_view.fields),
        rowid_negative_scale_xml_expected_flags(),
        "rowid_negative_scale XML flags"
    );
    assert_eq!(
        field_flag_pairs(&materialize_default_view(&native).fields),
        rowid_negative_scale_adtg_expected_flags(),
        "rowid_negative_scale ADTG flags"
    );
    assert_eq!(default_view.rows.len(), 1, "rowid_negative_scale row count");
    assert_eq!(
        default_view.rows[0].values,
        vec![Value::Integer(1), Value::Decimal("1234.56".to_string())],
        "rowid_negative_scale values"
    );
}

#[test]
fn fractional_timestamp_case_preserves_fractional_seconds() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("fractional_timestamp.xml");
    let adtg_path = dir.join("fractional_timestamp.adtg");
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    let native_default_view = materialize_default_view(&native);
    assert_eq!(
        native_default_view.fields[1].scale,
        Some(0),
        "fractional_timestamp ADTG DBTIMESTAMP scale"
    );
    let default_view = materialize_default_view(&expected);
    assert_status_counts(
        &default_view.rows,
        &[
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::Unmodified, 1),
            (RecordStatusFlag::New, 1),
        ],
        "fractional_timestamp default view",
    );

    let updated = default_view
        .rows
        .iter()
        .find(|row| row.status == RecordStatusFlag::Modified)
        .expect("fractional_timestamp modified row missing");
    assert_eq!(updated.values.first(), Some(&Value::Integer(1)));
    assert_datetime(updated, 1, "2026-01-02T03:04:05.987");

    let inserted = default_view
        .rows
        .iter()
        .find(|row| row.status == RecordStatusFlag::New)
        .expect("fractional_timestamp inserted row missing");
    assert_eq!(inserted.values.first(), Some(&Value::Integer(4)));
    assert_datetime(inserted, 1, "2026-04-05T06:07:08.25");

    let deleted = materialize_pending_view(&expected)
        .rows
        .into_iter()
        .find(|row| row.status == RecordStatusFlag::Deleted)
        .expect("fractional_timestamp deleted row missing");
    assert_eq!(deleted.values.first(), Some(&Value::Integer(2)));
    assert_datetime(&deleted, 1, "2026-02-03T04:05:06.5");
}

#[test]
fn filetime_fraction_case_matches_mdac_whole_second_reopen_behavior() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("filetime_fraction.xml");
    let adtg_path = dir.join("filetime_fraction.adtg");
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    let default_view = materialize_default_view(&expected);
    assert_status_counts(
        &default_view.rows,
        &[
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::Unmodified, 1),
            (RecordStatusFlag::New, 1),
        ],
        "filetime_fraction default view",
    );

    let updated = default_view
        .rows
        .iter()
        .find(|row| row.status == RecordStatusFlag::Modified)
        .expect("filetime_fraction modified row missing");
    assert_eq!(updated.values.first(), Some(&Value::Integer(1)));
    assert_datetime(updated, 1, "2026-01-02T03:04:05");

    let null_row = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(3)))
        .expect("filetime_fraction null row missing");
    assert_eq!(null_row.values.get(1), Some(&Value::Null));

    let inserted = default_view
        .rows
        .iter()
        .find(|row| row.status == RecordStatusFlag::New)
        .expect("filetime_fraction inserted row missing");
    assert_eq!(inserted.values.first(), Some(&Value::Integer(4)));
    assert_datetime(inserted, 1, "2026-04-05T06:07:08");

    let deleted = materialize_pending_view(&expected)
        .rows
        .into_iter()
        .find(|row| row.status == RecordStatusFlag::Deleted)
        .expect("filetime_fraction deleted row missing");
    assert_eq!(deleted.values.first(), Some(&Value::Integer(2)));
    assert_datetime(&deleted, 1, "2026-02-03T04:05:06");
}

#[test]
fn pre_epoch_date_case_matches_mdac_negative_ole_date_normalization() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("pre_epoch_date.xml");
    let adtg_path = dir.join("pre_epoch_date.adtg");
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    let default_view = materialize_default_view(&expected);
    assert_status_counts(
        &default_view.rows,
        &[
            (RecordStatusFlag::Unmodified, 2),
            (RecordStatusFlag::New, 1),
        ],
        "pre_epoch_date default view",
    );

    let first = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(1)))
        .expect("pre_epoch_date row 1 missing");
    assert_datetime(first, 1, "1899-12-30T11:25:04");

    let null_row = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(3)))
        .expect("pre_epoch_date null row missing");
    assert_eq!(null_row.values.get(1), Some(&Value::Null));

    let inserted = default_view
        .rows
        .iter()
        .find(|row| row.status == RecordStatusFlag::New)
        .expect("pre_epoch_date inserted row missing");
    assert_eq!(inserted.values.first(), Some(&Value::Integer(4)));
    assert_datetime(inserted, 1, "1899-12-29T12:00:00");

    let pending = materialize_pending_view(&expected);
    assert_status_counts(
        &pending.rows,
        &[(RecordStatusFlag::Deleted, 1), (RecordStatusFlag::New, 1)],
        "pre_epoch_date pending view",
    );
    let deleted = pending
        .rows
        .iter()
        .find(|row| row.status == RecordStatusFlag::Deleted)
        .expect("pre_epoch_date deleted row missing");
    assert_eq!(deleted.values.first(), Some(&Value::Integer(2)));
    assert_datetime(deleted, 1, "1899-12-30T23:59:59");
}

#[test]
fn temporal_extremes_case_preserves_mdac_min_max_temporal_values() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("temporal_extremes.xml");
    let adtg_path = dir.join("temporal_extremes.adtg");
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    let default_view = materialize_default_view(&expected);
    assert_status_counts(
        &default_view.rows,
        &[(RecordStatusFlag::Unmodified, 3)],
        "temporal_extremes default view",
    );

    let min_row = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(1)))
        .expect("temporal_extremes min row missing");
    assert_eq!(
        min_row.values,
        vec![
            Value::Integer(1),
            Value::DateTime("0100-01-01T00:00:00".to_string()),
            Value::Date("0100-01-01".to_string()),
            Value::Time("00:00:00".to_string()),
            Value::DateTime("0100-01-01T00:00:00".to_string()),
            Value::DateTime("1601-01-01T00:00:00".to_string()),
        ],
        "temporal_extremes minimum row"
    );

    let max_row = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(2)))
        .expect("temporal_extremes max row missing");
    assert_eq!(
        max_row.values,
        vec![
            Value::Integer(2),
            Value::DateTime("9999-12-31T23:59:59".to_string()),
            Value::Date("9999-12-31".to_string()),
            Value::Time("23:59:59".to_string()),
            Value::DateTime("9999-12-31T23:59:59".to_string()),
            Value::DateTime("9999-12-31T23:59:59".to_string()),
        ],
        "temporal_extremes maximum row"
    );

    let null_row = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(3)))
        .expect("temporal_extremes null row missing");
    for index in 1..=5 {
        assert_eq!(null_row.values.get(index), Some(&Value::Null));
    }
}

#[test]
fn binary_c1_case_matches_mdac_reopen_normalization() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("binary_c1.xml");
    let adtg_path = dir.join("binary_c1.adtg");
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    let default_view = materialize_default_view(&expected);
    assert_eq!(default_view.rows.len(), 3, "binary_c1 row count");
    assert_binary_hex(
        &default_view.rows[0],
        1,
        "AC811A921E262021C6306039528D7D8F9018191C1D221314DC22613A539D7E78",
    );
    assert_binary_hex(
        &default_view.rows[0],
        2,
        "7C7D7E7FAC811A921E262021C6306039528D7D8F9018191C1D221314DC22613A539D7E78A0A1A2A3",
    );
    assert_binary_hex(&default_view.rows[2], 2, "AC818D8F909D7E78");
}

#[test]
fn binary_full_range_case_preserves_every_byte_with_mdac_normalization() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("binary_full_range.xml");
    let adtg_path = dir.join("binary_full_range.adtg");
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    let default_view = materialize_default_view(&expected);
    assert_status_counts(
        &default_view.rows,
        &[
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::Unmodified, 1),
            (RecordStatusFlag::New, 1),
        ],
        "binary_full_range default view",
    );

    let updated = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(1)))
        .expect("binary_full_range updated row missing");
    let updated_hex = byte_cycle_hex(0x11, 256);
    assert_binary_hex(updated, 1, &updated_hex);
    assert_binary_hex(updated, 2, &updated_hex);
    assert_binary_hex(updated, 3, &updated_hex);

    let null_row = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(3)))
        .expect("binary_full_range null row missing");
    assert_eq!(null_row.values.get(1), Some(&Value::Null));
    assert_eq!(null_row.values.get(2), Some(&Value::Null));
    assert_eq!(null_row.values.get(3), Some(&Value::Null));

    let inserted = default_view
        .rows
        .iter()
        .find(|row| row.values.first() == Some(&Value::Integer(4)))
        .expect("binary_full_range inserted row missing");
    let inserted_hex = byte_cycle_hex(0x40, 256);
    assert_binary_hex(inserted, 1, &inserted_hex);
    assert_binary_hex(inserted, 2, &inserted_hex);
    assert_binary_hex(inserted, 3, &inserted_hex);

    let pending = materialize_pending_view(&expected);
    assert_status_counts(
        &pending.rows,
        &[
            (RecordStatusFlag::Deleted, 1),
            (RecordStatusFlag::Modified, 1),
            (RecordStatusFlag::New, 1),
        ],
        "binary_full_range pending view",
    );
    let deleted = pending
        .rows
        .iter()
        .find(|row| row.status == RecordStatusFlag::Deleted)
        .expect("binary_full_range deleted row missing");
    assert_eq!(deleted.values.first(), Some(&Value::Integer(2)));
    let deleted_hex = byte_range_descending_hex(0xff, 0x00);
    assert_binary_hex(deleted, 1, &deleted_hex);
    assert_binary_hex(deleted, 2, &deleted_hex);
    assert_binary_hex(deleted, 3, &deleted_hex);
}

#[test]
fn multi_change_case_keeps_multiple_pending_statuses() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("multi_changes.xml");
    let adtg_path = dir.join("multi_changes.adtg");
    let expected = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap()).unwrap_or_else(|err| {
        panic!(
            "failed to parse native ADTG {}: {err:#}",
            adtg_path.display()
        )
    });

    assert_materialized_views_match(&expected, &native, &adtg_path.display().to_string());
    assert_status_counts(
        &materialize_default_view(&expected).rows,
        &[
            (RecordStatusFlag::Modified, 2),
            (RecordStatusFlag::Unmodified, 2),
            (RecordStatusFlag::New, 2),
        ],
        "multi_changes default view",
    );
    assert_status_counts(
        &materialize_pending_view(&expected).rows,
        &[
            (RecordStatusFlag::Deleted, 2),
            (RecordStatusFlag::Modified, 2),
            (RecordStatusFlag::New, 2),
        ],
        "multi_changes pending view",
    );
}

#[test]
fn com_adtg_roundtrips_keep_xml_materialized_views_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fuzz");
    if !dir.exists() {
        return;
    }

    let mut checked = 0usize;
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if extension(&path).as_deref() != Some("xml")
            || is_roundtrip(&path)
            || is_documented_xml_reader_fixture(&path)
        {
            continue;
        }

        let roundtrip = roundtrip_path(&path);
        if !roundtrip.exists() {
            continue;
        }

        let original = parse_ado_xml_bytes(&fs::read(&path).unwrap())
            .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", path.display()));
        let reparsed = parse_ado_xml_bytes(&fs::read(&roundtrip).unwrap())
            .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", roundtrip.display()));

        assert_materialized_views_match(&original, &reparsed, &path.display().to_string());
        checked += 1;
    }

    assert!(checked >= 100, "expected at least 100 roundtrip pairs");
}

fn assert_materialized_views_match(
    original: &tablegram::model::Recordset,
    reparsed: &tablegram::model::Recordset,
    label: &str,
) {
    assert_eq!(original.fields.len(), reparsed.fields.len(), "{label}");

    let original_default = materialize_default_view(original);
    let reparsed_default = materialize_default_view(reparsed);
    assert_eq!(
        field_names(&original_default.fields),
        field_names(&reparsed_default.fields),
        "{label} default fields"
    );
    assert_ordered_rows_eq(original_default.rows, reparsed_default.rows, label);

    let original_pending = materialize_pending_view(original);
    let reparsed_pending = materialize_pending_view(reparsed);
    assert_eq!(
        field_names(&original_pending.fields),
        field_names(&reparsed_pending.fields),
        "{label} pending fields"
    );
    assert_rows_unordered_eq(original_pending.rows, reparsed_pending.rows, label);
}

fn field_names(fields: &[tablegram::compat::MaterializedField]) -> Vec<&str> {
    fields.iter().map(|field| field.name.as_str()).collect()
}

fn field_type_codes(fields: &[tablegram::compat::MaterializedField]) -> Vec<Option<u16>> {
    fields.iter().map(|field| field.ado_type_code).collect()
}

fn field_flag_pairs(fields: &[tablegram::compat::MaterializedField]) -> Vec<(String, u32)> {
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

fn assert_ordered_rows_eq(left: Vec<MaterializedRow>, right: Vec<MaterializedRow>, label: &str) {
    assert_eq!(left.len(), right.len(), "{label}: default row count");
    for (index, (left, right)) in left.iter().zip(right.iter()).enumerate() {
        assert!(
            rows_match(left, right),
            "{label}: default row {index}: left={left:?} right={right:?}"
        );
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

fn rows_match(left: &MaterializedRow, right: &MaterializedRow) -> bool {
    left.status == right.status
        && left.values.len() == right.values.len()
        && left
            .values
            .iter()
            .zip(right.values.iter())
            .all(|(left, right)| values_match(left, right))
}

fn assert_status_counts(
    rows: &[MaterializedRow],
    expected: &[(RecordStatusFlag, usize)],
    label: &str,
) {
    assert_eq!(
        rows.len(),
        expected.iter().map(|(_, count)| count).sum::<usize>(),
        "{label}: row count"
    );
    for (status, count) in expected {
        assert_eq!(
            rows.iter().filter(|row| row.status == *status).count(),
            *count,
            "{label}: {status:?}"
        );
    }
}

fn assert_binary_hex(row: &MaterializedRow, field_index: usize, expected: &str) {
    assert_eq!(
        row.values.get(field_index),
        Some(&Value::BinaryHex(expected.to_string())),
        "field {field_index}"
    );
}

fn byte_cycle_hex(first_value: u8, count: usize) -> String {
    normalized_binary_hex((0..count).map(|index| first_value.wrapping_add(index as u8)))
}

fn byte_range_descending_hex(first_value: u8, last_value: u8) -> String {
    normalized_binary_hex((last_value..=first_value).rev())
}

fn normalized_binary_hex<I>(bytes: I) -> String
where
    I: IntoIterator<Item = u8>,
{
    bytes
        .into_iter()
        .map(normalized_ado_binary_byte)
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join("")
}

fn normalized_ado_binary_byte(byte: u8) -> u8 {
    match byte {
        0x80 => 0xAC,
        0x82 => 0x1A,
        0x83 => 0x92,
        0x84 => 0x1E,
        0x85 => 0x26,
        0x86 => 0x20,
        0x87 => 0x21,
        0x88 => 0xC6,
        0x89 => 0x30,
        0x8A => 0x60,
        0x8B => 0x39,
        0x8C => 0x52,
        0x8E => 0x7D,
        0x91 => 0x18,
        0x92 => 0x19,
        0x93 => 0x1C,
        0x94 => 0x1D,
        0x95 => 0x22,
        0x96 => 0x13,
        0x97 => 0x14,
        0x98 => 0xDC,
        0x99 => 0x22,
        0x9A => 0x61,
        0x9B => 0x3A,
        0x9C => 0x53,
        0x9E => 0x7E,
        0x9F => 0x78,
        other => other,
    }
}

fn assert_string_chars(row: &MaterializedRow, field_index: usize, expected: char, count: usize) {
    match row.values.get(field_index) {
        Some(Value::String(actual)) => {
            assert_eq!(actual.chars().count(), count, "field {field_index} length");
            assert!(
                actual.chars().all(|ch| ch == expected),
                "field {field_index} contents"
            );
        }
        other => panic!("field {field_index}: expected string, got {other:?}"),
    }
}

fn assert_text_escape_row(row: &MaterializedRow, row_id: i64, phase: i64) {
    assert_eq!(
        row.values.get(1),
        Some(&Value::String(text_escape_ascii(row_id, phase))),
        "text_escapes TXT_VAR"
    );
    assert_eq!(
        row.values.get(2),
        Some(&Value::String(text_escape_wide(row_id, phase))),
        "text_escapes TXT_WIDE"
    );
    assert_eq!(
        row.values.get(3),
        Some(&Value::String(text_escape_long(row_id, phase))),
        "text_escapes TXT_LONG"
    );
}

fn assert_text_controls_row(row: &MaterializedRow, row_id: i64, phase: i64) {
    let ascii = text_control_chars_value("A", row_id, phase);
    let wide = text_control_chars_value("W", row_id, phase);
    assert_padded_string(row, 1, &ascii, "text_controls FIXED_ANSI_CTL");
    assert_padded_string(row, 2, &wide, "text_controls FIXED_WIDE_CTL");
    assert_eq!(
        row.values.get(3),
        Some(&Value::String(ascii.clone())),
        "text_controls VAR_ANSI_CTL"
    );
    assert_eq!(
        row.values.get(4),
        Some(&Value::String(wide.clone())),
        "text_controls VAR_WIDE_CTL"
    );
    assert_eq!(
        row.values.get(5),
        Some(&Value::String(text_control_long_value(&ascii))),
        "text_controls LONG_ANSI_CTL"
    );
    assert_eq!(
        row.values.get(6),
        Some(&Value::String(text_control_long_value(&wide))),
        "text_controls LONG_WIDE_CTL"
    );
}

fn assert_padded_string(row: &MaterializedRow, field_index: usize, expected: &str, label: &str) {
    match row.values.get(field_index) {
        Some(Value::String(actual)) => {
            assert!(actual.starts_with(expected), "{label}: {actual:?}");
            assert!(
                actual[expected.len()..].chars().all(|ch| ch == ' '),
                "{label}: fixed-width suffix should be spaces: {actual:?}"
            );
        }
        other => panic!("{label}: expected string, got {other:?}"),
    }
}

fn assert_korean_ansi_row(row: &MaterializedRow, row_id: i64, phase: i64) {
    assert_eq!(
        row.values.get(1),
        Some(&Value::String(korean_ansi_fixed_text(row_id, phase))),
        "text_korean_ansi FIXED_ANSI_KR"
    );
    assert_eq!(
        row.values.get(2),
        Some(&Value::String(korean_ansi_var_text(row_id, phase))),
        "text_korean_ansi VAR_ANSI_KR"
    );
    assert_eq!(
        row.values.get(3),
        Some(&Value::String(korean_ansi_long_text(row_id, phase))),
        "text_korean_ansi LONG_ANSI_KR"
    );
}

fn assert_special_field_names_row(row: &MaterializedRow, row_id: i64, phase: i64) {
    assert_eq!(
        row.values.get(1),
        Some(&Value::String(special_field_value("amp", row_id, phase))),
        "special_field_names amp"
    );
    assert_eq!(
        row.values.get(2),
        Some(&Value::String(special_field_value("quote", row_id, phase))),
        "special_field_names quote"
    );
    assert_eq!(
        row.values.get(3),
        Some(&Value::String(special_field_value(
            "apostrophe",
            row_id,
            phase
        ))),
        "special_field_names apostrophe"
    );
    assert_eq!(
        row.values.get(4),
        Some(&Value::String(special_field_value("less", row_id, phase))),
        "special_field_names less"
    );
    assert_eq!(
        row.values.get(5),
        Some(&Value::String(special_field_value(
            "greater", row_id, phase
        ))),
        "special_field_names greater"
    );
}

fn assert_whitespace_field_names_row(row: &MaterializedRow, row_id: i64, phase: i64) {
    assert_eq!(row.values.first(), Some(&Value::Integer(row_id)));
    assert_eq!(
        row.values.get(1),
        Some(&Value::String(whitespace_field_value(
            "space", row_id, phase
        ))),
        "whitespace_field_names space"
    );
    assert_eq!(
        row.values.get(2),
        Some(&Value::String(whitespace_field_value(
            "edge", row_id, phase
        ))),
        "whitespace_field_names edge"
    );
    assert_eq!(
        row.values.get(3),
        Some(&Value::String(whitespace_field_value("tab", row_id, phase))),
        "whitespace_field_names tab"
    );
    assert_eq!(
        row.values.get(4),
        Some(&Value::String(whitespace_field_value("lf", row_id, phase))),
        "whitespace_field_names lf"
    );
    assert_eq!(
        row.values.get(5),
        Some(&Value::String(whitespace_field_value("cr", row_id, phase))),
        "whitespace_field_names cr"
    );
}

fn whitespace_field_value(kind: &str, row_id: i64, phase: i64) -> String {
    format!("{kind}|row={row_id}|phase={phase}")
}

fn assert_long_flag_row(row: &MaterializedRow, row_id: i64, phase: i64) {
    assert_eq!(row.values.first(), Some(&Value::Integer(row_id)));
    assert_eq!(
        row.values.get(1),
        Some(&Value::String(long_flag_text(row_id, phase))),
        "long_flag_fields text"
    );
    assert_binary_hex(
        row,
        2,
        &format!("{:02X}{:02X}DEADBEEF", row_id as u8, phase as u8),
    );
}

fn assert_float_row(row: &MaterializedRow, row_id: i64, single: f64, double: f64) {
    assert_eq!(row.values.first(), Some(&Value::Integer(row_id)));
    assert_float_value(row, 1, single);
    assert_float_value(row, 2, double);
}

fn assert_float_value(row: &MaterializedRow, field_index: usize, expected: f64) {
    match row.values.get(field_index) {
        Some(Value::Float(actual)) if float_values_match(*actual, expected) => {}
        other => panic!("field {field_index}: expected float {expected:?}, got {other:?}"),
    }
}

fn long_flag_text(row_id: i64, phase: i64) -> String {
    format!("longflag|{row_id}|{phase}|\u{d55c}\u{ae00}")
}

fn special_field_value(kind: &str, row_id: i64, phase: i64) -> String {
    format!("{kind}|row={row_id}|phase={phase}|\u{d55c}\u{ae00}")
}

fn text_escape_ascii(row_id: i64, phase: i64) -> String {
    format!("row={row_id}|phase={phase}|\"dq\"|'sq'|<&>|tab\tcr\rlf\nend")
}

fn text_escape_wide(row_id: i64, phase: i64) -> String {
    format!(
        "{}|wide=\u{d55c}\u{ae00}_\u{ac12}_\u{20ac}",
        text_escape_ascii(row_id, phase)
    )
}

fn text_escape_long(row_id: i64, phase: i64) -> String {
    let mut out = String::new();
    for part in 1..=18 {
        out.push_str(&text_escape_wide(row_id, phase));
        out.push_str(&format!("|part={part}\r\n"));
    }
    out
}

fn text_control_chars_value(prefix: &str, row_id: i64, phase: i64) -> String {
    format!("{prefix}{row_id}p{phase}\0\u{1}\u{8}\u{b}\u{c}\u{e}\u{1f}Z")
}

fn text_control_long_value(value: &str) -> String {
    format!("{value}|tail|{value}")
}

fn korean_ansi_fixed_text(row_id: i64, phase: i64) -> String {
    match (row_id, phase) {
        (1, 1) => "\u{ce74}\u{d0c0}\u{d30c}\u{d558}\u{ac12}".to_string(),
        (2, 0) => "\u{bc14}\u{c0ac}\u{c544}\u{c790}\u{cc28}".to_string(),
        (4, 2) => "\u{d55c}\u{ae00}\u{c790}\u{b8cc}\u{b05d}".to_string(),
        _ => "\u{ac00}\u{b098}\u{b2e4}\u{b77c}\u{b9c8}".to_string(),
    }
}

fn korean_ansi_var_text(row_id: i64, phase: i64) -> String {
    format!("\u{d55c}\u{ae00}_{row_id}_p{phase}_\u{ac12}\u{ce74}")
}

fn korean_ansi_long_text(row_id: i64, phase: i64) -> String {
    (1..=24)
        .map(|index| format!("{}|{index};", korean_ansi_var_text(row_id, phase)))
        .collect::<String>()
}

fn assert_text_space_row(row: &MaterializedRow, row_id: i64, phase: i64) {
    assert_eq!(
        row.values.get(1),
        Some(&Value::String(fixed_space_ascii(row_id, phase))),
        "text_spaces FIXED_ASCII"
    );
    assert_eq!(
        row.values.get(2),
        Some(&Value::String(fixed_space_wide(row_id, phase))),
        "text_spaces FIXED_WIDE"
    );
    assert_eq!(
        row.values.get(3),
        Some(&Value::String(variable_space_ascii(row_id, phase))),
        "text_spaces VAR_ASCII"
    );
    assert_eq!(
        row.values.get(4),
        Some(&Value::String(variable_space_wide(row_id, phase))),
        "text_spaces VAR_WIDE"
    );
    assert_eq!(
        row.values.get(5),
        Some(&Value::String(long_space_wide(row_id, phase))),
        "text_spaces LONG_WIDE"
    );
}

fn fixed_space_ascii(row_id: i64, phase: i64) -> String {
    format!(" A{row_id}  P{phase}   Z     ")
}

fn fixed_space_wide(row_id: i64, phase: i64) -> String {
    format!(" \u{d55c}{row_id}  \u{ac12}{phase}   \u{20ac}     ")
}

fn variable_space_ascii(row_id: i64, phase: i64) -> String {
    format!("  row {row_id}   phase {phase}  end  ")
}

fn variable_space_wide(row_id: i64, phase: i64) -> String {
    format!("  \u{d55c}\u{ae00} {row_id}   \u{ac12} {phase}  \u{20ac}  ")
}

fn long_space_wide(row_id: i64, phase: i64) -> String {
    let mut out = String::new();
    for block in 1..=20 {
        out.push_str(&variable_space_wide(row_id, phase));
        out.push_str(&format!(" block  {block}   "));
    }
    out
}

fn assert_empty_text_row(row: &MaterializedRow, row_id: i64) {
    assert_eq!(row.values.first(), Some(&Value::Integer(row_id)));
    assert_eq!(
        row.values.get(1),
        Some(&Value::String("    ".to_string())),
        "text_empty_strings fixed ascii"
    );
    assert_eq!(
        row.values.get(2),
        Some(&Value::String("    ".to_string())),
        "text_empty_strings fixed wide"
    );
    for index in 3..=6 {
        assert_eq!(
            row.values.get(index),
            Some(&Value::String(String::new())),
            "text_empty_strings empty field {index}"
        );
    }
}

fn assert_supplementary_unicode_row(row: &MaterializedRow, row_id: i64, phase: i64) {
    assert_eq!(
        row.values.get(1),
        Some(&Value::String(fixed_supplementary_text(row_id, phase))),
        "unicode_supplementary SUPP_FIXED"
    );
    assert_eq!(
        row.values.get(2),
        Some(&Value::String(supplementary_text(row_id, phase))),
        "unicode_supplementary SUPP_VAR"
    );
    assert_eq!(
        row.values.get(3),
        Some(&Value::String(long_supplementary_text(row_id, phase))),
        "unicode_supplementary SUPP_LONG"
    );
}

fn supplementary_text(row_id: i64, phase: i64) -> String {
    format!("row={row_id}|phase={phase}|\u{1f600}|\u{20000}|\u{d55c}\u{ae00}")
}

fn fixed_supplementary_text(row_id: i64, phase: i64) -> String {
    format!(" \u{1f600} R{row_id} P{phase} \u{20000}    ")
}

fn long_supplementary_text(row_id: i64, phase: i64) -> String {
    let mut out = String::new();
    for part in 1..=24 {
        out.push_str(&supplementary_text(row_id, phase));
        out.push_str(&format!("|part={part} "));
    }
    out
}

fn large_varbinary_expected_hex() -> String {
    let bytes = (0..260)
        .map(|index| normalize_c1_byte((index % 256) as u8))
        .collect::<Vec<_>>();
    hex::encode_upper(bytes)
}

fn normalize_c1_byte(byte: u8) -> u8 {
    match byte {
        0x80 => 0xAC,
        0x82 => 0x1A,
        0x83 => 0x92,
        0x84 => 0x1E,
        0x85 => 0x26,
        0x86 => 0x20,
        0x87 => 0x21,
        0x88 => 0xC6,
        0x89 => 0x30,
        0x8A => 0x60,
        0x8B => 0x39,
        0x8C => 0x52,
        0x8E => 0x7D,
        0x91 => 0x18,
        0x92 => 0x19,
        0x93 => 0x1C,
        0x94 => 0x1D,
        0x95 => 0x22,
        0x96 => 0x13,
        0x97 => 0x14,
        0x98 => 0xDC,
        0x99 => 0x22,
        0x9A => 0x61,
        0x9B => 0x3A,
        0x9C => 0x53,
        0x9E => 0x7E,
        0x9F => 0x78,
        other => other,
    }
}

fn assert_datetime(row: &MaterializedRow, field_index: usize, expected: &str) {
    match row.values.get(field_index) {
        Some(Value::DateTime(actual)) => assert_eq!(
            canonical_datetime_text(actual),
            canonical_datetime_text(expected),
            "field {field_index}"
        ),
        other => panic!("field {field_index}: expected datetime, got {other:?}"),
    }
}

fn assert_xml_name_is_mapped(recordset: &tablegram::model::Recordset, field_name: &str) {
    let field = recordset
        .fields
        .iter()
        .find(|field| field.name == field_name)
        .unwrap_or_else(|| panic!("missing field {field_name}"));
    assert_ne!(
        field.xml_name, field.name,
        "{field_name} should use an XML-safe name with rs:name mapping"
    );
}

fn name_mapping_expected_names() -> Vec<String> {
    vec![
        "ID".to_string(),
        "Field Space Text".to_string(),
        "1LeadingInteger".to_string(),
        "Name-With-Dash".to_string(),
        korean_field_name(),
    ]
}

fn korean_field_name() -> String {
    "\u{d55c}\u{ae00} \u{d544}\u{b4dc}".to_string()
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

fn extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
}

fn is_roundtrip(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.contains(".roundtrip."))
        .unwrap_or(false)
}

fn is_documented_xml_reader_fixture(path: &Path) -> bool {
    path.file_stem()
        .and_then(|name| name.to_str())
        .map(|name| name.starts_with("doc_"))
        .unwrap_or(false)
}

fn roundtrip_path(path: &Path) -> PathBuf {
    let file_stem = path.file_stem().unwrap().to_string_lossy();
    path.with_file_name(format!("{file_stem}.roundtrip.xml"))
}

fn read_csv_rows(path: &Path) -> Vec<Vec<String>> {
    let text = fs::read_to_string(path).unwrap();
    text.lines().skip(1).map(parse_csv_line).collect()
}

fn add_expected_artifact(
    dir: &Path,
    artifact: &str,
    expected: &mut BTreeSet<String>,
    missing: &mut Vec<String>,
) {
    let normalized = artifact.replace('\\', "/");
    let path = Path::new(&normalized);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_else(|| panic!("invalid artifact path {artifact}"));
    add_expected_file_name(dir, file_name, expected, missing);
}

fn add_expected_file_name(
    dir: &Path,
    file_name: &str,
    expected: &mut BTreeSet<String>,
    missing: &mut Vec<String>,
) {
    if !dir.join(file_name).exists() {
        missing.push(file_name.to_string());
    }
    expected.insert(file_name.to_string());
}

fn corpus_artifacts(dir: &Path) -> BTreeSet<String> {
    fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            matches!(
                path.extension().and_then(|value| value.to_str()),
                Some("adtg" | "xml")
            )
        })
        .map(|path| path.file_name().unwrap().to_str().unwrap().to_string())
        .collect()
}

fn assert_text_korean_ansi_failure_recorded(dir: &Path) {
    let rows = read_csv_rows(&dir.join("failures.csv"));
    let row = rows
        .iter()
        .find(|row| row[0] == "text_korean_ansi")
        .expect("text_korean_ansi should be generated or recorded as an MDAC failure");
    assert_eq!(row[1], "ansi_codepage", "text_korean_ansi failure stage");
    assert!(
        !row[2].is_empty(),
        "text_korean_ansi failure should record an MDAC error number"
    );
    assert!(
        !row[3].is_empty(),
        "text_korean_ansi failure should record an MDAC error description"
    );
    assert_absent(&dir.join("text_korean_ansi.xml"));
    assert_absent(&dir.join("text_korean_ansi.adtg"));
    assert_absent(&dir.join("text_korean_ansi.roundtrip.xml"));
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

fn assert_absent(path: &Path) {
    assert!(
        !path.exists(),
        "unsupported type-matrix artifact should not exist: {}",
        path.display()
    );
}
