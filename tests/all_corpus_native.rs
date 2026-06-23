use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use tablegram::adtg::{
    adtg_descriptor_type_codes, parse_adtg_bytes, SUPPORTED_ADTG_DESCRIPTOR_TYPE_CODES,
    SUPPORTED_NATIVE_ADTG_ADO_TYPE_CODES,
};
use tablegram::corpus_policy::{
    documented_com_verification_skip_reason_name, documented_com_verification_skip_reason_path,
    documented_native_adtg_only_reason_name, documented_native_adtg_only_reason_path,
    is_documented_native_adtg_only_path, NativeAdtgOnlyReason,
    DOCUMENTED_COM_VERIFICATION_SKIP_ARTIFACTS, DOCUMENTED_NATIVE_ADTG_ONLY_ARTIFACTS,
};
use tablegram::model::{FieldAttribute, RecordStatusFlag, RowChangeKind, RowState, Value};
use tablegram::native_compare::compare_mdac_resaved_recordsets;
use tablegram::xml::parse_ado_xml_bytes;
use tablegram::Recordset;
use tablegram::{parse_recordset_bytes, parse_recordset_file};

#[test]
fn every_checked_corpus_xml_parses_natively() {
    let paths = corpus_files("xml");
    assert_eq!(
        paths.len(),
        expected_xml_corpus_count(),
        "checked XML corpus file count changed"
    );

    let mut failures = Vec::new();
    for path in paths {
        let bytes = fs::read(&path).unwrap_or_else(|err| {
            panic!("failed to read XML corpus file {}: {err:#}", path.display())
        });
        if let Err(err) = parse_ado_xml_bytes(&bytes) {
            failures.push(format!("{}: {err:#}", path.display()));
        }
    }

    assert!(
        failures.is_empty(),
        "failed to parse XML corpus files natively:\n{}",
        failures.join("\n")
    );
}

#[test]
fn every_checked_corpus_adtg_parses_natively() {
    let paths = corpus_files("adtg");
    assert_eq!(
        paths.len(),
        expected_adtg_corpus_count(),
        "checked ADTG corpus file count changed"
    );

    let mut failures = Vec::new();
    for path in paths {
        let bytes = fs::read(&path).unwrap_or_else(|err| {
            panic!(
                "failed to read ADTG corpus file {}: {err:#}",
                path.display()
            )
        });
        if let Err(err) = parse_adtg_bytes(&bytes) {
            failures.push(format!("{}: {err:#}", path.display()));
        }
    }

    assert!(
        failures.is_empty(),
        "failed to parse ADTG corpus files natively:\n{}",
        failures.join("\n")
    );
}

#[test]
fn every_advertised_native_adtg_ado_type_has_checked_corpus_coverage() {
    let paths = corpus_files("adtg");
    assert_eq!(
        paths.len(),
        expected_adtg_corpus_count(),
        "checked ADTG corpus file count changed"
    );

    let mut observed = BTreeSet::new();
    for path in paths {
        let bytes = fs::read(&path).unwrap_or_else(|err| {
            panic!(
                "failed to read ADTG corpus file {}: {err:#}",
                path.display()
            )
        });
        let recordset = parse_adtg_bytes(&bytes)
            .unwrap_or_else(|err| panic!("failed to parse ADTG {}: {err:#}", path.display()));
        collect_ado_type_codes(&recordset, &mut observed);
    }

    assert_eq!(
        observed,
        advertised_native_adtg_ado_type_codes(),
        "checked ADTG corpus should cover every advertised native ADTG ADO type"
    );
}

