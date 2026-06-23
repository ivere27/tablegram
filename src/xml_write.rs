//! Writer for ADO XML persistence documents.
//!
//! This module serializes the shared `Recordset` model into MDAC-style XML and
//! normalizes field XML names through `rs:name` when needed. It rejects chapter
//! update shapes that ADO XML cannot roundtrip without changing the materialized
//! view.

use std::collections::{HashMap, HashSet};

use anyhow::{bail, Context, Result};

use crate::model::{
    ChapterRelation, Field, FieldAttribute, Recordset, Row, RowChangeKind, RowState, Value,
};

const SCHEMA_NS: &str = "uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882";
const DATATYPE_NS: &str = "uuid:C2F41010-65B3-11d1-A29F-00AA00C14882";
const ROWSET_NS: &str = "urn:schemas-microsoft-com:rowset";
const ROWSET_SCHEMA_NS: &str = "#RowsetSchema";

/// Serialize an ADO Recordset as UTF-8 ADO XML bytes.
///
/// The writer validates the supplied [`Recordset`] and normalizes invalid or
/// duplicate XML attribute names through ADO-style `rs:name` mappings.
pub fn write_ado_xml(recordset: &Recordset) -> Result<Vec<u8>> {
    Ok(write_ado_xml_string(recordset)?.into_bytes())
}

/// Serialize an ADO Recordset as an ADO XML string.
///
/// XML persistence cannot represent pending row changes inside nested chapter
/// recordsets, and cannot represent pending root updates that contain chapter
/// values. Those cases return an error instead of emitting lossy XML.
pub fn write_ado_xml_string(recordset: &Recordset) -> Result<String> {
    crate::validate_recordset_shape(recordset)
        .context("cannot write inconsistent ADO Recordset shape")?;
    let recordset = normalize_writer_xml_names(recordset);
    crate::validate_recordset_shape(&recordset)
        .context("cannot write normalized ADO Recordset shape")?;
    validate_writer_input(&recordset)?;

    let mut out = String::new();
    write_root_open(&mut out);
    write_schema(&mut out, &recordset)?;
    write_data(&mut out, &recordset)?;
    out.push_str("</xml>\n");
    Ok(out)
}

fn normalize_writer_xml_names(recordset: &Recordset) -> Recordset {
    let mut normalized = recordset.clone();
    normalize_recordset_xml_names(&mut normalized);
    normalized
}

fn normalize_recordset_xml_names(recordset: &mut Recordset) {
    normalize_field_xml_names(&mut recordset.fields);
    for row in &mut recordset.rows {
        for value in &mut row.values {
            if let Value::Chapter(chapter) = value {
                normalize_recordset_xml_names(chapter);
            }
        }
    }
}

fn normalize_field_xml_names(fields: &mut [Field]) {
    let xml_names = generated_xml_names(fields);
    for (field, xml_name) in fields.iter_mut().zip(xml_names) {
        field.xml_name = xml_name;
        if let Some(chapter_fields) = &mut field.chapter_fields {
            normalize_field_xml_names(chapter_fields);
        }
    }
}

fn generated_xml_names(fields: &[Field]) -> Vec<String> {
    let mut used = HashSet::new();
    let mut generated_index = 1usize;
    fields
        .iter()
        .map(|field| {
            if is_xml_attribute_name(&field.xml_name)
                && used.insert(field.xml_name.to_ascii_lowercase())
            {
                return field.xml_name.clone();
            }
            loop {
                let candidate = format!("c{generated_index}");
                generated_index += 1;
                if used.insert(candidate.to_ascii_lowercase()) {
                    return candidate;
                }
            }
        })
        .collect()
}

fn validate_writer_input(recordset: &Recordset) -> Result<()> {
    validate_recordset_for_writer(recordset, "recordset", false)
}

