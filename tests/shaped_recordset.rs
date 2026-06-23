use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use tablegram::adtg::parse_adtg_bytes;
use tablegram::compat::{materialize_default_view, materialize_pending_view};
use tablegram::model::{FieldAttribute, RecordStatusFlag, Value};
use tablegram::xml::parse_ado_xml_bytes;

type AggregateLineExpectation<'a> = (i64, i64, i64, i64, &'a str, &'a str);
type NestedSiblingGrandchildLineExpectation<'a> =
    (i64, i64, i64, i64, i64, &'a str, &'a str, i64, &'a str);
type DeepNestedLineExpectation<'a> = (i64, i64, i64, i64, i64, i64, &'a str, &'a str);

struct AggregateParentExpectation<'a> {
    order_id: i64,
    customer_id: i64,
    freight: &'a str,
    total_sum: &'a str,
    min_quantity: i64,
    max_quantity: i64,
    expected_lines: &'a [AggregateLineExpectation<'a>],
}

#[test]
fn shape_manifest_matches_checked_artifacts_when_present() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/shape");
    if !dir.exists() {
        return;
    }

    let rows = read_csv_rows(&dir.join("manifest.csv"));
    assert_eq!(rows.len(), 40, "shape manifest row count");

    let mut manifest_artifacts = BTreeSet::new();
    let mut missing = Vec::new();
    for row in rows {
        assert_eq!(row.len(), 7, "shape manifest columns: {row:?}");
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
        "shape manifest references missing artifacts: {missing:?}"
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
        "shape manifest artifact list"
    );
}

#[test]
fn shaped_xml_parses_chapter_recordsets() {
    let source = parse_ado_xml_bytes(include_bytes!("fixtures/shape/orders_lines_shape.xml"))
        .expect("failed to parse shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_lines_shape.roundtrip.xml"
    ))
    .expect("failed to parse shaped roundtrip XML");

    assert_eq!(source, roundtrip, "source and roundtrip shaped XML");
    assert_shaped_recordset_rows(&source);

    let chapter_field = &source.fields[3];
    assert_eq!(chapter_field.name, "Lines");
    assert_eq!(chapter_field.ado_type.map(|ty| ty.code), Some(136));
    assert_eq!(
        chapter_field.attributes,
        vec![FieldAttribute::Fixed, FieldAttribute::IsChapter]
    );

    let default_view = materialize_default_view(&source);
    assert_eq!(default_view.fields[3].attribute_flags, 0x2000 | 0x10);
}

#[test]
fn shaped_xml_ignores_prefixed_non_rowsetschema_chapter_children_like_mdac() {
    let source = include_str!("fixtures/shape/orders_lines_shape.xml");
    let first_line = "<Lines OrderId='100001' LineId='1000011' Quantity='4'/>";

    let cases = [
        (
            source.replace(
                "xmlns:z='#RowsetSchema'>",
                "xmlns:z='#RowsetSchema'\n\txmlns:x='urn:wrong'>",
            ),
            "<x:Lines OrderId='100001' LineId='1000011' Quantity='4'/>",
        ),
        (
            source.to_string(),
            "<rs:Lines OrderId='100001' LineId='1000011' Quantity='4'/>",
        ),
    ];

    for (xml, replacement) in cases {
        let xml = xml.replace(first_line, replacement);
        let recordset = parse_ado_xml_bytes(xml.as_bytes())
            .expect("prefixed non-RowsetSchema chapter child should be ignored");
        let lines = chapter_at(&recordset.rows[0].values, 3, "Lines");

        assert_eq!(lines.rows.len(), 2, "first parent Lines row count");
        assert_eq!(
            lines
                .rows
                .iter()
                .map(|row| row.values[1].clone())
                .collect::<Vec<_>>(),
            vec![Value::Integer(1000012), Value::Integer(1000013)]
        );
    }
}

#[test]
fn shaped_xml_accepts_unprefixed_chapter_children_with_default_namespaces_like_mdac() {
    let source = include_str!("fixtures/shape/orders_lines_shape.xml");
    let first_line = "<Lines OrderId='100001' LineId='1000011' Quantity='4'/>";

    for replacement in [
        "<Lines xmlns='urn:wrong' OrderId='100001' LineId='1000011' Quantity='4'/>",
        "<Lines xmlns='#RowsetSchema' OrderId='100001' LineId='1000011' Quantity='4'/>",
    ] {
        let xml = source.replace(first_line, replacement);
        let recordset = parse_ado_xml_bytes(xml.as_bytes())
            .expect("unprefixed chapter child should be accepted");
        let lines = chapter_at(&recordset.rows[0].values, 3, "Lines");

        assert_eq!(lines.rows.len(), 3, "first parent Lines row count");
        assert_eq!(
            lines
                .rows
                .iter()
                .map(|row| row.values[1].clone())
                .collect::<Vec<_>>(),
            vec![
                Value::Integer(1000011),
                Value::Integer(1000012),
                Value::Integer(1000013)
            ]
        );
    }
}

#[test]
fn shaped_xml_ignores_prefixed_rowsetschema_chapter_child_like_mdac() {
    let source = include_str!("fixtures/shape/orders_lines_shape.xml");
    let xml = source
        .replace("xmlns:z='#RowsetSchema'>", "xmlns:p='#RowsetSchema'>")
        .replace("<z:row", "<p:row")
        .replace("</z:row>", "</p:row>")
        .replace(
            "<Lines OrderId='100001' LineId='1000011' Quantity='4'/>",
            "<p:Lines OrderId='100001' LineId='1000011' Quantity='4'/>",
        );

    let recordset = parse_ado_xml_bytes(xml.as_bytes())
        .expect("prefixed RowsetSchema chapter child should be ignored");
    let lines = chapter_at(&recordset.rows[0].values, 3, "Lines");

    assert_eq!(lines.rows.len(), 2, "first parent Lines row count");
    assert_eq!(
        lines
            .rows
            .iter()
            .map(|row| row.values[1].clone())
            .collect::<Vec<_>>(),
        vec![Value::Integer(1000012), Value::Integer(1000013)]
    );
}

#[test]
fn shaped_xml_uses_root_row_prefix_binding_for_parent_rows_like_mdac() {
    let source = include_str!("fixtures/shape/orders_lines_shape.xml");

    let row_local_wrong_prefix = source.replace(
        "<z:row OrderId='100001'",
        "<z:row xmlns:z='urn:wrong' OrderId='100001'",
    );
    let recordset = parse_ado_xml_bytes(row_local_wrong_prefix.as_bytes())
        .expect("row-local prefix redeclaration should not hide the root row prefix");
    assert_eq!(recordset.rows.len(), 3);
    assert_eq!(recordset.rows[0].values[0], Value::Integer(100001));

    let row_local_rowset_prefix = source
        .replace("xmlns:z='#RowsetSchema'>", ">")
        .replace("<z:row", "<p:row xmlns:p='#RowsetSchema'")
        .replace("</z:row>", "</p:row>");
    let recordset = parse_ado_xml_bytes(row_local_rowset_prefix.as_bytes())
        .expect("row-local RowsetSchema prefix declarations should be ignored");
    assert!(recordset.rows.is_empty());

    let data_local_rowset_prefix = source
        .replace("xmlns:z='#RowsetSchema'>", ">")
        .replace("<rs:data>", "<rs:data xmlns:p='#RowsetSchema'>")
        .replace("<z:row", "<p:row")
        .replace("</z:row>", "</p:row>");
    let recordset = parse_ado_xml_bytes(data_local_rowset_prefix.as_bytes())
        .expect("data-local RowsetSchema prefix declarations should be ignored");
    assert!(recordset.rows.is_empty());
}

#[test]
fn shaped_xml_ignores_orphan_chapter_children_like_mdac() {
    let source = include_str!("fixtures/shape/orders_lines_shape.xml");
    let cases = [
        source.replace(
            "<rs:data>",
            "<rs:data>\n\t<Lines OrderId='999999' LineId='9999991' Quantity='1'/>",
        ),
        source.replace(
            "<rs:data>",
            "<rs:data>\n\t<Unexpected><Lines OrderId='999999' LineId='9999991' Quantity='1'/></Unexpected>",
        ),
        source
            .replace(
                "xmlns:z='#RowsetSchema'>",
                "xmlns:z='#RowsetSchema'\n\txmlns:x='urn:wrong'>",
            )
            .replace(
                "<rs:data>",
                "<rs:data>\n\t<x:Lines OrderId='999999' LineId='9999991' Quantity='1'/>",
            ),
    ];

    for xml in cases {
        let recordset =
            parse_ado_xml_bytes(xml.as_bytes()).expect("orphan chapter child should be ignored");
        let lines = chapter_at(&recordset.rows[0].values, 3, "Lines");

        assert_eq!(recordset.rows.len(), 3);
        assert_eq!(lines.rows.len(), 3, "first parent Lines row count");
        assert_eq!(
            lines
                .rows
                .iter()
                .map(|row| row.values[1].clone())
                .collect::<Vec<_>>(),
            vec![
                Value::Integer(1000011),
                Value::Integer(1000012),
                Value::Integer(1000013)
            ]
        );
    }
}

#[test]
fn shaped_xml_ignores_orphan_prefixed_rowsetschema_chapter_child_like_mdac() {
    let source = include_str!("fixtures/shape/orders_lines_shape.xml");
    let xml = source
        .replace("xmlns:z='#RowsetSchema'>", "xmlns:p='#RowsetSchema'>")
        .replace("<z:row", "<p:row")
        .replace("</z:row>", "</p:row>")
        .replace(
            "<rs:data>",
            "<rs:data>\n\t<p:Lines OrderId='999999' LineId='9999991' Quantity='1'/>",
        );

    let recordset = parse_ado_xml_bytes(xml.as_bytes())
        .expect("orphan prefixed RowsetSchema chapter child should be ignored");
    let lines = chapter_at(&recordset.rows[0].values, 3, "Lines");

    assert_eq!(recordset.rows.len(), 3);
    assert_eq!(lines.rows.len(), 3, "first parent Lines row count");
}

#[test]
fn shaped_xml_rejects_multiple_rowset_schema_namespace_prefixes_like_mdac() {
    let xml = include_str!("fixtures/shape/orders_lines_shape.xml").replace(
        "xmlns:z='#RowsetSchema'>",
        "xmlns:z='#RowsetSchema'\n\txmlns:p='#RowsetSchema'>",
    );

    let err = parse_ado_xml_bytes(xml.as_bytes())
        .expect_err("duplicate RowsetSchema namespace prefixes should be rejected");
    let message = format!("{err:#}");
    assert!(
        message.contains("declared multiple RowsetSchema namespace prefixes"),
        "{message}"
    );
}

#[test]
fn shaped_xml_ignores_descendants_of_ignored_prefixed_chapter_children_like_mdac() {
    let source = include_str!("fixtures/shape/orders_lines_product_shape.xml");
    let xml = add_shape_namespace(
        &replace_first_nested_lines_block(
            source,
            "<x:Lines OrderId='100001' LineId='1000011' LineNumber='1' ProductId='13' Quantity='4'>\n\t\t\t<Product ProductId='13' ProductName='Product 13 mixed' UnitCost='20.31'/>\n\t\t</x:Lines>",
        ),
        "xmlns:x='urn:wrong'",
    );

    let recordset = parse_ado_xml_bytes(xml.as_bytes())
        .expect("wrong-prefixed nested chapter subtree should be ignored");
    let lines = chapter_at(&recordset.rows[0].values, 3, "Lines");

    assert_eq!(lines.rows.len(), 2, "first parent Lines row count");
    assert_eq!(
        lines
            .rows
            .iter()
            .map(|row| row.values[1].clone())
            .collect::<Vec<_>>(),
        vec![Value::Integer(1000012), Value::Integer(1000013)]
    );
    for row in &lines.rows {
        assert_eq!(chapter_at(&row.values, 5, "Product").rows.len(), 1);
    }
}

#[test]
fn shaped_xml_ignores_wrong_prefixed_nested_chapter_children_like_mdac() {
    let source = include_str!("fixtures/shape/orders_lines_product_shape.xml");
    let xml = add_shape_namespace(
        &replace_first_nested_lines_block(
            source,
            "<Lines OrderId='100001' LineId='1000011' LineNumber='1' ProductId='13' Quantity='4'>\n\t\t\t<x:Product ProductId='13' ProductName='Product 13 mixed' UnitCost='20.31'/>\n\t\t</Lines>",
        ),
        "xmlns:x='urn:wrong'",
    );

    let recordset = parse_ado_xml_bytes(xml.as_bytes())
        .expect("wrong-prefixed nested chapter child should be ignored");
    let lines = chapter_at(&recordset.rows[0].values, 3, "Lines");

    assert_eq!(lines.rows.len(), 3, "first parent Lines row count");
    assert_eq!(
        chapter_at(&lines.rows[0].values, 5, "Product").rows.len(),
        0
    );
    assert_eq!(
        chapter_at(&lines.rows[1].values, 5, "Product").rows.len(),
        1
    );
    assert_eq!(
        chapter_at(&lines.rows[2].values, 5, "Product").rows.len(),
        1
    );
}

#[test]
fn shaped_xml_preserves_raw_child_chapter_attribute_values() {
    let control_text = "A\0\u{1}\u{8}Z";
    let xml = format!(
        r##"<?xml version="1.0"?>
<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"
     xmlns:dt="uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"
     xmlns:rs="urn:schemas-microsoft-com:rowset"
     xmlns:z="#RowsetSchema">
  <s:Schema id="RowsetSchema">
    <s:ElementType name="row" content="eltOnly">
      <s:AttributeType name="ID" rs:number="1">
        <s:datatype dt:type="int" dt:maxLength="4" rs:maybenull="false"/>
      </s:AttributeType>
      <s:ElementType name="Lines" content="eltOnly">
        <s:AttributeType name="LineText" rs:number="1">
          <s:datatype dt:type="string" dt:maxLength="16" rs:maybenull="false"/>
        </s:AttributeType>
        <s:extends type="rs:rowbase"/>
      </s:ElementType>
      <s:extends type="rs:rowbase"/>
    </s:ElementType>
  </s:Schema>
  <rs:data>
    <z:row ID="1"><Lines LineText="{control_text}"/></z:row>
  </rs:data>
</xml>"##
    );

    let recordset = parse_ado_xml_bytes(xml.as_bytes())
        .expect("failed to parse shaped XML with raw child control text");
    let lines = chapter_at(&recordset.rows[0].values, 1, "Lines");

    assert_eq!(
        lines.rows[0].values[0],
        Value::String(control_text.to_string())
    );
}

#[test]
fn shaped_xml_rejects_rs_forcenull_for_non_nullable_chapter_field_like_mdac() {
    let xml = r##"<?xml version="1.0"?>
<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"
     xmlns:dt="uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"
     xmlns:rs="urn:schemas-microsoft-com:rowset"
     xmlns:z="#RowsetSchema">
  <s:Schema id="RowsetSchema">
    <s:ElementType name="row" content="eltOnly">
      <s:AttributeType name="ID" rs:number="1">
        <s:datatype dt:type="int" dt:maxLength="4" rs:maybenull="false"/>
      </s:AttributeType>
      <s:ElementType name="Lines" content="eltOnly">
        <s:AttributeType name="LineText" rs:number="1">
          <s:datatype dt:type="string" dt:maxLength="16" rs:maybenull="false"/>
        </s:AttributeType>
        <s:extends type="rs:rowbase"/>
      </s:ElementType>
      <s:extends type="rs:rowbase"/>
    </s:ElementType>
  </s:Schema>
  <rs:data>
    <z:row ID="1"><Lines rs:forcenull="LineText"/></z:row>
  </rs:data>
</xml>"##;

    let err = parse_ado_xml_bytes(xml.as_bytes())
        .expect_err("non-nullable shaped force-null should be rejected");
    let message = format!("{err:#}");
    assert!(
        message.contains("force-null XML field LineText in Current row was not nullable"),
        "{message}"
    );
}

#[test]
fn shaped_xml_rejects_unexpected_chapter_child_elements() {
    let source = include_str!("fixtures/shape/orders_lines_shape.xml");
    let parent_child = source.replace(
        "<Lines OrderId='100001' LineId='1000011' Quantity='4'/>",
        "<Unexpected/><Lines OrderId='100001' LineId='1000011' Quantity='4'/>",
    );
    let err = parse_ado_xml_bytes(parent_child.as_bytes())
        .expect_err("unexpected shaped parent-row child should be rejected");
    let message = format!("{err:#}");
    assert!(
        message.contains("unexpected ADO XML row child element Unexpected"),
        "{message}"
    );

    let nested_child = source.replace(
        "<Lines OrderId='100001' LineId='1000011' Quantity='4'/>",
        "<Lines OrderId='100001' LineId='1000011' Quantity='4'><Unexpected/></Lines>",
    );
    let err = parse_ado_xml_bytes(nested_child.as_bytes())
        .expect_err("unexpected nested shaped row child should be rejected");
    let message = format!("{err:#}");
    assert!(
        message.contains("unexpected ADO XML Lines child element Unexpected"),
        "{message}"
    );
}

#[test]
fn shaped_xml_parses_unknown_data_child_elements_like_mdac() {
    let source = include_str!("fixtures/shape/orders_lines_shape.xml");
    let empty_child = source.replace("<rs:data>", "<rs:data><Unexpected/>");

    let recordset = parse_ado_xml_bytes(empty_child.as_bytes())
        .expect("unexpected shaped data child should be ignored");
    assert_eq!(recordset.rows.len(), 3);
    assert_eq!(recordset.rows[0].values[0], Value::Integer(100001));

    let nested_row = source.replace(
        "<rs:data>",
        "<rs:data><Unexpected><z:row OrderId='999999' CustomerId='9' Freight='1.25'/></Unexpected>",
    );
    let recordset = parse_ado_xml_bytes(nested_row.as_bytes())
        .expect("nested shaped RowsetSchema row should be parsed");
    assert_eq!(recordset.rows.len(), 4);
    assert_eq!(recordset.rows[0].values[0], Value::Integer(999999));
    assert_eq!(
        recordset.rows[0].values[2],
        Value::Decimal("1.25".to_string())
    );
}

#[test]
fn shaped_xml_parses_missing_data_section_as_empty_rowset_like_mdac() {
    let source = include_str!("fixtures/shape/orders_lines_shape.xml");
    let start = source
        .find("<rs:data>")
        .expect("fixture should contain data start");
    let end = source
        .find("</rs:data>")
        .expect("fixture should contain data end")
        + "</rs:data>".len();
    let xml = format!("{}{}", &source[..start], &source[end..]);

    let recordset = parse_ado_xml_bytes(xml.as_bytes()).unwrap();
    assert_eq!(recordset.fields.len(), 4);
    assert!(recordset.rows.is_empty());
    assert!(recordset.changes.is_empty());
}

#[test]
fn shaped_xml_parses_multiple_data_sections_like_mdac() {
    let source = include_str!("fixtures/shape/orders_lines_shape.xml");
    let start = source
        .find("<rs:data>")
        .expect("fixture should contain data start");
    let end = source
        .find("</rs:data>")
        .expect("fixture should contain data end")
        + "</rs:data>".len();
    let data_block = &source[start..end];
    let xml = format!("{}{}{}", &source[..end], data_block, &source[end..]);

    let recordset = parse_ado_xml_bytes(xml.as_bytes()).unwrap();
    assert_eq!(recordset.rows.len(), 6);
    assert_eq!(recordset.changes.len(), 6);
}

#[test]
fn shaped_xml_rejects_multiple_schema_sections() {
    let xml = include_str!("fixtures/shape/orders_lines_shape.xml")
        .replace("</s:Schema>", "</s:Schema><s:Schema id='OtherSchema'/>");

    let err = parse_ado_xml_bytes(xml.as_bytes())
        .expect_err("duplicate shaped schema section should be rejected");
    let message = format!("{err:#}");
    assert!(
        message.contains("ADO XML contained multiple schema sections"),
        "{message}"
    );
}

#[test]
fn shaped_xml_rejects_non_ado_root_element() {
    let xml = include_str!("fixtures/shape/orders_lines_shape.xml")
        .replace("<xml ", "<notxml ")
        .replace("</xml>", "</notxml>");

    let err =
        parse_ado_xml_bytes(xml.as_bytes()).expect_err("non-ADO shaped root should be rejected");
    let message = format!("{err:#}");
    assert!(
        message.contains("ADO XML root element must be xml, found notxml"),
        "{message}"
    );
}

#[test]
fn shaped_xml_rejects_unexpected_root_children() {
    let xml = include_str!("fixtures/shape/orders_lines_shape.xml")
        .replace("<rs:data>", "<extra/>\n<rs:data>");

    let err = parse_ado_xml_bytes(xml.as_bytes())
        .expect_err("unexpected shaped root child should be rejected");
    let message = format!("{err:#}");
    assert!(
        message.contains("unexpected ADO XML xml child element extra"),
        "{message}"
    );
}