#[test]
fn focused_native_adtg_extension_types_stay_bounded_to_expected_corpus_families() {
    let paths = corpus_files("adtg");
    assert_eq!(
        paths.len(),
        expected_adtg_corpus_count(),
        "checked ADTG corpus file count changed"
    );

    let mut by_code = BTreeMap::<u16, Vec<String>>::new();
    for path in paths {
        let bytes = fs::read(&path).unwrap_or_else(|err| {
            panic!(
                "failed to read ADTG corpus file {}: {err:#}",
                path.display()
            )
        });
        let recordset = parse_adtg_bytes(&bytes)
            .unwrap_or_else(|err| panic!("failed to parse ADTG {}: {err:#}", path.display()));

        for code in [12, 136, 139] {
            if recordset_has_ado_type_code(&recordset, code) {
                by_code
                    .entry(code)
                    .or_default()
                    .push(relative_corpus_path(&path));
            }
        }
    }

    let mut expected_variant_paths = vec![
        "corpus/variant/variant_boolean.adtg",
        "corpus/variant/variant_byte.adtg",
        "corpus/variant/variant_currency.adtg",
        "corpus/variant/variant_date.adtg",
        "corpus/variant/variant_decimal.adtg",
        "corpus/variant/variant_double.adtg",
        "corpus/variant/variant_empty.adtg",
        "corpus/variant/variant_int64.adtg",
        "corpus/variant/variant_integer.adtg",
        "corpus/variant/variant_null.adtg",
        "corpus/variant/variant_sbyte.adtg",
        "corpus/variant/variant_single.adtg",
        "corpus/variant/variant_smallint.adtg",
        "corpus/variant/variant_uint16.adtg",
        "corpus/variant/variant_uint32.adtg",
        "corpus/variant/variant_uint64.adtg",
    ];
    if sqlserver_sales_corpus_present() {
        expected_variant_paths.insert(0, "corpus/sqlserver_sales/sql_variant_supported.adtg");
    }
    assert_eq!(
        by_code.get(&12).cloned().unwrap_or_default(),
        expected_variant_paths,
        "adVariant native coverage should stay in focused variant/sql_variant corpora"
    );

    let chapter_paths = by_code.get(&136).cloned().unwrap_or_default();
    assert_eq!(
        chapter_paths.len(),
        40,
        "adChapter native coverage should stay bounded to checked SHAPE artifacts"
    );
    assert!(
        chapter_paths
            .iter()
            .all(|path| path.starts_with("corpus/shape/")),
        "adChapter native coverage should only come from the shape corpus: {chapter_paths:?}"
    );

    assert_eq!(
        by_code.get(&139).cloned().unwrap_or_default(),
        vec!["corpus/fuzz/doc_number_varnumeric.adtg"],
        "adVarNumeric native coverage should stay tied to the XML-reader-created ADTG fixture"
    );
}

#[test]
fn every_checked_xml_ado_type_has_corpus_coverage() {
    let paths = corpus_files("xml");
    assert_eq!(
        paths.len(),
        expected_xml_corpus_count(),
        "checked XML corpus file count changed"
    );

    let mut observed = BTreeSet::new();
    for path in paths {
        let bytes = fs::read(&path).unwrap_or_else(|err| {
            panic!("failed to read XML corpus file {}: {err:#}", path.display())
        });
        let recordset = parse_ado_xml_bytes(&bytes)
            .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", path.display()));
        collect_ado_type_codes(&recordset, &mut observed);
    }

    assert_eq!(
        observed,
        advertised_native_adtg_ado_type_codes(),
        "checked XML corpus should cover every advertised native ADTG ADO type through MDAC XML and XML-reader fixtures"
    );
}

#[test]
fn every_supported_raw_adtg_descriptor_type_has_checked_corpus_coverage() {
    let paths = corpus_files("adtg");
    assert_eq!(
        paths.len(),
        expected_adtg_corpus_count(),
        "checked ADTG corpus file count changed"
    );

    let mut observed = BTreeSet::new();
    for path in paths {
        let bytes = fs::read(&path).unwrap_or_else(|err| {
            panic!(
                "failed to read ADTG corpus file {}: {err:#}",
                path.display()
            )
        });
        for type_code in adtg_descriptor_type_codes(&bytes).unwrap_or_else(|err| {
            panic!(
                "failed to collect ADTG descriptor type codes {}: {err:#}",
                path.display()
            )
        }) {
            observed.insert(type_code);
        }
    }

    let expected = SUPPORTED_ADTG_DESCRIPTOR_TYPE_CODES
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        observed, expected,
        "checked ADTG corpus should cover every supported raw ADTG descriptor type"
    );
}

#[test]
fn advertised_native_adtg_ado_types_are_accounted_for_by_raw_descriptors() {
    let raw_descriptor_codes = SUPPORTED_ADTG_DESCRIPTOR_TYPE_CODES
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let mut expected_ado_codes = raw_descriptor_codes.clone();
    expected_ado_codes.extend([
        200, // adVarChar is stored with the adChar descriptor.
        201, // adLongVarChar is stored with the adChar descriptor.
        202, // adVarWChar is stored with the adWChar descriptor.
        203, // adLongVarWChar is stored with the adWChar descriptor.
        204, // adVarBinary is stored with the adBinary descriptor.
        205, // adLongVarBinary is stored with the adBinary descriptor.
    ]);

    assert_eq!(
        advertised_native_adtg_ado_type_codes(),
        expected_ado_codes,
        "advertised native ADTG ADO types should be raw descriptor types plus MDAC variable/long text and binary materializations"
    );
}

