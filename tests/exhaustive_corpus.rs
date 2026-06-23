use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::Path;

use tablegram::adtg::{inspect_adtg, parse_adtg_bytes, SUPPORTED_NATIVE_ADTG_ADO_TYPE_CODES};
use tablegram::compat::{materialize_default_view, materialize_pending_view, MaterializedRow};
use tablegram::model::Value;
use tablegram::xml::parse_ado_xml_bytes;

const SUPPORTED_FLAT_TYPES: &[(&str, &str)] = &[
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

const REQUIRED_SCENARIOS: &[&str] = &["boundaries", "states", "null_states"];

struct NumericPrecisionScenario {
    type_name: &'static str,
    type_code: u16,
    scenario: &'static str,
    precision: usize,
    scale: i32,
    values: &'static [&'static str],
}

const PRECISION_5_SCALE_0_VALUES: &[&str] = &["-99999", "-1", "0", "1", "99999"];
const PRECISION_9_SCALE_2_VALUES: &[&str] = &["-12345.67", "-1.23", "0", "1.23", "12345.67"];
const PRECISION_1_SCALE_0_VALUES: &[&str] = &["-9", "0", "9"];
const PRECISION_18_SCALE_6_VALUES: &[&str] = &[
    "-999999999999.999999",
    "-1.000001",
    "0",
    "1.000001",
    "999999999999.999999",
];
const PRECISION_28_SCALE_0_VALUES: &[&str] = &[
    "-9999999999999999999999999999",
    "-1",
    "0",
    "1",
    "9999999999999999999999999999",
];
const PRECISION_28_SCALE_10_VALUES: &[&str] = &[
    "-999999999999999999.9999999999",
    "-1.0000000001",
    "0",
    "1.0000000001",
    "999999999999999999.9999999999",
];
const PRECISION_28_SCALE_28_VALUES: &[&str] = &[
    "-0.9999999999999999999999999999",
    "-0.0000000000000000000000000001",
    "0",
    "0.0000000000000000000000000001",
    "0.9999999999999999999999999999",
];

const NUMERIC_PRECISION_SCENARIOS: &[NumericPrecisionScenario] = &[
    NumericPrecisionScenario {
        type_name: "Numeric",
        type_code: 131,
        scenario: "precision_scale_1_0",
        precision: 1,
        scale: 0,
        values: PRECISION_1_SCALE_0_VALUES,
    },
    NumericPrecisionScenario {
        type_name: "Numeric",
        type_code: 131,
        scenario: "precision_scale_5_0",
        precision: 5,
        scale: 0,
        values: PRECISION_5_SCALE_0_VALUES,
    },
    NumericPrecisionScenario {
        type_name: "Numeric",
        type_code: 131,
        scenario: "precision_scale_9_2",
        precision: 9,
        scale: 2,
        values: PRECISION_9_SCALE_2_VALUES,
    },
    NumericPrecisionScenario {
        type_name: "Numeric",
        type_code: 131,
        scenario: "precision_scale_18_6",
        precision: 18,
        scale: 6,
        values: PRECISION_18_SCALE_6_VALUES,
    },
    NumericPrecisionScenario {
        type_name: "Numeric",
        type_code: 131,
        scenario: "precision_scale_28_0",
        precision: 28,
        scale: 0,
        values: PRECISION_28_SCALE_0_VALUES,
    },
    NumericPrecisionScenario {
        type_name: "Numeric",
        type_code: 131,
        scenario: "precision_scale_28_10",
        precision: 28,
        scale: 10,
        values: PRECISION_28_SCALE_10_VALUES,
    },
    NumericPrecisionScenario {
        type_name: "Numeric",
        type_code: 131,
        scenario: "precision_scale_28_28",
        precision: 28,
        scale: 28,
        values: PRECISION_28_SCALE_28_VALUES,
    },
    NumericPrecisionScenario {
        type_name: "Decimal",
        type_code: 14,
        scenario: "precision_scale_1_0",
        precision: 1,
        scale: 0,
        values: PRECISION_1_SCALE_0_VALUES,
    },
    NumericPrecisionScenario {
        type_name: "Decimal",
        type_code: 14,
        scenario: "precision_scale_5_0",
        precision: 5,
        scale: 0,
        values: PRECISION_5_SCALE_0_VALUES,
    },
    NumericPrecisionScenario {
        type_name: "Decimal",
        type_code: 14,
        scenario: "precision_scale_9_2",
        precision: 9,
        scale: 2,
        values: PRECISION_9_SCALE_2_VALUES,
    },
    NumericPrecisionScenario {
        type_name: "Decimal",
        type_code: 14,
        scenario: "precision_scale_18_6",
        precision: 18,
        scale: 6,
        values: PRECISION_18_SCALE_6_VALUES,
    },
    NumericPrecisionScenario {
        type_name: "Decimal",
        type_code: 14,
        scenario: "precision_scale_28_0",
        precision: 28,
        scale: 0,
        values: PRECISION_28_SCALE_0_VALUES,
    },
    NumericPrecisionScenario {
        type_name: "Decimal",
        type_code: 14,
        scenario: "precision_scale_28_10",
        precision: 28,
        scale: 10,
        values: PRECISION_28_SCALE_10_VALUES,
    },
    NumericPrecisionScenario {
        type_name: "Decimal",
        type_code: 14,
        scenario: "precision_scale_28_28",
        precision: 28,
        scale: 28,
        values: PRECISION_28_SCALE_28_VALUES,
    },
];

