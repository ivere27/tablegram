//! Parser for ADO XML persistence documents.
//!
//! `roxmltree` is used for structural XML traversal, while a small raw
//! tag/attribute scanner runs alongside it to recover MDAC-visible details that
//! normal XML parsing normalizes away, including duplicate namespace prefixes
//! and original row attribute order.

use anyhow::{anyhow, Context, Result};
use roxmltree::{Document, Node};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use crate::detect::strip_utf8_bom;
use crate::model::{
    AdoDataType, ChapterRelation, ChapterRelationPair, Field, FieldAttribute, RecordStatusFlag,
    Recordset, Row, RowChange, RowChangeKind, RowState, Value,
};
use crate::util::gregorian_month_len;
use crate::ResourceLimits;

const SCHEMA_NS: &str = "uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882";
const DATATYPE_NS: &str = "uuid:C2F41010-65B3-11d1-A29F-00AA00C14882";
const ROWSET_NS: &str = "urn:schemas-microsoft-com:rowset";
const ROWSET_SCHEMA_NS: &str = "#RowsetSchema";

/// Parse persisted ADO XML bytes into a [`Recordset`].
///
/// UTF-8 and UTF-16 XML streams are accepted according to the document bytes.
pub fn parse_ado_xml_bytes(bytes: &[u8]) -> Result<Recordset> {
    parse_ado_xml_bytes_with_limits(bytes, ResourceLimits::default())
}

pub(crate) fn parse_ado_xml_bytes_with_limits(
    bytes: &[u8],
    limits: ResourceLimits,
) -> Result<Recordset> {
    limits.check_input_bytes(bytes.len(), "ADO XML input")?;
    let text = decode_ado_xml_text(bytes)?;
    parse_ado_xml_with_limits(text.trim_start_matches('\u{feff}'), limits)
}

/// Parse persisted ADO XML text into a [`Recordset`].
///
/// This accepts the XML schema/updategram shape produced by MDAC ADO
/// `Recordset.Save(..., adPersistXML)`.
pub fn parse_ado_xml(text: &str) -> Result<Recordset> {
    parse_ado_xml_with_limits(text, ResourceLimits::default())
}

pub(crate) fn parse_ado_xml_with_limits(text: &str, limits: ResourceLimits) -> Result<Recordset> {
    limits.check_input_bytes(text.len(), "ADO XML input")?;
    validate_raw_rowset_schema_namespace_prefixes(text)?;
    let root_rowset_schema_prefixes = raw_root_rowset_schema_prefixes(text)?;
    let position_mapper = StructuralPositionMapper::new(text);
    let structural_text = sanitize_xml_text_for_structural_parse(text);
    let doc = Document::parse(&structural_text).context("failed to parse ADO Recordset XML")?;
    validate_xml_root(&doc)?;
    validate_xml_root_sections(&doc)?;
    let schema_node = xml_schema_node(&doc)?;
    validate_xml_schema_tree(schema_node)?;
    let mut raw_field_attrs = RawElementAttributes::parse(text, "AttributeType", limits)?;

    let (fields, rows, changes) = if let Some(schema) =
        parse_shaped_schema(schema_node, &mut raw_field_attrs, limits)?
    {
        raw_field_attrs.finish()?;
        let row_element_names = shaped_row_element_names(&schema);
        let mut raw_row_attrs = RawRowAttributes::parse(
            text,
            &row_element_names,
            position_mapper,
            root_rowset_schema_prefixes,
            limits,
        )?;
        let (rows, changes) = parse_rows_with_schema(&doc, &schema, &mut raw_row_attrs, limits)?;
        raw_row_attrs.finish()?;
        (schema.fields, rows, changes)
    } else {
        let fields = parse_fields(schema_node, &mut raw_field_attrs, limits)?;
        raw_field_attrs.finish()?;
        let row_element_names = HashSet::from(["row".to_string()]);
        let mut raw_row_attrs = RawRowAttributes::parse(
            text,
            &row_element_names,
            position_mapper,
            root_rowset_schema_prefixes,
            limits,
        )?;
        let (rows, changes) = parse_rows(&doc, &fields, &mut raw_row_attrs, limits)?;
        raw_row_attrs.finish()?;
        (fields, rows, changes)
    };

    let recordset = Recordset {
        fields,
        rows,
        changes,
    };
    crate::validate_recordset_shape(&recordset)
        .context("parsed ADO XML Recordset shape was inconsistent")?;
    crate::validate_recordset_resource_limits(&recordset, limits)
        .context("parsed ADO XML Recordset exceeded resource limits")?;
    Ok(recordset)
}

fn sanitize_xml_text_for_structural_parse(text: &str) -> Cow<'_, str> {
    if text.chars().all(is_xml_char) {
        return Cow::Borrowed(text);
    }

    Cow::Owned(
        text.chars()
            .map(|ch| if is_xml_char(ch) { ch } else { '\u{fffd}' })
            .collect(),
    )
}

fn is_xml_char(ch: char) -> bool {
    matches!(
        ch as u32,
        0x09 | 0x0A | 0x0D | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF
    )
}

fn validate_raw_rowset_schema_namespace_prefixes(text: &str) -> Result<()> {
    let mut offset = 0usize;
    while let Some(relative) = text[offset..].find('<') {
        let tag_start = offset + relative;
        if let Some(next_offset) = ignored_xml_markup_end(text, tag_start)? {
            offset = next_offset;
            continue;
        }
        if let Some((_, tag_end)) = parse_end_tag(text, tag_start)? {
            offset = tag_end;
            continue;
        }
        let Some((tag_name, attrs_start, tag_end)) = parse_start_tag(text, tag_start)? else {
            offset = tag_start + 1;
            continue;
        };

        let raw_attrs_text = &text[attrs_start..tag_end];
        let rowset_schema_prefix_count = parse_raw_attribute_list(raw_attrs_text)?
            .iter()
            .filter(|attr| {
                attr.name.starts_with("xmlns:") && attr.value.eq_ignore_ascii_case(ROWSET_SCHEMA_NS)
            })
            .count();
        if rowset_schema_prefix_count > 1 {
            return Err(anyhow!(
                "ADO XML element {tag_name} declared multiple RowsetSchema namespace prefixes"
            ));
        }
        offset = tag_end + 1;
    }

    Ok(())
}

fn raw_root_rowset_schema_prefixes(text: &str) -> Result<HashSet<String>> {
    let mut offset = 0usize;
    while let Some(relative) = text[offset..].find('<') {
        let tag_start = offset + relative;
        if let Some(next_offset) = ignored_xml_markup_end(text, tag_start)? {
            offset = next_offset;
            continue;
        }
        if let Some((_, tag_end)) = parse_end_tag(text, tag_start)? {
            offset = tag_end;
            continue;
        }
        let Some((_, attrs_start, tag_end)) = parse_start_tag(text, tag_start)? else {
            offset = tag_start + 1;
            continue;
        };

        return parse_raw_attribute_list(&text[attrs_start..tag_end]).map(|attrs| {
            attrs
                .into_iter()
                .filter_map(|attr| {
                    attr.name
                        .strip_prefix("xmlns:")
                        .filter(|_| attr.value.eq_ignore_ascii_case(ROWSET_SCHEMA_NS))
                        .map(str::to_string)
                })
                .collect()
        });
    }

    Ok(HashSet::new())
}

fn validate_xml_root(doc: &Document<'_>) -> Result<()> {
    let root = doc.root_element();
    if !root.tag_name().name().eq_ignore_ascii_case("xml") {
        return Err(anyhow!(
            "ADO XML root element must be xml, found {}",
            root.tag_name().name()
        ));
    }
    Ok(())
}

fn validate_xml_root_sections(doc: &Document<'_>) -> Result<()> {
    let root = doc.root_element();
    let mut schema_count = 0usize;

    for child in root.children().filter(|node| node.is_element()) {
        if is_schema_element_named(child, "Schema") {
            schema_count += 1;
        } else if is_element_named(child, "data") {
        } else {
            return Err(unexpected_xml_child("xml", child));
        }
    }

    if schema_count == 0 {
        return Err(missing_or_nested_root_section_error(
            doc, "Schema", "schema",
        ));
    }
    if schema_count > 1 {
        return Err(anyhow!("ADO XML contained multiple schema sections"));
    }

    Ok(())
}

fn missing_or_nested_root_section_error(
    doc: &Document<'_>,
    local_name: &str,
    section_name: &str,
) -> anyhow::Error {
    if doc
        .descendants()
        .any(|node| is_schema_element_named(node, local_name) || is_element_named(node, local_name))
    {
        anyhow!("ADO XML {section_name} section must be a direct child of xml")
    } else {
        anyhow!("ADO XML {section_name} section was not found")
    }
}

fn xml_schema_node<'a, 'input>(doc: &'a Document<'input>) -> Result<Node<'a, 'input>> {
    let root = doc.root_element();
    let mut schema_nodes = root
        .children()
        .filter(|node| is_schema_element_named(*node, "Schema"));
    let Some(schema_node) = schema_nodes.next() else {
        return Err(anyhow!("ADO XML schema section was not found"));
    };
    if schema_nodes.next().is_some() {
        return Err(anyhow!("ADO XML contained multiple schema sections"));
    }
    Ok(schema_node)
}

fn validate_xml_schema_tree(schema_node: Node<'_, '_>) -> Result<()> {
    for child in schema_node.children().filter(|node| node.is_element()) {
        if is_schema_element_named(child, "ElementType") {
            validate_xml_element_type_tree(child, 0)?;
        } else if is_schema_element_named(child, "AttributeType") {
            validate_xml_attribute_type_tree(child)?;
        } else if is_ignored_xml_schema_noise_element(child) {
        } else {
            return Err(unexpected_xml_child("Schema", child));
        }
    }
    Ok(())
}

fn validate_xml_element_type_tree(element_type: Node<'_, '_>, depth: usize) -> Result<()> {
    validate_xml_chapter_depth("ADO XML schema tree", depth)?;
    for child in element_type.children().filter(|node| node.is_element()) {
        if is_schema_element_named(child, "ElementType") {
            validate_xml_element_type_tree(child, depth + 1)?;
        } else if is_schema_element_named(child, "AttributeType") {
            validate_xml_attribute_type_tree(child)?;
        } else if !is_schema_element_named(child, "attribute")
            && !is_schema_element_named(child, "extends")
            && !is_ignored_xml_schema_noise_element(child)
        {
            return Err(unexpected_xml_child("ElementType", child));
        }
    }
    Ok(())
}

fn validate_xml_chapter_depth(context: &str, depth: usize) -> Result<()> {
    if depth > crate::MAX_RECORDSET_DEPTH {
        return Err(anyhow!(
            "{context}: exceeded maximum ADO Recordset chapter depth {}",
            crate::MAX_RECORDSET_DEPTH
        ));
    }
    Ok(())
}

fn validate_xml_attribute_type_tree(attribute_type: Node<'_, '_>) -> Result<()> {
    for child in attribute_type.children().filter(|node| node.is_element()) {
        if !is_schema_element_named(child, "datatype")
            && !is_wrong_namespace_schema_element_named(child, "datatype")
            && !is_ignored_xml_schema_noise_element(child)
        {
            return Err(unexpected_xml_child("AttributeType", child));
        }
    }
    Ok(())
}