#[test]
fn every_checked_corpus_artifact_parses_through_public_api() {
    let mut paths = corpus_files("xml");
    paths.extend(corpus_files("adtg"));
    paths.sort();
    assert_eq!(
        paths.len(),
        expected_recordset_corpus_count(),
        "checked XML/ADTG corpus file count changed"
    );

    let mut failures = Vec::new();
    for path in paths {
        let bytes = fs::read(&path).unwrap_or_else(|err| {
            panic!(
                "failed to read Recordset corpus file {}: {err:#}",
                path.display()
            )
        });
        if let Err(err) = parse_recordset_bytes(&bytes) {
            failures.push(format!("{}: {err:#}", path.display()));
        }
    }

    assert!(
        failures.is_empty(),
        "failed to parse corpus files through the public API:\n{}",
        failures.join("\n")
    );
}

#[test]
fn every_checked_corpus_artifact_parses_through_file_api() {
    let mut paths = corpus_files("xml");
    paths.extend(corpus_files("adtg"));
    paths.sort();
    assert_eq!(
        paths.len(),
        expected_recordset_corpus_count(),
        "checked XML/ADTG corpus file count changed"
    );

    let mut failures = Vec::new();
    for path in paths {
        if let Err(err) = parse_recordset_file(&path) {
            failures.push(format!("{}: {err:#}", path.display()));
        }
    }

    assert!(
        failures.is_empty(),
        "failed to parse corpus files through the public file API:\n{}",
        failures.join("\n")
    );
}

#[test]
fn every_checked_corpus_recordset_has_consistent_row_change_graph() {
    let mut paths = corpus_files("xml");
    paths.extend(corpus_files("adtg"));
    paths.sort();
    assert_eq!(
        paths.len(),
        expected_recordset_corpus_count(),
        "checked XML/ADTG corpus file count changed"
    );

    let mut coverage = RowSemanticCoverage::default();
    for path in paths {
        let bytes = fs::read(&path).unwrap_or_else(|err| {
            panic!(
                "failed to read Recordset corpus file {}: {err:#}",
                path.display()
            )
        });
        let recordset = parse_recordset_bytes(&bytes)
            .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", path.display()));
        assert_recordset_row_change_graph(&recordset, &path.display().to_string(), &mut coverage);
    }

    assert_eq!(
        coverage.change_kinds,
        names(["current", "delete", "insert", "update"]),
        "checked corpus row change-kind coverage"
    );
    assert_eq!(
        coverage.row_states,
        names(["current", "deleted", "inserted", "original", "updated"]),
        "checked corpus row-state coverage"
    );
    assert_eq!(
        coverage.status_flags,
        names(["deleted", "modified", "new", "unmodified"]),
        "checked corpus row status coverage"
    );
}

#[test]
fn every_checked_corpus_exercises_supported_field_metadata() {
    let mut paths = corpus_files("xml");
    paths.extend(corpus_files("adtg"));
    paths.sort();
    assert_eq!(
        paths.len(),
        expected_recordset_corpus_count(),
        "checked XML/ADTG corpus file count changed"
    );

    let mut coverage = FieldMetadataCoverage::default();
    for path in paths {
        let bytes = fs::read(&path).unwrap_or_else(|err| {
            panic!(
                "failed to read Recordset corpus file {}: {err:#}",
                path.display()
            )
        });
        let recordset = parse_recordset_bytes(&bytes)
            .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", path.display()));
        collect_field_metadata(&recordset, &mut coverage);
    }

    assert_eq!(
        coverage.attributes,
        names([
            "cache_deferred",
            "fixed",
            "is_chapter",
            "is_nullable",
            "long",
            "may_be_null",
            "may_defer",
            "negative_scale",
            "row_id",
            "row_version",
            "unknown_updatable",
            "updatable",
        ]),
        "checked corpus persisted field-attribute coverage"
    );
    if sqlserver_sales_corpus_present() {
        assert!(
            coverage.has_key_column,
            "checked corpus key-column metadata"
        );
    }
    assert!(
        coverage.has_base_column,
        "checked corpus source/base-column metadata"
    );
    assert!(
        coverage.has_base_catalog,
        "checked corpus source/base-catalog metadata"
    );
    assert!(
        coverage.has_base_schema,
        "checked corpus source/base-schema metadata"
    );
    assert!(
        coverage.has_base_table,
        "checked corpus source/base-table metadata"
    );
    if sqlserver_sales_corpus_present() {
        assert!(
            coverage.has_duplicate_visible_names,
            "checked corpus duplicate visible field-name metadata"
        );
    }
    assert!(coverage.has_nullable, "checked corpus nullable fields");
    assert!(
        coverage.has_fixed_length,
        "checked corpus fixed-length fields"
    );
    assert!(coverage.has_long, "checked corpus long fields");
    assert!(coverage.has_writable, "checked corpus writable fields");
}