fn validate_recordset_for_writer(recordset: &Recordset, label: &str, nested: bool) -> Result<()> {
    if nested
        && recordset
            .changes
            .iter()
            .any(|change| change.kind != RowChangeKind::Current)
    {
        bail!("ADO XML writer cannot represent pending row changes inside nested chapter {label}");
    }
    if !nested
        && recordset.fields.iter().any(is_chapter_field)
        && recordset
            .changes
            .iter()
            .any(|change| change.kind == RowChangeKind::Update)
    {
        bail!(
            "ADO XML writer cannot represent pending root updates with chapter values in {label}"
        );
    }

    let mut seen_xml_names = HashSet::new();
    for (index, field) in recordset.fields.iter().enumerate() {
        if !is_xml_attribute_name(&field.xml_name) {
            bail!(
                "{label}: field {} XML name {:?} is not a valid unprefixed XML name",
                field.name,
                field.xml_name
            );
        }
        if !seen_xml_names.insert(field.xml_name.to_ascii_lowercase()) {
            bail!("{label}: duplicate field XML name {}", field.xml_name);
        }
        validate_xml_text(&field.name).with_context(|| {
            format!(
                "{label}: field {} display name is not valid XML text",
                index + 1
            )
        })?;
        validate_xml_text(&field.xml_name).with_context(|| {
            format!(
                "{label}: field {} XML name is not valid XML text",
                index + 1
            )
        })?;
        if let Some(base_catalog) = &field.base_catalog {
            validate_xml_text(base_catalog).with_context(|| {
                format!(
                    "{label}: field {} base catalog is not valid XML text",
                    field.name
                )
            })?;
        }
        if let Some(base_schema) = &field.base_schema {
            validate_xml_text(base_schema).with_context(|| {
                format!(
                    "{label}: field {} base schema is not valid XML text",
                    field.name
                )
            })?;
        }
        if let Some(base_table) = &field.base_table {
            validate_xml_text(base_table).with_context(|| {
                format!(
                    "{label}: field {} base table is not valid XML text",
                    field.name
                )
            })?;
        }
        if let Some(base_column) = &field.base_column {
            validate_xml_text(base_column).with_context(|| {
                format!(
                    "{label}: field {} base column is not valid XML text",
                    field.name
                )
            })?;
        }
        if is_chapter_field(field) && field.chapter_fields.is_none() {
            bail!(
                "{label}: chapter field {} has no child schema metadata",
                field.name
            );
        }
        if is_chapter_field(field) && field.chapter_relation.is_none() {
            bail!(
                "{label}: chapter field {} has no chapter relation metadata",
                field.name
            );
        }
    }

    for (row_index, row) in recordset.rows.iter().enumerate() {
        for (field_index, (field, value)) in
            recordset.fields.iter().zip(row.values.iter()).enumerate()
        {
            if is_chapter_field(field) {
                match value {
                    Value::Chapter(child) => {
                        let child_label = format!("{label}.row{row_index}.{}", field.xml_name);
                        if let Some(chapter_fields) = &field.chapter_fields {
                            if child.fields != *chapter_fields {
                                bail!(
                                    "{child_label}: child schema did not match field descriptor"
                                );
                            }
                        }
                        validate_recordset_for_writer(child, &child_label, true)?;
                    }
                    Value::Unavailable if row.state == RowState::Updated => {}
                    _ => bail!(
                        "{label}: row {row_index} field {field_index} chapter field had non-chapter value"
                    ),
                }
            } else if matches!(value, Value::Chapter(_)) {
                bail!("{label}: row {row_index} field {field_index} had unexpected chapter value");
            }
        }
    }
    Ok(())
}

fn write_root_open(out: &mut String) {
    out.push_str("<xml");
    write_raw_attr(out, "xmlns:s", SCHEMA_NS);
    out.push('\n');
    out.push('\t');
    write_raw_attr(out, "xmlns:dt", DATATYPE_NS);
    out.push('\n');
    out.push('\t');
    write_raw_attr(out, "xmlns:rs", ROWSET_NS);
    out.push('\n');
    out.push('\t');
    write_raw_attr(out, "xmlns:z", ROWSET_SCHEMA_NS);
    out.push_str(">\n");
}

fn write_schema(out: &mut String, recordset: &Recordset) -> Result<()> {
    out.push_str("<s:Schema id='RowsetSchema'>\n");
    write_element_schema(out, "row", &recordset.fields, None, "\t")?;
    out.push_str("</s:Schema>\n");
    Ok(())
}