fn parse_fields(
    schema_node: Node<'_, '_>,
    raw_field_attrs: &mut RawElementAttributes,
    limits: ResourceLimits,
) -> Result<Vec<Field>> {
    let row_element = xml_row_schema_element(schema_node)?;
    let mut fields: Vec<(usize, Field)> = Vec::new();
    let row_field_order = row_element_attribute_order(row_element);
    let field_scope = if row_field_order.is_some() {
        schema_node
    } else {
        row_element
    };

    for (index, node) in schema_node.descendants().enumerate() {
        if !is_schema_element_named(node, "AttributeType") {
            continue;
        }
        if has_ignored_xml_schema_noise_ancestor(node) {
            continue;
        }

        let raw_attrs = raw_field_attrs.take_next()?;
        if !node_is_or_descendant_of(node, field_scope) {
            continue;
        }

        if let Some(field) = parse_attribute_field(index, node, raw_attrs)? {
            fields.push((index, field));
            limits.check_fields(fields.len(), "ADO XML row schema")?;
        }
    }

    if let Some(row_field_order) = row_field_order {
        fields.retain(|(_, field)| row_field_order.contains_key(&xml_name_key(&field.xml_name)));
        fields.sort_by(|left, right| {
            row_field_order[&xml_name_key(&left.1.xml_name)]
                .cmp(&row_field_order[&xml_name_key(&right.1.xml_name)])
                .then_with(|| left.0.cmp(&right.0))
        });
    } else {
        fields.sort_by(|left, right| match (left.1.ordinal, right.1.ordinal) {
            (Some(a), Some(b)) => a.cmp(&b).then_with(|| left.0.cmp(&right.0)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => left.0.cmp(&right.0),
        });
    }

    let fields = fields
        .into_iter()
        .map(|(_, field)| field)
        .collect::<Vec<_>>();
    limits.check_fields(fields.len(), "ADO XML row schema")?;
    validate_xml_schema_fields("row", &fields)?;
    Ok(fields)
}

fn parse_attribute_field(
    _index: usize,
    node: Node<'_, '_>,
    raw_attrs: &[RawAttribute],
) -> Result<Option<Field>> {
    let Some(xml_name) = raw_attr_unprefixed(raw_attrs, "name")
        .cloned()
        .or_else(|| attr_unprefixed(node, "name"))
    else {
        return Ok(None);
    };
    if bool_attr_rs(node, "hidden")? {
        return Ok(None);
    }
    let datatype = datatype_node(node);
    let data_type = attr_dt(node, "type")
        .or_else(|| datatype.and_then(|n| attr_dt(n, "type")))
        .or_else(|| Some("string".to_string()));
    let db_type = attr_rs(node, "dbtype").or_else(|| datatype.and_then(|n| attr_rs(n, "dbtype")));

    let max_length_attr =
        attr_dt(node, "maxLength").or_else(|| datatype.and_then(|n| attr_dt(n, "maxLength")));
    let explicit_max_length = max_length_attr.is_some();
    let max_length = max_length_attr
        .as_deref()
        .map(parse_max_length)
        .transpose()?
        .flatten();
    let precision = attr_rs(node, "precision")
        .or_else(|| datatype.and_then(|n| attr_rs(n, "precision")))
        .map(|value| parse_xml_usize_attr(&value, "rs:precision"))
        .transpose()?;
    let scale = attr_rs(node, "scale")
        .or_else(|| datatype.and_then(|n| attr_rs(n, "scale")))
        .map(|value| parse_xml_i32_attr(&value, "rs:scale"))
        .transpose()?;

    let explicit_nullable = bool_attr_rs(node, "nullable")?
        || datatype
            .map(|n| bool_attr_rs(n, "nullable"))
            .transpose()?
            .unwrap_or(false);
    let node_maybenull = attr_rs(node, "maybenull");
    let datatype_maybenull = datatype.and_then(|n| attr_rs(n, "maybenull"));
    let may_be_null = if let Some(value) = node_maybenull {
        parse_xml_bool_attr(&value, "maybenull")?
    } else if let Some(value) = datatype_maybenull {
        parse_xml_bool_attr(&value, "maybenull")?
    } else {
        true
    };
    let nullable = explicit_nullable || may_be_null;
    let writable = bool_attr_rs(node, "write")?;
    let explicit_fixed_length = bool_attr_rs(node, "fixedlength")?
        || datatype
            .map(|n| bool_attr_rs(n, "fixedlength"))
            .transpose()?
            .unwrap_or(false);
    let explicit_long = bool_attr_rs(node, "long")?
        || datatype
            .map(|n| bool_attr_rs(n, "long"))
            .transpose()?
            .unwrap_or(false);
    let key_column = bool_attr_rs(node, "keycolumn")?;

    let ordinal = attr_rs(node, "number")
        .map(|value| parse_xml_usize_attr(&value, "rs:number"))
        .transpose()?;
    let ado_type = infer_ado_type(
        data_type.as_deref(),
        db_type.as_deref(),
        max_length,
        explicit_long,
        explicit_fixed_length,
    );
    validate_xml_variant_max_length(
        data_type.as_deref(),
        max_length,
        explicit_max_length,
        &xml_name,
    )?;
    let max_length = normalize_max_length(max_length, explicit_max_length, ado_type);
    let fixed_length = explicit_fixed_length || ado_type_is_fixed_length(ado_type);
    let long = explicit_long || matches!(ado_type.map(|ty| ty.code), Some(201 | 203 | 205));
    let attributes = field_attributes(
        node,
        explicit_nullable,
        may_be_null,
        writable,
        fixed_length,
        long,
    )?;

    Ok(Some(Field {
        name: raw_attr_exact(raw_attrs, "rs:name")
            .cloned()
            .or_else(|| attr_rs(node, "name"))
            .unwrap_or_else(|| xml_name.clone()),
        xml_name,
        ordinal,
        ado_type,
        data_type,
        db_type,
        max_length,
        precision,
        scale,
        nullable,
        writable,
        fixed_length,
        long,
        key_column,
        base_catalog: attr_rs(node, "basecatalog"),
        base_schema: attr_rs(node, "baseschema"),
        base_table: attr_rs(node, "basetable"),
        base_column: attr_rs(node, "basecolumn"),
        chapter_fields: None,
        chapter_relation: None,
        attributes,
    }))
}

#[derive(Debug, Clone)]
struct XmlSchema {
    fields: Vec<Field>,
    chapters: Vec<XmlChapterSchema>,
}

#[derive(Debug, Clone)]
struct XmlChapterSchema {
    element_name: String,
    field_index: usize,
    schema: XmlSchema,
}

fn shaped_row_element_names(schema: &XmlSchema) -> HashSet<String> {
    let mut names = HashSet::from(["row".to_string()]);
    collect_shaped_row_element_names(schema, &mut names);
    names
}

fn collect_shaped_row_element_names(schema: &XmlSchema, names: &mut HashSet<String>) {
    for chapter in &schema.chapters {
        names.insert(chapter.element_name.clone());
        collect_shaped_row_element_names(&chapter.schema, names);
    }
}

fn parse_shaped_schema(
    schema_node: Node<'_, '_>,
    raw_field_attrs: &mut RawElementAttributes,
    limits: ResourceLimits,
) -> Result<Option<XmlSchema>> {
    let row_element = xml_row_schema_element(schema_node)?;
    let has_chapter = row_element
        .children()
        .any(|child| is_schema_element_named(child, "ElementType"));
    if !has_chapter {
        return Ok(None);
    }

    parse_element_schema(row_element, raw_field_attrs, limits).map(Some)
}

fn parse_element_schema(
    element: Node<'_, '_>,
    raw_field_attrs: &mut RawElementAttributes,
    limits: ResourceLimits,
) -> Result<XmlSchema> {
    parse_element_schema_at(element, raw_field_attrs, limits, 0)
}

fn parse_element_schema_at(
    element: Node<'_, '_>,
    raw_field_attrs: &mut RawElementAttributes,
    limits: ResourceLimits,
    depth: usize,
) -> Result<XmlSchema> {
    validate_xml_chapter_depth("ADO XML shaped schema", depth)?;
    let mut fields = Vec::new();
    let mut chapters = Vec::new();

    for (index, child) in element
        .children()
        .filter(|node| node.is_element())
        .enumerate()
    {
        if is_schema_element_named(child, "AttributeType") {
            let raw_attrs = raw_field_attrs.take_next()?;
            if let Some(field) = parse_attribute_field(index, child, raw_attrs)? {
                fields.push(field);
                limits.check_fields(fields.len(), "ADO XML shaped schema")?;
            }
            continue;
        }

        if is_schema_element_named(child, "ElementType") {
            let element_name =
                attr_unprefixed(child, "name").unwrap_or_else(|| "chapter".to_string());
            let chapter_relation = attr_rs(child, "relation")
                .map(|value| parse_chapter_relation_attr(&value))
                .transpose()
                .with_context(|| format!("invalid rs:relation for chapter {element_name}"))?;
            let child_schema = parse_element_schema_at(child, raw_field_attrs, limits, depth + 1)?;
            let field_index = fields.len();
            fields.push(chapter_field(
                &element_name,
                field_index,
                child_schema.fields.clone(),
                chapter_relation,
            ));
            limits.check_fields(fields.len(), "ADO XML shaped schema")?;
            chapters.push(XmlChapterSchema {
                element_name,
                field_index,
                schema: child_schema,
            });
        }
    }

    let element_name = attr_unprefixed(element, "name").unwrap_or_else(|| "row".to_string());
    validate_xml_schema_fields(&element_name, &fields)?;
    Ok(XmlSchema { fields, chapters })
}

fn chapter_field(
    name: &str,
    field_index: usize,
    chapter_fields: Vec<Field>,
    chapter_relation: Option<ChapterRelation>,
) -> Field {
    Field {
        name: name.to_string(),
        xml_name: name.to_string(),
        ordinal: Some(field_index + 1),
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
        chapter_relation,
        attributes: vec![FieldAttribute::Fixed, FieldAttribute::IsChapter],
    }
}

fn parse_chapter_relation_attr(value: &str) -> Result<ChapterRelation> {
    let value = value
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    if value.is_empty() || !value.len().is_multiple_of(24) {
        return Err(anyhow!(
            "rs:relation must contain one or more 12-byte hexadecimal relation records"
        ));
    }
    if !value.iter().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(anyhow!("rs:relation contained non-hexadecimal text"));
    }

    let mut pairs = Vec::new();
    for chunk in value.chunks_exact(24) {
        let parent_ordinal = parse_relation_u32_le(&chunk[0..8])? as usize;
        let child_ordinal = parse_relation_u32_le(&chunk[8..16])? as usize;
        if parent_ordinal == 0 || child_ordinal == 0 {
            return Err(anyhow!("rs:relation contained a zero field ordinal"));
        }
        pairs.push(ChapterRelationPair {
            parent_ordinal,
            child_ordinal,
        });
    }

    Ok(ChapterRelation { pairs })
}

fn parse_relation_u32_le(hex: &[u8]) -> Result<u32> {
    let mut bytes = [0u8; 4];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = parse_hex_byte(&hex[index * 2..index * 2 + 2])?;
    }
    Ok(u32::from_le_bytes(bytes))
}

fn parse_hex_byte(hex: &[u8]) -> Result<u8> {
    let text = std::str::from_utf8(hex).context("rs:relation byte was not valid UTF-8")?;
    u8::from_str_radix(text, 16).with_context(|| format!("invalid rs:relation byte {text:?}"))
}

fn row_element_attribute_order(row_element: Node<'_, '_>) -> Option<HashMap<String, usize>> {
    let mut order = HashMap::new();
    for child in row_element
        .children()
        .filter(|node| is_schema_element_named(*node, "attribute"))
    {
        let Some(field_type) = attr_unprefixed(child, "type") else {
            continue;
        };
        let xml_name = xml_name_key(local_xml_name(&field_type));
        let next_index = order.len();
        order.entry(xml_name).or_insert(next_index);
    }

    (!order.is_empty()).then_some(order)
}

fn xml_row_schema_element<'a, 'input>(schema_node: Node<'a, 'input>) -> Result<Node<'a, 'input>> {
    let mut row_elements = schema_node.descendants().filter(|node| {
        is_schema_element_named(*node, "ElementType")
            && !has_ignored_xml_schema_noise_ancestor(*node)
            && attr_unprefixed(*node, "name")
                .map(|name| name.eq_ignore_ascii_case("row"))
                .unwrap_or(false)
    });
    let Some(row_element) = row_elements.next() else {
        return Err(anyhow!("ADO XML row schema was not found"));
    };
    if row_elements.next().is_some() {
        return Err(anyhow!("ADO XML contained multiple row schemas"));
    }
    Ok(row_element)
}

fn node_is_or_descendant_of(node: Node<'_, '_>, ancestor: Node<'_, '_>) -> bool {
    node == ancestor || node.ancestors().any(|candidate| candidate == ancestor)
}

fn validate_xml_schema_fields(element_name: &str, fields: &[Field]) -> Result<()> {
    if fields.is_empty() {
        return Err(anyhow!(
            "ADO XML {element_name} schema had no visible fields"
        ));
    }

    let mut seen = HashSet::new();
    for field in fields {
        if !seen.insert(xml_name_key(&field.xml_name)) {
            return Err(anyhow!(
                "ADO XML {element_name} schema contained duplicate field XML name {}",
                field.xml_name
            ));
        }
    }

    Ok(())
}

fn parse_rows(
    doc: &Document<'_>,
    fields: &[Field],
    raw_row_attrs: &mut RawRowAttributes,
    limits: ResourceLimits,
) -> Result<(Vec<Row>, Vec<RowChange>)> {
    let mut rows = Vec::new();
    let mut changes = Vec::new();

    for data_node in xml_data_nodes(doc) {
        for child in data_node.children().filter(|node| node.is_element()) {
            parse_flat_data_child(
                child,
                true,
                fields,
                raw_row_attrs,
                &mut rows,
                &mut changes,
                limits,
            )?;
        }
    }

    Ok((rows, changes))
}

fn parse_flat_data_child<'a, 'input>(
    child: Node<'a, 'input>,
    _direct_data_child: bool,
    fields: &[Field],
    raw_row_attrs: &mut RawRowAttributes,
    rows: &mut Vec<Row>,
    changes: &mut Vec<RowChange>,
    limits: ResourceLimits,
) -> Result<()> {
    if is_element_named(child, "row") {
        if raw_row_attrs.row_node_disposition(child)? == RawRowDisposition::Accepted {
            let change_index = changes.len();
            let raw_attrs = raw_row_attrs.take_for_node(child)?;
            let row_index = push_row(
                rows,
                fields,
                child,
                &raw_attrs,
                RowState::Current,
                Some(change_index),
                limits,
            )?;
            changes.push(RowChange {
                kind: RowChangeKind::Current,
                row_indices: vec![row_index],
            });
        }
        return Ok(());
    }

    if is_rowset_state_wrapper_element_named(child, "insert") {
        let mut change_index = changes.len();
        let mut row_indices = Vec::new();
        let mut ctx = FlatInsertDeleteContext {
            fields,
            raw_row_attrs,
            rows,
            changes,
            row_indices: &mut row_indices,
            change_index: &mut change_index,
            limits,
        };
        for row_node in child.children().filter(|node| node.is_element()) {
            parse_flat_insert_delete_child(
                row_node,
                "insert",
                RowChangeKind::Insert,
                RowState::Inserted,
                &mut ctx,
            )?;
        }
        flush_insert_delete_change(RowChangeKind::Insert, ctx.row_indices, ctx.changes);
        return Ok(());
    }

    if is_rowset_state_wrapper_element_named(child, "delete") {
        let mut change_index = changes.len();
        let mut row_indices = Vec::new();
        let mut ctx = FlatInsertDeleteContext {
            fields,
            raw_row_attrs,
            rows,
            changes,
            row_indices: &mut row_indices,
            change_index: &mut change_index,
            limits,
        };
        for row_node in child.children().filter(|node| node.is_element()) {
            parse_flat_insert_delete_child(
                row_node,
                "delete",
                RowChangeKind::Delete,
                RowState::Deleted,
                &mut ctx,
            )?;
        }
        flush_insert_delete_change(RowChangeKind::Delete, ctx.row_indices, ctx.changes);
        return Ok(());
    }

    if is_rowset_state_wrapper_element_named(child, "update") {
        let change_index = changes.len();
        let mut original_rows = Vec::new();
        let mut updated_rows = Vec::new();
        for update_child in child.children().filter(|node| node.is_element()) {
            if is_rowset_state_wrapper_element_named(update_child, "original") {
                for row_node in update_child.children().filter(|node| node.is_element()) {
                    if !is_element_named(row_node, "row") {
                        return Err(unexpected_xml_child("original", row_node));
                    }
                    if raw_row_attrs.row_node_disposition(row_node)? != RawRowDisposition::Accepted
                    {
                        continue;
                    }
                    let raw_attrs = raw_row_attrs.take_for_node(row_node)?;
                    original_rows.push((row_node, raw_attrs));
                }
            } else if is_element_named(update_child, "row") {
                if raw_row_attrs.row_node_disposition(update_child)? != RawRowDisposition::Accepted
                {
                    continue;
                }
                let raw_attrs = raw_row_attrs.take_for_node(update_child)?;
                updated_rows.push((update_child, raw_attrs));
            } else {
                return Err(unexpected_xml_child("update", update_child));
            }
        }

        let original_count = original_rows.len();
        let updated_count = updated_rows.len();
        if original_count == 1 && updated_count == 1 {
            let mut row_indices = Vec::new();
            let Some((row_node, raw_attrs)) = original_rows.into_iter().next() else {
                return Err(anyhow!("ADO XML update original row was missing"));
            };
            row_indices.push(push_row(
                rows,
                fields,
                row_node,
                &raw_attrs,
                RowState::Original,
                Some(change_index),
                limits,
            )?);
            let Some((row_node, raw_attrs)) = updated_rows.into_iter().next() else {
                return Err(anyhow!("ADO XML update row was missing"));
            };
            row_indices.push(push_row(
                rows,
                fields,
                row_node,
                &raw_attrs,
                RowState::Updated,
                Some(change_index),
                limits,
            )?);
            changes.push(RowChange {
                kind: RowChangeKind::Update,
                row_indices,
            });
        } else if let Some((row_node, raw_attrs)) =
            malformed_update_current_row(original_rows, updated_rows)
        {
            let row_index = push_row(
                rows,
                fields,
                row_node,
                &raw_attrs,
                RowState::Current,
                Some(change_index),
                limits,
            )?;
            changes.push(RowChange {
                kind: RowChangeKind::Current,
                row_indices: vec![row_index],
            });
        }
        return Ok(());
    }

    for nested_child in child.children().filter(|node| node.is_element()) {
        parse_flat_data_child(
            nested_child,
            false,
            fields,
            raw_row_attrs,
            rows,
            changes,
            limits,
        )?;
    }
    Ok(())
}

struct FlatInsertDeleteContext<'a> {
    fields: &'a [Field],
    raw_row_attrs: &'a mut RawRowAttributes,
    rows: &'a mut Vec<Row>,
    changes: &'a mut Vec<RowChange>,
    row_indices: &'a mut Vec<usize>,
    change_index: &'a mut usize,
    limits: ResourceLimits,
}

fn parse_flat_insert_delete_child<'a, 'input>(
    child: Node<'a, 'input>,
    wrapper_name: &str,
    wrapper_kind: RowChangeKind,
    state: RowState,
    ctx: &mut FlatInsertDeleteContext<'_>,
) -> Result<()> {
    if is_element_named(child, "row") {
        if ctx.raw_row_attrs.row_node_disposition(child)? == RawRowDisposition::Accepted {
            let raw_attrs = ctx.raw_row_attrs.take_for_node(child)?;
            ctx.row_indices.push(push_row(
                ctx.rows,
                ctx.fields,
                child,
                &raw_attrs,
                state,
                Some(*ctx.change_index),
                ctx.limits,
            )?);
        }
        return Ok(());
    }

    if is_nested_rowset_insert_delete_wrapper_element(child) {
        flush_insert_delete_change(wrapper_kind, ctx.row_indices, ctx.changes);
        parse_flat_data_child(
            child,
            false,
            ctx.fields,
            ctx.raw_row_attrs,
            ctx.rows,
            ctx.changes,
            ctx.limits,
        )?;
        *ctx.change_index = ctx.changes.len();
        return Ok(());
    }

    if is_nested_rowset_update_wrapper_with_row_content(child) {
        flush_insert_delete_change(wrapper_kind, ctx.row_indices, ctx.changes);
        parse_flat_data_child(
            child,
            false,
            ctx.fields,
            ctx.raw_row_attrs,
            ctx.rows,
            ctx.changes,
            ctx.limits,
        )?;
        *ctx.change_index = ctx.changes.len();
        return Ok(());
    }

    if is_any_rowset_state_wrapper_element(child) {
        return Err(unexpected_xml_child(wrapper_name, child));
    }

    for nested_child in child.children().filter(|node| node.is_element()) {
        parse_flat_insert_delete_child(nested_child, wrapper_name, wrapper_kind, state, ctx)?;
    }
    Ok(())
}

fn flush_insert_delete_change(
    kind: RowChangeKind,
    row_indices: &mut Vec<usize>,
    changes: &mut Vec<RowChange>,
) {
    if !row_indices.is_empty() {
        changes.push(RowChange {
            kind,
            row_indices: std::mem::take(row_indices),
        });
    }
}

