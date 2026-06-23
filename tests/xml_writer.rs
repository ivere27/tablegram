use std::fs;
use std::path::{Path, PathBuf};

use tablegram::adtg::parse_adtg_bytes;
use tablegram::native_compare::compare_native_recordsets;
use tablegram::xml::parse_ado_xml_bytes;
use tablegram::{parse_recordset_file, write_ado_xml, write_ado_xml_string, Value};

#[test]
fn writes_generated_xml_corpus_roundtrips() {
    assert_xml_dir_roundtrips("corpus/generated", false);
}

#[test]
fn writes_exhaustive_flat_xml_corpus_roundtrips() {
    assert_xml_dir_roundtrips("corpus/exhaustive", false);
}

#[test]
fn writes_variant_xml_corpus_roundtrips() {
    assert_xml_dir_roundtrips("corpus/variant", false);
}

#[test]
fn writes_shape_xml_corpus_roundtrips() {
    assert_xml_dir_roundtrips("corpus/shape", false);
}

#[test]
fn writes_focused_fuzz_xml_cases_roundtrip() {
    for name in [
        "binary_full_range.xml",
        "doc_empty_error_variant_types.xml",
        "doc_minimal_schema.xml",
        "doc_number_varnumeric.xml",
        "doc_nullable_attr_matrix.xml",
        "doc_schema_attribute_refs.xml",
        "empty_rowset.xml",
        "field_attributes.xml",
        "filter_save_pending.xml",
        "fractional_timestamp.xml",
        "multi_changes.xml",
        "name_mapping.xml",
        "special_field_names.xml",
        "text_escapes.xml",
        "text_spaces.xml",
        "whitespace_field_names.xml",
    ] {
        let path = corpus_path("corpus/fuzz").join(name);
        if path.exists() {
            assert_xml_file_roundtrips(&path);
        }
    }
}

