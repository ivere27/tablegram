use std::fs;
use std::path::Path;
use tablegram::adtg::{parse_adtg_bytes, parse_adtg_file};
use tablegram::compat::{
    materialize_affected_view, materialize_conflicting_view, materialize_default_view,
    materialize_pending_view, try_materialize_affected_view, try_materialize_conflicting_view,
    try_materialize_default_view, try_materialize_pending_view,
};
use tablegram::model::{
    AdoDataType, Field, FieldAttribute, RecordStatusFlag, Recordset, Row, RowChange, RowChangeKind,
    RowState, Value,
};
use tablegram::native_compare::compare_native_recordsets;
use tablegram::xml::parse_ado_xml_bytes;
use tablegram::{
    parse_recordset_bytes, parse_recordset_bytes_with_options, parse_recordset_file,
    validate_recordset_shape, write_ado_xml, write_adtg, RecordsetParseOptions, ResourceLimits,
    MAX_RECORDSET_DEPTH,
};

const ADO_XML_WITHOUT_DECL: &str = r##"<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"
     xmlns:dt="uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"
     xmlns:rs="urn:schemas-microsoft-com:rowset"
     xmlns:z="#RowsetSchema">
  <s:Schema id="RowsetSchema">
    <s:ElementType name="row" content="eltOnly" rs:updatable="true">
      <s:AttributeType name="ID" rs:number="1">
        <s:datatype dt:type="int" dt:maxLength="4" rs:maybenull="false"/>
      </s:AttributeType>
      <s:extends type="rs:rowbase"/>
    </s:ElementType>
  </s:Schema>
  <rs:data>
    <z:row ID="7"/>
  </rs:data>
</xml>"##;

#[test]
fn parse_recordset_bytes_auto_detects_xml_and_adtg() {
    let xml = parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
        .expect("XML should parse through unified API");
    let adtg = parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.adtg"))
        .expect("ADTG should parse through unified API");

    assert_eq!(xml.fields.len(), adtg.fields.len());
    assert_eq!(xml.rows.len(), adtg.rows.len());
    assert_eq!(xml.fields[0].name, "ID");
    assert_eq!(adtg.fields[0].name, "ID");
}

#[test]
fn parse_recordset_bytes_auto_detects_utf8_bom_xml_with_leading_whitespace() {
    let bytes = format!("\u{feff}\r\n\t {ADO_XML_WITHOUT_DECL}").into_bytes();

    let recordset =
        parse_recordset_bytes(&bytes).expect("UTF-8 BOM XML should parse through unified API");

    assert_eq!(recordset.fields.len(), 1);
    assert_eq!(recordset.fields[0].name, "ID");
    assert_eq!(recordset.rows.len(), 1);
}

#[test]
fn parse_recordset_bytes_auto_detects_utf16_xml_with_leading_whitespace() {
    for bytes in [
        utf16_bytes(&format!("\r\n\t {ADO_XML_WITHOUT_DECL}"), true),
        utf16_bytes(&format!("\r\n\t {ADO_XML_WITHOUT_DECL}"), false),
        utf16_bytes_without_bom(&format!("\r\n\t {ADO_XML_WITHOUT_DECL}"), true),
        utf16_bytes_without_bom(&format!("\r\n\t {ADO_XML_WITHOUT_DECL}"), false),
    ] {
        let recordset =
            parse_recordset_bytes(&bytes).expect("UTF-16 XML should parse through unified API");
        assert_eq!(recordset.fields.len(), 1);
        assert_eq!(recordset.fields[0].name, "ID");
        assert_eq!(recordset.rows.len(), 1);
    }
}

#[test]
fn parse_recordset_file_auto_detects_xml_and_adtg() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/generated");
    let xml = parse_recordset_file(root.join("strings_ascii.xml"))
        .expect("XML file should parse through unified API");
    let adtg = parse_recordset_file(root.join("strings_ascii.adtg"))
        .expect("ADTG file should parse through unified API");

    assert_eq!(xml.fields.len(), adtg.fields.len());
    assert_eq!(xml.fields[0].name, "ID");
    assert_eq!(adtg.fields[0].name, "ID");
}

#[test]
fn parse_recordset_bytes_auto_detects_nested_chaptered_adtg() {
    let recordset = parse_recordset_bytes(include_bytes!(
        "../corpus/shape/orders_lines_product_category_shape.adtg"
    ))
    .expect("nested chaptered ADTG should parse through unified API");

    assert!(
        recordset
            .fields
            .iter()
            .any(|field| field.ado_type.map(|ty| ty.code) == Some(136)),
        "chaptered ADTG should expose top-level chapter fields"
    );
    assert!(
        max_chapter_depth(&recordset) >= 3,
        "chaptered ADTG should preserve nested child/grandchild Recordset values"
    );
}

#[test]
fn parse_recordset_file_auto_detects_nested_chaptered_adtg() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("corpus/shape/orders_lines_product_category_shape.adtg");

    let recordset = parse_recordset_file(path)
        .expect("nested chaptered ADTG file should parse through unified API");

    assert!(
        recordset
            .fields
            .iter()
            .any(|field| field.ado_type.map(|ty| ty.code) == Some(136)),
        "chaptered ADTG should expose top-level chapter fields"
    );
    assert!(
        max_chapter_depth(&recordset) >= 3,
        "chaptered ADTG should preserve nested child/grandchild Recordset values"
    );
}