fn parse_rows_with_schema(
    doc: &Document<'_>,
    schema: &XmlSchema,
    raw_row_attrs: &mut RawRowAttributes,
    limits: ResourceLimits,
) -> Result<(Vec<Row>, Vec<RowChange>)> {
    let mut rows = Vec::new();
    let mut changes = Vec::new();

    for data_node in xml_data_nodes(doc) {
        for child in data_node.children().filter(|node| node.is_element()) {
            parse_shaped_data_child(
                child,
                true,
                schema,
                raw_row_attrs,
                &mut rows,
                &mut changes,
                limits,
            )?;
        }
    }

    Ok((rows, changes))
}

fn parse_shaped_data_child<'a, 'input>(
    child: Node<'a, 'input>,
    _direct_data_child: bool,
    schema: &XmlSchema,
    raw_row_attrs: &mut RawRowAttributes,
    rows: &mut Vec<Row>,
    changes: &mut Vec<RowChange>,
    limits: ResourceLimits,
) -> Result<()> {
    if is_element_named(child, "row") {
        if raw_row_attrs.row_node_disposition(child)? == RawRowDisposition::Accepted {
            let change_index = changes.len();
            let raw_attrs = raw_row_attrs.take_for_node(child)?;
            let row_index = push_row_with_schema(
                rows,
                schema,
                child,
                &raw_attrs,
                raw_row_attrs,
                RowState::Current,
                Some(change_index),
            )?;
            changes.push(RowChange {
                kind: RowChangeKind::Current,
                row_indices: vec![row_index],
            });
        }
        return Ok(());
    }

    if is_rowset_state_wrapper_element_named(child, "insert") {
        let mut change_index = changes.len();
        let mut row_indices = Vec::new();
        let mut ctx = ShapedInsertDeleteContext {
            schema,
            raw_row_attrs,
            rows,
            changes,
            row_indices: &mut row_indices,
            change_index: &mut change_index,
            limits,
        };
        for row_node in child.children().filter(|node| node.is_element()) {
            parse_shaped_insert_delete_child(
                row_node,
                "insert",
                RowChangeKind::Insert,
                RowState::Inserted,
                &mut ctx,
            )?;
        }
        flush_insert_delete_change(RowChangeKind::Insert, ctx.row_indices, ctx.changes);
        return Ok(());
    }

    if is_rowset_state_wrapper_element_named(child, "delete") {
        let mut change_index = changes.len();
        let mut row_indices = Vec::new();
        let mut ctx = ShapedInsertDeleteContext {
            schema,
            raw_row_attrs,
            rows,
            changes,
            row_indices: &mut row_indices,
            change_index: &mut change_index,
            limits,
        };
        for row_node in child.children().filter(|node| node.is_element()) {
            parse_shaped_insert_delete_child(
                row_node,
                "delete",
                RowChangeKind::Delete,
                RowState::Deleted,
                &mut ctx,
            )?;
        }
        flush_insert_delete_change(RowChangeKind::Delete, ctx.row_indices, ctx.changes);
        return Ok(());
    }

    if is_rowset_state_wrapper_element_named(child, "update") {
        let change_index = changes.len();
        let mut original_rows = Vec::new();
        let mut updated_rows = Vec::new();
        for update_child in child.children().filter(|node| node.is_element()) {
            if is_rowset_state_wrapper_element_named(update_child, "original") {
                for row_node in update_child.children().filter(|node| node.is_element()) {
                    if !is_element_named(row_node, "row") {
                        return Err(unexpected_xml_child("original", row_node));
                    }
                    if raw_row_attrs.row_node_disposition(row_node)? != RawRowDisposition::Accepted
                    {
                        continue;
                    }
                    let raw_attrs = raw_row_attrs.take_for_node(row_node)?;
                    original_rows.push((row_node, raw_attrs));
                }
            } else if is_element_named(update_child, "row") {
                if raw_row_attrs.row_node_disposition(update_child)? != RawRowDisposition::Accepted
                {
                    continue;
                }
                let raw_attrs = raw_row_attrs.take_for_node(update_child)?;
                updated_rows.push((update_child, raw_attrs));
            } else {
                return Err(unexpected_xml_child("update", update_child));
            }
        }

        let original_count = original_rows.len();
        let updated_count = updated_rows.len();
        if original_count == 1 && updated_count == 1 {
            let mut row_indices = Vec::new();
            let Some((row_node, raw_attrs)) = original_rows.into_iter().next() else {
                return Err(anyhow!("ADO XML update original row was missing"));
            };
            row_indices.push(push_row_with_schema(
                rows,
                schema,
                row_node,
                &raw_attrs,
                raw_row_attrs,
                RowState::Original,
                Some(change_index),
            )?);
            let Some((row_node, raw_attrs)) = updated_rows.into_iter().next() else {
                return Err(anyhow!("ADO XML update row was missing"));
            };
            row_indices.push(push_row_with_schema(
                rows,
                schema,
                row_node,
                &raw_attrs,
                raw_row_attrs,
                RowState::Updated,
                Some(change_index),
            )?);
            changes.push(RowChange {
                kind: RowChangeKind::Update,
                row_indices,
            });
        } else if let Some((row_node, raw_attrs)) =
            malformed_update_current_row(original_rows, updated_rows)
        {
            let row_index = push_row_with_schema(
                rows,
                schema,
                row_node,
                &raw_attrs,
                raw_row_attrs,
                RowState::Current,
                Some(change_index),
            )?;
            changes.push(RowChange {
                kind: RowChangeKind::Current,
                row_indices: vec![row_index],
            });
        }
        return Ok(());
    }

    for nested_child in child.children().filter(|node| node.is_element()) {
        parse_shaped_data_child(
            nested_child,
            false,
            schema,
            raw_row_attrs,
            rows,
            changes,
            limits,
        )?;
    }
    Ok(())
}

struct ShapedInsertDeleteContext<'a> {
    schema: &'a XmlSchema,
    raw_row_attrs: &'a mut RawRowAttributes,
    rows: &'a mut Vec<Row>,
    changes: &'a mut Vec<RowChange>,
    row_indices: &'a mut Vec<usize>,
    change_index: &'a mut usize,
    limits: ResourceLimits,
}

fn parse_shaped_insert_delete_child<'a, 'input>(
    child: Node<'a, 'input>,
    wrapper_name: &str,
    wrapper_kind: RowChangeKind,
    state: RowState,
    ctx: &mut ShapedInsertDeleteContext<'_>,
) -> Result<()> {
    if is_element_named(child, "row") {
        if ctx.raw_row_attrs.row_node_disposition(child)? == RawRowDisposition::Accepted {
            let raw_attrs = ctx.raw_row_attrs.take_for_node(child)?;
            ctx.row_indices.push(push_row_with_schema(
                ctx.rows,
                ctx.schema,
                child,
                &raw_attrs,
                ctx.raw_row_attrs,
                state,
                Some(*ctx.change_index),
            )?);
        }
        return Ok(());
    }

    if is_nested_rowset_insert_delete_wrapper_element(child) {
        flush_insert_delete_change(wrapper_kind, ctx.row_indices, ctx.changes);
        parse_shaped_data_child(
            child,
            false,
            ctx.schema,
            ctx.raw_row_attrs,
            ctx.rows,
            ctx.changes,
            ctx.limits,
        )?;
        *ctx.change_index = ctx.changes.len();
        return Ok(());
    }

    if is_nested_rowset_update_wrapper_with_row_content(child) {
        flush_insert_delete_change(wrapper_kind, ctx.row_indices, ctx.changes);
        parse_shaped_data_child(
            child,
            false,
            ctx.schema,
            ctx.raw_row_attrs,
            ctx.rows,
            ctx.changes,
            ctx.limits,
        )?;
        *ctx.change_index = ctx.changes.len();
        return Ok(());
    }

    if is_any_rowset_state_wrapper_element(child) {
        return Err(unexpected_xml_child(wrapper_name, child));
    }

    for nested_child in child.children().filter(|node| node.is_element()) {
        parse_shaped_insert_delete_child(nested_child, wrapper_name, wrapper_kind, state, ctx)?;
    }
    Ok(())
}

fn xml_data_nodes<'a, 'input>(doc: &'a Document<'input>) -> Vec<Node<'a, 'input>> {
    let root = doc.root_element();
    root.children()
        .filter(|node| is_rowset_element_named(*node, "data"))
        .collect()
}

fn malformed_update_current_row<'a, 'input>(
    original_rows: Vec<(Node<'a, 'input>, HashMap<String, String>)>,
    updated_rows: Vec<(Node<'a, 'input>, HashMap<String, String>)>,
) -> Option<(Node<'a, 'input>, HashMap<String, String>)> {
    let original_count = original_rows.len();
    let updated_count = updated_rows.len();

    if updated_count == 0 {
        original_rows.into_iter().next()
    } else if original_count == 0 {
        updated_rows.into_iter().next()
    } else {
        let mut original_rows = original_rows.into_iter();
        let (_, mut merged_attrs) = original_rows.next()?;
        let (row_node, updated_attrs) = updated_rows.into_iter().next_back()?;
        for (name, value) in updated_attrs {
            merged_attrs.insert(name, value);
        }
        Some((row_node, merged_attrs))
    }
}

fn unexpected_xml_child(parent: &str, node: Node<'_, '_>) -> anyhow::Error {
    anyhow!(
        "unexpected ADO XML {parent} child element {}",
        node.tag_name().name()
    )
}

fn push_row(
    rows: &mut Vec<Row>,
    fields: &[Field],
    node: Node<'_, '_>,
    raw_attrs: &HashMap<String, String>,
    state: RowState,
    change_index: Option<usize>,
    limits: ResourceLimits,
) -> Result<usize> {
    let force_null_fields = force_null_fields(node);
    validate_flat_xml_row_children(node)?;

    let mut values = Vec::with_capacity(fields.len());
    for field in fields {
        let force_null = force_null_matches_field(&force_null_fields, field);
        let value = match raw_row_attr(raw_attrs, field.xml_name.as_str()) {
            Some(raw) => parse_value(raw, field, limits)
                .with_context(|| format!("failed to parse field {}", field.name))?,
            None if force_null && field.nullable => Value::Null,
            None if force_null => return Err(force_null_non_nullable_error(field, state)),
            None if state == RowState::Updated => Value::Unavailable,
            None if field.nullable => Value::Null,
            None => {
                return Err(anyhow!(
                    "missing required XML field {} in {:?} row",
                    field.name,
                    state
                ));
            }
        };
        values.push(value);
    }

    let index = rows.len();
    rows.push(Row {
        ordinal: index,
        state,
        status_flags: inferred_status_flags(state),
        change_index,
        values,
    });
    limits.check_rows(rows.len(), "ADO XML rows")?;
    Ok(index)
}

fn push_row_with_schema(
    rows: &mut Vec<Row>,
    schema: &XmlSchema,
    node: Node<'_, '_>,
    raw_attrs: &HashMap<String, String>,
    raw_row_attrs: &mut RawRowAttributes,
    state: RowState,
    change_index: Option<usize>,
) -> Result<usize> {
    let limits = raw_row_attrs.limits;
    let mut ctx = ShapedRowBuildContext {
        rows,
        raw_row_attrs,
        limits,
    };
    push_row_with_schema_at(&mut ctx, schema, node, raw_attrs, state, change_index, 0)
}

struct ShapedRowBuildContext<'a> {
    rows: &'a mut Vec<Row>,
    raw_row_attrs: &'a mut RawRowAttributes,
    limits: ResourceLimits,
}

fn push_row_with_schema_at(
    ctx: &mut ShapedRowBuildContext<'_>,
    schema: &XmlSchema,
    node: Node<'_, '_>,
    raw_attrs: &HashMap<String, String>,
    state: RowState,
    change_index: Option<usize>,
    depth: usize,
) -> Result<usize> {
    validate_xml_chapter_depth("ADO XML shaped row materialization", depth)?;
    let force_null_fields = force_null_fields(node);
    validate_shaped_xml_row_children(schema, node, ctx.raw_row_attrs)?;

    let mut values = Vec::with_capacity(schema.fields.len());
    for (field_index, field) in schema.fields.iter().enumerate() {
        if let Some(chapter) = schema
            .chapters
            .iter()
            .find(|chapter| chapter.field_index == field_index)
        {
            values.push(Value::Chapter(Box::new(chapter_recordset(
                chapter,
                node,
                ctx.raw_row_attrs,
                ctx.limits,
                depth + 1,
            )?)));
            continue;
        }

        let force_null = force_null_matches_field(&force_null_fields, field);
        let value = match raw_row_attr(raw_attrs, field.xml_name.as_str()) {
            Some(raw) => parse_value(raw, field, ctx.limits)
                .with_context(|| format!("failed to parse field {}", field.name))?,
            None if force_null && field.nullable => Value::Null,
            None if force_null => return Err(force_null_non_nullable_error(field, state)),
            None if state == RowState::Updated => Value::Unavailable,
            None if field.nullable => Value::Null,
            None => {
                return Err(anyhow!(
                    "missing required XML field {} in {:?} row",
                    field.name,
                    state
                ));
            }
        };
        values.push(value);
    }

    let index = ctx.rows.len();
    ctx.rows.push(Row {
        ordinal: index,
        state,
        status_flags: inferred_status_flags(state),
        change_index,
        values,
    });
    ctx.limits.check_rows(ctx.rows.len(), "ADO XML rows")?;
    Ok(index)
}

fn validate_flat_xml_row_children(row: Node<'_, '_>) -> Result<()> {
    if let Some(child) = row.children().find(|node| node.is_element()) {
        return Err(unexpected_xml_node_child(row, child));
    }
    Ok(())
}

fn validate_shaped_xml_row_children(
    schema: &XmlSchema,
    row: Node<'_, '_>,
    raw_row_attrs: &RawRowAttributes,
) -> Result<()> {
    for child in row.children().filter(|node| node.is_element()) {
        if schema
            .chapters
            .iter()
            .any(|chapter| is_element_named(child, &chapter.element_name))
        {
            let _ = raw_row_attrs.shaped_chapter_node_disposition(child)?;
        } else {
            return Err(unexpected_xml_node_child(row, child));
        }
    }
    Ok(())
}

fn unexpected_xml_node_child(parent: Node<'_, '_>, child: Node<'_, '_>) -> anyhow::Error {
    anyhow!(
        "unexpected ADO XML {} child element {}",
        parent.tag_name().name(),
        child.tag_name().name()
    )
}

fn chapter_recordset(
    chapter: &XmlChapterSchema,
    parent_row: Node<'_, '_>,
    raw_row_attrs: &mut RawRowAttributes,
    limits: ResourceLimits,
    depth: usize,
) -> Result<Recordset> {
    validate_xml_chapter_depth("ADO XML child Recordset materialization", depth)?;
    let mut rows = Vec::new();
    let mut changes = Vec::new();

    for child_row in parent_row.children().filter(|node| node.is_element()) {
        if !is_element_named(child_row, &chapter.element_name) {
            continue;
        }
        if raw_row_attrs.shaped_chapter_node_disposition(child_row)? != RawRowDisposition::Accepted
        {
            continue;
        }

        let change_index = changes.len();
        let attrs = raw_row_attrs.take_for_node(child_row)?;
        let mut ctx = ShapedRowBuildContext {
            rows: &mut rows,
            raw_row_attrs,
            limits,
        };
        let row_index = push_row_with_schema_at(
            &mut ctx,
            &chapter.schema,
            child_row,
            &attrs,
            RowState::Current,
            Some(change_index),
            depth,
        )?;
        changes.push(RowChange {
            kind: RowChangeKind::Current,
            row_indices: vec![row_index],
        });
    }

    Ok(Recordset {
        fields: chapter.schema.fields.clone(),
        rows,
        changes,
    })
}

struct RawRowAttributes {
    rows: Vec<RawRowAttribute>,
    ignored_row_starts: HashSet<usize>,
    position_mapper: StructuralPositionMapper,
    limits: ResourceLimits,
}