fn write_element_schema(
    out: &mut String,
    element_name: &str,
    fields: &[Field],
    chapter_relation: Option<&ChapterRelation>,
    indent: &str,
) -> Result<()> {
    out.push_str(indent);
    out.push_str("<s:ElementType");
    write_attr(out, "name", element_name)?;
    write_raw_attr(out, "content", "eltOnly");
    write_raw_attr(out, "rs:updatable", "true");
    if let Some(chapter_relation) = chapter_relation {
        write_attr(
            out,
            "rs:relation",
            &chapter_relation_to_xml_hex(chapter_relation)?,
        )?;
    }
    out.push_str(">\n");

    let child_indent = format!("{indent}\t");
    for (index, field) in fields.iter().enumerate() {
        if is_chapter_field(field) {
            let chapter_fields = field.chapter_fields.as_deref().ok_or_else(|| {
                anyhow::anyhow!("chapter field {} has no child schema metadata", field.name)
            })?;
            write_element_schema(
                out,
                &field.xml_name,
                chapter_fields,
                field.chapter_relation.as_ref(),
                &child_indent,
            )?;
        } else {
            write_field(out, field, index, &child_indent)?;
        }
    }
    out.push_str(&child_indent);
    out.push_str("<s:extends type='rs:rowbase'/>\n");
    out.push_str(indent);
    out.push_str("</s:ElementType>\n");
    Ok(())
}

fn chapter_relation_to_xml_hex(relation: &ChapterRelation) -> Result<String> {
    if relation.pairs.is_empty() {
        bail!("chapter relation had no key pairs");
    }

    let mut hex = String::with_capacity(relation.pairs.len() * 24);
    for pair in &relation.pairs {
        let parent_ordinal = u32::try_from(pair.parent_ordinal)
            .context("chapter relation parent ordinal exceeded u32 range")?;
        let child_ordinal = u32::try_from(pair.child_ordinal)
            .context("chapter relation child ordinal exceeded u32 range")?;
        if parent_ordinal == 0 || child_ordinal == 0 {
            bail!("chapter relation contained a zero field ordinal");
        }
        append_relation_u32_hex(&mut hex, parent_ordinal);
        append_relation_u32_hex(&mut hex, child_ordinal);
        append_relation_u32_hex(&mut hex, 0);
    }
    Ok(hex)
}

fn append_relation_u32_hex(out: &mut String, value: u32) {
    for byte in value.to_le_bytes() {
        out.push_str(&format!("{byte:02X}"));
    }
}

fn write_field(out: &mut String, field: &Field, index: usize, indent: &str) -> Result<()> {
    let datatype = datatype_spec(field)?;

    out.push_str(indent);
    out.push_str("<s:AttributeType");
    write_attr(out, "name", &field.xml_name)?;
    write_attr(out, "rs:number", &(index + 1).to_string())?;
    if field.name != field.xml_name {
        write_attr(out, "rs:name", &field.name)?;
    }
    if field.writable {
        write_raw_attr(out, "rs:write", "true");
    }
    if field.attributes.contains(&FieldAttribute::IsNullable) {
        write_raw_attr(out, "rs:nullable", "true");
    }
    if field.key_column {
        write_raw_attr(out, "rs:keycolumn", "true");
    }
    if let Some(base_catalog) = &field.base_catalog {
        write_attr(out, "rs:basecatalog", base_catalog)?;
    }
    if let Some(base_schema) = &field.base_schema {
        write_attr(out, "rs:baseschema", base_schema)?;
    }
    if let Some(base_table) = &field.base_table {
        write_attr(out, "rs:basetable", base_table)?;
    }
    if let Some(base_column) = &field.base_column {
        write_attr(out, "rs:basecolumn", base_column)?;
    }
    write_extra_field_attribute_flags(out, field);
    out.push_str(">\n");

    out.push_str(indent);
    out.push_str("\t<s:datatype");
    write_attr(out, "dt:type", datatype.dt_type)?;
    if let Some(db_type) = datatype.db_type {
        write_attr(out, "rs:dbtype", db_type)?;
    }
    if let Some(max_length) = datatype.max_length {
        write_attr(out, "dt:maxLength", &max_length.to_string())?;
    }
    if let Some(scale) = datatype.scale {
        write_attr(out, "rs:scale", &scale.to_string())?;
    }
    if let Some(precision) = datatype.precision {
        write_attr(out, "rs:precision", &precision.to_string())?;
    }
    if datatype.fixed_length {
        write_raw_attr(out, "rs:fixedlength", "true");
    }
    if datatype.long {
        write_raw_attr(out, "rs:long", "true");
    }
    write_raw_attr(
        out,
        "rs:maybenull",
        if field.attributes.contains(&FieldAttribute::MayBeNull) {
            "true"
        } else {
            "false"
        },
    );
    out.push_str("/>\n");
    out.push_str(indent);
    out.push_str("</s:AttributeType>\n");
    Ok(())
}