#[test]
fn shaped_xml_rejects_multiple_row_schemas() {
    let xml = include_str!("fixtures/shape/orders_lines_shape.xml").replace(
        "</s:Schema>",
        "<s:ElementType name='row' content='eltOnly'><s:extends type='rs:rowbase'/></s:ElementType></s:Schema>",
    );

    let err = parse_ado_xml_bytes(xml.as_bytes())
        .expect_err("duplicate shaped row schema should be rejected");
    let message = format!("{err:#}");
    assert!(
        message.contains("ADO XML contained multiple row schemas"),
        "{message}"
    );
}

#[test]
fn shaped_xml_rejects_chapter_schema_without_visible_fields() {
    let xml = r##"<?xml version="1.0"?>
<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"
     xmlns:rs="urn:schemas-microsoft-com:rowset"
     xmlns:z="#RowsetSchema">
  <s:Schema id="RowsetSchema">
    <s:ElementType name="row" content="eltOnly">
      <s:ElementType name="Lines" content="eltOnly">
        <s:extends type="rs:rowbase"/>
      </s:ElementType>
      <s:extends type="rs:rowbase"/>
    </s:ElementType>
  </s:Schema>
  <rs:data>
    <z:row/>
  </rs:data>
</xml>"##;

    let err = parse_ado_xml_bytes(xml.as_bytes())
        .expect_err("chapter schema with no visible fields should be rejected");
    let message = format!("{err:#}");
    assert!(
        message.contains("ADO XML Lines schema had no visible fields"),
        "{message}"
    );
}

#[test]
fn shaped_adtg_parses_chapter_recordsets() {
    let adtg = parse_adtg_bytes(include_bytes!("fixtures/shape/orders_lines_shape.adtg"))
        .expect("failed to parse shaped ADTG");

    assert_shaped_recordset_rows(&adtg);
    let child = first_chapter(&adtg);
    assert_eq!(
        materialize_default_view(child).fields[1].attribute_flags,
        0x8000 | 0x10 | 0x08,
        "ADTG child LineId preserves the key-column field flag"
    );
}

#[test]
fn shaped_xml_parses_multiple_chapter_recordsets() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_lines_payments_shape.xml"
    ))
    .expect("failed to parse multi-chapter shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_lines_payments_shape.roundtrip.xml"
    ))
    .expect("failed to parse multi-chapter shaped roundtrip XML");

    assert_eq!(source, roundtrip, "source and roundtrip shaped XML");
    assert_multi_chapter_recordset_rows(&source);
    assert_eq!(source.fields[4].name, "Lines");
    assert_eq!(source.fields[5].name, "Payments");
    assert_eq!(source.fields[4].ado_type.map(|ty| ty.code), Some(136));
    assert_eq!(source.fields[5].ado_type.map(|ty| ty.code), Some(136));
}

#[test]
fn shaped_adtg_parses_multiple_chapter_recordsets() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_lines_payments_shape.adtg"
    ))
    .expect("failed to parse multi-chapter shaped ADTG");

    assert_multi_chapter_recordset_rows(&adtg);
    let lines = chapter_at(&adtg.rows[0].values, 4, "Lines");
    let payments = chapter_at(&adtg.rows[0].values, 5, "Payments");
    assert_eq!(
        materialize_default_view(lines).fields[1].attribute_flags,
        0x8000 | 0x10 | 0x08,
        "ADTG child LineId preserves the key-column field flag"
    );
    assert_eq!(
        materialize_default_view(payments).fields[1].attribute_flags,
        0x8000 | 0x10 | 0x08,
        "ADTG child PaymentId preserves the key-column field flag"
    );
}

#[test]
fn shaped_xml_parses_append_aggregate_columns() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_lines_aggregate_shape.xml"
    ))
    .expect("failed to parse aggregate shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_lines_aggregate_shape.roundtrip.xml"
    ))
    .expect("failed to parse aggregate shaped roundtrip XML");

    assert_eq!(
        source, roundtrip,
        "source and roundtrip aggregate shaped XML"
    );
    assert_aggregate_chapter_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_append_aggregate_columns() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_lines_aggregate_shape.adtg"
    ))
    .expect("failed to parse aggregate shaped ADTG");

    assert_aggregate_chapter_recordset_rows(&adtg);
    assert_eq!(adtg.fields[4].base_column, None);
    assert_eq!(adtg.fields[4].ado_type.map(|ty| ty.code), Some(6));
    assert_eq!(
        adtg.fields[4].attributes,
        vec![FieldAttribute::Fixed, FieldAttribute::MayBeNull],
        "ADTG aggregate descriptor preserves fixed/may-null flags"
    );
    let lines = chapter_at(&adtg.rows[0].values, 3, "Lines");
    assert_eq!(
        materialize_default_view(lines).fields[1].attribute_flags,
        0x8000 | 0x10 | 0x08,
        "ADTG aggregate-shape child LineId preserves the key-column field flag"
    );
}

#[test]
fn shaped_xml_parses_statistics_aggregate_columns() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_lines_statistics_shape.xml"
    ))
    .expect("failed to parse statistics shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_lines_statistics_shape.roundtrip.xml"
    ))
    .expect("failed to parse statistics shaped roundtrip XML");

    assert_eq!(
        source, roundtrip,
        "source and roundtrip statistics shaped XML"
    );
    assert_statistics_aggregate_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_statistics_aggregate_columns() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_lines_statistics_shape.adtg"
    ))
    .expect("failed to parse statistics shaped ADTG");

    assert_statistics_aggregate_recordset_rows(&adtg);
    assert_eq!(adtg.fields[4].ado_type.map(|ty| ty.code), Some(5));
    assert_eq!(adtg.fields[5].ado_type.map(|ty| ty.code), Some(5));
    assert_eq!(
        adtg.fields[4].attributes,
        vec![FieldAttribute::Fixed, FieldAttribute::MayBeNull],
        "ADTG AVG descriptor preserves fixed/may-null flags"
    );
    assert_eq!(
        adtg.fields[6].attributes,
        vec![FieldAttribute::Fixed, FieldAttribute::MayBeNull],
        "ADTG COUNT(alias) descriptor preserves fixed/may-null flags"
    );
}

#[test]
fn shaped_xml_parses_grandchild_aggregate_columns() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_lines_product_aggregate_shape.xml"
    ))
    .expect("failed to parse grandchild aggregate shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_lines_product_aggregate_shape.roundtrip.xml"
    ))
    .expect("failed to parse grandchild aggregate shaped roundtrip XML");

    assert_eq!(
        source, roundtrip,
        "source and roundtrip grandchild aggregate shaped XML"
    );
    assert_grandchild_aggregate_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_grandchild_aggregate_columns() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_lines_product_aggregate_shape.adtg"
    ))
    .expect("failed to parse grandchild aggregate shaped ADTG");

    assert_grandchild_aggregate_recordset_rows(&adtg);
    assert_eq!(adtg.fields[4].ado_type.map(|ty| ty.code), Some(6));
    assert_eq!(adtg.fields[5].ado_type.map(|ty| ty.code), Some(3));
    assert_eq!(
        adtg.fields[4].attributes,
        vec![FieldAttribute::Fixed, FieldAttribute::MayBeNull],
        "ADTG grandchild SUM descriptor preserves fixed/may-null flags"
    );
    let lines = chapter_at(&adtg.rows[0].values, 3, "Lines");
    let product = chapter_at(&lines.rows[0].values, 5, "Product");
    assert_eq!(
        materialize_default_view(product).fields[0].attribute_flags,
        0x8000 | 0x10 | 0x08,
        "ADTG grandchild aggregate ProductId preserves the key-column field flag"
    );
}

#[test]
fn shaped_xml_parses_sparse_aggregate_columns() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_sparse_aggregate_shape.xml"
    ))
    .expect("failed to parse sparse aggregate shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_sparse_aggregate_shape.roundtrip.xml"
    ))
    .expect("failed to parse sparse aggregate shaped roundtrip XML");

    assert_eq!(
        source, roundtrip,
        "source and roundtrip sparse aggregate shaped XML"
    );
    assert_sparse_aggregate_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_sparse_aggregate_columns() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_sparse_aggregate_shape.adtg"
    ))
    .expect("failed to parse sparse aggregate shaped ADTG");

    assert_sparse_aggregate_recordset_rows(&adtg);
    assert_eq!(adtg.fields[4].ado_type.map(|ty| ty.code), Some(6));
    assert_eq!(adtg.fields[5].ado_type.map(|ty| ty.code), Some(5));
    assert_eq!(
        adtg.fields[4].attributes,
        vec![FieldAttribute::Fixed, FieldAttribute::MayBeNull],
        "ADTG sparse SUM descriptor preserves fixed/may-null flags"
    );
    assert_eq!(
        adtg.fields[6].attributes,
        vec![FieldAttribute::Fixed, FieldAttribute::MayBeNull],
        "ADTG sparse COUNT(alias) descriptor preserves fixed/may-null flags"
    );
}

#[test]
fn shaped_xml_parses_compute_group_hierarchy() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/customer_lines_compute_shape.xml"
    ))
    .expect("failed to parse compute shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/customer_lines_compute_shape.roundtrip.xml"
    ))
    .expect("failed to parse compute shaped roundtrip XML");

    assert_eq!(source, roundtrip, "source and roundtrip compute shaped XML");
    assert_compute_group_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_compute_group_hierarchy() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/customer_lines_compute_shape.adtg"
    ))
    .expect("failed to parse compute shaped ADTG");

    assert_compute_group_recordset_rows(&adtg);
    assert_eq!(adtg.fields[0].name, "Lines");
    assert_eq!(adtg.fields[0].ado_type.map(|ty| ty.code), Some(136));
    assert_eq!(adtg.fields[1].base_column, None);
    assert_eq!(
        adtg.fields[1].attributes,
        vec![FieldAttribute::Fixed, FieldAttribute::MayBeNull],
        "ADTG compute aggregate descriptor preserves fixed/may-null flags"
    );
    let lines = chapter_at(&adtg.rows[0].values, 0, "Lines");
    assert_eq!(
        materialize_default_view(lines).fields[2].attribute_flags,
        0x8000 | 0x10 | 0x08,
        "ADTG compute child LineId preserves the key-column field flag"
    );
}

#[test]
fn shaped_xml_parses_calc_and_new_columns() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_child_calc_new_shape.xml"
    ))
    .expect("failed to parse CALC/NEW shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_child_calc_new_shape.roundtrip.xml"
    ))
    .expect("failed to parse CALC/NEW shaped roundtrip XML");

    assert_eq!(
        source, roundtrip,
        "source and roundtrip CALC/NEW shaped XML"
    );
    assert_calc_new_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_calc_and_new_columns() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_child_calc_new_shape.adtg"
    ))
    .expect("failed to parse CALC/NEW shaped ADTG");

    assert_calc_new_recordset_rows(&adtg);
    assert_eq!(
        adtg.fields[3].attributes,
        vec![FieldAttribute::Fixed, FieldAttribute::MayBeNull],
        "ADTG CALC descriptor preserves fixed/may-null flags"
    );
    assert_eq!(
        adtg.fields[4].attributes,
        vec![
            FieldAttribute::IsNullable,
            FieldAttribute::MayBeNull,
            FieldAttribute::Updatable
        ],
        "ADTG NEW adVarWChar descriptor preserves nullable/updatable flags"
    );
    let lines = chapter_at(&adtg.rows[0].values, 5, "Lines");
    assert_eq!(
        lines.fields[5].attributes,
        vec![FieldAttribute::Fixed, FieldAttribute::MayBeNull],
        "ADTG child CALC descriptor preserves fixed/may-null flags"
    );
    assert_eq!(
        lines.fields[6].attributes,
        vec![
            FieldAttribute::Fixed,
            FieldAttribute::IsNullable,
            FieldAttribute::MayBeNull,
            FieldAttribute::Updatable
        ],
        "ADTG child NEW adInteger descriptor preserves nullable/updatable flags"
    );
}

#[test]
fn shaped_adtg_parses_pending_calc_new_values() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_calc_new_pending_shape.adtg"
    ))
    .expect("failed to parse pending CALC/NEW shaped ADTG");

    assert_calc_new_pending_recordset_rows(&adtg);
}

#[test]
fn shaped_xml_parses_composite_chapter_relation() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_lines_composite_shape.xml"
    ))
    .expect("failed to parse composite shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_lines_composite_shape.roundtrip.xml"
    ))
    .expect("failed to parse composite shaped roundtrip XML");

    assert_eq!(
        source, roundtrip,
        "source and roundtrip composite shaped XML"
    );
    assert_composite_chapter_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_composite_chapter_relation() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_lines_composite_shape.adtg"
    ))
    .expect("failed to parse composite shaped ADTG");

    assert_composite_chapter_recordset_rows(&adtg);
}

#[test]
fn shaped_xml_parses_date_currency_chapter_relations() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_date_currency_relation_shape.xml"
    ))
    .expect("failed to parse date/currency shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_date_currency_relation_shape.roundtrip.xml"
    ))
    .expect("failed to parse date/currency shaped roundtrip XML");

    assert_eq!(
        source, roundtrip,
        "source and roundtrip date/currency shaped XML"
    );
    assert_date_currency_relation_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_date_currency_chapter_relations() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_date_currency_relation_shape.adtg"
    ))
    .expect("failed to parse date/currency shaped ADTG");

    assert_date_currency_relation_recordset_rows(&adtg);
}

#[test]
fn shaped_xml_parses_smalldatetime_smallmoney_chapter_relations() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_products_smalldatetime_smallmoney_relation_shape.xml"
    ))
    .expect("failed to parse smalldatetime/smallmoney shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_products_smalldatetime_smallmoney_relation_shape.roundtrip.xml"
    ))
    .expect("failed to parse smalldatetime/smallmoney shaped roundtrip XML");

    assert_eq!(
        source, roundtrip,
        "source and roundtrip smalldatetime/smallmoney shaped XML"
    );
    assert_smalldatetime_smallmoney_relation_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_smalldatetime_smallmoney_chapter_relations() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_products_smalldatetime_smallmoney_relation_shape.adtg"
    ))
    .expect("failed to parse smalldatetime/smallmoney shaped ADTG");

    assert_smalldatetime_smallmoney_relation_recordset_rows(&adtg);
}

#[test]
fn shaped_xml_parses_tinyint_smallint_chapter_relations() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/customers_orders_tinyint_smallint_relation_shape.xml"
    ))
    .expect("failed to parse tinyint/smallint shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/customers_orders_tinyint_smallint_relation_shape.roundtrip.xml"
    ))
    .expect("failed to parse tinyint/smallint shaped roundtrip XML");

    assert_eq!(
        source, roundtrip,
        "source and roundtrip tinyint/smallint shaped XML"
    );
    assert_tinyint_smallint_relation_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_tinyint_smallint_chapter_relations() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/customers_orders_tinyint_smallint_relation_shape.adtg"
    ))
    .expect("failed to parse tinyint/smallint shaped ADTG");

    assert_tinyint_smallint_relation_recordset_rows(&adtg);
}

#[test]
fn shaped_xml_parses_bigint_chapter_relation() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/lines_legacy_bigint_relation_shape.xml"
    ))
    .expect("failed to parse bigint shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/lines_legacy_bigint_relation_shape.roundtrip.xml"
    ))
    .expect("failed to parse bigint shaped roundtrip XML");

    assert_eq!(source, roundtrip, "source and roundtrip bigint shaped XML");
    assert_bigint_relation_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_bigint_chapter_relation() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/lines_legacy_bigint_relation_shape.adtg"
    ))
    .expect("failed to parse bigint shaped ADTG");

    assert_bigint_relation_recordset_rows(&adtg);
    let legacy = chapter_at(&adtg.rows[0].values, 3, "Legacy");
    assert!(
        legacy
            .fields
            .iter()
            .all(|field| field.name != "LegacyRowVersion"),
        "provider-added hidden rowversion suffix should not be visible"
    );
}

#[test]
fn shaped_xml_parses_decimal_numeric_chapter_relations() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/lines_decimal_numeric_relation_shape.xml"
    ))
    .expect("failed to parse decimal/numeric shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/lines_decimal_numeric_relation_shape.roundtrip.xml"
    ))
    .expect("failed to parse decimal/numeric shaped roundtrip XML");

    assert_eq!(
        source, roundtrip,
        "source and roundtrip decimal/numeric shaped XML"
    );
    assert_decimal_numeric_relation_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_decimal_numeric_chapter_relations() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/lines_decimal_numeric_relation_shape.adtg"
    ))
    .expect("failed to parse decimal/numeric shaped ADTG");

    assert_decimal_numeric_relation_recordset_rows(&adtg);
}

#[test]
fn shaped_xml_parses_real_float_chapter_relations() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/products_real_float_relation_shape.xml"
    ))
    .expect("failed to parse real/float shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/products_real_float_relation_shape.roundtrip.xml"
    ))
    .expect("failed to parse real/float shaped roundtrip XML");

    assert_eq!(
        source, roundtrip,
        "source and roundtrip real/float shaped XML"
    );
    assert_real_float_relation_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_real_float_chapter_relations() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/products_real_float_relation_shape.adtg"
    ))
    .expect("failed to parse real/float shaped ADTG");

    assert_real_float_relation_recordset_rows(&adtg);
}

#[test]
fn shaped_xml_parses_guid_chapter_relation() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/customers_orders_guid_shape.xml"
    ))
    .expect("failed to parse GUID-relation shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/customers_orders_guid_shape.roundtrip.xml"
    ))
    .expect("failed to parse GUID-relation shaped roundtrip XML");

    assert_eq!(source, roundtrip, "source and roundtrip GUID shaped XML");
    assert_guid_chapter_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_guid_chapter_relation() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/customers_orders_guid_shape.adtg"
    ))
    .expect("failed to parse GUID-relation shaped ADTG");

    assert_guid_chapter_recordset_rows(&adtg);
    assert_eq!(adtg.fields[1].base_column.as_deref(), Some("CustomerGuid"));
    let orders = chapter_at(&adtg.rows[0].values, 3, "Orders");
    assert_eq!(
        orders.fields[0].base_column.as_deref(),
        Some("CustomerGuid")
    );
    assert_eq!(orders.fields[1].base_column.as_deref(), Some("OrderId"));
    assert_eq!(
        materialize_default_view(orders).fields[1].attribute_flags,
        0x8000 | 0x10 | 0x08,
        "ADTG GUID-relation child OrderId preserves the key-column field flag"
    );
}

#[test]
fn shaped_xml_parses_text_chapter_relation() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/regions_customers_text_shape.xml"
    ))
    .expect("failed to parse text-relation shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/regions_customers_text_shape.roundtrip.xml"
    ))
    .expect("failed to parse text-relation shaped roundtrip XML");

    assert_eq!(source, roundtrip, "source and roundtrip text shaped XML");
    assert_text_chapter_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_text_chapter_relation() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/regions_customers_text_shape.adtg"
    ))
    .expect("failed to parse text-relation shaped ADTG");

    assert_text_chapter_recordset_rows(&adtg);
    assert_eq!(adtg.fields[0].base_column.as_deref(), Some("RegionCode"));
    let customers = chapter_at(&adtg.rows[0].values, 3, "Customers");
    assert_eq!(
        customers.fields[0].base_column.as_deref(),
        Some("RegionCode")
    );
    assert_eq!(
        customers.fields[1].base_column.as_deref(),
        Some("CustomerId")
    );
    assert_eq!(
        materialize_default_view(customers).fields[1].attribute_flags,
        0x8000 | 0x10 | 0x08,
        "ADTG text-relation child CustomerId preserves the key-column field flag"
    );
}

#[test]
fn shaped_xml_parses_unicode_chapter_relation() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/customers_orders_unicode_shape.xml"
    ))
    .expect("failed to parse Unicode-relation shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/customers_orders_unicode_shape.roundtrip.xml"
    ))
    .expect("failed to parse Unicode-relation shaped roundtrip XML");

    assert_eq!(
        source, roundtrip,
        "source and roundtrip Unicode-relation shaped XML"
    );
    assert_unicode_relation_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_unicode_chapter_relation() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/customers_orders_unicode_shape.adtg"
    ))
    .expect("failed to parse Unicode-relation shaped ADTG");

    assert_unicode_relation_recordset_rows(&adtg);
}