struct RawRowAttribute {
    start: usize,
    element_name: String,
    attrs: HashMap<String, String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RawRowDisposition {
    Accepted,
    Ignored,
}

#[derive(Clone, Copy, Debug, Default)]
struct RawRowFrame {
    accepted: bool,
    ignored_shaped_chapter: bool,
}

#[derive(Clone, Debug)]
struct StructuralPositionMapper {
    checkpoints: Vec<(usize, usize)>,
}

#[derive(Default)]
struct RawNamespaceContext {
    default_namespace: Option<String>,
    prefixes: HashMap<String, String>,
    root_rowset_schema_prefixes: HashSet<String>,
}

#[derive(Default)]
struct RawNamespaceFrame {
    default_namespace: Option<Option<String>>,
    prefixes: Vec<(String, Option<String>)>,
}

struct RawElementAttributes {
    element_name: &'static str,
    elements: Vec<Vec<RawAttribute>>,
    next: usize,
}

#[derive(Debug, Clone)]
struct RawAttribute {
    name: String,
    value: String,
}

impl RawElementAttributes {
    fn parse(text: &str, element_name: &'static str, limits: ResourceLimits) -> Result<Self> {
        let mut elements = Vec::new();
        let mut namespaces = RawNamespaceContext::default();
        let mut namespace_frames = Vec::new();
        let mut ignored_schema_depth = 0usize;
        let mut offset = 0usize;
        while let Some(relative) = text[offset..].find('<') {
            let tag_start = offset + relative;
            if let Some(next_offset) = ignored_xml_markup_end(text, tag_start)? {
                offset = next_offset;
                continue;
            }
            if let Some((tag_name, tag_end)) = parse_end_tag(text, tag_start)? {
                if ignored_schema_depth > 0 && namespaces.tag_is_ignored_schema_noise(tag_name) {
                    ignored_schema_depth -= 1;
                }
                if let Some(frame) = namespace_frames.pop() {
                    namespaces.pop_declarations(frame);
                }
                offset = tag_end;
                continue;
            }
            let Some((tag_name, attrs_start, tag_end)) = parse_start_tag(text, tag_start)? else {
                offset = tag_start + 1;
                continue;
            };

            let raw_attrs_text = &text[attrs_start..tag_end];
            let raw_attrs = parse_raw_attribute_list(raw_attrs_text)?;
            let namespace_frame = namespaces.push_declarations(&raw_attrs);
            let self_closing = raw_start_tag_is_self_closing(raw_attrs_text);
            if ignored_schema_depth == 0 && namespaces.tag_is_schema_element(tag_name, element_name)
            {
                elements.push(raw_attrs);
                limits.check_fields(elements.len(), "ADO XML raw AttributeType scan")?;
            }
            let ignored_schema_element = namespaces.tag_is_ignored_schema_noise(tag_name);
            if ignored_schema_element && !self_closing {
                ignored_schema_depth += 1;
            }
            if self_closing {
                namespaces.pop_declarations(namespace_frame);
            } else {
                namespace_frames.push(namespace_frame);
            }
            offset = tag_end + 1;
        }

        Ok(Self {
            element_name,
            elements,
            next: 0,
        })
    }

    fn take_next(&mut self) -> Result<&[RawAttribute]> {
        let index = self.next;
        self.next += 1;
        self.elements.get(index).map(Vec::as_slice).ok_or_else(|| {
            anyhow!(
                "ADO XML {} element was missing raw attribute data",
                self.element_name
            )
        })
    }

    fn finish(&self) -> Result<()> {
        if self.next == self.elements.len() {
            Ok(())
        } else {
            Err(anyhow!(
                "ADO XML raw {} scan found {} elements but parsed {}",
                self.element_name,
                self.elements.len(),
                self.next
            ))
        }
    }
}

impl StructuralPositionMapper {
    fn new(text: &str) -> Self {
        let mut checkpoints = Vec::new();
        let mut structural_offset = 0usize;
        let mut original_offset = 0usize;

        for ch in text.chars() {
            let original_len = ch.len_utf8();
            let structural_len = if is_xml_char(ch) {
                original_len
            } else {
                '\u{fffd}'.len_utf8()
            };

            structural_offset += structural_len;
            original_offset += original_len;
            if structural_len != original_len {
                checkpoints.push((structural_offset, original_offset));
            }
        }

        Self { checkpoints }
    }

    fn original_offset(&self, structural_offset: usize) -> usize {
        let checkpoint_count = self
            .checkpoints
            .partition_point(|(checkpoint, _)| *checkpoint <= structural_offset);
        let (structural_base, original_base) = if checkpoint_count == 0 {
            (0, 0)
        } else {
            self.checkpoints[checkpoint_count - 1]
        };

        original_base + structural_offset.saturating_sub(structural_base)
    }
}

impl RawRowAttributes {
    fn parse(
        text: &str,
        element_names: &HashSet<String>,
        position_mapper: StructuralPositionMapper,
        root_rowset_schema_prefixes: HashSet<String>,
        limits: ResourceLimits,
    ) -> Result<Self> {
        let mut rows = Vec::new();
        let mut ignored_row_starts = HashSet::new();
        let mut namespaces =
            RawNamespaceContext::with_root_rowset_schema_prefixes(root_rowset_schema_prefixes);
        let mut namespace_frames = Vec::new();
        let mut row_frames: Vec<RawRowFrame> = Vec::new();
        let mut rowset_data_depth = 0usize;
        let mut rowset_wrapper_depth = 0usize;
        let mut data_relative_depth = 0usize;
        let mut accepted_row_depth = 0usize;
        let mut ignored_shaped_chapter_depth = 0usize;
        let mut offset = 0usize;
        while let Some(relative) = text[offset..].find('<') {
            let tag_start = offset + relative;
            if let Some(next_offset) = ignored_xml_markup_end(text, tag_start)? {
                offset = next_offset;
                continue;
            }
            if let Some((tag_name, tag_end)) = parse_end_tag(text, tag_start)? {
                if rowset_data_depth > 0 && namespaces.tag_is_rowset_element(tag_name, "data") {
                    if rowset_data_depth > 1 && data_relative_depth > 0 {
                        data_relative_depth -= 1;
                    }
                    rowset_data_depth -= 1;
                } else if rowset_wrapper_depth > 0
                    && namespaces.tag_is_rowset_wrapper_element(tag_name)
                {
                    rowset_wrapper_depth -= 1;
                    data_relative_depth = data_relative_depth.saturating_sub(1);
                } else if rowset_data_depth > 0 {
                    data_relative_depth = data_relative_depth.saturating_sub(1);
                }
                if let Some(row_frame) = row_frames.pop() {
                    if row_frame.accepted {
                        accepted_row_depth = accepted_row_depth.saturating_sub(1);
                    }
                    if row_frame.ignored_shaped_chapter {
                        ignored_shaped_chapter_depth =
                            ignored_shaped_chapter_depth.saturating_sub(1);
                    }
                }
                if let Some(frame) = namespace_frames.pop() {
                    namespaces.pop_declarations(frame);
                }
                offset = tag_end;
                continue;
            }
            let Some((tag_name, attrs_start, tag_end)) = parse_start_tag(text, tag_start)? else {
                offset = tag_start + 1;
                continue;
            };

            let raw_attrs_text = &text[attrs_start..tag_end];
            let raw_attrs = parse_raw_attribute_list(raw_attrs_text)?;
            let namespace_frame = namespaces.push_declarations(&raw_attrs);
            let self_closing = raw_start_tag_is_self_closing(raw_attrs_text);
            if namespaces.tag_is_rowset_element(tag_name, "data") {
                if rowset_data_depth > 0 && !self_closing {
                    data_relative_depth += 1;
                }
                if !self_closing {
                    rowset_data_depth += 1;
                    row_frames.push(RawRowFrame::default());
                    namespace_frames.push(namespace_frame);
                } else {
                    namespaces.pop_declarations(namespace_frame);
                }
                offset = tag_end + 1;
                continue;
            }
            if rowset_data_depth > 0 && namespaces.tag_is_rowset_wrapper_element(tag_name) {
                if !self_closing {
                    rowset_wrapper_depth += 1;
                    data_relative_depth += 1;
                    row_frames.push(RawRowFrame::default());
                    namespace_frames.push(namespace_frame);
                } else {
                    namespaces.pop_declarations(namespace_frame);
                }
                offset = tag_end + 1;
                continue;
            }

            let local_name = local_xml_name(tag_name);
            let mut row_frame = RawRowFrame::default();
            if rowset_data_depth > 0
                && ignored_shaped_chapter_depth == 0
                && contains_element_name(element_names, local_name)
            {
                let row_disposition = if local_name.eq_ignore_ascii_case("row") {
                    let accepted = if rowset_wrapper_depth > 0 {
                        namespaces.tag_is_wrapped_row_data_element(tag_name, "row")
                    } else if data_relative_depth == 0 {
                        namespaces.tag_is_row_data_element(tag_name, "row")
                    } else {
                        namespaces.tag_is_wrapped_row_data_element(tag_name, "row")
                    };
                    Some(if accepted {
                        RawRowDisposition::Accepted
                    } else {
                        RawRowDisposition::Ignored
                    })
                } else if accepted_row_depth > 0 {
                    Some(namespaces.shaped_chapter_row_disposition(tag_name))
                } else {
                    None
                };

                match row_disposition {
                    Some(RawRowDisposition::Accepted) => {
                        row_frame.accepted = true;
                        rows.push(RawRowAttribute {
                            start: tag_start,
                            element_name: local_name.to_string(),
                            attrs: raw_attributes_from_list(raw_attrs),
                        });
                        limits.check_rows(rows.len(), "ADO XML raw row scan")?;
                    }
                    Some(RawRowDisposition::Ignored) => {
                        row_frame.ignored_shaped_chapter = !local_name.eq_ignore_ascii_case("row");
                        ignored_row_starts.insert(tag_start);
                    }
                    None => {}
                }
            }
            if rowset_data_depth > 0 && !self_closing {
                data_relative_depth += 1;
            }
            if self_closing {
                namespaces.pop_declarations(namespace_frame);
            } else {
                if row_frame.accepted {
                    accepted_row_depth += 1;
                }
                if row_frame.ignored_shaped_chapter {
                    ignored_shaped_chapter_depth += 1;
                }
                row_frames.push(row_frame);
                namespace_frames.push(namespace_frame);
            }
            offset = tag_end + 1;
        }

        Ok(Self {
            rows,
            ignored_row_starts,
            position_mapper,
            limits,
        })
    }

    fn take_for_node(&mut self, node: Node<'_, '_>) -> Result<HashMap<String, String>> {
        let element_name = node.tag_name().name();
        let start = self.raw_start_for_node(node);
        let Some(index) = self.rows.iter().position(|row| {
            row.start == start && row.element_name.eq_ignore_ascii_case(element_name)
        }) else {
            return Err(anyhow!(
                "ADO XML {element_name} element was missing raw attribute data"
            ));
        };
        Ok(self.rows.remove(index).attrs)
    }

    fn shaped_chapter_node_disposition(&self, node: Node<'_, '_>) -> Result<RawRowDisposition> {
        let start = self.raw_start_for_node(node);
        if self.ignored_row_starts.contains(&start) {
            return Ok(RawRowDisposition::Ignored);
        }
        if self.rows.iter().any(|row| {
            row.start == start
                && row
                    .element_name
                    .eq_ignore_ascii_case(node.tag_name().name())
        }) {
            return Ok(RawRowDisposition::Accepted);
        }
        Err(anyhow!(
            "ADO XML {} element was missing raw attribute data",
            node.tag_name().name()
        ))
    }

    fn row_node_disposition(&self, node: Node<'_, '_>) -> Result<RawRowDisposition> {
        let start = self.raw_start_for_node(node);
        if self.ignored_row_starts.contains(&start) {
            return Ok(RawRowDisposition::Ignored);
        }
        if self
            .rows
            .iter()
            .any(|row| row.start == start && row.element_name.eq_ignore_ascii_case("row"))
        {
            return Ok(RawRowDisposition::Accepted);
        }
        Err(anyhow!(
            "ADO XML row element was missing raw attribute data"
        ))
    }

    fn raw_start_for_node(&self, node: Node<'_, '_>) -> usize {
        self.position_mapper.original_offset(node.range().start)
    }

    fn finish(&self) -> Result<()> {
        if self.rows.is_empty() {
            Ok(())
        } else {
            let names = self
                .rows
                .iter()
                .map(|row| row.element_name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            Err(anyhow!(
                "ADO XML raw row scan found unparsed row elements: {names}"
            ))
        }
    }
}

impl RawNamespaceContext {
    fn with_root_rowset_schema_prefixes(root_rowset_schema_prefixes: HashSet<String>) -> Self {
        Self {
            root_rowset_schema_prefixes,
            ..Self::default()
        }
    }

    fn push_declarations(&mut self, attrs: &[RawAttribute]) -> RawNamespaceFrame {
        let mut frame = RawNamespaceFrame::default();
        for attr in attrs {
            if attr.name == "xmlns" {
                frame
                    .default_namespace
                    .get_or_insert_with(|| self.default_namespace.clone());
                self.default_namespace = raw_namespace_value(&attr.value);
            } else if let Some(prefix) = attr.name.strip_prefix("xmlns:") {
                frame
                    .prefixes
                    .push((prefix.to_string(), self.prefixes.get(prefix).cloned()));
                if let Some(namespace) = raw_namespace_value(&attr.value) {
                    self.prefixes.insert(prefix.to_string(), namespace);
                } else {
                    self.prefixes.remove(prefix);
                }
            }
        }
        frame
    }

    fn pop_declarations(&mut self, frame: RawNamespaceFrame) {
        if let Some(previous) = frame.default_namespace {
            self.default_namespace = previous;
        }
        for (prefix, previous) in frame.prefixes.into_iter().rev() {
            if let Some(namespace) = previous {
                self.prefixes.insert(prefix, namespace);
            } else {
                self.prefixes.remove(&prefix);
            }
        }
    }

    fn tag_is_rowset_element(&self, tag_name: &str, local_name: &str) -> bool {
        local_xml_name(tag_name).eq_ignore_ascii_case(local_name)
            && self.tag_namespace(tag_name) == Some(ROWSET_NS)
    }

    fn tag_is_schema_element(&self, tag_name: &str, local_name: &str) -> bool {
        local_xml_name(tag_name).eq_ignore_ascii_case(local_name)
            && matches!(self.tag_namespace(tag_name), None | Some(SCHEMA_NS))
    }

    fn tag_is_ignored_schema_noise(&self, tag_name: &str) -> bool {
        let local_name = local_xml_name(tag_name);
        if matches!(
            local_name.to_ascii_lowercase().as_str(),
            "xml" | "schema" | "data" | "row"
        ) {
            return false;
        }
        if matches!(
            local_name.to_ascii_lowercase().as_str(),
            "elementtype" | "attributetype" | "attribute" | "extends" | "datatype"
        ) {
            return !matches!(self.tag_namespace(tag_name), None | Some(SCHEMA_NS));
        }
        true
    }

    fn tag_is_row_data_element(&self, tag_name: &str, local_name: &str) -> bool {
        if !local_xml_name(tag_name).eq_ignore_ascii_case(local_name) {
            return false;
        }
        if let Some(prefix) = raw_xml_name_prefix(tag_name) {
            return self.root_rowset_schema_prefixes.contains(prefix);
        }
        if self.root_rowset_schema_prefixes.is_empty() {
            true
        } else {
            matches!(self.tag_namespace(tag_name), None | Some(ROWSET_SCHEMA_NS))
        }
    }

    fn tag_is_wrapped_row_data_element(&self, tag_name: &str, local_name: &str) -> bool {
        if !local_xml_name(tag_name).eq_ignore_ascii_case(local_name) {
            return false;
        }
        if let Some(prefix) = raw_xml_name_prefix(tag_name) {
            return self.root_rowset_schema_prefixes.contains(prefix);
        }
        if self.root_rowset_schema_prefixes.is_empty() {
            return true;
        }
        self.tag_namespace(tag_name) == Some(ROWSET_SCHEMA_NS)
    }

    fn shaped_chapter_row_disposition(&self, tag_name: &str) -> RawRowDisposition {
        if raw_xml_name_prefix(tag_name).is_none() {
            RawRowDisposition::Accepted
        } else {
            RawRowDisposition::Ignored
        }
    }

    fn tag_is_rowset_wrapper_element(&self, tag_name: &str) -> bool {
        self.tag_namespace(tag_name) == Some(ROWSET_NS)
            && matches!(
                local_xml_name(tag_name).to_ascii_lowercase().as_str(),
                "insert" | "delete" | "update" | "original"
            )
    }

    fn tag_namespace<'a>(&'a self, tag_name: &str) -> Option<&'a str> {
        if let Some((prefix, _)) = tag_name.split_once(':') {
            self.prefixes.get(prefix).map(String::as_str)
        } else {
            self.default_namespace.as_deref()
        }
    }
}