fn write_extra_field_attribute_flags(out: &mut String, field: &Field) {
    for (attribute, xml_name) in [
        (FieldAttribute::CacheDeferred, "rs:cachedeferred"),
        (FieldAttribute::IsCollection, "rs:iscollection"),
        (FieldAttribute::IsDefaultStream, "rs:isdefaultstream"),
        (FieldAttribute::IsRowUrl, "rs:isrowurl"),
        (FieldAttribute::MayDefer, "rs:maydefer"),
        (FieldAttribute::NegativeScale, "rs:negativescale"),
        (FieldAttribute::RowId, "rs:rowid"),
        (FieldAttribute::RowVersion, "rs:rowversion"),
        (FieldAttribute::UnknownUpdatable, "rs:writeunknown"),
    ] {
        if field.attributes.contains(&attribute) {
            write_raw_attr(out, xml_name, "true");
        }
    }
}

#[derive(Debug, Clone)]
struct DatatypeSpec<'a> {
    dt_type: &'a str,
    db_type: Option<&'a str>,
    max_length: Option<usize>,
    precision: Option<usize>,
    scale: Option<i32>,
    fixed_length: bool,
    long: bool,
}

fn datatype_spec(field: &Field) -> Result<DatatypeSpec<'_>> {
    let code = field.ado_type.map(|ty| ty.code);
    if code == Some(12) {
        return Ok(DatatypeSpec {
            dt_type: "string",
            db_type: None,
            max_length: None,
            precision: None,
            scale: None,
            fixed_length: false,
            long: false,
        });
    }

    let (dt_type, db_type) = match code {
        Some(2) => ("i2", None),
        Some(3) => ("int", None),
        Some(4) => ("r4", None),
        Some(5) => ("float", None),
        Some(6) => ("number", Some("currency")),
        Some(7) => ("dateTime", Some("variantdate")),
        Some(11) => ("boolean", None),
        Some(14) => ("number", Some("decimal")),
        Some(16) => ("i1", None),
        Some(17) => ("ui1", None),
        Some(18) => ("ui2", None),
        Some(19) => ("ui4", None),
        Some(20) => ("i8", None),
        Some(21) => ("ui8", None),
        Some(64) => ("dateTime", Some("filetime")),
        Some(72) => ("uuid", None),
        Some(128 | 204 | 205) => ("bin.hex", None),
        Some(129 | 200 | 201) => ("string", Some("str")),
        Some(130 | 202 | 203) => ("string", None),
        Some(131) => ("number", Some("numeric")),
        Some(133) => ("date", None),
        Some(134) => ("time", None),
        Some(135) => ("dateTime", Some("dbtimestamp")),
        Some(139) => ("number", Some("varnumeric")),
        Some(0 | 8 | 9 | 10 | 13 | 132 | 136 | 138) => {
            bail!(
                "ADO XML writer does not support ADO type {} ({})",
                field.ado_type.unwrap().name,
                field.ado_type.unwrap().code
            )
        }
        Some(code) if code & 0x2000 != 0 => {
            bail!("ADO XML writer does not support ADO array type code {code}")
        }
        Some(code) => bail!("ADO XML writer does not support ADO type code {code}"),
        None => (
            field.data_type.as_deref().unwrap_or("string"),
            field.db_type.as_deref(),
        ),
    };

    Ok(DatatypeSpec {
        dt_type,
        db_type,
        max_length: field.max_length,
        precision: field.precision,
        scale: field.scale,
        fixed_length: field.fixed_length,
        long: field.long,
    })
}