#[test]
fn shaped_xml_parses_binary_chapter_relation() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/products_lines_binary_shape.xml"
    ))
    .expect("failed to parse binary-relation shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/products_lines_binary_shape.roundtrip.xml"
    ))
    .expect("failed to parse binary-relation shaped roundtrip XML");

    assert_eq!(source, roundtrip, "source and roundtrip binary shaped XML");
    assert_binary_chapter_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_binary_chapter_relation() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/products_lines_binary_shape.adtg"
    ))
    .expect("failed to parse binary-relation shaped ADTG");

    assert_binary_chapter_recordset_rows(&adtg);
    assert_eq!(adtg.fields[1].base_column.as_deref(), Some("ProductSku"));
    let lines = chapter_at(&adtg.rows[0].values, 3, "Lines");
    assert_eq!(lines.fields[0].base_column.as_deref(), Some("ProductSku"));
    assert_eq!(lines.fields[1].base_column.as_deref(), Some("LineId"));
    assert_eq!(
        materialize_default_view(lines).fields[1].attribute_flags,
        0x8000 | 0x10 | 0x08,
        "ADTG binary-relation child LineId preserves the key-column field flag"
    );
}

#[test]
fn shaped_xml_parses_rowversion_chapter_relation() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/legacy_rowversion_relation_shape.xml"
    ))
    .expect("failed to parse rowversion-relation shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/legacy_rowversion_relation_shape.roundtrip.xml"
    ))
    .expect("failed to parse rowversion-relation shaped roundtrip XML");

    assert_eq!(
        source, roundtrip,
        "source and roundtrip rowversion shaped XML"
    );
    assert_rowversion_chapter_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_rowversion_chapter_relation() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/legacy_rowversion_relation_shape.adtg"
    ))
    .expect("failed to parse rowversion-relation shaped ADTG");

    assert_rowversion_chapter_recordset_rows(&adtg);
    assert_eq!(
        adtg.fields[0].base_column.as_deref(),
        Some("LegacyRowVersion")
    );
    let rows = chapter_at(&adtg.rows[0].values, 2, "LegacyRows");
    assert_eq!(
        rows.fields[0].base_column.as_deref(),
        Some("LegacyRowVersion")
    );
}

#[test]
fn shaped_xml_parses_boolean_chapter_relation() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/regions_customers_boolean_shape.xml"
    ))
    .expect("failed to parse boolean-relation shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/regions_customers_boolean_shape.roundtrip.xml"
    ))
    .expect("failed to parse boolean-relation shaped roundtrip XML");

    assert_eq!(
        source, roundtrip,
        "source and roundtrip boolean-relation shaped XML"
    );
    assert_boolean_relation_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_boolean_chapter_relation() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/regions_customers_boolean_shape.adtg"
    ))
    .expect("failed to parse boolean-relation shaped ADTG");

    assert_boolean_relation_recordset_rows(&adtg);
}

#[test]
fn shaped_xml_parses_nullable_key_chapter_relation() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/customers_orders_nullable_key_shape.xml"
    ))
    .expect("failed to parse nullable-key shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/customers_orders_nullable_key_shape.roundtrip.xml"
    ))
    .expect("failed to parse nullable-key shaped roundtrip XML");

    assert_eq!(
        source, roundtrip,
        "source and roundtrip nullable-key shaped XML"
    );
    assert_nullable_key_chapter_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_nullable_key_chapter_relation() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/customers_orders_nullable_key_shape.adtg"
    ))
    .expect("failed to parse nullable-key shaped ADTG");

    assert_nullable_key_chapter_recordset_rows(&adtg);
    assert_eq!(adtg.fields[1].base_column.as_deref(), None);
    let orders = chapter_at(&adtg.rows[0].values, 3, "Orders");
    assert_eq!(orders.fields[0].base_column.as_deref(), None);
    assert_eq!(orders.fields[1].base_column.as_deref(), Some("OrderId"));
    assert_eq!(
        materialize_default_view(orders).fields[1].attribute_flags,
        0x8000 | 0x10 | 0x08,
        "ADTG nullable-key child OrderId preserves the key-column field flag"
    );
}

#[test]
fn shaped_xml_parses_duplicate_parent_key_chapter_relation() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/customers_orders_duplicate_parent_shape.xml"
    ))
    .expect("failed to parse duplicate-parent shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/customers_orders_duplicate_parent_shape.roundtrip.xml"
    ))
    .expect("failed to parse duplicate-parent shaped roundtrip XML");

    assert_eq!(
        source, roundtrip,
        "source and roundtrip duplicate-parent shaped XML"
    );
    assert_duplicate_parent_chapter_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_duplicate_parent_key_chapter_relation() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/customers_orders_duplicate_parent_shape.adtg"
    ))
    .expect("failed to parse duplicate-parent shaped ADTG");

    assert_duplicate_parent_chapter_recordset_rows(&adtg);
    assert_eq!(adtg.fields[0].base_column.as_deref(), Some("RegionCode"));
    let orders = chapter_at(&adtg.rows[0].values, 3, "Orders");
    assert_eq!(orders.fields[0].base_column.as_deref(), Some("RegionCode"));
    assert_eq!(orders.fields[1].base_column.as_deref(), Some("OrderId"));
    assert_eq!(
        materialize_default_view(orders).fields[1].attribute_flags,
        0x8000 | 0x10 | 0x08,
        "ADTG duplicate-parent child OrderId preserves the key-column field flag"
    );
}

#[test]
fn shaped_xml_parses_nullable_chapter_rows() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_nullable_lines_shape.xml"
    ))
    .expect("failed to parse nullable shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_nullable_lines_shape.roundtrip.xml"
    ))
    .expect("failed to parse nullable shaped roundtrip XML");

    assert_eq!(
        source, roundtrip,
        "source and roundtrip nullable shaped XML"
    );
    assert_nullable_chapter_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_nullable_chapter_rows() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_nullable_lines_shape.adtg"
    ))
    .expect("failed to parse nullable shaped ADTG");

    assert_nullable_chapter_recordset_rows(&adtg);
}

#[test]
fn shaped_xml_parses_wide_nullable_chapter_mask() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_wide_nullable_lines_shape.xml"
    ))
    .expect("failed to parse wide nullable shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_wide_nullable_lines_shape.roundtrip.xml"
    ))
    .expect("failed to parse wide nullable shaped roundtrip XML");

    assert_eq!(
        source, roundtrip,
        "source and roundtrip wide nullable shaped XML"
    );
    assert_wide_nullable_chapter_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_wide_nullable_chapter_mask() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_wide_nullable_lines_shape.adtg"
    ))
    .expect("failed to parse wide nullable shaped ADTG");

    assert_wide_nullable_chapter_recordset_rows(&adtg);
}

#[test]
fn shaped_xml_parses_sparse_child_chapters() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_sparse_lines_shape.xml"
    ))
    .expect("failed to parse sparse child shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_sparse_lines_shape.roundtrip.xml"
    ))
    .expect("failed to parse sparse child shaped roundtrip XML");

    assert_eq!(
        source, roundtrip,
        "source and roundtrip sparse child shaped XML"
    );
    assert_sparse_child_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_sparse_child_chapters() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_sparse_lines_shape.adtg"
    ))
    .expect("failed to parse sparse child shaped ADTG");

    assert_sparse_child_recordset_rows(&adtg);
}

#[test]
fn shaped_adtg_parses_mdac_xml_resaved_source_column_descriptors() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_sparse_lines_shape.xml_resaved.adtg"
    ))
    .expect("failed to parse sparse child shaped ADTG resaved from XML by MDAC");

    assert_eq!(
        adtg.fields[0].base_column.as_deref(),
        Some("OrderId"),
        "0xf0 source-column descriptor should preserve the parent source column"
    );
    let Value::Chapter(lines) = &adtg.rows[0].values[3] else {
        panic!("expected Lines chapter in XML-resaved ADTG");
    };
    assert_eq!(
        lines.fields[0].base_column.as_deref(),
        Some("OrderId"),
        "0xf0 source-column descriptor should preserve the child source column"
    );
    assert_sparse_child_recordset_rows(&adtg);
}

#[test]
fn shaped_xml_parses_empty_child_group() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_empty_lines_shape.xml"
    ))
    .expect("failed to parse empty child shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_empty_lines_shape.roundtrip.xml"
    ))
    .expect("failed to parse empty child shaped roundtrip XML");

    assert_eq!(
        source, roundtrip,
        "source and roundtrip empty child shaped XML"
    );
    assert_empty_child_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_empty_child_group() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_empty_lines_shape.adtg"
    ))
    .expect("failed to parse empty child shaped ADTG");

    assert_empty_child_recordset_rows(&adtg);
}

#[test]
fn shaped_xml_parses_empty_parent_chapter_schema() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_empty_parent_lines_shape.xml"
    ))
    .expect("failed to parse empty-parent shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_empty_parent_lines_shape.roundtrip.xml"
    ))
    .expect("failed to parse empty-parent shaped roundtrip XML");

    assert_eq!(
        source, roundtrip,
        "source and roundtrip empty-parent shaped XML"
    );
    assert_empty_parent_recordset_schema(&source);
}

#[test]
fn shaped_adtg_parses_empty_parent_chapter_schema() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_empty_parent_lines_shape.adtg"
    ))
    .expect("failed to parse empty-parent shaped ADTG");

    assert_empty_parent_recordset_schema(&adtg);
    assert_eq!(adtg.fields[0].base_column.as_deref(), Some("OrderId"));
    assert_eq!(adtg.fields[1].base_column.as_deref(), Some("CustomerId"));
    assert_eq!(adtg.fields[2].base_column.as_deref(), Some("Freight"));
    assert_eq!(adtg.fields[3].base_column.as_deref(), None);
}

#[test]
fn shaped_adtg_parses_pending_parent_and_child_changes() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_pending_changes_shape.adtg"
    ))
    .expect("failed to parse pending-change shaped ADTG");

    let default = materialize_default_view(&adtg);
    assert_eq!(default.rows.len(), 2, "default parent row count");
    assert_eq!(default.rows[0].status, RecordStatusFlag::Modified);
    assert_eq!(default.rows[1].status, RecordStatusFlag::Unmodified);
    assert_eq!(default.rows[0].values[0], Value::Integer(100001));
    assert_eq!(
        default.rows[0].values[2],
        Value::Decimal("123.45".to_string())
    );

    let first_lines = chapter_at(&default.rows[0].values, 3, "Lines");
    let first_lines_default = materialize_default_view(first_lines);
    assert_eq!(
        first_lines_default
            .rows
            .iter()
            .map(|row| row.status)
            .collect::<Vec<_>>(),
        vec![
            RecordStatusFlag::Modified,
            RecordStatusFlag::Unmodified,
            RecordStatusFlag::New,
        ],
        "default child statuses"
    );
    assert_eq!(
        first_lines_default
            .rows
            .iter()
            .map(|row| row.values[1].clone())
            .collect::<Vec<_>>(),
        vec![
            Value::Integer(1000011),
            Value::Integer(1000013),
            Value::Integer(1999999999),
        ],
        "default child LineId values"
    );
    assert_eq!(
        first_lines_default
            .rows
            .iter()
            .map(|row| row.values[3].clone())
            .collect::<Vec<_>>(),
        vec![Value::Integer(777), Value::Integer(6), Value::Integer(42)],
        "default child Quantity values"
    );

    let pending = materialize_pending_view(&adtg);
    assert_eq!(pending.rows.len(), 1, "pending parent row count");
    assert_eq!(pending.rows[0].status, RecordStatusFlag::Modified);

    let pending_lines = chapter_at(&pending.rows[0].values, 3, "Lines");
    let child_pending = materialize_pending_view(pending_lines);
    assert_eq!(
        child_pending
            .rows
            .iter()
            .map(|row| row.status)
            .collect::<Vec<_>>(),
        vec![
            RecordStatusFlag::Deleted,
            RecordStatusFlag::Modified,
            RecordStatusFlag::New,
        ],
        "pending child statuses"
    );
    assert_eq!(
        child_pending
            .rows
            .iter()
            .map(|row| row.values[1].clone())
            .collect::<Vec<_>>(),
        vec![
            Value::Integer(1000012),
            Value::Integer(1000011),
            Value::Integer(1999999999),
        ],
        "pending child LineId values"
    );
}

#[test]
fn shaped_adtg_parses_parent_insert_delete_changes() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_parent_insert_delete_shape.adtg"
    ))
    .expect("failed to parse parent insert/delete shaped ADTG");

    let default = materialize_default_view(&adtg);
    assert_eq!(default.rows.len(), 3, "default parent row count");
    assert_eq!(
        default
            .rows
            .iter()
            .map(|row| row.status)
            .collect::<Vec<_>>(),
        vec![
            RecordStatusFlag::Unmodified,
            RecordStatusFlag::Unmodified,
            RecordStatusFlag::New,
        ],
        "default parent statuses"
    );
    assert_eq!(
        default
            .rows
            .iter()
            .map(|row| row.values[0].clone())
            .collect::<Vec<_>>(),
        vec![
            Value::Integer(100002),
            Value::Integer(100003),
            Value::Integer(199999),
        ],
        "default parent OrderId values"
    );
    assert_eq!(
        default.rows[2].values[2],
        Value::Decimal("77.77".to_string()),
        "inserted parent Freight"
    );

    let inserted_lines = chapter_at(&default.rows[2].values, 3, "Lines");
    assert!(
        inserted_lines.rows.is_empty(),
        "inserted parent should have an empty child chapter"
    );

    let pending = materialize_pending_view(&adtg);
    assert_eq!(pending.rows.len(), 2, "pending parent row count");
    assert_eq!(
        pending
            .rows
            .iter()
            .map(|row| row.status)
            .collect::<Vec<_>>(),
        vec![RecordStatusFlag::Deleted, RecordStatusFlag::New],
        "pending parent statuses"
    );
    assert_eq!(pending.rows[0].values[0], Value::Integer(100001));
    assert_eq!(pending.rows[1].values[0], Value::Integer(199999));

    let deleted_lines = chapter_at(&pending.rows[0].values, 3, "Lines");
    assert_eq!(
        materialize_default_view(deleted_lines).rows.len(),
        3,
        "deleted parent should preserve its original child chapter rows"
    );
}

#[test]
fn shaped_adtg_parses_parent_relation_key_update_changes() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_parent_relation_key_update_shape.adtg"
    ))
    .expect("failed to parse parent relation-key update shaped ADTG");

    let default = materialize_default_view(&adtg);
    assert_eq!(default.rows.len(), 2, "default parent row count");
    assert_eq!(
        default
            .rows
            .iter()
            .map(|row| row.status)
            .collect::<Vec<_>>(),
        vec![RecordStatusFlag::Modified, RecordStatusFlag::Unmodified],
        "default parent statuses"
    );
    assert_eq!(
        default
            .rows
            .iter()
            .map(|row| row.values[0].clone())
            .collect::<Vec<_>>(),
        vec![Value::Integer(199998), Value::Integer(100002)],
        "default parent OrderId values"
    );
    assert_eq!(
        default.rows[0].values[2],
        Value::Decimal("123.45".to_string()),
        "updated parent Freight"
    );

    let updated_parent_lines = chapter_at(&default.rows[0].values, 3, "Lines");
    assert!(
        materialize_default_view(updated_parent_lines)
            .rows
            .is_empty(),
        "parent relation-key update should not retain old child rows"
    );

    let second_lines = chapter_at(&default.rows[1].values, 3, "Lines");
    let second_lines_default = materialize_default_view(second_lines);
    assert_eq!(second_lines_default.rows.len(), 3);
    assert!(
        second_lines_default
            .rows
            .iter()
            .all(|row| row.values[0] == Value::Integer(100002)),
        "unmodified parent should retain its original child rows"
    );

    let pending = materialize_pending_view(&adtg);
    assert_eq!(pending.rows.len(), 1, "pending parent row count");
    assert_eq!(pending.rows[0].status, RecordStatusFlag::Modified);
    assert_eq!(pending.rows[0].values[0], Value::Integer(199998));
    let pending_lines = chapter_at(&pending.rows[0].values, 3, "Lines");
    assert!(
        materialize_default_view(pending_lines).rows.is_empty(),
        "pending relation-key update should expose an empty child chapter"
    );
}

#[test]
fn shaped_adtg_parses_child_relation_key_update_changes() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_child_relation_key_update_shape.adtg"
    ))
    .expect("failed to parse child relation-key update shaped ADTG");

    let default = materialize_default_view(&adtg);
    assert_eq!(default.rows.len(), 2, "default parent row count");
    assert_eq!(
        default
            .rows
            .iter()
            .map(|row| row.status)
            .collect::<Vec<_>>(),
        vec![RecordStatusFlag::Unmodified, RecordStatusFlag::Unmodified],
        "default parent statuses"
    );

    let first_lines = chapter_at(&default.rows[0].values, 3, "Lines");
    let first_lines_default = materialize_default_view(first_lines);
    assert_eq!(
        first_lines_default
            .rows
            .iter()
            .map(|row| row.values[1].clone())
            .collect::<Vec<_>>(),
        vec![Value::Integer(1000012), Value::Integer(1000013)],
        "child relation-key update should move the modified line out of the old parent"
    );

    let second_lines = chapter_at(&default.rows[1].values, 3, "Lines");
    let second_lines_default = materialize_default_view(second_lines);
    assert_eq!(
        second_lines_default
            .rows
            .iter()
            .map(|row| row.status)
            .collect::<Vec<_>>(),
        vec![
            RecordStatusFlag::Modified,
            RecordStatusFlag::Unmodified,
            RecordStatusFlag::Unmodified,
            RecordStatusFlag::Unmodified,
        ],
        "default child statuses after relation-key update"
    );
    assert_eq!(
        second_lines_default
            .rows
            .iter()
            .map(|row| row.values[0].clone())
            .collect::<Vec<_>>(),
        vec![
            Value::Integer(100002),
            Value::Integer(100002),
            Value::Integer(100002),
            Value::Integer(100002),
        ],
        "updated child rows should relate to the new parent key"
    );
    assert_eq!(
        second_lines_default
            .rows
            .iter()
            .map(|row| row.values[1].clone())
            .collect::<Vec<_>>(),
        vec![
            Value::Integer(1000011),
            Value::Integer(1000021),
            Value::Integer(1000022),
            Value::Integer(1000023),
        ],
        "default child LineId values after relation-key update"
    );
    assert_eq!(
        second_lines_default.rows[0].values[3],
        Value::Integer(115),
        "updated child Quantity"
    );

    assert!(
        materialize_pending_view(&adtg).rows.is_empty(),
        "nested-only relation-key changes should not create top-level pending rows"
    );
    let raw_second_lines = chapter_at(&adtg.rows[1].values, 3, "Lines");
    let child_pending = materialize_pending_view(raw_second_lines);
    assert_eq!(child_pending.rows.len(), 1);
    assert_eq!(child_pending.rows[0].status, RecordStatusFlag::Modified);
    assert_eq!(child_pending.rows[0].values[0], Value::Integer(100002));
    assert_eq!(child_pending.rows[0].values[1], Value::Integer(1000011));
    assert_eq!(child_pending.rows[0].values[3], Value::Integer(115));
}

#[test]
fn shaped_adtg_parses_composite_parent_relation_key_update_changes() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_composite_parent_relation_key_update_shape.adtg"
    ))
    .expect("failed to parse composite parent relation-key update shaped ADTG");

    let default = materialize_default_view(&adtg);
    assert_eq!(default.rows.len(), 2, "default parent row count");
    assert_eq!(
        default
            .rows
            .iter()
            .map(|row| row.status)
            .collect::<Vec<_>>(),
        vec![RecordStatusFlag::Modified, RecordStatusFlag::Unmodified],
        "default parent statuses"
    );
    assert_eq!(
        default
            .rows
            .iter()
            .map(|row| row.values[0].clone())
            .collect::<Vec<_>>(),
        vec![Value::Integer(100002), Value::Integer(100002)],
        "default parent OrderId values"
    );
    assert_eq!(
        default
            .rows
            .iter()
            .map(|row| row.values[1].clone())
            .collect::<Vec<_>>(),
        vec![Value::Integer(14), Value::Integer(14)],
        "default parent ProductId values"
    );

    for row in &default.rows {
        let lines = chapter_at(&row.values, 2, "Lines");
        let lines_default = materialize_default_view(lines);
        assert_eq!(
            lines_default.rows.len(),
            1,
            "each retargeted parent should expose the new composite-key child chapter"
        );
        assert_eq!(
            lines_default.rows[0].values,
            vec![
                Value::Integer(100002),
                Value::Integer(14),
                Value::Integer(1000021),
                Value::Integer(1),
                Value::Integer(5),
            ]
        );
    }

    let pending = materialize_pending_view(&adtg);
    assert_eq!(pending.rows.len(), 1, "pending parent row count");
    assert_eq!(pending.rows[0].status, RecordStatusFlag::Modified);
    assert_eq!(pending.rows[0].values[0], Value::Integer(100002));
    assert_eq!(pending.rows[0].values[1], Value::Integer(14));
    let pending_lines = chapter_at(&pending.rows[0].values, 2, "Lines");
    let pending_lines_default = materialize_default_view(pending_lines);
    assert_eq!(pending_lines_default.rows.len(), 1);
    assert_eq!(
        pending_lines_default.rows[0].values,
        vec![
            Value::Integer(100002),
            Value::Integer(14),
            Value::Integer(1000021),
            Value::Integer(1),
            Value::Integer(5),
        ]
    );
}

