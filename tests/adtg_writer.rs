use std::fs;
use std::path::{Path, PathBuf};

use tablegram::adtg::{parse_adtg_bytes, parse_adtg_bytes_with_options, AdtgParseOptions};
use tablegram::model::AdoDataType;
use tablegram::native_compare::{compare_mdac_resaved_recordsets, compare_native_recordsets};
use tablegram::{
    parse_recordset_file, write_adtg, write_adtg_with_options, AdtgWriteOptions, Value,
};

#[test]
fn writes_generated_adtg_corpus_roundtrips() {
    assert_adtg_dir_roundtrips("corpus/generated");
}

#[test]
fn writes_exhaustive_adtg_corpus_roundtrips() {
    assert_adtg_dir_roundtrips("corpus/exhaustive");
}

#[test]
fn writes_variant_adtg_corpus_roundtrips() {
    assert_adtg_dir_roundtrips("corpus/variant");
}

#[test]
fn writes_fuzz_adtg_corpus_roundtrips() {
    assert_adtg_dir_roundtrips("corpus/fuzz");
}

#[test]
fn writes_xml_varnumeric_with_mdac_unspecified_precision_scale() {
    let xml_path = corpus_path("corpus/fuzz/doc_number_varnumeric.xml");
    if !xml_path.exists() {
        return;
    }

    let source = parse_recordset_file(&xml_path)
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", xml_path.display()));
    let written = write_adtg(&source)
        .unwrap_or_else(|err| panic!("failed to write {} as ADTG: {err:#}", xml_path.display()));
    let reparsed = parse_adtg_bytes(&written)
        .unwrap_or_else(|err| panic!("failed to parse written ADTG: {err:#}"));

    for field in reparsed.fields.iter().skip(1) {
        assert_eq!(
            field.ado_type.map(|ado_type| ado_type.code),
            Some(139),
            "{} should stay adVarNumeric",
            field.name
        );
        assert_eq!(
            field.precision,
            Some(255),
            "{} should use MDAC unspecified precision sentinel",
            field.name
        );
        assert_eq!(
            field.scale,
            Some(255),
            "{} should use MDAC unspecified scale sentinel",
            field.name
        );
    }

    let mismatches = compare_mdac_resaved_recordsets(&source, &reparsed);
    assert!(
        mismatches.is_empty(),
        "XML-to-ADTG varnumeric writer mismatches:\n{}",
        mismatches.join("\n")
    );
}

#[test]
fn writes_ansi_text_with_configured_windows_codepage() {
    let mut source = parse_recordset_file(corpus_path("corpus/generated/strings_ascii.xml"))
        .expect("string XML fixture should parse");
    source.fields[1].ado_type = Some(AdoDataType::new("adVarChar", 200));
    source.rows[0].values[1] = Value::String("euro €".to_string());

    let written = write_adtg_with_options(
        &source,
        AdtgWriteOptions::default()
            .with_ansi_encoding_label(b"windows-1252")
            .expect("known Windows-1252 encoding label"),
    )
    .expect("Windows-1252 ANSI text should write as ADTG");

    assert!(
        written
            .windows(b"euro \x80".len())
            .any(|window| window == b"euro \x80"),
        "ADTG writer should persist the Windows-1252 euro byte"
    );

    let reparsed = parse_adtg_bytes_with_options(
        &written,
        AdtgParseOptions::default()
            .with_ansi_encoding_label(b"windows-1252")
            .expect("known Windows-1252 encoding label"),
    )
    .expect("Windows-1252 ADTG should parse with matching options");

    assert_eq!(
        reparsed.rows[0].values[1],
        Value::String("euro €".to_string())
    );
}