#[test]
fn every_recordset_value_variant_has_checked_corpus_coverage() {
    let mut paths = corpus_files("xml");
    paths.extend(corpus_files("adtg"));
    paths.sort();
    assert_eq!(
        paths.len(),
        expected_recordset_corpus_count(),
        "checked XML/ADTG corpus file count changed"
    );

    let mut observed = BTreeSet::new();
    for path in paths {
        let bytes = fs::read(&path).unwrap_or_else(|err| {
            panic!(
                "failed to read Recordset corpus file {}: {err:#}",
                path.display()
            )
        });
        let recordset = parse_recordset_bytes(&bytes)
            .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", path.display()));
        collect_value_kinds(&recordset, &mut observed);
    }

    assert_eq!(
        observed,
        names([
            "binary_hex",
            "boolean",
            "chapter",
            "date",
            "date_time",
            "decimal",
            "empty",
            "float",
            "guid",
            "integer",
            "null",
            "string",
            "time",
            "unavailable",
            "unsigned_integer",
        ]),
        "checked corpus Recordset value-kind coverage"
    );
}

#[test]
fn xml_and_adtg_have_expected_format_specific_value_coverage() {
    let xml_paths = corpus_files("xml");
    let adtg_paths = corpus_files("adtg");
    assert_eq!(
        xml_paths.len(),
        expected_xml_corpus_count(),
        "checked XML corpus file count changed"
    );
    assert_eq!(
        adtg_paths.len(),
        expected_adtg_corpus_count(),
        "checked ADTG corpus file count changed"
    );

    let xml_kinds = value_kinds_for_paths(xml_paths);
    let adtg_kinds = value_kinds_for_paths(adtg_paths);

    assert_eq!(
        xml_kinds,
        names([
            "binary_hex",
            "boolean",
            "chapter",
            "date",
            "date_time",
            "decimal",
            "float",
            "guid",
            "integer",
            "null",
            "string",
            "time",
            "unavailable",
            "unsigned_integer",
        ]),
        "checked XML corpus value-kind coverage"
    );
    assert_eq!(
        adtg_kinds,
        names([
            "binary_hex",
            "boolean",
            "chapter",
            "date",
            "date_time",
            "decimal",
            "empty",
            "float",
            "guid",
            "integer",
            "null",
            "string",
            "time",
            "unavailable",
            "unsigned_integer",
        ]),
        "checked ADTG corpus value-kind coverage"
    );
}

fn collect_ado_type_codes(recordset: &Recordset, out: &mut BTreeSet<u16>) {
    for field in &recordset.fields {
        if let Some(ado_type) = field.ado_type {
            out.insert(ado_type.code);
        }
    }

    for row in &recordset.rows {
        for value in &row.values {
            if let tablegram::model::Value::Chapter(child) = value {
                collect_ado_type_codes(child, out);
            }
        }
    }
}

fn recordset_has_ado_type_code(recordset: &Recordset, code: u16) -> bool {
    recordset
        .fields
        .iter()
        .any(|field| field.ado_type.map(|ty| ty.code) == Some(code))
        || recordset.rows.iter().any(|row| {
            row.values.iter().any(|value| {
                matches!(value, Value::Chapter(child) if recordset_has_ado_type_code(child, code))
            })
        })
}

fn advertised_native_adtg_ado_type_codes() -> BTreeSet<u16> {
    SUPPORTED_NATIVE_ADTG_ADO_TYPE_CODES
        .iter()
        .copied()
        .collect()
}

fn relative_corpus_path(path: &Path) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn value_kinds_for_paths(paths: Vec<PathBuf>) -> BTreeSet<&'static str> {
    let mut observed = BTreeSet::new();
    for path in paths {
        let bytes = fs::read(&path).unwrap_or_else(|err| {
            panic!(
                "failed to read Recordset corpus file {}: {err:#}",
                path.display()
            )
        });
        let recordset = parse_recordset_bytes(&bytes)
            .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", path.display()));
        collect_value_kinds(&recordset, &mut observed);
    }
    observed
}

fn collect_value_kinds(recordset: &Recordset, out: &mut BTreeSet<&'static str>) {
    for row in &recordset.rows {
        for value in &row.values {
            out.insert(value_kind_name(value));
            if let Value::Chapter(child) = value {
                collect_value_kinds(child, out);
            }
        }
    }
}

