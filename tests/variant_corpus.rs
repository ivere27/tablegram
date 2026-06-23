use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use tablegram::adtg::{inspect_adtg, parse_adtg_bytes};
use tablegram::compat::{materialize_default_view, materialize_pending_view, MaterializedRow};
use tablegram::model::{RecordStatusFlag, Value};
use tablegram::native_compare::compare_native_recordsets;
use tablegram::xml::parse_ado_xml_bytes;

const ACCEPTED_VARIANTS: &[(&str, &str)] = &[
    ("variant_sbyte", "sbyte"),
    ("variant_byte", "byte"),
    ("variant_smallint", "smallint"),
    ("variant_integer", "integer"),
    ("variant_single", "single"),
    ("variant_double", "double"),
    ("variant_currency", "currency"),
    ("variant_boolean", "boolean"),
    ("variant_date", "date"),
    ("variant_empty", "empty"),
    ("variant_null", "null"),
    ("variant_decimal", "decimal"),
    ("variant_int64", "int64"),
    ("variant_uint16", "uint16"),
    ("variant_uint32", "uint32"),
    ("variant_uint64", "uint64"),
];

const FAILED_VARIANTS: &[(&str, &str, &str)] = &[
    ("variant_string", "string", "-2147217891"),
    ("variant_binary", "binary", "-2147217891"),
    ("variant_mixed", "mixed", "-2147217891"),
    ("variant_error", "error", "-2147352562"),
];

#[test]
fn variant_generator_assets_cover_declared_subtype_matrix() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let script = fs::read_to_string(root.join("tools/make_variant_corpus.vbs")).unwrap();
    let supplement = fs::read_to_string(root.join("tools/make_variant_decimal.ps1")).unwrap();

    for (case_name, scenario) in ACCEPTED_VARIANTS {
        if is_variant_supplement_case(case_name) {
            assert!(
                supplement.contains(&format!("Name = '{case_name}'"))
                    && supplement.contains(&format!("Scenario = '{scenario}'")),
                "variant PowerShell supplement should generate {case_name}/{scenario}"
            );
        } else {
            assert!(
                script.contains(&format!("MakeScenario \"{case_name}\", \"{scenario}\"")),
                "variant VBScript generator should generate {case_name}/{scenario}"
            );
        }
    }

    for (case_name, scenario, _error_number) in FAILED_VARIANTS {
        if *case_name == "variant_error" {
            assert!(
                script.contains("WriteVariantErrorProbeFailure"),
                "variant error probe should stay documented as a generator failure"
            );
        } else {
            assert!(
                script.contains(&format!("MakeScenario \"{case_name}\", \"{scenario}\"")),
                "variant VBScript generator should probe failed case {case_name}/{scenario}"
            );
        }
    }
}

#[test]
fn variant_manifest_matches_checked_artifacts_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/variant");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    assert_eq!(
        rows.len(),
        ACCEPTED_VARIANTS.len() + FAILED_VARIANTS.len(),
        "variant manifest should cover every focused subtype probe"
    );

    let mut manifest_artifacts = BTreeSet::new();
    let mut missing = Vec::new();
    for row in rows {
        assert_eq!(row.len(), 8, "variant manifest columns: {row:?}");
        if row[2] == "fail" {
            assert!(row[3].is_empty(), "{} XML should be empty", row[0]);
            assert!(row[4].is_empty(), "{} ADTG should be empty", row[0]);
            assert!(
                row[5].is_empty(),
                "{} roundtrip XML should be empty",
                row[0]
            );
            continue;
        }

        for artifact in &row[3..6] {
            let path = manifest_artifact_path(&dir, artifact);
            if !path.exists() {
                missing.push(artifact.clone());
            }
            manifest_artifacts.insert(
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_else(|| panic!("invalid variant manifest path {artifact}"))
                    .to_string(),
            );
        }
    }
    assert!(
        missing.is_empty(),
        "variant manifest references missing artifacts: {missing:?}"
    );

    let actual_artifacts = corpus_artifacts(&dir);
    assert_eq!(
        actual_artifacts, manifest_artifacts,
        "variant manifest artifact list"
    );
}