#[test]
fn shaped_adtg_parses_composite_child_relation_key_update_changes() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_composite_child_relation_key_update_shape.adtg"
    ))
    .expect("failed to parse composite child relation-key update shaped ADTG");

    let default = materialize_default_view(&adtg);
    assert_eq!(default.rows.len(), 2, "default parent row count");
    assert_eq!(
        default
            .rows
            .iter()
            .map(|row| row.status)
            .collect::<Vec<_>>(),
        vec![RecordStatusFlag::Unmodified, RecordStatusFlag::Unmodified],
        "default parent statuses"
    );
    assert_eq!(default.rows[0].values[0], Value::Integer(100001));
    assert_eq!(default.rows[0].values[1], Value::Integer(13));
    assert_eq!(default.rows[1].values[0], Value::Integer(100002));
    assert_eq!(default.rows[1].values[1], Value::Integer(14));

    let old_key_lines = chapter_at(&default.rows[0].values, 2, "Lines");
    assert!(
        materialize_default_view(old_key_lines).rows.is_empty(),
        "composite child relation-key update should move out of the old key pair"
    );

    let new_key_lines = chapter_at(&default.rows[1].values, 2, "Lines");
    let new_key_default = materialize_default_view(new_key_lines);
    assert_eq!(
        new_key_default
            .rows
            .iter()
            .map(|row| row.status)
            .collect::<Vec<_>>(),
        vec![RecordStatusFlag::Modified, RecordStatusFlag::Unmodified],
        "default child statuses after composite relation-key update"
    );
    assert_eq!(
        new_key_default
            .rows
            .iter()
            .map(|row| row.values.clone())
            .collect::<Vec<_>>(),
        vec![
            vec![
                Value::Integer(100002),
                Value::Integer(14),
                Value::Integer(1000011),
                Value::Integer(1),
                Value::Integer(226),
            ],
            vec![
                Value::Integer(100002),
                Value::Integer(14),
                Value::Integer(1000021),
                Value::Integer(1),
                Value::Integer(5),
            ],
        ],
        "default child values after composite relation-key update"
    );

    assert!(
        materialize_pending_view(&adtg).rows.is_empty(),
        "nested-only composite relation-key changes should not create top-level pending rows"
    );
    let raw_new_key_lines = chapter_at(&adtg.rows[1].values, 2, "Lines");
    let child_pending = materialize_pending_view(raw_new_key_lines);
    assert_eq!(child_pending.rows.len(), 1);
    assert_eq!(child_pending.rows[0].status, RecordStatusFlag::Modified);
    assert_eq!(
        child_pending.rows[0].values,
        vec![
            Value::Integer(100002),
            Value::Integer(14),
            Value::Integer(1000011),
            Value::Integer(1),
            Value::Integer(226),
        ]
    );
}

#[test]
fn shaped_adtg_parses_nested_pending_child_changes() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_lines_product_pending_shape.adtg"
    ))
    .expect("failed to parse nested pending-change shaped ADTG");

    let default = materialize_default_view(&adtg);
    assert_eq!(default.rows.len(), 2, "default parent row count");
    assert_eq!(
        default
            .rows
            .iter()
            .map(|row| row.status)
            .collect::<Vec<_>>(),
        vec![RecordStatusFlag::Unmodified, RecordStatusFlag::Unmodified],
        "default parent statuses"
    );
    assert!(
        materialize_pending_view(&adtg).rows.is_empty(),
        "nested-only pending changes should not create top-level pending rows"
    );

    let first_lines = chapter_at(&default.rows[0].values, 3, "Lines");
    let first_lines_default = materialize_default_view(first_lines);
    assert_eq!(
        first_lines_default
            .rows
            .iter()
            .map(|row| row.status)
            .collect::<Vec<_>>(),
        vec![
            RecordStatusFlag::Modified,
            RecordStatusFlag::Unmodified,
            RecordStatusFlag::Unmodified,
        ],
        "first parent default Lines statuses"
    );
    assert_eq!(
        first_lines_default
            .rows
            .iter()
            .map(|row| row.values[1].clone())
            .collect::<Vec<_>>(),
        vec![
            Value::Integer(1000011),
            Value::Integer(1000012),
            Value::Integer(1000013),
        ],
        "first parent default LineId values"
    );
    assert_eq!(
        first_lines_default
            .rows
            .iter()
            .map(|row| row.values[4].clone())
            .collect::<Vec<_>>(),
        vec![Value::Integer(888), Value::Integer(5), Value::Integer(6)],
        "first parent default Quantity values"
    );

    let modified_product = chapter_at(&first_lines_default.rows[0].values, 5, "Product");
    let modified_product_default = materialize_default_view(modified_product);
    assert_eq!(modified_product_default.rows.len(), 1);
    assert_eq!(
        modified_product_default.rows[0].status,
        RecordStatusFlag::Modified
    );
    assert_eq!(
        modified_product_default.rows[0].values[2],
        Value::Decimal("321.09".to_string())
    );

    let deleted_product = chapter_at(&first_lines_default.rows[1].values, 5, "Product");
    assert!(
        materialize_default_view(deleted_product).rows.is_empty(),
        "deleted nested product should be absent from default view"
    );
    let deleted_product_pending = materialize_pending_view(deleted_product);
    assert_eq!(deleted_product_pending.rows.len(), 1);
    assert_eq!(
        deleted_product_pending.rows[0].status,
        RecordStatusFlag::Deleted
    );
    assert_eq!(
        deleted_product_pending.rows[0].values[0],
        Value::Integer(14)
    );

    let second_lines = chapter_at(&default.rows[1].values, 3, "Lines");
    let second_lines_default = materialize_default_view(second_lines);
    let shared_deleted_product = chapter_at(&second_lines_default.rows[0].values, 5, "Product");
    assert!(
        materialize_default_view(shared_deleted_product)
            .rows
            .is_empty(),
        "global ProductId relation should apply the nested delete to every matching line"
    );
}

#[test]
fn shaped_adtg_parses_nested_sibling_grandchild_pending_changes() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_lines_product_legacy_pending_shape.adtg"
    ))
    .expect("failed to parse nested sibling-grandchild pending-change shaped ADTG");

    let default = materialize_default_view(&adtg);
    assert_eq!(default.rows.len(), 2, "default parent row count");
    assert_eq!(
        default
            .rows
            .iter()
            .map(|row| row.status)
            .collect::<Vec<_>>(),
        vec![RecordStatusFlag::Unmodified, RecordStatusFlag::Unmodified],
        "default parent statuses"
    );
    assert!(
        materialize_pending_view(&adtg).rows.is_empty(),
        "nested sibling-grandchild pending changes should not create top-level pending rows"
    );

    let first_lines = chapter_at(&default.rows[0].values, 3, "Lines");
    let first_lines_default = materialize_default_view(first_lines);
    assert_eq!(
        first_lines_default
            .rows
            .iter()
            .map(|row| row.status)
            .collect::<Vec<_>>(),
        vec![
            RecordStatusFlag::Modified,
            RecordStatusFlag::Unmodified,
            RecordStatusFlag::Unmodified,
        ],
        "first parent default Lines statuses"
    );
    assert_eq!(
        first_lines_default
            .rows
            .iter()
            .map(|row| row.values[4].clone())
            .collect::<Vec<_>>(),
        vec![Value::Integer(999), Value::Integer(5), Value::Integer(6)],
        "first parent default Quantity values"
    );

    let modified_product = chapter_at(&first_lines_default.rows[0].values, 6, "Product");
    let modified_product_default = materialize_default_view(modified_product);
    assert_eq!(modified_product_default.rows.len(), 1);
    assert_eq!(
        modified_product_default.rows[0].status,
        RecordStatusFlag::Modified
    );
    assert_eq!(
        modified_product_default.rows[0].values[3],
        Value::Decimal("456.78".to_string())
    );

    let modified_legacy = chapter_at(&first_lines_default.rows[0].values, 7, "Legacy");
    let modified_legacy_default = materialize_default_view(modified_legacy);
    assert_eq!(modified_legacy_default.rows.len(), 1);
    assert_eq!(
        modified_legacy_default.rows[0].status,
        RecordStatusFlag::Modified
    );
    assert_eq!(
        modified_legacy_default.rows[0].values[2],
        Value::String("PX0001".to_string())
    );

    let deleted_product = chapter_at(&first_lines_default.rows[1].values, 6, "Product");
    assert!(
        materialize_default_view(deleted_product).rows.is_empty(),
        "deleted sibling product should be absent from default view"
    );
    let deleted_product_pending = materialize_pending_view(deleted_product);
    assert_eq!(deleted_product_pending.rows.len(), 1);
    assert_eq!(
        deleted_product_pending.rows[0].status,
        RecordStatusFlag::Deleted
    );
    assert_eq!(
        deleted_product_pending.rows[0].values[0],
        Value::Integer(14)
    );

    let deleted_legacy = chapter_at(&first_lines_default.rows[1].values, 7, "Legacy");
    assert!(
        materialize_default_view(deleted_legacy).rows.is_empty(),
        "deleted sibling legacy row should be absent from default view"
    );
    let deleted_legacy_pending = materialize_pending_view(deleted_legacy);
    assert_eq!(deleted_legacy_pending.rows.len(), 1);
    assert_eq!(
        deleted_legacy_pending.rows[0].status,
        RecordStatusFlag::Deleted
    );
    assert_eq!(
        deleted_legacy_pending.rows[0].values[0],
        Value::Integer(1000012)
    );

    let second_lines = chapter_at(&default.rows[1].values, 3, "Lines");
    let second_lines_default = materialize_default_view(second_lines);
    let shared_deleted_product = chapter_at(&second_lines_default.rows[0].values, 6, "Product");
    assert!(
        materialize_default_view(shared_deleted_product)
            .rows
            .is_empty(),
        "global ProductId relation should apply the sibling product delete to every matching line"
    );
    let unaffected_legacy = chapter_at(&second_lines_default.rows[0].values, 7, "Legacy");
    let unaffected_legacy_default = materialize_default_view(unaffected_legacy);
    assert_eq!(unaffected_legacy_default.rows.len(), 1);
    assert_eq!(
        unaffected_legacy_default.rows[0].values[0],
        Value::Integer(1000021)
    );
}

#[test]
fn shaped_xml_parses_nested_chapter_recordsets() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_lines_product_shape.xml"
    ))
    .expect("failed to parse nested shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_lines_product_shape.roundtrip.xml"
    ))
    .expect("failed to parse nested shaped roundtrip XML");

    assert_eq!(source, roundtrip, "source and roundtrip nested shaped XML");
    assert_nested_chapter_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_nested_chapter_recordsets() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_lines_product_shape.adtg"
    ))
    .expect("failed to parse nested shaped ADTG");

    assert_nested_chapter_recordset_rows(&adtg);
    assert_eq!(adtg.fields[0].base_column.as_deref(), Some("OrderId"));
    assert_eq!(adtg.fields[1].base_column.as_deref(), Some("CustomerId"));
    assert_eq!(adtg.fields[3].base_column, None);
    let lines = chapter_at(&adtg.rows[0].values, 3, "Lines");
    assert_eq!(lines.fields[1].base_column.as_deref(), Some("LineId"));
    assert_eq!(lines.fields[5].base_column, None);
    assert_eq!(
        materialize_default_view(lines).fields[1].attribute_flags,
        0x8000 | 0x10 | 0x08,
        "ADTG nested child LineId preserves the key-column field flag"
    );
    let product = chapter_at(&lines.rows[0].values, 5, "Product");
    assert_eq!(product.fields[0].base_column.as_deref(), Some("ProductId"));
    assert_eq!(
        materialize_default_view(product).fields[0].attribute_flags,
        0x8000 | 0x10 | 0x08,
        "ADTG nested grandchild ProductId preserves the key-column field flag"
    );
}

#[test]
fn shaped_xml_parses_nested_sibling_grandchild_recordsets() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_lines_product_legacy_shape.xml"
    ))
    .expect("failed to parse nested sibling-grandchild shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_lines_product_legacy_shape.roundtrip.xml"
    ))
    .expect("failed to parse nested sibling-grandchild shaped roundtrip XML");

    assert_eq!(
        source, roundtrip,
        "source and roundtrip nested sibling-grandchild shaped XML"
    );
    assert_nested_sibling_grandchild_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_nested_sibling_grandchild_recordsets() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_lines_product_legacy_shape.adtg"
    ))
    .expect("failed to parse nested sibling-grandchild shaped ADTG");

    assert_nested_sibling_grandchild_recordset_rows(&adtg);
    let lines = chapter_at(&adtg.rows[0].values, 3, "Lines");
    assert_eq!(lines.fields[6].base_column, None);
    assert_eq!(lines.fields[7].base_column, None);
    let product = chapter_at(&lines.rows[0].values, 6, "Product");
    let legacy = chapter_at(&lines.rows[0].values, 7, "Legacy");
    assert_eq!(product.fields[0].base_column.as_deref(), Some("ProductId"));
    assert_eq!(legacy.fields[0].base_column.as_deref(), Some("LineId"));
    assert_eq!(
        materialize_default_view(product).fields[0].attribute_flags,
        0x8000 | 0x10 | 0x08,
        "ADTG nested sibling ProductId preserves the key-column field flag"
    );
    assert_eq!(
        materialize_default_view(legacy).fields[0].attribute_flags,
        0x10 | 0x08,
        "ADTG nested sibling Legacy LineId preserves fixed/unknown-updatable flags without key-column metadata"
    );
    assert!(
        legacy.fields[3]
            .attributes
            .contains(&FieldAttribute::RowVersion),
        "ADTG nested sibling Legacy rowversion preserves the row-version flag"
    );
}

#[test]
fn shaped_xml_parses_deep_nested_chapter_recordsets() {
    let source = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_lines_product_category_shape.xml"
    ))
    .expect("failed to parse deep nested shaped source XML");
    let roundtrip = parse_ado_xml_bytes(include_bytes!(
        "fixtures/shape/orders_lines_product_category_shape.roundtrip.xml"
    ))
    .expect("failed to parse deep nested shaped roundtrip XML");

    assert_eq!(
        source, roundtrip,
        "source and roundtrip deep nested shaped XML"
    );
    assert_deep_nested_chapter_recordset_rows(&source);
}

#[test]
fn shaped_adtg_parses_deep_nested_chapter_recordsets() {
    let adtg = parse_adtg_bytes(include_bytes!(
        "fixtures/shape/orders_lines_product_category_shape.adtg"
    ))
    .expect("failed to parse deep nested shaped ADTG");

    assert_deep_nested_chapter_recordset_rows(&adtg);
    let lines = chapter_at(&adtg.rows[0].values, 3, "Lines");
    let product = chapter_at(&lines.rows[0].values, 5, "Product");
    let category = chapter_at(&product.rows[0].values, 4, "Category");
    assert_eq!(
        materialize_default_view(category).fields[0].attribute_flags,
        0x8000 | 0x10 | 0x08,
        "ADTG deep nested CategoryId preserves the key-column field flag"
    );
}

fn assert_shaped_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 4, "parent field count");
    assert_eq!(recordset.rows.len(), 3, "parent row count");

    assert_parent_row(
        &recordset.rows[0].values,
        100001,
        1,
        "7.35",
        &[
            (100001, 1000011, 4),
            (100001, 1000012, 5),
            (100001, 1000013, 6),
        ],
    );
    assert_parent_row(
        &recordset.rows[1].values,
        100002,
        2,
        "9.7",
        &[
            (100002, 1000021, 5),
            (100002, 1000022, 6),
            (100002, 1000023, 7),
        ],
    );
    assert_parent_row(
        &recordset.rows[2].values,
        100003,
        3,
        "12.05",
        &[
            (100003, 1000031, 6),
            (100003, 1000032, 7),
            (100003, 1000033, 8),
        ],
    );
}

fn assert_parent_row(
    values: &[Value],
    order_id: i64,
    customer_id: i64,
    freight: &str,
    expected_lines: &[(i64, i64, i64)],
) {
    assert_eq!(values[0], Value::Integer(order_id));
    assert_eq!(values[1], Value::Integer(customer_id));
    assert_eq!(values[2], Value::Decimal(freight.to_string()));

    let Value::Chapter(chapter) = &values[3] else {
        panic!("expected Lines chapter value, got {:?}", values[3]);
    };
    assert_eq!(chapter.fields.len(), 3, "child field count");
    assert_eq!(chapter.rows.len(), expected_lines.len(), "child row count");
    assert_eq!(chapter.fields[0].name, "OrderId");
    assert_eq!(chapter.fields[1].name, "LineId");
    assert_eq!(chapter.fields[2].name, "Quantity");

    for (row, (line_order_id, line_id, quantity)) in chapter.rows.iter().zip(expected_lines) {
        assert_eq!(row.values[0], Value::Integer(*line_order_id));
        assert_eq!(row.values[1], Value::Integer(*line_id));
        assert_eq!(row.values[2], Value::Integer(*quantity));
    }
}

fn assert_multi_chapter_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 6, "multi parent field count");
    assert_eq!(recordset.rows.len(), 5, "multi parent row count");

    assert_multi_parent_row(
        &recordset.rows[0].values,
        100001,
        1,
        "7.35",
        "2024-01-01T09:13:00",
        &[
            (100001, 1000011, 1, 4, "99.88"),
            (100001, 1000012, 2, 5, "103.09"),
            (100001, 1000013, 3, 6, "106.3"),
        ],
        (100001, 1, "wire", "1567.8119", false),
    );
    assert_multi_parent_row(
        &recordset.rows[4].values,
        100005,
        5,
        "16.75",
        "2024-01-01T14:05:00",
        &[
            (100005, 1000051, 1, 8, "112.72"),
            (100005, 1000052, 2, 9, "115.93"),
            (100005, 1000053, 3, 1, "119.14"),
        ],
        (100005, 5, "wire", "2087.1275", true),
    );
}

fn assert_multi_parent_row(
    values: &[Value],
    order_id: i64,
    customer_id: i64,
    freight: &str,
    order_date: &str,
    expected_lines: &[(i64, i64, i64, i64, &str)],
    expected_payment: (i64, i64, &str, &str, bool),
) {
    assert_eq!(values[0], Value::Integer(order_id));
    assert_eq!(values[1], Value::Integer(customer_id));
    assert_eq!(values[2], Value::Decimal(freight.to_string()));
    assert_eq!(values[3], Value::DateTime(order_date.to_string()));

    let lines = chapter_at(values, 4, "Lines");
    assert_eq!(lines.fields.len(), 5, "Lines field count");
    assert_eq!(lines.rows.len(), expected_lines.len(), "Lines row count");
    assert_eq!(lines.fields[0].name, "OrderId");
    assert_eq!(lines.fields[1].name, "LineId");
    assert_eq!(lines.fields[2].name, "LineNumber");
    assert_eq!(lines.fields[3].name, "Quantity");
    assert_eq!(lines.fields[4].name, "UnitPrice");
    for (row, (line_order_id, line_id, line_number, quantity, unit_price)) in
        lines.rows.iter().zip(expected_lines)
    {
        assert_eq!(row.values[0], Value::Integer(*line_order_id));
        assert_eq!(row.values[1], Value::Integer(*line_id));
        assert_eq!(row.values[2], Value::Integer(*line_number));
        assert_eq!(row.values[3], Value::Integer(*quantity));
        assert_eq!(row.values[4], Value::Decimal((*unit_price).to_string()));
    }

    let payments = chapter_at(values, 5, "Payments");
    assert_eq!(payments.fields.len(), 5, "Payments field count");
    assert_eq!(payments.rows.len(), 1, "Payments row count");
    assert_eq!(payments.fields[0].name, "OrderId");
    assert_eq!(payments.fields[1].name, "PaymentId");
    assert_eq!(payments.fields[2].name, "PaymentMethod");
    assert_eq!(payments.fields[3].name, "PaymentAmount");
    assert_eq!(payments.fields[4].name, "Approved");
    let payment = &payments.rows[0].values;
    assert_eq!(payment[0], Value::Integer(expected_payment.0));
    assert_eq!(payment[1], Value::Integer(expected_payment.1));
    assert_eq!(payment[2], Value::String(expected_payment.2.to_string()));
    assert_eq!(payment[3], Value::Decimal(expected_payment.3.to_string()));
    assert_eq!(payment[4], Value::Boolean(expected_payment.4));
}