#[test]
fn writes_or_explicitly_rejects_shape_adtg_corpus() {
    let dir = corpus_path("corpus/shape");
    if !dir.exists() {
        return;
    }

    let mut written = 0usize;
    let mut unexpected = Vec::new();
    for path in adtg_files(&dir) {
        let source = parse_adtg_bytes(&fs::read(&path).unwrap())
            .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", path.display()));
        match write_adtg(&source) {
            Ok(written_bytes) => {
                written += 1;
                let reparsed = parse_adtg_bytes(&written_bytes).unwrap_or_else(|err| {
                    panic!("failed to parse written ADTG {}: {err:#}", path.display())
                });
                let mismatches = compare_native_recordsets(&source, &reparsed);
                if !mismatches.is_empty() {
                    unexpected.push(format!(
                        "{} writer roundtrip mismatches:\n{}",
                        path.display(),
                        mismatches.join("\n")
                    ));
                }
            }
            Err(err) => {
                unexpected.push(format!("{}: {err:#}", path.display()));
            }
        }
    }

    assert!(
        written > 0,
        "expected at least one shaped ADTG fixture to write"
    );
    assert!(
        unexpected.is_empty(),
        "unexpected shaped ADTG writer results:\n{}",
        unexpected.join("\n\n")
    );
}

#[test]
fn writes_shape_xml_corpus_with_provider_metadata() {
    let dir = corpus_path("corpus/shape");
    if !dir.exists() {
        return;
    }

    let mut written = 0usize;
    let mut unexpected = Vec::new();
    for path in recordset_files(&dir, "xml") {
        let source = parse_recordset_file(&path)
            .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", path.display()));
        match write_adtg(&source) {
            Ok(written_bytes) => {
                written += 1;
                let reparsed = parse_adtg_bytes(&written_bytes).unwrap_or_else(|err| {
                    panic!("failed to parse written ADTG {}: {err:#}", path.display())
                });
                let mismatches = compare_mdac_resaved_recordsets(&source, &reparsed);
                if !mismatches.is_empty() {
                    unexpected.push(format!(
                        "{} XML-to-ADTG writer mismatches:\n{}",
                        path.display(),
                        mismatches.join("\n")
                    ));
                }
            }
            Err(err) => unexpected.push(format!("{}: {err:#}", path.display())),
        }
    }

    assert!(
        written > 0,
        "expected at least one shaped XML fixture to write"
    );
    assert!(
        unexpected.is_empty(),
        "unexpected shaped XML-to-ADTG writer results:\n{}",
        unexpected.join("\n\n")
    );
}