fn value_kind_name(value: &Value) -> &'static str {
    match value {
        Value::Empty => "empty",
        Value::Null => "null",
        Value::Unavailable => "unavailable",
        Value::String(_) => "string",
        Value::Boolean(_) => "boolean",
        Value::Integer(_) => "integer",
        Value::UnsignedInteger(_) => "unsigned_integer",
        Value::Float(_) => "float",
        Value::Decimal(_) => "decimal",
        Value::Date(_) => "date",
        Value::Time(_) => "time",
        Value::DateTime(_) => "date_time",
        Value::Guid(_) => "guid",
        Value::BinaryHex(_) => "binary_hex",
        Value::Chapter(_) => "chapter",
    }
}

#[derive(Default)]
struct FieldMetadataCoverage {
    attributes: BTreeSet<&'static str>,
    has_base_catalog: bool,
    has_base_column: bool,
    has_base_schema: bool,
    has_base_table: bool,
    has_duplicate_visible_names: bool,
    has_fixed_length: bool,
    has_key_column: bool,
    has_long: bool,
    has_nullable: bool,
    has_writable: bool,
}

fn collect_field_metadata(recordset: &Recordset, coverage: &mut FieldMetadataCoverage) {
    let mut names_seen = BTreeSet::new();
    for field in &recordset.fields {
        if !names_seen.insert(field.name.as_str()) {
            coverage.has_duplicate_visible_names = true;
        }
        coverage.has_base_catalog |= field.base_catalog.is_some();
        coverage.has_base_column |= field.base_column.is_some();
        coverage.has_base_schema |= field.base_schema.is_some();
        coverage.has_base_table |= field.base_table.is_some();
        coverage.has_fixed_length |= field.fixed_length;
        coverage.has_key_column |= field.key_column;
        coverage.has_long |= field.long;
        coverage.has_nullable |= field.nullable;
        coverage.has_writable |= field.writable;
        for attribute in &field.attributes {
            coverage.attributes.insert(field_attribute_name(*attribute));
        }
    }

    for row in &recordset.rows {
        for value in &row.values {
            if let Value::Chapter(child) = value {
                collect_field_metadata(child, coverage);
            }
        }
    }
}

fn field_attribute_name(attribute: FieldAttribute) -> &'static str {
    match attribute {
        FieldAttribute::CacheDeferred => "cache_deferred",
        FieldAttribute::Fixed => "fixed",
        FieldAttribute::IsChapter => "is_chapter",
        FieldAttribute::IsCollection => "is_collection",
        FieldAttribute::IsDefaultStream => "is_default_stream",
        FieldAttribute::IsNullable => "is_nullable",
        FieldAttribute::IsRowUrl => "is_row_url",
        FieldAttribute::Long => "long",
        FieldAttribute::MayBeNull => "may_be_null",
        FieldAttribute::MayDefer => "may_defer",
        FieldAttribute::NegativeScale => "negative_scale",
        FieldAttribute::RowId => "row_id",
        FieldAttribute::RowVersion => "row_version",
        FieldAttribute::UnknownUpdatable => "unknown_updatable",
        FieldAttribute::Updatable => "updatable",
    }
}

#[derive(Default)]
struct RowSemanticCoverage {
    change_kinds: BTreeSet<&'static str>,
    row_states: BTreeSet<&'static str>,
    status_flags: BTreeSet<&'static str>,
}

fn assert_recordset_row_change_graph(
    recordset: &Recordset,
    label: &str,
    coverage: &mut RowSemanticCoverage,
) {
    for (row_index, row) in recordset.rows.iter().enumerate() {
        assert_eq!(row.ordinal, row_index, "{label}: row ordinal {row_index}");
        coverage.row_states.insert(row_state_name(row.state));
        let expected_status = expected_status_for_row_state(row.state);
        assert_eq!(
            row.status_flags,
            vec![expected_status],
            "{label}: row {row_index} status flags"
        );
        coverage
            .status_flags
            .insert(status_flag_name(expected_status));

        let change_index = row
            .change_index
            .unwrap_or_else(|| panic!("{label}: row {row_index} has no change index"));
        let change = recordset.changes.get(change_index).unwrap_or_else(|| {
            panic!("{label}: row {row_index} invalid change index {change_index}")
        });
        assert!(
            change.row_indices.contains(&row_index),
            "{label}: row {row_index} missing from change {change_index}"
        );
    }

    for (change_index, change) in recordset.changes.iter().enumerate() {
        coverage
            .change_kinds
            .insert(row_change_kind_name(change.kind));
        assert!(
            !change.row_indices.is_empty(),
            "{label}: change {change_index} has no rows"
        );
        let mut states = Vec::new();
        for row_index in &change.row_indices {
            let row = recordset.rows.get(*row_index).unwrap_or_else(|| {
                panic!("{label}: change {change_index} references invalid row {row_index}")
            });
            assert_eq!(
                row.change_index,
                Some(change_index),
                "{label}: row {row_index} back-reference for change {change_index}"
            );
            states.push(row.state);
        }
        assert_change_states(change.kind, &states, label, change_index);
    }

    for row in &recordset.rows {
        for value in &row.values {
            if let Value::Chapter(child) = value {
                assert_recordset_row_change_graph(child, label, coverage);
            }
        }
    }
}