fn assert_aggregate_chapter_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 9, "aggregate parent field count");
    assert_eq!(recordset.rows.len(), 3, "aggregate parent row count");
    assert_eq!(recordset.fields[3].name, "Lines");
    assert_eq!(recordset.fields[4].name, "LineTotalSum");
    assert_eq!(recordset.fields[5].name, "LineCount");
    assert_eq!(recordset.fields[6].name, "MinQuantity");
    assert_eq!(recordset.fields[7].name, "MaxQuantity");
    assert_eq!(recordset.fields[8].name, "AnyLineNumber");
    assert_eq!(recordset.fields[3].ado_type.map(|ty| ty.code), Some(136));
    assert_eq!(recordset.fields[4].ado_type.map(|ty| ty.code), Some(6));
    assert_eq!(recordset.fields[5].ado_type.map(|ty| ty.code), Some(3));
    assert_eq!(recordset.fields[6].ado_type.map(|ty| ty.code), Some(2));
    assert_eq!(recordset.fields[7].ado_type.map(|ty| ty.code), Some(2));
    assert_eq!(recordset.fields[8].ado_type.map(|ty| ty.code), Some(2));

    assert_aggregate_parent_row(
        &recordset.rows[0].values,
        AggregateParentExpectation {
            order_id: 100001,
            customer_id: 1,
            freight: "7.35",
            total_sum: "1552.77",
            min_quantity: 4,
            max_quantity: 6,
            expected_lines: &[
                (100001, 1000011, 1, 4, "99.88", "399.52"),
                (100001, 1000012, 2, 5, "103.09", "515.45"),
                (100001, 1000013, 3, 6, "106.3", "637.8"),
            ],
        },
    );
    assert_aggregate_parent_row(
        &recordset.rows[1].values,
        AggregateParentExpectation {
            order_id: 100002,
            customer_id: 2,
            freight: "9.7",
            total_sum: "1919.82",
            min_quantity: 5,
            max_quantity: 7,
            expected_lines: &[
                (100002, 1000021, 1, 5, "103.09", "515.45"),
                (100002, 1000022, 2, 6, "106.3", "637.8"),
                (100002, 1000023, 3, 7, "109.51", "766.57"),
            ],
        },
    );
    assert_aggregate_parent_row(
        &recordset.rows[2].values,
        AggregateParentExpectation {
            order_id: 100003,
            customer_id: 3,
            freight: "12.05",
            total_sum: "2306.13",
            min_quantity: 6,
            max_quantity: 8,
            expected_lines: &[
                (100003, 1000031, 1, 6, "106.3", "637.8"),
                (100003, 1000032, 2, 7, "109.51", "766.57"),
                (100003, 1000033, 3, 8, "112.72", "901.76"),
            ],
        },
    );
}

fn assert_aggregate_parent_row(values: &[Value], expected: AggregateParentExpectation<'_>) {
    let AggregateParentExpectation {
        order_id,
        customer_id,
        freight,
        total_sum,
        min_quantity,
        max_quantity,
        expected_lines,
    } = expected;

    assert_eq!(values[0], Value::Integer(order_id));
    assert_eq!(values[1], Value::Integer(customer_id));
    assert_eq!(values[2], Value::Decimal(freight.to_string()));
    assert_eq!(values[4], Value::Decimal(total_sum.to_string()));
    assert_eq!(values[5], Value::Integer(3));
    assert_eq!(values[6], Value::Integer(min_quantity));
    assert_eq!(values[7], Value::Integer(max_quantity));
    assert_eq!(values[8], Value::Integer(1));

    let lines = chapter_at(values, 3, "Lines");
    assert_eq!(lines.fields.len(), 6, "aggregate Lines field count");
    assert_eq!(
        lines.rows.len(),
        expected_lines.len(),
        "aggregate Lines rows"
    );
    assert_eq!(lines.fields[5].name, "LineTotal");
    for (row, (line_order_id, line_id, line_number, quantity, unit_price, line_total)) in
        lines.rows.iter().zip(expected_lines)
    {
        assert_eq!(row.values[0], Value::Integer(*line_order_id));
        assert_eq!(row.values[1], Value::Integer(*line_id));
        assert_eq!(row.values[2], Value::Integer(*line_number));
        assert_eq!(row.values[3], Value::Integer(*quantity));
        assert_eq!(row.values[4], Value::Decimal((*unit_price).to_string()));
        assert_eq!(row.values[5], Value::Decimal((*line_total).to_string()));
    }
}

fn assert_statistics_aggregate_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 8, "statistics parent field count");
    assert_eq!(recordset.rows.len(), 3, "statistics parent row count");
    assert_eq!(recordset.fields[3].name, "Lines");
    assert_eq!(recordset.fields[4].name, "AvgQuantity");
    assert_eq!(recordset.fields[5].name, "QuantityStdev");
    assert_eq!(recordset.fields[6].name, "LineRows");
    assert_eq!(recordset.fields[7].name, "LineTotalCount");
    assert_eq!(recordset.fields[4].ado_type.map(|ty| ty.code), Some(5));
    assert_eq!(recordset.fields[5].ado_type.map(|ty| ty.code), Some(5));
    assert_eq!(recordset.fields[6].ado_type.map(|ty| ty.code), Some(3));

    assert_statistics_aggregate_parent_row(&recordset.rows[0].values, 100001, 5.0);
    assert_statistics_aggregate_parent_row(&recordset.rows[1].values, 100002, 6.0);
    assert_statistics_aggregate_parent_row(&recordset.rows[2].values, 100003, 7.0);
}

fn assert_statistics_aggregate_parent_row(values: &[Value], order_id: i64, avg_quantity: f64) {
    assert_eq!(values[0], Value::Integer(order_id));
    assert_eq!(values[4], Value::Float(avg_quantity));
    assert_eq!(values[5], Value::Float(1.0));
    assert_eq!(values[6], Value::Integer(3));
    assert_eq!(values[7], Value::Integer(3));

    let lines = chapter_at(values, 3, "Lines");
    assert_eq!(lines.fields.len(), 6, "statistics Lines field count");
    assert_eq!(lines.rows.len(), 3, "statistics Lines rows");
    for row in &lines.rows {
        assert_eq!(row.values[0], Value::Integer(order_id));
    }
}

fn assert_grandchild_aggregate_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(
        recordset.fields.len(),
        6,
        "grandchild aggregate parent field count"
    );
    assert_eq!(recordset.rows.len(), 2, "grandchild aggregate parent rows");
    assert_eq!(recordset.fields[3].name, "Lines");
    assert_eq!(recordset.fields[4].name, "ProductCostSum");
    assert_eq!(recordset.fields[5].name, "ProductRows");
    assert_eq!(recordset.fields[3].ado_type.map(|ty| ty.code), Some(136));
    assert_eq!(recordset.fields[4].ado_type.map(|ty| ty.code), Some(6));
    assert_eq!(recordset.fields[5].ado_type.map(|ty| ty.code), Some(3));

    assert_grandchild_aggregate_parent_row(
        &recordset.rows[0].values,
        100001,
        "65.04",
        &[
            (1000011, 13, "20.31"),
            (1000012, 14, "21.68"),
            (1000013, 15, "23.05"),
        ],
    );
    assert_grandchild_aggregate_parent_row(
        &recordset.rows[1].values,
        100002,
        "69.15",
        &[
            (1000021, 14, "21.68"),
            (1000022, 15, "23.05"),
            (1000023, 16, "24.42"),
        ],
    );
}

fn assert_grandchild_aggregate_parent_row(
    values: &[Value],
    order_id: i64,
    product_cost_sum: &str,
    expected_products: &[(i64, i64, &str)],
) {
    assert_eq!(values[0], Value::Integer(order_id));
    assert_eq!(values[4], Value::Decimal(product_cost_sum.to_string()));
    assert_eq!(values[5], Value::Integer(expected_products.len() as i64));

    let lines = chapter_at(values, 3, "Lines");
    assert_eq!(
        lines.fields.len(),
        6,
        "grandchild aggregate Lines field count"
    );
    assert_eq!(
        lines.rows.len(),
        expected_products.len(),
        "grandchild aggregate Lines rows"
    );
    assert_eq!(lines.fields[5].name, "Product");
    for (row, (line_id, product_id, unit_cost)) in lines.rows.iter().zip(expected_products) {
        assert_eq!(row.values[0], Value::Integer(order_id));
        assert_eq!(row.values[1], Value::Integer(*line_id));
        assert_eq!(row.values[3], Value::Integer(*product_id));
        let product = chapter_at(&row.values, 5, "Product");
        assert_eq!(product.fields.len(), 3, "grandchild Product field count");
        assert_eq!(product.rows.len(), 1, "grandchild Product rows");
        assert_eq!(product.rows[0].values[0], Value::Integer(*product_id));
        assert_string_starts_with(
            &product.rows[0].values[1],
            &format!("Product {product_id} "),
            "grandchild ProductName",
        );
        assert_eq!(
            product.rows[0].values[2],
            Value::Decimal((*unit_cost).to_string())
        );
    }
}

fn assert_sparse_aggregate_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(
        recordset.fields.len(),
        8,
        "sparse aggregate parent field count"
    );
    assert_eq!(recordset.rows.len(), 5, "sparse aggregate parent rows");
    assert_eq!(recordset.fields[3].name, "Lines");
    assert_eq!(recordset.fields[4].name, "QuantitySum");
    assert_eq!(recordset.fields[5].name, "AvgQuantity");
    assert_eq!(recordset.fields[6].name, "LineRows");
    assert_eq!(recordset.fields[7].name, "QuantityCount");

    assert_sparse_aggregate_present_parent(&recordset.rows[0].values, 100001, "15", 5.0);
    assert_sparse_aggregate_empty_parent(&recordset.rows[1].values, 100002);
    assert_sparse_aggregate_present_parent(&recordset.rows[2].values, 100003, "21", 7.0);
    assert_sparse_aggregate_empty_parent(&recordset.rows[3].values, 100004);
    assert_sparse_aggregate_empty_parent(&recordset.rows[4].values, 100005);
}

fn assert_sparse_aggregate_present_parent(
    values: &[Value],
    order_id: i64,
    quantity_sum: &str,
    avg_quantity: f64,
) {
    assert_eq!(values[0], Value::Integer(order_id));
    assert_eq!(values[4], Value::Decimal(quantity_sum.to_string()));
    assert_eq!(values[5], Value::Float(avg_quantity));
    assert_eq!(values[6], Value::Integer(3));
    assert_eq!(values[7], Value::Integer(3));
    let lines = chapter_at(values, 3, "Lines");
    assert_eq!(lines.rows.len(), 3, "sparse aggregate non-empty child rows");
    for row in &lines.rows {
        assert_eq!(row.values[0], Value::Integer(order_id));
    }
}

fn assert_sparse_aggregate_empty_parent(values: &[Value], order_id: i64) {
    assert_eq!(values[0], Value::Integer(order_id));
    assert_eq!(
        values[4],
        Value::Null,
        "empty child SUM should persist as null"
    );
    assert_eq!(
        values[5],
        Value::Null,
        "empty child AVG should persist as null"
    );
    assert_eq!(values[6], Value::Integer(0), "COUNT(alias) should be zero");
    assert_eq!(values[7], Value::Integer(0), "COUNT(column) should be zero");
    let lines = chapter_at(values, 3, "Lines");
    assert!(lines.rows.is_empty(), "sparse aggregate empty child rows");
}

fn assert_compute_group_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 4, "compute parent field count");
    assert_eq!(recordset.rows.len(), 3, "compute parent row count");
    assert_eq!(recordset.fields[0].name, "Lines");
    assert_eq!(recordset.fields[1].name, "LineTotalSum");
    assert_eq!(recordset.fields[2].name, "LineCount");
    assert_eq!(recordset.fields[3].name, "CustomerId");
    assert_eq!(recordset.fields[0].ado_type.map(|ty| ty.code), Some(136));
    assert_eq!(recordset.fields[1].ado_type.map(|ty| ty.code), Some(6));
    assert_eq!(recordset.fields[2].ado_type.map(|ty| ty.code), Some(3));
    assert_eq!(recordset.fields[3].ado_type.map(|ty| ty.code), Some(3));

    assert_compute_group_parent_row(&recordset.rows[0].values, 1, "2366.37", 100001, 100097);
    assert_compute_group_parent_row(&recordset.rows[1].values, 2, "2838.24", 100002, 100098);
    assert_compute_group_parent_row(&recordset.rows[2].values, 3, "3339", 100003, 100099);
}

fn assert_compute_group_parent_row(
    values: &[Value],
    customer_id: i64,
    total_sum: &str,
    first_order_id: i64,
    last_order_id: i64,
) {
    assert_eq!(values[1], Value::Decimal(total_sum.to_string()));
    assert_eq!(values[2], Value::Integer(9));
    assert_eq!(values[3], Value::Integer(customer_id));

    let lines = chapter_at(values, 0, "Lines");
    assert_eq!(lines.fields.len(), 6, "compute Lines field count");
    assert_eq!(lines.fields[0].name, "CustomerId");
    assert_eq!(lines.fields[1].name, "OrderId");
    assert_eq!(lines.fields[5].name, "LineTotal");
    assert_eq!(lines.rows.len(), 9, "compute Lines row count");
    assert_eq!(lines.rows[0].values[1], Value::Integer(first_order_id));
    assert_eq!(lines.rows[8].values[1], Value::Integer(last_order_id));
    for row in &lines.rows {
        assert_eq!(row.values[0], Value::Integer(customer_id));
    }
}

fn assert_calc_new_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 6, "CALC/NEW parent field count");
    assert_eq!(recordset.rows.len(), 3, "CALC/NEW parent row count");
    assert_eq!(recordset.fields[3].name, "OrderCustomerCalc");
    assert_eq!(recordset.fields[4].name, "ReviewNote");
    assert_eq!(recordset.fields[5].name, "Lines");
    assert_eq!(recordset.fields[3].ado_type.map(|ty| ty.code), Some(3));
    assert_eq!(recordset.fields[4].ado_type.map(|ty| ty.code), Some(202));
    assert_eq!(recordset.fields[5].ado_type.map(|ty| ty.code), Some(136));
    assert!(recordset.fields[4].writable, "NEW parent field is writable");

    assert_calc_new_parent_row(
        &recordset.rows[0].values,
        100001,
        1,
        "7.35",
        100002,
        &[5, 7, 9],
    );
    assert_calc_new_parent_row(
        &recordset.rows[1].values,
        100002,
        2,
        "9.7",
        100004,
        &[6, 8, 10],
    );
    assert_calc_new_parent_row(
        &recordset.rows[2].values,
        100003,
        3,
        "12.05",
        100006,
        &[7, 9, 11],
    );
}

fn assert_calc_new_parent_row(
    values: &[Value],
    order_id: i64,
    customer_id: i64,
    freight: &str,
    calc: i64,
    child_calc_values: &[i64],
) {
    assert_eq!(values[0], Value::Integer(order_id));
    assert_eq!(values[1], Value::Integer(customer_id));
    assert_eq!(values[2], Value::Decimal(freight.to_string()));
    assert_eq!(values[3], Value::Integer(calc));
    assert_eq!(values[4], Value::Null);

    let lines = chapter_at(values, 5, "Lines");
    assert_eq!(lines.fields.len(), 7, "CALC/NEW Lines field count");
    assert_eq!(
        lines.rows.len(),
        child_calc_values.len(),
        "CALC/NEW Lines rows"
    );
    assert_eq!(lines.fields[5].name, "QuantityLineCalc");
    assert_eq!(lines.fields[6].name, "LineScore");
    assert!(lines.fields[6].writable, "NEW child field is writable");
    for (row, child_calc) in lines.rows.iter().zip(child_calc_values) {
        assert_eq!(row.values[0], Value::Integer(order_id));
        assert_eq!(row.values[5], Value::Integer(*child_calc));
        assert_eq!(row.values[6], Value::Null);
    }
}

fn assert_calc_new_pending_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 6, "pending CALC/NEW parent fields");

    let default = materialize_default_view(recordset);
    assert_eq!(default.rows.len(), 3, "pending CALC/NEW default rows");
    assert_eq!(
        default
            .rows
            .iter()
            .map(|row| row.status)
            .collect::<Vec<_>>(),
        vec![
            RecordStatusFlag::Modified,
            RecordStatusFlag::Modified,
            RecordStatusFlag::Unmodified,
        ],
        "pending CALC/NEW default parent statuses"
    );
    assert_eq!(default.rows[0].values[0], Value::Integer(100001));
    assert_eq!(
        default.rows[0].values[4],
        Value::String("parent-note-100001".to_string())
    );
    assert_eq!(default.rows[1].values[0], Value::Integer(100002));
    assert_eq!(
        default.rows[1].values[4],
        Value::String("parent-note-100002".to_string())
    );
    assert_eq!(default.rows[2].values[4], Value::Null);

    assert_calc_new_pending_lines(&default.rows[0].values, 100001, 501);
    assert_calc_new_pending_lines(&default.rows[1].values, 100002, 502);

    let pending = materialize_pending_view(recordset);
    assert_eq!(pending.rows.len(), 2, "pending CALC/NEW pending rows");
    assert_eq!(
        pending
            .rows
            .iter()
            .map(|row| row.status)
            .collect::<Vec<_>>(),
        vec![RecordStatusFlag::Modified, RecordStatusFlag::Modified],
        "pending CALC/NEW pending parent statuses"
    );
    assert_eq!(
        pending.rows[0].values[4],
        Value::String("parent-note-100001".to_string())
    );
    assert_eq!(
        pending.rows[1].values[4],
        Value::String("parent-note-100002".to_string())
    );
    assert_calc_new_pending_child_view(&pending.rows[0].values, 100001, 501);
    assert_calc_new_pending_child_view(&pending.rows[1].values, 100002, 502);
}

fn assert_calc_new_pending_lines(values: &[Value], order_id: i64, line_score: i64) {
    let lines = chapter_at(values, 5, "Lines");
    let lines_default = materialize_default_view(lines);
    assert_eq!(lines_default.rows.len(), 3, "pending CALC/NEW child rows");
    assert_eq!(
        lines_default
            .rows
            .iter()
            .map(|row| row.status)
            .collect::<Vec<_>>(),
        vec![
            RecordStatusFlag::Modified,
            RecordStatusFlag::Unmodified,
            RecordStatusFlag::Unmodified,
        ],
        "pending CALC/NEW default child statuses"
    );
    assert_eq!(lines_default.rows[0].values[0], Value::Integer(order_id));
    assert_eq!(lines_default.rows[0].values[6], Value::Integer(line_score));
    assert_eq!(lines_default.rows[1].values[6], Value::Null);
    assert_eq!(lines_default.rows[2].values[6], Value::Null);
}

fn assert_calc_new_pending_child_view(values: &[Value], order_id: i64, line_score: i64) {
    let lines = chapter_at(values, 5, "Lines");
    let child_pending = materialize_pending_view(lines);
    assert_eq!(
        child_pending.rows.len(),
        1,
        "pending CALC/NEW child pending rows"
    );
    assert_eq!(
        child_pending.rows[0].status,
        RecordStatusFlag::Modified,
        "pending CALC/NEW child pending status"
    );
    assert_eq!(child_pending.rows[0].values[0], Value::Integer(order_id));
    assert_eq!(child_pending.rows[0].values[6], Value::Integer(line_score));
}

fn assert_composite_chapter_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 4, "composite parent field count");
    assert_eq!(recordset.rows.len(), 3, "composite parent row count");

    assert_composite_parent_row(
        &recordset.rows[0].values,
        100001,
        1,
        "7.35",
        &[
            (100001, 1, 1000011, 1, 4),
            (100001, 1, 1000012, 2, 5),
            (100001, 1, 1000013, 3, 6),
        ],
    );
    assert_composite_parent_row(
        &recordset.rows[2].values,
        100003,
        3,
        "12.05",
        &[
            (100003, 3, 1000031, 1, 6),
            (100003, 3, 1000032, 2, 7),
            (100003, 3, 1000033, 3, 8),
        ],
    );
}

fn assert_composite_parent_row(
    values: &[Value],
    order_id: i64,
    customer_id: i64,
    freight: &str,
    expected_lines: &[(i64, i64, i64, i64, i64)],
) {
    assert_eq!(values[0], Value::Integer(order_id));
    assert_eq!(values[1], Value::Integer(customer_id));
    assert_eq!(values[2], Value::Decimal(freight.to_string()));

    let lines = chapter_at(values, 3, "Lines");
    assert_eq!(lines.fields.len(), 5, "composite Lines field count");
    assert_eq!(
        lines.rows.len(),
        expected_lines.len(),
        "composite relation should exclude wrong-CustomerId decoy rows"
    );
    assert_eq!(lines.fields[0].name, "OrderId");
    assert_eq!(lines.fields[1].name, "CustomerId");
    assert_eq!(lines.fields[2].name, "LineId");
    assert_eq!(lines.fields[3].name, "LineNumber");
    assert_eq!(lines.fields[4].name, "Quantity");

    for (row, (line_order_id, line_customer_id, line_id, line_number, quantity)) in
        lines.rows.iter().zip(expected_lines)
    {
        assert_eq!(row.values[0], Value::Integer(*line_order_id));
        assert_eq!(row.values[1], Value::Integer(*line_customer_id));
        assert_eq!(row.values[2], Value::Integer(*line_id));
        assert_eq!(row.values[3], Value::Integer(*line_number));
        assert_eq!(row.values[4], Value::Integer(*quantity));
        assert!(
            *line_id < 900_000_000,
            "composite relation leaked a deliberately wrong-key decoy row"
        );
    }
}