#[test]
fn writes_shape_xml_provider_descriptor_bytes_like_mdac_fixture() {
    let xml_path = corpus_path("corpus/shape/orders_lines_product_shape.xml");
    let adtg_path = corpus_path("corpus/shape/orders_lines_product_shape.adtg");
    if !xml_path.exists() || !adtg_path.exists() {
        return;
    }

    let source = parse_recordset_file(&xml_path)
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", xml_path.display()));
    let written = write_adtg(&source)
        .unwrap_or_else(|err| panic!("failed to write {} as ADTG: {err:#}", xml_path.display()));
    let expected = fs::read(&adtg_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err:#}", adtg_path.display()));

    assert_eq!(
        written,
        expected,
        "XML-to-ADTG shaped provider descriptor bytes changed for {}",
        xml_path.display()
    );
}

#[test]
fn writes_composite_relation_child_schema_with_mdac_wide_layout() {
    let path = corpus_path("corpus/shape/orders_lines_composite_shape.adtg");
    if !path.exists() {
        return;
    }

    let source = parse_recordset_file(&path)
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", path.display()));
    let written = write_adtg(&source)
        .unwrap_or_else(|err| panic!("failed to write {} as ADTG: {err:#}", path.display()));
    let child_offset = find_child_shape_header(&written)
        .unwrap_or_else(|| panic!("written ADTG had no child shape header: {}", path.display()));

    assert_eq!(
        written[child_offset + 0x1c],
        0,
        "composite relation child schema without provider metadata should not inherit provider pending flag"
    );
    assert_eq!(
        &written[child_offset + 0x1f..child_offset + 0x21],
        &9u16.to_be_bytes(),
        "composite relation child schema should describe the materialized child rows"
    );
    assert_eq!(
        &written[child_offset + 0x91..child_offset + 0x93],
        &[0xff, 0xff],
        "composite relation child schema should use MDAC's wide first-size sentinel"
    );
    assert_eq!(
        &written[child_offset + 0x99..child_offset + 0x9b],
        &[0xff, 0xff],
        "composite relation child schema should use MDAC's wide second-size sentinel"
    );
    assert_eq!(
        &written[child_offset + 0xa1..child_offset + 0xa5],
        &0x78u32.to_le_bytes(),
        "composite relation child schema should use MDAC's wide row-size sentinel"
    );
}

#[test]
fn writes_shaped_adtg_provider_catalog_schema_metadata() {
    let path = corpus_path("corpus/shape").join("orders_lines_product_pending_shape.adtg");
    if !path.exists() {
        return;
    }

    let source = parse_adtg_bytes(&fs::read(&path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", path.display()));
    assert_eq!(
        source.fields[0].base_catalog.as_deref(),
        Some("AdoRecordsetSales")
    );
    assert_eq!(source.fields[0].base_schema.as_deref(), Some("dbo"));
    assert_eq!(source.fields[0].base_table.as_deref(), Some("SalesOrders"));
    let source_lines = chapter_field(&source, 0, 3);
    assert_eq!(
        source_lines.fields[0].base_table.as_deref(),
        Some("SalesOrderLines")
    );

    let written = write_adtg(&source)
        .unwrap_or_else(|err| panic!("failed to write {} as ADTG: {err:#}", path.display()));
    let reparsed = parse_adtg_bytes(&written)
        .unwrap_or_else(|err| panic!("failed to parse written ADTG {}: {err:#}", path.display()));
    assert_eq!(
        reparsed.fields[0].base_catalog.as_deref(),
        Some("AdoRecordsetSales")
    );
    assert_eq!(reparsed.fields[0].base_schema.as_deref(), Some("dbo"));
    assert_eq!(
        reparsed.fields[0].base_table.as_deref(),
        Some("SalesOrders")
    );
    let reparsed_lines = chapter_field(&reparsed, 0, 3);
    assert_eq!(
        reparsed_lines.fields[0].base_table.as_deref(),
        Some("SalesOrderLines")
    );
}

fn chapter_field(
    recordset: &tablegram::Recordset,
    row_index: usize,
    field_index: usize,
) -> &tablegram::Recordset {
    match &recordset.rows[row_index].values[field_index] {
        Value::Chapter(chapter) => chapter,
        other => panic!("expected chapter at row {row_index} field {field_index}, got {other:?}"),
    }
}

fn find_child_shape_header(bytes: &[u8]) -> Option<usize> {
    const CHILD_SHAPE_PREFIX: &[u8] = &[0x03, 0x71, 0x00, 0xd2, 0xad, 0x63];
    bytes
        .windows(CHILD_SHAPE_PREFIX.len())
        .position(|window| window == CHILD_SHAPE_PREFIX)
}

fn assert_adtg_dir_roundtrips(relative: &str) {
    let dir = corpus_path(relative);
    if !dir.exists() {
        return;
    }

    let mut unexpected = Vec::new();
    for path in recordset_files(&dir, "adtg") {
        let source = parse_adtg_bytes(&fs::read(&path).unwrap())
            .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", path.display()));
        match write_adtg(&source) {
            Ok(written) => {
                let reparsed = parse_adtg_bytes(&written).unwrap_or_else(|err| {
                    panic!("failed to parse written ADTG {}: {err:#}", path.display())
                });
                let mismatches = compare_native_recordsets(&source, &reparsed);
                if !mismatches.is_empty() {
                    unexpected.push(format!(
                        "{} writer roundtrip mismatches:\n{}",
                        path.display(),
                        mismatches.join("\n")
                    ));
                }
            }
            Err(err) => unexpected.push(format!("{}: {err:#}", path.display())),
        }
    }

    assert!(
        unexpected.is_empty(),
        "unexpected ADTG writer results:\n{}",
        unexpected.join("\n\n")
    );
}

fn adtg_files(dir: &Path) -> Vec<PathBuf> {
    recordset_files(dir, "adtg")
}

fn recordset_files(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut paths = fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some(extension))
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

fn corpus_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