fn write_data(out: &mut String, recordset: &Recordset) -> Result<()> {
    out.push_str("<rs:data>\n");
    let mut duplicate_tracker = DuplicateTracker::default();
    for change in &recordset.changes {
        match change.kind {
            RowChangeKind::Current => {
                for row_index in &change.row_indices {
                    write_row_by_index(
                        out,
                        recordset,
                        *row_index,
                        RowWriteOptions::new("\t", "z:row", "", false),
                        &mut duplicate_tracker,
                    )?;
                }
            }
            RowChangeKind::Insert => {
                out.push_str("\t<rs:insert>\n");
                for row_index in &change.row_indices {
                    write_row_by_index(
                        out,
                        recordset,
                        *row_index,
                        RowWriteOptions::new("\t\t", "z:row", "", false),
                        &mut duplicate_tracker,
                    )?;
                }
                out.push_str("\t</rs:insert>\n");
            }
            RowChangeKind::Delete => {
                out.push_str("\t<rs:delete>\n");
                for row_index in &change.row_indices {
                    write_row_by_index(
                        out,
                        recordset,
                        *row_index,
                        RowWriteOptions::new("\t\t", "z:row", "", false),
                        &mut duplicate_tracker,
                    )?;
                }
                out.push_str("\t</rs:delete>\n");
            }
            RowChangeKind::Update => {
                let original = change.row_indices[0];
                let updated = change.row_indices[1];
                out.push_str("\t<rs:update>\n");
                out.push_str("\t\t<rs:original>\n");
                write_row_by_index(
                    out,
                    recordset,
                    original,
                    RowWriteOptions::new("\t\t\t", "z:row", "", false),
                    &mut duplicate_tracker,
                )?;
                out.push_str("\t\t</rs:original>\n");
                write_row_by_index(
                    out,
                    recordset,
                    updated,
                    RowWriteOptions::new("\t\t", "z:row", "", false),
                    &mut duplicate_tracker,
                )?;
                out.push_str("\t</rs:update>\n");
            }
        }
    }
    out.push_str("</rs:data>\n");
    Ok(())
}

#[derive(Clone, Copy)]
struct RowWriteOptions<'a> {
    indent: &'a str,
    element_name: &'a str,
    element_path: &'a str,
    duplicate: bool,
}

impl<'a> RowWriteOptions<'a> {
    fn new(indent: &'a str, element_name: &'a str, element_path: &'a str, duplicate: bool) -> Self {
        Self {
            indent,
            element_name,
            element_path,
            duplicate,
        }
    }
}

fn write_row_by_index(
    out: &mut String,
    recordset: &Recordset,
    row_index: usize,
    options: RowWriteOptions<'_>,
    duplicate_tracker: &mut DuplicateTracker,
) -> Result<()> {
    let row = &recordset.rows[row_index];
    write_row(out, &recordset.fields, row, options, duplicate_tracker)
        .with_context(|| format!("failed to write row {row_index}"))
}