fn contains_element_name(element_names: &HashSet<String>, name: &str) -> bool {
    element_names
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(name))
}

fn raw_namespace_value(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

fn ignored_xml_markup_end(text: &str, tag_start: usize) -> Result<Option<usize>> {
    let rest = &text[tag_start..];
    if rest.starts_with("<!--") {
        return text[tag_start + 4..]
            .find("-->")
            .map(|relative| Ok(Some(tag_start + 4 + relative + 3)))
            .unwrap_or_else(|| Err(anyhow!("unterminated XML comment")));
    }

    if rest.starts_with("<![CDATA[") {
        return text[tag_start + 9..]
            .find("]]>")
            .map(|relative| Ok(Some(tag_start + 9 + relative + 3)))
            .unwrap_or_else(|| Err(anyhow!("unterminated XML CDATA section")));
    }

    if rest.starts_with("<?") {
        return text[tag_start + 2..]
            .find("?>")
            .map(|relative| Ok(Some(tag_start + 2 + relative + 2)))
            .unwrap_or_else(|| Err(anyhow!("unterminated XML processing instruction")));
    }

    if rest.starts_with("<!") {
        return Ok(Some(xml_declaration_markup_end(text, tag_start)?));
    }

    Ok(None)
}

fn xml_declaration_markup_end(text: &str, tag_start: usize) -> Result<usize> {
    let bytes = text.as_bytes();
    let mut cursor = tag_start + 2;
    let mut quote = None;
    let mut bracket_depth = 0usize;
    while let Some(byte) = bytes.get(cursor).copied() {
        match (byte, quote) {
            (b'\'' | b'"', None) => quote = Some(byte),
            (value, Some(current)) if value == current => quote = None,
            (b'[', None) => bracket_depth += 1,
            (b']', None) => bracket_depth = bracket_depth.saturating_sub(1),
            (b'>', None) if bracket_depth == 0 => return Ok(cursor + 1),
            _ => {}
        }
        cursor += 1;
    }

    Err(anyhow!("unterminated XML declaration markup"))
}

fn parse_start_tag(text: &str, tag_start: usize) -> Result<Option<(&str, usize, usize)>> {
    let bytes = text.as_bytes();
    if bytes.get(tag_start) != Some(&b'<') {
        return Ok(None);
    }

    let mut cursor = tag_start + 1;
    if matches!(bytes.get(cursor), Some(b'/' | b'!' | b'?')) {
        return Ok(None);
    }

    while matches!(bytes.get(cursor), Some(byte) if byte.is_ascii_whitespace()) {
        cursor += 1;
    }
    let name_start = cursor;
    cursor = xml_name_end(text, cursor);
    if cursor == name_start {
        return Ok(None);
    }

    let tag_name = &text[name_start..cursor];
    let attrs_start = cursor;
    let mut quote = None;
    while let Some(byte) = bytes.get(cursor).copied() {
        match (byte, quote) {
            (b'\'' | b'"', None) => quote = Some(byte),
            (value, Some(current)) if value == current => quote = None,
            (b'>', None) => return Ok(Some((tag_name, attrs_start, cursor))),
            _ => {}
        }
        cursor += 1;
    }

    Err(anyhow!("unterminated XML start tag for {tag_name}"))
}

fn parse_end_tag(text: &str, tag_start: usize) -> Result<Option<(&str, usize)>> {
    let bytes = text.as_bytes();
    if bytes.get(tag_start) != Some(&b'<') || bytes.get(tag_start + 1) != Some(&b'/') {
        return Ok(None);
    }

    let mut cursor = tag_start + 2;
    while matches!(bytes.get(cursor), Some(byte) if byte.is_ascii_whitespace()) {
        cursor += 1;
    }
    let name_start = cursor;
    cursor = xml_name_end(text, cursor);
    if cursor == name_start {
        return Ok(None);
    }

    let tag_name = &text[name_start..cursor];
    while matches!(bytes.get(cursor), Some(byte) if byte.is_ascii_whitespace()) {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'>') {
        return Err(anyhow!("unterminated XML end tag for {tag_name}"));
    }
    Ok(Some((tag_name, cursor + 1)))
}

fn raw_start_tag_is_self_closing(raw_attrs_text: &str) -> bool {
    raw_attrs_text.trim_end().ends_with('/')
}

fn raw_attributes_from_list(attrs: Vec<RawAttribute>) -> HashMap<String, String> {
    attrs
        .into_iter()
        .filter(|attr| !attr.name.contains(':') && !attr.name.eq_ignore_ascii_case("xmlns"))
        .map(|attr| (attr.name.to_ascii_lowercase(), attr.value))
        .collect()
}

fn raw_row_attr<'a>(attrs: &'a HashMap<String, String>, xml_name: &str) -> Option<&'a String> {
    attrs.get(&xml_name.to_ascii_lowercase())
}

fn force_null_matches(force_null_fields: &HashSet<String>, field_name: &str) -> bool {
    force_null_fields
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(field_name))
}

fn force_null_matches_field(force_null_fields: &HashSet<String>, field: &Field) -> bool {
    force_null_matches(force_null_fields, field.xml_name.as_str())
}

fn force_null_non_nullable_error(field: &Field, state: RowState) -> anyhow::Error {
    anyhow!(
        "force-null XML field {} in {:?} row was not nullable",
        field.name,
        state
    )
}

fn parse_raw_attribute_list(text: &str) -> Result<Vec<RawAttribute>> {
    let bytes = text.as_bytes();
    let mut attrs = Vec::new();
    let mut cursor = 0usize;

    while cursor < bytes.len() {
        while matches!(bytes.get(cursor), Some(byte) if byte.is_ascii_whitespace() || *byte == b'/')
        {
            cursor += 1;
        }
        if cursor >= bytes.len() {
            break;
        }

        let name_start = cursor;
        cursor = xml_name_end(text, cursor);
        if cursor == name_start {
            break;
        }
        let raw_name = &text[name_start..cursor];
        while matches!(bytes.get(cursor), Some(byte) if byte.is_ascii_whitespace()) {
            cursor += 1;
        }
        if bytes.get(cursor) != Some(&b'=') {
            return Err(anyhow!("XML attribute {raw_name} was missing '='"));
        }
        cursor += 1;
        while matches!(bytes.get(cursor), Some(byte) if byte.is_ascii_whitespace()) {
            cursor += 1;
        }

        let quote = bytes
            .get(cursor)
            .copied()
            .ok_or_else(|| anyhow!("XML attribute {raw_name} was missing a value"))?;
        if quote != b'\'' && quote != b'"' {
            return Err(anyhow!("XML attribute {raw_name} value was not quoted"));
        }
        cursor += 1;
        let value_start = cursor;
        while !matches!(bytes.get(cursor), Some(value) if *value == quote) {
            if cursor >= bytes.len() {
                return Err(anyhow!("unterminated XML attribute {raw_name}"));
            }
            cursor += 1;
        }
        let raw_value = &text[value_start..cursor];
        cursor += 1;

        attrs.push(RawAttribute {
            name: raw_name.to_string(),
            value: decode_xml_attribute_raw(raw_value)?,
        });
    }

    Ok(attrs)
}

fn raw_attr_exact<'a>(attrs: &'a [RawAttribute], raw_name: &str) -> Option<&'a String> {
    attrs
        .iter()
        .find(|attr| attr.name.eq_ignore_ascii_case(raw_name))
        .map(|attr| &attr.value)
}

fn raw_attr_unprefixed<'a>(attrs: &'a [RawAttribute], local_name: &str) -> Option<&'a String> {
    attrs
        .iter()
        .find(|attr| !attr.name.contains(':') && attr.name.eq_ignore_ascii_case(local_name))
        .map(|attr| &attr.value)
}

fn decode_xml_attribute_raw(raw: &str) -> Result<String> {
    let mut out = String::new();
    let mut cursor = 0usize;
    while let Some(relative) = raw[cursor..].find('&') {
        let amp = cursor + relative;
        out.push_str(&raw[cursor..amp]);
        let semi = raw[amp + 1..]
            .find(';')
            .map(|index| amp + 1 + index)
            .ok_or_else(|| anyhow!("unterminated XML entity in attribute value"))?;
        let entity = &raw[amp + 1..semi];
        match entity {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "apos" => out.push('\''),
            "quot" => out.push('"'),
            _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                let code = u32::from_str_radix(&entity[2..], 16)
                    .with_context(|| format!("invalid XML hex entity &{entity};"))?;
                out.push(xml_char_from_entity(code, entity)?);
            }
            _ if entity.starts_with('#') => {
                let code = entity[1..]
                    .parse::<u32>()
                    .with_context(|| format!("invalid XML decimal entity &{entity};"))?;
                out.push(xml_char_from_entity(code, entity)?);
            }
            _ => return Err(anyhow!("unsupported XML entity &{entity};")),
        }
        cursor = semi + 1;
    }
    out.push_str(&raw[cursor..]);
    Ok(out)
}

fn xml_char_from_entity(code: u32, entity: &str) -> Result<char> {
    if !is_xml_char_code(code) {
        return Err(anyhow!("invalid XML character entity &{entity};"));
    }
    char::from_u32(code).ok_or_else(|| anyhow!("invalid XML character entity &{entity};"))
}

fn is_xml_char_code(code: u32) -> bool {
    matches!(code, 0x09 | 0x0A | 0x0D | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}

fn local_xml_name(name: &str) -> &str {
    name.rsplit_once(':')
        .map(|(_, local)| local)
        .unwrap_or(name)
}

fn raw_xml_name_prefix(name: &str) -> Option<&str> {
    name.split_once(':').map(|(prefix, _)| prefix)
}

fn xml_name_key(name: &str) -> String {
    name.to_ascii_lowercase()
}

fn xml_name_end(text: &str, start: usize) -> usize {
    let mut end = start;
    for (relative, ch) in text[start..].char_indices() {
        let absolute = start + relative;
        let valid = if absolute == start {
            is_xml_name_start_char(ch)
        } else {
            is_xml_name_char(ch)
        };
        if !valid {
            break;
        }
        end = absolute + ch.len_utf8();
    }
    end
}

fn is_xml_name_start_char(ch: char) -> bool {
    matches!(
        ch as u32,
        0x003A
            | 0x0041..=0x005A
            | 0x005F
            | 0x0061..=0x007A
            | 0x00C0..=0x00D6
            | 0x00D8..=0x00F6
            | 0x00F8..=0x02FF
            | 0x0370..=0x037D
            | 0x037F..=0x1FFF
            | 0x200C..=0x200D
            | 0x2070..=0x218F
            | 0x2C00..=0x2FEF
            | 0x3001..=0xD7FF
            | 0xF900..=0xFDCF
            | 0xFDF0..=0xFFFD
            | 0x10000..=0xEFFFF
    )
}

fn is_xml_name_char(ch: char) -> bool {
    is_xml_name_start_char(ch)
        || matches!(
            ch as u32,
            0x002D | 0x002E | 0x0030..=0x0039 | 0x00B7 | 0x0300..=0x036F | 0x203F..=0x2040
        )
}

fn parse_value(raw: &str, field: &Field, limits: ResourceLimits) -> Result<Value> {
    limits.check_value_bytes(raw.len(), &format!("XML field {}", field.name))?;
    let Some(data_type) = field.data_type.as_deref() else {
        return Ok(Value::String(raw.to_string()));
    };

    let normalized = data_type.to_ascii_lowercase();
    match normalized.as_str() {
        "bool" | "boolean" => parse_bool(raw)
            .map(Value::Boolean)
            .ok_or_else(|| anyhow!("invalid boolean value {raw:?}")),
        "i1" | "i2" | "i4" | "int" | "integer" | "i8" => {
            parse_signed_xml_integer(raw, normalized.as_str()).map(Value::Integer)
        }
        "ui1" | "ui2" | "ui4" | "ui8" => {
            parse_unsigned_xml_integer(raw, normalized.as_str()).map(Value::UnsignedInteger)
        }
        "r4" | "r8" | "float" => parse_float_value(raw, field),
        "number" if field_db_type_is(field, "numeric") => {
            xml_numeric_text(raw, field, limits).map(Value::Decimal)
        }
        "number" if field_db_type_is(field, "currency") => {
            xml_currency_text(raw, limits).map(Value::Decimal)
        }
        "number" if field_db_type_is(field, "decimal") => {
            canonical_decimal_text(raw, limits).map(Value::Decimal)
        }
        "number" if matches!(field.ado_type.map(|ty| ty.code), Some(139)) => {
            Ok(Value::Decimal(xml_varnumeric_text(raw, field, limits)?))
        }
        "number" => parse_finite_xml_float(raw, "numeric").map(Value::Float),
        "string" | "char" | "empty" | "entity" | "entities" | "enumeration" | "error"
        | "fixed.14.4" | "id" | "idref" | "idrefs" | "nmtoken" | "nmtokens" | "notation"
        | "time.tz" | "uri" => Ok(Value::String(xml_text_for_field(raw, field))),
        "decimal" => canonical_decimal_text(raw, limits).map(Value::Decimal),
        "currency" => xml_currency_text(raw, limits).map(Value::Decimal),
        "date" => parse_xml_date_value(raw).map(Value::Date),
        "time" => parse_xml_time_value(raw).map(Value::Time),
        "datetime" => parse_xml_datetime_typed_value(raw, field),
        // MDAC 2.8 treats dateTime.tz as Unicode text, preserving the timezone
        // suffix rather than converting it to an ADO date/time value.
        "datetime.tz" => Ok(Value::String(xml_text_for_field(raw, field))),
        "uuid" => parse_xml_uuid(raw).map(Value::Guid),
        "bin.hex" => {
            let compact = compact_xml_bin_hex(raw)?;
            limits.check_value_bytes(compact.len(), &format!("XML field {}", field.name))?;
            let mut bytes =
                hex::decode(&compact).with_context(|| format!("invalid bin.hex value {raw:?}"))?;
            truncate_xml_bytes_to_field_width(&mut bytes, field);
            normalize_ado_xml_binary_bytes(&mut bytes);
            let encoded = hex::encode_upper(bytes);
            limits.check_value_bytes(encoded.len(), &format!("XML field {}", field.name))?;
            Ok(Value::BinaryHex(encoded))
        }
        // MDAC 2.8 accepts dt:type="binary" but reopens it as Unicode text.
        "binary" => Ok(Value::String(xml_text_for_field(raw, field))),
        // MDAC 2.8 accepts dt:type="bin.base64" but reopens it as Unicode text,
        // preserving the original base64 string rather than decoding bytes.
        "bin.base64" => Ok(Value::String(xml_text_for_field(raw, field))),
        _ => Ok(Value::String(raw.to_string())),
    }
}

fn xml_text_for_field(raw: &str, field: &Field) -> String {
    if field.long {
        return raw.to_string();
    }
    let Some(max_length) = field.max_length else {
        return raw.to_string();
    };
    raw.chars().take(max_length).collect()
}

fn truncate_xml_bytes_to_field_width(bytes: &mut Vec<u8>, field: &Field) {
    if field.long {
        return;
    }
    if let Some(max_length) = field.max_length {
        bytes.truncate(max_length);
    }
}

fn compact_xml_bin_hex(raw: &str) -> Result<String> {
    let mut compact = String::with_capacity(raw.len());

    for ch in raw.chars() {
        if ch.is_ascii_hexdigit() {
            compact.push(ch);
        } else if is_xml_bin_hex_separator(ch) {
            if compact.len() % 2 == 1 {
                return Err(anyhow!("invalid bin.hex value {raw:?}"));
            }
        } else {
            return Err(anyhow!("invalid bin.hex value {raw:?}"));
        }
    }

    if compact.len() % 2 == 1 {
        return Err(anyhow!("invalid bin.hex value {raw:?}"));
    }

    Ok(compact)
}

fn is_xml_bin_hex_separator(ch: char) -> bool {
    ch.is_ascii() && !ch.is_ascii_alphanumeric()
}

fn normalize_ado_xml_binary_bytes(bytes: &mut [u8]) {
    for byte in bytes {
        *byte = match *byte {
            0x80 => 0xAC,
            0x82 => 0x1A,
            0x83 => 0x92,
            0x84 => 0x1E,
            0x85 => 0x26,
            0x86 => 0x20,
            0x87 => 0x21,
            0x88 => 0xC6,
            0x89 => 0x30,
            0x8A => 0x60,
            0x8B => 0x39,
            0x8C => 0x52,
            0x8E => 0x7D,
            0x91 => 0x18,
            0x92 => 0x19,
            0x93 => 0x1C,
            0x94 => 0x1D,
            0x95 => 0x22,
            0x96 => 0x13,
            0x97 => 0x14,
            0x98 => 0xDC,
            0x99 => 0x22,
            0x9A => 0x61,
            0x9B => 0x3A,
            0x9C => 0x53,
            0x9E => 0x7E,
            0x9F => 0x78,
            other => other,
        };
    }
}

fn parse_float_value(raw: &str, field: &Field) -> Result<Value> {
    if matches!(
        field
            .data_type
            .as_deref()
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("r4")
    ) {
        return parse_finite_xml_r4_float(raw).map(Value::Float);
    }

    if matches!(
        field
            .data_type
            .as_deref()
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("float" | "r8")
    ) && field.max_length == Some(4)
    {
        return Ok(Value::Float(0.0));
    }

    parse_finite_xml_float(raw, "float").map(Value::Float)
}

fn parse_finite_xml_r4_float(raw: &str) -> Result<f64> {
    let value = parse_xml_float_lexical(raw, "r4")? as f32;
    if !value.is_finite() {
        return Err(anyhow!("non-finite XML r4 value {raw:?}"));
    }
    Ok(value as f64)
}

fn parse_finite_xml_float(raw: &str, kind: &str) -> Result<f64> {
    let value = parse_xml_float_lexical(raw, kind)?;
    if !value.is_finite() {
        return Err(anyhow!("non-finite XML {kind} value {raw:?}"));
    }
    Ok(value)
}

fn parse_xml_float_lexical(raw: &str, kind: &str) -> Result<f64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("invalid {kind} value {raw:?}"));
    }

    if let Some(hex) = strip_ascii_prefix(trimmed, "&H") {
        return parse_prefixed_xml_integer(raw, hex, 16, kind).map(|value| value as f64);
    }
    if let Some(octal) = strip_ascii_prefix(trimmed, "&O") {
        return parse_prefixed_xml_integer(raw, octal, 8, kind).map(|value| value as f64);
    }

    let normalized = if trimmed.contains(',') {
        trimmed.chars().filter(|ch| *ch != ',').collect::<String>()
    } else {
        trimmed.to_string()
    };
    normalized
        .parse::<f64>()
        .with_context(|| format!("invalid {kind} value {raw:?}"))
}