const EXPECTED_UNSUPPORTED: &[(&str, &str, &str)] = &[
    ("Empty", "0", "fail"),
    ("VarNumeric", "139", "fail"),
    ("Error", "10", "fail"),
    ("Variant", "12", "probe_ok_not_exhaustive"),
    ("IDispatch", "9", "fail"),
    ("IUnknown", "13", "fail"),
    ("Chapter", "136", "fail"),
    ("PropVariant", "138", "fail"),
    ("UserDefined", "132", "fail"),
    ("ArrayInteger", "8195", "fail"),
];

const ADO_DATATYPE_ENUM_NON_ARRAY_TYPES: &[(&str, &str)] = &[
    ("BigInt", "20"),
    ("Binary", "128"),
    ("Boolean", "11"),
    ("BSTR", "8"),
    ("Chapter", "136"),
    ("Char", "129"),
    ("Currency", "6"),
    ("Date", "7"),
    ("DBDate", "133"),
    ("DBTime", "134"),
    ("DBTimeStamp", "135"),
    ("Decimal", "14"),
    ("Double", "5"),
    ("Empty", "0"),
    ("Error", "10"),
    ("FileTime", "64"),
    ("GUID", "72"),
    ("IDispatch", "9"),
    ("Integer", "3"),
    ("IUnknown", "13"),
    ("LongVarBinary", "205"),
    ("LongVarChar", "201"),
    ("LongVarWChar", "203"),
    ("Numeric", "131"),
    ("PropVariant", "138"),
    ("Single", "4"),
    ("SmallInt", "2"),
    ("TinyInt", "16"),
    ("UnsignedBigInt", "21"),
    ("UnsignedInt", "19"),
    ("UnsignedSmallInt", "18"),
    ("UnsignedTinyInt", "17"),
    ("UserDefined", "132"),
    ("VarBinary", "204"),
    ("VarChar", "200"),
    ("Variant", "12"),
    ("VarNumeric", "139"),
    ("VarWChar", "202"),
    ("WChar", "130"),
];

