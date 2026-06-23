use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn parse_command_auto_detects_adtg_as_native_recordset() {
    let output = Command::new(env!("CARGO_BIN_EXE_tablegram"))
        .args([
            "parse",
            "--input",
            corpus_path("generated/types_basic.adtg").to_str().unwrap(),
            "--json",
        ])
        .output()
        .expect("failed to run tablegram parse");

    assert_parse_output_is_recordset_json(output);
}

#[test]
fn parse_command_uses_native_recordset_parser_for_explicit_adtg() {
    let output = Command::new(env!("CARGO_BIN_EXE_tablegram"))
        .args([
            "parse",
            "--input",
            corpus_path("generated/types_basic.adtg").to_str().unwrap(),
            "--format",
            "adtg",
            "--json",
        ])
        .output()
        .expect("failed to run tablegram parse");

    assert_parse_output_is_recordset_json(output);
}

#[test]
fn parse_command_auto_detects_adtg_without_process_path() {
    let empty_path = unique_temp_dir("empty_path");
    fs::create_dir(&empty_path).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_tablegram"))
        .env("PATH", &empty_path)
        .args([
            "parse",
            "--input",
            corpus_path("generated/types_basic.adtg").to_str().unwrap(),
            "--json",
        ])
        .output()
        .expect("failed to run tablegram parse");
    let _ = fs::remove_dir_all(&empty_path);

    assert_parse_output_is_recordset_json(output);
}

#[test]
fn parse_command_auto_detects_chaptered_adtg_without_process_path() {
    let empty_path = unique_temp_dir("empty_path");
    fs::create_dir(&empty_path).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_tablegram"))
        .env("PATH", &empty_path)
        .args([
            "parse",
            "--input",
            corpus_path("shape/orders_lines_product_category_shape.adtg")
                .to_str()
                .unwrap(),
            "--json",
        ])
        .output()
        .expect("failed to run tablegram parse");
    let _ = fs::remove_dir_all(&empty_path);

    let json = assert_parse_output_is_recordset_json(output);
    assert_recordset_json_has_chapter_value(&json);
}

#[test]
fn parse_command_auto_detects_sqlserver_sales_join_without_process_path() {
    let input = corpus_path("sqlserver_sales/sales_mixed_join.adtg");
    if !input.exists() {
        return;
    }

    let empty_path = unique_temp_dir("empty_path");
    fs::create_dir(&empty_path).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_tablegram"))
        .env("PATH", &empty_path)
        .args(["parse", "--input", input.to_str().unwrap()])
        .output()
        .expect("failed to run tablegram parse");
    let _ = fs::remove_dir_all(&empty_path);

    assert!(
        output.status.success(),
        "parse failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ADO ADTG native: 58 fields, 720 rows, 720 changes"),
        "{stdout}"
    );
    for marker in [
        "ORDER_ID type=int ado=adInteger",
        "CUSTOMER_NAME type=string ado=adVarWChar",
        "SHIP_LABEL type=bin.hex ado=adLongVarBinary",
        "RATIO_NUMERIC type=number ado=adNumeric",
    ] {
        assert!(
            stdout.contains(marker),
            "sales_mixed_join summary should contain {marker:?}\n{stdout}"
        );
    }
}

#[test]
fn parse_command_auto_detects_xml_without_process_path() {
    let empty_path = unique_temp_dir("empty_path");
    fs::create_dir(&empty_path).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_tablegram"))
        .env("PATH", &empty_path)
        .args([
            "parse",
            "--input",
            corpus_path("generated/types_basic.xml").to_str().unwrap(),
            "--json",
        ])
        .output()
        .expect("failed to run tablegram parse");
    let _ = fs::remove_dir_all(&empty_path);

    assert_parse_output_is_recordset_json(output);
}

#[test]
fn parse_command_explicit_adtg_parses_chaptered_adtg_without_process_path() {
    let empty_path = unique_temp_dir("empty_path");
    fs::create_dir(&empty_path).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_tablegram"))
        .env("PATH", &empty_path)
        .args([
            "parse",
            "--input",
            corpus_path("shape/orders_lines_product_category_shape.adtg")
                .to_str()
                .unwrap(),
            "--format",
            "adtg",
            "--json",
        ])
        .output()
        .expect("failed to run tablegram parse --format adtg");
    let _ = fs::remove_dir_all(&empty_path);

    assert_parse_output_is_recordset_json(output);
}