fn assert_date_currency_relation_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 5, "date/currency parent fields");
    assert_eq!(
        recordset.rows.len(),
        3,
        "date/currency relation parent rows"
    );
    assert_eq!(recordset.fields[0].name, "OrderId");
    assert_eq!(recordset.fields[1].name, "OrderDate");
    assert_eq!(recordset.fields[2].name, "Freight");
    assert_eq!(recordset.fields[3].name, "DateLines");
    assert_eq!(recordset.fields[4].name, "FreightLines");

    assert_date_currency_relation_parent_row(
        &recordset.rows[0].values,
        100001,
        "2024-01-01T09:13:00",
        "7.35",
        &[(1000011, 4), (1000012, 5), (1000013, 6)],
    );
    assert_date_currency_relation_parent_row(
        &recordset.rows[1].values,
        100002,
        "2024-01-01T10:26:00",
        "9.7",
        &[(1000021, 5), (1000022, 6), (1000023, 7)],
    );
    assert_date_currency_relation_parent_row(
        &recordset.rows[2].values,
        100003,
        "2024-01-01T11:39:00",
        "12.05",
        &[(1000031, 6), (1000032, 7), (1000033, 8)],
    );
}

fn assert_date_currency_relation_parent_row(
    values: &[Value],
    order_id: i64,
    order_date: &str,
    freight: &str,
    expected_lines: &[(i64, i64)],
) {
    assert_eq!(values[0], Value::Integer(order_id));
    assert_eq!(values[1], Value::DateTime(order_date.to_string()));
    assert_eq!(values[2], Value::Decimal(freight.to_string()));

    let date_lines = chapter_at(values, 3, "DateLines");
    assert_eq!(date_lines.fields.len(), 4, "DateLines field count");
    assert_eq!(date_lines.fields[0].name, "OrderDate");
    assert_eq!(date_lines.fields[1].name, "LineId");
    assert_eq!(date_lines.fields[2].name, "OrderId");
    assert_eq!(date_lines.fields[3].name, "Quantity");
    assert_eq!(
        date_lines.rows.len(),
        expected_lines.len(),
        "DateTime-keyed child rows"
    );
    for (row, (line_id, quantity)) in date_lines.rows.iter().zip(expected_lines) {
        assert_eq!(row.values[0], Value::DateTime(order_date.to_string()));
        assert_eq!(row.values[1], Value::Integer(*line_id));
        assert_eq!(row.values[2], Value::Integer(order_id));
        assert_eq!(row.values[3], Value::Integer(*quantity));
    }

    let freight_lines = chapter_at(values, 4, "FreightLines");
    assert_eq!(freight_lines.fields.len(), 4, "FreightLines field count");
    assert_eq!(freight_lines.fields[0].name, "Freight");
    assert_eq!(freight_lines.fields[1].name, "LineId");
    assert_eq!(freight_lines.fields[2].name, "OrderId");
    assert_eq!(freight_lines.fields[3].name, "Quantity");
    assert_eq!(
        freight_lines.rows.len(),
        expected_lines.len(),
        "currency-keyed child rows"
    );
    for (row, (line_id, quantity)) in freight_lines.rows.iter().zip(expected_lines) {
        assert_eq!(row.values[0], Value::Decimal(freight.to_string()));
        assert_eq!(row.values[1], Value::Integer(*line_id));
        assert_eq!(row.values[2], Value::Integer(order_id));
        assert_eq!(row.values[3], Value::Integer(*quantity));
    }
}

fn assert_smalldatetime_smallmoney_relation_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(
        recordset.fields.len(),
        5,
        "smalldatetime/smallmoney parent fields"
    );
    assert_eq!(
        recordset.rows.len(),
        3,
        "smalldatetime/smallmoney relation parent rows"
    );
    assert_eq!(recordset.fields[0].name, "RequiredDate");
    assert_eq!(recordset.fields[1].name, "UnitCost");
    assert_eq!(recordset.fields[2].name, "ProductId");
    assert_eq!(recordset.fields[3].name, "RequiredOrders");
    assert_eq!(recordset.fields[4].name, "UnitProducts");
    assert_eq!(recordset.fields[0].ado_type.map(|ty| ty.code), Some(135));
    assert_eq!(recordset.fields[1].ado_type.map(|ty| ty.code), Some(6));
    assert_eq!(recordset.fields[3].ado_type.map(|ty| ty.code), Some(136));
    assert_eq!(recordset.fields[4].ado_type.map(|ty| ty.code), Some(136));

    assert_smalldatetime_smallmoney_parent_row(
        &recordset.rows[0].values,
        "2024-01-05T09:13:00",
        "3.87",
        1,
        100001,
        1,
        2,
    );
    assert_smalldatetime_smallmoney_parent_row(
        &recordset.rows[1].values,
        "2024-01-06T10:26:00",
        "5.24",
        2,
        100002,
        2,
        3,
    );
    assert_smalldatetime_smallmoney_parent_row(
        &recordset.rows[2].values,
        "2024-01-07T11:39:00",
        "6.61",
        3,
        100003,
        3,
        4,
    );
}

fn assert_smalldatetime_smallmoney_parent_row(
    values: &[Value],
    required_date: &str,
    unit_cost: &str,
    product_id: i64,
    order_id: i64,
    customer_id: i64,
    priority: u64,
) {
    assert_eq!(values[0], Value::DateTime(required_date.to_string()));
    assert_eq!(values[1], Value::Decimal(unit_cost.to_string()));
    assert_eq!(values[2], Value::Integer(product_id));

    let required_orders = chapter_at(values, 3, "RequiredOrders");
    assert_eq!(
        required_orders.fields.len(),
        4,
        "RequiredOrders field count"
    );
    assert_eq!(required_orders.fields[0].name, "RequiredDate");
    assert_eq!(required_orders.fields[1].name, "OrderId");
    assert_eq!(required_orders.fields[2].name, "CustomerId");
    assert_eq!(required_orders.fields[3].name, "Priority");
    assert_eq!(required_orders.rows.len(), 1, "RequiredOrders row count");
    let order = &required_orders.rows[0].values;
    assert_eq!(order[0], Value::DateTime(required_date.to_string()));
    assert_eq!(order[1], Value::Integer(order_id));
    assert_eq!(order[2], Value::Integer(customer_id));
    assert_eq!(order[3], Value::UnsignedInteger(priority));

    let unit_products = chapter_at(values, 4, "UnitProducts");
    assert_eq!(unit_products.fields.len(), 4, "UnitProducts field count");
    assert_eq!(unit_products.fields[0].name, "UnitCost");
    assert_eq!(unit_products.fields[1].name, "ProductId");
    assert_eq!(unit_products.fields[2].name, "CategoryId");
    assert_eq!(unit_products.fields[3].name, "ProductName");
    assert_eq!(unit_products.rows.len(), 1, "UnitProducts row count");
    let product = &unit_products.rows[0].values;
    assert_eq!(product[0], Value::Decimal(unit_cost.to_string()));
    assert_eq!(product[1], Value::Integer(product_id));
    assert_eq!(product[2], Value::UnsignedInteger(product_id as u64));
    assert_string_starts_with(
        &product[3],
        &format!("Product {product_id} "),
        "ProductName",
    );
}

fn assert_tinyint_smallint_relation_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 4, "tinyint/smallint parent fields");
    assert_eq!(
        recordset.rows.len(),
        3,
        "tinyint/smallint relation parent rows"
    );
    assert_eq!(recordset.fields[0].name, "RegionId");
    assert_eq!(recordset.fields[1].name, "EmployeeId");
    assert_eq!(recordset.fields[2].name, "RegionCustomers");
    assert_eq!(recordset.fields[3].name, "EmployeeOrders");
    assert_eq!(recordset.fields[0].ado_type.map(|ty| ty.code), Some(17));
    assert_eq!(recordset.fields[1].ado_type.map(|ty| ty.code), Some(2));
    assert_eq!(recordset.fields[2].ado_type.map(|ty| ty.code), Some(136));
    assert_eq!(recordset.fields[3].ado_type.map(|ty| ty.code), Some(136));

    assert_tinyint_smallint_parent_row(
        &recordset.rows[0].values,
        1,
        1,
        &[(1, "CUST0001"), (7, "CUST0007")],
        &[(100001, 1, 2), (100013, 13, 4)],
    );
    assert_tinyint_smallint_parent_row(
        &recordset.rows[1].values,
        2,
        2,
        &[(2, "CUST0002"), (8, "CUST0008")],
        &[(100002, 2, 3), (100014, 14, 5)],
    );
    assert_tinyint_smallint_parent_row(
        &recordset.rows[2].values,
        3,
        3,
        &[(3, "CUST0003"), (9, "CUST0009")],
        &[(100003, 3, 4), (100015, 15, 1)],
    );
}

fn assert_tinyint_smallint_parent_row(
    values: &[Value],
    region_id: u64,
    employee_id: i64,
    expected_customers: &[(i64, &str)],
    expected_orders: &[(i64, i64, u64)],
) {
    assert_eq!(values[0], Value::UnsignedInteger(region_id));
    assert_eq!(values[1], Value::Integer(employee_id));

    let customers = chapter_at(values, 2, "RegionCustomers");
    assert_eq!(customers.fields.len(), 3, "RegionCustomers field count");
    assert_eq!(customers.fields[0].name, "RegionId");
    assert_eq!(customers.fields[1].name, "CustomerId");
    assert_eq!(customers.fields[2].name, "CustomerCode");
    assert_eq!(
        customers.rows.len(),
        expected_customers.len(),
        "RegionCustomers row count"
    );
    for (row, (customer_id, customer_code)) in customers.rows.iter().zip(expected_customers) {
        assert_eq!(row.values[0], Value::UnsignedInteger(region_id));
        assert_eq!(row.values[1], Value::Integer(*customer_id));
        assert_eq!(row.values[2], Value::String((*customer_code).to_string()));
    }

    let orders = chapter_at(values, 3, "EmployeeOrders");
    assert_eq!(orders.fields.len(), 4, "EmployeeOrders field count");
    assert_eq!(orders.fields[0].name, "EmployeeId");
    assert_eq!(orders.fields[1].name, "OrderId");
    assert_eq!(orders.fields[2].name, "CustomerId");
    assert_eq!(orders.fields[3].name, "Priority");
    assert_eq!(
        orders.rows.len(),
        expected_orders.len(),
        "EmployeeOrders row count"
    );
    for (row, (order_id, customer_id, priority)) in orders.rows.iter().zip(expected_orders) {
        assert_eq!(row.values[0], Value::Integer(employee_id));
        assert_eq!(row.values[1], Value::Integer(*order_id));
        assert_eq!(row.values[2], Value::Integer(*customer_id));
        assert_eq!(row.values[3], Value::UnsignedInteger(*priority));
    }
}

fn assert_bigint_relation_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 4, "bigint parent fields");
    assert_eq!(recordset.rows.len(), 3, "bigint relation parent rows");
    assert_eq!(recordset.fields[0].name, "LineId");
    assert_eq!(recordset.fields[1].name, "OrderId");
    assert_eq!(recordset.fields[2].name, "LineNumber");
    assert_eq!(recordset.fields[3].name, "Legacy");
    assert_eq!(recordset.fields[0].ado_type.map(|ty| ty.code), Some(20));
    assert_eq!(recordset.fields[1].ado_type.map(|ty| ty.code), Some(3));
    assert_eq!(recordset.fields[2].ado_type.map(|ty| ty.code), Some(2));
    assert_eq!(recordset.fields[3].ado_type.map(|ty| ty.code), Some(136));

    assert_bigint_parent_row(&recordset.rows[0].values, 1000011, 1, 1);
    assert_bigint_parent_row(&recordset.rows[1].values, 1000012, 2, 2);
    assert_bigint_parent_row(&recordset.rows[2].values, 1000013, 3, 3);
}

fn assert_bigint_parent_row(values: &[Value], line_id: i64, line_number: i64, legacy_doc_id: i64) {
    assert_eq!(values[0], Value::Integer(line_id));
    assert_eq!(values[1], Value::Integer(100001));
    assert_eq!(values[2], Value::Integer(line_number));

    let legacy = chapter_at(values, 3, "Legacy");
    assert_eq!(legacy.fields.len(), 3, "Legacy visible field count");
    assert_eq!(legacy.fields[0].name, "LineId");
    assert_eq!(legacy.fields[1].name, "LegacyDocId");
    assert_eq!(legacy.fields[2].name, "LegacyCode");
    assert_eq!(legacy.fields[0].ado_type.map(|ty| ty.code), Some(20));
    assert_eq!(legacy.fields[1].ado_type.map(|ty| ty.code), Some(3));
    assert_eq!(legacy.rows.len(), 1, "Legacy row count");

    let row = &legacy.rows[0].values;
    assert_eq!(row[0], Value::Integer(line_id));
    assert_eq!(row[1], Value::Integer(legacy_doc_id));
    assert_eq!(
        row[2],
        Value::String(format!("LG{legacy_doc_id:04}")),
        "LegacyCode"
    );
}

fn assert_decimal_numeric_relation_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 4, "decimal/numeric parent fields");
    assert_eq!(
        recordset.rows.len(),
        3,
        "decimal/numeric relation parent rows"
    );
    assert_eq!(recordset.fields[0].name, "DiscountRate");
    assert_eq!(recordset.fields[1].name, "TaxRate");
    assert_eq!(recordset.fields[2].name, "DiscountLines");
    assert_eq!(recordset.fields[3].name, "TaxLines");
    assert_eq!(recordset.fields[0].ado_type.map(|ty| ty.code), Some(131));
    assert_eq!(recordset.fields[1].ado_type.map(|ty| ty.code), Some(131));
    assert_eq!(recordset.fields[2].ado_type.map(|ty| ty.code), Some(136));
    assert_eq!(recordset.fields[3].ado_type.map(|ty| ty.code), Some(136));

    assert_decimal_numeric_relation_parent_row(
        &recordset.rows[0].values,
        "0",
        "0",
        &[
            (1000011, 100001, 1, 4),
            (1000053, 100005, 3, 1),
            (1000062, 100006, 2, 1),
        ],
        &[
            (1000093, 100009, 3, 5),
            (1000102, 100010, 2, 5),
            (1000111, 100011, 1, 5),
        ],
    );
    assert_decimal_numeric_relation_parent_row(
        &recordset.rows[1].values,
        "0.01",
        "0.01",
        &[
            (1000012, 100001, 2, 5),
            (1000021, 100002, 1, 5),
            (1000063, 100006, 3, 2),
        ],
        &[
            (1000011, 100001, 1, 4),
            (1000103, 100010, 3, 6),
            (1000112, 100011, 2, 6),
        ],
    );
    assert_decimal_numeric_relation_parent_row(
        &recordset.rows[2].values,
        "0.02",
        "0.02",
        &[
            (1000013, 100001, 3, 6),
            (1000022, 100002, 2, 6),
            (1000031, 100003, 1, 6),
        ],
        &[
            (1000012, 100001, 2, 5),
            (1000021, 100002, 1, 5),
            (1000113, 100011, 3, 7),
        ],
    );
}

fn assert_decimal_numeric_relation_parent_row(
    values: &[Value],
    discount_rate: &str,
    tax_rate: &str,
    expected_discount_lines: &[(i64, i64, i64, i64)],
    expected_tax_lines: &[(i64, i64, i64, i64)],
) {
    assert_eq!(values[0], Value::Decimal(discount_rate.to_string()));
    assert_eq!(values[1], Value::Decimal(tax_rate.to_string()));

    let discount_lines = chapter_at(values, 2, "DiscountLines");
    assert_decimal_numeric_child_rows(
        discount_lines,
        "DiscountRate",
        discount_rate,
        expected_discount_lines,
    );

    let tax_lines = chapter_at(values, 3, "TaxLines");
    assert_decimal_numeric_child_rows(tax_lines, "TaxRate", tax_rate, expected_tax_lines);
}

fn assert_decimal_numeric_child_rows(
    recordset: &tablegram::Recordset,
    key_name: &str,
    key_value: &str,
    expected_rows: &[(i64, i64, i64, i64)],
) {
    assert_eq!(recordset.fields.len(), 5, "{key_name} child field count");
    assert_eq!(recordset.fields[0].name, key_name);
    assert_eq!(recordset.fields[1].name, "LineId");
    assert_eq!(recordset.fields[2].name, "OrderId");
    assert_eq!(recordset.fields[3].name, "LineNumber");
    assert_eq!(recordset.fields[4].name, "Quantity");
    assert_eq!(
        recordset.rows.len(),
        expected_rows.len(),
        "{key_name} child row count"
    );

    for (row, (line_id, order_id, line_number, quantity)) in
        recordset.rows.iter().zip(expected_rows)
    {
        assert_eq!(row.values[0], Value::Decimal(key_value.to_string()));
        assert_eq!(row.values[1], Value::Integer(*line_id));
        assert_eq!(row.values[2], Value::Integer(*order_id));
        assert_eq!(row.values[3], Value::Integer(*line_number));
        assert_eq!(row.values[4], Value::Integer(*quantity));
    }
}

fn assert_real_float_relation_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 4, "real/float parent fields");
    assert_eq!(recordset.rows.len(), 3, "real/float relation parent rows");
    assert_eq!(recordset.fields[0].name, "WeightReal");
    assert_eq!(recordset.fields[1].name, "RatingFloat");
    assert_eq!(recordset.fields[2].name, "RealProducts");
    assert_eq!(recordset.fields[3].name, "FloatProducts");
    assert_eq!(recordset.fields[0].ado_type.map(|ty| ty.code), Some(4));
    assert_eq!(recordset.fields[1].ado_type.map(|ty| ty.code), Some(5));
    assert_eq!(recordset.fields[2].ado_type.map(|ty| ty.code), Some(136));
    assert_eq!(recordset.fields[3].ado_type.map(|ty| ty.code), Some(136));

    assert_real_float_relation_parent_row(&recordset.rows[0].values, 0.625, 1.28125, 1, 1);
    assert_real_float_relation_parent_row(&recordset.rows[1].values, 0.75, 1.3125, 2, 2);
    assert_real_float_relation_parent_row(&recordset.rows[2].values, 0.875, 1.34375, 3, 3);
}

fn assert_real_float_relation_parent_row(
    values: &[Value],
    weight_real: f64,
    rating_float: f64,
    product_id: i64,
    category_id: u64,
) {
    assert_eq!(values[0], Value::Float(weight_real));
    assert_eq!(values[1], Value::Float(rating_float));

    let real_products = chapter_at(values, 2, "RealProducts");
    assert_real_float_child_rows(
        real_products,
        "WeightReal",
        weight_real,
        product_id,
        category_id,
    );

    let float_products = chapter_at(values, 3, "FloatProducts");
    assert_real_float_child_rows(
        float_products,
        "RatingFloat",
        rating_float,
        product_id,
        category_id,
    );
}

fn assert_real_float_child_rows(
    recordset: &tablegram::Recordset,
    key_name: &str,
    key_value: f64,
    product_id: i64,
    category_id: u64,
) {
    assert_eq!(recordset.fields.len(), 4, "{key_name} child field count");
    assert_eq!(recordset.fields[0].name, key_name);
    assert_eq!(recordset.fields[1].name, "ProductId");
    assert_eq!(recordset.fields[2].name, "CategoryId");
    assert_eq!(recordset.fields[3].name, "ProductName");
    assert_eq!(recordset.rows.len(), 1, "{key_name} child row count");

    let row = &recordset.rows[0].values;
    assert_eq!(row[0], Value::Float(key_value));
    assert_eq!(row[1], Value::Integer(product_id));
    assert_eq!(row[2], Value::UnsignedInteger(category_id));
    assert_string_starts_with(&row[3], &format!("Product {product_id} "), "ProductName");
}

fn assert_guid_chapter_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 4, "GUID parent field count");
    assert_eq!(recordset.rows.len(), 4, "GUID parent row count");
    assert_eq!(recordset.fields[0].name, "CustomerId");
    assert_eq!(recordset.fields[1].name, "CustomerGuid");
    assert_eq!(recordset.fields[2].name, "CustomerName");
    assert_eq!(recordset.fields[3].name, "Orders");
    assert_eq!(recordset.fields[1].ado_type.map(|ty| ty.code), Some(72));
    assert_eq!(recordset.fields[3].ado_type.map(|ty| ty.code), Some(136));

    assert_guid_parent_row(
        &recordset.rows[0].values,
        1,
        "{00000001-1111-2222-3333-000000000001}",
        &[
            (100001, "7.35"),
            (100049, "40.25"),
            (100097, "33.2"),
            (100145, "26.15"),
            (100193, "19.1"),
        ],
    );
    assert_guid_parent_row(
        &recordset.rows[3].values,
        4,
        "{00000004-1111-2222-3333-000000000004}",
        &[
            (100004, "14.4"),
            (100052, "7.35"),
            (100100, "40.25"),
            (100148, "33.2"),
            (100196, "26.15"),
        ],
    );
}