#[test]
fn documented_ado_datatype_enum_surface_is_classified() {
    let mut classified = HashMap::new();
    for (type_name, type_code) in SUPPORTED_FLAT_TYPES {
        assert_eq!(
            classified.insert(*type_name, *type_code),
            None,
            "duplicate classified supported type {type_name}"
        );
    }
    for (type_name, type_code, _result) in EXPECTED_UNSUPPORTED {
        if *type_name == "ArrayInteger" {
            continue;
        }
        assert_eq!(
            classified.insert(*type_name, *type_code),
            None,
            "duplicate classified unsupported type {type_name}"
        );
    }

    let expected = ADO_DATATYPE_ENUM_NON_ARRAY_TYPES
        .iter()
        .copied()
        .collect::<HashMap<_, _>>();
    assert_eq!(
        classified, expected,
        "supported and unsupported flat corpus policy should account for every non-array ADO DataTypeEnum constant"
    );
    assert!(
        EXPECTED_UNSUPPORTED.contains(&("ArrayInteger", "8195", "fail")),
        "adArray is a flag, so the corpus should keep at least one combined array probe"
    );
}

#[test]
fn advertised_native_adtg_types_match_flat_policy_plus_focused_extensions() {
    let mut expected = SUPPORTED_FLAT_TYPES
        .iter()
        .map(|(type_name, declared_code)| {
            expected_persisted_ado_type_code(type_name, declared_code)
        })
        .collect::<BTreeSet<_>>();

    // These are supported by focused native corpora, but not by the flat
    // Recordset.Append exhaustive matrix as directly saveable flat fields.
    expected.extend([12, 136, 139]);

    let actual = SUPPORTED_NATIVE_ADTG_ADO_TYPE_CODES
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual, expected,
        "advertised native ADTG ADO type support should match the exhaustive flat persisted type policy plus variant/chapter/varnumeric corpus extensions"
    );
    assert!(
        !actual.contains(&8),
        "adBSTR is accepted by MDAC Append but persists as adLongVarWChar, not native ADTG type 8"
    );
}

#[test]
fn parses_exhaustive_flat_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/exhaustive");
    if !dir.exists() {
        return;
    }

    let coverage = fs::read_to_string(dir.join("coverage.csv")).unwrap();
    assert!(
        !coverage.contains("\",\"fail\","),
        "exhaustive coverage.csv contains failed supported scenarios"
    );

    let mut xml_count = 0usize;
    let mut adtg_count = 0usize;
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        match path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("xml") => {
                let recordset =
                    parse_ado_xml_bytes(&fs::read(&path).unwrap()).unwrap_or_else(|err| {
                        panic!("failed to parse exhaustive XML {}: {err:#}", path.display())
                    });
                assert!(!recordset.fields.is_empty(), "{}", path.display());
                xml_count += 1;
            }
            Some("adtg") => {
                let bytes = fs::read(&path).unwrap();
                let document = inspect_adtg(&bytes).unwrap_or_else(|err| {
                    panic!(
                        "failed to inspect exhaustive ADTG {}: {err:#}",
                        path.display()
                    )
                });
                assert_eq!(document.length, bytes.len(), "{}", path.display());
                adtg_count += 1;
            }
            _ => {}
        }
    }

    assert_eq!(
        xml_count,
        exhaustive_case_count() * 2,
        "exhaustive XML and roundtrip XML files"
    );
    assert_eq!(adtg_count, exhaustive_case_count(), "exhaustive ADTG files");
}

#[test]
fn exhaustive_coverage_lists_every_supported_flat_type_and_scenario() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/exhaustive");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("coverage.csv"));
    assert_eq!(
        rows.len(),
        exhaustive_case_count(),
        "coverage.csv should contain every supported flat type/scenario pair"
    );

    let mut seen = HashSet::new();
    let mut codes = HashMap::new();
    for row in rows {
        assert_eq!(row[3], "ok", "coverage row should be ok: {row:?}");
        seen.insert((row[0].clone(), row[2].clone()));
        codes.insert(row[0].clone(), row[1].clone());
        assert!(
            Path::new(&row[4]).exists(),
            "coverage XML file is missing: {}",
            row[4]
        );
        assert!(
            Path::new(&row[5]).exists(),
            "coverage ADTG file is missing: {}",
            row[5]
        );
        assert!(
            Path::new(&row[6]).exists(),
            "coverage roundtrip XML file is missing: {}",
            row[6]
        );
    }

    for (type_name, type_code) in SUPPORTED_FLAT_TYPES {
        assert_eq!(
            codes.get(*type_name).map(String::as_str),
            Some(*type_code),
            "{type_name} type code"
        );
        for scenario in REQUIRED_SCENARIOS {
            assert!(
                seen.contains(&(type_name.to_string(), scenario.to_string())),
                "missing exhaustive coverage for {type_name}/{scenario}"
            );
        }
    }

    for scenario in NUMERIC_PRECISION_SCENARIOS {
        assert!(
            seen.contains(&(
                scenario.type_name.to_string(),
                scenario.scenario.to_string()
            )),
            "missing exhaustive coverage for {}/{}",
            scenario.type_name,
            scenario.scenario
        );
    }
}