#[test]
fn parse_adtg_native_command_parses_chaptered_adtg_without_process_path() {
    let empty_path = unique_temp_dir("empty_path");
    fs::create_dir(&empty_path).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_tablegram"))
        .env("PATH", &empty_path)
        .args([
            "parse-adtg-native",
            "--input",
            corpus_path("shape/orders_lines_product_category_shape.adtg")
                .to_str()
                .unwrap(),
            "--json",
        ])
        .output()
        .expect("failed to run tablegram parse-adtg-native");
    let _ = fs::remove_dir_all(&empty_path);

    assert_parse_output_is_recordset_json(output);
}

#[test]
fn inspect_adtg_command_still_returns_binary_inspection_json() {
    let output = Command::new(env!("CARGO_BIN_EXE_tablegram"))
        .args([
            "inspect-adtg",
            "--input",
            corpus_path("generated/types_basic.adtg").to_str().unwrap(),
            "--json",
        ])
        .output()
        .expect("failed to run tablegram inspect-adtg");

    assert!(
        output.status.success(),
        "inspect-adtg failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value =
        serde_json::from_slice(&output.stdout).expect("inspect-adtg output was not JSON");
    assert!(json.get("header_hex").is_some(), "header hex missing");
    assert!(
        json.get("fields").is_none(),
        "inspect-adtg returned Recordset JSON instead of inspection JSON"
    );
}

#[test]
fn verify_native_corpus_checks_all_checked_corpora_without_com() {
    let mut corpora = vec![
        (
            "generated",
            "Native corpus verification ok: 8 XML files, 8 ADTG files, 8 ADTG/XML pairs",
        ),
        (
            "fuzz",
            "Native corpus verification ok: 352 XML files, 174 ADTG files, 174 ADTG/XML pairs",
        ),
        (
            "exhaustive",
            "Native corpus verification ok: 208 XML files, 104 ADTG files, 104 ADTG/XML pairs",
        ),
        (
            "variant",
            "Native corpus verification ok: 32 XML files, 16 ADTG files, 16 ADTG/XML pairs",
        ),
    ];
    if sqlserver_sales_corpus_present() {
        corpora.push((
            "sqlserver_sales",
            "Native corpus verification ok: 24 XML files, 12 ADTG files, 12 ADTG/XML pairs",
        ));
    }
    corpora.push(
        (
            "shape",
            "Native corpus verification ok: 62 XML files, 40 ADTG files, 31 ADTG/XML pairs, 9 ADTG-only artifacts",
        ),
    );

    for (corpus, expected) in corpora {
        let output = Command::new(env!("CARGO_BIN_EXE_tablegram"))
            .args([
                "verify-native-corpus",
                "--dir",
                corpus_path(corpus).to_str().unwrap(),
            ])
            .output()
            .expect("failed to run tablegram verify-native-corpus");

        assert!(
            output.status.success(),
            "{corpus} verify-native-corpus failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains(expected), "{corpus}: {stdout}");
    }
}

#[test]
fn verify_native_corpus_rejects_unpaired_adtg_artifacts() {
    let temp_dir = unique_temp_dir("unpaired_adtg");
    fs::create_dir(&temp_dir).unwrap();
    fs::copy(
        corpus_path("generated/types_basic.adtg"),
        temp_dir.join("types_basic.adtg"),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_tablegram"))
        .args(["verify-native-corpus", "--dir", temp_dir.to_str().unwrap()])
        .output()
        .expect("failed to run tablegram verify-native-corpus");

    let _ = fs::remove_dir_all(&temp_dir);

    assert!(
        !output.status.success(),
        "verify-native-corpus should reject unpaired ADTG\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("missing matching .roundtrip.xml or same-stem .xml"),
        "{stderr}"
    );
}

#[test]
fn compare_native_command_accepts_equivalent_xml_and_adtg() {
    let output = Command::new(env!("CARGO_BIN_EXE_tablegram"))
        .args([
            "compare-native",
            "--left",
            corpus_path("generated/types_basic.xml").to_str().unwrap(),
            "--right",
            corpus_path("generated/types_basic.adtg").to_str().unwrap(),
        ])
        .output()
        .expect("failed to run tablegram compare-native");

    assert!(
        output.status.success(),
        "compare-native failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Native comparison ok"), "{stdout}");
}

#[test]
fn compare_native_command_rejects_mismatched_recordsets() {
    let output = Command::new(env!("CARGO_BIN_EXE_tablegram"))
        .args([
            "compare-native",
            "--left",
            corpus_path("generated/types_basic.xml").to_str().unwrap(),
            "--right",
            corpus_path("fuzz/empty_rowset.xml").to_str().unwrap(),
        ])
        .output()
        .expect("failed to run tablegram compare-native");

    assert!(
        !output.status.success(),
        "compare-native should reject mismatched recordsets\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("native comparison failed"), "{stderr}");
}

#[test]
fn verify_native_corpus_still_parses_documented_adtg_only_artifacts() {
    let temp_dir = unique_temp_dir("corrupt_documented_adtg_only");
    fs::create_dir(&temp_dir).unwrap();
    fs::write(
        temp_dir.join("orders_pending_changes_shape.adtg"),
        b"not a valid ADTG recordset",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_tablegram"))
        .args(["verify-native-corpus", "--dir", temp_dir.to_str().unwrap()])
        .output()
        .expect("failed to run tablegram verify-native-corpus");

    let _ = fs::remove_dir_all(&temp_dir);

    assert!(
        !output.status.success(),
        "documented ADTG-only artifact should still be parsed before semantic verification\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("orders_pending_changes_shape.adtg"),
        "failure should identify the documented ADTG-only artifact: {stderr}"
    );
    assert!(
        stderr.contains("native corpus verification failed"),
        "{stderr}"
    );
}

#[test]
#[cfg(not(feature = "oracle"))]
fn oracle_commands_are_hidden_without_oracle_feature() {
    let output = Command::new(env!("CARGO_BIN_EXE_tablegram"))
        .arg("--help")
        .output()
        .expect("failed to run tablegram --help");

    assert!(
        output.status.success(),
        "--help failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    for hidden in [
        "parse-adtg-com",
        "verify-com",
        "verify-writer-com",
        "verify-corpus",
    ] {
        assert!(
            !stdout.contains(hidden),
            "{hidden} should require --features oracle\n{stdout}"
        );
    }
}

#[test]
#[cfg(feature = "oracle")]
fn verify_corpus_still_parses_documented_com_verification_skips() {
    let temp_dir = unique_temp_dir("corrupt_com_skip");
    fs::create_dir(&temp_dir).unwrap();
    fs::write(
        temp_dir.join("doc_float_type_aliases.xml"),
        b"not a valid ADO XML recordset",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_tablegram"))
        .args(["verify-corpus", "--dir", temp_dir.to_str().unwrap()])
        .output()
        .expect("failed to run tablegram verify-corpus");

    let _ = fs::remove_dir_all(&temp_dir);

    assert!(
        !output.status.success(),
        "documented COM verification skip should still be parsed natively\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("doc_float_type_aliases.xml"),
        "failure should identify the documented COM skip artifact: {stderr}"
    );
    assert!(
        stderr.contains("documented COM verification skip failed native parse"),
        "{stderr}"
    );
    assert!(
        stderr.contains("COM corpus verification failed"),
        "{stderr}"
    );
}

#[test]
#[cfg(feature = "oracle")]
fn verify_corpus_format_filter_applies_before_com_skip_preflight() {
    let temp_dir = unique_temp_dir("format_filtered_com_skips");
    fs::create_dir(&temp_dir).unwrap();
    fs::write(
        temp_dir.join("doc_float_type_aliases.xml"),
        b"not a valid ADO XML recordset",
    )
    .unwrap();
    fs::write(
        temp_dir.join("doc_float_type_aliases.adtg"),
        b"not a valid ADTG recordset",
    )
    .unwrap();

    let adtg_only = Command::new(env!("CARGO_BIN_EXE_tablegram"))
        .args([
            "verify-corpus",
            "--dir",
            temp_dir.to_str().unwrap(),
            "--format",
            "adtg",
        ])
        .output()
        .expect("failed to run tablegram verify-corpus --format adtg");
    let xml_only = Command::new(env!("CARGO_BIN_EXE_tablegram"))
        .args([
            "verify-corpus",
            "--dir",
            temp_dir.to_str().unwrap(),
            "--format",
            "xml",
        ])
        .output()
        .expect("failed to run tablegram verify-corpus --format xml");

    let _ = fs::remove_dir_all(&temp_dir);

    assert!(
        !adtg_only.status.success(),
        "ADTG filter should still parse matching documented COM skip\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&adtg_only.stdout),
        String::from_utf8_lossy(&adtg_only.stderr)
    );
    let adtg_stderr = String::from_utf8_lossy(&adtg_only.stderr);
    assert!(
        adtg_stderr.contains("doc_float_type_aliases.adtg"),
        "{adtg_stderr}"
    );
    assert!(
        !adtg_stderr.contains("doc_float_type_aliases.xml"),
        "{adtg_stderr}"
    );

    assert!(
        !xml_only.status.success(),
        "XML filter should still parse matching documented COM skip\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&xml_only.stdout),
        String::from_utf8_lossy(&xml_only.stderr)
    );
    let xml_stderr = String::from_utf8_lossy(&xml_only.stderr);
    assert!(
        xml_stderr.contains("doc_float_type_aliases.xml"),
        "{xml_stderr}"
    );
    assert!(
        !xml_stderr.contains("doc_float_type_aliases.adtg"),
        "{xml_stderr}"
    );
}

#[test]
fn verify_native_corpus_runs_without_process_path() {
    let empty_path = unique_temp_dir("empty_path");
    fs::create_dir(&empty_path).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_tablegram"))
        .env("PATH", &empty_path)
        .args([
            "verify-native-corpus",
            "--dir",
            corpus_path("generated").to_str().unwrap(),
        ])
        .output()
        .expect("failed to run tablegram verify-native-corpus");
    let _ = fs::remove_dir_all(&empty_path);

    assert!(
        output.status.success(),
        "verify-native-corpus failed without PATH\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Native corpus verification ok: 8 XML files, 8 ADTG files"),
        "{stdout}"
    );
}

#[test]
fn verify_native_corpus_checks_chaptered_shape_corpus_without_process_path() {
    let empty_path = unique_temp_dir("empty_path");
    fs::create_dir(&empty_path).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_tablegram"))
        .env("PATH", &empty_path)
        .args([
            "verify-native-corpus",
            "--dir",
            corpus_path("shape").to_str().unwrap(),
        ])
        .output()
        .expect("failed to run tablegram verify-native-corpus");
    let _ = fs::remove_dir_all(&empty_path);

    assert!(
        output.status.success(),
        "shape verify-native-corpus failed without PATH\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(
            "Native corpus verification ok: 62 XML files, 40 ADTG files, 31 ADTG/XML pairs, 9 ADTG-only artifacts"
        ),
        "{stdout}"
    );
}

#[test]
fn verify_native_corpus_checks_full_checked_corpus_without_process_path() {
    let empty_path = unique_temp_dir("empty_path");
    fs::create_dir(&empty_path).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_tablegram"))
        .env("PATH", &empty_path)
        .args([
            "verify-native-corpus",
            "--dir",
            corpus_path("").to_str().unwrap(),
        ])
        .output()
        .expect("failed to run tablegram verify-native-corpus");
    let _ = fs::remove_dir_all(&empty_path);

    assert!(
        output.status.success(),
        "full verify-native-corpus failed without PATH\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(expected_full_native_summary()), "{stdout}");
}

fn corpus_path(relative: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("corpus")
        .join(relative)
}

fn expected_full_native_summary() -> &'static str {
    if sqlserver_sales_corpus_present() {
        "Native corpus verification ok: 686 XML files, 354 ADTG files, 345 ADTG/XML pairs, 9 ADTG-only artifacts"
    } else {
        "Native corpus verification ok: 662 XML files, 342 ADTG files, 333 ADTG/XML pairs, 9 ADTG-only artifacts"
    }
}

fn sqlserver_sales_corpus_present() -> bool {
    corpus_path("sqlserver_sales").exists()
}

fn unique_temp_dir(label: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "tablegram_{label}_{}_{}",
        std::process::id(),
        unique
    ))
}

fn assert_parse_output_is_recordset_json(output: std::process::Output) -> Value {
    assert!(
        output.status.success(),
        "parse failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).expect("parse output was not JSON");
    assert!(json.get("fields").is_some(), "recordset fields missing");
    assert!(json.get("rows").is_some(), "recordset rows missing");
    assert!(
        json.get("header_hex").is_none(),
        "parse command returned ADTG inspection JSON instead of Recordset JSON"
    );
    json
}

fn assert_recordset_json_has_chapter_value(json: &Value) {
    let fields = json
        .get("fields")
        .and_then(Value::as_array)
        .expect("recordset fields should be an array");
    assert!(
        fields
            .iter()
            .any(|field| field.pointer("/ado_type/code").and_then(Value::as_u64) == Some(136)),
        "recordset JSON should contain an adChapter field: {json}"
    );

    let rows = json
        .get("rows")
        .and_then(Value::as_array)
        .expect("recordset rows should be an array");
    assert!(
        rows.iter().any(|row| row
            .get("values")
            .and_then(Value::as_array)
            .is_some_and(|values| values.iter().any(is_chapter_value))),
        "recordset JSON should contain a nested chapter value: {json}"
    );
}

fn is_chapter_value(value: &Value) -> bool {
    value.get("kind").and_then(Value::as_str) == Some("chapter")
        && value
            .pointer("/value/fields")
            .and_then(Value::as_array)
            .is_some()
        && value
            .pointer("/value/rows")
            .and_then(Value::as_array)
            .is_some()
}
