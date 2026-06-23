use tablegram::compat::materialize_pending_view;
use tablegram::model::{FieldAttribute, RecordStatusFlag, RowState, Value};
use tablegram::xml::{parse_ado_xml, parse_ado_xml_bytes};

const SAMPLE: &str = r##"<?xml version="1.0"?>
<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"
     xmlns:dt="uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"
     xmlns:rs="urn:schemas-microsoft-com:rowset"
     xmlns:z="#RowsetSchema">
  <s:Schema id="RowsetSchema">
    <s:ElementType name="row" content="eltOnly" rs:updatable="true">
      <s:AttributeType name="ID" rs:number="1">
        <s:datatype dt:type="int" dt:maxLength="4" rs:maybenull="false"/>
      </s:AttributeType>
      <s:AttributeType name="s1" rs:name="NAME" rs:number="2">
        <s:datatype dt:type="string" dt:maxLength="255" rs:maybenull="true"/>
      </s:AttributeType>
      <s:AttributeType name="PAYLOAD" rs:number="3">
        <s:datatype dt:type="bin.hex" rs:maybenull="true"/>
      </s:AttributeType>
      <s:extends type="rs:rowbase"/>
    </s:ElementType>
  </s:Schema>
  <rs:data>
    <z:row ID="1" s1="&#xD55C;&#xAE00;" PAYLOAD="DEADBEEF"/>
    <z:row ID="2" s1=""/>
  </rs:data>
</xml>"##;

#[test]
fn parses_ado_xml_schema_and_rows() {
    let recordset = parse_ado_xml(SAMPLE).unwrap();

    assert_eq!(recordset.fields.len(), 3);
    assert_eq!(recordset.fields[0].name, "ID");
    assert_eq!(recordset.fields[1].name, "NAME");
    assert_eq!(recordset.fields[1].xml_name, "s1");

    assert_eq!(recordset.rows.len(), 2);
    assert_eq!(recordset.rows[0].state, RowState::Current);
    assert_eq!(recordset.rows[0].values[0], Value::Integer(1));
    assert_eq!(
        recordset.rows[0].values[1],
        Value::String("한글".to_string())
    );
    assert_eq!(
        recordset.rows[0].values[2],
        Value::BinaryHex("DEADBEEF".to_string())
    );

    assert_eq!(recordset.rows[1].values[1], Value::String(String::new()));
    assert_eq!(recordset.rows[1].values[2], Value::Null);
}

#[test]
fn parses_unicode_xml_attribute_names_without_raw_attribute_drift() {
    let xml = r##"<?xml version="1.0"?>
<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"
     xmlns:dt="uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"
     xmlns:rs="urn:schemas-microsoft-com:rowset"
     xmlns:z="#RowsetSchema">
  <s:Schema id="RowsetSchema">
    <s:ElementType name="row" content="eltOnly">
      <s:AttributeType name="ID" rs:number="1">
        <s:datatype dt:type="int" dt:maxLength="4"/>
      </s:AttributeType>
      <s:AttributeType name="값" rs:number="2">
        <s:datatype dt:type="string" dt:maxLength="255" rs:maybenull="true"/>
      </s:AttributeType>
      <s:extends type="rs:rowbase"/>
    </s:ElementType>
  </s:Schema>
  <rs:data>
    <z:row ID="1" 값="한글 &amp; 값"/>
  </rs:data>
</xml>"##;

    let recordset = parse_ado_xml(xml).unwrap();

    assert_eq!(
        recordset
            .fields
            .iter()
            .map(|field| (field.name.as_str(), field.xml_name.as_str()))
            .collect::<Vec<_>>(),
        vec![("ID", "ID"), ("값", "값")]
    );
    assert_eq!(recordset.rows.len(), 1);
    assert_eq!(recordset.rows[0].values[0], Value::Integer(1));
    assert_eq!(
        recordset.rows[0].values[1],
        Value::String("한글 & 값".to_string())
    );
}