#[test]
fn exhaustive_supported_flat_types_parse_with_expected_persisted_ado_type() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/exhaustive");
    if !dir.exists() {
        return;
    }

    for (type_name, declared_code) in SUPPORTED_FLAT_TYPES {
        let expected_code = expected_persisted_ado_type_code(type_name, declared_code);
        for scenario in REQUIRED_SCENARIOS {
            let stem = format!("flat_{type_name}_{scenario}");
            let xml_path = dir.join(format!("{stem}.xml"));
            let adtg_path = dir.join(format!("{stem}.adtg"));

            let xml = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
                .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", xml_path.display()));
            let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap())
                .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", adtg_path.display()));

            assert_value_field_ado_type(&xml, expected_code, &format!("{stem} XML"));
            assert_value_field_ado_type(&native, expected_code, &format!("{stem} ADTG"));
        }
    }
}

#[test]
fn numeric_precision_scale_cases_preserve_metadata_and_values() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/exhaustive");
    if !dir.exists() {
        return;
    }

    for scenario in NUMERIC_PRECISION_SCENARIOS {
        let stem = format!("flat_{}_{}", scenario.type_name, scenario.scenario);
        let xml_path = dir.join(format!("{stem}.xml"));
        let adtg_path = dir.join(format!("{stem}.adtg"));

        let xml = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
            .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", xml_path.display()));
        let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap())
            .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", adtg_path.display()));

        assert_materialized_views_match(&xml, &native, &stem);
        assert_precision_scale_metadata(&xml, scenario, &format!("{stem} xml"));
        assert_precision_scale_metadata(&native, scenario, &format!("{stem} adtg"));
        assert_precision_scale_values(&xml, scenario, &format!("{stem} xml"));
        assert_precision_scale_values(&native, scenario, &format!("{stem} adtg"));
    }
}

#[test]
fn tinyint_boundaries_are_signed_i8_values() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/exhaustive");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("flat_TinyInt_boundaries.xml");
    let adtg_path = dir.join("flat_TinyInt_boundaries.adtg");
    let xml = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", adtg_path.display()));

    assert_materialized_views_match(&xml, &native, "flat_TinyInt_boundaries");
    for (label, recordset) in [("xml", &xml), ("adtg", &native)] {
        let materialized = materialize_default_view(recordset);
        let values = materialized
            .rows
            .iter()
            .map(|row| row.values.get(1).cloned())
            .collect::<Vec<_>>();
        assert_eq!(
            values,
            vec![
                Some(Value::Integer(-128)),
                Some(Value::Integer(-1)),
                Some(Value::Integer(0)),
                Some(Value::Integer(1)),
                Some(Value::Integer(127)),
                Some(Value::Null),
            ],
            "{label}: signed adTinyInt boundaries"
        );
    }
}