fn parse_xml_uuid(raw: &str) -> Result<String> {
    let bytes = raw.as_bytes();
    let valid = bytes.len() == 38
        && bytes[0] == b'{'
        && bytes[37] == b'}'
        && [9, 14, 19, 24]
            .into_iter()
            .all(|index| bytes[index] == b'-')
        && bytes
            .iter()
            .enumerate()
            .filter(|(index, _)| !matches!(index, 0 | 9 | 14 | 19 | 24 | 37))
            .all(|(_, byte)| byte.is_ascii_hexdigit());

    if !valid {
        return Err(anyhow!("invalid XML uuid value {raw:?}"));
    }
    Ok(raw.to_ascii_uppercase())
}

fn parse_bool(raw: &str) -> Option<bool> {
    if raw.eq_ignore_ascii_case("true") {
        return Some(true);
    }
    if raw.eq_ignore_ascii_case("false") {
        return Some(false);
    }

    parse_numeric_xml_bool(raw)
}

fn parse_numeric_xml_bool(raw: &str) -> Option<bool> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(hex) = strip_ascii_prefix(trimmed, "&H") {
        return parse_radix_xml_bool(hex, 16);
    }
    if let Some(octal) = strip_ascii_prefix(trimmed, "&O") {
        return parse_radix_xml_bool(octal, 8);
    }

    let parsed = if trimmed.contains(',') {
        if trimmed.starts_with(',') {
            return None;
        }
        let without_group_separators: String = trimmed.chars().filter(|ch| *ch != ',').collect();
        without_group_separators.parse::<f64>().ok()?
    } else {
        trimmed.parse::<f64>().ok()?
    };
    if !parsed.is_finite() {
        return None;
    }
    Some(parsed != 0.0)
}

fn strip_ascii_prefix<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    let bytes = value.as_bytes();
    let prefix = prefix.as_bytes();
    if bytes.len() < prefix.len() {
        return None;
    }
    if bytes
        .iter()
        .zip(prefix)
        .all(|(actual, expected)| actual.eq_ignore_ascii_case(expected))
    {
        return Some(&value[prefix.len()..]);
    }
    None
}

fn parse_radix_xml_bool(raw: &str, radix: u32) -> Option<bool> {
    if raw.is_empty() {
        return None;
    }

    let mut nonzero = false;
    for ch in raw.chars() {
        let digit = ch.to_digit(radix)?;
        nonzero |= digit != 0;
    }
    Some(nonzero)
}

fn parse_signed_xml_integer(raw: &str, data_type: &str) -> Result<i64> {
    let value = if data_type == "i8" {
        parse_xml_i8_integer(raw)?
    } else {
        parse_xml_small_integer(raw, "integer")?
    };
    let (min, max) = match data_type {
        "i1" => (i8::MIN as i128, i8::MAX as i128),
        "i2" => (i16::MIN as i128, i16::MAX as i128),
        "i4" | "int" | "integer" => (i32::MIN as i128, i32::MAX as i128),
        "i8" => (i64::MIN as i128, i64::MAX as i128),
        _ => unreachable!("unexpected XML signed integer type {data_type}"),
    };
    if !(min..=max).contains(&value) {
        return Err(anyhow!(
            "XML {data_type} integer value {raw:?} is out of range"
        ));
    }
    Ok(value as i64)
}

fn parse_unsigned_xml_integer(raw: &str, data_type: &str) -> Result<u64> {
    if data_type == "ui8" {
        return parse_xml_ui8_integer(raw);
    }

    let value = parse_xml_small_integer(raw, "unsigned integer")?;
    let max = match data_type {
        "ui1" => u8::MAX as i128,
        "ui2" => u16::MAX as i128,
        "ui4" => u32::MAX as i128,
        _ => unreachable!("unexpected XML unsigned integer type {data_type}"),
    };
    if value < 0 || value > max {
        return Err(anyhow!(
            "XML {data_type} unsigned integer value {raw:?} is out of range"
        ));
    }
    Ok(value as u64)
}

fn parse_xml_small_integer(raw: &str, kind: &str) -> Result<i128> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("invalid {kind} value {raw:?}"));
    }

    if let Some(hex) = strip_ascii_prefix(trimmed, "&H") {
        return parse_prefixed_xml_integer(raw, hex, 16, kind);
    }
    if let Some(octal) = strip_ascii_prefix(trimmed, "&O") {
        return parse_prefixed_xml_integer(raw, octal, 8, kind);
    }

    let normalized = if trimmed.contains(',') {
        trimmed.chars().filter(|ch| *ch != ',').collect::<String>()
    } else {
        trimmed.to_string()
    };
    let value = normalized
        .parse::<f64>()
        .with_context(|| format!("invalid {kind} value {raw:?}"))?;
    if !value.is_finite() {
        return Err(anyhow!("invalid {kind} value {raw:?}"));
    }
    Ok(value.round_ties_even() as i128)
}

fn parse_prefixed_xml_integer(raw: &str, digits: &str, radix: u32, kind: &str) -> Result<i128> {
    if digits.is_empty() {
        return Err(anyhow!("invalid {kind} value {raw:?}"));
    }

    let mut value = 0i128;
    for ch in digits.chars() {
        let Some(digit) = ch.to_digit(radix) else {
            return Err(anyhow!("invalid {kind} value {raw:?}"));
        };
        value = value
            .checked_mul(i128::from(radix))
            .and_then(|value| value.checked_add(i128::from(digit)))
            .ok_or_else(|| anyhow!("invalid {kind} value {raw:?}"))?;
    }
    Ok(value)
}

fn parse_xml_i8_integer(raw: &str) -> Result<i128> {
    let (negative, magnitude) = parse_xml_i8_magnitude(raw)?;
    if negative {
        Ok(-(magnitude as i128))
    } else {
        Ok(magnitude as i128)
    }
}

fn parse_xml_ui8_integer(raw: &str) -> Result<u64> {
    let (negative, magnitude) = parse_xml_i8_magnitude(raw)
        .with_context(|| format!("invalid unsigned integer value {raw:?}"))?;
    if negative {
        return Err(anyhow!(
            "XML ui8 unsigned integer value {raw:?} is out of range"
        ));
    }
    Ok(magnitude)
}

fn parse_xml_i8_magnitude(raw: &str) -> Result<(bool, u64)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok((false, 0));
    }
    if trimmed.contains([',', 'e', 'E'])
        || strip_ascii_prefix(trimmed, "&H").is_some()
        || strip_ascii_prefix(trimmed, "&O").is_some()
    {
        return Err(anyhow!("invalid integer value {raw:?}"));
    }

    let (negative, body) = trimmed
        .strip_prefix('-')
        .map(|body| (true, body))
        .or_else(|| trimmed.strip_prefix('+').map(|body| (false, body)))
        .unwrap_or((false, trimmed));
    let (whole, fraction) = body.split_once('.').unwrap_or((body, ""));
    if whole.is_empty() && fraction.is_empty() {
        return Err(anyhow!("invalid integer value {raw:?}"));
    }
    if !whole.chars().all(|ch| ch.is_ascii_digit())
        || !fraction.chars().all(|ch| ch.is_ascii_digit())
    {
        return Err(anyhow!("invalid integer value {raw:?}"));
    }

    let whole = if whole.is_empty() { "0" } else { whole };
    let mut magnitude = whole
        .parse::<u64>()
        .with_context(|| format!("invalid integer value {raw:?}"))?;
    if fraction
        .as_bytes()
        .first()
        .map(|digit| *digit >= b'5')
        .unwrap_or(false)
    {
        magnitude = magnitude
            .checked_add(1)
            .ok_or_else(|| anyhow!("invalid integer value {raw:?}"))?;
    }
    Ok((negative, magnitude))
}

fn parse_xml_date_value(raw: &str) -> Result<String> {
    if raw.contains('T') {
        return Err(anyhow!("invalid XML date value {raw:?}"));
    }
    if raw.contains(':') {
        parse_xml_time_parts(raw, "date")?;
        return Ok("1899-12-30".to_string());
    }

    parse_xml_date_parts(raw, "date")?;
    Ok(raw.to_string())
}

fn parse_xml_time_value(raw: &str) -> Result<String> {
    if raw.contains('T') {
        return Err(anyhow!("invalid XML time value {raw:?}"));
    }
    if raw.contains(':') {
        parse_xml_time_parts(raw, "time")?;
        return Ok(raw.to_string());
    }

    parse_xml_time_date_parts(raw)?;
    Ok("00:00:00".to_string())
}

fn parse_xml_datetime_typed_value(raw: &str, field: &Field) -> Result<Value> {
    match field
        .db_type
        .as_deref()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("date") => parse_xml_datetime_dbdate_value(raw).map(Value::Date),
        Some("time") => parse_xml_datetime_dbtime_value(raw).map(Value::Time),
        _ => parse_xml_datetime_value(raw, field).map(Value::DateTime),
    }
}

fn parse_xml_datetime_dbdate_value(raw: &str) -> Result<String> {
    parse_xml_date_value(raw)
}

fn parse_xml_datetime_dbtime_value(raw: &str) -> Result<String> {
    parse_xml_time_value(raw)
}

fn parse_xml_datetime_value(raw: &str, field: &Field) -> Result<String> {
    let normalized = if let Some((date, time)) = raw.split_once('T') {
        let (year, _, _) = parse_xml_date_parts(date, "datetime")?;
        parse_xml_datetime_time_part(time, raw)?;
        if is_xml_filetime_field(field) && year < 1601 {
            return Err(anyhow!("XML filetime year {year} is out of range"));
        }
        raw.to_string()
    } else if raw.contains(':') {
        parse_xml_datetime_time_part(raw, raw)?;
        format!("1899-12-30T{raw}")
    } else {
        let (year, _, _) = parse_xml_date_parts(raw, "datetime")?;
        if is_xml_filetime_field(field) && year < 1601 {
            return Err(anyhow!("XML filetime year {year} is out of range"));
        }
        format!("{raw}T00:00:00")
    };

    Ok(datetime_text_for_field(&normalized, field))
}

fn parse_xml_date_parts(raw: &str, kind: &str) -> Result<(u16, u16, u16)> {
    let bytes = raw.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return Err(anyhow!("invalid XML {kind} value {raw:?}"));
    }

    let year = parse_fixed_xml_u16(bytes, 0, 4, kind, raw)?;
    let month = parse_fixed_xml_u16(bytes, 5, 2, kind, raw)?;
    let day = parse_fixed_xml_u16(bytes, 8, 2, kind, raw)?;
    validate_xml_date(year, month, day, kind)?;
    Ok((year, month, day))
}

fn parse_xml_time_date_parts(raw: &str) -> Result<(u16, u16, u16)> {
    let bytes = raw.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return Err(anyhow!("invalid XML time value {raw:?}"));
    }

    let year = parse_fixed_xml_u16(bytes, 0, 4, "time", raw)?;
    let month = parse_fixed_xml_u16(bytes, 5, 2, "time", raw)?;
    let day = parse_fixed_xml_u16(bytes, 8, 2, "time", raw)?;
    validate_xml_time_date(year, month, day)?;
    Ok((year, month, day))
}

fn parse_xml_time_parts(raw: &str, kind: &str) -> Result<(u16, u16, u16)> {
    let bytes = raw.as_bytes();
    if bytes.len() != 8 || bytes[2] != b':' || bytes[5] != b':' {
        return Err(anyhow!("invalid XML {kind} value {raw:?}"));
    }

    parse_xml_time_parts_from_bytes(bytes, kind, raw)
}

fn parse_xml_datetime_time_part(raw_time: &str, raw_datetime: &str) -> Result<(u16, u16, u16)> {
    let bytes = raw_time.as_bytes();
    if bytes.len() < 8 || bytes[2] != b':' || bytes[5] != b':' {
        return Err(anyhow!("invalid XML datetime value {raw_datetime:?}"));
    }
    if bytes.len() > 8 {
        if bytes[8] != b'.' {
            return Err(anyhow!("invalid XML datetime value {raw_datetime:?}"));
        }
        let fraction = &bytes[9..];
        if fraction.is_empty()
            || fraction.len() > 9
            || !fraction.iter().all(|byte| byte.is_ascii_digit())
        {
            return Err(anyhow!("invalid XML datetime fraction {raw_datetime:?}"));
        }
    }

    parse_xml_time_parts_from_bytes(bytes, "datetime", raw_datetime)
}

fn parse_xml_time_parts_from_bytes(bytes: &[u8], kind: &str, raw: &str) -> Result<(u16, u16, u16)> {
    let hour = parse_fixed_xml_u16(bytes, 0, 2, kind, raw)?;
    let minute = parse_fixed_xml_u16(bytes, 3, 2, kind, raw)?;
    let second = parse_fixed_xml_u16(bytes, 6, 2, kind, raw)?;
    validate_xml_time(hour, minute, second, kind)?;
    Ok((hour, minute, second))
}

fn parse_fixed_xml_u16(
    bytes: &[u8],
    offset: usize,
    len: usize,
    kind: &str,
    raw: &str,
) -> Result<u16> {
    let Some(slice) = bytes.get(offset..offset + len) else {
        return Err(anyhow!("invalid XML {kind} value {raw:?}"));
    };
    if !slice.iter().all(|byte| byte.is_ascii_digit()) {
        return Err(anyhow!("invalid XML {kind} value {raw:?}"));
    }

    let mut value = 0u16;
    for byte in slice {
        value = value * 10 + u16::from(byte - b'0');
    }
    Ok(value)
}

fn validate_xml_date(year: u16, month: u16, day: u16, kind: &str) -> Result<()> {
    if !(1..=9999).contains(&year) {
        return Err(anyhow!("invalid XML {kind} year {year}"));
    }
    validate_xml_date_month_day(year, month, day, kind)
}

fn validate_xml_time_date(year: u16, month: u16, day: u16) -> Result<()> {
    if year > 9999 {
        return Err(anyhow!("invalid XML time year {year}"));
    }
    validate_xml_date_month_day(year, month, day, "time")
}