fn assert_guid_parent_row(
    values: &[Value],
    customer_id: i64,
    customer_guid: &str,
    expected_orders: &[(i64, &str)],
) {
    assert_eq!(values[0], Value::Integer(customer_id));
    assert_eq!(values[1], Value::Guid(customer_guid.to_string()));
    assert!(matches!(values[2], Value::String(_)));

    let orders = chapter_at(values, 3, "Orders");
    assert_eq!(orders.fields.len(), 3, "GUID Orders field count");
    assert_eq!(
        orders.rows.len(),
        expected_orders.len(),
        "GUID Orders row count"
    );
    assert_eq!(orders.fields[0].name, "CustomerGuid");
    assert_eq!(orders.fields[1].name, "OrderId");
    assert_eq!(orders.fields[2].name, "Freight");

    for (row, (order_id, freight)) in orders.rows.iter().zip(expected_orders) {
        assert_eq!(row.values[0], Value::Guid(customer_guid.to_string()));
        assert_eq!(row.values[1], Value::Integer(*order_id));
        assert_eq!(row.values[2], Value::Decimal((*freight).to_string()));
    }
}

fn assert_text_chapter_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 4, "text parent field count");
    assert_eq!(recordset.rows.len(), 6, "text parent row count");
    assert_eq!(recordset.fields[0].name, "RegionCode");
    assert_eq!(recordset.fields[1].name, "RegionName");
    assert_eq!(recordset.fields[2].name, "IsDomestic");
    assert_eq!(recordset.fields[3].name, "Customers");
    assert_eq!(recordset.fields[0].ado_type.map(|ty| ty.code), Some(129));
    assert_eq!(recordset.fields[3].ado_type.map(|ty| ty.code), Some(136));

    assert_text_parent_row(
        &recordset.rows[0].values,
        "SEL",
        "Seoul",
        true,
        &[(1, "CUST0001"), (7, "CUST0007"), (13, "CUST0013")],
    );
    assert_text_parent_row(
        &recordset.rows[5].values,
        "BER",
        "Berlin",
        false,
        &[(6, "CUST0006"), (12, "CUST0012"), (18, "CUST0018")],
    );
}

fn assert_text_parent_row(
    values: &[Value],
    region_code: &str,
    region_name: &str,
    is_domestic: bool,
    expected_customers: &[(i64, &str)],
) {
    assert_eq!(values[0], Value::String(region_code.to_string()));
    assert_eq!(values[1], Value::String(region_name.to_string()));
    assert_eq!(values[2], Value::Boolean(is_domestic));

    let customers = chapter_at(values, 3, "Customers");
    assert_eq!(customers.fields.len(), 4, "text Customers field count");
    assert_eq!(
        customers.rows.len(),
        expected_customers.len(),
        "text Customers row count"
    );
    assert_eq!(customers.fields[0].name, "RegionCode");
    assert_eq!(customers.fields[1].name, "CustomerId");
    assert_eq!(customers.fields[2].name, "CustomerCode");
    assert_eq!(customers.fields[3].name, "CustomerName");

    for (row, (customer_id, customer_code)) in customers.rows.iter().zip(expected_customers) {
        assert_eq!(row.values[0], Value::String(region_code.to_string()));
        assert_eq!(row.values[1], Value::Integer(*customer_id));
        assert_eq!(row.values[2], Value::String((*customer_code).to_string()));
        assert!(matches!(row.values[3], Value::String(_)));
    }
}

fn assert_unicode_relation_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 3, "Unicode parent field count");
    assert_eq!(recordset.rows.len(), 3, "Unicode parent row count");
    assert_eq!(recordset.fields[0].name, "CustomerId");
    assert_eq!(recordset.fields[1].name, "CustomerName");
    assert_eq!(recordset.fields[2].name, "Orders");
    assert_eq!(recordset.fields[1].ado_type.map(|ty| ty.code), Some(202));
    assert_eq!(recordset.fields[2].ado_type.map(|ty| ty.code), Some(136));

    assert_unicode_parent_row(
        &recordset.rows[0].values,
        1,
        "고객 1 / Customer 1",
        &[
            (100001, "7.35"),
            (100049, "40.25"),
            (100097, "33.2"),
            (100145, "26.15"),
            (100193, "19.1"),
        ],
    );
    assert_unicode_parent_row(
        &recordset.rows[1].values,
        2,
        "고객 2 / Customer 2",
        &[
            (100002, "9.7"),
            (100050, "42.6"),
            (100098, "35.55"),
            (100146, "28.5"),
            (100194, "21.45"),
        ],
    );
    assert_unicode_parent_row(
        &recordset.rows[2].values,
        3,
        "고객 3 / Customer 3",
        &[
            (100003, "12.05"),
            (100051, "5"),
            (100099, "37.9"),
            (100147, "30.85"),
            (100195, "23.8"),
        ],
    );
}

fn assert_unicode_parent_row(
    values: &[Value],
    customer_id: i64,
    customer_name: &str,
    expected_orders: &[(i64, &str)],
) {
    assert_eq!(values[0], Value::Integer(customer_id));
    assert_eq!(values[1], Value::String(customer_name.to_string()));

    let orders = chapter_at(values, 2, "Orders");
    assert_eq!(orders.fields.len(), 3, "Unicode Orders field count");
    assert_eq!(orders.fields[0].name, "CustomerName");
    assert_eq!(orders.fields[1].name, "OrderId");
    assert_eq!(orders.fields[2].name, "Freight");
    assert_eq!(
        orders.rows.len(),
        expected_orders.len(),
        "Unicode-keyed Orders row count"
    );
    for (row, (order_id, freight)) in orders.rows.iter().zip(expected_orders) {
        assert_eq!(row.values[0], Value::String(customer_name.to_string()));
        assert_eq!(row.values[1], Value::Integer(*order_id));
        assert_eq!(row.values[2], Value::Decimal((*freight).to_string()));
    }
}

fn assert_binary_chapter_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 4, "binary parent field count");
    assert_eq!(recordset.rows.len(), 4, "binary parent row count");
    assert_eq!(recordset.fields[0].name, "ProductId");
    assert_eq!(recordset.fields[1].name, "ProductSku");
    assert_eq!(recordset.fields[2].name, "ProductName");
    assert_eq!(recordset.fields[3].name, "Lines");
    assert_eq!(recordset.fields[1].ado_type.map(|ty| ty.code), Some(128));
    assert_eq!(recordset.fields[3].ado_type.map(|ty| ty.code), Some(136));

    assert_binary_parent_row(
        &recordset.rows[0].values,
        1,
        "0001000100000000",
        (1000173, 100017, 4),
        (1002291, 100229, 7),
    );
    assert_binary_parent_row(
        &recordset.rows[3].values,
        4,
        "0004000400000000",
        (1000203, 100020, 7),
        (1002321, 100232, 1),
    );
}

fn assert_binary_parent_row(
    values: &[Value],
    product_id: i64,
    product_sku: &str,
    expected_first: (i64, i64, i64),
    expected_last: (i64, i64, i64),
) {
    assert_eq!(values[0], Value::Integer(product_id));
    assert_eq!(values[1], Value::BinaryHex(product_sku.to_string()));
    assert!(matches!(values[2], Value::String(_)));

    let lines = chapter_at(values, 3, "Lines");
    assert_eq!(lines.fields.len(), 4, "binary Lines field count");
    assert_eq!(lines.rows.len(), 24, "binary Lines row count");
    assert_eq!(lines.fields[0].name, "ProductSku");
    assert_eq!(lines.fields[1].name, "LineId");
    assert_eq!(lines.fields[2].name, "OrderId");
    assert_eq!(lines.fields[3].name, "Quantity");

    for row in &lines.rows {
        assert_eq!(row.values[0], Value::BinaryHex(product_sku.to_string()));
    }

    let first = &lines.rows[0].values;
    assert_eq!(first[1], Value::Integer(expected_first.0));
    assert_eq!(first[2], Value::Integer(expected_first.1));
    assert_eq!(first[3], Value::Integer(expected_first.2));

    let last = &lines.rows[lines.rows.len() - 1].values;
    assert_eq!(last[1], Value::Integer(expected_last.0));
    assert_eq!(last[2], Value::Integer(expected_last.1));
    assert_eq!(last[3], Value::Integer(expected_last.2));
}

fn assert_rowversion_chapter_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 3, "rowversion parent field count");
    assert_eq!(recordset.rows.len(), 3, "rowversion parent row count");
    assert_eq!(recordset.fields[0].name, "LegacyRowVersion");
    assert_eq!(recordset.fields[1].name, "LegacyDocId");
    assert_eq!(recordset.fields[2].name, "LegacyRows");
    assert_eq!(recordset.fields[0].ado_type.map(|ty| ty.code), Some(128));
    assert_eq!(recordset.fields[2].ado_type.map(|ty| ty.code), Some(136));
    assert!(
        recordset.fields[0]
            .attributes
            .contains(&FieldAttribute::RowVersion),
        "parent rowversion field should preserve row-version metadata"
    );

    assert_rowversion_parent_row(&recordset.rows[0].values, "00000000000007D1", 1, 1000011);
    assert_rowversion_parent_row(&recordset.rows[1].values, "00000000000007D4", 2, 1000012);
    assert_rowversion_parent_row(&recordset.rows[2].values, "00000000000007D7", 3, 1000013);
}

fn assert_rowversion_parent_row(
    values: &[Value],
    rowversion: &str,
    legacy_doc_id: i64,
    line_id: i64,
) {
    assert_eq!(values[0], Value::BinaryHex(rowversion.to_string()));
    assert_eq!(values[1], Value::Integer(legacy_doc_id));

    let rows = chapter_at(values, 2, "LegacyRows");
    assert_eq!(rows.fields.len(), 4, "rowversion LegacyRows field count");
    assert_eq!(rows.fields[0].name, "LegacyRowVersion");
    assert_eq!(rows.fields[1].name, "LegacyDocId");
    assert_eq!(rows.fields[2].name, "LineId");
    assert_eq!(rows.fields[3].name, "LegacyCode");
    assert!(
        rows.fields[0]
            .attributes
            .contains(&FieldAttribute::RowVersion),
        "child rowversion field should preserve row-version metadata"
    );
    assert_eq!(rows.rows.len(), 1, "rowversion LegacyRows row count");

    let row = &rows.rows[0].values;
    assert_eq!(row[0], Value::BinaryHex(rowversion.to_string()));
    assert_eq!(row[1], Value::Integer(legacy_doc_id));
    assert_eq!(row[2], Value::Integer(line_id));
    assert_eq!(
        row[3],
        Value::String(format!("LG{legacy_doc_id:04}")),
        "LegacyCode"
    );
}

fn assert_boolean_relation_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 3, "boolean parent field count");
    assert_eq!(recordset.rows.len(), 2, "boolean parent rows");
    assert_eq!(recordset.fields[0].name, "IsDomestic");
    assert_eq!(recordset.fields[1].name, "Bucket");
    assert_eq!(recordset.fields[2].name, "Customers");

    assert_boolean_parent_row(
        &recordset.rows[0].values,
        true,
        "Domestic",
        &[
            (1, "CUST0001", "SEL"),
            (2, "CUST0002", "BUS"),
            (3, "CUST0003", "ICN"),
            (7, "CUST0007", "SEL"),
            (8, "CUST0008", "BUS"),
            (9, "CUST0009", "ICN"),
            (13, "CUST0013", "SEL"),
            (14, "CUST0014", "BUS"),
            (15, "CUST0015", "ICN"),
        ],
    );
    assert_boolean_parent_row(
        &recordset.rows[1].values,
        false,
        "International",
        &[
            (4, "CUST0004", "TYO"),
            (5, "CUST0005", "SFO"),
            (6, "CUST0006", "BER"),
            (10, "CUST0010", "TYO"),
            (11, "CUST0011", "SFO"),
            (12, "CUST0012", "BER"),
            (16, "CUST0016", "TYO"),
            (17, "CUST0017", "SFO"),
            (18, "CUST0018", "BER"),
        ],
    );
}

fn assert_boolean_parent_row(
    values: &[Value],
    is_domestic: bool,
    bucket: &str,
    expected_customers: &[(i64, &str, &str)],
) {
    assert_eq!(values[0], Value::Boolean(is_domestic));
    assert_eq!(values[1], Value::String(bucket.to_string()));

    let customers = chapter_at(values, 2, "Customers");
    assert_eq!(customers.fields.len(), 4, "boolean Customers field count");
    assert_eq!(customers.fields[0].name, "IsDomestic");
    assert_eq!(customers.fields[1].name, "CustomerId");
    assert_eq!(customers.fields[2].name, "CustomerCode");
    assert_eq!(customers.fields[3].name, "RegionCode");
    assert_eq!(
        customers.rows.len(),
        expected_customers.len(),
        "boolean-keyed Customers row count"
    );

    for (row, (customer_id, customer_code, region_code)) in
        customers.rows.iter().zip(expected_customers)
    {
        assert_eq!(row.values[0], Value::Boolean(is_domestic));
        assert_eq!(row.values[1], Value::Integer(*customer_id));
        assert_eq!(row.values[2], Value::String((*customer_code).to_string()));
        assert_eq!(row.values[3], Value::String((*region_code).to_string()));
    }
}

fn assert_nullable_key_chapter_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 4, "nullable-key parent field count");
    assert_eq!(recordset.rows.len(), 6, "nullable-key parent row count");
    assert_eq!(recordset.fields[0].name, "CustomerId");
    assert_eq!(recordset.fields[1].name, "NullableCustomerId");
    assert_eq!(recordset.fields[2].name, "CustomerName");
    assert_eq!(recordset.fields[3].name, "Orders");
    assert_eq!(recordset.fields[1].ado_type.map(|ty| ty.code), Some(3));
    assert!(recordset.fields[1].nullable);
    assert_eq!(recordset.fields[3].ado_type.map(|ty| ty.code), Some(136));

    assert_nullable_key_parent_row(
        &recordset.rows[0].values,
        1,
        Some(1),
        &[
            (1, 100001, "7.35"),
            (1, 100049, "40.25"),
            (1, 100097, "33.2"),
            (1, 100145, "26.15"),
            (1, 100193, "19.1"),
        ],
    );
    assert_nullable_key_parent_row(&recordset.rows[1].values, 2, None, &[]);
    assert_nullable_key_parent_row(
        &recordset.rows[2].values,
        3,
        Some(3),
        &[
            (3, 100003, "12.05"),
            (3, 100051, "5"),
            (3, 100099, "37.9"),
            (3, 100147, "30.85"),
            (3, 100195, "23.8"),
        ],
    );
    assert_nullable_key_parent_row(&recordset.rows[3].values, 4, None, &[]);
    assert_nullable_key_parent_row(
        &recordset.rows[4].values,
        5,
        Some(5),
        &[
            (5, 100005, "16.75"),
            (5, 100053, "9.7"),
            (5, 100101, "42.6"),
            (5, 100149, "35.55"),
            (5, 100197, "28.5"),
        ],
    );
    assert_nullable_key_parent_row(&recordset.rows[5].values, 6, None, &[]);
}

fn assert_nullable_key_parent_row(
    values: &[Value],
    customer_id: i64,
    nullable_customer_id: Option<i64>,
    expected_orders: &[(i64, i64, &str)],
) {
    assert_eq!(values[0], Value::Integer(customer_id));
    match nullable_customer_id {
        Some(nullable_customer_id) => assert_eq!(values[1], Value::Integer(nullable_customer_id)),
        None => assert_eq!(values[1], Value::Null),
    }
    assert!(matches!(values[2], Value::String(_)));

    let orders = chapter_at(values, 3, "Orders");
    assert_eq!(orders.fields.len(), 3, "nullable-key Orders field count");
    assert_eq!(
        orders.rows.len(),
        expected_orders.len(),
        "nullable-key Orders row count for parent {customer_id}"
    );
    assert_eq!(orders.fields[0].name, "NullableCustomerId");
    assert_eq!(orders.fields[1].name, "OrderId");
    assert_eq!(orders.fields[2].name, "Freight");

    for (row, (child_customer_id, order_id, freight)) in orders.rows.iter().zip(expected_orders) {
        assert_eq!(row.values[0], Value::Integer(*child_customer_id));
        assert_eq!(row.values[1], Value::Integer(*order_id));
        assert_eq!(row.values[2], Value::Decimal((*freight).to_string()));
    }
}

fn assert_duplicate_parent_chapter_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 4, "duplicate-parent field count");
    assert_eq!(recordset.rows.len(), 6, "duplicate-parent row count");
    assert_eq!(recordset.fields[0].name, "RegionCode");
    assert_eq!(recordset.fields[1].name, "CustomerId");
    assert_eq!(recordset.fields[2].name, "CustomerCode");
    assert_eq!(recordset.fields[3].name, "Orders");
    assert_eq!(recordset.fields[0].ado_type.map(|ty| ty.code), Some(129));
    assert_eq!(recordset.fields[3].ado_type.map(|ty| ty.code), Some(136));

    for (row, (customer_id, customer_code)) in recordset.rows.iter().zip([
        (1, "CUST0001"),
        (7, "CUST0007"),
        (13, "CUST0013"),
        (19, "CUST0019"),
        (25, "CUST0025"),
        (31, "CUST0031"),
    ]) {
        assert_eq!(row.values[0], Value::String("SEL".to_string()));
        assert_eq!(row.values[1], Value::Integer(customer_id));
        assert_eq!(row.values[2], Value::String(customer_code.to_string()));
        assert_duplicate_parent_orders(&row.values);
    }
}

fn assert_duplicate_parent_orders(values: &[Value]) {
    let orders = chapter_at(values, 3, "Orders");
    assert_eq!(orders.fields.len(), 4, "duplicate-parent Orders fields");
    assert_eq!(orders.rows.len(), 10, "duplicate-parent Orders row count");
    assert_eq!(orders.fields[0].name, "RegionCode");
    assert_eq!(orders.fields[1].name, "OrderId");
    assert_eq!(orders.fields[2].name, "CustomerId");
    assert_eq!(orders.fields[3].name, "Freight");

    let expected = [
        (100001, 1, "7.35"),
        (100007, 7, "21.45"),
        (100049, 1, "40.25"),
        (100055, 7, "14.4"),
        (100097, 1, "33.2"),
        (100103, 7, "7.35"),
        (100145, 1, "26.15"),
        (100151, 7, "40.25"),
        (100193, 1, "19.1"),
        (100199, 7, "33.2"),
    ];

    for (row, (order_id, customer_id, freight)) in orders.rows.iter().zip(expected) {
        assert_eq!(row.values[0], Value::String("SEL".to_string()));
        assert_eq!(row.values[1], Value::Integer(order_id));
        assert_eq!(row.values[2], Value::Integer(customer_id));
        assert_eq!(row.values[3], Value::Decimal(freight.to_string()));
    }
}

fn assert_nullable_chapter_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 4, "nullable parent field count");
    assert_eq!(recordset.rows.len(), 5, "nullable parent row count");
    assert_eq!(recordset.fields[2].name, "CustomerNotes");
    assert!(
        recordset.fields[2].nullable,
        "CustomerNotes should be nullable"
    );

    assert_nullable_parent_row(
        &recordset.rows[0].values,
        100001,
        1,
        Some("Notes row 1 & <sales> \"mixed\""),
        &[
            (100001, 1000011, 1, 4, None),
            (100001, 1000012, 2, 5, Some("line comment 100001-2")),
            (100001, 1000013, 3, 6, Some("line comment 100001-3")),
        ],
    );
    assert_nullable_parent_row(
        &recordset.rows[4].values,
        100005,
        5,
        None,
        &[
            (100005, 1000051, 1, 8, Some("line comment 100005-1")),
            (100005, 1000052, 2, 9, Some("line comment 100005-2")),
            (100005, 1000053, 3, 1, Some("line comment 100005-3")),
        ],
    );
}