#[test]
fn rejects_invalid_numeric_xml_character_entities() {
    let legal = SAMPLE.replace(r#"s1="&#xD55C;&#xAE00;""#, r#"s1="a&#x9;b&#10;c&#xD;d""#);
    let recordset = parse_ado_xml(&legal).unwrap();
    assert_eq!(
        recordset.rows[0].values[1],
        Value::String("a\tb\nc\rd".to_string())
    );

    for (label, entity) in [("nul", "&#x0;"), ("unit separator", "&#31;")] {
        let xml = SAMPLE.replace(r#"s1="&#xD55C;&#xAE00;""#, &format!(r#"s1="{entity}""#));
        let err = parse_ado_xml(&xml).expect_err(label);
        let message = format!("{err:#}");
        assert!(
            message.contains("invalid XML character entity")
                || message.contains("malformed entity reference"),
            "{label}: {message}"
        );
    }
}

#[test]
fn parses_xml_comments_and_cdata_without_raw_attribute_drift() {
    let xml = SAMPLE
        .replace(
            "  <s:Schema",
            "  <!-- <s:AttributeType name=\"BOGUS\" dt:type=\"int\"/> -->\n  <![CDATA[<s:AttributeType name=\"BOGUS_CDATA\" dt:type=\"int\"/>]]>\n  <s:Schema",
        )
        .replace(
            "  <rs:data>",
            "  <rs:data>\n    <!-- <z:row ID=\"999\" s1=\"comment\" PAYLOAD=\"00\"/> -->\n    <![CDATA[<z:row ID=\"998\" s1=\"cdata\" PAYLOAD=\"11\"/>]]>",
        );

    let recordset = parse_ado_xml(&xml).unwrap();

    assert_eq!(
        recordset
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["ID", "NAME", "PAYLOAD"]
    );
    assert_eq!(recordset.rows.len(), 2);
    assert_eq!(recordset.rows[0].values[0], Value::Integer(1));
    assert_eq!(
        recordset.rows[0].values[1],
        Value::String("한글".to_string())
    );
    assert_eq!(
        recordset.rows[0].values[2],
        Value::BinaryHex("DEADBEEF".to_string())
    );
    assert_eq!(recordset.rows[1].values[0], Value::Integer(2));
    assert_eq!(recordset.rows[1].values[1], Value::String(String::new()));
    assert_eq!(recordset.rows[1].values[2], Value::Null);
}

#[test]
fn parses_minimal_xml_schema_defaults_like_mdac() {
    let xml = r##"<?xml version="1.0"?>
<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"
     xmlns:dt="uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"
     xmlns:rs="urn:schemas-microsoft-com:rowset"
     xmlns:z="#RowsetSchema">
  <s:Schema id="RowsetSchema">
    <s:ElementType name="row" content="eltOnly">
      <s:AttributeType name="ID" dt:type="int"/>
      <s:AttributeType name="s1" rs:name="Friendly Name"/>
      <s:AttributeType name="s2" rs:name="Direct Int" dt:type="int" rs:maybenull="true"/>
      <s:AttributeType name="s3" rs:name="Direct Binary" dt:type="bin.hex" rs:maybenull="true"/>
      <s:Extends type="rs:rowbase"/>
    </s:ElementType>
  </s:Schema>
  <rs:data>
    <z:row ID="1" s1="alpha" s2="42" s3="000102" ignored="not in schema"/>
    <z:row ID="2" s1=""/>
  </rs:data>
</xml>"##;

    let recordset = parse_ado_xml(xml).unwrap();

    assert_eq!(
        recordset
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["ID", "Friendly Name", "Direct Int", "Direct Binary"]
    );
    assert_eq!(
        recordset
            .fields
            .iter()
            .map(|field| field.ado_type.map(|ty| ty.code))
            .collect::<Vec<_>>(),
        vec![Some(3), Some(203), Some(3), Some(205)]
    );
    assert_eq!(
        recordset
            .fields
            .iter()
            .map(|field| field.attributes.as_slice())
            .collect::<Vec<_>>(),
        vec![
            &[FieldAttribute::Fixed, FieldAttribute::MayBeNull][..],
            &[FieldAttribute::Long, FieldAttribute::MayBeNull][..],
            &[FieldAttribute::Fixed, FieldAttribute::MayBeNull][..],
            &[FieldAttribute::Long, FieldAttribute::MayBeNull][..],
        ]
    );
    assert_eq!(
        recordset.rows[0].values,
        vec![
            Value::Integer(1),
            Value::String("alpha".to_string()),
            Value::Integer(42),
            Value::BinaryHex("000102".to_string()),
        ]
    );
    assert_eq!(
        recordset.rows[1].values,
        vec![
            Value::Integer(2),
            Value::String(String::new()),
            Value::Null,
            Value::Null,
        ]
    );
}

#[test]
fn parses_row_attribute_refs_as_field_membership_and_order_like_mdac() {
    let xml = r##"<?xml version="1.0"?>
<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"
     xmlns:dt="uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"
     xmlns:rs="urn:schemas-microsoft-com:rowset"
     xmlns:z="#RowsetSchema">
  <s:Schema id="RowsetSchema">
    <s:ElementType name="row" content="eltOnly">
      <s:attribute type="s2"/>
      <s:attribute type="s1"/>
      <s:attribute type="s3"/>
      <s:extends type="rs:rowbase"/>
    </s:ElementType>
    <s:AttributeType name="s1" rs:name="Number After Text" dt:type="int"/>
    <s:AttributeType name="s2" rs:name="Text First"/>
    <s:AttributeType name="unused" dt:type="int"/>
    <s:AttributeType name="s3" rs:name="Binary Third" dt:type="bin.hex" rs:maybenull="true"/>
  </s:Schema>
  <rs:data>
    <z:row s1="10" s2="alpha" s3="0A0B" unused="99"/>
    <z:row s1="20" s2="beta"/>
  </rs:data>
</xml>"##;

    let recordset = parse_ado_xml(xml).unwrap();

    assert_eq!(
        recordset
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Text First", "Number After Text", "Binary Third"]
    );
    assert_eq!(
        recordset
            .fields
            .iter()
            .map(|field| field.xml_name.as_str())
            .collect::<Vec<_>>(),
        vec!["s2", "s1", "s3"]
    );
    assert_eq!(
        recordset
            .fields
            .iter()
            .map(|field| field.ado_type.map(|ty| ty.code))
            .collect::<Vec<_>>(),
        vec![Some(203), Some(3), Some(205)]
    );
    assert_eq!(
        recordset.rows[0].values,
        vec![
            Value::String("alpha".to_string()),
            Value::Integer(10),
            Value::BinaryHex("0A0B".to_string()),
        ]
    );
    assert_eq!(
        recordset.rows[1].values,
        vec![
            Value::String("beta".to_string()),
            Value::Integer(20),
            Value::Null,
        ]
    );
}

#[test]
fn parses_schema_attribute_refs_case_insensitively_like_mdac() {
    let xml = r##"<?xml version="1.0"?>
<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"
     xmlns:dt="uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"
     xmlns:rs="urn:schemas-microsoft-com:rowset"
     xmlns:z="#RowsetSchema">
  <s:Schema id="RowsetSchema">
    <s:ElementType name="row" content="eltOnly">
      <s:attribute type="S2"/>
      <s:attribute type="S1"/>
      <s:extends type="rs:rowbase"/>
    </s:ElementType>
    <s:AttributeType name="s1" rs:name="Number After Text" dt:type="int"/>
    <s:AttributeType name="s2" rs:name="Text First"/>
    <s:AttributeType name="unused" dt:type="int"/>
  </s:Schema>
  <rs:data>
    <z:row s1="10" s2="alpha"/>
  </rs:data>
</xml>"##;

    let recordset = parse_ado_xml(xml).unwrap();

    assert_eq!(
        recordset
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Text First", "Number After Text"]
    );
    assert_eq!(
        recordset.rows[0].values,
        vec![Value::String("alpha".to_string()), Value::Integer(10)]
    );
}

#[test]
fn parses_nullable_and_maybenull_field_flags_like_mdac() {
    let xml = r##"<?xml version="1.0"?>
<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"
     xmlns:dt="uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"
     xmlns:rs="urn:schemas-microsoft-com:rowset"
     xmlns:z="#RowsetSchema">
  <s:Schema id="RowsetSchema">
    <s:ElementType name="row" content="eltOnly">
      <s:AttributeType name="INT_DEFAULT" dt:type="int"/>
      <s:AttributeType name="INT_MAYBE_FALSE" dt:type="int" rs:maybenull="false"/>
      <s:AttributeType name="INT_NULLABLE_TRUE" dt:type="int" rs:nullable="true"/>
      <s:AttributeType name="INT_NULLABLE_TRUE_MAYBE_FALSE" dt:type="int" rs:nullable="true" rs:maybenull="false"/>
      <s:AttributeType name="TEXT_DEFAULT"/>
      <s:AttributeType name="TEXT_MAYBE_FALSE" rs:maybenull="false"/>
      <s:extends type="rs:rowbase"/>
    </s:ElementType>
  </s:Schema>
  <rs:data>
    <z:row INT_DEFAULT="10" INT_MAYBE_FALSE="11" INT_NULLABLE_TRUE="12" INT_NULLABLE_TRUE_MAYBE_FALSE="13" TEXT_DEFAULT="alpha" TEXT_MAYBE_FALSE="beta"/>
  </rs:data>
</xml>"##;

    let recordset = parse_ado_xml(xml).unwrap();

    assert_eq!(
        recordset
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "INT_DEFAULT",
            "INT_MAYBE_FALSE",
            "INT_NULLABLE_TRUE",
            "INT_NULLABLE_TRUE_MAYBE_FALSE",
            "TEXT_DEFAULT",
            "TEXT_MAYBE_FALSE",
        ]
    );
    assert_eq!(
        recordset
            .fields
            .iter()
            .map(|field| field.nullable)
            .collect::<Vec<_>>(),
        vec![true, false, true, true, true, false]
    );
    assert_eq!(
        recordset
            .fields
            .iter()
            .map(|field| field.attributes.as_slice())
            .collect::<Vec<_>>(),
        vec![
            &[FieldAttribute::Fixed, FieldAttribute::MayBeNull][..],
            &[FieldAttribute::Fixed][..],
            &[
                FieldAttribute::Fixed,
                FieldAttribute::IsNullable,
                FieldAttribute::MayBeNull,
            ][..],
            &[FieldAttribute::Fixed, FieldAttribute::IsNullable][..],
            &[FieldAttribute::Long, FieldAttribute::MayBeNull][..],
            &[FieldAttribute::Long][..],
        ]
    );
    assert_eq!(
        recordset.rows[0].values,
        vec![
            Value::Integer(10),
            Value::Integer(11),
            Value::Integer(12),
            Value::Integer(13),
            Value::String("alpha".to_string()),
            Value::String("beta".to_string()),
        ]
    );
}

#[test]
fn parses_nullable_metadata_location_and_precedence_like_mdac() {
    let xml = r##"<?xml version="1.0"?>
<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"
     xmlns:dt="uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"
     xmlns:rs="urn:schemas-microsoft-com:rowset"
     xmlns:z="#RowsetSchema">
  <s:Schema id="RowsetSchema">
    <s:ElementType name="row" content="eltOnly">
      <s:AttributeType name="DEFAULT" dt:type="int"/>
      <s:AttributeType name="ATTR_NULLABLE_FALSE" dt:type="int" rs:nullable="false"/>
      <s:AttributeType name="ATTR_NULLABLE_FALSE_MAYBE_FALSE" dt:type="int" rs:nullable="false" rs:maybenull="false"/>
      <s:AttributeType name="DATATYPE_NULLABLE_TRUE_MAYBE_FALSE">
        <s:datatype dt:type="int" rs:nullable="true" rs:maybenull="false"/>
      </s:AttributeType>
      <s:AttributeType name="DATATYPE_NULLABLE_FALSE_MAYBE_TRUE">
        <s:datatype dt:type="int" rs:nullable="false" rs:maybenull="true"/>
      </s:AttributeType>
      <s:AttributeType name="ATTR_NULLABLE_FALSE_DATATYPE_MAYBE_FALSE" dt:type="int" rs:nullable="false">
        <s:datatype dt:type="int" rs:maybenull="false"/>
      </s:AttributeType>
      <s:AttributeType name="ATTR_MAYBE_FALSE_DATATYPE_NULLABLE_TRUE" dt:type="int" rs:maybenull="false">
        <s:datatype dt:type="int" rs:nullable="true"/>
      </s:AttributeType>
      <s:extends type="rs:rowbase"/>
    </s:ElementType>
  </s:Schema>
  <rs:data>
    <z:row DEFAULT="10" ATTR_NULLABLE_FALSE="11" ATTR_NULLABLE_FALSE_MAYBE_FALSE="12" DATATYPE_NULLABLE_TRUE_MAYBE_FALSE="13" DATATYPE_NULLABLE_FALSE_MAYBE_TRUE="14" ATTR_NULLABLE_FALSE_DATATYPE_MAYBE_FALSE="15" ATTR_MAYBE_FALSE_DATATYPE_NULLABLE_TRUE="16"/>
  </rs:data>
</xml>"##;

    let recordset = parse_ado_xml(xml).unwrap();

    assert_eq!(
        recordset
            .fields
            .iter()
            .map(|field| field.attributes.as_slice())
            .collect::<Vec<_>>(),
        vec![
            &[FieldAttribute::Fixed, FieldAttribute::MayBeNull][..],
            &[FieldAttribute::Fixed, FieldAttribute::MayBeNull][..],
            &[FieldAttribute::Fixed][..],
            &[FieldAttribute::Fixed, FieldAttribute::IsNullable][..],
            &[FieldAttribute::Fixed, FieldAttribute::MayBeNull][..],
            &[FieldAttribute::Fixed][..],
            &[FieldAttribute::Fixed, FieldAttribute::IsNullable][..],
        ]
    );
    assert_eq!(
        recordset
            .fields
            .iter()
            .map(|field| field.nullable)
            .collect::<Vec<_>>(),
        vec![true, true, false, true, true, false, true]
    );
}

#[test]
fn parses_xml_metadata_by_namespace_uri_not_prefix() {
    let xml = r##"<?xml version="1.0"?>
<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"
     xmlns:d="uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"
     xmlns:r="urn:schemas-microsoft-com:rowset"
     xmlns:z="#RowsetSchema">
  <s:Schema id="RowsetSchema">
    <s:ElementType name="row" content="eltOnly">
      <s:AttributeType name="c1" r:name="VISIBLE_INT" r:number="1">
        <s:datatype d:type="int" d:maxLength="4" r:maybenull="false"/>
      </s:AttributeType>
      <s:extends type="rs:rowbase"/>
    </s:ElementType>
  </s:Schema>
  <r:data>
    <z:row c1="42"/>
  </r:data>
</xml>"##;

    let recordset = parse_ado_xml(xml).unwrap();

    assert_eq!(recordset.fields.len(), 1);
    assert_eq!(recordset.fields[0].name, "VISIBLE_INT");
    assert_eq!(recordset.fields[0].xml_name, "c1");
    assert_eq!(recordset.fields[0].ordinal, Some(1));
    assert_eq!(recordset.fields[0].ado_type.unwrap().code, 3);
    assert!(!recordset.fields[0].nullable);
    assert_eq!(recordset.rows[0].values, vec![Value::Integer(42)]);
}

#[test]
fn ignores_wrong_namespace_xml_metadata_like_mdac() {
    let xml = r##"<?xml version="1.0"?>
<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"
     xmlns:dt="uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"
     xmlns:rs="urn:schemas-microsoft-com:rowset"
     xmlns:x="urn:wrong"
     xmlns:z="#RowsetSchema">
  <s:Schema id="RowsetSchema">
    <s:ElementType name="row" content="eltOnly">
      <s:AttributeType name="c1" x:name="VISIBLE" x:type="int" x:maxLength="1" x:dbtype="str" x:number="7" x:maybenull="false" x:fixedlength="true">
        <s:datatype x:type="int" x:maxLength="1" x:dbtype="str" x:precision="9" x:scale="2" x:maybenull="false" x:fixedlength="true"/>
      </s:AttributeType>
      <s:extends type="rs:rowbase"/>
    </s:ElementType>
  </s:Schema>
  <rs:data>
    <z:row c1="42"/>
  </rs:data>
</xml>"##;

    let recordset = parse_ado_xml(xml).unwrap();
    let field = &recordset.fields[0];

    assert_eq!(field.name, "c1");
    assert_eq!(field.xml_name, "c1");
    assert_eq!(field.ordinal, None);
    assert_eq!(field.ado_type.unwrap().code, 203);
    assert_eq!(field.data_type.as_deref(), Some("string"));
    assert_eq!(field.db_type, None);
    assert_eq!(field.max_length, None);
    assert_eq!(field.precision, None);
    assert_eq!(field.scale, None);
    assert!(field.nullable);
    assert!(!field.fixed_length);
    assert!(field.long);
    assert_eq!(
        field.attributes,
        vec![FieldAttribute::Long, FieldAttribute::MayBeNull]
    );
    assert_eq!(
        recordset.rows[0].values,
        vec![Value::String("42".to_string())]
    );
}

#[test]
fn ignores_wrong_namespace_xml_schema_elements_like_mdac() {
    for (label, schema_insert) in [
        (
            "wrong namespace AttributeType",
            r#"<x:AttributeType name="SHADOW" rs:name="BAD" rs:number="1">
          <s:datatype dt:type="int"/>
        </x:AttributeType>"#,
        ),
        (
            "wrong namespace ElementType",
            r#"<x:ElementType name="shadow">
          <x:AttributeType name="SHADOW" rs:name="BAD" rs:number="1"/>
        </x:ElementType>"#,
        ),
        (
            "unprefixed AttributeType inside wrong namespace ElementType",
            r#"<x:ElementType name="shadow">
          <AttributeType name="SHADOW" rs:name="BAD" rs:number="1">
            <datatype dt:type="int"/>
          </AttributeType>
        </x:ElementType>"#,
        ),
        (
            "unprefixed AttributeType inside wrong namespace extends",
            r#"<x:extends>
          <AttributeType name="SHADOW" rs:name="BAD" rs:number="1">
            <datatype dt:type="int"/>
          </AttributeType>
        </x:extends>"#,
        ),
    ] {
        let xml = format!(
            r##"<?xml version="1.0"?>
<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"
     xmlns:dt="uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"
     xmlns:rs="urn:schemas-microsoft-com:rowset"
     xmlns:x="urn:wrong"
     xmlns:z="#RowsetSchema">
  <s:Schema id="RowsetSchema">
    <s:ElementType name="row" content="eltOnly">
      {schema_insert}
      <s:AttributeType name="ID" rs:number="1">
        <s:datatype dt:type="int" dt:maxLength="4" rs:maybenull="false"/>
      </s:AttributeType>
      <s:AttributeType name="VALUE_FIELD" rs:number="2">
        <s:datatype dt:type="int" dt:maxLength="4" rs:maybenull="false"/>
      </s:AttributeType>
      <s:extends type="rs:rowbase"/>
    </s:ElementType>
  </s:Schema>
  <rs:data>
    <z:row ID="1" VALUE_FIELD="42"/>
  </rs:data>
</xml>"##
        );

        let recordset = parse_ado_xml(&xml).unwrap_or_else(|err| panic!("{label}: {err:#}"));
        assert_eq!(
            recordset
                .fields
                .iter()
                .map(|field| field.name.as_str())
                .collect::<Vec<_>>(),
            vec!["ID", "VALUE_FIELD"],
            "{label}"
        );
        assert_eq!(
            recordset.rows[0].values,
            vec![Value::Integer(1), Value::Integer(42)],
            "{label}"
        );
    }
}

#[test]
fn ignores_unknown_xml_schema_children_like_mdac() {
    for (label, schema_prefix, schema_insert, id_body) in [
        (
            "schema direct unknown",
            "<foo/>",
            "",
            r#"<s:datatype dt:type="int" dt:maxLength="4" rs:maybenull="false"/>"#,
        ),
        (
            "schema namespace unknown",
            "<s:foo/>",
            "",
            r#"<s:datatype dt:type="int" dt:maxLength="4" rs:maybenull="false"/>"#,
        ),
        (
            "wrong namespace unknown with field-like descendant",
            r#"<x:foo><AttributeType name="SHADOW"><datatype dt:type="int"/></AttributeType></x:foo>"#,
            "",
            r#"<s:datatype dt:type="int" dt:maxLength="4" rs:maybenull="false"/>"#,
        ),
        (
            "element child unknown with field-like descendant",
            "",
            r#"<s:foo><AttributeType name="SHADOW"><datatype dt:type="int"/></AttributeType></s:foo>"#,
            r#"<s:datatype dt:type="int" dt:maxLength="4" rs:maybenull="false"/>"#,
        ),
        (
            "attribute child unknown",
            "",
            "",
            r#"<foo/><s:datatype dt:type="int" dt:maxLength="4" rs:maybenull="false"/>"#,
        ),
        (
            "attribute child wrong namespace unknown",
            "",
            "",
            r#"<x:foo><datatype dt:type="string"/></x:foo><s:datatype dt:type="int" dt:maxLength="4" rs:maybenull="false"/>"#,
        ),
    ] {
        let xml = format!(
            r##"<?xml version="1.0"?>
<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"
     xmlns:dt="uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"
     xmlns:rs="urn:schemas-microsoft-com:rowset"
     xmlns:x="urn:wrong"
     xmlns:z="#RowsetSchema">
  <s:Schema id="RowsetSchema">
    {schema_prefix}
    <s:ElementType name="row" content="eltOnly">
      {schema_insert}
      <s:AttributeType name="ID" rs:number="1">{id_body}</s:AttributeType>
      <s:AttributeType name="VALUE_FIELD" rs:number="2">
        <s:datatype dt:type="int" dt:maxLength="4" rs:maybenull="false"/>
      </s:AttributeType>
      <s:extends type="rs:rowbase"/>
    </s:ElementType>
  </s:Schema>
  <rs:data>
    <z:row ID="1" VALUE_FIELD="42" SHADOW="99"/>
  </rs:data>
</xml>"##
        );

        let recordset = parse_ado_xml(&xml).unwrap_or_else(|err| panic!("{label}: {err:#}"));
        assert_eq!(
            recordset
                .fields
                .iter()
                .map(|field| field.name.as_str())
                .collect::<Vec<_>>(),
            vec!["ID", "VALUE_FIELD"],
            "{label}"
        );
        assert_eq!(
            recordset.rows[0].values,
            vec![Value::Integer(1), Value::Integer(42)],
            "{label}"
        );
    }
}

#[test]
fn rejects_wrong_namespace_xml_schema_elements_like_mdac() {
    let xml = r##"<?xml version="1.0"?>
<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"
     xmlns:dt="uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"
     xmlns:rs="urn:schemas-microsoft-com:rowset"
     xmlns:x="urn:wrong"
     xmlns:z="#RowsetSchema">
  <s:Schema id="RowsetSchema">
    <s:ElementType name="row" content="eltOnly">
      <x:Schema id="Other">
        <ElementType name="row">
          <AttributeType name="SHADOW">
            <datatype dt:type="int"/>
          </AttributeType>
        </ElementType>
      </x:Schema>
      <s:AttributeType name="ID" rs:number="1">
        <s:datatype dt:type="int" dt:maxLength="4" rs:maybenull="false"/>
      </s:AttributeType>
      <s:extends type="rs:rowbase"/>
    </s:ElementType>
  </s:Schema>
  <rs:data>
    <z:row ID="1" SHADOW="99"/>
  </rs:data>
</xml>"##;

    let err = parse_ado_xml(xml).expect_err("wrong-namespace Schema child should be rejected");
    let message = format!("{err:#}");
    assert!(
        message.contains("unexpected ADO XML ElementType child element Schema"),
        "{message}"
    );
}

#[test]
fn parses_unprefixed_schema_datatype_element_like_mdac() {
    let xml = r##"<?xml version="1.0"?>
<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"
     xmlns:dt="uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"
     xmlns:rs="urn:schemas-microsoft-com:rowset"
     xmlns:z="#RowsetSchema">
  <s:Schema id="RowsetSchema">
    <s:ElementType name="row" content="eltOnly">
      <s:AttributeType name="VALUE_FIELD">
        <datatype dt:type="int"/>
      </s:AttributeType>
      <s:extends type="rs:rowbase"/>
    </s:ElementType>
  </s:Schema>
  <rs:data>
    <z:row VALUE_FIELD="42"/>
  </rs:data>
</xml>"##;

    let recordset = parse_ado_xml(xml).unwrap();

    assert_eq!(recordset.fields[0].ado_type.unwrap().code, 3);
    assert_eq!(recordset.rows[0].values, vec![Value::Integer(42)]);
}

#[test]
fn ignores_wrong_namespace_datatype_element_like_mdac() {
    let xml = r##"<?xml version="1.0"?>
<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"
     xmlns:dt="uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"
     xmlns:rs="urn:schemas-microsoft-com:rowset"
     xmlns:x="urn:wrong"
     xmlns:z="#RowsetSchema">
  <s:Schema id="RowsetSchema">
    <s:ElementType name="row" content="eltOnly">
      <s:AttributeType name="VALUE_FIELD">
        <x:datatype dt:type="int"/>
      </s:AttributeType>
      <s:extends type="rs:rowbase"/>
    </s:ElementType>
  </s:Schema>
  <rs:data>
    <z:row VALUE_FIELD="42"/>
  </rs:data>
</xml>"##;

    let recordset = parse_ado_xml(xml).unwrap();
    let field = &recordset.fields[0];

    assert_eq!(field.ado_type.unwrap().code, 203);
    assert_eq!(field.data_type.as_deref(), Some("string"));
    assert_eq!(
        field.attributes,
        vec![FieldAttribute::Long, FieldAttribute::MayBeNull]
    );
    assert_eq!(
        recordset.rows[0].values,
        vec![Value::String("42".to_string())]
    );
}

#[test]
fn parses_xml_boolean_schema_metadata_tokens_like_mdac() {
    let xml = SAMPLE
        .replace(
            r#"<s:AttributeType name="PAYLOAD" rs:number="3">"#,
            r#"<s:AttributeType name="PAYLOAD" rs:number="3" rs:hidden="0" rs:write="1" rs:rowversion="-1">"#,
        )
        .replace(
            r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
            r#"<s:datatype dt:type="string" dt:maxLength="12" rs:fixedlength="-1" rs:maybenull="0"/>"#,
        )
        .replace(r#"PAYLOAD="DEADBEEF""#, r#"PAYLOAD="fixed text""#)
        .replace(r#"<z:row ID="2" s1=""/>"#, r#"<z:row ID="2" s1="" PAYLOAD="second"/>"#);

    let recordset = parse_ado_xml(&xml).unwrap();
    let field = &recordset.fields[2];
    assert_eq!(field.ado_type.map(|ty| ty.code), Some(130));
    assert_eq!(field.max_length, Some(12));
    assert!(!field.nullable);
    assert!(field.fixed_length);
    assert_eq!(
        field.attributes,
        vec![
            FieldAttribute::Fixed,
            FieldAttribute::RowVersion,
            FieldAttribute::Updatable,
        ]
    );
    assert_eq!(
        recordset.rows[0].values[2],
        Value::String("fixed text".to_string())
    );
}

#[test]
fn rejects_malformed_xml_boolean_schema_metadata() {
    for (label, xml, expected) in [
        (
            "hidden",
            SAMPLE.replace(
                r#"<s:AttributeType name="PAYLOAD" rs:number="3">"#,
                r#"<s:AttributeType name="PAYLOAD" rs:number="3" rs:hidden="maybe">"#,
            ),
            r#"invalid XML boolean attribute hidden value "maybe""#,
        ),
        (
            "maybenull",
            SAMPLE.replace(
                r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
                r#"<s:datatype dt:type="bin.hex" rs:maybenull="maybe"/>"#,
            ),
            r#"invalid XML boolean attribute maybenull value "maybe""#,
        ),
        (
            "fixedlength",
            SAMPLE.replace(
                r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
                r#"<s:datatype dt:type="bin.hex" rs:fixedlength="maybe" rs:maybenull="true"/>"#,
            ),
            r#"invalid XML boolean attribute fixedlength value "maybe""#,
        ),
        (
            "rowversion",
            SAMPLE.replace(
                r#"<s:AttributeType name="PAYLOAD" rs:number="3">"#,
                r#"<s:AttributeType name="PAYLOAD" rs:number="3" rs:rowversion="maybe">"#,
            ),
            r#"invalid XML boolean attribute rowversion value "maybe""#,
        ),
    ] {
        let err = parse_ado_xml(&xml).expect_err(&format!("{label} should be rejected"));
        assert!(format!("{err:#}").contains(expected), "{label}: {err:#}");
    }
}

#[test]
fn parses_utf16_ado_xml_bytes() {
    let utf16_le = utf16_bytes(SAMPLE, true);
    let utf16_be = utf16_bytes(SAMPLE, false);

    for bytes in [utf16_le, utf16_be] {
        let recordset = parse_ado_xml_bytes(&bytes).unwrap();
        assert_eq!(recordset.fields.len(), 3);
        assert_eq!(recordset.fields[1].name, "NAME");
        assert_eq!(recordset.rows[0].values[0], Value::Integer(1));
        assert_eq!(
            recordset.rows[0].values[1],
            Value::String("한글".to_string())
        );
    }
}

#[test]
fn normalizes_ado_xml_binary_c1_bytes_like_mdac() {
    let xml = SAMPLE.replace(
        "PAYLOAD=\"DEADBEEF\"",
        "PAYLOAD=\"001326394C5F728598ABBED1\"",
    );
    let recordset = parse_ado_xml(&xml).unwrap();

    assert_eq!(
        recordset.rows[0].values[2],
        Value::BinaryHex("001326394C5F7226DCABBED1".to_string())
    );
}

#[test]
fn truncates_bounded_xml_text_and_binary_values_like_mdac() {
    for (datatype, value, expected) in [
        (
            r#"<s:datatype dt:type="string" dt:maxLength="4" rs:maybenull="true"/>"#,
            "ABCDE",
            Value::String("ABCD".to_string()),
        ),
        (
            r#"<s:datatype dt:type="string" dt:maxLength="4" rs:fixedlength="true" rs:maybenull="true"/>"#,
            "AB",
            Value::String("AB".to_string()),
        ),
        (
            r#"<s:datatype dt:type="string" dt:maxLength="4" rs:long="true" rs:maybenull="true"/>"#,
            "ABCDE",
            Value::String("ABCDE".to_string()),
        ),
        (
            r#"<s:datatype dt:type="bin.base64" dt:maxLength="4" rs:maybenull="true"/>"#,
            "AAECAwQF",
            Value::String("AAEC".to_string()),
        ),
        (
            r#"<s:datatype dt:type="bin.base64" dt:maxLength="4" rs:long="true" rs:maybenull="true"/>"#,
            "AAECAwQF",
            Value::String("AAECAwQF".to_string()),
        ),
        (
            r#"<s:datatype dt:type="dateTime.tz" dt:maxLength="10" rs:maybenull="true"/>"#,
            "2026-06-12T01:02:03Z",
            Value::String("2026-06-12".to_string()),
        ),
        (
            r#"<s:datatype dt:type="dateTime.tz" dt:maxLength="10" rs:long="true" rs:maybenull="true"/>"#,
            "2026-06-12T01:02:03Z",
            Value::String("2026-06-12T01:02:03Z".to_string()),
        ),
        (
            r#"<s:datatype dt:type="fixed.14.4" dt:maxLength="4" rs:maybenull="true"/>"#,
            "123456",
            Value::String("1234".to_string()),
        ),
        (
            r#"<s:datatype dt:type="fixed.14.4" dt:maxLength="4" rs:long="true" rs:maybenull="true"/>"#,
            "123456",
            Value::String("123456".to_string()),
        ),
        (
            r#"<s:datatype dt:type="bin.hex" dt:maxLength="4" rs:maybenull="true"/>"#,
            "0102030405",
            Value::BinaryHex("01020304".to_string()),
        ),
        (
            r#"<s:datatype dt:type="bin.hex" dt:maxLength="4" rs:fixedlength="true" rs:maybenull="true"/>"#,
            "0102",
            Value::BinaryHex("0102".to_string()),
        ),
        (
            r#"<s:datatype dt:type="bin.hex" dt:maxLength="4" rs:long="true" rs:maybenull="true"/>"#,
            "0102030405",
            Value::BinaryHex("0102030405".to_string()),
        ),
    ] {
        let xml = sample_with_payload_datatype_and_value(datatype, value);
        let recordset = parse_ado_xml(&xml).unwrap();
        assert_eq!(recordset.rows[0].values[2], expected, "{datatype}");
    }
}

#[test]
fn rejects_non_finite_xml_float_values() {
    let xml = SAMPLE
        .replace(
            r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
            r#"<s:datatype dt:type="r8" rs:maybenull="true"/>"#,
        )
        .replace(r#"PAYLOAD="DEADBEEF""#, r#"PAYLOAD="NaN""#);

    let err = parse_ado_xml(&xml).expect_err("non-finite XML float should be rejected");
    assert!(
        format!("{err:#}").contains("non-finite XML float value \"NaN\""),
        "{err:#}"
    );
}

#[test]
fn parses_xml_r4_values_as_single_precision_like_mdac() {
    for (value, expected) in [
        ("3.4028231E+38", 3.402_823e38_f32 as f64),
        ("1.401298464E-45", f32::from_bits(1) as f64),
        ("-0", -0.0f32 as f64),
    ] {
        let xml = sample_with_payload_type_and_value("r4", value);
        let recordset = parse_ado_xml(&xml).unwrap();
        assert_eq!(
            recordset.rows[0].values[2],
            Value::Float(expected),
            "{value}"
        );
        if value == "-0" {
            let Value::Float(actual) = recordset.rows[0].values[2] else {
                panic!("expected r4 negative zero");
            };
            assert_eq!(actual.to_bits(), (-0.0f64).to_bits());
        }
    }
}

#[test]
fn parses_xml_float_lexical_values_like_mdac() {
    for (data_type, value, expected) in [
        ("r8", "  -1  ", -1.0),
        ("r8", "+1", 1.0),
        ("r8", ".5", 0.5),
        ("r8", "-.5", -0.5),
        ("r8", "1,000", 1000.0),
        ("r8", "&amp;H1", 1.0),
        ("r8", "&amp;O10", 8.0),
        ("r8", "1e-999", 0.0),
        ("float", "1E+39", 1.0e39),
        ("r4", "1,000", 1000.0f32 as f64),
        ("r4", "&amp;H1", 1.0f32 as f64),
        ("r4", ".5", 0.5f32 as f64),
    ] {
        let xml = sample_with_payload_datatype_and_value(
            &format!(r#"<s:datatype dt:type="{data_type}" rs:maybenull="true"/>"#),
            value,
        );
        let recordset = parse_ado_xml(&xml).unwrap();
        assert_eq!(
            recordset.rows[0].values[2],
            Value::Float(expected),
            "{data_type}: {value}"
        );
    }
}

#[test]
fn rejects_invalid_xml_float_lexical_values_like_mdac() {
    for (data_type, value, expected) in [
        ("r8", "", "invalid float value"),
        ("r8", "0x1", "invalid float value"),
        ("r8", "INF", "non-finite XML float value"),
        ("r8", "1E+309", "non-finite XML float value"),
        ("r4", "1E+39", "non-finite XML r4 value"),
    ] {
        let xml = sample_with_payload_datatype_and_value(
            &format!(r#"<s:datatype dt:type="{data_type}" rs:maybenull="true"/>"#),
            value,
        );
        let err = parse_ado_xml(&xml).expect_err(&format!(
            "invalid XML float {data_type} {value:?} should be rejected"
        ));
        assert!(
            format!("{err:#}").contains(expected),
            "{data_type} {value}: {err:#}"
        );
    }
}

#[test]
fn rejects_out_of_range_xml_r4_values_like_mdac() {
    let xml = sample_with_payload_type_and_value("r4", "1E+39");

    let err = parse_ado_xml(&xml).expect_err("out-of-range XML r4 should be rejected");
    assert!(
        format!("{err:#}").contains(r#"non-finite XML r4 value "1E+39""#),
        "{err:#}"
    );
}

#[test]
fn parses_xml_temporal_boundaries_like_mdac() {
    for (datatype, value, expected) in [
        (
            r#"<s:datatype dt:type="date" rs:maybenull="true"/>"#,
            "0100-01-01",
            Value::Date("0100-01-01".to_string()),
        ),
        (
            r#"<s:datatype dt:type="date" rs:maybenull="true"/>"#,
            "03:04:05",
            Value::Date("1899-12-30".to_string()),
        ),
        (
            r#"<s:datatype dt:type="time" rs:maybenull="true"/>"#,
            "23:59:59",
            Value::Time("23:59:59".to_string()),
        ),
        (
            r#"<s:datatype dt:type="time" rs:maybenull="true"/>"#,
            "2026-01-02",
            Value::Time("00:00:00".to_string()),
        ),
        (
            r#"<s:datatype dt:type="time" rs:maybenull="true"/>"#,
            "0000-02-29",
            Value::Time("00:00:00".to_string()),
        ),
        (
            r#"<s:datatype dt:type="dateTime" rs:dbtype="timestamp" dt:maxLength="16" rs:scale="9" rs:precision="29" rs:fixedlength="true" rs:maybenull="true"/>"#,
            "9999-12-31T23:59:59.123456789",
            Value::DateTime("9999-12-31T23:59:59.123456789".to_string()),
        ),
        (
            r#"<s:datatype dt:type="dateTime" rs:dbtype="filetime" dt:maxLength="16" rs:precision="0" rs:fixedlength="true" rs:maybenull="true"/>"#,
            "1601-01-01T00:00:00.987000000",
            Value::DateTime("1601-01-01T00:00:00".to_string()),
        ),
    ] {
        let xml = sample_with_payload_datatype_and_value(datatype, value);
        let recordset = parse_ado_xml(&xml).unwrap();
        assert_eq!(recordset.rows[0].values[2], expected, "{datatype}");
    }
}

#[test]
fn parses_xml_datetime_dbtype_aliases_like_mdac() {
    for (datatype, value, ado_type, expected) in [
        (
            r#"<s:datatype dt:type="dateTime" rs:dbtype="date" dt:maxLength="6" rs:maybenull="true"/>"#,
            "2026-01-02",
            133,
            Value::Date("2026-01-02".to_string()),
        ),
        (
            r#"<s:datatype dt:type="dateTime" rs:dbtype="date" dt:maxLength="6" rs:maybenull="true"/>"#,
            "03:04:05",
            133,
            Value::Date("1899-12-30".to_string()),
        ),
        (
            r#"<s:datatype dt:type="dateTime" rs:dbtype="time" dt:maxLength="6" rs:maybenull="true"/>"#,
            "03:04:05",
            134,
            Value::Time("03:04:05".to_string()),
        ),
        (
            r#"<s:datatype dt:type="dateTime" rs:dbtype="time" dt:maxLength="6" rs:maybenull="true"/>"#,
            "2026-01-02",
            134,
            Value::Time("00:00:00".to_string()),
        ),
        (
            r#"<s:datatype dt:type="dateTime" rs:dbtype="variantdate" dt:maxLength="8" rs:maybenull="true"/>"#,
            "2026-01-02",
            7,
            Value::DateTime("2026-01-02T00:00:00".to_string()),
        ),
        (
            r#"<s:datatype dt:type="dateTime" rs:dbtype="variantdate" dt:maxLength="8" rs:maybenull="true"/>"#,
            "03:04:05",
            7,
            Value::DateTime("1899-12-30T03:04:05".to_string()),
        ),
        (
            r#"<s:datatype dt:type="dateTime" rs:dbtype="variantdate" dt:maxLength="8" rs:maybenull="true"/>"#,
            "2026-01-02T03:04:05.123",
            7,
            Value::DateTime("2026-01-02T03:04:05.123".to_string()),
        ),
        (
            r#"<s:datatype dt:type="dateTime" rs:dbtype="dbdate" dt:maxLength="16" rs:maybenull="true"/>"#,
            "2026-01-02T03:04:05",
            135,
            Value::DateTime("2026-01-02T03:04:05".to_string()),
        ),
        (
            r#"<s:datatype dt:type="dateTime" rs:dbtype="dbtime" dt:maxLength="16" rs:maybenull="true"/>"#,
            "03:04:05",
            135,
            Value::DateTime("1899-12-30T03:04:05".to_string()),
        ),
        (
            r#"<s:datatype dt:type="dateTime" rs:dbtype="timestamp" dt:maxLength="16" rs:maybenull="true"/>"#,
            "2026-01-02",
            135,
            Value::DateTime("2026-01-02T00:00:00".to_string()),
        ),
    ] {
        let xml = sample_with_payload_datatype_and_value(datatype, value);
        let recordset = parse_ado_xml(&xml).unwrap();
        assert_eq!(
            recordset.fields[2].ado_type.map(|ty| ty.code),
            Some(ado_type),
            "{datatype}"
        );
        assert_eq!(recordset.rows[0].values[2], expected, "{datatype}");
    }
}

#[test]
fn rejects_invalid_xml_temporal_values_like_mdac() {
    for (datatype, value, expected) in [
        (
            r#"<s:datatype dt:type="date" rs:maybenull="true"/>"#,
            "2026-02-30",
            "invalid XML date day 30 for 2026-02",
        ),
        (
            r#"<s:datatype dt:type="date" rs:maybenull="true"/>"#,
            "0000-01-01",
            "invalid XML date year 0",
        ),
        (
            r#"<s:datatype dt:type="time" rs:maybenull="true"/>"#,
            "24:00:00",
            "invalid XML time 24:00:00",
        ),
        (
            r#"<s:datatype dt:type="time" rs:maybenull="true"/>"#,
            "12:00:00.1",
            r#"invalid XML time value "12:00:00.1""#,
        ),
        (
            r#"<s:datatype dt:type="time" rs:maybenull="true"/>"#,
            "2026-02-29",
            "invalid XML time day 29 for 2026-02",
        ),
        (
            r#"<s:datatype dt:type="time" rs:maybenull="true"/>"#,
            "abcd-ef-gh",
            r#"invalid XML time value "abcd-ef-gh""#,
        ),
        (
            r#"<s:datatype dt:type="dateTime" rs:dbtype="timestamp" dt:maxLength="16" rs:scale="9" rs:precision="29" rs:fixedlength="true" rs:maybenull="true"/>"#,
            "2026-01-02 03:04:05",
            "invalid XML datetime value",
        ),
        (
            r#"<s:datatype dt:type="dateTime" rs:dbtype="timestamp" dt:maxLength="16" rs:scale="9" rs:precision="29" rs:fixedlength="true" rs:maybenull="true"/>"#,
            "2026-01-02T03:04:05.1234567890",
            "invalid XML datetime fraction",
        ),
        (
            r#"<s:datatype dt:type="dateTime" rs:dbtype="timestamp" dt:maxLength="16" rs:scale="9" rs:precision="29" rs:fixedlength="true" rs:maybenull="true"/>"#,
            "2025-02-29T03:04:05",
            "invalid XML datetime day 29 for 2025-02",
        ),
        (
            r#"<s:datatype dt:type="dateTime" rs:dbtype="filetime" dt:maxLength="16" rs:precision="0" rs:fixedlength="true" rs:maybenull="true"/>"#,
            "1600-12-31T23:59:59",
            "XML filetime year 1600 is out of range",
        ),
        (
            r#"<s:datatype dt:type="dateTime" rs:dbtype="date" dt:maxLength="6" rs:maybenull="true"/>"#,
            "2026-01-02T03:04:05",
            "invalid XML date value",
        ),
        (
            r#"<s:datatype dt:type="dateTime" rs:dbtype="time" dt:maxLength="6" rs:maybenull="true"/>"#,
            "2026-01-02T03:04:05",
            "invalid XML time value",
        ),
    ] {
        let xml = sample_with_payload_datatype_and_value(datatype, value);
        let err = parse_ado_xml(&xml).expect_err(&format!(
            "invalid temporal XML value {value:?} should be rejected"
        ));
        assert!(
            format!("{err:#}").contains(expected),
            "{datatype} {value}: {err:#}"
        );
    }
}

#[test]
fn rejects_invalid_xml_integer_values_like_mdac() {
    let xml = SAMPLE.replace(r#"ID="1""#, r#"ID="not-int""#);

    let err = parse_ado_xml(&xml).expect_err("invalid XML integer should be rejected");
    assert!(
        format!("{err:#}").contains(r#"invalid integer value "not-int""#),
        "{err:#}"
    );
}

#[test]
fn parses_xml_open_ended_max_length_sentinel_like_mdac() {
    let xml = SAMPLE
        .replace(
            r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
            r#"<s:datatype dt:type="string" dt:maxLength="4294967295" rs:maybenull="true"/>"#,
        )
        .replace(r#"PAYLOAD="DEADBEEF""#, r#"PAYLOAD="long text""#);

    let recordset = parse_ado_xml(&xml).unwrap();
    let field = &recordset.fields[2];
    assert_eq!(field.ado_type.map(|ty| ty.code), Some(203));
    assert_eq!(field.max_length, None);
    assert!(field.long);
    assert_eq!(
        recordset.rows[0].values[2],
        Value::String("long text".to_string())
    );
}

#[test]
fn rejects_malformed_xml_schema_numeric_metadata() {
    for (label, xml, expected) in [
        (
            "maxLength",
            SAMPLE.replace(
                r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
                r#"<s:datatype dt:type="string" dt:maxLength="wide" rs:maybenull="true"/>"#,
            ),
            r#"invalid XML dt:maxLength value "wide""#,
        ),
        (
            "precision",
            SAMPLE.replace(
                r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
                r#"<s:datatype dt:type="number" rs:dbtype="numeric" dt:maxLength="19" rs:precision="wide" rs:scale="4" rs:maybenull="true"/>"#,
            ),
            r#"invalid XML rs:precision value "wide""#,
        ),
        (
            "scale",
            SAMPLE.replace(
                r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
                r#"<s:datatype dt:type="number" rs:dbtype="numeric" dt:maxLength="19" rs:precision="18" rs:scale="deep" rs:maybenull="true"/>"#,
            ),
            r#"invalid XML rs:scale value "deep""#,
        ),
        (
            "ordinal",
            SAMPLE.replace(r#"rs:number="2""#, r#"rs:number="two""#),
            r#"invalid XML rs:number value "two""#,
        ),
    ] {
        let err = parse_ado_xml(&xml).expect_err(&format!("{label} should be rejected"));
        assert!(format!("{err:#}").contains(expected), "{label}: {err:#}");
    }
}

#[test]
fn parses_xml_integer_type_boundaries_like_mdac() {
    for (data_type, value, expected) in [
        ("i1", "-128", Value::Integer(-128)),
        ("i2", "-32768", Value::Integer(-32768)),
        ("i4", "-2147483648", Value::Integer(-2147483648)),
        ("int", "2147483647", Value::Integer(2147483647)),
        ("integer", "2147483647", Value::Integer(2147483647)),
        ("i8", "-9223372036854775808", Value::Integer(i64::MIN)),
        ("ui1", "255", Value::UnsignedInteger(255)),
        ("ui2", "65535", Value::UnsignedInteger(65535)),
        ("ui4", "4294967295", Value::UnsignedInteger(4294967295)),
        (
            "ui8",
            "18446744073709551615",
            Value::UnsignedInteger(u64::MAX),
        ),
    ] {
        let xml = sample_with_id_type_and_value(data_type, value);
        let recordset = parse_ado_xml(&xml).unwrap();
        assert_eq!(recordset.rows[0].values[0], expected, "{data_type}");
    }
}

#[test]
fn parses_xml_integer_lexical_values_like_mdac() {
    for (data_type, value, expected) in [
        ("i4", "  -1  ", Value::Integer(-1)),
        ("i4", "+1", Value::Integer(1)),
        ("i4", "1.5", Value::Integer(2)),
        ("i4", "2.5", Value::Integer(2)),
        ("i4", "1E1", Value::Integer(10)),
        ("i4", "1,000", Value::Integer(1000)),
        ("i4", "&amp;HFF", Value::Integer(255)),
        ("i4", "&amp;O10", Value::Integer(8)),
        ("i1", "0.5", Value::Integer(0)),
        ("i1", "1,0", Value::Integer(10)),
        ("ui1", "&amp;HFF", Value::UnsignedInteger(255)),
        ("ui4", "-.5", Value::UnsignedInteger(0)),
        ("i8", "", Value::Integer(0)),
        ("i8", "  ", Value::Integer(0)),
        ("i8", "0.5", Value::Integer(1)),
        ("i8", "2.5", Value::Integer(3)),
        ("i8", "-2.5", Value::Integer(-3)),
        ("ui8", "0.5", Value::UnsignedInteger(1)),
        (
            "ui8",
            "18446744073709551615",
            Value::UnsignedInteger(u64::MAX),
        ),
    ] {
        let xml = sample_with_id_type_and_value(data_type, value);
        let recordset = parse_ado_xml(&xml).unwrap();
        assert_eq!(
            recordset.rows[0].values[0], expected,
            "{data_type}: {value}"
        );
    }
}

#[test]
fn rejects_out_of_range_xml_integer_values_like_mdac() {
    for (data_type, value, expected) in [
        ("i1", "128", r#"XML i1 integer value "128" is out of range"#),
        (
            "i2",
            "-32769",
            r#"XML i2 integer value "-32769" is out of range"#,
        ),
        (
            "i4",
            "2147483648",
            r#"XML i4 integer value "2147483648" is out of range"#,
        ),
        (
            "int",
            "-2147483649",
            r#"XML int integer value "-2147483649" is out of range"#,
        ),
        (
            "integer",
            "2147483648",
            r#"XML integer integer value "2147483648" is out of range"#,
        ),
        (
            "ui1",
            "256",
            r#"XML ui1 unsigned integer value "256" is out of range"#,
        ),
        (
            "ui2",
            "65536",
            r#"XML ui2 unsigned integer value "65536" is out of range"#,
        ),
        (
            "ui4",
            "4294967296",
            r#"XML ui4 unsigned integer value "4294967296" is out of range"#,
        ),
    ] {
        let xml = sample_with_id_type_and_value(data_type, value);
        let err = parse_ado_xml(&xml).expect_err(&format!(
            "{data_type} out-of-range XML integer should be rejected"
        ));
        assert!(
            format!("{err:#}").contains(expected),
            "{data_type}: {err:#}"
        );
    }
}

#[test]
fn parses_xml_boolean_lexical_values_like_mdac() {
    for (value, expected) in [
        ("true", true),
        ("True", true),
        ("false", false),
        ("False", false),
        ("1", true),
        ("0", false),
        ("-1", true),
        ("2", true),
        ("-2", true),
        ("+1", true),
        ("+0", false),
        (" 1 ", true),
        (" -0 ", false),
        ("1.5", true),
        ("0E0", false),
        ("1E0", true),
        ("1e-999", false),
        ("4e-324", true),
        ("1,0", true),
        ("0,0", false),
        ("&amp;H1", true),
        ("&amp;H0", false),
        ("&amp;O10", true),
    ] {
        let xml = sample_with_payload_datatype_and_value(
            r#"<s:datatype dt:type="boolean" rs:maybenull="true"/>"#,
            value,
        );
        let recordset = parse_ado_xml(&xml).unwrap();
        assert_eq!(
            recordset.rows[0].values[2],
            Value::Boolean(expected),
            "{value}"
        );
    }
}

#[test]
fn rejects_invalid_xml_integer_lexical_values_like_mdac() {
    for (data_type, value, expected) in [
        ("i4", "", "invalid integer value"),
        ("i4", "abc", "invalid integer value"),
        ("i4", "0x1", "invalid integer value"),
        ("i1", "&amp;HFF", "XML i1 integer value"),
        ("ui1", "-0.6", "XML ui1 unsigned integer value"),
        ("i8", "1E1", "invalid integer value"),
        ("i8", "&amp;H1", "invalid integer value"),
        ("i8", "1,0", "invalid integer value"),
        ("ui8", "-0.4", "XML ui8 unsigned integer value"),
    ] {
        let xml = sample_with_id_type_and_value(data_type, value);
        let err = parse_ado_xml(&xml).expect_err(&format!(
            "invalid XML integer {data_type} {value:?} should be rejected"
        ));
        assert!(
            format!("{err:#}").contains(expected),
            "{data_type} {value}: {err:#}"
        );
    }
}

#[test]
fn rejects_invalid_xml_boolean_values_like_mdac() {
    for value in [
        "maybe", " true ", "yes", "NaN", "Infinity", "1e999", ",1", "0x1", "-&amp;H1",
    ] {
        let xml = sample_with_payload_datatype_and_value(
            r#"<s:datatype dt:type="boolean" rs:maybenull="true"/>"#,
            value,
        );

        let err = parse_ado_xml(&xml)
            .expect_err(&format!("invalid XML boolean {value:?} should be rejected"));
        assert!(
            format!("{err:#}").contains("invalid boolean value"),
            "{value}: {err:#}"
        );
    }
}

#[test]
fn parses_xml_bin_hex_separators_like_mdac() {
    for (value, expected) in [
        ("deadbeef", "DEADBEEF"),
        ("DeAdBeEf", "DEADBEEF"),
        ("", ""),
        (" ", ""),
        (" DEADBEEF ", "DEADBEEF"),
        ("DE AD BE EF", "DEADBEEF"),
        ("DE&#x9;AD", "DEAD"),
        ("DE&#xA;AD", "DEAD"),
        ("DE-AD", "DEAD"),
        ("DE_AD", "DEAD"),
        ("DE,AD", "DEAD"),
        ("DE.AD", "DEAD"),
        ("--", ""),
    ] {
        let xml = sample_with_payload_datatype_and_value(
            r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
            value,
        );
        let recordset = parse_ado_xml(&xml).unwrap();
        assert_eq!(
            recordset.rows[0].values[2],
            Value::BinaryHex(expected.to_string()),
            "{value}"
        );
    }
}

#[test]
fn rejects_invalid_xml_bin_hex_values_like_mdac() {
    for value in ["DEADZEEF", "DEA", "D", "D-E", "D E", "F_", "0xDE"] {
        let xml = sample_with_payload_datatype_and_value(
            r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
            value,
        );

        let err = parse_ado_xml(&xml)
            .expect_err(&format!("invalid XML bin.hex {value:?} should be rejected"));
        assert!(
            format!("{err:#}").contains("invalid bin.hex value"),
            "{value}: {err:#}"
        );
    }
}

#[test]
fn parses_xml_binary_type_as_unicode_text_like_mdac() {
    for (datatype, value, ado_type, max_length, fixed, long, expected) in [
        (
            r#"<s:datatype dt:type="binary" rs:maybenull="true"/>"#,
            "DE AD",
            Some(203),
            None,
            false,
            true,
            "DE AD",
        ),
        (
            r#"<s:datatype dt:type="binary" dt:maxLength="4" rs:maybenull="true"/>"#,
            "DEADBEEFCAFE",
            Some(202),
            Some(4),
            false,
            false,
            "DEAD",
        ),
        (
            r#"<s:datatype dt:type="binary" dt:maxLength="4" rs:fixedlength="true" rs:maybenull="true"/>"#,
            "DEADBEEF",
            Some(130),
            Some(4),
            true,
            false,
            "DEAD",
        ),
        (
            r#"<s:datatype dt:type="binary" dt:maxLength="4" rs:long="true" rs:maybenull="true"/>"#,
            "DEADBEEFCAFE",
            Some(203),
            Some(4),
            false,
            true,
            "DEADBEEFCAFE",
        ),
        (
            r#"<s:datatype dt:type="binary" dt:maxLength="4" rs:maybenull="true"/>"#,
            "D-E",
            Some(202),
            Some(4),
            false,
            false,
            "D-E",
        ),
    ] {
        let xml = sample_with_payload_datatype_and_value(datatype, value);
        let recordset = parse_ado_xml(&xml).unwrap();
        let field = &recordset.fields[2];
        assert_eq!(field.ado_type.map(|ty| ty.code), ado_type, "{datatype}");
        assert_eq!(field.max_length, max_length, "{datatype}");
        assert_eq!(field.fixed_length, fixed, "{datatype}");
        assert_eq!(field.long, long, "{datatype}");
        assert_eq!(
            recordset.rows[0].values[2],
            Value::String(expected.to_string()),
            "{datatype}"
        );
    }
}

#[test]
fn parses_xml_bstr_dbtype_as_unicode_text_like_mdac() {
    for (datatype, ado_type, max_length, fixed, long, attributes, expected) in [
        (
            r#"<s:datatype dt:type="string" rs:dbtype="bstr" rs:maybenull="true"/>"#,
            Some(203),
            None,
            false,
            true,
            vec![FieldAttribute::Long, FieldAttribute::MayBeNull],
            "ABCDE",
        ),
        (
            r#"<s:datatype dt:type="string" rs:dbtype="bstr" dt:maxLength="4" rs:maybenull="true"/>"#,
            Some(202),
            Some(4),
            false,
            false,
            vec![FieldAttribute::MayBeNull],
            "ABCD",
        ),
        (
            r#"<s:datatype dt:type="string" rs:dbtype="bstr" dt:maxLength="4" rs:fixedlength="true" rs:maybenull="true"/>"#,
            Some(130),
            Some(4),
            true,
            false,
            vec![FieldAttribute::Fixed, FieldAttribute::MayBeNull],
            "ABCD",
        ),
        (
            r#"<s:datatype dt:type="string" rs:dbtype="bstr" dt:maxLength="4" rs:long="true" rs:maybenull="true"/>"#,
            Some(203),
            Some(4),
            false,
            true,
            vec![FieldAttribute::Long, FieldAttribute::MayBeNull],
            "ABCDE",
        ),
        (
            r#"<s:datatype dt:type="string" rs:dbtype="bstr" dt:maxLength="4294967295" rs:maybenull="true"/>"#,
            Some(203),
            None,
            false,
            true,
            vec![FieldAttribute::Long, FieldAttribute::MayBeNull],
            "ABCDE",
        ),
    ] {
        let xml = sample_with_payload_datatype_and_value(datatype, "ABCDE");

        let recordset = parse_ado_xml(&xml).unwrap();
        let field = &recordset.fields[2];
        assert_eq!(field.ado_type.map(|ty| ty.code), ado_type, "{datatype}");
        assert_eq!(field.max_length, max_length, "{datatype}");
        assert_eq!(field.fixed_length, fixed, "{datatype}");
        assert_eq!(field.long, long, "{datatype}");
        assert_eq!(field.attributes, attributes, "{datatype}");
        assert_eq!(
            recordset.rows[0].values[2],
            Value::String(expected.to_string()),
            "{datatype}"
        );
    }
}

#[test]
fn parses_xml_decimal_values_after_validation() {
    let xml = SAMPLE
        .replace(
            r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
            r#"<s:datatype dt:type="decimal" rs:maybenull="true"/>"#,
        )
        .replace(r#"PAYLOAD="DEADBEEF""#, r#"PAYLOAD="-12.3400""#);

    let recordset = parse_ado_xml(&xml).unwrap();
    assert_eq!(
        recordset.rows[0].values[2],
        Value::Decimal("-12.34".to_string())
    );
}

#[test]
fn parses_xml_decimal_lexical_values_like_mdac() {
    for (datatype, value, expected) in [
        (
            r#"<s:datatype dt:type="decimal" rs:maybenull="true"/>"#,
            "  -1.25  ",
            "-1.25",
        ),
        (
            r#"<s:datatype dt:type="decimal" rs:maybenull="true"/>"#,
            "1,000.25",
            "1000.25",
        ),
        (
            r#"<s:datatype dt:type="decimal" rs:maybenull="true"/>"#,
            "1E1",
            "10",
        ),
        (
            r#"<s:datatype dt:type="decimal" rs:maybenull="true"/>"#,
            "1e-3",
            "0.001",
        ),
        (
            r#"<s:datatype dt:type="decimal" rs:maybenull="true"/>"#,
            "&amp;H1",
            "1",
        ),
        (
            r#"<s:datatype dt:type="decimal" rs:maybenull="true"/>"#,
            "&amp;O10",
            "8",
        ),
        (
            r#"<s:datatype dt:type="number" rs:dbtype="decimal" rs:maybenull="true"/>"#,
            "1,0",
            "10",
        ),
        (
            r#"<s:datatype dt:type="number" rs:dbtype="numeric" dt:maxLength="19" rs:precision="18" rs:scale="4" rs:maybenull="true"/>"#,
            "1E1",
            "10",
        ),
        (
            r#"<s:datatype dt:type="number" rs:dbtype="numeric" dt:maxLength="19" rs:precision="18" rs:scale="4" rs:maybenull="true"/>"#,
            "1e-3",
            "0.001",
        ),
    ] {
        let xml = sample_with_payload_datatype_and_value(datatype, value);
        let recordset = parse_ado_xml(&xml).unwrap();
        assert_eq!(
            recordset.rows[0].values[2],
            Value::Decimal(expected.to_string()),
            "{datatype}: {value}"
        );
    }
}

#[test]
fn parses_xml_number_dbtype_values_case_insensitively() {
    for (datatype, value, expected_type, expected_value) in [
        (
            r#"<s:datatype dt:type="number" rs:dbtype="Numeric" dt:maxLength="19" rs:precision="18" rs:scale="4" rs:maybenull="true"/>"#,
            "1E1",
            131,
            "10",
        ),
        (
            r#"<s:datatype dt:type="number" rs:dbtype="Currency" rs:maybenull="true"/>"#,
            "123.45678",
            6,
            "123.4568",
        ),
        (
            r#"<s:datatype dt:type="number" rs:dbtype="Decimal" rs:maybenull="true"/>"#,
            "1,000.25",
            14,
            "1000.25",
        ),
    ] {
        let xml = sample_with_payload_datatype_and_value(datatype, value);
        let recordset = parse_ado_xml(&xml).unwrap();
        assert_eq!(
            recordset.fields[2].ado_type.map(|ty| ty.code),
            Some(expected_type),
            "{datatype}"
        );
        assert_eq!(
            recordset.rows[0].values[2],
            Value::Decimal(expected_value.to_string()),
            "{datatype}: {value}"
        );
    }
}

#[test]
fn rejects_malformed_xml_decimal_values() {
    for (datatype, value, expected) in [
        (
            r#"<s:datatype dt:type="decimal" rs:maybenull="true"/>"#,
            "12x",
            "invalid XML decimal value",
        ),
        (
            r#"<s:datatype dt:type="decimal" rs:maybenull="true"/>"#,
            "0x1",
            "invalid XML decimal value",
        ),
        (
            r#"<s:datatype dt:type="decimal" rs:maybenull="true"/>"#,
            "",
            "invalid XML decimal value",
        ),
        (
            r#"<s:datatype dt:type="number" rs:dbtype="numeric" dt:maxLength="19" rs:precision="18" rs:scale="4" rs:maybenull="true"/>"#,
            "1,000",
            "invalid XML numeric value",
        ),
        (
            r#"<s:datatype dt:type="number" rs:dbtype="numeric" dt:maxLength="19" rs:precision="18" rs:scale="4" rs:maybenull="true"/>"#,
            "&amp;H1",
            "invalid XML numeric value",
        ),
    ] {
        let xml = sample_with_payload_datatype_and_value(datatype, value);

        let err = parse_ado_xml(&xml).expect_err(&format!(
            "malformed XML decimal-family value {value:?} should be rejected"
        ));
        assert!(
            format!("{err:#}").contains(expected),
            "{datatype} {value}: {err:#}"
        );
    }
}

#[test]
fn rejects_xml_number_without_dbtype_and_bounded_width_like_mdac() {
    let xml = SAMPLE
        .replace(
            r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
            r#"<s:datatype dt:type="number" rs:maybenull="true"/>"#,
        )
        .replace(r#"PAYLOAD="DEADBEEF""#, r#"PAYLOAD="123""#);

    let err = parse_ado_xml(&xml).expect_err("unbounded XML varnumeric should be rejected");
    assert!(
        format!("{err:#}").contains("MDAC XML varnumeric value has no bounded dt:maxLength"),
        "{err:#}"
    );
}

#[test]
fn rejects_xml_number_without_dbtype_when_payload_exceeds_width_like_mdac() {
    for (max_length, value) in [("1", "1"), ("2", "1"), ("4", "65536")] {
        let xml = SAMPLE
            .replace(
                r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
                &format!(
                    r#"<s:datatype dt:type="number" dt:maxLength="{max_length}" rs:maybenull="true"/>"#
                ),
            )
            .replace(r#"PAYLOAD="DEADBEEF""#, &format!(r#"PAYLOAD="{value}""#));

        let err = parse_ado_xml(&xml).expect_err(&format!(
            "XML varnumeric maxLength {max_length} should fail"
        ));
        assert!(
            format!("{err:#}").contains("MDAC XML varnumeric payload length"),
            "maxLength {max_length}: {err:#}"
        );
    }
}

#[test]
fn parses_xml_varnumeric_lexical_values_like_mdac() {
    for (value, expected) in [
        ("  -1.25  ", "-1.25"),
        (".5", "0.5"),
        ("-.5", "-0.5"),
        ("001.2300", "1.23"),
        ("1E1", "10"),
        ("1e-3", "0.001"),
        ("123456.789", "123456.789"),
    ] {
        let xml = sample_with_payload_datatype_and_value(
            r#"<s:datatype dt:type="number" dt:maxLength="19" rs:maybenull="true"/>"#,
            value,
        );

        let recordset = parse_ado_xml(&xml).unwrap();
        assert_eq!(
            recordset.rows[0].values[2],
            Value::Decimal(expected.to_string()),
            "{value}"
        );
    }
}

#[test]
fn rejects_invalid_xml_varnumeric_lexical_values_like_mdac() {
    for value in ["1,000", "&amp;H1", "&amp;O10", "", "abc", "Infinity"] {
        let xml = sample_with_payload_datatype_and_value(
            r#"<s:datatype dt:type="number" dt:maxLength="19" rs:maybenull="true"/>"#,
            value,
        );

        let err = parse_ado_xml(&xml).expect_err(&format!(
            "invalid XML varnumeric value {value:?} should be rejected"
        ));
        assert!(
            format!("{err:#}").contains("invalid XML varnumeric value"),
            "{value}: {err:#}"
        );
    }
}

#[test]
fn rejects_malformed_xml_number_decimal_values() {
    let xml = SAMPLE
        .replace(
            r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
            r#"<s:datatype dt:type="number" rs:dbtype="currency" rs:maybenull="true"/>"#,
        )
        .replace(r#"PAYLOAD="DEADBEEF""#, r#"PAYLOAD="--1""#);

    let err = parse_ado_xml(&xml).expect_err("malformed XML currency should be rejected");
    assert!(
        format!("{err:#}").contains("invalid XML currency value \"--1\""),
        "{err:#}"
    );
}

#[test]
fn parses_xml_currency_rounding_and_range_like_mdac() {
    for (datatype, value, expected) in [
        (
            r#"<s:datatype dt:type="number" rs:dbtype="currency" rs:maybenull="true"/>"#,
            "123.4567",
            "123.4567",
        ),
        (
            r#"<s:datatype dt:type="number" rs:dbtype="currency" rs:maybenull="true"/>"#,
            "1,000.25",
            "1000.25",
        ),
        (
            r#"<s:datatype dt:type="number" rs:dbtype="currency" rs:maybenull="true"/>"#,
            "1E1",
            "10",
        ),
        (
            r#"<s:datatype dt:type="number" rs:dbtype="currency" rs:maybenull="true"/>"#,
            "1e-3",
            "0.001",
        ),
        (
            r#"<s:datatype dt:type="number" rs:dbtype="currency" rs:maybenull="true"/>"#,
            "&amp;H1",
            "1",
        ),
        (
            r#"<s:datatype dt:type="currency" rs:maybenull="true"/>"#,
            "&amp;O10",
            "8",
        ),
        (
            r#"<s:datatype dt:type="number" rs:dbtype="currency" rs:maybenull="true"/>"#,
            "123.45678",
            "123.4568",
        ),
        (
            r#"<s:datatype dt:type="number" rs:dbtype="currency" rs:maybenull="true"/>"#,
            "123.45685",
            "123.4568",
        ),
        (
            r#"<s:datatype dt:type="number" rs:dbtype="currency" rs:maybenull="true"/>"#,
            "0.00005",
            "0",
        ),
        (
            r#"<s:datatype dt:type="number" rs:dbtype="currency" rs:maybenull="true"/>"#,
            "0.00015",
            "0.0002",
        ),
        (
            r#"<s:datatype dt:type="number" rs:dbtype="currency" rs:maybenull="true"/>"#,
            "-123.45675",
            "-123.4568",
        ),
        (
            r#"<s:datatype dt:type="currency" rs:maybenull="true"/>"#,
            "123.45678",
            "123.4568",
        ),
        (
            r#"<s:datatype dt:type="number" rs:dbtype="currency" rs:maybenull="true"/>"#,
            "922337203685477.5807",
            "922337203685477.5807",
        ),
        (
            r#"<s:datatype dt:type="number" rs:dbtype="currency" rs:maybenull="true"/>"#,
            "-922337203685477.5808",
            "-922337203685477.5808",
        ),
    ] {
        let xml = sample_with_payload_datatype_and_value(datatype, value);
        let recordset = parse_ado_xml(&xml).unwrap();
        assert_eq!(
            recordset.rows[0].values[2],
            Value::Decimal(expected.to_string()),
            "{value}"
        );
    }
}

#[test]
fn rejects_xml_currency_values_outside_mdac_range() {
    for value in ["922337203685477.5808", "-922337203685477.5809"] {
        let xml = sample_with_payload_datatype_and_value(
            r#"<s:datatype dt:type="number" rs:dbtype="currency" rs:maybenull="true"/>"#,
            value,
        );

        let err = parse_ado_xml(&xml).expect_err("out-of-range XML currency should be rejected");
        assert!(
            format!("{err:#}")
                .contains(&format!(r#"XML currency value "{value}" is out of range"#)),
            "{value}: {err:#}"
        );
    }
}

#[test]
fn parses_xml_numeric_precision_scale_like_mdac() {
    for (value, precision, scale, expected) in [
        ("123.4", "5", "2", "123.4"),
        ("123.459", "6", "2", "123.45"),
        ("000123.45", "5", "2", "123.45"),
        ("0.0019", "3", "3", "0.001"),
        ("-123.45", "5", "2", "-123.45"),
    ] {
        let xml = sample_with_payload_datatype_and_value(
            &format!(
                r#"<s:datatype dt:type="number" rs:dbtype="numeric" dt:maxLength="19" rs:precision="{precision}" rs:scale="{scale}" rs:maybenull="true"/>"#
            ),
            value,
        );

        let recordset = parse_ado_xml(&xml).unwrap();
        assert_eq!(
            recordset.rows[0].values[2],
            Value::Decimal(expected.to_string()),
            "{value}"
        );
    }
}

#[test]
fn rejects_xml_numeric_values_outside_declared_precision_like_mdac() {
    for (label, value, precision, scale, expected) in [
        (
            "zero precision",
            "1",
            "0",
            "0",
            "invalid XML numeric descriptor precision 0 for field PAYLOAD",
        ),
        (
            "scale exceeds precision",
            "1",
            "2",
            "3",
            "invalid XML numeric descriptor scale 3 exceeds precision 2 for field PAYLOAD",
        ),
        (
            "integer precision overflow",
            "123456",
            "5",
            "0",
            r#"XML numeric value "123456" exceeds precision 5 for field PAYLOAD"#,
        ),
        (
            "fractional zero precision overflow",
            "100.00",
            "3",
            "2",
            r#"XML numeric value "100.00" exceeds precision 3 for field PAYLOAD"#,
        ),
    ] {
        let xml = sample_with_payload_datatype_and_value(
            &format!(
                r#"<s:datatype dt:type="number" rs:dbtype="numeric" dt:maxLength="19" rs:precision="{precision}" rs:scale="{scale}" rs:maybenull="true"/>"#
            ),
            value,
        );

        let err = parse_ado_xml(&xml).expect_err(label);
        assert!(format!("{err:#}").contains(expected), "{label}: {err:#}");
    }
}

#[test]
fn parses_xml_bin_base64_type_as_unicode_text_like_mdac() {
    for (datatype, value, ado_type, max_length, fixed, long, attributes) in [
        (
            r#"<s:datatype dt:type="bin.base64" rs:maybenull="true"/>"#,
            "AAECAwQF+v8=",
            Some(203),
            None,
            false,
            true,
            &[FieldAttribute::Long, FieldAttribute::MayBeNull][..],
        ),
        (
            r#"<s:datatype dt:type="bin.base64" dt:maxLength="12" rs:maybenull="true"/>"#,
            "YWJj",
            Some(202),
            Some(12),
            false,
            false,
            &[FieldAttribute::MayBeNull][..],
        ),
        (
            r#"<s:datatype dt:type="bin.base64" dt:maxLength="12" rs:fixedlength="true" rs:maybenull="true"/>"#,
            "MTIzNA==",
            Some(130),
            Some(12),
            true,
            false,
            &[FieldAttribute::Fixed, FieldAttribute::MayBeNull][..],
        ),
    ] {
        let xml = SAMPLE
            .replace(
                r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
                datatype,
            )
            .replace(r#"PAYLOAD="DEADBEEF""#, &format!(r#"PAYLOAD="{value}""#));

        let recordset = parse_ado_xml(&xml).unwrap();
        let field = &recordset.fields[2];
        assert_eq!(field.ado_type.map(|ty| ty.code), ado_type, "{datatype}");
        assert_eq!(field.max_length, max_length, "{datatype}");
        assert_eq!(field.fixed_length, fixed, "{datatype}");
        assert_eq!(field.long, long, "{datatype}");
        assert_eq!(field.attributes.as_slice(), attributes, "{datatype}");
        assert_eq!(
            recordset.rows[0].values[2],
            Value::String(value.to_string()),
            "{datatype}"
        );
        assert_eq!(recordset.rows[1].values[2], Value::Null, "{datatype}");
    }
}

#[test]
fn parses_xml_datetime_tz_type_as_unicode_text_like_mdac() {
    for (datatype, value, ado_type, max_length, fixed, long, attributes) in [
        (
            r#"<s:datatype dt:type="dateTime.tz" rs:maybenull="true"/>"#,
            "2026-06-12T01:02:03Z",
            Some(203),
            None,
            false,
            true,
            &[FieldAttribute::Long, FieldAttribute::MayBeNull][..],
        ),
        (
            r#"<s:datatype dt:type="dateTime.tz" dt:maxLength="32" rs:maybenull="true"/>"#,
            "2026-06-12T01:02:03+09:30",
            Some(202),
            Some(32),
            false,
            false,
            &[FieldAttribute::MayBeNull][..],
        ),
        (
            r#"<s:datatype dt:type="dateTime.tz" dt:maxLength="32" rs:fixedlength="true" rs:maybenull="true"/>"#,
            "2026-06-12T01:02:03-04:00",
            Some(130),
            Some(32),
            true,
            false,
            &[FieldAttribute::Fixed, FieldAttribute::MayBeNull][..],
        ),
    ] {
        let xml = SAMPLE
            .replace(
                r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
                datatype,
            )
            .replace(r#"PAYLOAD="DEADBEEF""#, &format!(r#"PAYLOAD="{value}""#));

        let recordset = parse_ado_xml(&xml).unwrap();
        let field = &recordset.fields[2];
        assert_eq!(field.ado_type.map(|ty| ty.code), ado_type, "{datatype}");
        assert_eq!(field.max_length, max_length, "{datatype}");
        assert_eq!(field.fixed_length, fixed, "{datatype}");
        assert_eq!(field.long, long, "{datatype}");
        assert_eq!(field.attributes.as_slice(), attributes, "{datatype}");
        assert_eq!(
            recordset.rows[0].values[2],
            Value::String(value.to_string()),
            "{datatype}"
        );
        assert_eq!(recordset.rows[1].values[2], Value::Null, "{datatype}");
    }
}

#[test]
fn parses_xml_text_alias_width_rules_like_mdac() {
    for type_name in [
        "char",
        "empty",
        "entity",
        "entities",
        "enumeration",
        "error",
        "fixed.14.4",
        "id",
        "idref",
        "idrefs",
        "nmtoken",
        "nmtokens",
        "notation",
        "time.tz",
        "uri",
    ] {
        for (datatype, ado_type, max_length, fixed, long, attributes, expected) in [
            (
                format!(
                    r#"<s:datatype dt:type="{type_name}" dt:maxLength="4" rs:maybenull="true"/>"#
                ),
                Some(202),
                Some(4),
                false,
                false,
                vec![FieldAttribute::MayBeNull],
                "ABCD",
            ),
            (
                format!(
                    r#"<s:datatype dt:type="{type_name}" dt:maxLength="4" rs:fixedlength="true" rs:maybenull="true"/>"#
                ),
                Some(130),
                Some(4),
                true,
                false,
                vec![FieldAttribute::Fixed, FieldAttribute::MayBeNull],
                "ABCD",
            ),
            (
                format!(
                    r#"<s:datatype dt:type="{type_name}" dt:maxLength="4" rs:long="true" rs:maybenull="true"/>"#
                ),
                Some(203),
                Some(4),
                false,
                true,
                vec![FieldAttribute::Long, FieldAttribute::MayBeNull],
                "ABCDE",
            ),
        ] {
            let xml = sample_with_payload_datatype_and_value(&datatype, "ABCDE");

            let recordset = parse_ado_xml(&xml).unwrap();
            let field = &recordset.fields[2];
            assert_eq!(
                field.ado_type.map(|ty| ty.code),
                ado_type,
                "{type_name}: {datatype}"
            );
            assert_eq!(field.max_length, max_length, "{type_name}: {datatype}");
            assert_eq!(field.fixed_length, fixed, "{type_name}: {datatype}");
            assert_eq!(field.long, long, "{type_name}: {datatype}");
            assert_eq!(field.attributes, attributes, "{type_name}: {datatype}");
            assert_eq!(
                recordset.rows[0].values[2],
                Value::String(expected.to_string()),
                "{type_name}: {datatype}"
            );
        }
    }
}

#[test]
fn parses_xml_fixed_14_4_type_as_long_unicode_text_like_mdac() {
    let xml = SAMPLE
        .replace(
            r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
            r#"<s:datatype dt:type="fixed.14.4" rs:maybenull="true"/>"#,
        )
        .replace(r#"PAYLOAD="DEADBEEF""#, r#"PAYLOAD="1000.1234""#);

    let recordset = parse_ado_xml(&xml).unwrap();
    let field = &recordset.fields[2];
    assert_eq!(field.ado_type.map(|ty| ty.code), Some(203));
    assert_eq!(field.max_length, None);
    assert!(field.long);
    assert!(!field.fixed_length);
    assert_eq!(
        field.attributes,
        vec![FieldAttribute::Long, FieldAttribute::MayBeNull]
    );
    assert_eq!(
        recordset.rows[0].values[2],
        Value::String("1000.1234".to_string())
    );
    assert_eq!(recordset.rows[1].values[2], Value::Null);
}

#[test]
fn parses_xml_float_and_r8_width4_as_zero_like_mdac() {
    for type_name in ["float", "r8"] {
        let xml = SAMPLE
            .replace(
                r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
                &format!(
                    r#"<s:datatype dt:type="{type_name}" dt:maxLength="4" rs:maybenull="true"/>"#
                ),
            )
            .replace(r#"PAYLOAD="DEADBEEF""#, r#"PAYLOAD="123.5""#);

        let recordset = parse_ado_xml(&xml).unwrap();
        let field = &recordset.fields[2];
        assert_eq!(field.ado_type.map(|ty| ty.code), Some(5), "{type_name}");
        assert_eq!(field.max_length, Some(4), "{type_name}");
        assert!(field.fixed_length, "{type_name}");
        assert!(!field.long, "{type_name}");
        assert_eq!(
            field.attributes,
            vec![FieldAttribute::Fixed, FieldAttribute::MayBeNull],
            "{type_name}"
        );
        assert_eq!(
            recordset.rows[0].values[2],
            Value::Float(0.0),
            "{type_name}"
        );
        assert_eq!(recordset.rows[1].values[2], Value::Null, "{type_name}");
    }
}

#[test]
fn parses_xml_empty_type_as_long_unicode_text_like_mdac() {
    let xml = SAMPLE
        .replace(
            r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
            r#"<s:datatype dt:type="empty" rs:maybenull="true"/>"#,
        )
        .replace(r#"PAYLOAD="DEADBEEF""#, r#"PAYLOAD="anything""#);

    let recordset = parse_ado_xml(&xml).unwrap();
    let field = &recordset.fields[2];
    assert_eq!(field.ado_type.map(|ty| ty.code), Some(203));
    assert_eq!(field.max_length, None);
    assert!(field.long);
    assert!(!field.fixed_length);
    assert_eq!(
        field.attributes,
        vec![FieldAttribute::Long, FieldAttribute::MayBeNull]
    );
    assert_eq!(
        recordset.rows[0].values[2],
        Value::String("anything".to_string())
    );
    assert_eq!(recordset.rows[1].values[2], Value::Null);
}

#[test]
fn parses_xml_error_type_as_long_unicode_text_like_mdac() {
    let xml = SAMPLE
        .replace(
            r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
            r#"<s:datatype dt:type="error" rs:maybenull="true"/>"#,
        )
        .replace(r#"PAYLOAD="DEADBEEF""#, r#"PAYLOAD="5""#);

    let recordset = parse_ado_xml(&xml).unwrap();
    let field = &recordset.fields[2];
    assert_eq!(field.ado_type.map(|ty| ty.code), Some(203));
    assert_eq!(field.max_length, None);
    assert!(field.long);
    assert!(!field.fixed_length);
    assert_eq!(
        field.attributes,
        vec![FieldAttribute::Long, FieldAttribute::MayBeNull]
    );
    assert_eq!(recordset.rows[0].values[2], Value::String("5".to_string()));
    assert_eq!(recordset.rows[1].values[2], Value::Null);
}

#[test]
fn parses_xml_variant_type_as_fixed_variant_text_like_mdac() {
    let xml = SAMPLE
        .replace(
            r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
            r#"<s:datatype dt:type="variant" rs:maybenull="true"/>"#,
        )
        .replace(r#"PAYLOAD="DEADBEEF""#, r#"PAYLOAD="plain text""#);

    let recordset = parse_ado_xml(&xml).unwrap();
    let field = &recordset.fields[2];
    assert_eq!(field.ado_type.map(|ty| ty.code), Some(12));
    assert_eq!(field.max_length, Some(16));
    assert!(!field.long);
    assert!(field.fixed_length);
    assert_eq!(
        field.attributes,
        vec![FieldAttribute::Fixed, FieldAttribute::MayBeNull]
    );
    assert_eq!(
        recordset.rows[0].values[2],
        Value::String("plain text".to_string())
    );
    assert_eq!(recordset.rows[1].values[2], Value::Null);
}

#[test]
fn parses_xml_variant_metadata_widths_like_mdac() {
    for (datatype, max_length, long, attributes) in [
        (
            r#"<s:datatype dt:type="variant" dt:maxLength="11" rs:maybenull="true"/>"#,
            Some(11),
            false,
            vec![FieldAttribute::Fixed, FieldAttribute::MayBeNull],
        ),
        (
            r#"<s:datatype dt:type="variant" dt:maxLength="32" rs:maybenull="true"/>"#,
            Some(32),
            false,
            vec![FieldAttribute::Fixed, FieldAttribute::MayBeNull],
        ),
        (
            r#"<s:datatype dt:type="variant" dt:maxLength="4294967295" rs:maybenull="true"/>"#,
            None,
            false,
            vec![FieldAttribute::Fixed, FieldAttribute::MayBeNull],
        ),
        (
            r#"<s:datatype dt:type="variant" rs:long="true" rs:maybenull="true"/>"#,
            Some(16),
            true,
            vec![
                FieldAttribute::Fixed,
                FieldAttribute::Long,
                FieldAttribute::MayBeNull,
            ],
        ),
    ] {
        let xml = sample_with_payload_datatype_and_value(datatype, "ABCDEFGHIJKL");

        let recordset = parse_ado_xml(&xml).unwrap();
        let field = &recordset.fields[2];
        assert_eq!(field.ado_type.map(|ty| ty.code), Some(12), "{datatype}");
        assert_eq!(field.max_length, max_length, "{datatype}");
        assert!(field.fixed_length, "{datatype}");
        assert_eq!(field.long, long, "{datatype}");
        assert_eq!(field.attributes, attributes, "{datatype}");
        assert_eq!(
            recordset.rows[0].values[2],
            Value::String("ABCDEFGHIJKL".to_string()),
            "{datatype}"
        );
    }
}

#[test]
fn rejects_small_xml_variant_max_length_like_mdac() {
    for max_length in ["0", "1", "8", "10"] {
        let datatype = format!(
            r#"<s:datatype dt:type="variant" dt:maxLength="{max_length}" rs:maybenull="true"/>"#
        );
        let xml = sample_with_payload_datatype_and_value(&datatype, "ABCDE");

        let err = parse_ado_xml(&xml).expect_err(&format!(
            "variant maxLength {max_length} should be rejected"
        ));
        assert!(
            format!("{err:#}").contains(&format!(
                "invalid XML variant dt:maxLength {max_length} for field PAYLOAD"
            )),
            "{max_length}: {err:#}"
        );
    }
}

#[test]
fn parses_braced_xml_uuid_values() {
    let xml = SAMPLE
        .replace(
            r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
            r#"<s:datatype dt:type="uuid" rs:maybenull="true"/>"#,
        )
        .replace(
            r#"PAYLOAD="DEADBEEF""#,
            r#"PAYLOAD="{00112233-4455-6677-8899-AABBCCDDEEFF}""#,
        );

    let recordset = parse_ado_xml(&xml).unwrap();
    assert_eq!(
        recordset.rows[0].values[2],
        Value::Guid("{00112233-4455-6677-8899-AABBCCDDEEFF}".to_string())
    );
}

#[test]
fn parses_lowercase_xml_uuid_values_as_canonical_guid() {
    let xml = SAMPLE
        .replace(
            r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
            r#"<s:datatype dt:type="uuid" rs:maybenull="true"/>"#,
        )
        .replace(
            r#"PAYLOAD="DEADBEEF""#,
            r#"PAYLOAD="{00112233-4455-6677-8899-aabbccddeeff}""#,
        );

    let recordset = parse_ado_xml(&xml).unwrap();
    assert_eq!(
        recordset.rows[0].values[2],
        Value::Guid("{00112233-4455-6677-8899-AABBCCDDEEFF}".to_string())
    );
}

#[test]
fn rejects_unbraced_xml_uuid_values_like_mdac() {
    let xml = SAMPLE
        .replace(
            r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
            r#"<s:datatype dt:type="uuid" rs:maybenull="true"/>"#,
        )
        .replace(
            r#"PAYLOAD="DEADBEEF""#,
            r#"PAYLOAD="00112233-4455-6677-8899-AABBCCDDEEFF""#,
        );

    let err = parse_ado_xml(&xml).expect_err("unbraced XML uuid should be rejected");
    assert!(
        format!("{err:#}").contains("invalid XML uuid value"),
        "{err:#}"
    );
}

#[test]
fn rejects_malformed_xml_uuid_values_like_mdac() {
    let xml = SAMPLE
        .replace(
            r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
            r#"<s:datatype dt:type="uuid" rs:maybenull="true"/>"#,
        )
        .replace(
            r#"PAYLOAD="DEADBEEF""#,
            r#"PAYLOAD="{00112233-4455-6677-8899-AABBCCDDEEFX}""#,
        );

    let err = parse_ado_xml(&xml).expect_err("malformed XML uuid should be rejected");
    assert!(
        format!("{err:#}").contains("invalid XML uuid value"),
        "{err:#}"
    );
}

#[test]
fn rejects_missing_required_current_row_fields_like_mdac() {
    let xml = SAMPLE.replace(
        r#"<z:row ID="1" s1="&#xD55C;&#xAE00;" PAYLOAD="DEADBEEF"/>"#,
        r#"<z:row s1="&#xD55C;&#xAE00;" PAYLOAD="DEADBEEF"/>"#,
    );

    let err = parse_ado_xml(&xml).expect_err("missing required current field should be rejected");
    assert!(
        format!("{err:#}").contains("missing required XML field ID"),
        "{err:#}"
    );
}

#[test]
fn parses_row_values_with_case_insensitive_xml_field_names_like_mdac() {
    let xml = SAMPLE
        .replace(
            r#"<s:AttributeType name="PAYLOAD" rs:number="3">"#,
            r#"<s:AttributeType name="VALUE_FIELD" rs:number="3">"#,
        )
        .replace(
            r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
            r#"<s:datatype dt:type="string" dt:maxLength="255" rs:maybenull="true"/>"#,
        )
        .replace(r#" PAYLOAD="DEADBEEF""#, r#" value_field="case-folded""#);

    let recordset = parse_ado_xml(&xml).unwrap();

    assert_eq!(
        recordset.rows[0].values[2],
        Value::String("case-folded".to_string())
    );
}

#[test]
fn duplicate_case_insensitive_row_attributes_use_last_value_like_mdac() {
    let xml = SAMPLE
        .replace(
            r#"<s:AttributeType name="PAYLOAD" rs:number="3">"#,
            r#"<s:AttributeType name="VALUE_FIELD" rs:number="3">"#,
        )
        .replace(
            r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
            r#"<s:datatype dt:type="string" dt:maxLength="255" rs:maybenull="true"/>"#,
        )
        .replace(
            r#" PAYLOAD="DEADBEEF""#,
            r#" VALUE_FIELD="upper" value_field="lower""#,
        );

    let recordset = parse_ado_xml(&xml).unwrap();

    assert_eq!(
        recordset.rows[0].values[2],
        Value::String("lower".to_string())
    );
}

#[test]
fn ignores_xml_namespace_declarations_in_raw_row_attributes() {
    let xml = SAMPLE.replace(
        r#"<z:row ID="1" s1="&#xD55C;&#xAE00;" PAYLOAD="DEADBEEF"/>"#,
        r##"<z:row xmlns="urn:temp" ID="1" s1="&#xD55C;&#xAE00;" PAYLOAD="DEADBEEF"/>"##,
    );

    let recordset = parse_ado_xml(&xml).unwrap();
    assert_eq!(recordset.rows[0].values[0], Value::Integer(1));
}

#[test]
fn parses_rs_forcenull_without_shadowing_forcenull_field_data() {
    let xml = SAMPLE
        .replace(
            r#"<s:AttributeType name="s1" rs:name="NAME" rs:number="2">"#,
            r#"<s:AttributeType name="forcenull" rs:number="2">"#,
        )
        .replace(
            r#"<s:AttributeType name="PAYLOAD" rs:number="3">"#,
            r#"<s:AttributeType name="VALUE_FIELD" rs:number="3">"#,
        )
        .replace(
            r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
            r#"<s:datatype dt:type="string" dt:maxLength="255" rs:maybenull="true"/>"#,
        )
        .replace(
            r#"<z:row ID="1" s1="&#xD55C;&#xAE00;" PAYLOAD="DEADBEEF"/>"#,
            r#"<z:row ID="1" forcenull="field-value" rs:forcenull="VALUE_FIELD"/>"#,
        )
        .replace(
            r#"<z:row ID="2" s1=""/>"#,
            r#"<z:row ID="2" forcenull="" VALUE_FIELD="value-2"/>"#,
        );

    let recordset = parse_ado_xml(&xml).unwrap();

    assert_eq!(
        recordset
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["ID", "forcenull", "VALUE_FIELD"]
    );
    assert!(recordset.fields[2].nullable);
    assert_eq!(
        recordset.rows[0].values,
        vec![
            Value::Integer(1),
            Value::String("field-value".to_string()),
            Value::Null,
        ]
    );
    assert_eq!(
        recordset.rows[1].values,
        vec![
            Value::Integer(2),
            Value::String(String::new()),
            Value::String("value-2".to_string()),
        ]
    );
}

#[test]
fn rejects_rs_forcenull_for_non_nullable_field_like_mdac() {
    let xml = SAMPLE
        .replace(
            r#"<s:AttributeType name="PAYLOAD" rs:number="3">"#,
            r#"<s:AttributeType name="VALUE_FIELD" rs:number="3">"#,
        )
        .replace(
            r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
            r#"<s:datatype dt:type="string" dt:maxLength="255" rs:maybenull="false"/>"#,
        )
        .replace(
            r#"<z:row ID="1" s1="&#xD55C;&#xAE00;" PAYLOAD="DEADBEEF"/>"#,
            r#"<z:row ID="1" s1="&#xD55C;&#xAE00;" rs:forcenull="VALUE_FIELD"/>"#,
        )
        .replace(
            r#"<z:row ID="2" s1=""/>"#,
            r#"<z:row ID="2" s1="" VALUE_FIELD="value-2"/>"#,
        );

    let err = parse_ado_xml(&xml).expect_err("non-nullable force-null should be rejected");
    let message = format!("{err:#}");
    assert!(
        message.contains("force-null XML field VALUE_FIELD in Current row was not nullable"),
        "{message}"
    );
}

#[test]
fn rs_forcenull_does_not_override_present_field_values_like_mdac() {
    let xml = SAMPLE
        .replace(
            r#"<s:AttributeType name="PAYLOAD" rs:number="3">"#,
            r#"<s:AttributeType name="VALUE_FIELD" rs:number="3">"#,
        )
        .replace(
            r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
            r#"<s:datatype dt:type="string" dt:maxLength="255" rs:maybenull="true"/>"#,
        )
        .replace(
            r#"<z:row ID="1" s1="&#xD55C;&#xAE00;" PAYLOAD="DEADBEEF"/>"#,
            r#"<z:row ID="1" s1="&#xD55C;&#xAE00;" VALUE_FIELD="field-value" rs:forcenull="VALUE_FIELD"/>"#,
        )
        .replace(
            r#"<z:row ID="2" s1=""/>"#,
            r#"<z:row ID="2" s1="" VALUE_FIELD="value-2"/>"#,
        );

    let recordset = parse_ado_xml(&xml).unwrap();

    assert_eq!(
        recordset.rows[0].values[2],
        Value::String("field-value".to_string())
    );
}

#[test]
fn parses_rs_forcenull_as_whitespace_token_list_like_mdac() {
    for (force_null, expected_a, expected_b) in [
        ("A B", Value::Null, Value::Null),
        (
            "A,B",
            Value::String("old-a".to_string()),
            Value::String("old-b".to_string()),
        ),
        (
            "A;B",
            Value::String("old-a".to_string()),
            Value::String("old-b".to_string()),
        ),
        ("A, B", Value::String("old-a".to_string()), Value::Null),
    ] {
        let xml = format!(
            r##"<?xml version="1.0"?>
<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"
     xmlns:dt="uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"
     xmlns:rs="urn:schemas-microsoft-com:rowset"
     xmlns:z="#RowsetSchema">
  <s:Schema id="RowsetSchema">
    <s:ElementType name="row" content="eltOnly" rs:updatable="true">
      <s:AttributeType name="ID" rs:number="1" rs:write="true">
        <s:datatype dt:type="int" dt:maxLength="4" rs:maybenull="false"/>
      </s:AttributeType>
      <s:AttributeType name="A" rs:number="2" rs:write="true">
        <s:datatype dt:type="string" dt:maxLength="255" rs:maybenull="true"/>
      </s:AttributeType>
      <s:AttributeType name="B" rs:number="3" rs:write="true">
        <s:datatype dt:type="string" dt:maxLength="255" rs:maybenull="true"/>
      </s:AttributeType>
      <s:extends type="rs:rowbase"/>
    </s:ElementType>
  </s:Schema>
  <rs:data>
    <rs:update>
      <rs:original><z:row ID="1" A="old-a" B="old-b"/></rs:original>
      <z:row ID="1" rs:forcenull="{force_null}"/>
    </rs:update>
  </rs:data>
</xml>"##
        );

        let recordset = parse_ado_xml(&xml).unwrap();
        let pending = materialize_pending_view(&recordset);

        assert_eq!(pending.rows.len(), 1, "{force_null}");
        assert_eq!(pending.rows[0].status, RecordStatusFlag::Modified);
        assert_eq!(pending.rows[0].values[1], expected_a, "{force_null} A");
        assert_eq!(pending.rows[0].values[2], expected_b, "{force_null} B");
    }
}

#[test]
fn parses_rs_forcenull_against_xml_name_not_visible_rs_name_like_mdac() {
    for (force_null, expected_value) in [
        ("c1", Value::Null),
        ("C1", Value::Null),
        ("VISIBLE_FIELD", Value::String("old-value".to_string())),
        ("visible_field", Value::String("old-value".to_string())),
    ] {
        let xml = format!(
            r##"<?xml version="1.0"?>
<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"
     xmlns:dt="uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"
     xmlns:rs="urn:schemas-microsoft-com:rowset"
     xmlns:z="#RowsetSchema">
  <s:Schema id="RowsetSchema">
    <s:ElementType name="row" content="eltOnly" rs:updatable="true">
      <s:AttributeType name="ID" rs:number="1" rs:write="true">
        <s:datatype dt:type="int" dt:maxLength="4" rs:maybenull="false"/>
      </s:AttributeType>
      <s:AttributeType name="c1" rs:name="VISIBLE_FIELD" rs:number="2" rs:write="true">
        <s:datatype dt:type="string" dt:maxLength="255" rs:maybenull="true"/>
      </s:AttributeType>
      <s:extends type="rs:rowbase"/>
    </s:ElementType>
  </s:Schema>
  <rs:data>
    <rs:update>
      <rs:original><z:row ID="1" c1="old-value"/></rs:original>
      <z:row ID="1" rs:forcenull="{force_null}"/>
    </rs:update>
  </rs:data>
</xml>"##
        );

        let recordset = parse_ado_xml(&xml).unwrap();
        let pending = materialize_pending_view(&recordset);

        assert_eq!(pending.rows.len(), 1, "{force_null}");
        assert_eq!(pending.rows[0].status, RecordStatusFlag::Modified);
        assert_eq!(pending.rows[0].values[1], expected_value, "{force_null}");
    }
}

#[test]
fn parses_rs_forcenull_case_insensitively_like_mdac() {
    let xml = SAMPLE
        .replace(
            r#"<s:AttributeType name="PAYLOAD" rs:number="3">"#,
            r#"<s:AttributeType name="VALUE_FIELD" rs:number="3">"#,
        )
        .replace(
            r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
            r#"<s:datatype dt:type="string" dt:maxLength="255" rs:maybenull="true"/>"#,
        )
        .replace(
            r#"<z:row ID="1" s1="&#xD55C;&#xAE00;" PAYLOAD="DEADBEEF"/>"#,
            r#"<z:row ID="1" s1="&#xD55C;&#xAE00;" rs:forcenull="value_field"/>"#,
        );

    let recordset = parse_ado_xml(&xml).unwrap();

    assert_eq!(recordset.rows[0].values[2], Value::Null);
}

#[test]
fn marks_pending_change_rows() {
    let xml = SAMPLE.replace(
        "<z:row ID=\"2\" s1=\"\"/>",
        "<rs:update><rs:original><z:row ID=\"2\" s1=\"old\"/></rs:original><z:row s1=\"new\"/></rs:update>",
    );
    let recordset = parse_ado_xml(&xml).unwrap();

    assert_eq!(recordset.rows[1].state, RowState::Original);
    assert_eq!(recordset.rows[2].state, RowState::Updated);
    assert_eq!(recordset.rows[2].values[0], Value::Unavailable);
    assert_eq!(
        recordset.rows[2].values[1],
        Value::String("new".to_string())
    );
}

#[test]
fn parses_malformed_xml_update_wrappers_as_current_rows_like_mdac() {
    for (label, replacement, expected_text) in [
        (
            "missing updated row",
            "<rs:update><rs:original><z:row ID=\"2\" s1=\"old\"/></rs:original></rs:update>",
            "old",
        ),
        (
            "missing original row",
            "<rs:update><z:row ID=\"2\" s1=\"new\"/></rs:update>",
            "new",
        ),
        (
            "multiple updated rows",
            "<rs:update><rs:original><z:row ID=\"2\" s1=\"old\"/></rs:original><z:row s1=\"new\"/><z:row s1=\"newer\"/></rs:update>",
            "newer",
        ),
    ] {
        let xml = SAMPLE.replace("<z:row ID=\"2\" s1=\"\"/>", replacement);
        let recordset = parse_ado_xml(&xml).unwrap_or_else(|err| panic!("{label}: {err:#}"));

        assert_eq!(recordset.rows.len(), 2, "{label}");
        assert_eq!(recordset.rows[1].state, RowState::Current, "{label}");
        assert_eq!(recordset.rows[1].values[0], Value::Integer(2), "{label}");
        assert_eq!(
            recordset.rows[1].values[1],
            Value::String(expected_text.to_string()),
            "{label}"
        );
        assert!(materialize_pending_view(&recordset).rows.is_empty(), "{label}");
    }
}

#[test]
fn parses_wrong_namespace_change_wrappers_as_current_rows_like_mdac() {
    for (label, replacement, expected_texts) in [
        (
            "wrong namespace insert",
            "<x:insert><z:row ID=\"2\" s1=\"wrapped\"/></x:insert>",
            vec!["wrapped"],
        ),
        (
            "unprefixed delete",
            "<delete><z:row ID=\"2\" s1=\"wrapped\"/></delete>",
            vec!["wrapped"],
        ),
        (
            "wrong namespace update",
            "<x:update><x:original><z:row ID=\"2\" s1=\"old\"/></x:original><z:row ID=\"3\" s1=\"new\"/></x:update>",
            vec!["old", "new"],
        ),
    ] {
        let xml = SAMPLE
            .replace(
                r#"<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882""#,
                r#"<xml xmlns:x="urn:wrong" xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882""#,
            )
            .replace("<z:row ID=\"2\" s1=\"\"/>", replacement);

        let recordset = parse_ado_xml(&xml).unwrap_or_else(|err| panic!("{label}: {err:#}"));
        assert_eq!(recordset.rows.len(), expected_texts.len() + 1, "{label}");
        assert_eq!(recordset.rows[0].values[0], Value::Integer(1), "{label}");
        for (index, expected_text) in expected_texts.iter().enumerate() {
            let row = &recordset.rows[index + 1];
            assert_eq!(row.state, RowState::Current, "{label}");
            assert_eq!(
                row.values[1],
                Value::String((*expected_text).to_string()),
                "{label}"
            );
        }
        assert!(materialize_pending_view(&recordset).rows.is_empty(), "{label}");
    }
}

#[test]
fn parses_empty_xml_change_wrappers_as_noops_like_mdac() {
    for (label, replacement) in [
        ("empty insert", "<rs:insert></rs:insert>"),
        ("empty delete", "<rs:delete></rs:delete>"),
        ("empty update", "<rs:update></rs:update>"),
    ] {
        let xml = SAMPLE.replace("<z:row ID=\"2\" s1=\"\"/>", replacement);
        let recordset = parse_ado_xml(&xml).unwrap_or_else(|err| panic!("{label}: {err:#}"));
        assert_eq!(recordset.rows.len(), 1, "{label}");
        assert_eq!(recordset.rows[0].values[0], Value::Integer(1), "{label}");
        assert_eq!(recordset.changes.len(), 1, "{label}");
    }
}

#[test]
fn parses_unknown_children_inside_insert_and_delete_wrappers_like_mdac() {
    for (label, replacement, expected_state, expected_status, expected_text) in [
        (
            "insert nested row",
            "<rs:insert><extra><z:row ID=\"2\" s1=\"inserted\"/></extra></rs:insert>",
            RowState::Inserted,
            RecordStatusFlag::New,
            "inserted",
        ),
        (
            "delete nested row",
            "<rs:delete><extra><z:row ID=\"2\" s1=\"deleted\"/></extra></rs:delete>",
            RowState::Deleted,
            RecordStatusFlag::Deleted,
            "deleted",
        ),
    ] {
        let xml = SAMPLE.replace("<z:row ID=\"2\" s1=\"\"/>", replacement);
        let recordset = parse_ado_xml(&xml).unwrap_or_else(|err| panic!("{label}: {err:#}"));

        assert_eq!(recordset.rows.len(), 2, "{label}");
        assert_eq!(recordset.rows[1].state, expected_state, "{label}");
        assert_eq!(
            recordset.rows[1].values[1],
            Value::String(expected_text.to_string()),
            "{label}"
        );
        let pending = materialize_pending_view(&recordset);
        assert_eq!(pending.rows.len(), 1, "{label}");
        assert_eq!(pending.rows[0].status, expected_status, "{label}");
    }

    for (label, replacement) in [
        ("insert empty child", "<rs:insert><extra/></rs:insert>"),
        (
            "insert wrong namespace nested row",
            "<rs:insert><extra><x:row ID=\"2\" s1=\"ignored\"/></extra></rs:insert>",
        ),
        ("delete empty child", "<rs:delete><extra/></rs:delete>"),
        (
            "delete wrong namespace nested row",
            "<rs:delete><extra><x:row ID=\"2\" s1=\"ignored\"/></extra></rs:delete>",
        ),
    ] {
        let xml = SAMPLE
            .replace(
                r#"<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882""#,
                r#"<xml xmlns:x="urn:wrong" xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882""#,
            )
            .replace("<z:row ID=\"2\" s1=\"\"/>", replacement);
        let recordset = parse_ado_xml(&xml).unwrap_or_else(|err| panic!("{label}: {err:#}"));

        assert_eq!(recordset.rows.len(), 1, "{label}");
        assert_eq!(recordset.rows[0].values[0], Value::Integer(1), "{label}");
        assert!(
            materialize_pending_view(&recordset).rows.is_empty(),
            "{label}"
        );
    }
}

#[test]
fn parses_unprefixed_rows_inside_wrappers_without_root_row_prefix_like_mdac() {
    for (label, replacement, expected_state, expected_status, expected_text) in [
        (
            "insert unprefixed no default",
            "<rs:insert><row ID=\"2\" s1=\"inserted\"/></rs:insert>",
            RowState::Inserted,
            RecordStatusFlag::New,
            "inserted",
        ),
        (
            "insert unprefixed wrong default",
            "<rs:insert><row xmlns=\"urn:wrong\" ID=\"2\" s1=\"inserted\"/></rs:insert>",
            RowState::Inserted,
            RecordStatusFlag::New,
            "inserted",
        ),
        (
            "delete unprefixed no default",
            "<rs:delete><row ID=\"2\" s1=\"deleted\"/></rs:delete>",
            RowState::Deleted,
            RecordStatusFlag::Deleted,
            "deleted",
        ),
        (
            "delete unprefixed wrong default",
            "<rs:delete><row xmlns=\"urn:wrong\" ID=\"2\" s1=\"deleted\"/></rs:delete>",
            RowState::Deleted,
            RecordStatusFlag::Deleted,
            "deleted",
        ),
    ] {
        let xml = SAMPLE
            .replace(r##"     xmlns:z="#RowsetSchema">"##, ">")
            .replace(
                "<z:row ID=\"1\" s1=\"&#xD55C;&#xAE00;\" PAYLOAD=\"DEADBEEF\"/>",
                "<row ID=\"1\" s1=\"&#xD55C;&#xAE00;\" PAYLOAD=\"DEADBEEF\"/>",
            )
            .replace("<z:row ID=\"2\" s1=\"\"/>", replacement);
        let recordset = parse_ado_xml(&xml).unwrap_or_else(|err| panic!("{label}: {err:#}"));

        assert_eq!(recordset.rows.len(), 2, "{label}");
        assert_eq!(recordset.rows[1].state, expected_state, "{label}");
        assert_eq!(
            recordset.rows[1].values[1],
            Value::String(expected_text.to_string()),
            "{label}"
        );
        let pending = materialize_pending_view(&recordset);
        assert_eq!(pending.rows.len(), 1, "{label}");
        assert_eq!(pending.rows[0].status, expected_status, "{label}");
    }
}

#[test]
fn parses_nested_insert_delete_wrappers_inside_insert_and_delete_like_mdac() {
    for (label, replacement, expected_status, expected_text) in [
        (
            "insert containing insert row",
            "<rs:insert><rs:insert><z:row ID=\"2\" s1=\"inner-insert\"/></rs:insert></rs:insert>",
            RecordStatusFlag::New,
            "inner-insert",
        ),
        (
            "delete containing insert row",
            "<rs:delete><rs:insert><z:row ID=\"2\" s1=\"inner-insert\"/></rs:insert></rs:delete>",
            RecordStatusFlag::New,
            "inner-insert",
        ),
        (
            "insert containing delete row",
            "<rs:insert><rs:delete><z:row ID=\"2\" s1=\"inner-delete\"/></rs:delete></rs:insert>",
            RecordStatusFlag::Deleted,
            "inner-delete",
        ),
    ] {
        let xml = SAMPLE.replace("<z:row ID=\"2\" s1=\"\"/>", replacement);
        let recordset = parse_ado_xml(&xml).unwrap_or_else(|err| panic!("{label}: {err:#}"));
        let pending = materialize_pending_view(&recordset);

        assert_eq!(pending.rows.len(), 1, "{label}");
        assert_eq!(pending.rows[0].status, expected_status, "{label}");
        assert_eq!(
            pending.rows[0].values[1],
            Value::String(expected_text.to_string()),
            "{label}"
        );
    }

    for (label, replacement) in [
        (
            "insert containing empty insert",
            "<rs:insert><rs:insert/></rs:insert>",
        ),
        (
            "delete containing empty insert",
            "<rs:delete><rs:insert/></rs:delete>",
        ),
    ] {
        let xml = SAMPLE.replace("<z:row ID=\"2\" s1=\"\"/>", replacement);
        let recordset = parse_ado_xml(&xml).unwrap_or_else(|err| panic!("{label}: {err:#}"));

        assert_eq!(recordset.rows.len(), 1, "{label}");
        assert!(
            materialize_pending_view(&recordset).rows.is_empty(),
            "{label}"
        );
    }
}

#[test]
fn parses_nested_update_wrappers_inside_insert_and_delete_like_mdac() {
    for (label, replacement, expected_status, expected_text) in [
        (
            "insert containing update pair",
            "<rs:insert><rs:update><rs:original><z:row ID=\"2\" s1=\"old\"/></rs:original><z:row ID=\"2\" s1=\"new\"/></rs:update></rs:insert>",
            RecordStatusFlag::Modified,
            "new",
        ),
        (
            "delete containing update pair",
            "<rs:delete><rs:update><rs:original><z:row ID=\"2\" s1=\"old\"/></rs:original><z:row ID=\"2\" s1=\"new\"/></rs:update></rs:delete>",
            RecordStatusFlag::Modified,
            "new",
        ),
    ] {
        let xml = SAMPLE.replace("<z:row ID=\"2\" s1=\"\"/>", replacement);
        let recordset = parse_ado_xml(&xml).unwrap_or_else(|err| panic!("{label}: {err:#}"));
        let pending = materialize_pending_view(&recordset);

        assert_eq!(recordset.rows.len(), 3, "{label}");
        assert_eq!(pending.rows.len(), 1, "{label}");
        assert_eq!(pending.rows[0].status, expected_status, "{label}");
        assert_eq!(
            pending.rows[0].values[1],
            Value::String(expected_text.to_string()),
            "{label}"
        );
    }

    let xml = SAMPLE.replace(
        "<z:row ID=\"2\" s1=\"\"/>",
        "<rs:insert><rs:update><z:row ID=\"2\" s1=\"inner-update\"/></rs:update></rs:insert>",
    );
    let recordset = parse_ado_xml(&xml).expect("single-row nested update should parse");

    assert_eq!(recordset.rows.len(), 2);
    assert_eq!(recordset.rows[1].state, RowState::Current);
    assert_eq!(
        recordset.rows[1].values[1],
        Value::String("inner-update".to_string())
    );
    assert!(materialize_pending_view(&recordset).rows.is_empty());
}

#[test]
fn ignores_wrong_namespace_rows_inside_insert_and_delete_wrappers_like_mdac() {
    for (label, replacement) in [
        (
            "insert unprefixed row",
            "<rs:insert><row ID=\"2\" s1=\"ignored\"/></rs:insert>",
        ),
        (
            "insert wrong namespace row",
            "<rs:insert><x:row ID=\"2\" s1=\"ignored\"/></rs:insert>",
        ),
        (
            "insert rowset namespace row",
            "<rs:insert><rs:row ID=\"2\" s1=\"ignored\"/></rs:insert>",
        ),
        (
            "delete unprefixed row",
            "<rs:delete><row ID=\"2\" s1=\"ignored\"/></rs:delete>",
        ),
        (
            "delete wrong namespace row",
            "<rs:delete><x:row ID=\"2\" s1=\"ignored\"/></rs:delete>",
        ),
    ] {
        let xml = SAMPLE
            .replace(
                r#"<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882""#,
                r#"<xml xmlns:x="urn:wrong" xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882""#,
            )
            .replace("<z:row ID=\"2\" s1=\"\"/>", replacement);

        let recordset = parse_ado_xml(&xml).unwrap_or_else(|err| panic!("{label}: {err:#}"));
        assert_eq!(recordset.rows.len(), 1, "{label}");
        assert_eq!(recordset.rows[0].values[0], Value::Integer(1), "{label}");
        assert_eq!(recordset.changes.len(), 1, "{label}");
    }
}

#[test]
fn rejects_nested_original_pseudo_rows_with_view_dependent_mdac_values() {
    // MDAC accepts rs:original under rs:insert/rs:delete in hand-authored XML,
    // but exposes inserted pseudo-rows with different values in default and
    // pending views. The native row model stores one value vector per row, so
    // keep this malformed reader-only shape rejected until per-view row values
    // are modeled explicitly.
    for (label, replacement, expected_message) in [
        (
            "insert original child",
            "<rs:insert><rs:original><z:row ID=\"2\" s1=\"bad\"/></rs:original></rs:insert>",
            "unexpected ADO XML insert child element original",
        ),
        (
            "delete original child",
            "<rs:delete><rs:original><z:row ID=\"2\" s1=\"bad\"/></rs:original></rs:delete>",
            "unexpected ADO XML delete child element original",
        ),
    ] {
        let xml = SAMPLE.replace("<z:row ID=\"2\" s1=\"\"/>", replacement);
        let err = parse_ado_xml(&xml).expect_err(label);
        let message = format!("{err:#}");
        assert!(message.contains(expected_message), "{label}: {message}");
    }
}

#[test]
fn rejects_unexpected_xml_row_state_wrapper_children() {
    for (label, replacement, expected_message) in [
        (
            "update delete child",
            "<rs:update><rs:delete><z:row ID=\"2\" s1=\"bad\"/></rs:delete></rs:update>",
            "unexpected ADO XML update child element delete",
        ),
        (
            "insert empty update child",
            "<rs:insert><rs:update></rs:update></rs:insert>",
            "unexpected ADO XML insert child element update",
        ),
        (
            "original insert child",
            "<rs:update><rs:original><rs:insert><z:row ID=\"2\" s1=\"bad\"/></rs:insert></rs:original><z:row s1=\"new\"/></rs:update>",
            "unexpected ADO XML original child element insert",
        ),
    ] {
        let xml = SAMPLE.replace("<z:row ID=\"2\" s1=\"\"/>", replacement);
        let err = parse_ado_xml(&xml).expect_err(label);
        let message = format!("{err:#}");
        assert!(message.contains(expected_message), "{label}: {message}");
    }
}

#[test]
fn rejects_unexpected_flat_xml_row_children() {
    let xml = SAMPLE.replace(
        "<z:row ID=\"2\" s1=\"\"/>",
        "<z:row ID=\"2\" s1=\"\"><extra/></z:row>",
    );

    let err = parse_ado_xml(&xml).expect_err("unexpected flat row child should be rejected");
    let message = format!("{err:#}");
    assert!(
        message.contains("unexpected ADO XML row child element extra"),
        "{message}"
    );
}

#[test]
fn parses_unknown_xml_data_children_like_mdac() {
    for (label, replacement, expected_texts) in [
        ("empty child", "<extra/>", vec!["한글"]),
        (
            "nested rowset schema row",
            "<extra><z:row ID=\"2\" s1=\"wrapped\"/></extra>",
            vec!["한글", "wrapped"],
        ),
        (
            "deep nested rowset schema row",
            "<extra><inner><z:row ID=\"2\" s1=\"wrapped\"/></inner></extra>",
            vec!["한글", "wrapped"],
        ),
        (
            "nested unprefixed row",
            "<extra><row ID=\"2\" s1=\"ignored\"/></extra>",
            vec!["한글"],
        ),
        (
            "nested wrong namespace row",
            "<extra><x:row ID=\"2\" s1=\"ignored\"/></extra>",
            vec!["한글"],
        ),
    ] {
        let xml = SAMPLE
            .replace(
                r#"<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882""#,
                r#"<xml xmlns:x="urn:wrong" xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882""#,
            )
            .replace("<z:row ID=\"2\" s1=\"\"/>", replacement);

        let recordset = parse_ado_xml(&xml).unwrap_or_else(|err| panic!("{label}: {err:#}"));
        assert_eq!(recordset.rows.len(), expected_texts.len(), "{label}");
        for (row, expected_text) in recordset.rows.iter().zip(expected_texts) {
            assert_eq!(row.state, RowState::Current, "{label}");
            assert_eq!(
                row.values[1],
                Value::String(expected_text.to_string()),
                "{label}"
            );
        }
        assert!(
            materialize_pending_view(&recordset).rows.is_empty(),
            "{label}"
        );
    }
}

#[test]
fn parses_rowset_change_wrappers_inside_unknown_data_children_like_mdac() {
    for (label, replacement, expected_state, expected_status) in [
        (
            "nested insert",
            "<extra><rs:insert><z:row ID=\"2\" s1=\"inserted\"/></rs:insert></extra>",
            RowState::Inserted,
            RecordStatusFlag::New,
        ),
        (
            "nested delete",
            "<extra><rs:delete><z:row ID=\"2\" s1=\"deleted\"/></rs:delete></extra>",
            RowState::Deleted,
            RecordStatusFlag::Deleted,
        ),
    ] {
        let xml = SAMPLE.replace("<z:row ID=\"2\" s1=\"\"/>", replacement);
        let recordset = parse_ado_xml(&xml).unwrap_or_else(|err| panic!("{label}: {err:#}"));

        assert_eq!(recordset.rows.len(), 2, "{label}");
        assert_eq!(recordset.rows[1].state, expected_state, "{label}");
        let pending = materialize_pending_view(&recordset);
        assert_eq!(pending.rows.len(), 1, "{label}");
        assert_eq!(pending.rows[0].status, expected_status, "{label}");
    }

    let xml = SAMPLE.replace(
        "<z:row ID=\"2\" s1=\"\"/>",
        "<extra><rs:update><rs:original><z:row ID=\"2\" s1=\"old\"/></rs:original><z:row ID=\"2\" s1=\"new\"/></rs:update></extra>",
    );
    let recordset = parse_ado_xml(&xml).unwrap();

    assert_eq!(recordset.rows.len(), 3);
    assert_eq!(recordset.rows[1].state, RowState::Original);
    assert_eq!(recordset.rows[2].state, RowState::Updated);
    let pending = materialize_pending_view(&recordset);
    assert_eq!(pending.rows.len(), 1);
    assert_eq!(pending.rows[0].status, RecordStatusFlag::Modified);
    assert_eq!(pending.rows[0].values[1], Value::String("new".to_string()));
}

#[test]
fn parses_missing_xml_data_section_as_empty_rowset_like_mdac() {
    let xml = SAMPLE.replace(
        "  <rs:data>\n    <z:row ID=\"1\" s1=\"&#xD55C;&#xAE00;\" PAYLOAD=\"DEADBEEF\"/>\n    <z:row ID=\"2\" s1=\"\"/>\n  </rs:data>\n",
        "",
    );

    let recordset = parse_ado_xml(&xml).unwrap();
    assert_eq!(recordset.fields.len(), 3);
    assert!(recordset.rows.is_empty());
    assert!(recordset.changes.is_empty());
}

#[test]
fn ignores_wrong_namespace_xml_data_sections_like_mdac() {
    for (label, open, close) in [
        ("wrong namespace", r#"<x:data>"#, r#"</x:data>"#),
        ("unprefixed", r#"<data>"#, r#"</data>"#),
    ] {
        let xml = SAMPLE
            .replace(
                r#"<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882""#,
                r#"<xml xmlns:x="urn:wrong" xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882""#,
            )
            .replace("<rs:data>", open)
            .replace("</rs:data>", close);

        let recordset = parse_ado_xml(&xml).unwrap_or_else(|err| panic!("{label}: {err:#}"));
        assert_eq!(recordset.fields.len(), 3, "{label}");
        assert!(recordset.rows.is_empty(), "{label}");
        assert!(recordset.changes.is_empty(), "{label}");
    }
}

#[test]
fn parses_unprefixed_and_rowset_schema_namespace_rows_like_mdac() {
    for (label, xml) in [
        ("unprefixed", SAMPLE.replace("<z:row", "<row")),
        (
            "alternate RowsetSchema prefix",
            SAMPLE
                .replace(
                    r##"xmlns:z="#RowsetSchema">"##,
                    r##"xmlns:p="#RowsetSchema">"##,
                )
                .replace("<z:row", "<p:row"),
        ),
    ] {
        let recordset = parse_ado_xml(&xml).unwrap_or_else(|err| panic!("{label}: {err:#}"));
        assert_eq!(recordset.rows.len(), 2, "{label}");
        assert_eq!(recordset.rows[0].values[0], Value::Integer(1), "{label}");
        assert_eq!(recordset.rows[1].values[0], Value::Integer(2), "{label}");
    }
}

#[test]
fn rejects_multiple_rowset_schema_namespace_prefixes_like_mdac() {
    let xml = SAMPLE.replace(
        r##"xmlns:z="#RowsetSchema">"##,
        r##"xmlns:z="#RowsetSchema" xmlns:p="#RowsetSchema">"##,
    );

    let err = parse_ado_xml(&xml)
        .expect_err("duplicate RowsetSchema namespace prefixes should be rejected");
    let message = format!("{err:#}");
    assert!(
        message.contains("declared multiple RowsetSchema namespace prefixes"),
        "{message}"
    );
}

#[test]
fn ignores_wrong_namespace_xml_rows_like_mdac() {
    for (label, open) in [
        ("wrong namespace", r#"<x:row"#),
        ("rowset namespace", r#"<rs:row"#),
    ] {
        let xml = SAMPLE
            .replace(
                r#"<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882""#,
                r#"<xml xmlns:x="urn:wrong" xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882""#,
            )
            .replace(r#"<z:row ID="1""#, &format!(r#"{open} ID="1""#));

        let recordset = parse_ado_xml(&xml).unwrap_or_else(|err| panic!("{label}: {err:#}"));
        assert_eq!(recordset.rows.len(), 1, "{label}");
        assert_eq!(recordset.rows[0].values[0], Value::Integer(2), "{label}");
    }
}

#[test]
fn parses_xml_rows_after_scoped_namespace_redeclarations() {
    for (label, xml, expected_rows) in [
        (
            "self-closing prefix redeclaration",
            SAMPLE.replace(
                r#"<z:row ID="1" s1="&#xD55C;&#xAE00;" PAYLOAD="DEADBEEF"/>"#,
                r#"<z:row xmlns:z="urn:wrong" ID="1" s1="ignored" PAYLOAD="00"/>"#,
            ),
            vec![(1, "ignored"), (2, "")],
        ),
        (
            "self-closing default redeclaration",
            SAMPLE
                .replace(
                    r#"<z:row ID="1" s1="&#xD55C;&#xAE00;" PAYLOAD="DEADBEEF"/>"#,
                    r#"<row xmlns="urn:wrong" ID="1" s1="ignored" PAYLOAD="00"/>"#,
                )
                .replace(r#"<z:row ID="2" s1=""/>"#, r#"<row ID="2" s1="plain"/>"#),
            vec![(2, "plain")],
        ),
        (
            "container prefix redeclaration",
            SAMPLE.replace(
                r#"<z:row ID="2" s1=""/>"#,
                r#"<extra xmlns:z="urn:wrong"><z:row ID="9" s1="ignored"/></extra><z:row ID="2" s1="after"/>"#,
            ),
            vec![(1, "한글"), (9, "ignored"), (2, "after")],
        ),
        (
            "row-local RowsetSchema prefix declaration",
            SAMPLE
                .replace(r##"     xmlns:z="#RowsetSchema">"##, ">")
                .replace("<z:row", r##"<p:row xmlns:p="#RowsetSchema""##),
            vec![],
        ),
        (
            "data-local RowsetSchema prefix declaration",
            SAMPLE
                .replace(r##"     xmlns:z="#RowsetSchema">"##, ">")
                .replace("<rs:data>", r##"<rs:data xmlns:p="#RowsetSchema">"##)
                .replace("<z:row", "<p:row"),
            vec![],
        ),
        (
            "unknown ancestor RowsetSchema prefix declaration",
            SAMPLE.replace(
                r#"<z:row ID="1" s1="&#xD55C;&#xAE00;" PAYLOAD="DEADBEEF"/>"#,
                r##"<extra xmlns:p="#RowsetSchema"><p:row ID="1" s1="ignored" PAYLOAD="00"/></extra>"##,
            ),
            vec![(2, "")],
        ),
        (
            "unprefixed rows with local wrong default and no root row prefix",
            SAMPLE
                .replace(r##"     xmlns:z="#RowsetSchema">"##, ">")
                .replace("<z:row", r#"<row xmlns="urn:wrong""#),
            vec![(1, "한글"), (2, "")],
        ),
    ] {
        let recordset = parse_ado_xml(&xml).unwrap_or_else(|err| panic!("{label}: {err:#}"));
        assert_eq!(recordset.rows.len(), expected_rows.len(), "{label}");
        for (row, (expected_id, expected_text)) in recordset.rows.iter().zip(expected_rows) {
            assert_eq!(row.values[0], Value::Integer(expected_id), "{label}");
            assert_eq!(
                row.values[1],
                Value::String(expected_text.to_string()),
                "{label}"
            );
        }
    }
}

#[test]
fn rejects_missing_xml_schema_section() {
    let xml = SAMPLE
        .replace(
            "  <s:Schema id=\"RowsetSchema\">\n    <s:ElementType name=\"row\" content=\"eltOnly\" rs:updatable=\"true\">\n      <s:AttributeType name=\"ID\" rs:number=\"1\">\n        <s:datatype dt:type=\"int\" dt:maxLength=\"4\" rs:maybenull=\"false\"/>\n      </s:AttributeType>\n      <s:AttributeType name=\"s1\" rs:name=\"NAME\" rs:number=\"2\">\n        <s:datatype dt:type=\"string\" dt:maxLength=\"255\" rs:maybenull=\"true\"/>\n      </s:AttributeType>\n      <s:AttributeType name=\"PAYLOAD\" rs:number=\"3\">\n        <s:datatype dt:type=\"bin.hex\" rs:maybenull=\"true\"/>\n      </s:AttributeType>\n      <s:extends type=\"rs:rowbase\"/>\n    </s:ElementType>\n  </s:Schema>\n",
            "",
        );

    let err = parse_ado_xml(&xml).expect_err("missing schema section should be rejected");
    let message = format!("{err:#}");
    assert!(
        message.contains("ADO XML schema section was not found"),
        "{message}"
    );
}

#[test]
fn rejects_non_ado_xml_root_element() {
    let xml = SAMPLE
        .replace("<xml ", "<notxml ")
        .replace("</xml>", "</notxml>");

    let err = parse_ado_xml(&xml).expect_err("non-ADO root should be rejected");
    let message = format!("{err:#}");
    assert!(
        message.contains("ADO XML root element must be xml, found notxml"),
        "{message}"
    );
}

#[test]
fn rejects_multiple_xml_schema_sections() {
    let xml = SAMPLE.replace("</s:Schema>", "</s:Schema><s:Schema id=\"OtherSchema\"/>");

    let err = parse_ado_xml(&xml).expect_err("duplicate schema section should be rejected");
    let message = format!("{err:#}");
    assert!(
        message.contains("ADO XML contained multiple schema sections"),
        "{message}"
    );
}

#[test]
fn parses_multiple_xml_data_sections_like_mdac() {
    let data_block = "  <rs:data>\n    <z:row ID=\"1\" s1=\"&#xD55C;&#xAE00;\" PAYLOAD=\"DEADBEEF\"/>\n    <z:row ID=\"2\" s1=\"\"/>\n  </rs:data>\n";
    let xml = SAMPLE.replace(data_block, &format!("{data_block}{data_block}"));

    let recordset = parse_ado_xml(&xml).unwrap();
    assert_eq!(recordset.rows.len(), 4);
    assert_eq!(
        recordset
            .rows
            .iter()
            .map(|row| row.values[0].clone())
            .collect::<Vec<_>>(),
        vec![
            Value::Integer(1),
            Value::Integer(2),
            Value::Integer(1),
            Value::Integer(2),
        ]
    );
}

#[test]
fn rejects_nested_xml_schema_section() {
    let xml = r##"<?xml version="1.0"?>
<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"
     xmlns:dt="uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"
     xmlns:rs="urn:schemas-microsoft-com:rowset"
     xmlns:z="#RowsetSchema">
  <rs:data>
    <s:Schema id="RowsetSchema">
      <s:ElementType name="row" content="eltOnly">
        <s:AttributeType name="ID" rs:number="1">
          <s:datatype dt:type="int" dt:maxLength="4" rs:maybenull="false"/>
        </s:AttributeType>
        <s:extends type="rs:rowbase"/>
      </s:ElementType>
    </s:Schema>
    <z:row ID="1"/>
  </rs:data>
</xml>"##;

    let err = parse_ado_xml(xml).expect_err("nested schema section should be rejected");
    let message = format!("{err:#}");
    assert!(
        message.contains("ADO XML schema section must be a direct child of xml"),
        "{message}"
    );
}

#[test]
fn rejects_nested_xml_data_section() {
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
      <s:extends type="rs:rowbase"/>
    </s:ElementType>
    <rs:data>
      <z:row ID="1"/>
    </rs:data>
  </s:Schema>
</xml>"##;

    let err = parse_ado_xml(xml).expect_err("nested data section should be rejected");
    let message = format!("{err:#}");
    assert!(
        message.contains("unexpected ADO XML Schema child element data"),
        "{message}"
    );
}

#[test]
fn rejects_unexpected_xml_root_children() {
    let xml = SAMPLE.replace("  <rs:data>", "  <extra/>\n  <rs:data>");

    let err = parse_ado_xml(&xml).expect_err("unexpected root child should be rejected");
    let message = format!("{err:#}");
    assert!(
        message.contains("unexpected ADO XML xml child element extra"),
        "{message}"
    );
}

#[test]
fn rejects_row_like_elements_inside_xml_schema() {
    let xml = SAMPLE.replace(
        "  </s:Schema>",
        "    <z:row ID=\"999\" s1=\"schema\" PAYLOAD=\"00\"/>\n  </s:Schema>",
    );

    let err = parse_ado_xml(&xml).expect_err("schema row-like element should be rejected");
    let message = format!("{err:#}");
    assert!(
        message.contains("unexpected ADO XML Schema child element row"),
        "{message}"
    );
}

#[test]
fn rejects_row_schema_outside_direct_schema_section() {
    let xml = r##"<?xml version="1.0"?>
<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"
     xmlns:dt="uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"
     xmlns:rs="urn:schemas-microsoft-com:rowset"
     xmlns:z="#RowsetSchema">
  <s:Schema id="RowsetSchema"/>
  <rs:data>
    <s:ElementType name="row" content="eltOnly">
      <s:AttributeType name="ID">
        <s:datatype dt:type="int" dt:maxLength="4" rs:maybenull="false"/>
      </s:AttributeType>
      <s:extends type="rs:rowbase"/>
    </s:ElementType>
    <z:row ID="1"/>
  </rs:data>
</xml>"##;

    let err = parse_ado_xml(xml).expect_err("row schema outside direct schema should be rejected");
    let message = format!("{err:#}");
    assert!(
        message.contains("ADO XML row schema was not found"),
        "{message}"
    );
}

#[test]
fn ignores_unreferenced_schema_attribute_types_outside_row_element() {
    let xml = r##"<?xml version="1.0"?>
<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"
     xmlns:dt="uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"
     xmlns:rs="urn:schemas-microsoft-com:rowset"
     xmlns:z="#RowsetSchema">
  <s:Schema id="RowsetSchema">
    <s:AttributeType name="UNREFERENCED" dt:type="int" rs:maybenull="false"/>
    <s:ElementType name="row" content="eltOnly">
      <s:AttributeType name="ID" dt:type="int" rs:maybenull="false"/>
      <s:extends type="rs:rowbase"/>
    </s:ElementType>
  </s:Schema>
  <rs:data>
    <z:row ID="1"/>
  </rs:data>
</xml>"##;

    let recordset = parse_ado_xml(xml).unwrap();
    assert_eq!(
        recordset
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["ID"]
    );
    assert_eq!(recordset.rows[0].values, vec![Value::Integer(1)]);
}

#[test]
fn rejects_xml_without_row_schema() {
    let xml = r##"<?xml version="1.0"?>
<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"
     xmlns:rs="urn:schemas-microsoft-com:rowset"
     xmlns:z="#RowsetSchema">
  <s:Schema id="RowsetSchema"/>
  <rs:data>
    <z:row ID="1"/>
  </rs:data>
</xml>"##;

    let err = parse_ado_xml(xml).expect_err("missing row schema should be rejected");
    let message = format!("{err:#}");
    assert!(
        message.contains("ADO XML row schema was not found"),
        "{message}"
    );
}

#[test]
fn rejects_multiple_xml_row_schemas() {
    let xml = SAMPLE.replace(
        "      <s:extends type=\"rs:rowbase\"/>\n    </s:ElementType>",
        "      <s:extends type=\"rs:rowbase\"/>\n    </s:ElementType>\n    <s:ElementType name=\"row\" content=\"eltOnly\"><s:extends type=\"rs:rowbase\"/></s:ElementType>",
    );

    let err = parse_ado_xml(&xml).expect_err("duplicate row schema should be rejected");
    let message = format!("{err:#}");
    assert!(
        message.contains("ADO XML contained multiple row schemas"),
        "{message}"
    );
}

#[test]
fn rejects_xml_schema_without_visible_fields() {
    let xml = r##"<?xml version="1.0"?>
<xml xmlns:s="uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"
     xmlns:dt="uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"
     xmlns:rs="urn:schemas-microsoft-com:rowset"
     xmlns:z="#RowsetSchema">
  <s:Schema id="RowsetSchema">
    <s:ElementType name="row" content="eltOnly">
      <s:AttributeType name="HIDDEN" rs:hidden="true">
        <s:datatype dt:type="int" dt:maxLength="4" rs:maybenull="false"/>
      </s:AttributeType>
      <s:extends type="rs:rowbase"/>
    </s:ElementType>
  </s:Schema>
  <rs:data>
    <z:row HIDDEN="1"/>
  </rs:data>
</xml>"##;

    let err = parse_ado_xml(xml).expect_err("schema with no visible fields should be rejected");
    let message = format!("{err:#}");
    assert!(
        message.contains("ADO XML row schema had no visible fields"),
        "{message}"
    );
}

#[test]
fn rejects_duplicate_xml_field_names() {
    let xml = SAMPLE
        .replace(
            r#"<s:AttributeType name="PAYLOAD" rs:number="3">"#,
            r#"<s:AttributeType name="s1" rs:name="ALSO_NAME" rs:number="3">"#,
        )
        .replace(r#" PAYLOAD="DEADBEEF""#, "");

    let err = parse_ado_xml(&xml).expect_err("duplicate XML field name should be rejected");
    let message = format!("{err:#}");
    assert!(
        message.contains("ADO XML row schema contained duplicate field XML name s1"),
        "{message}"
    );
}

#[test]
fn rejects_duplicate_xml_field_names_differing_only_by_case_like_mdac() {
    let xml = SAMPLE
        .replace(
            r#"<s:AttributeType name="PAYLOAD" rs:number="3">"#,
            r#"<s:AttributeType name="S1" rs:name="ALSO_NAME" rs:number="3">"#,
        )
        .replace(r#" PAYLOAD="DEADBEEF""#, "");

    let err = parse_ado_xml(&xml).expect_err("case-only duplicate XML field name should reject");
    let message = format!("{err:#}");
    assert!(
        message.contains("ADO XML row schema contained duplicate field XML name S1"),
        "{message}"
    );
}

#[test]
fn allows_duplicate_visible_names_with_unique_xml_field_names() {
    let xml = SAMPLE
        .replace(r#"rs:name="NAME""#, r#"rs:name="DUP""#)
        .replace(
            r#"<s:AttributeType name="PAYLOAD" rs:number="3">"#,
            r#"<s:AttributeType name="c1" rs:name="DUP" rs:number="3">"#,
        )
        .replace(
            r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
            r#"<s:datatype dt:type="string" dt:maxLength="255" rs:maybenull="true"/>"#,
        )
        .replace(r#" PAYLOAD="DEADBEEF""#, r#" c1="alias-value""#);

    let recordset = parse_ado_xml(&xml).unwrap();
    assert_eq!(
        recordset
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["ID", "DUP", "DUP"]
    );
    assert_eq!(
        recordset
            .fields
            .iter()
            .map(|field| field.xml_name.as_str())
            .collect::<Vec<_>>(),
        vec!["ID", "s1", "c1"]
    );
    assert_eq!(
        recordset.rows[0].values[2],
        Value::String("alias-value".to_string())
    );
}

fn sample_with_id_type_and_value(data_type: &str, value: &str) -> String {
    SAMPLE
        .replace(
            r#"dt:type="int" dt:maxLength="4""#,
            &format!(r#"dt:type="{data_type}" dt:maxLength="4""#),
        )
        .replace(r#"ID="1""#, &format!(r#"ID="{value}""#))
}

fn sample_with_payload_type_and_value(data_type: &str, value: &str) -> String {
    sample_with_payload_datatype_and_value(
        &format!(r#"<s:datatype dt:type="{data_type}" dt:maxLength="4" rs:maybenull="true"/>"#),
        value,
    )
}

fn sample_with_payload_datatype_and_value(datatype: &str, value: &str) -> String {
    SAMPLE
        .replace(
            r#"<s:datatype dt:type="bin.hex" rs:maybenull="true"/>"#,
            datatype,
        )
        .replace(r#"PAYLOAD="DEADBEEF""#, &format!(r#"PAYLOAD="{value}""#))
}

fn utf16_bytes(text: &str, little_endian: bool) -> Vec<u8> {
    let mut bytes = if little_endian {
        vec![0xff, 0xfe]
    } else {
        vec![0xfe, 0xff]
    };
    for unit in text.encode_utf16() {
        let encoded = if little_endian {
            unit.to_le_bytes()
        } else {
            unit.to_be_bytes()
        };
        bytes.extend_from_slice(&encoded);
    }
    bytes
}