fn validate_xml_date_month_day(year: u16, month: u16, day: u16, kind: &str) -> Result<()> {
    let Some(max_day) = gregorian_month_len(year, month) else {
        return Err(anyhow!("invalid XML {kind} month {month}"));
    };
    if !(1..=max_day).contains(&day) {
        return Err(anyhow!(
            "invalid XML {kind} day {day} for {year:04}-{month:02}"
        ));
    }
    Ok(())
}

fn validate_xml_time(hour: u16, minute: u16, second: u16, kind: &str) -> Result<()> {
    if hour > 23 || minute > 59 || second > 59 {
        if kind == "time" {
            return Err(anyhow!(
                "invalid XML time {hour:02}:{minute:02}:{second:02}"
            ));
        }
        return Err(anyhow!(
            "invalid XML {kind} time {hour:02}:{minute:02}:{second:02}"
        ));
    }
    Ok(())
}

fn canonical_decimal_text(raw: &str, limits: ResourceLimits) -> Result<String> {
    let parts = parse_xml_decimal_parts(raw, "decimal", limits)?;
    Ok(format_xml_decimal_parts(
        parts.negative,
        &parts.whole,
        &parts.fraction,
        false,
    ))
}

struct XmlDecimalParts {
    negative: bool,
    whole: String,
    fraction: String,
}

fn parse_xml_decimal_parts(
    raw: &str,
    kind: &str,
    limits: ResourceLimits,
) -> Result<XmlDecimalParts> {
    let trimmed = raw.trim();
    limits.check_value_bytes(trimmed.len(), &format!("XML {kind} value"))?;
    if trimmed.is_empty() {
        return Err(anyhow!("invalid XML {kind} value {raw:?}"));
    }
    let allow_variant_numeric_forms = matches!(kind, "decimal" | "currency");
    if allow_variant_numeric_forms {
        if let Some(hex) = strip_ascii_prefix(trimmed, "&H") {
            let value = parse_prefixed_xml_integer(raw, hex, 16, kind)?;
            return Ok(XmlDecimalParts {
                negative: false,
                whole: value.to_string(),
                fraction: String::new(),
            });
        }
        if let Some(octal) = strip_ascii_prefix(trimmed, "&O") {
            let value = parse_prefixed_xml_integer(raw, octal, 8, kind)?;
            return Ok(XmlDecimalParts {
                negative: false,
                whole: value.to_string(),
                fraction: String::new(),
            });
        }
    }

    let normalized;
    let text = if allow_variant_numeric_forms && trimmed.contains(',') {
        normalized = trimmed.chars().filter(|ch| *ch != ',').collect::<String>();
        normalized.as_str()
    } else {
        trimmed
    };

    let (negative, body) = text
        .strip_prefix('-')
        .map(|body| (true, body))
        .or_else(|| text.strip_prefix('+').map(|body| (false, body)))
        .unwrap_or((false, text));
    let (body, exponent) = parse_xml_decimal_exponent(body, raw, kind)?;

    let mut saw_dot = false;
    let mut saw_digit = false;
    for ch in body.chars() {
        match ch {
            '.' if !saw_dot => saw_dot = true,
            '.' => return Err(anyhow!("invalid XML {kind} value {raw:?}")),
            _ if ch.is_ascii_digit() => saw_digit = true,
            _ => return Err(anyhow!("invalid XML {kind} value {raw:?}")),
        }
    }
    if !saw_digit {
        return Err(anyhow!("invalid XML {kind} value {raw:?}"));
    }

    let (whole, fraction) = body.split_once('.').unwrap_or((body, ""));
    let (whole, fraction) =
        apply_xml_decimal_exponent(whole, fraction, exponent, raw, kind, limits)?;
    Ok(XmlDecimalParts {
        negative,
        whole,
        fraction,
    })
}

fn parse_xml_decimal_exponent<'a>(body: &'a str, raw: &str, kind: &str) -> Result<(&'a str, i32)> {
    let mut parts = body.split(['e', 'E']);
    let significand = parts.next().unwrap_or_default();
    let Some(exponent) = parts.next() else {
        return Ok((significand, 0));
    };
    if parts.next().is_some() || exponent.is_empty() {
        return Err(anyhow!("invalid XML {kind} value {raw:?}"));
    }
    let exponent = exponent
        .parse::<i32>()
        .with_context(|| format!("invalid XML {kind} value {raw:?}"))?;
    Ok((significand, exponent))
}

fn apply_xml_decimal_exponent(
    whole: &str,
    fraction: &str,
    exponent: i32,
    raw: &str,
    kind: &str,
    limits: ResourceLimits,
) -> Result<(String, String)> {
    let digits_len = whole
        .len()
        .checked_add(fraction.len())
        .ok_or_else(|| anyhow!("invalid XML {kind} value {raw:?}"))?;
    if exponent == 0 {
        let expanded_len = if fraction.is_empty() {
            digits_len
        } else {
            digits_len
                .checked_add(1)
                .ok_or_else(|| anyhow!("invalid XML {kind} value {raw:?}"))?
        };
        check_xml_decimal_expanded_len(expanded_len, raw, kind, limits)?;
        return Ok((whole.to_string(), fraction.to_string()));
    }

    let decimal_pos = i64::try_from(whole.len())
        .ok()
        .and_then(|whole_len| whole_len.checked_add(i64::from(exponent)))
        .ok_or_else(|| anyhow!("invalid XML {kind} value {raw:?}"))?;
    if decimal_pos <= 0 {
        let zeros_len = usize::try_from(-decimal_pos)
            .map_err(|_| anyhow!("invalid XML {kind} value {raw:?}"))?;
        let expanded_len = 2usize
            .checked_add(zeros_len)
            .and_then(|len| len.checked_add(digits_len))
            .ok_or_else(|| anyhow!("invalid XML {kind} value {raw:?}"))?;
        check_xml_decimal_expanded_len(expanded_len, raw, kind, limits)?;
        let digits = format!("{whole}{fraction}");
        let zeros = "0".repeat(zeros_len);
        return Ok(("0".to_string(), format!("{zeros}{digits}")));
    }

    let decimal_pos =
        usize::try_from(decimal_pos).map_err(|_| anyhow!("invalid XML {kind} value {raw:?}"))?;
    if decimal_pos >= digits_len {
        let zeros_len = decimal_pos - digits_len;
        let expanded_len = digits_len
            .checked_add(zeros_len)
            .ok_or_else(|| anyhow!("invalid XML {kind} value {raw:?}"))?;
        check_xml_decimal_expanded_len(expanded_len, raw, kind, limits)?;
        let digits = format!("{whole}{fraction}");
        let zeros = "0".repeat(zeros_len);
        return Ok((format!("{digits}{zeros}"), String::new()));
    }

    let expanded_len = digits_len
        .checked_add(1)
        .ok_or_else(|| anyhow!("invalid XML {kind} value {raw:?}"))?;
    check_xml_decimal_expanded_len(expanded_len, raw, kind, limits)?;
    let digits = format!("{whole}{fraction}");
    Ok((
        digits[..decimal_pos].to_string(),
        digits[decimal_pos..].to_string(),
    ))
}

fn check_xml_decimal_expanded_len(
    len: usize,
    raw: &str,
    kind: &str,
    limits: ResourceLimits,
) -> Result<()> {
    if len > limits.max_xml_decimal_expanded_len {
        return Err(anyhow!(
            "XML {kind} value {raw:?} expanded to length {len}, exceeding maximum decimal expansion length {}",
            limits.max_xml_decimal_expanded_len
        ));
    }
    limits.check_value_bytes(len, &format!("XML {kind} value"))?;
    Ok(())
}

fn xml_numeric_text(raw: &str, field: &Field, limits: ResourceLimits) -> Result<String> {
    let parts = parse_xml_decimal_parts(raw, "numeric", limits)?;
    let (Some(precision), Some(scale)) = (field.precision, field.scale) else {
        return Ok(format_xml_decimal_parts(
            parts.negative,
            &parts.whole,
            &parts.fraction,
            false,
        ));
    };
    if precision == 0 {
        return Err(anyhow!(
            "invalid XML numeric descriptor precision 0 for field {}",
            field.name
        ));
    }
    if scale < 0 {
        return Err(anyhow!(
            "invalid XML numeric descriptor scale {scale} for field {}",
            field.name
        ));
    }
    let scale = scale as usize;
    if scale > precision {
        return Err(anyhow!(
            "invalid XML numeric descriptor scale {scale} exceeds precision {precision} for field {}",
            field.name
        ));
    }

    let kept_fraction = if parts.fraction.len() > scale {
        &parts.fraction[..scale]
    } else {
        &parts.fraction
    };
    let precision_digits = numeric_precision_digits(&parts.whole, kept_fraction);
    if precision_digits > precision {
        return Err(anyhow!(
            "XML numeric value {raw:?} exceeds precision {precision} for field {}",
            field.name
        ));
    }

    Ok(format_xml_decimal_parts(
        parts.negative,
        &parts.whole,
        kept_fraction,
        true,
    ))
}

fn xml_currency_text(raw: &str, limits: ResourceLimits) -> Result<String> {
    let parts = parse_xml_decimal_parts(raw, "currency", limits)?;
    let mut scaled = xml_currency_scaled_abs(&parts)?;
    if xml_currency_should_round_up(&parts.fraction) {
        scaled = scaled
            .checked_add(1)
            .ok_or_else(|| anyhow!("XML currency value {raw:?} is out of range"))?;
    }

    let max = if parts.negative {
        (i64::MAX as u128) + 1
    } else {
        i64::MAX as u128
    };
    if scaled > max {
        return Err(anyhow!("XML currency value {raw:?} is out of range"));
    }

    Ok(format_scaled_u128_decimal(scaled, 4, parts.negative))
}

fn xml_currency_scaled_abs(parts: &XmlDecimalParts) -> Result<u128> {
    let whole = parts.whole.trim_start_matches('0');
    let whole = if whole.is_empty() { "0" } else { whole };
    let whole = whole
        .parse::<u128>()
        .context("invalid XML currency integer magnitude")?;
    let mut scaled = whole
        .checked_mul(10_000)
        .ok_or_else(|| anyhow!("XML currency integer magnitude is out of range"))?;

    let mut fraction = 0u128;
    let mut digits = 0usize;
    for byte in parts.fraction.bytes().take(4) {
        fraction = fraction * 10 + u128::from(byte - b'0');
        digits += 1;
    }
    for _ in digits..4 {
        fraction *= 10;
    }
    scaled = scaled
        .checked_add(fraction)
        .ok_or_else(|| anyhow!("XML currency value is out of range"))?;
    Ok(scaled)
}

fn xml_currency_should_round_up(fraction: &str) -> bool {
    let bytes = fraction.as_bytes();
    if bytes.len() <= 4 {
        return false;
    }
    match bytes[4] {
        b'0'..=b'4' => false,
        b'6'..=b'9' => true,
        b'5' => {
            bytes[5..].iter().any(|byte| *byte != b'0') || xml_currency_kept_digit_is_odd(bytes)
        }
        _ => unreachable!("currency fraction was already digit-validated"),
    }
}

fn xml_currency_kept_digit_is_odd(fraction: &[u8]) -> bool {
    let kept_digit = fraction.get(3).copied().unwrap_or(b'0');
    (kept_digit - b'0') % 2 == 1
}

fn numeric_precision_digits(whole: &str, fraction: &str) -> usize {
    let mut digits = String::with_capacity(whole.len() + fraction.len());
    digits.push_str(whole);
    digits.push_str(fraction);
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        1
    } else {
        digits.len()
    }
}

fn format_xml_decimal_parts(
    negative: bool,
    whole: &str,
    fraction: &str,
    trim_leading_whole_zeros: bool,
) -> String {
    let whole = if trim_leading_whole_zeros {
        let trimmed = whole.trim_start_matches('0');
        if trimmed.is_empty() {
            "0"
        } else {
            trimmed
        }
    } else if whole.is_empty() {
        "0"
    } else {
        whole
    };
    let fraction = fraction.trim_end_matches('0');
    let is_zero = whole.chars().all(|ch| ch == '0') && fraction.is_empty();

    if fraction.is_empty() {
        if negative && !is_zero {
            format!("-{whole}")
        } else {
            whole.to_string()
        }
    } else if negative {
        format!("-{whole}.{fraction}")
    } else {
        format!("{whole}.{fraction}")
    }
}

struct XmlVarNumeric {
    negative: bool,
    scale: i32,
    magnitude: u128,
}

fn xml_varnumeric_text(raw: &str, field: &Field, limits: ResourceLimits) -> Result<String> {
    let value = parse_xml_varnumeric(raw, limits)?;
    let magnitude_len = varnumeric_magnitude_len(value.magnitude);
    let payload_len = 3usize + magnitude_len;
    let Some(max_length) = field.max_length else {
        return Err(anyhow!(
            "MDAC XML varnumeric value has no bounded dt:maxLength"
        ));
    };

    if payload_len <= max_length {
        return Ok(format_scaled_u128_signed(
            value.magnitude,
            value.scale,
            value.negative,
        ));
    }

    if max_length >= 3 && payload_len == max_length + 1 {
        let kept_magnitude_bytes = max_length - 3;
        let mut magnitude = 0u128;
        for index in 0..kept_magnitude_bytes {
            let byte = (value.magnitude >> (index * 8)) & 0xff;
            magnitude |= byte << (index * 8);
        }
        magnitude |= (payload_len as u128) << (kept_magnitude_bytes * 8);
        return Ok(format_scaled_u128_signed(
            magnitude,
            value.scale,
            value.negative,
        ));
    }

    Err(anyhow!(
        "MDAC XML varnumeric payload length {payload_len} exceeds dt:maxLength {max_length}"
    ))
}

fn parse_xml_varnumeric(raw: &str, limits: ResourceLimits) -> Result<XmlVarNumeric> {
    let parts = parse_xml_decimal_parts(raw, "varnumeric", limits)?;
    let (digits, scale) = if !parts.fraction.is_empty() {
        let whole = parts.whole.as_str();
        let fraction = parts.fraction.as_str();
        let fraction = fraction.trim_end_matches('0');
        let scale = fraction.len() as i32;
        (format!("{whole}{fraction}"), scale)
    } else {
        let mut digits = parts.whole.clone();
        let mut negative_scale = 0i32;
        while digits.len() > 1 && digits.ends_with('0') {
            digits.pop();
            negative_scale -= 1;
        }
        (digits, negative_scale)
    };

    let digits = digits.trim_start_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    if !digits.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(anyhow!("invalid XML varnumeric value {raw:?}"));
    }

    let magnitude = digits
        .parse::<u128>()
        .with_context(|| format!("invalid XML varnumeric value {raw:?}"))?;
    Ok(XmlVarNumeric {
        negative: parts.negative && magnitude != 0,
        scale,
        magnitude,
    })
}

fn varnumeric_magnitude_len(magnitude: u128) -> usize {
    let mut value = magnitude;
    let mut len = 1usize;
    while value > 0xff {
        value >>= 8;
        len += 1;
    }
    len
}

fn format_scaled_u128_signed(magnitude: u128, scale: i32, negative: bool) -> String {
    if scale < 0 {
        let zeros = "0".repeat((-scale) as usize);
        if negative && magnitude != 0 {
            format!("-{magnitude}{zeros}")
        } else {
            format!("{magnitude}{zeros}")
        }
    } else {
        format_scaled_u128_decimal(magnitude, scale as usize, negative)
    }
}

fn format_scaled_u128_decimal(magnitude: u128, scale: usize, negative: bool) -> String {
    let mut digits = magnitude.to_string();
    if scale > 0 {
        if digits.len() <= scale {
            let zeros = "0".repeat(scale + 1 - digits.len());
            digits = format!("{zeros}{digits}");
        }
        let split = digits.len() - scale;
        digits.insert(split, '.');
        while digits.ends_with('0') {
            digits.pop();
        }
        if digits.ends_with('.') {
            digits.pop();
        }
    }

    if negative && magnitude != 0 {
        format!("-{digits}")
    } else {
        digits
    }
}

fn datetime_text_for_field(raw: &str, field: &Field) -> String {
    if is_xml_filetime_field(field) {
        return raw
            .split_once('.')
            .map(|(head, _)| head.to_string())
            .unwrap_or_else(|| raw.to_string());
    }

    raw.to_string()
}

fn is_xml_filetime_field(field: &Field) -> bool {
    field_db_type_is(field, "filetime")
}

fn field_db_type_is(field: &Field, expected: &str) -> bool {
    field
        .db_type
        .as_deref()
        .is_some_and(|db_type| db_type.eq_ignore_ascii_case(expected))
}