fn assert_change_states(
    kind: RowChangeKind,
    states: &[RowState],
    label: &str,
    change_index: usize,
) {
    match kind {
        RowChangeKind::Current => assert!(
            states.iter().all(|state| *state == RowState::Current),
            "{label}: current change {change_index} states {states:?}"
        ),
        RowChangeKind::Insert => assert!(
            states.iter().all(|state| *state == RowState::Inserted),
            "{label}: insert change {change_index} states {states:?}"
        ),
        RowChangeKind::Delete => assert!(
            states.iter().all(|state| *state == RowState::Deleted),
            "{label}: delete change {change_index} states {states:?}"
        ),
        RowChangeKind::Update => assert_eq!(
            states,
            &[RowState::Original, RowState::Updated],
            "{label}: update change {change_index} states"
        ),
    }
}

fn expected_status_for_row_state(state: RowState) -> RecordStatusFlag {
    match state {
        RowState::Current => RecordStatusFlag::Unmodified,
        RowState::Original | RowState::Updated => RecordStatusFlag::Modified,
        RowState::Inserted => RecordStatusFlag::New,
        RowState::Deleted => RecordStatusFlag::Deleted,
    }
}

fn row_change_kind_name(kind: RowChangeKind) -> &'static str {
    match kind {
        RowChangeKind::Current => "current",
        RowChangeKind::Insert => "insert",
        RowChangeKind::Update => "update",
        RowChangeKind::Delete => "delete",
    }
}

fn row_state_name(state: RowState) -> &'static str {
    match state {
        RowState::Current => "current",
        RowState::Original => "original",
        RowState::Updated => "updated",
        RowState::Inserted => "inserted",
        RowState::Deleted => "deleted",
    }
}

fn status_flag_name(status: RecordStatusFlag) -> &'static str {
    match status {
        RecordStatusFlag::Ok => "ok",
        RecordStatusFlag::New => "new",
        RecordStatusFlag::Modified => "modified",
        RecordStatusFlag::Deleted => "deleted",
        RecordStatusFlag::Unmodified => "unmodified",
    }
}

