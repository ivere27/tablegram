use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use tablegram::adtg::parse_adtg_bytes;
use tablegram::compat::{materialize_default_view, materialize_pending_view, MaterializedRow};
use tablegram::model::{Field, FieldAttribute, RecordStatusFlag, Recordset, Value};
use tablegram::native_compare::compare_native_recordsets;
use tablegram::xml::parse_ado_xml_bytes;

const SQLSERVER_MANIFEST_ROWS: usize = 12;

const SQL_VARIANT_FIELDS: [&str; 8] = [
    "VAR_INT",
    "VAR_BIGINT",
    "VAR_DECIMAL",
    "VAR_MONEY",
    "VAR_FLOAT",
    "VAR_REAL",
    "VAR_BIT",
    "VAR_DATETIME",
];

fn sqlserver_sales_corpus_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/sqlserver_sales");
    assert!(
        dir.exists(),
        "SQL Server corpus is missing; generate it with tools/make_sales_sqlserver_corpus.vbs"
    );
    dir
}

#[test]
fn sqlserver_sales_source_assets_keep_required_mixed_join_volume() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let generator = fs::read_to_string(root.join("tools/make_sales_sqlserver_corpus.vbs")).unwrap();
    let shape_generator = fs::read_to_string(root.join("tools/make_shape_corpus.vbs")).unwrap();
    let shape_probe = fs::read_to_string(root.join("tools/probe_shape_query.vbs")).unwrap();
    let seed = fs::read_to_string(root.join("tools/sales_sqlserver_seed.sql")).unwrap();
    let join = fs::read_to_string(root.join("tools/sales_sqlserver_join.sql")).unwrap();

    for required_default in [
        r#"server = ArgText(0, "SERVER")"#,
        r#"userName = ArgText(1, "USER")"#,
        r#"password = ArgText(2, "<password>")"#,
        r#"databaseName = ArgText(3, "AdoRecordsetSales")"#,
    ] {
        for (label, script) in [
            ("sales corpus generator", generator.as_str()),
            ("shape corpus generator", shape_generator.as_str()),
            ("shape query probe", shape_probe.as_str()),
        ] {
            assert!(
                script.contains(required_default),
                "{label} should keep placeholder SQL Server default {required_default:?}"
            );
        }
    }

    for (label, script) in [
        ("sales corpus generator", generator.as_str()),
        ("shape corpus generator", shape_generator.as_str()),
        ("shape query probe", shape_probe.as_str()),
    ] {
        let forbidden_fragments = [
            format!("{}.", ["192", "168"].join(".")),
            ["sa", "password"].join(""),
            format!(r#"ArgText(1, "{}")"#, "sa"),
        ];
        for forbidden in &forbidden_fragments {
            assert!(
                !script.contains(forbidden.as_str()),
                "{label} should not contain lab credential fragment {forbidden:?}"
            );
        }
    }

    let seed_tables = [
        "SalesRegions",
        "SalesCustomers",
        "SalesEmployees",
        "SalesCategories",
        "SalesProducts",
        "SalesOrders",
        "SalesOrderLines",
        "SalesPayments",
        "SalesShipments",
        "SalesLegacyDocs",
    ];
    assert_eq!(seed_tables.len(), 10, "sales seed table count");
    for table in seed_tables {
        assert!(
            seed.contains(&format!("CREATE TABLE dbo.{table}")),
            "seed SQL should create {table}"
        );
    }

    let joined_tables = [
        "SalesOrders",
        "SalesCustomers",
        "SalesRegions",
        "SalesEmployees",
        "SalesOrderLines",
        "SalesProducts",
        "SalesCategories",
        "SalesPayments",
        "SalesShipments",
    ];
    assert_eq!(
        joined_tables.len(),
        9,
        "sales_mixed_join joined table count"
    );
    for table in joined_tables {
        assert!(
            join.contains(&format!("dbo.{table}")),
            "join SQL should reference {table}"
        );
    }

    assert!(
        seed.contains("WHILE @i <= 240") && seed.contains("WHILE @lineNo <= 3"),
        "sales seed should keep 240 orders with 3 lines each"
    );
    assert!(
        generator
            .contains(r#"Csv(Array("sales_mixed_join", "sqlserver_join_mixed_types", "9", "720""#),
        "sales manifest writer should keep the 9-table, 720-row mixed join contract"
    );

    for required_fragment in [
        "WHILE @i <= 48",
        "CAST(o.OrderDate AS date)",
        "CAST(o.OrderDate AS time(0))",
        "CAST(o.OrderDate AS datetime2(3))",
        "CAST(ol.LineId * 100000 AS bigint)",
        "CAST(ol.LineNumber AS smallint)",
        "numeric(18,4)",
        "uniqueidentifier",
        "varbinary(max)",
        "nvarchar(max)",
        "smallmoney",
        "rowversion",
    ] {
        assert!(
            seed.contains(required_fragment) || join.contains(required_fragment),
            "SQL Server sales assets should keep mixed-data fragment {required_fragment:?}"
        );
    }
}

#[test]
#[ignore = "requires SQL Server corpus under corpus/sqlserver_sales"]
fn sqlserver_sales_manifest_matches_checked_artifacts_when_present() {
    let dir = sqlserver_sales_corpus_dir();

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    assert_eq!(
        rows.len(),
        SQLSERVER_MANIFEST_ROWS,
        "SQL Server sales manifest row count"
    );

    let mut manifest_artifacts = BTreeSet::new();
    let mut missing = Vec::new();
    for row in rows {
        assert_eq!(row.len(), 7, "SQL Server sales manifest columns: {row:?}");
        for artifact in &row[4..7] {
            if artifact.is_empty() {
                continue;
            }
            let path = manifest_artifact_path(artifact);
            if !path.exists() {
                missing.push(artifact.clone());
            }
            manifest_artifacts.insert(
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_else(|| panic!("invalid manifest path {artifact}"))
                    .to_string(),
            );
        }
    }
    assert!(
        missing.is_empty(),
        "SQL Server sales manifest references missing artifacts: {missing:?}"
    );

    let actual_artifacts = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            matches!(
                path.extension().and_then(|value| value.to_str()),
                Some("adtg" | "xml")
            )
        })
        .map(|path| path.file_name().unwrap().to_str().unwrap().to_string())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual_artifacts, manifest_artifacts,
        "SQL Server sales manifest artifact list"
    );

    assert_eq!(
        read_csv_rows(&dir.join("sql_variant_failures.csv")).len(),
        4,
        "SQL Server sql_variant failure row count"
    );
}