fn write_row(
    out: &mut String,
    fields: &[Field],
    row: &Row,
    options: RowWriteOptions<'_>,
    duplicate_tracker: &mut DuplicateTracker,
) -> Result<()> {
    out.push_str(options.indent);
    out.push('<');
    out.push_str(options.element_name);
    if options.duplicate {
        write_raw_attr(out, "rs:duplicate", "true");
    }

    let mut force_null_fields = Vec::new();
    let mut chapters = Vec::new();
    for (field, value) in fields.iter().zip(row.values.iter()) {
        if is_chapter_field(field) {
            match value {
                Value::Chapter(chapter) => chapters.push((field, chapter.as_ref())),
                Value::Unavailable => {}
                _ => bail!("field {} expected chapter value", field.name),
            }
            continue;
        }

        match value {
            Value::Unavailable => continue,
            Value::Null => {
                if !field.nullable {
                    bail!("field {} is non-nullable but row contains null", field.name);
                }
                if row.state == RowState::Updated {
                    force_null_fields.push(field.xml_name.as_str());
                }
            }
            Value::Empty if field.ado_type.map(|ty| ty.code) == Some(12) && field.nullable => {
                if row.state == RowState::Updated {
                    force_null_fields.push(field.xml_name.as_str());
                }
            }
            value => {
                let text = value_to_xml_text(value, field)?;
                write_attr(out, &field.xml_name, &text)?;
            }
        }
    }

    if !force_null_fields.is_empty() {
        write_attr(out, "rs:forcenull", &force_null_fields.join(" "))?;
    }
    if chapters.is_empty() {
        out.push_str("/>\n");
    } else {
        out.push_str(">\n");
        let child_indent = format!("{}\t", options.indent);
        for (field, chapter) in chapters {
            write_nested_chapter_rows(
                out,
                field,
                chapter,
                &child_indent,
                options.element_path,
                duplicate_tracker,
            )?;
        }
        out.push_str(options.indent);
        out.push_str("</");
        out.push_str(options.element_name);
        out.push_str(">\n");
    }
    Ok(())
}

fn write_nested_chapter_rows(
    out: &mut String,
    field: &Field,
    chapter: &Recordset,
    indent: &str,
    parent_path: &str,
    duplicate_tracker: &mut DuplicateTracker,
) -> Result<()> {
    let chapter_path = if parent_path.is_empty() {
        field.xml_name.clone()
    } else {
        format!("{parent_path}/{}", field.xml_name)
    };
    for change in &chapter.changes {
        if change.kind != RowChangeKind::Current {
            bail!(
                "ADO XML writer cannot represent pending row changes inside nested chapter {}",
                field.name
            );
        }
        for row_index in &change.row_indices {
            let row = &chapter.rows[*row_index];
            let duplicate = duplicate_tracker.mark_row(&chapter_path, &chapter.fields, row);
            write_row_by_index(
                out,
                chapter,
                *row_index,
                RowWriteOptions::new(indent, &field.xml_name, &chapter_path, duplicate),
                duplicate_tracker,
            )?;
        }
    }
    Ok(())
}

#[derive(Default)]
struct DuplicateTracker {
    seen: HashMap<String, HashSet<String>>,
}

impl DuplicateTracker {
    fn mark_row(&mut self, element_path: &str, fields: &[Field], row: &Row) -> bool {
        let identity = duplicate_row_identity(fields, row);
        !self
            .seen
            .entry(element_path.to_string())
            .or_default()
            .insert(identity)
    }
}

fn duplicate_row_identity(fields: &[Field], row: &Row) -> String {
    let mut indices = fields
        .iter()
        .enumerate()
        .filter_map(|(index, field)| {
            (field.key_column && !is_chapter_field(field)).then_some(index)
        })
        .collect::<Vec<_>>();
    if indices.is_empty() {
        indices = fields
            .iter()
            .enumerate()
            .filter_map(|(index, field)| (!is_chapter_field(field)).then_some(index))
            .collect();
    }

    let mut identity = String::new();
    for index in indices {
        identity.push_str(&index.to_string());
        identity.push('=');
        identity.push_str(
            &row.values
                .get(index)
                .map(|value| format!("{value:?}"))
                .unwrap_or_else(|| "<missing>".to_string()),
        );
        identity.push('\u{1f}');
    }
    identity
}