#[test]
fn currency_boundaries_are_full_scaled_i64_values() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/exhaustive");
    if !dir.exists() {
        return;
    }

    let xml_path = dir.join("flat_Currency_boundaries.xml");
    let adtg_path = dir.join("flat_Currency_boundaries.adtg");
    let xml = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", xml_path.display()));
    let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", adtg_path.display()));

    assert_materialized_views_match(&xml, &native, "flat_Currency_boundaries");
    let expected = [
        "-922337203685477.5808",
        "-1.0001",
        "0",
        "1.0001",
        "922337203685477.5807",
    ];
    for (label, recordset) in [("xml", &xml), ("adtg", &native)] {
        let materialized = materialize_default_view(recordset);
        for (index, expected) in expected.iter().enumerate() {
            match materialized.rows[index].values.get(1) {
                Some(Value::Decimal(actual)) => assert!(
                    numeric_text_matches(actual, expected),
                    "{label}: row {} currency mismatch: actual={actual:?} expected={expected:?}",
                    index + 1
                ),
                other => panic!(
                    "{label}: row {} expected currency, got {other:?}",
                    index + 1
                ),
            }
        }
        assert_eq!(
            materialized.rows.last().and_then(|row| row.values.get(1)),
            Some(&Value::Null),
            "{label}: trailing null row"
        );
    }
}

#[test]
fn bstr_exhaustive_cases_persist_as_long_unicode_text() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/exhaustive");
    if !dir.exists() {
        return;
    }

    for scenario in REQUIRED_SCENARIOS {
        let stem = format!("flat_BSTR_{scenario}");
        let xml_path = dir.join(format!("{stem}.xml"));
        let adtg_path = dir.join(format!("{stem}.adtg"));

        let xml = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
            .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", xml_path.display()));
        let native = parse_adtg_bytes(&fs::read(&adtg_path).unwrap())
            .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", adtg_path.display()));

        assert_materialized_views_match(&xml, &native, &stem);
        assert_bstr_materialized_as_long_unicode(&xml, &format!("{stem} XML"));
        assert_bstr_materialized_as_long_unicode(&native, &format!("{stem} ADTG"));
    }
}

#[test]
fn unsupported_mdac_flat_types_are_documented() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/exhaustive");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("unsupported.csv"));
    let mut seen = HashMap::new();
    for row in rows {
        assert_eq!(row.len(), 5, "unsupported.csv columns: {row:?}");
        assert!(
            seen.insert(row[0].clone(), row).is_none(),
            "duplicate unsupported.csv row"
        );
    }

    for (type_name, type_code, result) in EXPECTED_UNSUPPORTED {
        let row = seen
            .get(*type_name)
            .unwrap_or_else(|| panic!("missing unsupported row for {type_name}"));
        assert_eq!(
            (row[1].as_str(), row[2].as_str()),
            (*type_code, *result),
            "{type_name} unsupported status"
        );
        match *result {
            "fail" => {
                assert!(
                    !row[3].is_empty(),
                    "{type_name} MDAC failure should record an error number"
                );
                assert!(
                    !row[4].is_empty(),
                    "{type_name} MDAC failure should record an error description"
                );
            }
            "probe_ok_not_exhaustive" => {
                assert!(row[3].is_empty(), "{type_name} probe error number");
                assert!(row[4].is_empty(), "{type_name} probe error description");
            }
            other => panic!("unexpected unsupported result {other} for {type_name}"),
        }

        let stem = format!("unsupported_probe_{type_name}");
        assert_absent(&dir.join(format!("{stem}.xml")));
        assert_absent(&dir.join(format!("{stem}.adtg")));
        assert_absent(&dir.join(format!("{stem}.roundtrip.xml")));
    }

    assert_eq!(
        seen.len(),
        EXPECTED_UNSUPPORTED.len(),
        "unsupported.csv row count"
    );
}

#[test]
fn exhaustive_roundtrips_keep_xml_materialized_views() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/exhaustive");
    if !dir.exists() {
        return;
    }

    let mut checked = 0usize;
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("xml")
            || is_roundtrip(&path)
        {
            continue;
        }

        let roundtrip = roundtrip_path(&path);
        let original = parse_ado_xml_bytes(&fs::read(&path).unwrap())
            .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", path.display()));
        let reparsed = parse_ado_xml_bytes(&fs::read(&roundtrip).unwrap())
            .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", roundtrip.display()));

        assert_materialized_views_match(&original, &reparsed, &path.display().to_string());
        checked += 1;
    }

    assert_eq!(checked, exhaustive_case_count());
}