#[test]
fn validate_recordset_shape_rejects_row_value_count_mismatch() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.rows[0].values.pop();

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject row value count mismatch");

    assert!(
        format!("{err:#}").contains("row 0 had"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn writers_reject_row_value_count_mismatch_before_serializing() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.rows[0].values.pop();

    let adtg_err =
        write_adtg(&recordset).expect_err("ADTG writer should validate caller-built shapes");
    let xml_err =
        write_ado_xml(&recordset).expect_err("XML writer should validate caller-built shapes");

    assert!(
        format!("{adtg_err:#}").contains("cannot write inconsistent ADO Recordset shape")
            && format!("{adtg_err:#}").contains("row 0 had"),
        "unexpected ADTG writer error: {adtg_err:#}"
    );
    assert!(
        format!("{xml_err:#}").contains("cannot write inconsistent ADO Recordset shape")
            && format!("{xml_err:#}").contains("row 0 had"),
        "unexpected XML writer error: {xml_err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_duplicate_field_ordinals() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].ordinal = Some(2);
    recordset.fields[1].ordinal = Some(2);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject duplicate ordinals");

    assert!(
        format!("{err:#}").contains("duplicate field ordinal 2"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_empty_base_catalog_metadata() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].base_catalog = Some(String::new());

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject empty base catalog metadata");

    assert!(
        format!("{err:#}").contains("empty base catalog"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_empty_base_schema_metadata() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].base_schema = Some(String::new());

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject empty base schema metadata");

    assert!(
        format!("{err:#}").contains("empty base schema"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_empty_base_table_metadata() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].base_table = Some(String::new());

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject empty base table metadata");

    assert!(
        format!("{err:#}").contains("empty base table"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_empty_base_column_metadata() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].base_column = Some(String::new());

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject empty base column metadata");

    assert!(
        format!("{err:#}").contains("empty base column"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_empty_data_type_metadata() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].data_type = Some(String::new());

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject empty data type metadata");

    assert!(
        format!("{err:#}").contains("empty data type"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_empty_db_type_metadata() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].db_type = Some(String::new());

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject empty DB type metadata");

    assert!(
        format!("{err:#}").contains("empty DB type"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_field_ordinals_outside_field_count() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].ordinal = Some(recordset.fields.len() + 1);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject ordinals outside the field count");

    assert!(
        format!("{err:#}").contains("exceeded field count"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_incomplete_field_ordinals() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].ordinal = None;

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject partial field ordinal metadata");

    assert!(
        format!("{err:#}").contains("field ordinals were incomplete"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_duplicate_change_row_indices() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.changes[0].row_indices.push(0);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject duplicate row indices in a change");

    assert!(
        format!("{err:#}").contains("change 0 had duplicate row index 0"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn parse_recordset_bytes_allows_adtg_duplicate_alias_field_names() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("corpus/sqlserver_sales/sales_customers_duplicate_alias.adtg");
    if !path.exists() {
        return;
    }

    let recordset = parse_recordset_bytes(&fs::read(&path).unwrap())
        .expect("SQL Server duplicate-alias ADTG should parse through public API");

    let duplicate_alias_types = recordset
        .fields
        .iter()
        .filter(|field| field.xml_name == "DUP")
        .map(|field| field.ado_type.map(|ty| ty.code))
        .collect::<Vec<_>>();

    assert_eq!(
        duplicate_alias_types,
        vec![Some(3), Some(17), Some(200), Some(6)],
        "duplicate alias ADTG should preserve every DUP field"
    );
}

#[test]
fn validate_recordset_shape_rejects_unavailable_values_outside_updated_rows() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.rows[0].values[0] = Value::Unavailable;

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject unavailable values outside updated rows");

    assert!(
        format!("{err:#}").contains("unavailable value outside an updated row"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_chapter_field_value_mismatch() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].ado_type = Some(AdoDataType::new("adChapter", 136));
    recordset.fields[0]
        .attributes
        .push(FieldAttribute::IsChapter);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject chapter field/value mismatch");

    assert!(
        format!("{err:#}").contains("chapter/value mismatch"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_value_kind_that_does_not_match_ado_type() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.rows[0].values[0] = Value::String("not an integer".to_string());

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject value kind that does not match field type");

    assert!(
        format!("{err:#}").contains("value kind did not match ADO type"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_bounded_string_payloads_wider_than_field() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/strings_ascii.xml"))
            .expect("string fixture should parse before mutation");
    let string_field = recordset
        .fields
        .iter()
        .position(|field| {
            matches!(
                field.ado_type.map(|ty| ty.code),
                Some(200 | 202 | 129 | 130)
            ) && !field.long
                && field.max_length.is_some()
        })
        .expect("string fixture should contain a bounded string field");
    let max_length = recordset.fields[string_field].max_length.unwrap();
    recordset.rows[0].values[string_field] = Value::String("X".repeat(max_length + 1));

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject bounded string payloads wider than max length");

    assert!(
        format!("{err:#}").contains("string length"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_allows_long_string_payloads_wider_than_max_length() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/strings_ascii.xml"))
            .expect("string fixture should parse before mutation");
    let string_field = recordset
        .fields
        .iter()
        .position(|field| {
            matches!(
                field.ado_type.map(|ty| ty.code),
                Some(200 | 202 | 129 | 130)
            ) && !field.long
                && field.max_length.is_some()
        })
        .expect("string fixture should contain a bounded string field");
    let max_length = recordset.fields[string_field].max_length.unwrap();
    recordset.fields[string_field].ado_type = Some(AdoDataType::new("adLongVarWChar", 203));
    recordset.fields[string_field].long = true;
    recordset.fields[string_field].fixed_length = false;
    recordset.fields[string_field]
        .attributes
        .retain(|attribute| *attribute != FieldAttribute::Fixed);
    recordset.fields[string_field]
        .attributes
        .push(FieldAttribute::Long);
    recordset.rows[0].values[string_field] = Value::String("X".repeat(max_length + 1));

    validate_recordset_shape(&recordset)
        .expect("long string fields may carry payloads wider than max length");
}

#[test]
fn validate_recordset_shape_allows_variant_string_payloads_wider_than_max_length() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].ado_type = Some(AdoDataType::new("adVariant", 12));
    recordset.fields[0].max_length = Some(11);
    recordset.rows[0].values[0] = Value::String("ABCDEFGHIJKL".to_string());

    validate_recordset_shape(&recordset)
        .expect("adVariant string payloads may exceed variant max length metadata");
}

#[test]
fn validate_recordset_shape_rejects_signed_integer_payloads_outside_declared_width() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].ado_type = Some(AdoDataType::new("adTinyInt", 16));
    recordset.fields[0].max_length = Some(1);
    recordset.rows[0].values[0] = Value::Integer(128);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject out-of-range signed integer payloads");

    assert!(
        format!("{err:#}").contains("outside adTinyInt range -128..=127"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_unsigned_integer_payloads_outside_declared_width() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].ado_type = Some(AdoDataType::new("adUnsignedTinyInt", 17));
    recordset.fields[0].max_length = Some(1);
    recordset.rows[0].values[0] = Value::UnsignedInteger(256);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject out-of-range unsigned integer payloads");

    assert!(
        format!("{err:#}").contains("outside adUnsignedTinyInt range 0..=255"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_non_finite_float_payloads() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.rows[0].values[3] = Value::Float(f64::NAN);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject non-finite float payloads");

    assert!(
        format!("{err:#}").contains("non-finite float"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_single_payloads_outside_f32_range() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[3].ado_type = Some(AdoDataType::new("adSingle", 4));
    recordset.rows[0].values[3] = Value::Float((f32::MAX as f64) * 2.0);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject adSingle payloads outside f32 range");

    assert!(
        format!("{err:#}").contains("outside adSingle finite range"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_accepts_single_payloads_normalized_to_f32() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[3].ado_type = Some(AdoDataType::new("adSingle", 4));
    recordset.rows[0].values[3] = Value::Float(0.5f32 as f64);

    validate_recordset_shape(&recordset)
        .expect("validator should accept parser-normalized adSingle payloads");
}

#[test]
fn validate_recordset_shape_rejects_single_payloads_not_normalized_to_f32() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[3].ado_type = Some(AdoDataType::new("adSingle", 4));
    recordset.rows[0].values[3] = Value::Float(0.1);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject non-normalized adSingle payloads");

    assert!(
        format!("{err:#}").contains("was not normalized as adSingle"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_accepts_double_width_four_like_mdac_xml() {
    let recordset =
        parse_recordset_bytes(include_bytes!("../corpus/fuzz/doc_float_type_aliases.xml"))
            .expect("MDAC XML adDouble width-4 fixture should parse");

    assert_eq!(
        recordset.fields[2].ado_type,
        Some(AdoDataType::new("adDouble", 5))
    );
    assert_eq!(recordset.fields[2].max_length, Some(4));
    assert_eq!(recordset.rows[0].values[2], Value::Float(0.0));
    validate_recordset_shape(&recordset).expect("validator should accept adDouble width 4");
}

#[test]
fn validate_recordset_shape_accepts_double_width_four_payloads_normalized_to_f32() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[3].max_length = Some(4);
    recordset.rows[0].values[3] = Value::Float(0.5f32 as f64);

    validate_recordset_shape(&recordset)
        .expect("validator should accept parser-normalized 4-byte adDouble payloads");
}

#[test]
fn validate_recordset_shape_rejects_double_width_four_payloads_not_normalized_to_f32() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[3].max_length = Some(4);
    recordset.rows[0].values[3] = Value::Float(0.1);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject non-normalized 4-byte adDouble payloads");

    assert!(
        format!("{err:#}").contains("was not normalized as 4-byte adDouble"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_double_metadata_with_non_mdac_width() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[3].max_length = Some(5);

    let err =
        validate_recordset_shape(&recordset).expect_err("validator should reject adDouble width 5");

    assert!(
        format!("{err:#}").contains("adDouble max length 5 was not MDAC width 4 or 8"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_invalid_guid_payloads() {
    let mut recordset = parse_recordset_bytes(include_bytes!(
        "../corpus/shape/customers_orders_guid_shape.adtg"
    ))
    .expect("GUID fixture should parse before mutation");
    recordset.rows[0].values[1] = Value::Guid("00112233-4455-6677-8899-AABBCCDDEEFF".to_string());

    let err =
        validate_recordset_shape(&recordset).expect_err("validator should reject malformed GUIDs");

    assert!(
        format!("{err:#}").contains("invalid GUID"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_non_canonical_guid_payloads() {
    let mut recordset = parse_recordset_bytes(include_bytes!(
        "../corpus/shape/customers_orders_guid_shape.adtg"
    ))
    .expect("GUID fixture should parse before mutation");
    recordset.rows[0].values[1] = Value::Guid("{00112233-4455-6677-8899-aabbccddeeff}".to_string());

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject lowercase GUID payloads");

    assert!(
        format!("{err:#}").contains("non-canonical GUID"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_invalid_binary_hex_payloads() {
    let mut recordset = parse_recordset_bytes(include_bytes!("../corpus/generated/binary.xml"))
        .expect("binary fixture should parse before mutation");
    recordset.rows[0].values[1] = Value::BinaryHex("ABC".to_string());

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject odd-length binary hex");

    assert!(
        format!("{err:#}").contains("invalid binary hex"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_non_canonical_binary_hex_payloads() {
    let mut recordset = parse_recordset_bytes(include_bytes!("../corpus/generated/binary.xml"))
        .expect("binary fixture should parse before mutation");
    recordset.rows[0].values[1] = Value::BinaryHex("deadbeef".to_string());

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject lowercase binary hex");

    assert!(
        format!("{err:#}").contains("non-canonical binary hex"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_binary_payloads_wider_than_field() {
    let mut recordset = parse_recordset_bytes(include_bytes!("../corpus/generated/binary.xml"))
        .expect("binary fixture should parse before mutation");
    let max_length = recordset.fields[1]
        .max_length
        .expect("binary fixture should expose fixed max length");
    recordset.rows[0].values[1] = Value::BinaryHex("00".repeat(max_length + 1));

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject binary payloads wider than max length");

    assert!(
        format!("{err:#}").contains("binary payload length"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_allows_long_binary_payloads_wider_than_max_length() {
    let mut recordset = parse_recordset_bytes(include_bytes!("../corpus/generated/binary.xml"))
        .expect("binary fixture should parse before mutation");
    let max_length = recordset.fields[1]
        .max_length
        .expect("binary fixture should expose fixed max length");
    recordset.fields[1].ado_type = Some(AdoDataType::new("adLongVarBinary", 205));
    recordset.fields[1].long = true;
    recordset.fields[1].fixed_length = false;
    recordset.fields[1]
        .attributes
        .retain(|attribute| *attribute != FieldAttribute::Fixed);
    recordset.fields[1].attributes.push(FieldAttribute::Long);
    recordset.rows[0].values[1] = Value::BinaryHex("00".repeat(max_length + 1));

    validate_recordset_shape(&recordset)
        .expect("long binary fields may carry payloads wider than max length");
}

#[test]
fn validate_recordset_shape_rejects_invalid_decimal_payloads() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.rows[0].values[2] = Value::Decimal("12..34".to_string());

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject malformed decimal text");

    assert!(
        format!("{err:#}").contains("invalid decimal"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_currency_payloads_outside_declared_scale() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.rows[0].values[2] = Value::Decimal("1.23456".to_string());

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject currency values outside fixed scale");

    assert!(
        format!("{err:#}").contains("exceeded adCurrency scale 4"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_currency_payloads_outside_mdac_range() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.rows[0].values[2] = Value::Decimal("922337203685477.5808".to_string());

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject currency values outside the scaled i64 range");

    assert!(
        format!("{err:#}").contains("outside adCurrency range"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_allows_negative_currency_minimum() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.rows[0].values[2] = Value::Decimal("-922337203685477.5808".to_string());

    validate_recordset_shape(&recordset)
        .expect("negative adCurrency minimum should fit the scaled i64 range");
}

#[test]
fn validate_recordset_shape_rejects_currency_metadata_with_non_mdac_width() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[2].max_length = Some(9);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject impossible adCurrency width");

    assert!(
        format!("{err:#}").contains("adCurrency max length 9 was not MDAC width 8"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_numeric_payloads_outside_declared_precision() {
    let mut recordset = parse_recordset_bytes(include_bytes!(
        "../corpus/shape/lines_decimal_numeric_relation_shape.adtg"
    ))
    .expect("numeric fixture should parse before mutation");
    recordset.rows[0].values[0] = Value::Decimal("1234567890".to_string());

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject numeric values outside declared precision");

    assert!(
        format!("{err:#}").contains("exceeded precision 9"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_numeric_payloads_outside_declared_scale() {
    let mut recordset = parse_recordset_bytes(include_bytes!(
        "../corpus/shape/lines_decimal_numeric_relation_shape.adtg"
    ))
    .expect("numeric fixture should parse before mutation");
    recordset.rows[0].values[0] = Value::Decimal("1.23456".to_string());

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject numeric values outside declared scale");

    assert!(
        format!("{err:#}").contains("scale 5 exceeded declared scale 4"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_invalid_date_payloads() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].ado_type = Some(AdoDataType::new("adDBDate", 133));
    recordset.fields[0].max_length = Some(6);
    recordset.rows[0].values[0] = Value::Date("2026-02-29".to_string());

    let err =
        validate_recordset_shape(&recordset).expect_err("validator should reject invalid dates");

    assert!(
        format!("{err:#}").contains("invalid date"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_invalid_time_payloads() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].ado_type = Some(AdoDataType::new("adDBTime", 134));
    recordset.fields[0].max_length = Some(6);
    recordset.rows[0].values[0] = Value::Time("24:00:00".to_string());

    let err =
        validate_recordset_shape(&recordset).expect_err("validator should reject invalid times");

    assert!(
        format!("{err:#}").contains("invalid time"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_invalid_datetime_payloads() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.rows[0].values[4] = Value::DateTime("2026-01-01T12:00:00.".to_string());

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject invalid datetimes");

    assert!(
        format!("{err:#}").contains("invalid datetime"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_accepts_date_metadata_at_adtg_width() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[4].max_length = Some(8);

    validate_recordset_shape(&recordset).expect("validator should accept ADTG-width adDate");
}

#[test]
fn validate_recordset_shape_rejects_date_metadata_with_non_mdac_width() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[4].max_length = Some(9);

    let err =
        validate_recordset_shape(&recordset).expect_err("validator should reject adDate width 9");

    assert!(
        format!("{err:#}").contains("adDate max length 9 was not MDAC width 8 or 16"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_dbdate_metadata_with_non_mdac_width() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].ado_type = Some(AdoDataType::new("adDBDate", 133));
    recordset.fields[0].max_length = Some(8);
    recordset.rows[0].values[0] = Value::Date("2026-01-01".to_string());

    let err =
        validate_recordset_shape(&recordset).expect_err("validator should reject DBDate width 8");

    assert!(
        format!("{err:#}").contains("adDBDate max length 8 was not MDAC width 6"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_dbtime_metadata_with_non_mdac_width() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].ado_type = Some(AdoDataType::new("adDBTime", 134));
    recordset.fields[0].max_length = Some(8);
    recordset.rows[0].values[0] = Value::Time("12:34:56".to_string());

    let err =
        validate_recordset_shape(&recordset).expect_err("validator should reject DBTime width 8");

    assert!(
        format!("{err:#}").contains("adDBTime max length 8 was not MDAC width 6"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_accepts_filetime_metadata_at_xml_width() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[4].ado_type = Some(AdoDataType::new("adFileTime", 64));
    recordset.fields[4].max_length = Some(16);

    validate_recordset_shape(&recordset).expect("validator should accept XML-width adFileTime");
}

#[test]
fn validate_recordset_shape_rejects_filetime_metadata_with_non_mdac_width() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[4].ado_type = Some(AdoDataType::new("adFileTime", 64));
    recordset.fields[4].max_length = Some(12);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject impossible FileTime width");

    assert!(
        format!("{err:#}").contains("adFileTime max length 12 was not MDAC width 8 or 16"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_timestamp_metadata_with_non_mdac_width() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[4].ado_type = Some(AdoDataType::new("adDBTimeStamp", 135));
    recordset.fields[4].max_length = Some(24);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject impossible DBTIMESTAMP width");

    assert!(
        format!("{err:#}").contains("adDBTimeStamp max length 24 was not MDAC width 16"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_accepts_timestamp_scale_nine() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[4].ado_type = Some(AdoDataType::new("adDBTimeStamp", 135));
    recordset.fields[4].max_length = Some(16);
    recordset.fields[4].scale = Some(9);
    recordset.rows[0].values[4] = Value::DateTime("2026-01-01T12:00:00.123456789".to_string());

    validate_recordset_shape(&recordset)
        .expect("validator should accept DBTIMESTAMP scale 9 metadata");
}

#[test]
fn validate_recordset_shape_rejects_timestamp_scale_above_nine() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[4].ado_type = Some(AdoDataType::new("adDBTimeStamp", 135));
    recordset.fields[4].max_length = Some(16);
    recordset.fields[4].scale = Some(10);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject impossible DBTIMESTAMP scale metadata");

    assert!(
        format!("{err:#}").contains("adDBTimeStamp scale 10 outside 0..=9"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_filetime_payloads_before_1601() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[4].ado_type = Some(AdoDataType::new("adFileTime", 64));
    recordset.fields[4].max_length = Some(8);
    recordset.rows[0].values[4] = Value::DateTime("1600-12-31T23:59:59".to_string());

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject out-of-range adFileTime values");

    assert!(
        format!("{err:#}").contains("adFileTime year 1600 is out of range"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_fractional_filetime_payloads() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[4].ado_type = Some(AdoDataType::new("adFileTime", 64));
    recordset.fields[4].max_length = Some(8);
    recordset.rows[0].values[4] = Value::DateTime("1601-01-01T00:00:00.1".to_string());

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject non-canonical adFileTime values");

    assert!(
        format!("{err:#}").contains("adFileTime had fractional seconds"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_field_boolean_attribute_disagreement() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].writable = !recordset.fields[0].writable;

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject field boolean/attribute disagreement");

    assert!(
        format!("{err:#}").contains("writable flag disagreed with attributes"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_boolean_metadata_with_non_mdac_width() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[1].max_length = Some(4);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject impossible adBoolean width");

    assert!(
        format!("{err:#}").contains("adBoolean max length 4 was not MDAC width 2"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_non_fixed_metadata_for_fixed_width_ado_types() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].fixed_length = false;
    recordset.fields[0]
        .attributes
        .retain(|attribute| *attribute != FieldAttribute::Fixed);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject non-fixed metadata for fixed-width ADO types");

    assert!(
        format!("{err:#}").contains("adInteger should be fixed-length"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_non_fixed_metadata_for_fixed_binary_types() {
    let mut recordset = parse_recordset_bytes(include_bytes!("../corpus/generated/binary.xml"))
        .expect("binary fixture should parse before mutation");
    recordset.fields[1].fixed_length = false;
    recordset.fields[1]
        .attributes
        .retain(|attribute| *attribute != FieldAttribute::Fixed);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject non-fixed metadata for adBinary");

    assert!(
        format!("{err:#}").contains("adBinary should be fixed-length"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_non_fixed_metadata_for_variant_types() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].ado_type = Some(AdoDataType::new("adVariant", 12));
    recordset.fields[0].fixed_length = false;
    recordset.fields[0]
        .attributes
        .retain(|attribute| *attribute != FieldAttribute::Fixed);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject non-fixed metadata for adVariant");

    assert!(
        format!("{err:#}").contains("adVariant should be fixed-length"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_variant_metadata_below_mdac_width() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].ado_type = Some(AdoDataType::new("adVariant", 12));
    recordset.fields[0].max_length = Some(10);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject impossible adVariant max length");

    assert!(
        format!("{err:#}").contains("adVariant max length 10 below MDAC minimum 11"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_accepts_xml_varnumeric_width_three() {
    let recordset = parse_recordset_bytes(include_bytes!(
        "../corpus/fuzz/doc_number_varnumeric_small_width.xml"
    ))
    .expect("XML adVarNumeric width-3 fixture should parse");

    assert_eq!(
        recordset.fields[1].ado_type,
        Some(AdoDataType::new("adVarNumeric", 139))
    );
    assert_eq!(recordset.fields[1].max_length, Some(3));
    validate_recordset_shape(&recordset)
        .expect("validator should accept MDAC XML adVarNumeric width 3");
}

#[test]
fn validate_recordset_shape_rejects_varnumeric_without_max_length() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/fuzz/doc_number_varnumeric.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[1].max_length = None;

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject unbounded adVarNumeric metadata");

    assert!(
        format!("{err:#}").contains("adVarNumeric missing max length"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_varnumeric_metadata_below_xml_width() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/fuzz/doc_number_varnumeric.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[1].max_length = Some(2);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject impossible adVarNumeric width");

    assert!(
        format!("{err:#}").contains("adVarNumeric max length 2 below MDAC XML minimum 3"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_varnumeric_payloads_wider_than_field() {
    let mut recordset = parse_recordset_bytes(include_bytes!(
        "../corpus/fuzz/doc_number_varnumeric_small_width.xml"
    ))
    .expect("fixture should parse before mutation");
    recordset.rows[0].values[1] = Value::Decimal("6000.75".to_string());

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject oversized adVarNumeric payloads");

    assert!(
        format!("{err:#}").contains("adVarNumeric payload length"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_varnumeric_exponent_payloads() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/fuzz/doc_number_varnumeric.xml"))
            .expect("fixture should parse before mutation");
    recordset.rows[0].values[1] = Value::Decimal("1e1".to_string());

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject non-normalized adVarNumeric payloads");

    assert!(
        format!("{err:#}").contains("adVarNumeric decimal"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_fixed_metadata_for_non_fixed_ado_types() {
    let mut recordset = parse_recordset_bytes(include_bytes!("../corpus/generated/binary.xml"))
        .expect("binary fixture should parse before mutation");
    recordset.fields[1].ado_type = Some(AdoDataType::new("adVarBinary", 204));

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject fixed metadata for adVarBinary");

    assert!(
        format!("{err:#}").contains("non-fixed ADO type adVarBinary had fixed-length metadata"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_non_long_metadata_for_long_ado_types() {
    let mut recordset = parse_recordset_bytes(include_bytes!("../corpus/generated/binary.xml"))
        .expect("binary fixture should parse before mutation");
    recordset.fields[2].long = false;
    recordset.fields[2]
        .attributes
        .retain(|attribute| *attribute != FieldAttribute::Long);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject non-long metadata for adLongVarBinary");

    assert!(
        format!("{err:#}").contains("adLongVarBinary should be long"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_long_metadata_for_non_long_ado_types() {
    let mut recordset = parse_recordset_bytes(include_bytes!("../corpus/generated/binary.xml"))
        .expect("binary fixture should parse before mutation");
    recordset.fields[1].long = true;
    recordset.fields[1].attributes.push(FieldAttribute::Long);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject long metadata for adBinary");

    assert!(
        format!("{err:#}").contains("non-long ADO type adBinary had long metadata"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_duplicate_field_attributes() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    let attribute = recordset.fields[0].attributes[0];
    recordset.fields[0].attributes.push(attribute);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject duplicate field attributes");

    assert!(
        format!("{err:#}").contains("duplicate attribute"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_mismatched_ado_type_name_and_code() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].ado_type = Some(AdoDataType::new("adInteger", 5));

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject mismatched ADO type metadata");

    assert!(
        format!("{err:#}").contains("ADO type code 5 was named adInteger, expected adDouble"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_unknown_ado_type_code() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].ado_type = Some(AdoDataType::new("adFuture", 4096));

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject unknown ADO type metadata");

    assert!(
        format!("{err:#}").contains("unknown ADO type code 4096"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_zero_field_max_length() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].max_length = Some(0);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject zero max length metadata");

    assert!(
        format!("{err:#}").contains("zero max length"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_fixed_width_metadata_below_type_width() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].max_length = Some(3);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject impossible fixed-width metadata");

    assert!(
        format!("{err:#}").contains("below adInteger minimum width 4"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_bigint_metadata_below_type_width() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].ado_type = Some(AdoDataType::new("adBigInt", 20));
    recordset.fields[0].max_length = Some(3);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject impossible adBigInt metadata");

    assert!(
        format!("{err:#}").contains("below adBigInt minimum width 4"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_unsigned_bigint_metadata_below_type_width() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].ado_type = Some(AdoDataType::new("adUnsignedBigInt", 21));
    recordset.fields[0].max_length = Some(3);
    recordset.rows[0].values[0] = Value::UnsignedInteger(7);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject impossible adUnsignedBigInt metadata");

    assert!(
        format!("{err:#}").contains("below adUnsignedBigInt minimum width 4"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_guid_metadata_with_non_mdac_width() {
    let mut recordset = parse_recordset_bytes(include_bytes!(
        "../corpus/shape/customers_orders_guid_shape.adtg"
    ))
    .expect("GUID fixture should parse before mutation");
    recordset.fields[1].max_length = Some(17);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject impossible adGUID width");

    assert!(
        format!("{err:#}").contains("adGUID max length 17 was not MDAC width 16"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_negative_field_scale() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect("fixture should parse before mutation");
    recordset.fields[0].scale = Some(-1);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject negative field scale metadata");

    assert!(
        format!("{err:#}").contains("negative scale -1"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_numeric_metadata_with_non_mdac_width() {
    let mut recordset = parse_recordset_bytes(include_bytes!(
        "../corpus/shape/lines_decimal_numeric_relation_shape.adtg"
    ))
    .expect("numeric fixture should parse before mutation");
    recordset.fields[0].max_length = Some(20);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject impossible adNumeric width");

    assert!(
        format!("{err:#}").contains("adNumeric max length 20 was not MDAC width 19"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_invalid_numeric_precision_scale() {
    let mut recordset = parse_recordset_bytes(include_bytes!(
        "../corpus/shape/lines_decimal_numeric_relation_shape.adtg"
    ))
    .expect("numeric fixture should parse before mutation");
    recordset.fields[0].precision = Some(0);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject invalid adNumeric precision");

    assert!(
        format!("{err:#}").contains("adNumeric precision 0 outside 1..=38"),
        "unexpected validation error: {err:#}"
    );

    let mut recordset = parse_recordset_bytes(include_bytes!(
        "../corpus/shape/lines_decimal_numeric_relation_shape.adtg"
    ))
    .expect("numeric fixture should parse before mutation");
    recordset.fields[0].scale = Some(10);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject adNumeric scale above precision");

    assert!(
        format!("{err:#}").contains("adNumeric scale 10 exceeds precision 9"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_decimal_metadata_with_non_mdac_width() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/fuzz/rowid_negative_scale.adtg"))
            .expect("decimal fixture should parse before mutation");
    recordset.fields[1].max_length = Some(17);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject impossible adDecimal width");

    assert!(
        format!("{err:#}").contains("adDecimal max length 17 was not MDAC width 16"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_invalid_decimal_precision_scale() {
    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/fuzz/rowid_negative_scale.adtg"))
            .expect("decimal fixture should parse before mutation");
    recordset.fields[1].precision = Some(29);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject invalid adDecimal precision");

    assert!(
        format!("{err:#}").contains("adDecimal precision 29 outside 1..=28"),
        "unexpected validation error: {err:#}"
    );

    let mut recordset =
        parse_recordset_bytes(include_bytes!("../corpus/fuzz/rowid_negative_scale.adtg"))
            .expect("decimal fixture should parse before mutation");
    recordset.fields[1].scale = Some(10);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject adDecimal scale above precision");

    assert!(
        format!("{err:#}").contains("adDecimal scale 10 exceeds precision 9"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_chapter_attribute_disagreement() {
    let mut recordset = parse_recordset_bytes(include_bytes!(
        "../corpus/shape/orders_lines_product_category_shape.adtg"
    ))
    .expect("chaptered fixture should parse before mutation");
    recordset.fields[3]
        .attributes
        .retain(|attribute| *attribute != FieldAttribute::IsChapter);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject chapter attribute disagreement");

    assert!(
        format!("{err:#}").contains("chapter flag disagreed with attributes"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_rejects_chapter_descriptor_mismatch() {
    let mut recordset = parse_recordset_bytes(include_bytes!(
        "../corpus/shape/orders_lines_product_category_shape.adtg"
    ))
    .expect("chaptered fixture should parse before mutation");
    recordset.fields[3].max_length = Some(8);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject invalid chapter descriptor metadata");

    assert!(
        format!("{err:#}").contains("adChapter max length was Some(8), expected 4"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn validate_recordset_shape_accepts_mdac_chapter_unspecified_precision_scale() {
    let mut recordset = parse_recordset_bytes(include_bytes!(
        "../corpus/shape/orders_lines_product_category_shape.adtg"
    ))
    .expect("chaptered fixture should parse before mutation");
    recordset.fields[3].precision = Some(255);
    recordset.fields[3].scale = Some(255);

    validate_recordset_shape(&recordset)
        .expect("MDAC-resaved chapter precision/scale sentinels should validate");
}

#[test]
fn validate_recordset_shape_rejects_excessive_chapter_depth() {
    let recordset = nested_schema_recordset(MAX_RECORDSET_DEPTH + 1);

    let err = validate_recordset_shape(&recordset)
        .expect_err("validator should reject excessive chapter depth");

    assert!(
        format!("{err:#}").contains("maximum ADO Recordset chapter depth"),
        "unexpected validation error: {err:#}"
    );
}

#[test]
fn writers_reject_excessive_chapter_depth() {
    let recordset = nested_schema_recordset(MAX_RECORDSET_DEPTH + 1);

    let adtg_err = write_adtg(&recordset).expect_err("ADTG writer should reject deep chapters");
    let xml_err = write_ado_xml(&recordset).expect_err("XML writer should reject deep chapters");

    assert!(
        format!("{adtg_err:#}").contains("maximum ADO Recordset chapter depth"),
        "unexpected ADTG writer error: {adtg_err:#}"
    );
    assert!(
        format!("{xml_err:#}").contains("maximum ADO Recordset chapter depth"),
        "unexpected XML writer error: {xml_err:#}"
    );
}

#[test]
fn try_materialize_views_reject_excessive_chapter_depth() {
    let recordset = nested_value_recordset(MAX_RECORDSET_DEPTH + 1);

    for (label, result) in [
        (
            "default",
            try_materialize_default_view(&recordset).map(|_| ()),
        ),
        (
            "pending",
            try_materialize_pending_view(&recordset).map(|_| ()),
        ),
        (
            "affected",
            try_materialize_affected_view(&recordset).map(|_| ()),
        ),
        (
            "conflicting",
            try_materialize_conflicting_view(&recordset).map(|_| ()),
        ),
    ] {
        let err = match result {
            Ok(()) => panic!("materializing {label} should reject deep chapters"),
            Err(err) => err,
        };
        assert!(
            format!("{err:#}").contains("maximum ADO Recordset chapter depth"),
            "unexpected {label} materialization error: {err:#}"
        );
    }
}

#[test]
fn compatibility_materialize_views_do_not_panic_on_invalid_recordsets() {
    let recordset = nested_value_recordset(MAX_RECORDSET_DEPTH + 1);

    let result = std::panic::catch_unwind(|| {
        let _ = materialize_default_view(&recordset);
        let _ = materialize_pending_view(&recordset);
        let _ = materialize_affected_view(&recordset);
        let _ = materialize_conflicting_view(&recordset);
    });

    assert!(
        result.is_ok(),
        "compatibility materializers should not panic on caller-built invalid Recordsets"
    );
}

#[test]
fn parse_ado_xml_bytes_rejects_excessive_chapter_schema_depth() {
    let xml = deeply_nested_ado_xml(MAX_RECORDSET_DEPTH + 1);

    let err = parse_ado_xml_bytes(xml.as_bytes())
        .expect_err("XML parser should reject excessive shaped schema depth");

    assert!(
        format!("{err:#}").contains("maximum ADO Recordset chapter depth"),
        "unexpected XML parser error: {err:#}"
    );
}

#[test]
fn parse_recordset_bytes_with_options_rejects_oversized_input() {
    let options = RecordsetParseOptions::default().with_resource_limits(
        ResourceLimits::default().with_max_input_bytes(ADO_XML_WITHOUT_DECL.len() - 1),
    );

    let err = parse_recordset_bytes_with_options(ADO_XML_WITHOUT_DECL.as_bytes(), options)
        .expect_err("parser should reject input larger than configured limit");

    assert!(
        format!("{err:#}").contains("maximum input length"),
        "unexpected limit error: {err:#}"
    );
}

#[test]
fn parse_recordset_bytes_with_options_rejects_row_limit_excess() {
    let options = RecordsetParseOptions::default()
        .with_resource_limits(ResourceLimits::default().with_max_rows_per_recordset(0));

    let err = parse_recordset_bytes_with_options(ADO_XML_WITHOUT_DECL.as_bytes(), options)
        .expect_err("parser should reject rows beyond configured limit");

    assert!(
        format!("{err:#}").contains("maximum row count"),
        "unexpected row-limit error: {err:#}"
    );
}

#[test]
fn parse_recordset_bytes_with_options_rejects_excessive_decimal_exponent_expansion() {
    let xml = decimal_ado_xml("1e5000");
    let options = RecordsetParseOptions::default()
        .with_resource_limits(ResourceLimits::default().with_max_xml_decimal_expanded_len(128));

    let err = parse_recordset_bytes_with_options(xml.as_bytes(), options)
        .expect_err("bounded XML parser should reject excessive decimal exponent expansion");

    assert!(
        format!("{err:#}").contains("maximum decimal expansion length"),
        "unexpected decimal expansion error: {err:#}"
    );
}

#[test]
fn native_compare_reports_excessive_chapter_depth() {
    let recordset = nested_schema_recordset(MAX_RECORDSET_DEPTH + 1);

    let mismatches = compare_native_recordsets(&recordset, &recordset);

    assert_eq!(mismatches.len(), 1, "unexpected mismatches: {mismatches:?}");
    assert!(
        mismatches[0].contains("maximum ADO Recordset chapter depth"),
        "unexpected native compare mismatch: {:?}",
        mismatches[0]
    );
}

#[test]
fn parse_adtg_bytes_parses_flat_and_chaptered_adtg() {
    let flat = parse_adtg_bytes(include_bytes!("../corpus/generated/types_basic.adtg"))
        .expect("flat ADTG should parse through direct ADTG API");
    let chaptered = parse_adtg_bytes(include_bytes!(
        "../corpus/shape/orders_lines_product_category_shape.adtg"
    ))
    .expect("chaptered ADTG should parse through direct ADTG API");

    assert_eq!(flat.fields[0].name, "ID");
    assert!(
        chaptered
            .fields
            .iter()
            .any(|field| field.ado_type.map(|ty| ty.code) == Some(136)),
        "chaptered ADTG should expose chapter fields"
    );
}

#[test]
fn format_specific_parsers_return_validated_recordset_shapes() {
    let xml = parse_ado_xml_bytes(include_bytes!("../corpus/generated/types_basic.xml"))
        .expect("direct XML parser should return a valid Recordset shape");
    let adtg = parse_adtg_bytes(include_bytes!(
        "../corpus/shape/orders_lines_product_category_shape.adtg"
    ))
    .expect("direct ADTG parser should return a valid Recordset shape");

    validate_recordset_shape(&xml).expect("direct XML parser output shape");
    validate_recordset_shape(&adtg).expect("direct ADTG parser output shape");
}

#[test]
fn parse_adtg_file_parses_flat_and_chaptered_adtg() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus");
    let flat = parse_adtg_file(root.join("generated/types_basic.adtg"))
        .expect("flat ADTG file should parse through direct ADTG API");
    let chaptered = parse_adtg_file(root.join("shape/orders_lines_product_category_shape.adtg"))
        .expect("chaptered ADTG file should parse through direct ADTG API");

    assert_eq!(flat.fields[0].name, "ID");
    assert!(
        chaptered
            .fields
            .iter()
            .any(|field| field.ado_type.map(|ty| ty.code) == Some(136)),
        "chaptered ADTG should expose chapter fields"
    );
}

#[test]
fn parse_flat_adtg_compat_alias_parses_chaptered_adtg() {
    let recordset = tablegram::adtg::parse_flat_adtg(include_bytes!(
        "../corpus/shape/orders_lines_product_category_shape.adtg"
    ))
    .expect("legacy flat ADTG alias should parse checked chaptered ADTG layouts");

    assert!(
        recordset
            .fields
            .iter()
            .any(|field| field.ado_type.map(|ty| ty.code) == Some(136)),
        "chaptered ADTG should expose chapter fields through compatibility alias"
    );
    assert!(
        max_chapter_depth(&recordset) >= 3,
        "chaptered ADTG should preserve nested Recordset values through compatibility alias"
    );
}

#[test]
fn parse_recordset_bytes_rejects_xml_when_forced_through_adtg_parser() {
    let err =
        tablegram::adtg::parse_flat_adtg(include_bytes!("../corpus/generated/types_basic.xml"))
            .expect_err("ADTG parser should reject XML");
    assert!(err.to_string().contains("input looks like ADO XML"));
}

#[test]
fn native_library_parser_modules_stay_com_free() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let parser_modules = [
        "src/lib.rs",
        "src/adtg.rs",
        "src/xml.rs",
        "src/detect.rs",
        "src/model.rs",
        "src/compat.rs",
        "src/native_compare.rs",
    ];
    let disallowed = [
        "ADODB",
        "MSPersist",
        "cscript",
        "SysWOW64",
        "WScript",
        "std::process",
        "std::os::windows",
        "cfg(windows",
        "powershell",
        "cmd.exe",
        "ProcessCommand",
        "Command::new",
    ];

    for module in parser_modules {
        let text = fs::read_to_string(root.join(module)).unwrap();
        for needle in disallowed {
            assert!(
                !text.contains(needle),
                "{module} should remain native Rust parser code; found {needle:?}"
            );
        }
    }

    let manifest = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    for needle in [
        "windows =",
        "windows-sys",
        "windows-targets",
        "winapi",
        "com-rs",
        "wmi",
    ] {
        assert!(
            !manifest.contains(needle),
            "Cargo.toml should not add a direct native parser COM/Windows binding dependency: {needle:?}"
        );
    }
}

fn nested_schema_recordset(depth: usize) -> Recordset {
    let mut fields = vec![integer_field("Leaf")];
    for level in (0..depth).rev() {
        fields = vec![chapter_field(&format!("Chapter{level}"), fields)];
    }
    Recordset {
        fields,
        rows: Vec::new(),
        changes: Vec::new(),
    }
}

fn nested_value_recordset(depth: usize) -> Recordset {
    if depth == 0 {
        return current_recordset(vec![integer_field("Leaf")], vec![Value::Integer(1)]);
    }

    let child = nested_value_recordset(depth - 1);
    current_recordset(
        vec![chapter_field(
            &format!("Chapter{depth}"),
            child.fields.clone(),
        )],
        vec![Value::Chapter(Box::new(child))],
    )
}

fn current_recordset(fields: Vec<Field>, values: Vec<Value>) -> Recordset {
    Recordset {
        fields,
        rows: vec![Row {
            ordinal: 0,
            state: RowState::Current,
            status_flags: vec![RecordStatusFlag::Unmodified],
            change_index: Some(0),
            values,
        }],
        changes: vec![RowChange {
            kind: RowChangeKind::Current,
            row_indices: vec![0],
        }],
    }
}

fn integer_field(name: &str) -> Field {
    Field {
        name: name.to_string(),
        xml_name: name.to_string(),
        ordinal: Some(1),
        data_type: Some("int".to_string()),
        db_type: None,
        ado_type: Some(AdoDataType::new("adInteger", 3)),
        max_length: Some(4),
        precision: None,
        scale: None,
        nullable: false,
        writable: false,
        fixed_length: true,
        long: false,
        key_column: false,
        base_catalog: None,
        base_schema: None,
        base_table: None,
        base_column: None,
        chapter_fields: None,
        chapter_relation: None,
        attributes: vec![FieldAttribute::Fixed],
    }
}

fn chapter_field(name: &str, chapter_fields: Vec<Field>) -> Field {
    Field {
        name: name.to_string(),
        xml_name: name.to_string(),
        ordinal: Some(1),
        data_type: Some("chapter".to_string()),
        db_type: None,
        ado_type: Some(AdoDataType::new("adChapter", 136)),
        max_length: Some(4),
        precision: None,
        scale: None,
        nullable: false,
        writable: false,
        fixed_length: true,
        long: false,
        key_column: false,
        base_catalog: None,
        base_schema: None,
        base_table: None,
        base_column: None,
        chapter_fields: Some(chapter_fields),
        chapter_relation: None,
        attributes: vec![FieldAttribute::Fixed, FieldAttribute::IsChapter],
    }
}

fn deeply_nested_ado_xml(depth: usize) -> String {
    let mut xml = String::from(
        r##"<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882" xmlns:dt="uuid:C2F41010-65B3-11d1-A29F-00AA00C14882" xmlns:rs="urn:schemas-microsoft-com:rowset" xmlns:z="#RowsetSchema"><s:Schema id="RowsetSchema">"##,
    );
    for level in 0..=depth {
        let element_name = if level == 0 {
            "row".to_string()
        } else {
            format!("Chapter{level}")
        };
        xml.push_str(&format!(
            r#"<s:ElementType name="{element_name}" content="eltOnly"><s:AttributeType name="ID{level}" rs:number="1"><s:datatype dt:type="int" dt:maxLength="4" rs:maybenull="false"/></s:AttributeType>"#
        ));
    }
    for _ in 0..=depth {
        xml.push_str(r#"<s:extends type="rs:rowbase"/></s:ElementType>"#);
    }
    xml.push_str("</s:Schema><rs:data/></xml>");
    xml
}

fn decimal_ado_xml(value: &str) -> String {
    format!(
        r##"<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"
     xmlns:dt="uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"
     xmlns:rs="urn:schemas-microsoft-com:rowset"
     xmlns:z="#RowsetSchema">
  <s:Schema id="RowsetSchema">
    <s:ElementType name="row" content="eltOnly" rs:updatable="true">
      <s:AttributeType name="PAYLOAD" rs:number="1">
        <s:datatype dt:type="decimal" rs:maybenull="true"/>
      </s:AttributeType>
      <s:extends type="rs:rowbase"/>
    </s:ElementType>
  </s:Schema>
  <rs:data>
    <z:row PAYLOAD="{value}"/>
  </rs:data>
</xml>"##
    )
}

fn max_chapter_depth(recordset: &Recordset) -> usize {
    recordset
        .rows
        .iter()
        .flat_map(|row| row.values.iter())
        .filter_map(|value| match value {
            Value::Chapter(child) => Some(1 + max_chapter_depth(child)),
            _ => None,
        })
        .max()
        .unwrap_or(0)
}

fn utf16_bytes(text: &str, little_endian: bool) -> Vec<u8> {
    let mut out = if little_endian {
        vec![0xff, 0xfe]
    } else {
        vec![0xfe, 0xff]
    };
    for unit in text.encode_utf16() {
        let bytes = if little_endian {
            unit.to_le_bytes()
        } else {
            unit.to_be_bytes()
        };
        out.extend_from_slice(&bytes);
    }
    out
}

fn utf16_bytes_without_bom(text: &str, little_endian: bool) -> Vec<u8> {
    let mut out = Vec::new();
    for unit in text.encode_utf16() {
        let bytes = if little_endian {
            unit.to_le_bytes()
        } else {
            unit.to_be_bytes()
        };
        out.extend_from_slice(&bytes);
    }
    out
}