fn value_to_xml_text(value: &Value, field: &Field) -> Result<String> {
    if field.ado_type.map(|ty| ty.code) == Some(12) {
        return variant_value_to_xml_text(value);
    }

    Ok(match value {
        Value::Empty => String::new(),
        Value::Null | Value::Unavailable => return Ok(String::new()),
        Value::String(value) => value.clone(),
        Value::Boolean(value) => {
            if *value {
                "True".to_string()
            } else {
                "False".to_string()
            }
        }
        Value::Integer(value) => value.to_string(),
        Value::UnsignedInteger(value) => value.to_string(),
        Value::Float(value) => {
            if !value.is_finite() {
                bail!("cannot write non-finite float {value}");
            }
            value.to_string()
        }
        Value::Decimal(value) => value.clone(),
        Value::Date(value) => value.clone(),
        Value::Time(value) => value.clone(),
        Value::DateTime(value) => value.clone(),
        Value::Guid(value) => value.clone(),
        Value::BinaryHex(value) => binary_hex_to_xml_hex(value)?,
        Value::Chapter(_) => bail!("ADO XML writer does not yet support chapter values"),
    })
}

fn variant_value_to_xml_text(value: &Value) -> Result<String> {
    Ok(match value {
        Value::Empty | Value::Null | Value::Unavailable => String::new(),
        Value::String(value) => value.clone(),
        Value::Boolean(value) => {
            if *value {
                "True".to_string()
            } else {
                "False".to_string()
            }
        }
        Value::Integer(value) => value.to_string(),
        Value::UnsignedInteger(value) => value.to_string(),
        Value::Float(value) => {
            if !value.is_finite() {
                bail!("cannot write non-finite variant float {value}");
            }
            value.to_string()
        }
        Value::Decimal(value) => value.clone(),
        Value::Date(value) | Value::Time(value) | Value::DateTime(value) => value.clone(),
        Value::Guid(value) => value.clone(),
        Value::BinaryHex(value) => binary_hex_to_xml_hex(value)?,
        Value::Chapter(_) => bail!("ADO XML writer does not yet support chapter values"),
    })
}

fn write_attr(out: &mut String, name: &str, value: &str) -> Result<()> {
    validate_xml_text(value)
        .with_context(|| format!("attribute {name} contains invalid XML text"))?;
    out.push(' ');
    out.push_str(name);
    out.push_str("='");
    escape_attribute_value(out, value);
    out.push('\'');
    Ok(())
}

fn binary_hex_to_xml_hex(value: &str) -> Result<String> {
    let mut bytes =
        hex::decode(value).with_context(|| format!("invalid binary hex value {value:?}"))?;
    for byte in &mut bytes {
        *byte = xml_binary_preimage(*byte)?;
    }
    Ok(hex::encode_upper(bytes))
}

fn xml_binary_preimage(byte: u8) -> Result<u8> {
    if byte == 0x92 {
        return Ok(0x83);
    }
    if normalize_ado_xml_binary_byte(byte) == byte {
        return Ok(byte);
    }

    bail!("binary byte 0x{byte:02X} cannot be represented losslessly in ADO XML bin.hex")
}

fn normalize_ado_xml_binary_byte(byte: u8) -> u8 {
    match byte {
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
    }
}

fn write_raw_attr(out: &mut String, name: &str, value: &str) {
    out.push(' ');
    out.push_str(name);
    out.push_str("='");
    out.push_str(value);
    out.push('\'');
}

fn escape_attribute_value(out: &mut String, value: &str) {
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '\'' => out.push_str("&#x27;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\t' => out.push_str("&#x9;"),
            '\n' => out.push_str("&#xA;"),
            '\r' => out.push_str("&#xD;"),
            _ => out.push(ch),
        }
    }
}

fn validate_xml_text(value: &str) -> Result<()> {
    if let Some(ch) = value.chars().find(|ch| !is_xml_char(*ch)) {
        bail!("invalid XML character U+{:04X}", ch as u32);
    }
    Ok(())
}

fn is_xml_char(ch: char) -> bool {
    matches!(
        ch as u32,
        0x09 | 0x0A | 0x0D | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF
    )
}

fn is_chapter_field(field: &Field) -> bool {
    field.ado_type.map(|ty| ty.code) == Some(136)
}

fn is_xml_attribute_name(name: &str) -> bool {
    if name.is_empty() || name.contains(':') {
        return false;
    }
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    is_xml_name_start_char(first) && chars.all(is_xml_name_char)
}

fn is_xml_name_start_char(ch: char) -> bool {
    matches!(
        ch as u32,
        0x0041..=0x005A
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