fn exhaustive_case_count() -> usize {
    SUPPORTED_FLAT_TYPES.len() * REQUIRED_SCENARIOS.len() + NUMERIC_PRECISION_SCENARIOS.len()
}

fn expected_persisted_ado_type_code(type_name: &str, declared_code: &str) -> u16 {
    match type_name {
        // MDAC accepts adBSTR Recordset fields, but XML/ADTG persistence
        // reopens them as long Unicode text.
        "BSTR" => 203,
        _ => declared_code.parse().unwrap(),
    }
}

fn assert_value_field_ado_type(
    recordset: &tablegram::model::Recordset,
    expected_code: u16,
    label: &str,
) {
    let field = recordset
        .fields
        .iter()
        .find(|field| field.name == "VALUE_FIELD")
        .unwrap_or_else(|| panic!("{label}: VALUE_FIELD metadata missing"));

    assert_eq!(
        field.ado_type.map(|ty| ty.code),
        Some(expected_code),
        "{label}: VALUE_FIELD ADO type"
    );
}

fn assert_precision_scale_metadata(
    recordset: &tablegram::model::Recordset,
    scenario: &NumericPrecisionScenario,
    label: &str,
) {
    let materialized = materialize_default_view(recordset);
    let field = materialized
        .fields
        .iter()
        .find(|field| field.name == "VALUE_FIELD")
        .unwrap_or_else(|| panic!("{label}: VALUE_FIELD metadata missing"));
    assert_eq!(
        field.ado_type_code,
        Some(scenario.type_code),
        "{label}: ADO type"
    );
    assert_eq!(
        field.precision,
        Some(scenario.precision),
        "{label}: precision"
    );
    assert_eq!(field.scale, Some(scenario.scale), "{label}: scale");
}

fn assert_bstr_materialized_as_long_unicode(recordset: &tablegram::model::Recordset, label: &str) {
    let field = recordset
        .fields
        .iter()
        .find(|field| field.name == "VALUE_FIELD")
        .unwrap_or_else(|| panic!("{label}: VALUE_FIELD metadata missing"));

    assert_eq!(
        field.ado_type.map(|ty| ty.code),
        Some(203),
        "{label}: MDAC should persist accepted adBSTR fields as adLongVarWChar"
    );
    assert_eq!(field.max_length, None, "{label}: max length");
    assert!(field.long, "{label}: long flag");
}

fn assert_precision_scale_values(
    recordset: &tablegram::model::Recordset,
    scenario: &NumericPrecisionScenario,
    label: &str,
) {
    let materialized = materialize_default_view(recordset);
    assert_eq!(
        materialized.rows.len(),
        scenario.values.len() + 1,
        "{label}: row count"
    );

    for (index, expected) in scenario.values.iter().enumerate() {
        let row = &materialized.rows[index];
        match row.values.get(1) {
            Some(Value::Decimal(actual)) => assert!(
                numeric_text_matches(actual, expected),
                "{label}: row {} decimal mismatch: actual={actual:?} expected={expected:?}",
                index + 1
            ),
            other => panic!("{label}: row {} expected decimal, got {other:?}", index + 1),
        }
    }

    assert_eq!(
        materialized.rows.last().and_then(|row| row.values.get(1)),
        Some(&Value::Null),
        "{label}: trailing null row"
    );
}

fn read_csv_rows(path: &Path) -> Vec<Vec<String>> {
    let text = fs::read_to_string(path).unwrap();
    text.lines().skip(1).map(parse_csv_line).collect()
}

fn assert_absent(path: &Path) {
    assert!(!path.exists(), "unexpected artifact {}", path.display());
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

fn is_roundtrip(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.contains(".roundtrip."))
        .unwrap_or(false)
}

fn roundtrip_path(path: &Path) -> std::path::PathBuf {
    let file_stem = path.file_stem().unwrap().to_string_lossy();
    path.with_file_name(format!("{file_stem}.roundtrip.xml"))
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