#[test]
fn writes_native_variant_adtg_as_mdac_xml_text_storage() {
    let path = corpus_path("corpus/variant").join("variant_null.adtg");
    if !path.exists() {
        return;
    }

    let source = parse_adtg_bytes(&fs::read(&path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", path.display()));
    let xml = write_ado_xml_string(&source)
        .unwrap_or_else(|err| panic!("failed to write {}: {err:#}", path.display()));
    assert!(
        xml.contains("rs:forcenull='VALUE_FIELD'"),
        "updated variant null should be explicit in XML:\n{xml}"
    );

    let reparsed = parse_ado_xml_bytes(xml.as_bytes()).unwrap_or_else(|err| {
        panic!(
            "failed to parse written XML for {}: {err:#}",
            path.display()
        )
    });
    assert_native_match(&source, &reparsed, &path);
}

#[test]
fn writes_adtg_with_invalid_xml_names_using_rs_name_mapping() {
    let path = corpus_path("corpus/fuzz").join("doc_minimal_schema.adtg");
    if !path.exists() {
        return;
    }

    let source = parse_adtg_bytes(&fs::read(&path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", path.display()));
    let written = write_ado_xml(&source)
        .unwrap_or_else(|err| panic!("failed to write {}: {err:#}", path.display()));
    let written_text = String::from_utf8(written.clone()).expect("writer emitted non-UTF8 XML");
    assert!(
        written_text.contains("name='c1' rs:number='2' rs:name='Friendly Name'"),
        "writer did not generate an XML-safe name for Friendly Name:\n{written_text}"
    );
    assert!(
        written_text.contains("name='c2' rs:number='3' rs:name='Direct Int'"),
        "writer did not generate an XML-safe name for Direct Int:\n{written_text}"
    );

    let reparsed = parse_ado_xml_bytes(&written)
        .unwrap_or_else(|err| panic!("failed to parse written XML {}: {err:#}", path.display()));
    assert_native_match(&source, &reparsed, &path);
}

#[test]
fn writes_current_shaped_adtg_recordset_to_xml() {
    let path = corpus_path("corpus/shape").join("orders_lines_product_category_shape.adtg");
    if !path.exists() {
        return;
    }

    let source = parse_adtg_bytes(&fs::read(&path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", path.display()));
    let written = write_ado_xml(&source)
        .unwrap_or_else(|err| panic!("failed to write {}: {err:#}", path.display()));
    let reparsed = parse_ado_xml_bytes(&written)
        .unwrap_or_else(|err| panic!("failed to parse written XML {}: {err:#}", path.display()));
    assert_native_match(&source, &reparsed, &path);
    let written_text = String::from_utf8(written).expect("writer emitted non-UTF8 XML");
    assert!(
        written_text.contains("<Product rs:duplicate='true' ProductId='14'"),
        "writer omitted duplicate Product marker needed by MDAC:\n{written_text}"
    );
    assert!(
        written_text.contains("<Category rs:duplicate='true' CategoryId='6'"),
        "writer omitted duplicate Category marker needed by MDAC:\n{written_text}"
    );
}

#[test]
fn writes_shaped_xml_relation_metadata_required_by_xp_ado() {
    let path = corpus_path("corpus/shape").join("orders_lines_product_shape.adtg");
    if !path.exists() {
        return;
    }

    let source = parse_adtg_bytes(&fs::read(&path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", path.display()));
    let written = write_ado_xml(&source)
        .unwrap_or_else(|err| panic!("failed to write {}: {err:#}", path.display()));
    let written_text = String::from_utf8(written.clone()).expect("writer emitted non-UTF8 XML");
    assert!(
        written_text.contains("rs:relation='010000000100000000000000'"),
        "writer omitted Lines relation metadata:\n{written_text}"
    );
    assert!(
        written_text.contains("rs:relation='040000000100000000000000'"),
        "writer omitted Product relation metadata:\n{written_text}"
    );

    let reparsed = parse_ado_xml_bytes(&written)
        .unwrap_or_else(|err| panic!("failed to parse written XML {}: {err:#}", path.display()));
    assert_eq!(
        source.fields[3].chapter_relation,
        reparsed.fields[3].chapter_relation
    );
    let source_lines = chapter_field(&source, 0, 3);
    let reparsed_lines = chapter_field(&reparsed, 0, 3);
    let source_product = source_lines
        .fields
        .iter()
        .find(|field| field.name == "Product")
        .expect("source Lines chapter lacked Product field");
    let reparsed_product = reparsed_lines
        .fields
        .iter()
        .find(|field| field.name == "Product")
        .expect("reparsed Lines chapter lacked Product field");
    assert_eq!(
        source_product.chapter_relation,
        reparsed_product.chapter_relation
    );
}

#[test]
fn writes_or_explicitly_rejects_shape_adtg_corpus() {
    let dir = corpus_path("corpus/shape");
    if !dir.exists() {
        return;
    }

    let mut unexpected = Vec::new();
    for path in adtg_files(&dir) {
        let source = parse_adtg_bytes(&fs::read(&path).unwrap())
            .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", path.display()));
        match write_ado_xml(&source) {
            Ok(written) => {
                let reparsed = parse_ado_xml_bytes(&written).unwrap_or_else(|err| {
                    panic!("failed to parse written XML {}: {err:#}", path.display())
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
                let message = format!("{err:#}");
                if !expected_shape_adtg_writer_rejection(&path, &message) {
                    unexpected.push(format!("{}: {message}", path.display()));
                }
            }
        }
    }

    assert!(
        unexpected.is_empty(),
        "unexpected shaped ADTG writer results:\n{}",
        unexpected.join("\n\n")
    );
}

#[test]
fn writes_shaped_xml_provider_catalog_schema_metadata() {
    let path = corpus_path("corpus/shape").join("orders_lines_product_shape.xml");
    if !path.exists() {
        return;
    }

    let source = parse_recordset_file(&path)
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", path.display()));
    assert_eq!(
        source.fields[0].base_catalog.as_deref(),
        Some("AdoRecordsetSales")
    );
    assert_eq!(source.fields[0].base_schema.as_deref(), Some("dbo"));

    let written = write_ado_xml(&source)
        .unwrap_or_else(|err| panic!("failed to write {}: {err:#}", path.display()));
    let written_text = String::from_utf8(written.clone()).expect("writer emitted non-UTF8 XML");
    assert!(
        written_text.contains("rs:basecatalog='AdoRecordsetSales'"),
        "writer omitted base catalog metadata:\n{written_text}"
    );
    assert!(
        written_text.contains("rs:baseschema='dbo'"),
        "writer omitted base schema metadata:\n{written_text}"
    );

    let reparsed = parse_ado_xml_bytes(&written)
        .unwrap_or_else(|err| panic!("failed to parse written XML {}: {err:#}", path.display()));
    assert_eq!(
        reparsed.fields[0].base_catalog.as_deref(),
        Some("AdoRecordsetSales")
    );
    assert_eq!(reparsed.fields[0].base_schema.as_deref(), Some("dbo"));
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

#[test]
fn rejects_nested_pending_chapter_changes_that_ado_xml_cannot_represent() {
    let path = corpus_path("corpus/shape").join("orders_lines_product_pending_shape.adtg");
    if !path.exists() {
        return;
    }

    let source = parse_adtg_bytes(&fs::read(&path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", path.display()));
    let err = write_ado_xml(&source).expect_err("nested pending chapter should be explicit");
    assert!(
        err.to_string().contains("nested chapter"),
        "unexpected nested pending chapter error: {err:#}"
    );
}

fn assert_xml_dir_roundtrips(relative: &str, include_roundtrip_files: bool) {
    let dir = corpus_path(relative);
    if !dir.exists() {
        return;
    }

    for path in xml_files(&dir, include_roundtrip_files) {
        assert_xml_file_roundtrips(&path);
    }
}

fn assert_xml_file_roundtrips(path: &Path) {
    let bytes =
        fs::read(path).unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    let source = parse_ado_xml_bytes(&bytes)
        .unwrap_or_else(|err| panic!("failed to parse source XML {}: {err:#}", path.display()));
    let written = write_ado_xml(&source)
        .unwrap_or_else(|err| panic!("failed to write source XML {}: {err:#}", path.display()));
    let reparsed = parse_ado_xml_bytes(&written)
        .unwrap_or_else(|err| panic!("failed to parse written XML {}: {err:#}", path.display()));
    assert_native_match(&source, &reparsed, path);
}

fn assert_native_match(
    source: &tablegram::Recordset,
    reparsed: &tablegram::Recordset,
    path: &Path,
) {
    let mismatches = compare_native_recordsets(source, reparsed);
    assert!(
        mismatches.is_empty(),
        "{} writer roundtrip mismatches:\n{}",
        path.display(),
        mismatches.join("\n")
    );
}

fn xml_files(dir: &Path, include_roundtrip_files: bool) -> Vec<PathBuf> {
    let mut paths = fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("xml"))
        .filter(|path| {
            include_roundtrip_files
                || !path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(".roundtrip.xml"))
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

fn adtg_files(dir: &Path) -> Vec<PathBuf> {
    let mut paths = fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("adtg"))
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

fn expected_shape_adtg_writer_rejection(path: &Path, message: &str) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if matches!(
        name,
        "orders_child_relation_key_update_shape.adtg"
            | "orders_composite_child_relation_key_update_shape.adtg"
            | "orders_lines_product_legacy_pending_shape.adtg"
            | "orders_lines_product_pending_shape.adtg"
    ) {
        return message.contains("pending row changes inside nested chapter");
    }

    matches!(
        name,
        "orders_calc_new_pending_shape.adtg"
            | "orders_composite_parent_relation_key_update_shape.adtg"
            | "orders_parent_relation_key_update_shape.adtg"
            | "orders_pending_changes_shape.adtg"
    ) && message.contains("pending root updates with chapter values")
}

fn corpus_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