#[test]
fn parses_variant_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/variant");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    assert_eq!(
        rows.len(),
        ACCEPTED_VARIANTS.len() + FAILED_VARIANTS.len(),
        "variant manifest should cover every focused subtype probe"
    );
    let mut seen = HashMap::new();
    for row in rows {
        let case_name = row[0].clone();
        assert!(
            seen.insert(case_name.clone(), row).is_none(),
            "duplicate variant manifest row for {case_name}"
        );
    }

    for (case_name, scenario) in ACCEPTED_VARIANTS {
        let row = seen
            .get(*case_name)
            .unwrap_or_else(|| panic!("missing accepted variant row for {case_name}"));
        assert_eq!(row[1], *scenario, "{case_name} scenario");
        assert_eq!(row[2], "ok", "{case_name} result");
        assert!(Path::new(&row[3]).exists(), "{case_name} XML missing");
        assert!(Path::new(&row[4]).exists(), "{case_name} ADTG missing");
        assert!(
            Path::new(&row[5]).exists(),
            "{case_name} ADTG-to-XML roundtrip missing"
        );
    }

    for (case_name, scenario, error_number) in FAILED_VARIANTS {
        let row = seen
            .get(*case_name)
            .unwrap_or_else(|| panic!("missing failed variant row for {case_name}"));
        assert_eq!(row[1], *scenario, "{case_name} scenario");
        assert_eq!(row[2], "fail", "{case_name} result");
        assert!(row[3].is_empty(), "{case_name} XML should not be generated");
        assert!(
            row[4].is_empty(),
            "{case_name} ADTG should not be generated"
        );
        assert_eq!(row[6], *error_number, "{case_name} error number");
        assert_absent(&dir.join(format!("{case_name}.xml")));
        assert_absent(&dir.join(format!("{case_name}.adtg")));
        assert_absent(&dir.join(format!("{case_name}.roundtrip.xml")));
    }

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
                        panic!("failed to parse variant XML {}: {err:#}", path.display())
                    });
                assert_eq!(recordset.fields.len(), 2, "{}", path.display());
                xml_count += 1;
            }
            Some("adtg") => {
                let bytes = fs::read(&path).unwrap();
                let document = inspect_adtg(&bytes).unwrap_or_else(|err| {
                    panic!("failed to inspect variant ADTG {}: {err:#}", path.display())
                });
                assert_eq!(document.length, bytes.len(), "{}", path.display());
                adtg_count += 1;
            }
            _ => {}
        }
    }

    assert_eq!(
        xml_count,
        ACCEPTED_VARIANTS.len() * 2,
        "variant XML and ADTG-roundtrip XML files"
    );
    assert_eq!(adtg_count, ACCEPTED_VARIANTS.len(), "variant ADTG files");
}

#[test]
fn native_adtg_parses_accepted_variant_corpus_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/variant");
    if !dir.exists() {
        return;
    }

    for (file_name, expected) in [
        ("variant_sbyte.adtg", variant_number("-5", "1", "0", "127")),
        ("variant_byte.adtg", variant_number("1", "254", "42", "255")),
        (
            "variant_smallint.adtg",
            variant_number("-1", "1", "0", "32767"),
        ),
        (
            "variant_integer.adtg",
            variant_number("-1", "1", "0", "2147483647"),
        ),
        (
            "variant_int64.adtg",
            variant_number("-1", "1", "0", "9223372036854775807"),
        ),
        (
            "variant_uint16.adtg",
            variant_number("1", "65534", "42", "65535"),
        ),
        (
            "variant_uint32.adtg",
            variant_number("1", "4294967294", "42", "4294967295"),
        ),
        (
            "variant_uint64.adtg",
            variant_number("1", "18446744073709551614", "42", "18446744073709551615"),
        ),
        (
            "variant_single.adtg",
            variant_number("-1.5", "1.25", "0", "12345.5"),
        ),
        (
            "variant_double.adtg",
            variant_number("-1.5", "1.25", "0", "123456.5"),
        ),
        (
            "variant_currency.adtg",
            variant_number("-1.0001", "1.0001", "0", "1234.5678"),
        ),
        (
            "variant_decimal.adtg",
            variant_number("-1.0001", "1.0001", "0", "1234.5678"),
        ),
        ("variant_boolean.adtg", variant_bool()),
        ("variant_date.adtg", variant_date()),
        ("variant_empty.adtg", variant_empty()),
        ("variant_null.adtg", variant_null()),
    ] {
        let path = dir.join(file_name);
        let recordset = parse_adtg_bytes(&fs::read(&path).unwrap()).unwrap_or_else(|err| {
            panic!("failed to parse native ADTG {}: {err:#}", path.display())
        });

        assert_eq!(
            recordset.fields[1].ado_type.map(|ty| ty.code),
            Some(12),
            "{}",
            path.display()
        );

        let default_view = materialize_default_view(&recordset);
        assert_eq!(
            default_view.rows,
            expected.default_rows,
            "{} default view",
            path.display()
        );

        let pending_view = materialize_pending_view(&recordset);
        assert_rows_unordered_eq(
            pending_view.rows,
            expected.pending_rows,
            &format!("{} pending view", path.display()),
        );
    }
}

#[test]
fn native_variant_roundtrip_xml_text_compares_to_adtg_variant_values() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/variant");
    if !dir.exists() {
        return;
    }

    for (case_name, _scenario) in ACCEPTED_VARIANTS {
        let xml_path = dir.join(format!("{case_name}.roundtrip.xml"));
        let adtg_path = dir.join(format!("{case_name}.adtg"));
        let xml = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
            .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
        let adtg = parse_adtg_bytes(&fs::read(&adtg_path).unwrap())
            .unwrap_or_else(|err| panic!("failed to parse ADTG {}: {err:#}", adtg_path.display()));

        let mismatches = compare_native_recordsets(&xml, &adtg);
        assert!(
            mismatches.is_empty(),
            "{} variant text comparison mismatches:\n{}",
            case_name,
            mismatches.join("\n")
        );
    }
}