fn datatype_node<'a, 'input>(node: Node<'a, 'input>) -> Option<Node<'a, 'input>> {
    node.children()
        .find(|child| is_schema_element_named(*child, "datatype"))
}

fn is_schema_element_named(node: Node<'_, '_>, local_name: &str) -> bool {
    node.is_element()
        && node.tag_name().name().eq_ignore_ascii_case(local_name)
        && matches!(node.tag_name().namespace(), None | Some(SCHEMA_NS))
}

fn is_ignored_xml_schema_noise_element(node: Node<'_, '_>) -> bool {
    if !node.is_element() {
        return false;
    }
    if ["xml", "Schema", "data", "row"]
        .iter()
        .any(|local_name| node.tag_name().name().eq_ignore_ascii_case(local_name))
    {
        return false;
    }
    if [
        "ElementType",
        "AttributeType",
        "attribute",
        "extends",
        "datatype",
    ]
    .iter()
    .any(|local_name| node.tag_name().name().eq_ignore_ascii_case(local_name))
    {
        return !matches!(node.tag_name().namespace(), None | Some(SCHEMA_NS));
    }
    true
}

fn has_ignored_xml_schema_noise_ancestor(node: Node<'_, '_>) -> bool {
    node.ancestors()
        .skip(1)
        .any(is_ignored_xml_schema_noise_element)
}

fn is_wrong_namespace_schema_element_named(node: Node<'_, '_>, local_name: &str) -> bool {
    node.is_element()
        && node.tag_name().name().eq_ignore_ascii_case(local_name)
        && node
            .tag_name()
            .namespace()
            .is_some_and(|namespace| namespace != SCHEMA_NS)
}

fn is_rowset_element_named(node: Node<'_, '_>, local_name: &str) -> bool {
    node.is_element()
        && node.tag_name().name().eq_ignore_ascii_case(local_name)
        && node.tag_name().namespace() == Some(ROWSET_NS)
}

fn is_rowset_state_wrapper_element_named(node: Node<'_, '_>, local_name: &str) -> bool {
    is_rowset_element_named(node, local_name)
}

fn is_any_rowset_state_wrapper_element(node: Node<'_, '_>) -> bool {
    ["insert", "delete", "update", "original"]
        .iter()
        .any(|local_name| is_rowset_state_wrapper_element_named(node, local_name))
}

fn is_nested_rowset_insert_delete_wrapper_element(node: Node<'_, '_>) -> bool {
    ["insert", "delete"]
        .iter()
        .any(|local_name| is_rowset_state_wrapper_element_named(node, local_name))
}

fn is_nested_rowset_update_wrapper_with_row_content(node: Node<'_, '_>) -> bool {
    is_rowset_state_wrapper_element_named(node, "update")
        && node
            .children()
            .filter(|child| child.is_element())
            .any(|child| {
                is_element_named(child, "row")
                    || (is_rowset_state_wrapper_element_named(child, "original")
                        && child
                            .children()
                            .any(|original_child| is_element_named(original_child, "row")))
            })
}

fn is_element_named(node: Node<'_, '_>, local_name: &str) -> bool {
    node.is_element() && node.tag_name().name().eq_ignore_ascii_case(local_name)
}

fn attr_unprefixed(node: Node<'_, '_>, local_name: &str) -> Option<String> {
    node.attributes()
        .find(|attr| attr.namespace().is_none() && attr.name().eq_ignore_ascii_case(local_name))
        .map(|attr| attr.value().to_string())
}

fn attr_dt(node: Node<'_, '_>, local_name: &str) -> Option<String> {
    attr_ns(node, local_name, DATATYPE_NS)
}

fn attr_rs(node: Node<'_, '_>, local_name: &str) -> Option<String> {
    attr_ns(node, local_name, ROWSET_NS)
}

fn attr_ns(node: Node<'_, '_>, local_name: &str, namespace: &str) -> Option<String> {
    node.attributes()
        .find(|attr| {
            attr.name().eq_ignore_ascii_case(local_name) && attr.namespace() == Some(namespace)
        })
        .map(|attr| attr.value().to_string())
}

fn bool_attr_rs(node: Node<'_, '_>, local_name: &str) -> Result<bool> {
    attr_rs(node, local_name)
        .map(|value| parse_xml_bool_attr(&value, local_name))
        .transpose()
        .map(|value| value.unwrap_or(false))
}

fn parse_xml_bool_attr(value: &str, local_name: &str) -> Result<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "1" | "-1" => Ok(true),
        "false" | "0" => Ok(false),
        _ => Err(anyhow!(
            "invalid XML boolean attribute {local_name} value {value:?}"
        )),
    }
}

fn parse_max_length(value: &str) -> Result<Option<usize>> {
    let parsed = value
        .parse::<u64>()
        .with_context(|| format!("invalid XML dt:maxLength value {value:?}"))?;
    if parsed == 0xFFFF_FFFF {
        return Ok(None);
    }

    usize::try_from(parsed)
        .map(Some)
        .with_context(|| format!("XML dt:maxLength value {value:?} exceeds platform usize"))
}

fn parse_xml_usize_attr(value: &str, label: &str) -> Result<usize> {
    value
        .parse::<usize>()
        .with_context(|| format!("invalid XML {label} value {value:?}"))
}

fn parse_xml_i32_attr(value: &str, label: &str) -> Result<i32> {
    value
        .parse::<i32>()
        .with_context(|| format!("invalid XML {label} value {value:?}"))
}

fn validate_xml_variant_max_length(
    data_type: Option<&str>,
    max_length: Option<usize>,
    explicit_max_length: bool,
    field_name: &str,
) -> Result<()> {
    if !explicit_max_length
        || !matches!(
            data_type.map(str::to_ascii_lowercase).as_deref(),
            Some("variant")
        )
    {
        return Ok(());
    }
    if let Some(length @ 0..=10) = max_length {
        return Err(anyhow!(
            "invalid XML variant dt:maxLength {length} for field {field_name}"
        ));
    }

    Ok(())
}

fn normalize_max_length(
    max_length: Option<usize>,
    explicit_max_length: bool,
    ado_type: Option<AdoDataType>,
) -> Option<usize> {
    match ado_type.map(|ty| ty.code) {
        Some(12) if explicit_max_length => max_length,
        Some(12) => Some(16),
        Some(7 | 64) => Some(8),
        Some(133 | 134) => Some(6),
        Some(135) => Some(16),
        _ => max_length,
    }
}

fn ado_type_is_fixed_length(ado_type: Option<AdoDataType>) -> bool {
    matches!(
        ado_type.map(|ty| ty.code),
        Some(
            2 | 3
                | 4
                | 5
                | 6
                | 7
                | 11
                | 12
                | 14
                | 16
                | 17
                | 18
                | 19
                | 20
                | 21
                | 64
                | 72
                | 128
                | 129
                | 130
                | 131
                | 133
                | 134
                | 135
        )
    )
}

fn force_null_fields(node: Node<'_, '_>) -> HashSet<String> {
    attr_rs(node, "forcenull")
        .map(|value| {
            value
                .split_whitespace()
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn field_attributes(
    node: Node<'_, '_>,
    is_nullable: bool,
    may_be_null: bool,
    writable: bool,
    fixed_length: bool,
    long: bool,
) -> Result<Vec<FieldAttribute>> {
    let mut attributes = Vec::new();
    if bool_attr_rs(node, "cachedeferred")? {
        attributes.push(FieldAttribute::CacheDeferred);
    }
    if fixed_length {
        attributes.push(FieldAttribute::Fixed);
    }
    if bool_attr_rs(node, "ischapter")? {
        attributes.push(FieldAttribute::IsChapter);
    }
    if bool_attr_rs(node, "iscollection")? {
        attributes.push(FieldAttribute::IsCollection);
    }
    if bool_attr_rs(node, "isdefaultstream")? {
        attributes.push(FieldAttribute::IsDefaultStream);
    }
    if is_nullable {
        attributes.push(FieldAttribute::IsNullable);
    }
    if bool_attr_rs(node, "isrowurl")? {
        attributes.push(FieldAttribute::IsRowUrl);
    }
    if long {
        attributes.push(FieldAttribute::Long);
    }
    if may_be_null {
        attributes.push(FieldAttribute::MayBeNull);
    }
    if bool_attr_rs(node, "maydefer")? {
        attributes.push(FieldAttribute::MayDefer);
    }
    if bool_attr_rs(node, "negativescale")? {
        attributes.push(FieldAttribute::NegativeScale);
    }
    if bool_attr_rs(node, "rowid")? {
        attributes.push(FieldAttribute::RowId);
    }
    if bool_attr_rs(node, "rowversion")? || bool_attr_rs(node, "rowver")? {
        attributes.push(FieldAttribute::RowVersion);
    }
    if bool_attr_rs(node, "writeunknown")? {
        attributes.push(FieldAttribute::UnknownUpdatable);
    }
    if writable {
        attributes.push(FieldAttribute::Updatable);
    }
    Ok(attributes)
}

fn inferred_status_flags(state: RowState) -> Vec<RecordStatusFlag> {
    match state {
        RowState::Current => vec![RecordStatusFlag::Unmodified],
        RowState::Original | RowState::Updated => vec![RecordStatusFlag::Modified],
        RowState::Inserted => vec![RecordStatusFlag::New],
        RowState::Deleted => vec![RecordStatusFlag::Deleted],
    }
}

fn decode_ado_xml_text(bytes: &[u8]) -> Result<Cow<'_, str>> {
    if bytes.starts_with(&[0xff, 0xfe]) {
        return decode_utf16_xml(&bytes[2..], Utf16Endian::Little);
    }
    if bytes.starts_with(&[0xfe, 0xff]) {
        return decode_utf16_xml(&bytes[2..], Utf16Endian::Big);
    }
    if bytes.starts_with(&[b'<', 0x00]) {
        return decode_utf16_xml(bytes, Utf16Endian::Little);
    }
    if bytes.starts_with(&[0x00, b'<']) {
        return decode_utf16_xml(bytes, Utf16Endian::Big);
    }
    if starts_with_utf16_xml_body(bytes, Utf16Endian::Little) {
        return decode_utf16_xml(bytes, Utf16Endian::Little);
    }
    if starts_with_utf16_xml_body(bytes, Utf16Endian::Big) {
        return decode_utf16_xml(bytes, Utf16Endian::Big);
    }

    let bytes = strip_utf8_bom(bytes);
    std::str::from_utf8(bytes)
        .map(Cow::Borrowed)
        .context("ADO XML persistence is expected to be UTF-8 or UTF-16")
}

#[derive(Debug, Clone, Copy)]
enum Utf16Endian {
    Little,
    Big,
}

fn starts_with_utf16_xml_body(mut bytes: &[u8], endian: Utf16Endian) -> bool {
    while bytes.len() >= 2 {
        let unit = match endian {
            Utf16Endian::Little => u16::from_le_bytes([bytes[0], bytes[1]]),
            Utf16Endian::Big => u16::from_be_bytes([bytes[0], bytes[1]]),
        };
        match unit {
            0x09 | 0x0A | 0x0D | 0x20 => bytes = &bytes[2..],
            value => return value == b'<' as u16,
        }
    }
    false
}

fn decode_utf16_xml(bytes: &[u8], endian: Utf16Endian) -> Result<Cow<'_, str>> {
    let chunks = bytes.chunks_exact(2);
    if !chunks.remainder().is_empty() {
        return Err(anyhow!("odd-length UTF-16 ADO XML persistence text"));
    }

    let units = bytes.chunks_exact(2).map(|chunk| match endian {
        Utf16Endian::Little => u16::from_le_bytes([chunk[0], chunk[1]]),
        Utf16Endian::Big => u16::from_be_bytes([chunk[0], chunk[1]]),
    });
    let decoded: String = char::decode_utf16(units)
        .map(|item| item.map_err(|_| anyhow!("invalid UTF-16 ADO XML persistence text")))
        .collect::<Result<_>>()?;
    Ok(Cow::Owned(decoded))
}

fn infer_ado_type(
    data_type: Option<&str>,
    db_type: Option<&str>,
    max_length: Option<usize>,
    long: bool,
    fixed_length: bool,
) -> Option<AdoDataType> {
    let normalized = data_type?.to_ascii_lowercase();
    let db_type = db_type.map(str::to_ascii_lowercase);
    match normalized.as_str() {
        "binary" | "char" | "empty" | "entity" | "entities" | "enumeration" | "error" | "id"
        | "idref" | "idrefs" | "nmtoken" | "nmtokens" | "notation" | "time.tz" | "uri" => {
            Some(infer_unicode_text_ado_type(long, max_length, fixed_length))
        }
        "i1" => Some(AdoDataType::new("adTinyInt", 16)),
        "ui1" => Some(AdoDataType::new("adUnsignedTinyInt", 17)),
        "i2" => Some(AdoDataType::new("adSmallInt", 2)),
        "ui2" => Some(AdoDataType::new("adUnsignedSmallInt", 18)),
        "int" | "i4" | "integer" => Some(AdoDataType::new("adInteger", 3)),
        "ui4" => Some(AdoDataType::new("adUnsignedInt", 19)),
        "i8" => Some(AdoDataType::new("adBigInt", 20)),
        "ui8" => Some(AdoDataType::new("adUnsignedBigInt", 21)),
        "boolean" | "bool" => Some(AdoDataType::new("adBoolean", 11)),
        "r4" => Some(AdoDataType::new("adSingle", 4)),
        "float" | "r8" => Some(AdoDataType::new("adDouble", 5)),
        "number" => match db_type.as_deref() {
            Some("currency") => Some(AdoDataType::new("adCurrency", 6)),
            Some("decimal") => Some(AdoDataType::new("adDecimal", 14)),
            Some("numeric") => Some(AdoDataType::new("adNumeric", 131)),
            _ => Some(AdoDataType::new("adVarNumeric", 139)),
        },
        "fixed.14.4" => Some(infer_unicode_text_ado_type(long, max_length, fixed_length)),
        "currency" => Some(AdoDataType::new("adCurrency", 6)),
        "decimal" => Some(AdoDataType::new("adDecimal", 14)),
        "datetime" => match db_type.as_deref() {
            Some("variantdate") => Some(AdoDataType::new("adDate", 7)),
            Some("date") => Some(AdoDataType::new("adDBDate", 133)),
            Some("time") => Some(AdoDataType::new("adDBTime", 134)),
            Some("dbtimestamp" | "timestamp") => Some(AdoDataType::new("adDBTimeStamp", 135)),
            Some("filetime") => Some(AdoDataType::new("adFileTime", 64)),
            _ => Some(AdoDataType::new("adDBTimeStamp", 135)),
        },
        "datetime.tz" => Some(infer_unicode_text_ado_type(long, max_length, fixed_length)),
        "date" => Some(AdoDataType::new("adDBDate", 133)),
        "time" => Some(AdoDataType::new("adDBTime", 134)),
        "bin.hex" => match (long || max_length.is_none(), fixed_length) {
            (true, _) => Some(AdoDataType::new("adLongVarBinary", 205)),
            (false, true) => Some(AdoDataType::new("adBinary", 128)),
            (false, false) => Some(AdoDataType::new("adVarBinary", 204)),
        },
        "bin.base64" => Some(infer_unicode_text_ado_type(long, max_length, fixed_length)),
        "uuid" => Some(AdoDataType::new("adGUID", 72)),
        "string" => match db_type.as_deref() {
            Some("str") => {
                if long || max_length.is_none() {
                    Some(AdoDataType::new("adLongVarChar", 201))
                } else if fixed_length {
                    Some(AdoDataType::new("adChar", 129))
                } else {
                    Some(AdoDataType::new("adVarChar", 200))
                }
            }
            Some("bstr") => Some(infer_unicode_text_ado_type(long, max_length, fixed_length)),
            _ => Some(infer_unicode_text_ado_type(long, max_length, fixed_length)),
        },
        "variant" => Some(AdoDataType::new("adVariant", 12)),
        _ => None,
    }
}

fn infer_unicode_text_ado_type(
    long: bool,
    max_length: Option<usize>,
    fixed_length: bool,
) -> AdoDataType {
    if long || max_length.is_none() {
        AdoDataType::new("adLongVarWChar", 203)
    } else if fixed_length {
        AdoDataType::new("adWChar", 130)
    } else {
        AdoDataType::new("adVarWChar", 202)
    }
}