fn assert_nullable_parent_row(
    values: &[Value],
    order_id: i64,
    customer_id: i64,
    customer_notes: Option<&str>,
    expected_lines: &[(i64, i64, i64, i64, Option<&str>)],
) {
    assert_eq!(values[0], Value::Integer(order_id));
    assert_eq!(values[1], Value::Integer(customer_id));
    match customer_notes {
        Some(value) => assert_eq!(values[2], Value::String(value.to_string())),
        None => assert_eq!(values[2], Value::Null),
    }

    let lines = chapter_at(values, 3, "Lines");
    assert_eq!(lines.fields.len(), 5, "nullable Lines field count");
    assert_eq!(
        lines.rows.len(),
        expected_lines.len(),
        "nullable Lines row count"
    );
    assert_eq!(lines.fields[0].name, "OrderId");
    assert_eq!(lines.fields[1].name, "LineId");
    assert_eq!(lines.fields[2].name, "LineNumber");
    assert_eq!(lines.fields[3].name, "Quantity");
    assert_eq!(lines.fields[4].name, "LineComment");
    assert!(
        lines.fields[4].nullable,
        "LineComment should be nullable in chapter rows"
    );

    for (row, (line_order_id, line_id, line_number, quantity, line_comment)) in
        lines.rows.iter().zip(expected_lines)
    {
        assert_eq!(row.values[0], Value::Integer(*line_order_id));
        assert_eq!(row.values[1], Value::Integer(*line_id));
        assert_eq!(row.values[2], Value::Integer(*line_number));
        assert_eq!(row.values[3], Value::Integer(*quantity));
        match line_comment {
            Some(value) => assert_eq!(row.values[4], Value::String((*value).to_string())),
            None => assert_eq!(row.values[4], Value::Null),
        }
    }
}

fn assert_wide_nullable_chapter_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(
        recordset.fields.len(),
        4,
        "wide-nullable parent field count"
    );
    assert_eq!(recordset.rows.len(), 2, "wide-nullable parent row count");

    let first_parent = &recordset.rows[0].values;
    assert_eq!(first_parent[0], Value::Integer(100001));
    assert_eq!(first_parent[1], Value::Integer(1));
    assert_eq!(first_parent[2], Value::Decimal("7.35".to_string()));

    let lines = chapter_at(first_parent, 3, "Lines");
    assert_eq!(lines.fields.len(), 13, "wide-nullable Lines field count");
    assert_eq!(lines.rows.len(), 3, "wide-nullable Lines row count");
    assert_eq!(
        lines
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "OrderId",
            "LineId",
            "LineNumber",
            "LineComment",
            "MaybeComment2",
            "CustomerNotes",
            "ProductDescription",
            "ReceivedAt",
            "TrackingNumber",
            "ShipLabel",
            "MaybeUnitPrice",
            "MaybeDiscount",
            "MaybeProductGuid",
        ]
    );
    for field in &lines.fields[3..=12] {
        assert!(field.nullable, "{} should be nullable", field.name);
    }

    let row0 = &lines.rows[0].values;
    assert_eq!(row0[0], Value::Integer(100001));
    assert_eq!(row0[1], Value::Integer(1000011));
    assert_eq!(row0[2], Value::Integer(1));
    assert_eq!(row0[3], Value::Null);
    assert_eq!(row0[4], Value::Null);
    assert_eq!(
        row0[5],
        Value::String("Notes row 1 & <sales> \"mixed\"".to_string())
    );
    assert_nonempty_string(&row0[6], "ProductDescription row0");
    assert_eq!(row0[7], Value::DateTime("2024-01-01T17:00:00".to_string()));
    assert_eq!(row0[8], Value::String("TRK100001".to_string()));
    assert_eq!(
        row0[9],
        Value::BinaryHex("22D4C28F3579051F67D0D5ED6707AFF655542DE8".to_string())
    );
    assert_eq!(row0[10], Value::Decimal("99.88".to_string()));
    assert_eq!(row0[11], Value::Decimal("0".to_string()));
    assert_eq!(row0[12], Value::Null);

    let row1 = &lines.rows[1].values;
    assert_eq!(row1[0], Value::Integer(100001));
    assert_eq!(row1[1], Value::Integer(1000012));
    assert_eq!(row1[3], Value::String("line comment 100001-2".to_string()));
    assert_eq!(row1[4], Value::String("line comment 100001-2".to_string()));
    assert_nonempty_string(&row1[6], "ProductDescription row1");
    assert_eq!(row1[10], Value::Null);
    assert_eq!(row1[11], Value::Decimal("0.01".to_string()));
    assert_eq!(
        row1[12],
        Value::Guid("{00000014-AAAA-BBBB-CCCC-000000000014}".to_string())
    );

    let row2 = &lines.rows[2].values;
    assert_eq!(row2[0], Value::Integer(100001));
    assert_eq!(row2[1], Value::Integer(1000013));
    assert_eq!(row2[10], Value::Decimal("106.3".to_string()));
    assert_eq!(row2[11], Value::Null);
    assert_eq!(
        row2[12],
        Value::Guid("{00000015-AAAA-BBBB-CCCC-000000000015}".to_string())
    );

    let second_lines = chapter_at(&recordset.rows[1].values, 3, "Lines");
    assert_eq!(second_lines.rows.len(), 3, "second parent Lines row count");
    assert_eq!(second_lines.rows[0].values[0], Value::Integer(100002));
    assert_eq!(
        second_lines.rows[0].values[7],
        Value::DateTime("2024-01-01T18:00:00".to_string())
    );
}

fn assert_nonempty_string(value: &Value, label: &str) {
    match value {
        Value::String(value) if !value.is_empty() => {}
        other => panic!("{label} expected non-empty string, got {other:?}"),
    }
}

fn assert_string_starts_with(value: &Value, expected_prefix: &str, label: &str) {
    match value {
        Value::String(value) if value.starts_with(expected_prefix) => {}
        other => panic!("{label} expected prefix {expected_prefix:?}, got {other:?}"),
    }
}

fn assert_sparse_child_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 4, "sparse parent field count");
    assert_eq!(recordset.rows.len(), 5, "sparse parent row count");

    assert_sparse_parent_row(
        &recordset.rows[0].values,
        100001,
        &[
            (100001, 1000011, 4),
            (100001, 1000012, 5),
            (100001, 1000013, 6),
        ],
    );
    assert_sparse_parent_row(&recordset.rows[1].values, 100002, &[]);
    assert_sparse_parent_row(
        &recordset.rows[2].values,
        100003,
        &[
            (100003, 1000031, 6),
            (100003, 1000032, 7),
            (100003, 1000033, 8),
        ],
    );
    assert_sparse_parent_row(&recordset.rows[4].values, 100005, &[]);
}

fn assert_empty_child_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 4, "empty-child parent field count");
    assert_eq!(recordset.rows.len(), 3, "empty-child parent row count");

    for (row, order_id) in recordset.rows.iter().zip([100001, 100002, 100003]) {
        assert_sparse_parent_row(&row.values, order_id, &[]);
    }
}

fn assert_empty_parent_recordset_schema(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 4, "empty-parent field count");
    assert_eq!(
        recordset
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["OrderId", "CustomerId", "Freight", "Lines"],
        "empty-parent field names"
    );
    assert_eq!(recordset.fields[0].ado_type.map(|ty| ty.code), Some(3));
    assert_eq!(recordset.fields[1].ado_type.map(|ty| ty.code), Some(3));
    assert_eq!(recordset.fields[2].ado_type.map(|ty| ty.code), Some(6));
    assert_eq!(recordset.fields[3].ado_type.map(|ty| ty.code), Some(136));
    assert_eq!(recordset.rows.len(), 0, "empty-parent row count");
    assert_eq!(recordset.changes.len(), 0, "empty-parent changes");

    let default = materialize_default_view(recordset);
    assert_eq!(default.fields.len(), 4, "empty-parent default fields");
    assert_eq!(default.rows.len(), 0, "empty-parent default rows");
}

fn assert_sparse_parent_row(values: &[Value], order_id: i64, expected_lines: &[(i64, i64, i64)]) {
    assert_eq!(values[0], Value::Integer(order_id));
    let lines = chapter_at(values, 3, "Lines");
    assert_eq!(lines.fields.len(), 3, "sparse Lines field count");
    assert_eq!(
        lines.rows.len(),
        expected_lines.len(),
        "sparse Lines row count"
    );
    assert_eq!(lines.fields[0].name, "OrderId");
    assert_eq!(lines.fields[1].name, "LineId");
    assert_eq!(lines.fields[2].name, "Quantity");

    for (row, (line_order_id, line_id, quantity)) in lines.rows.iter().zip(expected_lines) {
        assert_eq!(row.values[0], Value::Integer(*line_order_id));
        assert_eq!(row.values[1], Value::Integer(*line_id));
        assert_eq!(row.values[2], Value::Integer(*quantity));
    }
}

fn assert_nested_chapter_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 4, "nested parent field count");
    assert_eq!(recordset.rows.len(), 3, "nested parent row count");

    assert_nested_parent_row(
        &recordset.rows[0].values,
        100001,
        1,
        "7.35",
        &[
            (100001, 1000011, 1, 13, 4, "20.31"),
            (100001, 1000012, 2, 14, 5, "21.68"),
            (100001, 1000013, 3, 15, 6, "23.05"),
        ],
    );
    assert_nested_parent_row(
        &recordset.rows[2].values,
        100003,
        3,
        "12.05",
        &[
            (100003, 1000031, 1, 15, 6, "23.05"),
            (100003, 1000032, 2, 16, 7, "24.42"),
            (100003, 1000033, 3, 17, 8, "25.79"),
        ],
    );
}

fn assert_nested_parent_row(
    values: &[Value],
    order_id: i64,
    customer_id: i64,
    freight: &str,
    expected_lines: &[(i64, i64, i64, i64, i64, &str)],
) {
    assert_eq!(values[0], Value::Integer(order_id));
    assert_eq!(values[1], Value::Integer(customer_id));
    assert_eq!(values[2], Value::Decimal(freight.to_string()));

    let lines = chapter_at(values, 3, "Lines");
    assert_eq!(lines.fields.len(), 6, "nested Lines field count");
    assert_eq!(
        lines.rows.len(),
        expected_lines.len(),
        "nested Lines row count"
    );
    assert_eq!(lines.fields[0].name, "OrderId");
    assert_eq!(lines.fields[1].name, "LineId");
    assert_eq!(lines.fields[2].name, "LineNumber");
    assert_eq!(lines.fields[3].name, "ProductId");
    assert_eq!(lines.fields[4].name, "Quantity");
    assert_eq!(lines.fields[5].name, "Product");

    for (row, (line_order_id, line_id, line_number, product_id, quantity, unit_cost)) in
        lines.rows.iter().zip(expected_lines)
    {
        assert_eq!(row.values[0], Value::Integer(*line_order_id));
        assert_eq!(row.values[1], Value::Integer(*line_id));
        assert_eq!(row.values[2], Value::Integer(*line_number));
        assert_eq!(row.values[3], Value::Integer(*product_id));
        assert_eq!(row.values[4], Value::Integer(*quantity));

        let product = chapter_at(&row.values, 5, "Product");
        assert_eq!(product.fields.len(), 3, "nested Product field count");
        assert_eq!(product.rows.len(), 1, "nested Product row count");
        assert_eq!(product.fields[0].name, "ProductId");
        assert_eq!(product.fields[1].name, "ProductName");
        assert_eq!(product.fields[2].name, "UnitCost");
        assert_eq!(product.rows[0].values[0], Value::Integer(*product_id));
        assert!(matches!(product.rows[0].values[1], Value::String(_)));
        assert_eq!(
            product.rows[0].values[2],
            Value::Decimal((*unit_cost).to_string())
        );
    }
}

fn assert_nested_sibling_grandchild_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(
        recordset.fields.len(),
        4,
        "nested sibling parent field count"
    );
    assert_eq!(recordset.rows.len(), 2, "nested sibling parent row count");

    assert_nested_sibling_grandchild_parent_row(
        &recordset.rows[0].values,
        100001,
        1,
        "7.35",
        &[
            (100001, 1000011, 1, 13, 4, "99.88", "20.31", 1, "LG0001"),
            (100001, 1000012, 2, 14, 5, "103.09", "21.68", 2, "LG0002"),
            (100001, 1000013, 3, 15, 6, "106.3", "23.05", 3, "LG0003"),
        ],
    );
    assert_nested_sibling_grandchild_parent_row(
        &recordset.rows[1].values,
        100002,
        2,
        "9.7",
        &[
            (100002, 1000021, 1, 14, 5, "103.09", "21.68", 4, "LG0004"),
            (100002, 1000022, 2, 15, 6, "106.3", "23.05", 5, "LG0005"),
            (100002, 1000023, 3, 16, 7, "109.51", "24.42", 6, "LG0006"),
        ],
    );
}

fn assert_nested_sibling_grandchild_parent_row(
    values: &[Value],
    order_id: i64,
    customer_id: i64,
    freight: &str,
    expected_lines: &[NestedSiblingGrandchildLineExpectation<'_>],
) {
    assert_eq!(values[0], Value::Integer(order_id));
    assert_eq!(values[1], Value::Integer(customer_id));
    assert_eq!(values[2], Value::Decimal(freight.to_string()));

    let lines = chapter_at(values, 3, "Lines");
    assert_eq!(lines.fields.len(), 8, "nested sibling Lines field count");
    assert_eq!(
        lines.rows.len(),
        expected_lines.len(),
        "nested sibling Lines row count"
    );
    assert_eq!(lines.fields[0].name, "OrderId");
    assert_eq!(lines.fields[1].name, "LineId");
    assert_eq!(lines.fields[2].name, "LineNumber");
    assert_eq!(lines.fields[3].name, "ProductId");
    assert_eq!(lines.fields[4].name, "Quantity");
    assert_eq!(lines.fields[5].name, "UnitPrice");
    assert_eq!(lines.fields[6].name, "Product");
    assert_eq!(lines.fields[7].name, "Legacy");

    for (
        row,
        (
            line_order_id,
            line_id,
            line_number,
            product_id,
            quantity,
            unit_price,
            unit_cost,
            legacy_doc_id,
            legacy_code,
        ),
    ) in lines.rows.iter().zip(expected_lines)
    {
        assert_eq!(row.values[0], Value::Integer(*line_order_id));
        assert_eq!(row.values[1], Value::Integer(*line_id));
        assert_eq!(row.values[2], Value::Integer(*line_number));
        assert_eq!(row.values[3], Value::Integer(*product_id));
        assert_eq!(row.values[4], Value::Integer(*quantity));
        assert_eq!(row.values[5], Value::Decimal((*unit_price).to_string()));

        let product = chapter_at(&row.values, 6, "Product");
        assert_eq!(
            product.fields.len(),
            4,
            "nested sibling Product field count"
        );
        assert_eq!(product.rows.len(), 1, "nested sibling Product row count");
        assert_eq!(product.fields[0].name, "ProductId");
        assert_eq!(product.fields[1].name, "ProductName");
        assert_eq!(product.fields[2].name, "ProductSku");
        assert_eq!(product.fields[3].name, "UnitCost");
        assert_eq!(product.rows[0].values[0], Value::Integer(*product_id));
        assert_string_starts_with(
            &product.rows[0].values[1],
            &format!("Product {product_id} "),
            "nested sibling ProductName",
        );
        assert!(matches!(product.rows[0].values[2], Value::BinaryHex(_)));
        assert_eq!(
            product.rows[0].values[3],
            Value::Decimal((*unit_cost).to_string())
        );

        let legacy = chapter_at(&row.values, 7, "Legacy");
        assert_eq!(legacy.fields.len(), 4, "nested sibling Legacy field count");
        assert_eq!(legacy.rows.len(), 1, "nested sibling Legacy row count");
        assert_eq!(legacy.fields[0].name, "LineId");
        assert_eq!(legacy.fields[1].name, "LegacyDocId");
        assert_eq!(legacy.fields[2].name, "LegacyCode");
        assert_eq!(legacy.fields[3].name, "LegacyRowVersion");
        assert_eq!(legacy.rows[0].values[0], Value::Integer(*line_id));
        assert_eq!(legacy.rows[0].values[1], Value::Integer(*legacy_doc_id));
        assert_eq!(
            legacy.rows[0].values[2],
            Value::String((*legacy_code).to_string())
        );
        assert!(
            matches!(&legacy.rows[0].values[3], Value::BinaryHex(hex) if hex.len() == 16),
            "nested sibling LegacyRowVersion should be an 8-byte rowversion"
        );
    }
}

fn assert_deep_nested_chapter_recordset_rows(recordset: &tablegram::Recordset) {
    assert_eq!(recordset.fields.len(), 4, "deep parent field count");
    assert_eq!(recordset.rows.len(), 2, "deep parent row count");

    assert_deep_nested_parent_row(
        &recordset.rows[0].values,
        100001,
        1,
        "7.35",
        &[
            (100001, 1000011, 1, 13, 4, 5, "20.31", "0.6583"),
            (100001, 1000012, 2, 14, 5, 6, "21.68", "0.7833"),
            (100001, 1000013, 3, 15, 6, 7, "23.05", "0.9083"),
        ],
    );
    assert_deep_nested_parent_row(
        &recordset.rows[1].values,
        100002,
        2,
        "9.7",
        &[
            (100002, 1000021, 1, 14, 5, 6, "21.68", "0.7833"),
            (100002, 1000022, 2, 15, 6, 7, "23.05", "0.9083"),
            (100002, 1000023, 3, 16, 7, 8, "24.42", "1.0333"),
        ],
    );
}

fn assert_deep_nested_parent_row(
    values: &[Value],
    order_id: i64,
    customer_id: i64,
    freight: &str,
    expected_lines: &[DeepNestedLineExpectation<'_>],
) {
    assert_eq!(values[0], Value::Integer(order_id));
    assert_eq!(values[1], Value::Integer(customer_id));
    assert_eq!(values[2], Value::Decimal(freight.to_string()));

    let lines = chapter_at(values, 3, "Lines");
    assert_eq!(lines.fields.len(), 6, "deep Lines field count");
    assert_eq!(
        lines.rows.len(),
        expected_lines.len(),
        "deep Lines row count"
    );

    for (
        row,
        (
            line_order_id,
            line_id,
            line_number,
            product_id,
            quantity,
            category_id,
            unit_cost,
            margin_target,
        ),
    ) in lines.rows.iter().zip(expected_lines)
    {
        assert_eq!(row.values[0], Value::Integer(*line_order_id));
        assert_eq!(row.values[1], Value::Integer(*line_id));
        assert_eq!(row.values[2], Value::Integer(*line_number));
        assert_eq!(row.values[3], Value::Integer(*product_id));
        assert_eq!(row.values[4], Value::Integer(*quantity));

        let product = chapter_at(&row.values, 5, "Product");
        assert_eq!(product.fields.len(), 5, "deep Product field count");
        assert_eq!(product.rows.len(), 1, "deep Product row count");
        assert_eq!(product.fields[0].name, "ProductId");
        assert_eq!(product.fields[1].name, "CategoryId");
        assert_eq!(product.fields[2].name, "ProductName");
        assert_eq!(product.fields[3].name, "UnitCost");
        assert_eq!(product.fields[4].name, "Category");
        assert_eq!(product.rows[0].values[0], Value::Integer(*product_id));
        assert_eq!(
            product.rows[0].values[1],
            Value::UnsignedInteger(*category_id as u64)
        );
        assert_eq!(
            product.rows[0].values[3],
            Value::Decimal((*unit_cost).to_string())
        );

        let category = chapter_at(&product.rows[0].values, 4, "Category");
        assert_eq!(category.fields.len(), 3, "deep Category field count");
        assert_eq!(category.rows.len(), 1, "deep Category row count");
        assert_eq!(category.fields[0].name, "CategoryId");
        assert_eq!(category.fields[1].name, "CategoryName");
        assert_eq!(category.fields[2].name, "MarginTarget");
        assert_eq!(
            category.rows[0].values[0],
            Value::UnsignedInteger(*category_id as u64)
        );
        assert_eq!(
            category.rows[0].values[1],
            Value::String(format!("Category {category_id}"))
        );
        assert_eq!(
            category.rows[0].values[2],
            Value::Decimal((*margin_target).to_string())
        );
    }
}

fn chapter_at<'a>(values: &'a [Value], index: usize, name: &str) -> &'a tablegram::Recordset {
    let Value::Chapter(chapter) = &values[index] else {
        panic!("expected {name} chapter value, got {:?}", values[index]);
    };
    chapter
}

fn add_shape_namespace(source: &str, namespace_decl: &str) -> String {
    source.replace(
        "xmlns:z='#RowsetSchema'>",
        &format!("xmlns:z='#RowsetSchema'\n\t{namespace_decl}>"),
    )
}

fn replace_first_nested_lines_block(source: &str, replacement: &str) -> String {
    let open =
        "<Lines OrderId='100001' LineId='1000011' LineNumber='1' ProductId='13' Quantity='4'>";
    let start = source
        .find(open)
        .expect("nested fixture should contain first Lines row");
    let end = source[start..]
        .find("</Lines>")
        .map(|relative| start + relative + "</Lines>".len())
        .expect("nested fixture should contain first Lines end tag");
    format!("{}{}{}", &source[..start], replacement, &source[end..])
}

fn first_chapter(recordset: &tablegram::Recordset) -> &tablegram::Recordset {
    let Value::Chapter(chapter) = &recordset.rows[0].values[3] else {
        panic!("expected first row Lines chapter");
    };
    chapter
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