fn names<const N: usize>(values: [&'static str; N]) -> BTreeSet<&'static str> {
    values.into_iter().collect()
}

#[test]
fn every_pairable_checked_adtg_matches_xml_materialized_views() {
    let paths = corpus_files("adtg");
    assert_eq!(
        paths.len(),
        expected_adtg_corpus_count(),
        "checked ADTG corpus file count changed"
    );

    let mut compared_pairs = 0usize;
    let mut adtg_only_artifacts = 0usize;
    let mut failures = Vec::new();
    for adtg_path in paths {
        if is_documented_native_adtg_only_path(&adtg_path) {
            adtg_only_artifacts += 1;
            continue;
        }

        let Some(xml_path) = comparison_xml_path(&adtg_path) else {
            failures.push(format!(
                "{}: missing matching XML comparison artifact",
                adtg_path.display()
            ));
            continue;
        };

        let xml = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap_or_else(|err| {
            panic!(
                "failed to read XML comparison file {}: {err:#}",
                xml_path.display()
            )
        }))
        .unwrap_or_else(|err| panic!("failed to parse XML {}: {err:#}", xml_path.display()));
        let adtg = parse_adtg_bytes(&fs::read(&adtg_path).unwrap_or_else(|err| {
            panic!(
                "failed to read ADTG corpus file {}: {err:#}",
                adtg_path.display()
            )
        }))
        .unwrap_or_else(|err| panic!("failed to parse ADTG {}: {err:#}", adtg_path.display()));

        let mismatches = compare_mdac_resaved_recordsets(&xml, &adtg);
        if mismatches.is_empty() {
            compared_pairs += 1;
        } else {
            failures.push(format!(
                "{} vs {}:\n{}",
                adtg_path.display(),
                xml_path.display(),
                mismatches.join("\n")
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "native ADTG/XML pair comparison failures:\n{}",
        failures.join("\n")
    );
    assert_eq!(
        compared_pairs,
        expected_adtg_xml_comparison_pair_count(),
        "native ADTG/XML comparison pair count"
    );
    assert_eq!(adtg_only_artifacts, 9, "native ADTG-only artifact count");
}

#[test]
fn every_checked_adtg_has_comparison_xml_or_documented_adtg_only_reason() {
    let paths = corpus_files("adtg");
    assert_eq!(
        paths.len(),
        expected_adtg_corpus_count(),
        "checked ADTG corpus file count changed"
    );

    let mut compared_pairs = 0usize;
    let mut adtg_only_artifacts = 0usize;
    let mut missing = Vec::new();
    for path in paths {
        if is_documented_native_adtg_only_path(&path) {
            adtg_only_artifacts += 1;
            continue;
        }

        if comparison_xml_path(&path).is_none() {
            missing.push(path.display().to_string());
            continue;
        }

        compared_pairs += 1;
    }

    assert!(
        missing.is_empty(),
        "ADTG corpus files missing comparison XML:\n{}",
        missing.join("\n")
    );
    assert_eq!(
        compared_pairs,
        expected_adtg_xml_comparison_pair_count(),
        "native ADTG/XML comparison pair count"
    );
    assert_eq!(adtg_only_artifacts, 9, "native ADTG-only artifact count");
}

#[test]
fn every_documented_native_adtg_only_artifact_is_bounded() {
    let mut artifacts = Vec::new();
    let mut reason_counts = BTreeMap::new();
    for path in corpus_files("adtg") {
        let Some(reason) = documented_native_adtg_only_reason_path(&path) else {
            continue;
        };
        artifacts.push(path.file_name().unwrap().to_str().unwrap().to_string());
        *reason_counts.entry(reason).or_insert(0usize) += 1;
    }
    artifacts.sort();

    let mut expected_artifacts = DOCUMENTED_NATIVE_ADTG_ONLY_ARTIFACTS
        .iter()
        .map(|(artifact, _reason)| (*artifact).to_string())
        .collect::<Vec<_>>();
    expected_artifacts.sort();
    let expected_reason_counts = reason_counts_from_policy(DOCUMENTED_NATIVE_ADTG_ONLY_ARTIFACTS);

    assert_eq!(
        artifacts, expected_artifacts,
        "documented native ADTG-only artifacts"
    );
    assert_eq!(
        reason_counts, expected_reason_counts,
        "documented native ADTG-only reasons"
    );
}

#[test]
fn documented_exception_policy_tables_are_self_consistent() {
    let checked_artifacts = checked_corpus_artifact_names();

    assert_policy_table_is_self_consistent(
        "native ADTG-only",
        DOCUMENTED_NATIVE_ADTG_ONLY_ARTIFACTS,
        documented_native_adtg_only_reason_name,
        &checked_artifacts,
    );
    assert_policy_table_is_self_consistent(
        "COM verification skip",
        &present_policy_entries(
            DOCUMENTED_COM_VERIFICATION_SKIP_ARTIFACTS,
            &checked_artifacts,
        ),
        documented_com_verification_skip_reason_name,
        &checked_artifacts,
    );
}

#[test]
fn every_documented_native_adtg_only_artifact_has_no_comparison_xml() {
    let mut shape_adtg_only_artifacts = 0usize;
    let mut failures = Vec::new();

    for path in corpus_files("adtg") {
        let Some(reason) = documented_native_adtg_only_reason_path(&path) else {
            continue;
        };

        let source_xml = same_stem_xml_path(&path);
        let roundtrip_xml = roundtrip_xml_path(&path);
        match reason {
            NativeAdtgOnlyReason::ShapeAdtgOnlyPendingChange => {
                shape_adtg_only_artifacts += 1;
                if source_xml.exists() || roundtrip_xml.exists() {
                    failures.push(format!(
                        "{}: SHAPE pending-change artifact should stay ADTG-only",
                        path.display()
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "documented native ADTG-only artifact-shape failures:\n{}",
        failures.join("\n")
    );
    assert_eq!(
        shape_adtg_only_artifacts, 9,
        "SHAPE ADTG-only pending-change artifacts"
    );
}

#[test]
fn every_documented_com_verification_skip_is_bounded() {
    let mut paths = corpus_files("xml");
    paths.extend(corpus_files("adtg"));

    let mut skips = Vec::new();
    let mut reason_counts = BTreeMap::new();
    for path in paths {
        let Some(reason) = documented_com_verification_skip_reason_path(&path) else {
            continue;
        };
        skips.push(path.file_name().unwrap().to_str().unwrap().to_string());
        *reason_counts.entry(reason).or_insert(0usize) += 1;
    }
    skips.sort();

    let present_policy = present_policy_entries(
        DOCUMENTED_COM_VERIFICATION_SKIP_ARTIFACTS,
        &checked_corpus_artifact_names(),
    );
    let mut expected_skips = present_policy
        .iter()
        .map(|(artifact, _reason)| (*artifact).to_string())
        .collect::<Vec<_>>();
    expected_skips.sort();
    let expected_reason_counts = reason_counts_from_policy(&present_policy);

    assert_eq!(skips, expected_skips, "documented COM verification skips");
    assert_eq!(
        reason_counts, expected_reason_counts,
        "documented COM verification skip reasons"
    );
}

fn reason_counts_from_policy<R: Copy + Ord>(policy: &[(&str, R)]) -> BTreeMap<R, usize> {
    let mut counts = BTreeMap::new();
    for (_artifact, reason) in policy {
        *counts.entry(*reason).or_insert(0usize) += 1;
    }
    counts
}

fn present_policy_entries<R: Copy>(
    policy: &[(&'static str, R)],
    checked_artifacts: &BTreeSet<String>,
) -> Vec<(&'static str, R)> {
    policy
        .iter()
        .copied()
        .filter(|(artifact, _reason)| checked_artifacts.contains(*artifact))
        .collect()
}

fn expected_xml_corpus_count() -> usize {
    if sqlserver_sales_corpus_present() {
        686
    } else {
        662
    }
}

fn expected_adtg_corpus_count() -> usize {
    if sqlserver_sales_corpus_present() {
        354
    } else {
        342
    }
}

fn expected_recordset_corpus_count() -> usize {
    expected_xml_corpus_count() + expected_adtg_corpus_count()
}

fn expected_adtg_xml_comparison_pair_count() -> usize {
    if sqlserver_sales_corpus_present() {
        345
    } else {
        333
    }
}

fn sqlserver_sales_corpus_present() -> bool {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("corpus/sqlserver_sales")
        .exists()
}

fn checked_corpus_artifact_names() -> BTreeSet<String> {
    let mut paths = corpus_files("xml");
    paths.extend(corpus_files("adtg"));
    paths
        .into_iter()
        .map(|path| path.file_name().unwrap().to_str().unwrap().to_string())
        .collect()
}

fn assert_policy_table_is_self_consistent<R>(
    label: &str,
    policy: &[(&str, R)],
    lookup: fn(&str) -> Option<R>,
    checked_artifacts: &BTreeSet<String>,
) where
    R: Copy + Ord + std::fmt::Debug,
{
    let mut names = BTreeSet::new();
    for (artifact, reason) in policy {
        assert!(!artifact.is_empty(), "{label} policy has an empty artifact");
        assert_eq!(
            artifact.trim(),
            *artifact,
            "{label} policy artifact has surrounding whitespace: {artifact:?}"
        );
        assert!(
            names.insert(*artifact),
            "{label} policy has duplicate artifact {artifact}"
        );
        assert_eq!(
            lookup(artifact),
            Some(*reason),
            "{label} policy lookup disagrees for {artifact}"
        );
        assert!(
            checked_artifacts.contains(*artifact),
            "{label} policy artifact is not present in the checked corpus: {artifact}"
        );
    }
}

fn corpus_files(extension: &str) -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus");
    if !root.exists() {
        return Vec::new();
    }

    let mut out = Vec::new();
    collect_files_with_extension(&root, extension, &mut out);
    out.sort();
    out
}

fn comparison_xml_path(adtg_path: &Path) -> Option<PathBuf> {
    let roundtrip = roundtrip_xml_path(adtg_path);
    if roundtrip.exists() {
        return Some(roundtrip);
    }

    let same_stem = same_stem_xml_path(adtg_path);
    same_stem.exists().then_some(same_stem)
}

fn same_stem_xml_path(adtg_path: &Path) -> PathBuf {
    adtg_path.with_extension("xml")
}

fn roundtrip_xml_path(adtg_path: &Path) -> PathBuf {
    let file_stem = adtg_path
        .file_stem()
        .and_then(|value| value.to_str())
        .expect("ADTG corpus path should have a UTF-8 file stem");
    adtg_path.with_file_name(format!("{file_stem}.roundtrip.xml"))
}

fn collect_files_with_extension(dir: &Path, extension: &str, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("failed to read corpus directory {}: {err:#}", dir.display()))
    {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_files_with_extension(&path, extension, out);
        } else if path.extension().and_then(|value| value.to_str()) == Some(extension) {
            out.push(path);
        }
    }
}