#[test]
fn variant_empty_xml_reopens_as_null_but_adtg_preserves_empty() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/variant");
    if !dir.exists() {
        return;
    }

    let xml = parse_ado_xml_bytes(&fs::read(dir.join("variant_empty.xml")).unwrap())
        .expect("failed to parse variant_empty XML");
    let adtg = parse_adtg_bytes(&fs::read(dir.join("variant_empty.adtg")).unwrap())
        .expect("failed to parse variant_empty ADTG");

    assert_variant_empty_values(
        &materialize_default_view(&xml).rows,
        &Value::Null,
        "XML default",
    );
    assert_variant_empty_values(
        &materialize_pending_view(&xml).rows,
        &Value::Null,
        "XML pending",
    );
    assert_variant_empty_values(
        &materialize_default_view(&adtg).rows,
        &Value::Empty,
        "ADTG default",
    );
    assert_variant_empty_values(
        &materialize_pending_view(&adtg).rows,
        &Value::Empty,
        "ADTG pending",
    );
}

struct ExpectedVariantRows {
    default_rows: Vec<MaterializedRow>,
    pending_rows: Vec<MaterializedRow>,
}

fn variant_number(
    deleted: &str,
    updated: &str,
    current: &str,
    inserted: &str,
) -> ExpectedVariantRows {
    variant_rows(
        Value::Decimal(deleted.to_string()),
        Value::Decimal(updated.to_string()),
        Value::Decimal(current.to_string()),
        Value::Decimal(inserted.to_string()),
    )
}

fn variant_bool() -> ExpectedVariantRows {
    variant_rows(
        Value::Boolean(true),
        Value::Boolean(true),
        Value::Boolean(false),
        Value::Boolean(false),
    )
}

fn variant_date() -> ExpectedVariantRows {
    variant_rows(
        Value::DateTime("1999-12-31T23:59:58".to_string()),
        Value::DateTime("2026-06-12T14:30:15".to_string()),
        Value::DateTime("2000-01-01T00:00:01".to_string()),
        Value::DateTime("2038-01-19T03:14:07".to_string()),
    )
}

fn variant_null() -> ExpectedVariantRows {
    variant_rows(Value::Null, Value::Null, Value::Null, Value::Null)
}

fn variant_empty() -> ExpectedVariantRows {
    variant_rows(Value::Empty, Value::Empty, Value::Empty, Value::Empty)
}

fn variant_rows(
    deleted: Value,
    updated: Value,
    current: Value,
    inserted: Value,
) -> ExpectedVariantRows {
    ExpectedVariantRows {
        default_rows: vec![
            row(RecordStatusFlag::Modified, 1, updated.clone()),
            row(RecordStatusFlag::Unmodified, 3, current),
            row(RecordStatusFlag::New, 4, inserted.clone()),
        ],
        pending_rows: vec![
            row(RecordStatusFlag::Modified, 1, updated),
            row(RecordStatusFlag::New, 4, inserted),
            row(RecordStatusFlag::Deleted, 2, deleted),
        ],
    }
}

fn row(status: RecordStatusFlag, id: i64, value: Value) -> MaterializedRow {
    MaterializedRow {
        status,
        values: vec![Value::Integer(id), value],
    }
}

fn assert_rows_unordered_eq(left: Vec<MaterializedRow>, right: Vec<MaterializedRow>, label: &str) {
    let mut unmatched = right;
    for row in left {
        let index = unmatched
            .iter()
            .position(|candidate| candidate == &row)
            .unwrap_or_else(|| panic!("{label}: pending row not found: {row:?}"));
        unmatched.remove(index);
    }
    assert!(
        unmatched.is_empty(),
        "{label}: unmatched pending rows: {unmatched:?}"
    );
}

fn assert_variant_empty_values(rows: &[MaterializedRow], expected: &Value, label: &str) {
    assert!(!rows.is_empty(), "{label}: expected rows");
    for row in rows {
        if matches!(row.status, RecordStatusFlag::Deleted)
            && row
                .values
                .iter()
                .all(|value| matches!(value, Value::Unavailable))
        {
            continue;
        }
        assert_eq!(row.values.get(1), Some(expected), "{label}: row {row:?}");
    }
}

fn is_variant_supplement_case(case_name: &str) -> bool {
    matches!(
        case_name,
        "variant_sbyte"
            | "variant_decimal"
            | "variant_int64"
            | "variant_uint16"
            | "variant_uint32"
            | "variant_uint64"
    )
}

fn read_csv_rows(path: &Path) -> Vec<Vec<String>> {
    let text = fs::read_to_string(path).unwrap();
    text.lines().skip(1).map(parse_csv_line).collect()
}

fn manifest_artifact_path(dir: &Path, artifact: &str) -> PathBuf {
    let normalized = artifact.replace('\\', "/");
    let path = Path::new(&normalized);
    if path.is_absolute() && path.exists() {
        path.to_path_buf()
    } else {
        let file_name = path
            .file_name()
            .unwrap_or_else(|| panic!("invalid manifest artifact path {artifact}"));
        dir.join(file_name)
    }
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
        "failed variant artifact should not exist: {}",
        path.display()
    );
}