#[test]
#[ignore = "requires SQL Server corpus under corpus/sqlserver_sales"]
fn sqlserver_sales_join_corpus_parses_and_matches_when_present() {
    let dir = sqlserver_sales_corpus_dir();

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    assert_eq!(
        rows.len(),
        SQLSERVER_MANIFEST_ROWS,
        "SQL Server sales manifest row count"
    );
    let manifest = manifest_row(&rows, "sales_mixed_join");
    assert_eq!(manifest[0], "sales_mixed_join");
    assert_eq!(manifest[1], "sqlserver_join_mixed_types");
    assert_eq!(manifest[2], "9", "joined table count");
    assert_eq!(manifest[3], "720", "joined row count");
    for artifact in &manifest[4..7] {
        assert!(Path::new(artifact).exists(), "missing artifact {artifact}");
    }

    let xml_path = dir.join("sales_mixed_join.xml");
    let adtg_path = dir.join("sales_mixed_join.adtg");
    let roundtrip_path = dir.join("sales_mixed_join.roundtrip.xml");

    let xml = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", xml_path.display()));
    let adtg = parse_adtg_bytes(&fs::read(&adtg_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", adtg_path.display()));
    let roundtrip = parse_ado_xml_bytes(&fs::read(&roundtrip_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", roundtrip_path.display()));

    assert_sqlserver_sales_shape(&xml, "source XML");
    assert_sqlserver_sales_shape(&adtg, "ADTG");
    assert_sqlserver_sales_shape(&roundtrip, "ADTG roundtrip XML");
    assert_sqlserver_sales_base_tables(&xml, "source XML");
    assert_sqlserver_sales_base_columns(&adtg, "ADTG");
    assert_recordset_values_match(&adtg, &roundtrip, "ADTG vs roundtrip XML");
    assert_recordset_values_match(&adtg, &xml, "ADTG vs source XML");
}

#[test]
#[ignore = "requires SQL Server corpus under corpus/sqlserver_sales"]
fn sqlserver_provider_pending_changes_parse_and_match_when_present() {
    let dir = sqlserver_sales_corpus_dir();

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    assert_eq!(
        rows.len(),
        SQLSERVER_MANIFEST_ROWS,
        "SQL Server sales manifest row count"
    );
    let manifest = manifest_row(&rows, "sales_customers_pending");
    assert_eq!(manifest[1], "sqlserver_provider_pending_changes");
    assert_eq!(manifest[2], "1", "table count");
    assert_eq!(manifest[3], "6", "default view row count");
    for artifact in &manifest[4..7] {
        assert!(Path::new(artifact).exists(), "missing artifact {artifact}");
    }

    let xml_path = dir.join("sales_customers_pending.xml");
    let adtg_path = dir.join("sales_customers_pending.adtg");
    let roundtrip_path = dir.join("sales_customers_pending.roundtrip.xml");

    let xml = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", xml_path.display()));
    let adtg = parse_adtg_bytes(&fs::read(&adtg_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", adtg_path.display()));
    let roundtrip = parse_ado_xml_bytes(&fs::read(&roundtrip_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", roundtrip_path.display()));

    assert_sqlserver_pending_shape(&xml, "source XML");
    assert_sqlserver_pending_shape(&adtg, "ADTG");
    assert_sqlserver_pending_shape(&roundtrip, "ADTG roundtrip XML");
    assert_sqlserver_pending_base_columns_and_key(&adtg, "ADTG");
    assert_recordset_values_match(&adtg, &roundtrip, "ADTG vs roundtrip XML");
    assert_recordset_values_match(&adtg, &xml, "ADTG vs source XML");
}

#[test]
#[ignore = "requires SQL Server corpus under corpus/sqlserver_sales"]
fn sqlserver_provider_marshal_modified_only_persists_reduced_adtg_when_present() {
    let dir = sqlserver_sales_corpus_dir();

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    assert_eq!(
        rows.len(),
        SQLSERVER_MANIFEST_ROWS,
        "SQL Server sales manifest row count"
    );
    let manifest = manifest_row(&rows, "sales_customers_marshal_modified");
    assert_eq!(manifest[1], "sqlserver_provider_marshal_modified_only");
    assert_eq!(manifest[2], "1", "table count");
    assert_eq!(manifest[3], "2", "ADTG default view row count");
    for artifact in &manifest[4..7] {
        assert!(Path::new(artifact).exists(), "missing artifact {artifact}");
    }

    let xml_path = dir.join("sales_customers_marshal_modified.xml");
    let adtg_path = dir.join("sales_customers_marshal_modified.adtg");
    let roundtrip_path = dir.join("sales_customers_marshal_modified.roundtrip.xml");

    let xml = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", xml_path.display()));
    let adtg = parse_adtg_bytes(&fs::read(&adtg_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", adtg_path.display()));
    let roundtrip = parse_ado_xml_bytes(&fs::read(&roundtrip_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", roundtrip_path.display()));

    assert_eq!(field_names(&xml), sqlserver_pending_field_names());
    assert_eq!(field_names(&adtg), sqlserver_pending_field_names());
    assert_eq!(field_names(&roundtrip), sqlserver_pending_field_names());
    assert_sqlserver_pending_base_columns_and_key(&adtg, "MarshalOptions ADTG");
    assert_recordset_values_match(&adtg, &roundtrip, "MarshalOptions ADTG vs roundtrip XML");

    let xml_default = materialize_default_view(&xml);
    let xml_pending = materialize_pending_view(&xml);
    assert_eq!(
        xml_default.rows.len(),
        6,
        "source XML keeps the full default view even with adMarshalModifiedOnly"
    );
    assert_eq!(xml_pending.rows.len(), 3, "source XML pending rows");
    assert!(
        xml_default
            .rows
            .iter()
            .any(|row| row.status == RecordStatusFlag::Unmodified
                && row.values.first() == Some(&Value::Integer(3))),
        "source XML should retain unmodified provider rows"
    );

    let adtg_default = materialize_default_view(&adtg);
    let adtg_pending = materialize_pending_view(&adtg);
    assert_eq!(
        adtg_default.rows.len(),
        2,
        "modified-only ADTG default rows"
    );
    assert_eq!(
        adtg_pending.rows.len(),
        3,
        "modified-only ADTG pending rows"
    );
    assert!(
        !adtg_default
            .rows
            .iter()
            .any(|row| row.status == RecordStatusFlag::Unmodified),
        "modified-only ADTG should omit unmodified rows"
    );

    assert_materialized_row(
        adtg_default
            .rows
            .iter()
            .find(|row| row.values.first() == Some(&Value::Integer(1)))
            .unwrap_or_else(|| panic!("MarshalOptions ADTG missing modified customer row")),
        RecordStatusFlag::Modified,
        &[
            Value::Integer(1),
            Value::UnsignedInteger(1),
            string_value("CUST0001"),
            string_value("고객 1 / Customer 1"),
            string_value("marshal modified-only provider note & <xml>"),
            decimal("22222.22"),
            Value::DateTime("2019-01-10T00:00:00".to_string()),
            Value::Boolean(false),
            Value::Guid("{00000001-1111-2222-3333-000000000001}".to_string()),
            binary_hex("3A11F2B6C86410C929233AFA11655811"),
        ],
        "MarshalOptions modified customer",
    );
    assert_materialized_row(
        adtg_default
            .rows
            .iter()
            .find(|row| row.values.first() == Some(&Value::Integer(900002)))
            .unwrap_or_else(|| panic!("MarshalOptions ADTG missing inserted customer row")),
        RecordStatusFlag::New,
        &[
            Value::Integer(900002),
            Value::UnsignedInteger(1),
            string_value("MOPT0002"),
            string_value("Marshal Modified Inserted"),
            string_value("marshal inserted note"),
            decimal("777.77"),
            Value::DateTime("2026-06-01T01:02:03".to_string()),
            Value::Boolean(true),
            Value::Guid("{00090002-1111-2222-3333-000000900002}".to_string()),
            binary_hex("100F0E0D0C0B0A090807060504030201"),
        ],
        "MarshalOptions inserted customer",
    );
    let deleted = adtg_pending
        .rows
        .iter()
        .find(|row| row.status == RecordStatusFlag::Deleted)
        .unwrap_or_else(|| panic!("MarshalOptions ADTG missing deleted customer row"));
    assert_eq!(
        deleted.values.first(),
        Some(&Value::Integer(2)),
        "MarshalOptions deleted customer id"
    );
}

#[test]
#[ignore = "requires SQL Server corpus under corpus/sqlserver_sales"]
fn sqlserver_provider_sorted_view_preserves_materialized_order_when_present() {
    let dir = sqlserver_sales_corpus_dir();

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    assert_eq!(
        rows.len(),
        SQLSERVER_MANIFEST_ROWS,
        "SQL Server sales manifest row count"
    );
    let manifest = manifest_row(&rows, "sales_customers_sorted");
    assert_eq!(manifest[1], "sqlserver_provider_sorted_view");
    assert_eq!(manifest[2], "1", "table count");
    assert_eq!(manifest[3], "8", "sorted default row count");
    for artifact in &manifest[4..7] {
        assert!(Path::new(artifact).exists(), "missing artifact {artifact}");
    }

    let xml_path = dir.join("sales_customers_sorted.xml");
    let adtg_path = dir.join("sales_customers_sorted.adtg");
    let roundtrip_path = dir.join("sales_customers_sorted.roundtrip.xml");

    let xml = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", xml_path.display()));
    let adtg = parse_adtg_bytes(&fs::read(&adtg_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", adtg_path.display()));
    let roundtrip = parse_ado_xml_bytes(&fs::read(&roundtrip_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", roundtrip_path.display()));

    assert_sqlserver_sorted_shape(&xml, "source XML");
    assert_sqlserver_sorted_shape(&adtg, "ADTG");
    assert_sqlserver_sorted_shape(&roundtrip, "ADTG roundtrip XML");
    assert_recordset_values_match(&adtg, &roundtrip, "sorted ADTG vs roundtrip XML");
    assert_recordset_values_match(&adtg, &xml, "sorted ADTG vs source XML");
}

#[test]
#[ignore = "requires SQL Server corpus under corpus/sqlserver_sales"]
fn sqlserver_provider_filtered_view_persists_criteria_view_when_present() {
    let dir = sqlserver_sales_corpus_dir();

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    assert_eq!(
        rows.len(),
        SQLSERVER_MANIFEST_ROWS,
        "SQL Server sales manifest row count"
    );
    let manifest = manifest_row(&rows, "sales_customers_filtered");
    assert_eq!(manifest[1], "sqlserver_provider_filtered_view");
    assert_eq!(manifest[2], "1", "table count");
    assert_eq!(manifest[3], "5", "filtered default row count");
    for artifact in &manifest[4..7] {
        assert!(Path::new(artifact).exists(), "missing artifact {artifact}");
    }

    let xml_path = dir.join("sales_customers_filtered.xml");
    let adtg_path = dir.join("sales_customers_filtered.adtg");
    let roundtrip_path = dir.join("sales_customers_filtered.roundtrip.xml");

    let xml = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", xml_path.display()));
    let adtg = parse_adtg_bytes(&fs::read(&adtg_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", adtg_path.display()));
    let roundtrip = parse_ado_xml_bytes(&fs::read(&roundtrip_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", roundtrip_path.display()));

    assert_sqlserver_filtered_shape(&xml, "source XML");
    assert_sqlserver_filtered_shape(&adtg, "ADTG");
    assert_sqlserver_filtered_shape(&roundtrip, "ADTG roundtrip XML");
    assert_recordset_values_match(&adtg, &roundtrip, "filtered ADTG vs roundtrip XML");
    assert_recordset_values_match(&adtg, &xml, "filtered ADTG vs source XML");
}

#[test]
#[ignore = "requires SQL Server corpus under corpus/sqlserver_sales"]
fn sqlserver_server_cursor_optimistic_extended_descriptors_parse_when_present() {
    let dir = sqlserver_sales_corpus_dir();

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    assert_eq!(
        rows.len(),
        SQLSERVER_MANIFEST_ROWS,
        "SQL Server sales manifest row count"
    );
    let manifest = manifest_row(&rows, "sales_customers_server_static_optimistic");
    assert_eq!(
        manifest[1],
        "sqlserver_server_cursor_optimistic_extended_descriptors"
    );
    assert_eq!(manifest[2], "1", "table count");
    assert_eq!(manifest[3], "6", "server cursor row count");
    for artifact in &manifest[4..7] {
        assert!(Path::new(artifact).exists(), "missing artifact {artifact}");
    }

    let xml_path = dir.join("sales_customers_server_static_optimistic.xml");
    let adtg_path = dir.join("sales_customers_server_static_optimistic.adtg");
    let roundtrip_path = dir.join("sales_customers_server_static_optimistic.roundtrip.xml");

    let xml = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", xml_path.display()));
    let adtg = parse_adtg_bytes(&fs::read(&adtg_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", adtg_path.display()));
    let roundtrip = parse_ado_xml_bytes(&fs::read(&roundtrip_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", roundtrip_path.display()));

    assert_sqlserver_server_cursor_shape(&xml, "source XML");
    assert_sqlserver_server_cursor_shape(&adtg, "ADTG");
    assert_sqlserver_server_cursor_shape(&roundtrip, "ADTG roundtrip XML");
    assert!(
        fields_by_name(&adtg)
            .get("CustomerId")
            .unwrap_or_else(|| panic!("ADTG missing CustomerId field"))
            .key_column,
        "server-side optimistic cursor ADTG should preserve CustomerId key-column flag"
    );
    assert_recordset_values_match(&adtg, &roundtrip, "server cursor ADTG vs roundtrip XML");
    assert_recordset_values_match(&adtg, &xml, "server cursor ADTG vs source XML");
}

#[test]
#[ignore = "requires SQL Server corpus under corpus/sqlserver_sales"]
fn sqlserver_server_cursor_keyset_readonly_key_descriptor_parse_when_present() {
    let dir = sqlserver_sales_corpus_dir();

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    assert_eq!(
        rows.len(),
        SQLSERVER_MANIFEST_ROWS,
        "SQL Server sales manifest row count"
    );
    let manifest = manifest_row(&rows, "sales_customers_server_keyset_readonly");
    assert_eq!(
        manifest[1],
        "sqlserver_server_cursor_keyset_readonly_key_descriptor"
    );
    assert_eq!(manifest[2], "1", "table count");
    assert_eq!(manifest[3], "6", "server cursor keyset row count");
    for artifact in &manifest[4..7] {
        assert!(Path::new(artifact).exists(), "missing artifact {artifact}");
    }

    let xml_path = dir.join("sales_customers_server_keyset_readonly.xml");
    let adtg_path = dir.join("sales_customers_server_keyset_readonly.adtg");
    let roundtrip_path = dir.join("sales_customers_server_keyset_readonly.roundtrip.xml");

    let xml = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", xml_path.display()));
    let adtg = parse_adtg_bytes(&fs::read(&adtg_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", adtg_path.display()));
    let roundtrip = parse_ado_xml_bytes(&fs::read(&roundtrip_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", roundtrip_path.display()));

    assert_sqlserver_server_cursor_keyset_shape(&xml, "source XML");
    assert_sqlserver_server_cursor_keyset_shape(&adtg, "ADTG");
    assert_sqlserver_server_cursor_keyset_shape(&roundtrip, "ADTG roundtrip XML");
    assert!(
        fields_by_name(&adtg)
            .get("CustomerId")
            .unwrap_or_else(|| panic!("ADTG missing CustomerId field"))
            .key_column,
        "server-side keyset read-only cursor ADTG should preserve CustomerId key-column flag"
    );
    assert!(
        !fields_by_name(&xml)
            .get("CustomerId")
            .unwrap_or_else(|| panic!("source XML missing CustomerId field"))
            .key_column,
        "source XML should not expose the key-column flag for this cursor mode"
    );
    assert_recordset_values_match(&adtg, &roundtrip, "keyset ADTG vs roundtrip XML");
    assert_recordset_values_match(&adtg, &xml, "keyset ADTG vs source XML");
}

#[test]
#[ignore = "requires SQL Server corpus under corpus/sqlserver_sales"]
fn sqlserver_server_cursor_forwardonly_readonly_unknown_row_count_parse_when_present() {
    let dir = sqlserver_sales_corpus_dir();

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    assert_eq!(
        rows.len(),
        SQLSERVER_MANIFEST_ROWS,
        "SQL Server sales manifest row count"
    );
    let manifest = manifest_row(&rows, "sales_customers_server_forwardonly_readonly");
    assert_eq!(
        manifest[1],
        "sqlserver_server_cursor_forwardonly_readonly_unknown_row_count"
    );
    assert_eq!(manifest[2], "1", "table count");
    assert_eq!(manifest[3], "5", "server forward-only row count");
    for artifact in &manifest[4..7] {
        assert!(Path::new(artifact).exists(), "missing artifact {artifact}");
    }

    let xml_path = dir.join("sales_customers_server_forwardonly_readonly.xml");
    let adtg_path = dir.join("sales_customers_server_forwardonly_readonly.adtg");
    let roundtrip_path = dir.join("sales_customers_server_forwardonly_readonly.roundtrip.xml");

    let xml = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", xml_path.display()));
    let adtg_bytes = fs::read(&adtg_path).unwrap();
    let adtg = parse_adtg_bytes(&adtg_bytes)
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", adtg_path.display()));
    let roundtrip = parse_ado_xml_bytes(&fs::read(&roundtrip_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", roundtrip_path.display()));

    assert_sqlserver_server_readonly_cursor_shape(&xml, "source XML");
    assert_sqlserver_server_readonly_cursor_shape(&adtg, "ADTG");
    assert_sqlserver_server_readonly_cursor_shape(&roundtrip, "ADTG roundtrip XML");
    assert_eq!(
        adtg_bytes.get(0x45..0x49),
        Some([0xff, 0xff, 0xff, 0xff].as_slice()),
        "forward-only server cursor ADTG should persist unknown row count"
    );
    assert_recordset_values_match(&adtg, &roundtrip, "forward-only ADTG vs roundtrip XML");
    assert_recordset_values_match(&adtg, &xml, "forward-only ADTG vs source XML");
}

#[test]
#[ignore = "requires SQL Server corpus under corpus/sqlserver_sales"]
fn sqlserver_server_cursor_dynamic_readonly_distinct_descriptor_bytes_parse_when_present() {
    let dir = sqlserver_sales_corpus_dir();

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    assert_eq!(
        rows.len(),
        SQLSERVER_MANIFEST_ROWS,
        "SQL Server sales manifest row count"
    );
    let manifest = manifest_row(&rows, "sales_customers_server_dynamic_readonly");
    assert_eq!(
        manifest[1],
        "sqlserver_server_cursor_dynamic_readonly_distinct_descriptor_bytes"
    );
    assert_eq!(manifest[2], "1", "table count");
    assert_eq!(manifest[3], "5", "server dynamic read-only row count");
    for artifact in &manifest[4..7] {
        assert!(Path::new(artifact).exists(), "missing artifact {artifact}");
    }

    let xml_path = dir.join("sales_customers_server_dynamic_readonly.xml");
    let adtg_path = dir.join("sales_customers_server_dynamic_readonly.adtg");
    let roundtrip_path = dir.join("sales_customers_server_dynamic_readonly.roundtrip.xml");

    let xml = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", xml_path.display()));
    let adtg_bytes = fs::read(&adtg_path).unwrap();
    let adtg = parse_adtg_bytes(&adtg_bytes)
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", adtg_path.display()));
    let roundtrip = parse_ado_xml_bytes(&fs::read(&roundtrip_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", roundtrip_path.display()));

    assert_sqlserver_server_readonly_cursor_shape(&xml, "source XML");
    assert_sqlserver_server_readonly_cursor_shape(&roundtrip, "ADTG roundtrip XML");
    assert_sqlserver_customer_projection_shape(
        &adtg,
        "ADTG",
        &[1, 2, 3, 4, 5],
        "server dynamic read-only cursor",
    );
    assert_sqlserver_customer_projection_base_columns(&adtg, "ADTG", false);
    assert_sqlserver_customer_projection_has_only_key_column(&adtg, "ADTG", "CustomerId");

    let forward_only_bytes =
        fs::read(dir.join("sales_customers_server_forwardonly_readonly.adtg")).unwrap();
    assert_ne!(
        adtg_bytes, forward_only_bytes,
        "requested dynamic read-only server cursor should persist distinct ADTG bytes"
    );
    assert_recordset_values_match(&adtg, &roundtrip, "dynamic read-only ADTG vs roundtrip XML");
    assert_recordset_values_match(&adtg, &xml, "dynamic read-only ADTG vs source XML");
}

#[test]
#[ignore = "requires SQL Server corpus under corpus/sqlserver_sales"]
fn sqlserver_provider_duplicate_aliases_preserve_duplicate_field_names_when_present() {
    let dir = sqlserver_sales_corpus_dir();

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    assert_eq!(
        rows.len(),
        SQLSERVER_MANIFEST_ROWS,
        "SQL Server sales manifest row count"
    );
    let manifest = manifest_row(&rows, "sales_customers_duplicate_alias");
    assert_eq!(manifest[1], "sqlserver_provider_duplicate_aliases");
    assert_eq!(manifest[2], "1", "table count");
    assert_eq!(manifest[3], "4", "duplicate-alias row count");
    for artifact in &manifest[4..7] {
        assert!(Path::new(artifact).exists(), "missing artifact {artifact}");
    }

    let xml_path = dir.join("sales_customers_duplicate_alias.xml");
    let adtg_path = dir.join("sales_customers_duplicate_alias.adtg");
    let roundtrip_path = dir.join("sales_customers_duplicate_alias.roundtrip.xml");

    let xml = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", xml_path.display()));
    let adtg = parse_adtg_bytes(&fs::read(&adtg_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", adtg_path.display()));
    let roundtrip = parse_ado_xml_bytes(&fs::read(&roundtrip_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", roundtrip_path.display()));

    assert_sqlserver_duplicate_alias_shape(&xml, "source XML", &["DUP", "c1", "c2", "c3"]);
    assert_sqlserver_duplicate_alias_shape(&adtg, "ADTG", &["DUP", "DUP", "DUP", "DUP"]);
    assert_sqlserver_duplicate_alias_shape(
        &roundtrip,
        "ADTG roundtrip XML",
        &["DUP", "c1", "c2", "c3"],
    );
    assert_recordset_values_match(&adtg, &roundtrip, "duplicate alias ADTG vs roundtrip XML");
    assert_recordset_values_match(&adtg, &xml, "duplicate alias ADTG vs source XML");
}

#[test]
#[ignore = "requires SQL Server corpus under corpus/sqlserver_sales"]
fn sqlserver_legacy_lob_rowversion_join_parses_and_matches_when_present() {
    let dir = sqlserver_sales_corpus_dir();

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    assert_eq!(
        rows.len(),
        SQLSERVER_MANIFEST_ROWS,
        "SQL Server sales manifest row count"
    );
    let manifest = manifest_row(&rows, "sales_legacy_lob_join");
    assert_eq!(manifest[1], "sqlserver_legacy_lob_rowversion_join");
    assert_eq!(manifest[2], "8", "joined table count");
    assert_eq!(manifest[3], "12", "row count");
    for artifact in &manifest[4..7] {
        assert!(Path::new(artifact).exists(), "missing artifact {artifact}");
    }

    let xml_path = dir.join("sales_legacy_lob_join.xml");
    let adtg_path = dir.join("sales_legacy_lob_join.adtg");
    let roundtrip_path = dir.join("sales_legacy_lob_join.roundtrip.xml");

    let xml = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", xml_path.display()));
    let adtg = parse_adtg_bytes(&fs::read(&adtg_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", adtg_path.display()));
    let roundtrip = parse_ado_xml_bytes(&fs::read(&roundtrip_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", roundtrip_path.display()));

    assert_sqlserver_legacy_lob_shape(&xml, "source XML");
    assert_sqlserver_legacy_lob_shape(&adtg, "ADTG");
    assert_sqlserver_legacy_lob_shape(&roundtrip, "ADTG roundtrip XML");
    assert_sqlserver_legacy_lob_base_columns_and_flags(&adtg, "ADTG");
    assert_recordset_values_match(&adtg, &roundtrip, "ADTG vs roundtrip XML");
    assert_recordset_values_match(&adtg, &xml, "ADTG vs source XML");
}

#[test]
#[ignore = "requires SQL Server corpus under corpus/sqlserver_sales"]
fn sqlserver_sql_variant_supported_corpus_pins_ado_subtypes() {
    let dir = sqlserver_sales_corpus_dir();

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    assert_eq!(
        rows.len(),
        SQLSERVER_MANIFEST_ROWS,
        "SQL Server sales manifest row count"
    );
    let manifest = manifest_row(&rows, "sql_variant_supported");
    assert_eq!(manifest[1], "sqlserver_sql_variant_supported_subtypes");
    assert_eq!(manifest[2], "0", "joined table count");
    assert_eq!(manifest[3], "2", "row count");
    for artifact in &manifest[4..7] {
        assert!(Path::new(artifact).exists(), "missing artifact {artifact}");
    }

    let xml_path = dir.join("sql_variant_supported.xml");
    let adtg_path = dir.join("sql_variant_supported.adtg");
    let roundtrip_path = dir.join("sql_variant_supported.roundtrip.xml");

    let xml = parse_ado_xml_bytes(&fs::read(&xml_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", xml_path.display()));
    let adtg = parse_adtg_bytes(&fs::read(&adtg_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", adtg_path.display()));
    let roundtrip = parse_ado_xml_bytes(&fs::read(&roundtrip_path).unwrap())
        .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", roundtrip_path.display()));

    assert_sql_variant_adtg_shape(&adtg, "ADTG");
    assert_sql_variant_xml_shape(&xml, "source XML");
    assert_sql_variant_xml_shape(&roundtrip, "ADTG roundtrip XML");

    let mismatches = compare_native_recordsets(&roundtrip, &adtg);
    assert!(
        mismatches.is_empty(),
        "SQL Server sql_variant text comparison mismatches:\n{}",
        mismatches.join("\n")
    );
}

#[test]
#[ignore = "requires SQL Server corpus under corpus/sqlserver_sales"]
fn sqlserver_sql_variant_failure_matrix_documents_rejected_subtypes() {
    let dir = sqlserver_sales_corpus_dir();

    let rows = read_csv_rows(&dir.join("sql_variant_failures.csv"));
    assert_eq!(rows.len(), 4, "SQL Server sql_variant failure count");
    let by_case = rows
        .iter()
        .map(|row| (row[0].as_str(), row))
        .collect::<HashMap<_, _>>();

    for case in [
        "variant_uniqueidentifier",
        "variant_varchar",
        "variant_nvarchar",
        "variant_varbinary",
    ] {
        let row = by_case
            .get(case)
            .unwrap_or_else(|| panic!("missing sql_variant failure row {case}"));
        assert_eq!(row[1], "12", "{case}: ADO type");
        assert_eq!(row[2], "error", "{case}: result");
        assert_eq!(row[3], "save_adtg", "{case}: failure stage");
        assert_eq!(row[4], "-2147217891", "{case}: failure error number");
        assert!(!row[5].is_empty(), "{case}: failure description");
    }
}

fn assert_sqlserver_sales_shape(recordset: &Recordset, label: &str) {
    assert_eq!(recordset.fields.len(), 58, "{label}: visible field count");
    assert_eq!(recordset.rows.len(), 720, "{label}: row count");

    let fields = fields_by_name(recordset);

    for hidden_name in [
        "CustomerId",
        "RegionId",
        "EmployeeId",
        "ProductId",
        "CategoryId",
        "PaymentId",
        "ShipmentId",
    ] {
        assert!(
            !fields.contains_key(hidden_name),
            "{label}: hidden SQL Server key field should not be public: {hidden_name}"
        );
    }

    for (name, expected_code) in [
        ("ORDER_ID", 3),
        ("LINE_ID", 20),
        ("REGION_CODE", 129),
        ("CUSTOMER_NOTES", 203),
        ("CUSTOMER_PROFILE_HASH", 204),
        ("PRODUCT_SKU", 128),
        ("ORDER_DATE", 135),
        ("ORDER_DATE_ONLY", 202),
        ("ORDER_TIME_ONLY", 202),
        ("ORDER_DATETIME2", 202),
        ("PRIORITY_TINYINT", 17),
        ("SHIP_LABEL", 205),
        ("RATIO_NUMERIC", 131),
    ] {
        let field = fields
            .get(name)
            .unwrap_or_else(|| panic!("{label}: missing field {name}"));
        assert_eq!(
            field.ado_type.map(|ty| ty.code),
            Some(expected_code),
            "{label}: {name} ADO type"
        );
    }
}

fn assert_sqlserver_sales_base_columns(recordset: &Recordset, label: &str) {
    let fields = fields_by_name(recordset);

    for (name, expected_base_column) in [
        ("ORDER_ID", Some("OrderId")),
        ("LINE_ID", Some("LineId")),
        ("LINE_NO", Some("LineNumber")),
        ("REGION_DOMESTIC", Some("IsDomestic")),
        ("CUSTOMER_CREDIT_LIMIT", Some("CreditLimit")),
        ("FREIGHT", Some("FREIGHT")),
        ("ORDER_DATE_ONLY", None),
        ("LINE_NET", None),
        ("HAS_SHIPPED", None),
        ("BIG_SEQUENCE", None),
        ("RATIO_NUMERIC", None),
    ] {
        let field = fields
            .get(name)
            .unwrap_or_else(|| panic!("{label}: missing field {name}"));
        assert_eq!(
            field.base_column.as_deref(),
            expected_base_column,
            "{label}: {name} base column"
        );
    }
}

fn assert_sqlserver_sales_base_tables(recordset: &Recordset, label: &str) {
    let tables = recordset
        .fields
        .iter()
        .filter_map(|field| field.base_table.as_deref())
        .collect::<BTreeSet<_>>();

    assert_eq!(
        tables,
        BTreeSet::from([
            "SalesCategories",
            "SalesCustomers",
            "SalesEmployees",
            "SalesOrderLines",
            "SalesOrders",
            "SalesPayments",
            "SalesProducts",
            "SalesRegions",
            "SalesShipments",
        ]),
        "{label}: sales_mixed_join base-table metadata"
    );
}

fn assert_sqlserver_pending_shape(recordset: &Recordset, label: &str) {
    assert_eq!(
        field_names(recordset),
        sqlserver_pending_field_names(),
        "{label}: fields"
    );

    let fields = fields_by_name(recordset);
    for (name, expected_code) in [
        ("CustomerId", 3),
        ("RegionId", 17),
        ("CustomerCode", 200),
        ("CustomerName", 202),
        ("CustomerNotes", 203),
        ("CreditLimit", 6),
        ("SignupDate", 135),
        ("IsPreferred", 11),
        ("CustomerGuid", 72),
        ("ProfileHash", 204),
    ] {
        let field = fields
            .get(name)
            .unwrap_or_else(|| panic!("{label}: missing field {name}"));
        assert_eq!(
            field.ado_type.map(|ty| ty.code),
            Some(expected_code),
            "{label}: {name} ADO type"
        );
    }

    let default_view = materialize_default_view(recordset);
    assert_eq!(default_view.rows.len(), 6, "{label}: default rows");
    assert_materialized_row(
        default_view
            .rows
            .iter()
            .find(|row| row.values.first() == Some(&Value::Integer(1)))
            .unwrap_or_else(|| panic!("{label}: missing modified customer row")),
        RecordStatusFlag::Modified,
        &[
            Value::Integer(1),
            Value::UnsignedInteger(1),
            string_value("CUST0001"),
            string_value("고객 1 / Customer 1"),
            string_value("updated provider note & <xml>"),
            decimal("12345.67"),
            Value::DateTime("2019-01-10T00:00:00".to_string()),
            Value::Boolean(false),
            Value::Guid("{00000001-1111-2222-3333-000000000001}".to_string()),
            binary_hex("3A11F2B6C86410C929233AFA11655811"),
        ],
        &format!("{label}: modified customer"),
    );
    assert_materialized_row(
        default_view
            .rows
            .iter()
            .find(|row| row.values.first() == Some(&Value::Integer(900001)))
            .unwrap_or_else(|| panic!("{label}: missing inserted customer row")),
        RecordStatusFlag::New,
        &[
            Value::Integer(900001),
            Value::UnsignedInteger(1),
            string_value("CUSTX001"),
            string_value("Inserted Customer"),
            string_value("inserted note"),
            decimal("555.55"),
            Value::DateTime("2025-01-02T03:04:05".to_string()),
            Value::Boolean(true),
            Value::Guid("{00090001-1111-2222-3333-000000900001}".to_string()),
            binary_hex("0102030405060708090A0B0C0D0E0F10"),
        ],
        &format!("{label}: inserted customer"),
    );

    let pending_view = materialize_pending_view(recordset);
    assert_eq!(pending_view.rows.len(), 3, "{label}: pending rows");
    let deleted = pending_view
        .rows
        .iter()
        .find(|row| row.status == RecordStatusFlag::Deleted)
        .unwrap_or_else(|| panic!("{label}: missing deleted customer row"));
    assert_eq!(
        deleted.values.first(),
        Some(&Value::Integer(2)),
        "{label}: deleted customer id"
    );
}

fn assert_sqlserver_pending_base_columns_and_key(recordset: &Recordset, label: &str) {
    let fields = fields_by_name(recordset);
    for name in sqlserver_pending_field_names() {
        let field = fields
            .get(name)
            .unwrap_or_else(|| panic!("{label}: missing field {name}"));
        assert_eq!(
            field.base_column.as_deref(),
            Some(name),
            "{label}: {name} base column"
        );
    }

    assert!(
        fields
            .get("CustomerId")
            .unwrap_or_else(|| panic!("{label}: missing CustomerId field"))
            .key_column,
        "{label}: CustomerId key column"
    );
}

fn assert_sqlserver_sorted_shape(recordset: &Recordset, label: &str) {
    assert_sqlserver_customer_projection_shape(
        recordset,
        label,
        &[7, 1, 8, 2, 3, 4, 5, 6],
        "sorted",
    );
    assert_sqlserver_customer_projection_base_columns(recordset, label, true);
}

fn assert_sqlserver_filtered_shape(recordset: &Recordset, label: &str) {
    assert_sqlserver_customer_projection_shape(recordset, label, &[4, 5, 6, 7, 8], "filtered");
    assert_sqlserver_customer_projection_base_columns(recordset, label, true);
}

fn assert_sqlserver_server_cursor_shape(recordset: &Recordset, label: &str) {
    assert_sqlserver_customer_projection_shape(
        recordset,
        label,
        &[1, 2, 3, 4, 5, 6],
        "server cursor",
    );
    assert_sqlserver_customer_projection_base_columns(recordset, label, true);
}

fn assert_sqlserver_server_cursor_keyset_shape(recordset: &Recordset, label: &str) {
    assert_sqlserver_customer_projection_shape(
        recordset,
        label,
        &[1, 2, 3, 4, 5, 6],
        "server cursor keyset",
    );
    assert_sqlserver_customer_projection_base_columns(recordset, label, false);
}

fn assert_sqlserver_server_readonly_cursor_shape(recordset: &Recordset, label: &str) {
    assert_sqlserver_customer_projection_shape(
        recordset,
        label,
        &[1, 2, 3, 4, 5],
        "server read-only cursor",
    );
    assert_sqlserver_customer_projection_base_columns(recordset, label, false);
    assert_sqlserver_customer_projection_has_no_key_columns(recordset, label);
}

fn assert_sqlserver_customer_projection_shape(
    recordset: &Recordset,
    label: &str,
    expected_customer_ids: &[i64],
    view_label: &str,
) {
    assert_eq!(
        field_names(recordset),
        sqlserver_customer_projection_field_names(),
        "{label}: fields"
    );

    let fields = fields_by_name(recordset);
    for (name, expected_code) in [
        ("CustomerId", 3),
        ("RegionId", 17),
        ("CustomerCode", 200),
        ("CreditLimit", 6),
    ] {
        let field = fields
            .get(name)
            .unwrap_or_else(|| panic!("{label}: missing field {name}"));
        assert_eq!(
            field.ado_type.map(|ty| ty.code),
            Some(expected_code),
            "{label}: {name} ADO type"
        );
    }

    let default_view = materialize_default_view(recordset);
    assert_eq!(
        default_view.rows.len(),
        expected_customer_ids.len(),
        "{label}: default rows"
    );
    assert_eq!(
        default_view
            .rows
            .iter()
            .map(|row| integer_value(row.values.first(), label))
            .collect::<Vec<_>>(),
        expected_customer_ids,
        "{label}: {view_label} customer row order"
    );
    assert!(
        default_view
            .rows
            .iter()
            .all(|row| row.status == RecordStatusFlag::Unmodified),
        "{label}: {view_label} rows should stay unmodified"
    );
    assert_eq!(
        materialize_pending_view(recordset).rows.len(),
        0,
        "{label}: pending rows"
    );
}

fn assert_sqlserver_customer_projection_base_columns(
    recordset: &Recordset,
    label: &str,
    expected_present: bool,
) {
    let fields = fields_by_name(recordset);
    for name in sqlserver_customer_projection_field_names() {
        let field = fields
            .get(name)
            .unwrap_or_else(|| panic!("{label}: missing field {name}"));
        let expected = expected_present.then_some(name);
        assert_eq!(
            field.base_column.as_deref(),
            expected,
            "{label}: {name} base column"
        );
    }
}

fn assert_sqlserver_customer_projection_has_no_key_columns(recordset: &Recordset, label: &str) {
    let fields = fields_by_name(recordset);
    for name in sqlserver_customer_projection_field_names() {
        assert!(
            !fields
                .get(name)
                .unwrap_or_else(|| panic!("{label}: missing field {name}"))
                .key_column,
            "{label}: {name} should not be a key column"
        );
    }
}

fn assert_sqlserver_customer_projection_has_only_key_column(
    recordset: &Recordset,
    label: &str,
    expected_key_name: &str,
) {
    let fields = fields_by_name(recordset);
    for name in sqlserver_customer_projection_field_names() {
        let field = fields
            .get(name)
            .unwrap_or_else(|| panic!("{label}: missing field {name}"));
        assert_eq!(
            field.key_column,
            name == expected_key_name,
            "{label}: {name} key-column flag"
        );
    }
}

fn assert_sqlserver_duplicate_alias_shape(
    recordset: &Recordset,
    label: &str,
    expected_xml_names: &[&str],
) {
    assert_eq!(
        field_names(recordset),
        vec!["DUP", "DUP", "DUP", "DUP"],
        "{label}: duplicate field names"
    );
    assert_eq!(
        recordset
            .fields
            .iter()
            .map(|field| field.xml_name.as_str())
            .collect::<Vec<_>>(),
        expected_xml_names,
        "{label}: XML attribute names"
    );

    for (index, (expected_base_column, expected_type_code)) in [
        ("CustomerId", 3),
        ("RegionId", 17),
        ("CustomerCode", 200),
        ("CreditLimit", 6),
    ]
    .iter()
    .copied()
    .enumerate()
    {
        let field = &recordset.fields[index];
        assert_eq!(
            field.base_column.as_deref(),
            Some(expected_base_column),
            "{label}: field {index} base column"
        );
        assert_eq!(
            field.ado_type.map(|ty| ty.code),
            Some(expected_type_code),
            "{label}: field {index} ADO type"
        );
    }

    assert_row_values(
        recordset,
        label,
        0,
        &[
            Value::Integer(1),
            Value::UnsignedInteger(1),
            string_value("CUST0001"),
            decimal("10432.75"),
        ],
    );
    assert_row_values(
        recordset,
        label,
        1,
        &[
            Value::Integer(2),
            Value::UnsignedInteger(2),
            string_value("CUST0002"),
            decimal("10865.5"),
        ],
    );
    assert_row_values(
        recordset,
        label,
        2,
        &[
            Value::Integer(3),
            Value::UnsignedInteger(3),
            string_value("CUST0003"),
            decimal("11298.25"),
        ],
    );
    assert_row_values(
        recordset,
        label,
        3,
        &[
            Value::Integer(4),
            Value::UnsignedInteger(4),
            string_value("CUST0004"),
            decimal("11731"),
        ],
    );
    assert_eq!(
        materialize_pending_view(recordset).rows.len(),
        0,
        "{label}: pending rows"
    );
}

fn assert_sqlserver_legacy_lob_shape(recordset: &Recordset, label: &str) {
    assert_eq!(
        field_names(recordset),
        sqlserver_legacy_lob_field_names(),
        "{label}: fields"
    );
    assert_eq!(recordset.rows.len(), 12, "{label}: row count");

    let fields = fields_by_name(recordset);
    for (name, expected_code, expected_length) in [
        ("DOC_ID", 3, Some(4)),
        ("ORDER_ID", 3, Some(4)),
        ("LINE_ID", 20, Some(8)),
        ("REGION_CODE", 129, Some(3)),
        ("CUSTOMER_CODE", 200, Some(12)),
        ("PRODUCT_NAME", 202, Some(100)),
        ("CATEGORY_NAME", 202, Some(60)),
        ("LEGACY_CODE", 129, Some(6)),
        ("LEGACY_TEXT", 201, Some(2_147_483_647)),
        ("LEGACY_NTEXT", 203, Some(1_073_741_823)),
        ("LEGACY_IMAGE", 205, Some(2_147_483_647)),
        ("LEGACY_ROWVERSION", 128, Some(8)),
        ("TRACKING_NUMBER", 200, Some(32)),
        ("SHIP_LABEL", 205, Some(2_147_483_647)),
    ] {
        let field = fields
            .get(name)
            .unwrap_or_else(|| panic!("{label}: missing field {name}"));
        assert_eq!(
            field.ado_type.map(|ty| ty.code),
            Some(expected_code),
            "{label}: {name} ADO type"
        );
        assert_eq!(
            field.max_length, expected_length,
            "{label}: {name} defined size"
        );
    }

    for name in ["LEGACY_TEXT", "LEGACY_NTEXT", "LEGACY_IMAGE", "SHIP_LABEL"] {
        let field = fields
            .get(name)
            .unwrap_or_else(|| panic!("{label}: missing field {name}"));
        assert!(field.long, "{label}: {name} should be a long field");
    }

    let row0 = &recordset.rows[0].values;
    assert_eq!(row0[0], Value::Integer(1));
    assert_eq!(row0[1], Value::Integer(100001));
    assert_eq!(row0[2], Value::Integer(1000011));
    assert_eq!(row0[3], string_value("SEL"));
    assert_eq!(row0[4], string_value("CUST0001"));
    assert_eq!(row0[6], string_value("Category 5"));
    assert_eq!(row0[7], string_value("LG0001"));
    assert_eq!(row0[8], string_value("legacy text <doc> & line 1000011"));
    assert_eq!(row0[9], string_value("레거시 문서 1000011"));
    assert_eq!(row0[10], Value::Null);
    assert_binary_hex_len(&row0[11], 16, &format!("{label}: row0 rowversion"));
    assert_eq!(row0[12], string_value("TRK100001"));
    assert_binary_hex_len(&row0[13], 40, &format!("{label}: row0 ship label"));

    let row1 = &recordset.rows[1].values;
    assert_eq!(row1[0], Value::Integer(2));
    assert_eq!(row1[8], Value::Null);
    assert_eq!(row1[9], string_value("레거시 문서 1000012"));
    assert_binary_hex_len(&row1[10], 40, &format!("{label}: row1 legacy image"));
    assert_binary_hex_len(&row1[11], 16, &format!("{label}: row1 rowversion"));

    let row2 = &recordset.rows[2].values;
    assert_eq!(row2[0], Value::Integer(3));
    assert_eq!(row2[8], string_value("legacy text <doc> & line 1000013"));
    assert_eq!(row2[9], Value::Null);
    assert_binary_hex_len(&row2[10], 40, &format!("{label}: row2 legacy image"));
    assert_binary_hex_len(&row2[11], 16, &format!("{label}: row2 rowversion"));
}

fn assert_sqlserver_legacy_lob_base_columns_and_flags(recordset: &Recordset, label: &str) {
    let fields = fields_by_name(recordset);
    for (name, expected_base_column) in [
        ("DOC_ID", "LegacyDocId"),
        ("ORDER_ID", "OrderId"),
        ("LINE_ID", "LineId"),
        ("REGION_CODE", "RegionCode"),
        ("CUSTOMER_CODE", "CustomerCode"),
        ("PRODUCT_NAME", "ProductName"),
        ("CATEGORY_NAME", "CategoryName"),
        ("LEGACY_CODE", "LegacyCode"),
        ("LEGACY_TEXT", "LegacyText"),
        ("LEGACY_NTEXT", "LegacyNText"),
        ("LEGACY_IMAGE", "LegacyImage"),
        ("LEGACY_ROWVERSION", "LegacyRowVersion"),
        ("TRACKING_NUMBER", "TrackingNumber"),
        ("SHIP_LABEL", "ShipLabel"),
    ] {
        let field = fields
            .get(name)
            .unwrap_or_else(|| panic!("{label}: missing field {name}"));
        assert_eq!(
            field.base_column.as_deref(),
            Some(expected_base_column),
            "{label}: {name} base column"
        );
    }

    let rowversion = fields
        .get("LEGACY_ROWVERSION")
        .unwrap_or_else(|| panic!("{label}: missing LEGACY_ROWVERSION field"));
    assert!(
        rowversion.attributes.contains(&FieldAttribute::RowVersion),
        "{label}: LEGACY_ROWVERSION should preserve the row-version field flag"
    );
    assert!(
        rowversion.fixed_length,
        "{label}: LEGACY_ROWVERSION should be fixed length"
    );
}

fn assert_sql_variant_adtg_shape(recordset: &Recordset, label: &str) {
    assert_eq!(
        field_names(recordset),
        sql_variant_field_names(),
        "{label}: fields"
    );
    assert_eq!(recordset.rows.len(), 2, "{label}: row count");

    let fields = fields_by_name(recordset);
    let id = fields
        .get("ID")
        .unwrap_or_else(|| panic!("{label}: missing ID field"));
    assert_eq!(
        id.ado_type.map(|ty| ty.code),
        Some(3),
        "{label}: ID ADO type"
    );

    for name in SQL_VARIANT_FIELDS {
        let field = fields
            .get(name)
            .unwrap_or_else(|| panic!("{label}: missing field {name}"));
        assert_eq!(
            field.ado_type.map(|ty| ty.code),
            Some(12),
            "{label}: {name} ADO type"
        );
        assert_eq!(
            field.data_type.as_deref(),
            Some("variant"),
            "{label}: {name} type"
        );
        assert_eq!(field.max_length, Some(16), "{label}: {name} defined size");
        assert_eq!(field.precision, Some(255), "{label}: {name} precision");
        assert_eq!(field.scale, None, "{label}: {name} scale");
        assert!(field.fixed_length, "{label}: {name} fixed length");
        assert!(field.nullable, "{label}: {name} nullable");
    }

    assert_row_values(
        recordset,
        label,
        0,
        &[
            Value::Integer(1),
            decimal("123"),
            decimal("922337203685477580"),
            decimal("123.45"),
            decimal("123.4567"),
            decimal("123.25"),
            decimal("12.5"),
            Value::Boolean(true),
            Value::DateTime("2024-02-03T04:05:06".to_string()),
        ],
    );
    assert_row_values(
        recordset,
        label,
        1,
        &[
            Value::Integer(2),
            decimal("-5"),
            decimal("-922337203685477580"),
            decimal("-987.65"),
            decimal("-987.6543"),
            decimal("-987.5"),
            decimal("-9.25"),
            Value::Boolean(false),
            Value::DateTime("1999-12-31T23:59:59".to_string()),
        ],
    );
}

fn assert_sql_variant_xml_shape(recordset: &Recordset, label: &str) {
    assert_eq!(
        field_names(recordset),
        sql_variant_field_names(),
        "{label}: fields"
    );
    assert_eq!(recordset.rows.len(), 2, "{label}: row count");

    let fields = fields_by_name(recordset);
    let id = fields
        .get("ID")
        .unwrap_or_else(|| panic!("{label}: missing ID field"));
    assert_eq!(
        id.ado_type.map(|ty| ty.code),
        Some(3),
        "{label}: ID ADO type"
    );

    for name in SQL_VARIANT_FIELDS {
        let field = fields
            .get(name)
            .unwrap_or_else(|| panic!("{label}: missing field {name}"));
        assert_eq!(
            field.ado_type.map(|ty| ty.code),
            Some(203),
            "{label}: {name} ADO type"
        );
        assert_eq!(
            field.data_type.as_deref(),
            Some("string"),
            "{label}: {name} type"
        );
        assert_eq!(field.max_length, None, "{label}: {name} defined size");
        assert_eq!(field.precision, None, "{label}: {name} precision");
        assert_eq!(field.scale, None, "{label}: {name} scale");
        assert!(!field.fixed_length, "{label}: {name} fixed length");
        assert!(field.long, "{label}: {name} long text");
        assert!(field.nullable, "{label}: {name} nullable");
    }

    assert_row_values(
        recordset,
        label,
        0,
        &[
            Value::Integer(1),
            string_value("123"),
            string_value("922337203685477580"),
            string_value("123.45"),
            string_value("123.4567"),
            string_value("123.25"),
            string_value("12.5"),
            string_value("True"),
            string_value("2/3/2024 4:05:06 AM"),
        ],
    );
    assert_row_values(
        recordset,
        label,
        1,
        &[
            Value::Integer(2),
            string_value("-5"),
            string_value("-922337203685477580"),
            string_value("-987.65"),
            string_value("-987.6543"),
            string_value("-987.5"),
            string_value("-9.25"),
            string_value("False"),
            string_value("12/31/1999 11:59:59 PM"),
        ],
    );
}

fn assert_recordset_values_match(left: &Recordset, right: &Recordset, label: &str) {
    assert_eq!(
        field_names(left),
        field_names(right),
        "{label}: field names"
    );
    assert_ordered_rows_eq(
        materialize_default_view(left).rows,
        materialize_default_view(right).rows,
        &format!("{label}: default view"),
    );
    assert_unordered_rows_eq(
        materialize_pending_view(left).rows,
        materialize_pending_view(right).rows,
        &format!("{label}: pending view"),
    );
}

fn field_names(recordset: &Recordset) -> Vec<&str> {
    recordset
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .collect()
}

fn sql_variant_field_names() -> Vec<&'static str> {
    let mut names = vec!["ID"];
    names.extend(SQL_VARIANT_FIELDS);
    names
}

fn sqlserver_pending_field_names() -> Vec<&'static str> {
    vec![
        "CustomerId",
        "RegionId",
        "CustomerCode",
        "CustomerName",
        "CustomerNotes",
        "CreditLimit",
        "SignupDate",
        "IsPreferred",
        "CustomerGuid",
        "ProfileHash",
    ]
}

fn sqlserver_customer_projection_field_names() -> Vec<&'static str> {
    vec!["CustomerId", "RegionId", "CustomerCode", "CreditLimit"]
}

fn sqlserver_legacy_lob_field_names() -> Vec<&'static str> {
    vec![
        "DOC_ID",
        "ORDER_ID",
        "LINE_ID",
        "REGION_CODE",
        "CUSTOMER_CODE",
        "PRODUCT_NAME",
        "CATEGORY_NAME",
        "LEGACY_CODE",
        "LEGACY_TEXT",
        "LEGACY_NTEXT",
        "LEGACY_IMAGE",
        "LEGACY_ROWVERSION",
        "TRACKING_NUMBER",
        "SHIP_LABEL",
    ]
}

fn fields_by_name(recordset: &Recordset) -> HashMap<&str, &Field> {
    recordset
        .fields
        .iter()
        .map(|field| (field.name.as_str(), field))
        .collect()
}

fn manifest_row<'a>(rows: &'a [Vec<String>], case: &str) -> &'a [String] {
    rows.iter()
        .find(|row| row.first().map(|value| value == case).unwrap_or(false))
        .unwrap_or_else(|| panic!("missing manifest row {case}"))
}

fn decimal(value: &str) -> Value {
    Value::Decimal(value.to_string())
}

fn string_value(value: &str) -> Value {
    Value::String(value.to_string())
}

fn binary_hex(value: &str) -> Value {
    Value::BinaryHex(value.to_string())
}

fn assert_binary_hex_len(value: &Value, expected_len: usize, label: &str) {
    let Value::BinaryHex(hex) = value else {
        panic!("{label}: expected binary hex, got {value:?}");
    };
    assert_eq!(hex.len(), expected_len, "{label}: binary hex length");
}

fn assert_materialized_row(
    row: &MaterializedRow,
    expected_status: RecordStatusFlag,
    expected_values: &[Value],
    label: &str,
) {
    assert_eq!(row.status, expected_status, "{label}: status");
    assert_eq!(row.values.as_slice(), expected_values, "{label}: values");
}

fn assert_row_values(recordset: &Recordset, label: &str, row_index: usize, expected: &[Value]) {
    assert_eq!(
        recordset.rows[row_index].values.as_slice(),
        expected,
        "{label}: row {row_index} values"
    );
}

fn integer_value(value: Option<&Value>, label: &str) -> i64 {
    let Some(Value::Integer(value)) = value else {
        panic!("{label}: expected integer value, got {value:?}");
    };
    *value
}

fn assert_ordered_rows_eq(left: Vec<MaterializedRow>, right: Vec<MaterializedRow>, label: &str) {
    assert_eq!(left.len(), right.len(), "{label}: row count");
    for (index, (left, right)) in left.iter().zip(right.iter()).enumerate() {
        assert!(
            rows_match(left, right),
            "{label}: row {index}: left={left:?} right={right:?}"
        );
    }
}

fn assert_unordered_rows_eq(left: Vec<MaterializedRow>, right: Vec<MaterializedRow>, label: &str) {
    assert_eq!(left.len(), right.len(), "{label}: row count");
    let mut used = vec![false; right.len()];
    for left_row in &left {
        let Some(index) = right.iter().enumerate().find_map(|(index, right_row)| {
            (!used[index] && rows_match(left_row, right_row)).then_some(index)
        }) else {
            panic!("{label}: unmatched left row: {left_row:?}; right={right:?}");
        };
        used[index] = true;
    }
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
        (Value::Decimal(left), Value::Decimal(right)) => {
            canonical_decimal_text(left) == canonical_decimal_text(right)
        }
        (Value::DateTime(left), Value::DateTime(right)) => {
            canonical_datetime_text(left) == canonical_datetime_text(right)
        }
        (Value::Guid(left), Value::Guid(right)) => left.eq_ignore_ascii_case(right),
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

fn read_csv_rows(path: &Path) -> Vec<Vec<String>> {
    let text = fs::read_to_string(path).unwrap();
    text.lines().skip(1).map(parse_csv_line).collect()
}

fn manifest_artifact_path(artifact: &str) -> PathBuf {
    let normalized = artifact.replace('\\', "/");
    let path = Path::new(&normalized);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
    }
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
