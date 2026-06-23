//! Writer for MDAC-compatible ADTG byte streams.
//!
//! The writer emits the observed ADTG header/schema blocks and row-group
//! markers used by MDAC, including shaped/chaptered rowsets. Validation stays
//! strict so unsupported `Recordset` shapes fail before any lossy binary output
//! is produced.

use anyhow::{anyhow, bail, Context, Result};
use encoding_rs::{Encoding, EUC_KR};
use std::collections::{BTreeMap, BTreeSet};

use crate::model::{ChapterRelation, Field, FieldAttribute, Recordset, Row, RowChangeKind, Value};
use crate::util::gregorian_month_len;

const ADTG_HEADER_HEX: &str = concat!(
    "010754472100000000021900B692F23F04B2CF118D2300AA005FFE5801000000",
    "0000000000036700D2AD63F602EBCF11B0E300AA003F000F0000000500050000",
    "0000000000020000000100C13C8EB6EB6DD0118DF600AA005FFE5807000B0000",
    "00040001000000130000000400010000000D00000000000E00000000000F0000",
    "000000100000000000120000000000107C000200BE22B5C8F35CCE11ADE500AA",
    "0044773D04007F0000000200FFFF860000000200FFFF22000000040000000000",
    "49000000040000000000C13C8EB6EB6DD0118DF600AA005FFE58050004000000",
    "04000F000000050000000400020000000300000004000F000000070000000400",
    "3200000008000000040003000000"
);

const ADTG_SHAPE_ROOT_HEADER_HEX: &str = concat!(
    "010754472100000000021900B692F23F04B2CF118D2300AA005FFE5801000000",
    "0000000000037300D2AD63F602EBCF11B0E300AA003F000F0000000400040000",
    "0000000000030000000100C13C8EB6EB6DD0118DF600AA005FFE5807000B0000",
    "00040001000000130000000400010000000D00000000000E00000000000F0000",
    "000000100000000000120000000C004F0072006400650072007300107C000200",
    "BE22B5C8F35CCE11ADE500AA0044773D04007F00000002000000860000000200",
    "00002200000004001E00000049000000040000000000C13C8EB6EB6DD0118DF6",
    "00AA005FFE5805000400000004000F0000000500000004000200000003000000",
    "04000F0000000700000004003200000008000000040003000000"
);

const ADTG_SHAPE_CHILD_HEADER_HEX: &str = concat!(
    "037100D2AD63F602EBCF11B0E300AA003F000F00000003000300000000000000",
    "0A0000000100C13C8EB6EB6DD0118DF600AA005FFE5807000B00000004000100",
    "0000130000000400010000000D00000000000E00000000000F00000000001000",
    "00000000120000000A004C0069006E0065007300107C000200BE22B5C8F35CCE",
    "11ADE500AA0044773D04007F0000000200000086000000020000002200000004",
    "001E00000049000000040000000000C13C8EB6EB6DD0118DF600AA005FFE5805",
    "000400000004000F000000050000000400020000000300000004000F00000007",
    "00000004003200000008000000040003000000"
);

const ADTG_PROVIDER_CATALOG: &str = "AdoRecordsetSales";
const ADTG_PROVIDER_SCHEMA: &str = "dbo";

#[derive(Debug, Clone)]
struct AdtgField {
    name: String,
    source_table: Option<String>,
    source_column: Option<String>,
    type_code: u16,
    defined_size: u32,
    precision: u32,
    scale: u32,
    attributes: u32,
    source_catalog: Option<String>,
    source_schema: Option<String>,
    chapter_relation: Option<ChapterRelation>,
}

#[derive(Debug, Clone)]
struct AdtgSchema {
    name: String,
    row_group_id: Option<u32>,
    wide_layout: bool,
    fields: Vec<AdtgField>,
    children: Vec<AdtgChildSchema>,
}

#[derive(Debug, Clone)]
struct AdtgChildSchema {
    field_index: usize,
    schema: AdtgSchema,
}

#[derive(Debug, Clone, Copy)]
enum ChapterRowMarker {
    Root,
    Child(u32),
}

#[derive(Debug, Clone)]
struct ChapterRowChange<'a> {
    kind: RowChangeKind,
    rows: Vec<&'a Row>,
}

/// Options for ADTG serialization.
///
/// MDAC ADTG stores ANSI text fields as raw bytes without declaring a codepage.
/// Use this when writing `adChar`/`adVarChar`/`adLongVarChar` data for a known
/// legacy consumer codepage.
#[derive(Clone, Copy)]
pub struct AdtgWriteOptions {
    /// Encoding used for non-Unicode text fields.
    pub ansi_encoding: &'static Encoding,
}

impl Default for AdtgWriteOptions {
    fn default() -> Self {
        Self {
            ansi_encoding: EUC_KR,
        }
    }
}

impl AdtgWriteOptions {
    /// Use a specific ANSI encoding for non-Unicode text fields.
    pub fn with_ansi_encoding(mut self, encoding: &'static Encoding) -> Self {
        self.ansi_encoding = encoding;
        self
    }

    /// Use an [`encoding_rs`] label such as `b"windows-1252"` for ANSI fields.
    pub fn with_ansi_encoding_label(mut self, label: &[u8]) -> Option<Self> {
        self.ansi_encoding = Encoding::for_label(label)?;
        Some(self)
    }
}

/// Serialize an ADO Recordset as ADTG bytes.
pub fn write_adtg(recordset: &Recordset) -> Result<Vec<u8>> {
    write_adtg_with_options(recordset, AdtgWriteOptions::default())
}

/// Serialize an ADO Recordset as ADTG bytes with explicit options.
///
/// The writer validates the supplied [`Recordset`] before serialization and
/// rejects shapes or values that cannot be represented safely in ADTG.
pub fn write_adtg_with_options(
    recordset: &Recordset,
    options: AdtgWriteOptions,
) -> Result<Vec<u8>> {
    crate::validate_recordset_shape(recordset)
        .context("cannot write inconsistent ADO Recordset shape")?;
    validate_adtg_writer_input(recordset)?;

    if recordset_has_chapters(recordset) {
        validate_chapter_writer_input(recordset)?;
        let mut next_row_group_id = 2;
        let schema = adtg_schema(recordset, "Orders", None, false, &mut next_row_group_id, 0)?;
        let roots = [recordset];
        let mut out = Vec::new();
        write_chapter_schema_blocks(&mut out, &schema, &roots, false)?;
        write_chapter_row_groups(&mut out, &schema, &roots, ChapterRowMarker::Root, &options)?;
        out.push(0x0f);
        return Ok(out);
    }

    let fields = adtg_fields(&recordset.fields)?;

    let mut out = adtg_header(fields.len(), default_row_count(recordset)?)?;
    for (index, field) in fields.iter().enumerate() {
        write_field_descriptor(&mut out, field, index + 1)?;
    }
    write_rows(&mut out, recordset, &fields, &options)?;
    out.push(0x0f);
    Ok(out)
}

fn validate_adtg_writer_input(recordset: &Recordset) -> Result<()> {
    validate_adtg_writer_input_at(recordset, 0)
}

fn validate_adtg_writer_input_at(recordset: &Recordset, depth: usize) -> Result<()> {
    validate_adtg_writer_depth("ADTG writer input", depth)?;
    if recordset.fields.is_empty() {
        bail!("ADTG writer requires at least one field");
    }
    if recordset.fields.len() > u16::MAX as usize {
        bail!("ADTG writer supports at most 65535 fields");
    }
    for field in &recordset.fields {
        if field.name.encode_utf16().count() > u16::MAX as usize {
            bail!("ADTG field name {} is too long", field.name);
        }
        if let Some(chapter_fields) = &field.chapter_fields {
            validate_adtg_writer_input_at(
                &Recordset {
                    fields: chapter_fields.clone(),
                    rows: Vec::new(),
                    changes: Vec::new(),
                },
                depth + 1,
            )?;
        }
    }
    Ok(())
}

fn validate_adtg_writer_depth(context: &str, depth: usize) -> Result<()> {
    if depth > crate::MAX_RECORDSET_DEPTH {
        bail!(
            "{context}: exceeded maximum ADO Recordset chapter depth {}",
            crate::MAX_RECORDSET_DEPTH
        );
    }
    Ok(())
}

fn validate_chapter_writer_input(recordset: &Recordset) -> Result<()> {
    validate_chapter_writer_input_at(recordset, 0)
}

fn validate_chapter_writer_input_at(recordset: &Recordset, depth: usize) -> Result<()> {
    validate_adtg_writer_depth("ADTG chapter writer input", depth)?;
    for change in &recordset.changes {
        for row_index in &change.row_indices {
            let row = row_at(recordset, *row_index)?;
            for value in &row.values {
                if let Value::Chapter(child) = value {
                    validate_chapter_writer_input_at(child, depth + 1)?;
                }
            }
        }
    }
    Ok(())
}

fn recordset_has_chapters(recordset: &Recordset) -> bool {
    recordset.fields.iter().any(|field| {
        field.ado_type.map(|ty| ty.code) == Some(136)
            || field.chapter_fields.is_some()
            || field.chapter_relation.is_some()
    })
}

fn adtg_fields(fields: &[Field]) -> Result<Vec<AdtgField>> {
    fields
        .iter()
        .enumerate()
        .map(|(index, field)| adtg_field(field, index, false))
        .collect()
}

fn adtg_schema(
    recordset: &Recordset,
    name: &str,
    row_group_id: Option<u32>,
    wide_layout: bool,
    next_row_group_id: &mut u32,
    depth: usize,
) -> Result<AdtgSchema> {
    validate_adtg_writer_depth("ADTG schema writer", depth)?;
    let fields = recordset
        .fields
        .iter()
        .enumerate()
        .map(|(index, field)| adtg_field(field, index, true))
        .collect::<Result<Vec<_>>>()?;
    let mut children = Vec::new();
    for (field_index, field) in recordset.fields.iter().enumerate() {
        if field.ado_type.map(|ty| ty.code) != Some(136) {
            continue;
        }
        let child_fields = field
            .chapter_fields
            .as_ref()
            .ok_or_else(|| anyhow!("chapter field {} had no child schema", field.name))?;
        let child_group_id = *next_row_group_id;
        *next_row_group_id = next_row_group_id
            .checked_add(1)
            .ok_or_else(|| anyhow!("ADTG chapter row group id overflow"))?;
        let child = Recordset {
            fields: child_fields.clone(),
            rows: Vec::new(),
            changes: Vec::new(),
        };
        let child_wide_layout = field
            .chapter_relation
            .as_ref()
            .is_some_and(|relation| relation.pairs.len() > 1);
        children.push(AdtgChildSchema {
            field_index,
            schema: adtg_schema(
                &child,
                &field.name,
                Some(child_group_id),
                child_wide_layout,
                next_row_group_id,
                depth + 1,
            )?,
        });
    }

    Ok(AdtgSchema {
        name: name.to_string(),
        row_group_id,
        wide_layout,
        fields,
        children,
    })
}

fn default_row_count(recordset: &Recordset) -> Result<usize> {
    let mut count = 0usize;
    for change in &recordset.changes {
        match change.kind {
            RowChangeKind::Current | RowChangeKind::Insert => {
                count = count
                    .checked_add(change.row_indices.len())
                    .ok_or_else(|| anyhow!("ADTG row count overflow"))?;
            }
            RowChangeKind::Update => {
                count = count
                    .checked_add(1)
                    .ok_or_else(|| anyhow!("ADTG row count overflow"))?;
            }
            RowChangeKind::Delete => {}
        }
    }
    Ok(count)
}

fn adtg_header(field_count: usize, row_count: usize) -> Result<Vec<u8>> {
    let mut header = hex::decode(ADTG_HEADER_HEX).context("invalid ADTG header template")?;
    if header.len() != 270 {
        bail!("invalid ADTG header template length {}", header.len());
    }
    let field_count_u16 =
        u16::try_from(field_count).context("ADTG field count exceeded header range")?;
    let field_count_u32 =
        u32::try_from(field_count).context("ADTG field count exceeded header range")?;
    let row_count_u16 = u16::try_from(row_count).context("ADTG row count exceeded header range")?;

    header[0x38..0x3c].copy_from_slice(&field_count_u32.to_be_bytes());
    header[0x3c..0x3e].copy_from_slice(&field_count_u16.to_be_bytes());
    header[0x44..0x46].copy_from_slice(&row_count_u16.to_be_bytes());
    Ok(header)
}

fn adtg_shape_header(
    schema: &AdtgSchema,
    row_count: usize,
    pending_changes: bool,
    provider_shape: bool,
) -> Result<Vec<u8>> {
    let root = schema.row_group_id.is_none();
    let template = if root {
        ADTG_SHAPE_ROOT_HEADER_HEX
    } else {
        ADTG_SHAPE_CHILD_HEADER_HEX
    };
    let mut header = hex::decode(template).context("invalid ADTG shape header template")?;
    let name_bytes = utf16le_bytes(&schema.name);
    if name_bytes.len() > u16::MAX as usize {
        bail!("ADTG shape schema name {} is too long", schema.name);
    }
    let (
        length_offset,
        field_count_u32_offset,
        field_count_u16_offset,
        pending_flag_offset,
        row_count_offset,
        name_offset,
        template_name_len,
    ) = if root {
        (0x26, 0x38, 0x3c, 0x41, 0x44, 0x8d, "Orders".len() * 2)
    } else {
        (0x01, 0x13, 0x17, 0x1c, 0x1f, 0x68, "Lines".len() * 2)
    };
    let suffix_offset = name_offset + 2 + template_name_len;
    let mut patched = Vec::with_capacity(header.len() - template_name_len + name_bytes.len());
    patched.extend_from_slice(&header[..name_offset]);
    patched.extend_from_slice(
        &u16::try_from(name_bytes.len())
            .context("ADTG shape schema name exceeded u16 range")?
            .to_le_bytes(),
    );
    patched.extend_from_slice(&name_bytes);
    patched.extend_from_slice(&header[suffix_offset..]);
    header = patched;

    let shape_len = 0x67usize
        .checked_add(name_bytes.len())
        .ok_or_else(|| anyhow!("ADTG shape header length overflow"))?;
    let field_count_u16 =
        u16::try_from(schema.fields.len()).context("ADTG field count exceeded header range")?;
    let field_count_u32 =
        u32::try_from(schema.fields.len()).context("ADTG field count exceeded header range")?;
    let row_count_u16 = u16::try_from(row_count).context("ADTG row count exceeded header range")?;
    header[length_offset..length_offset + 2]
        .copy_from_slice(&u16::try_from(shape_len)?.to_le_bytes());
    if provider_shape || schema.wide_layout {
        let name_len_delta = name_bytes.len() as isize - template_name_len as isize;
        let (first_size_offset, second_size_offset, row_size_offset) = if root {
            (0xb8, 0xc0, 0xc8)
        } else {
            (0x91, 0x99, 0xa1)
        };
        let first_size_offset = shifted_shape_offset(first_size_offset, name_len_delta)?;
        let second_size_offset = shifted_shape_offset(second_size_offset, name_len_delta)?;
        let row_size_offset = shifted_shape_offset(row_size_offset, name_len_delta)?;
        header[first_size_offset..first_size_offset + 2].copy_from_slice(&[0xff, 0xff]);
        header[second_size_offset..second_size_offset + 2].copy_from_slice(&[0xff, 0xff]);
        header[row_size_offset..row_size_offset + 4].copy_from_slice(&0x78u32.to_le_bytes());
    }
    header[field_count_u32_offset..field_count_u32_offset + 4]
        .copy_from_slice(&field_count_u32.to_be_bytes());
    header[field_count_u16_offset..field_count_u16_offset + 2]
        .copy_from_slice(&field_count_u16.to_be_bytes());
    header[pending_flag_offset] = u8::from(pending_changes);
    header[row_count_offset..row_count_offset + 2].copy_from_slice(&row_count_u16.to_be_bytes());
    Ok(header)
}

fn shifted_shape_offset(offset: usize, delta: isize) -> Result<usize> {
    offset
        .checked_add_signed(delta)
        .ok_or_else(|| anyhow!("ADTG shape header offset overflow"))
}

fn adtg_field(field: &Field, index: usize, shaped: bool) -> Result<AdtgField> {
    let ado_code = field
        .ado_type
        .map(|ty| ty.code)
        .ok_or_else(|| anyhow!("field {} has no ADO type metadata", field.name))?;
    let mut attributes = field_attribute_flags(field);
    let (type_code, defined_size) = match ado_code {
        2 => (2, exact_size(field, 2)?),
        3 => (3, exact_size(field, 4)?),
        4 => (4, min_size(field, 4)?),
        5 => {
            let size = field.max_length.unwrap_or(8);
            if !matches!(size, 4 | 8) {
                bail!("ADTG adDouble field {} width must be 4 or 8", field.name);
            }
            (5, size as u32)
        }
        6 => (6, exact_size(field, 8)?),
        7 => (7, exact_size(field, 8)?),
        11 => (11, exact_size(field, 2)?),
        12 => (12, exact_size(field, 16)?),
        14 => (14, exact_size(field, 16)?),
        16 => (16, exact_size(field, 1)?),
        17 => (17, exact_size(field, 1)?),
        18 => (18, exact_size(field, 2)?),
        19 => (19, exact_size(field, 4)?),
        20 => (20, exact_size(field, 8)?),
        21 => (21, exact_size(field, 8)?),
        64 => (64, exact_size(field, 8)?),
        72 => (72, exact_size(field, 16)?),
        128 => {
            attributes |= 0x10;
            (128, required_max_length(field)?)
        }
        204 => {
            attributes &= !0x10;
            attributes &= !0x80;
            (128, required_max_length(field)?)
        }
        205 => {
            attributes &= !0x10;
            attributes |= 0x80;
            (128, required_max_length(field)?)
        }
        129 => {
            attributes |= 0x10;
            (129, required_max_length(field)?)
        }
        200 => {
            attributes &= !0x10;
            attributes &= !0x80;
            (129, required_max_length(field)?)
        }
        201 => {
            attributes &= !0x10;
            attributes |= 0x80;
            (129, required_max_length(field)?)
        }
        130 => {
            attributes |= 0x10;
            (130, required_max_length(field)?)
        }
        202 => {
            attributes &= !0x10;
            attributes &= !0x80;
            (130, required_max_length(field)?)
        }
        203 => {
            attributes &= !0x10;
            attributes |= 0x80;
            (130, required_max_length(field)?)
        }
        131 => (131, exact_size(field, 19)?),
        133 => (133, exact_size(field, 6)?),
        134 => (134, exact_size(field, 6)?),
        135 => (135, exact_size(field, 16)?),
        136 => {
            attributes |= 0x10 | 0x2000;
            (136, exact_size(field, 4)?)
        }
        139 => (139, required_max_length(field)?),
        other => bail!(
            "ADTG writer does not support ADO type code {other} for field {}",
            field.name
        ),
    };

    let (precision, scale) = descriptor_precision_scale(field, type_code, shaped)?;
    if field.ordinal.is_some_and(|ordinal| ordinal != index + 1) {
        bail!(
            "ADTG writer requires sequential field ordinals; field {} had {:?}, expected {}",
            field.name,
            field.ordinal,
            index + 1
        );
    }

    Ok(AdtgField {
        name: field.name.clone(),
        source_table: field.base_table.clone(),
        source_column: field.base_column.clone(),
        source_catalog: field.base_catalog.clone(),
        source_schema: field.base_schema.clone(),
        type_code,
        defined_size,
        precision,
        scale,
        attributes,
        chapter_relation: field.chapter_relation.clone(),
    })
}

fn exact_size(field: &Field, expected: usize) -> Result<u32> {
    let actual = field.max_length.unwrap_or(expected);
    if actual != expected {
        bail!(
            "ADTG field {} width {actual} did not match required width {expected}",
            field.name
        );
    }
    Ok(expected as u32)
}

fn min_size(field: &Field, minimum: usize) -> Result<u32> {
    let actual = field.max_length.unwrap_or(minimum);
    if actual < minimum {
        bail!(
            "ADTG field {} width {actual} was smaller than required width {minimum}",
            field.name
        );
    }
    Ok(actual as u32)
}

fn required_max_length(field: &Field) -> Result<u32> {
    if field.long && field.max_length.is_none() {
        return Ok(u32::MAX);
    }
    let max_length = field
        .max_length
        .ok_or_else(|| anyhow!("ADTG field {} requires max_length metadata", field.name))?;
    if max_length == 0 {
        bail!("ADTG field {} had zero max_length", field.name);
    }
    u32::try_from(max_length).context("ADTG max_length exceeded u32 range")
}

fn descriptor_precision_scale(field: &Field, type_code: u16, shaped: bool) -> Result<(u32, u32)> {
    match type_code {
        14 => {
            let precision = field.precision.unwrap_or(255);
            let scale = field.scale.map(|value| value as u32).unwrap_or(255);
            if (precision, scale) == (255, 255) {
                return Ok((precision as u32, scale));
            }
            validate_decimal_descriptor(precision, scale, 28, &field.name)?;
            Ok((precision as u32, scale))
        }
        131 => {
            let precision = field.precision.unwrap_or(18);
            let scale = field.scale.map(|value| value as u32).unwrap_or(0);
            validate_decimal_descriptor(precision, scale, 38, &field.name)?;
            Ok((precision as u32, scale))
        }
        139 => {
            let precision = field.precision.unwrap_or(255);
            let scale = field.scale.map(|value| value as u32).unwrap_or(255);
            if (precision, scale) == (255, 255) {
                return Ok((precision as u32, scale));
            }
            validate_decimal_descriptor(precision, scale, 38, &field.name)?;
            Ok((precision as u32, scale))
        }
        _ => {
            let precision = match field.precision {
                Some(value) => value as u32,
                None if shaped && type_code != 136 => 255,
                None => 0,
            };
            let scale = match field.scale {
                Some(value) => value as u32,
                None if shaped && type_code != 136 => 255,
                None => 0,
            };
            Ok((precision, scale))
        }
    }
}

fn validate_decimal_descriptor(
    precision: usize,
    scale: u32,
    max_precision: usize,
    field_name: &str,
) -> Result<()> {
    if !(1..=max_precision).contains(&precision) {
        bail!("invalid ADTG decimal precision {precision} for field {field_name}");
    }
    if scale > max_precision as u32 || scale as usize > precision {
        bail!("invalid ADTG decimal scale {scale} for field {field_name}");
    }
    Ok(())
}

fn field_attribute_flags(field: &Field) -> u32 {
    let mut flags = FieldAttribute::bits(&field.attributes);
    if field.key_column {
        flags |= 0x8000;
    }
    flags
}

fn write_field_descriptor(out: &mut Vec<u8>, field: &AdtgField, ordinal: usize) -> Result<()> {
    write_field_descriptor_inner(out, field, ordinal, None, false)
}

fn schema_has_provider_source(schema: &AdtgSchema) -> bool {
    schema
        .fields
        .iter()
        .any(|field| field.source_column.is_some())
}

fn schema_tree_has_pending(schema: &AdtgSchema, recordsets: &[&Recordset]) -> Result<bool> {
    if chapter_row_changes(recordsets, &schema.fields)?
        .iter()
        .any(|change| change.kind != RowChangeKind::Current)
    {
        return Ok(true);
    }
    for child in &schema.children {
        let child_recordsets = chapter_recordsets_for_child(recordsets, child.field_index)?;
        if schema_tree_has_pending(&child.schema, &child_recordsets)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn write_provider_source_block(out: &mut Vec<u8>, schema: &AdtgSchema) -> Result<()> {
    let catalog_name = provider_catalog_name(schema);
    let schema_name = provider_schema_name(schema);
    let table_name = provider_table_name(schema);
    let qualified_name = format!("\"{catalog_name}\".\"{schema_name}\".\"{table_name}\"");
    let scalar_source_count = schema
        .fields
        .iter()
        .filter(|field| field.type_code != 136 && field.source_column.is_some())
        .count();
    let key_ordinal = schema
        .fields
        .iter()
        .position(|field| field.type_code != 136 && field.attributes & 0x8000 != 0)
        .map(|index| index + 1)
        .unwrap_or(1);

    let mut block = Vec::new();
    block.push(0x05);
    block.push(0);
    block.push(0);
    block.extend_from_slice(&1u16.to_le_bytes());
    write_utf16_len_string(&mut block, &qualified_name, "ADTG provider source name")?;
    write_utf16_len_string(&mut block, &table_name, "ADTG provider rowset name")?;
    block.extend_from_slice(&0u16.to_le_bytes());
    block.extend_from_slice(
        &u16::try_from(scalar_source_count)
            .context("ADTG provider source column count exceeded u16 range")?
            .to_le_bytes(),
    );
    block.extend_from_slice(&1u16.to_le_bytes());
    block.extend_from_slice(
        &u16::try_from(key_ordinal)
            .context("ADTG provider source key ordinal exceeded u16 range")?
            .to_le_bytes(),
    );
    let block_len = block
        .len()
        .checked_sub(3)
        .ok_or_else(|| anyhow!("invalid ADTG provider source block length"))?;
    block[1] = u8::try_from(block_len).context("ADTG provider source block exceeded 255 bytes")?;
    out.extend_from_slice(&block);
    Ok(())
}

fn provider_table_name(schema: &AdtgSchema) -> String {
    if let Some(table_name) = provider_table_name_from_metadata(schema) {
        return table_name;
    }

    if schema.fields.iter().any(|field| field.name == "LineId")
        || schema.fields.iter().any(|field| field.name == "LineNumber")
    {
        return "SalesOrderLines".to_string();
    }
    if schema
        .fields
        .iter()
        .any(|field| field.name == "ProductName")
        || schema.fields.iter().any(|field| field.name == "UnitCost")
    {
        return "SalesProducts".to_string();
    }
    if schema
        .fields
        .iter()
        .any(|field| field.name == "CustomerName")
        || schema
            .fields
            .iter()
            .any(|field| field.name == "CustomerCode")
    {
        return "SalesCustomers".to_string();
    }
    if schema.fields.iter().any(|field| field.name == "RegionName") {
        return "SalesRegions".to_string();
    }
    if schema.fields.iter().any(|field| field.name == "PaymentId") {
        return "SalesPayments".to_string();
    }
    if schema
        .fields
        .iter()
        .any(|field| field.name == "CategoryName")
    {
        return "SalesCategories".to_string();
    }
    if schema.fields.iter().any(|field| field.name == "OrderId") {
        return "SalesOrders".to_string();
    }

    let mut name = schema
        .name
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>();
    if name.is_empty() {
        name.push_str("Rows");
    }
    name
}

fn provider_catalog_name(schema: &AdtgSchema) -> String {
    provider_metadata_name(schema, |field| field.source_catalog.as_deref())
        .unwrap_or_else(|| ADTG_PROVIDER_CATALOG.to_string())
}

fn provider_schema_name(schema: &AdtgSchema) -> String {
    provider_metadata_name(schema, |field| field.source_schema.as_deref())
        .unwrap_or_else(|| ADTG_PROVIDER_SCHEMA.to_string())
}

fn provider_table_name_from_metadata(schema: &AdtgSchema) -> Option<String> {
    first_provider_metadata_name(schema, |field| field.source_table.as_deref())
}

fn provider_metadata_name(
    schema: &AdtgSchema,
    value: impl Fn(&AdtgField) -> Option<&str>,
) -> Option<String> {
    let mut counts = BTreeMap::<String, usize>::new();
    for field in &schema.fields {
        if field.type_code == 136 {
            continue;
        }
        if let Some(value) = value(field).filter(|value| !value.is_empty()) {
            *counts.entry(value.to_string()).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(&left.0)))
        .map(|(table, _)| table)
}

fn first_provider_metadata_name(
    schema: &AdtgSchema,
    value: impl Fn(&AdtgField) -> Option<&str>,
) -> Option<String> {
    schema
        .fields
        .iter()
        .filter(|field| field.type_code != 136)
        .find_map(|field| value(field).filter(|value| !value.is_empty()))
        .map(str::to_string)
}

fn write_utf16_len_string(out: &mut Vec<u8>, value: &str, context: &str) -> Result<()> {
    let units = value.encode_utf16().collect::<Vec<_>>();
    out.extend_from_slice(
        &u16::try_from(units.len())
            .with_context(|| format!("{context} exceeded u16 length"))?
            .to_le_bytes(),
    );
    for unit in units {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    Ok(())
}

fn write_field_descriptor_inner(
    out: &mut Vec<u8>,
    field: &AdtgField,
    ordinal: usize,
    row_group_id: Option<u32>,
    provider_shape: bool,
) -> Result<()> {
    let provider_descriptor = provider_shape && field.source_column.is_some();
    let mut descriptor = Vec::new();
    descriptor.push(0x06);
    descriptor.push(0);
    if provider_descriptor {
        descriptor.extend_from_slice(&[0x00, 0xf3, 0x01, 0x00]);
    } else {
        descriptor.extend_from_slice(&[0x00, 0x80, 0x01, 0x00]);
    }
    descriptor.extend_from_slice(
        &u16::try_from(ordinal)
            .context("ADTG descriptor ordinal exceeded u16 range")?
            .to_le_bytes(),
    );
    let name_units = field.name.encode_utf16().collect::<Vec<_>>();
    descriptor.extend_from_slice(
        &u16::try_from(name_units.len())
            .context("ADTG field name length exceeded u16 range")?
            .to_le_bytes(),
    );
    for unit in name_units {
        descriptor.extend_from_slice(&unit.to_le_bytes());
    }
    if provider_descriptor {
        let source_column = field
            .source_column
            .as_ref()
            .ok_or_else(|| anyhow!("ADTG provider descriptor had no source column"))?;
        descriptor.extend_from_slice(&1u16.to_le_bytes());
        descriptor.extend_from_slice(
            &u16::try_from(ordinal)
                .context("ADTG source ordinal exceeded u16 range")?
                .to_le_bytes(),
        );
        write_utf16_len_string(&mut descriptor, source_column, "ADTG source column name")?;
    }
    descriptor.extend_from_slice(&field.type_code.to_le_bytes());
    descriptor.extend_from_slice(&field.defined_size.to_le_bytes());
    descriptor.extend_from_slice(&field.precision.to_le_bytes());
    descriptor.extend_from_slice(&field.scale.to_le_bytes());
    descriptor.extend_from_slice(&field.attributes.to_le_bytes());
    if provider_descriptor {
        let source_catalog = field
            .source_catalog
            .as_deref()
            .unwrap_or(ADTG_PROVIDER_CATALOG);
        let source_schema = field
            .source_schema
            .as_deref()
            .unwrap_or(ADTG_PROVIDER_SCHEMA);
        write_utf16_len_string(
            &mut descriptor,
            source_catalog,
            "ADTG provider catalog name",
        )?;
        write_utf16_len_string(&mut descriptor, source_schema, "ADTG provider schema name")?;
        descriptor.extend_from_slice(&0u16.to_le_bytes());
    } else {
        descriptor.extend_from_slice(&0u16.to_le_bytes());
    }
    descriptor.extend_from_slice(&[0xff, 0xff]);
    if let Some(relation) = &field.chapter_relation {
        if let Some(row_group_id) = row_group_id {
            descriptor.extend_from_slice(&row_group_id.to_le_bytes());
        }
        let relation_len = relation
            .pairs
            .len()
            .checked_mul(12)
            .ok_or_else(|| anyhow!("ADTG chapter relation length overflow"))?;
        descriptor.extend_from_slice(
            &u32::try_from(relation_len)
                .context("ADTG chapter relation length exceeded u32 range")?
                .to_le_bytes(),
        );
        for pair in &relation.pairs {
            descriptor.extend_from_slice(
                &u32::try_from(pair.parent_ordinal)
                    .context("ADTG chapter parent ordinal exceeded u32 range")?
                    .to_le_bytes(),
            );
            descriptor.extend_from_slice(
                &u32::try_from(pair.child_ordinal)
                    .context("ADTG chapter child ordinal exceeded u32 range")?
                    .to_le_bytes(),
            );
            descriptor.extend_from_slice(&0u32.to_le_bytes());
        }
        descriptor.extend_from_slice(&0u32.to_le_bytes());
    } else if let Some(row_group_id) = row_group_id {
        descriptor.extend_from_slice(&row_group_id.to_le_bytes());
    }
    let descriptor_len = descriptor
        .len()
        .checked_sub(3)
        .ok_or_else(|| anyhow!("invalid ADTG descriptor length"))?;
    descriptor[1] = u8::try_from(descriptor_len).context("ADTG descriptor exceeded 255 bytes")?;
    out.extend_from_slice(&descriptor);
    Ok(())
}

fn write_chapter_schema_blocks(
    out: &mut Vec<u8>,
    schema: &AdtgSchema,
    recordsets: &[&Recordset],
    inherited_pending_changes: bool,
) -> Result<()> {
    let changes = chapter_row_changes(recordsets, &schema.fields)?;
    let row_count = changes
        .iter()
        .filter(|change| change.kind != RowChangeKind::Delete)
        .count();
    let provider_shape = schema_has_provider_source(schema);
    let tree_pending_changes =
        inherited_pending_changes || schema_tree_has_pending(schema, recordsets)?;
    let schema_pending_changes = tree_pending_changes || provider_shape;
    out.extend_from_slice(&adtg_shape_header(
        schema,
        row_count,
        schema_pending_changes,
        provider_shape,
    )?);
    if provider_shape {
        write_provider_source_block(out, schema)?;
    }
    for (index, field) in schema.fields.iter().enumerate() {
        write_field_descriptor_inner(out, field, index + 1, schema.row_group_id, provider_shape)?;
    }
    for child in &schema.children {
        let child_recordsets = chapter_recordsets_for_child(recordsets, child.field_index)?;
        write_chapter_schema_blocks(out, &child.schema, &child_recordsets, tree_pending_changes)?;
    }
    Ok(())
}

fn write_chapter_row_groups(
    out: &mut Vec<u8>,
    schema: &AdtgSchema,
    recordsets: &[&Recordset],
    marker: ChapterRowMarker,
    options: &AdtgWriteOptions,
) -> Result<()> {
    for change in chapter_row_changes(recordsets, &schema.fields)? {
        write_chapter_row_change(out, &change, schema, marker, options)?;
    }
    for child in &schema.children {
        let child_recordsets = chapter_recordsets_for_child(recordsets, child.field_index)?;
        let child_group_id = child
            .schema
            .row_group_id
            .ok_or_else(|| anyhow!("ADTG child schema had no row group id"))?;
        write_chapter_row_groups(
            out,
            &child.schema,
            &child_recordsets,
            ChapterRowMarker::Child(child_group_id),
            options,
        )?;
    }
    Ok(())
}

fn write_chapter_row_change(
    out: &mut Vec<u8>,
    change: &ChapterRowChange<'_>,
    schema: &AdtgSchema,
    marker: ChapterRowMarker,
    options: &AdtgWriteOptions,
) -> Result<()> {
    match change.kind {
        RowChangeKind::Current => {
            let row = *change
                .rows
                .first()
                .ok_or_else(|| anyhow!("ADTG current chapter change had no row"))?;
            write_chapter_current_row(out, row, schema, marker, options)?;
        }
        RowChangeKind::Insert => {
            let row = *change
                .rows
                .first()
                .ok_or_else(|| anyhow!("ADTG insert chapter change had no row"))?;
            write_chapter_insert_row(out, row, schema, marker, options)?;
        }
        RowChangeKind::Delete => {
            let row = *change
                .rows
                .first()
                .ok_or_else(|| anyhow!("ADTG delete chapter change had no row"))?;
            write_chapter_current_row(out, row, schema, marker, options)?;
            write_chapter_marker(out, marker, ChapterMarkerKind::Delete);
        }
        RowChangeKind::Update => {
            if change.rows.len() != 2 {
                bail!("ADTG update chapter change requires original and updated rows");
            }
            let original = change.rows[0];
            let updated = change.rows[1];
            write_chapter_current_row(out, original, schema, marker, options)?;
            write_chapter_marker(out, marker, ChapterMarkerKind::Update);
            write_chapter_update_values(out, original, updated, &schema.fields, options)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum ChapterMarkerKind {
    Current,
    Insert,
    Update,
    Delete,
}

fn write_chapter_marker(out: &mut Vec<u8>, marker: ChapterRowMarker, kind: ChapterMarkerKind) {
    match marker {
        ChapterRowMarker::Root => {
            out.push(match kind {
                ChapterMarkerKind::Current => 0x07,
                ChapterMarkerKind::Insert => 0x0d,
                ChapterMarkerKind::Update => 0x0a,
                ChapterMarkerKind::Delete => 0x0c,
            });
        }
        ChapterRowMarker::Child(row_group_id) => {
            out.push(match kind {
                ChapterMarkerKind::Current => 0x87,
                ChapterMarkerKind::Insert => 0x8d,
                ChapterMarkerKind::Update => 0x8a,
                ChapterMarkerKind::Delete => 0x8c,
            });
            out.extend_from_slice(&row_group_id.to_le_bytes());
        }
    }
}

fn write_chapter_current_row(
    out: &mut Vec<u8>,
    row: &Row,
    schema: &AdtgSchema,
    marker: ChapterRowMarker,
    options: &AdtgWriteOptions,
) -> Result<()> {
    write_chapter_marker(out, marker, ChapterMarkerKind::Current);

    let mask_indices = chapter_row_mask_indices(&schema.fields);
    if !mask_indices.is_empty() {
        let mask = mask_bytes(mask_indices.len(), true, |mask_index| {
            let field_index = mask_indices[mask_index];
            schema.fields[field_index].type_code != 136
                && !matches!(row.values[field_index], Value::Null | Value::Unavailable)
        });
        out.extend_from_slice(&mask);
    }

    for (field_index, field) in schema.fields.iter().enumerate() {
        if field.type_code == 136 {
            continue;
        }
        let value = &row.values[field_index];
        if !matches!(value, Value::Null | Value::Unavailable) {
            write_value(out, field, value, options)?;
        } else if !field_allows_null(field) {
            bail!(
                "field {} is non-nullable but chapter row contains null",
                field.name
            );
        }
    }
    Ok(())
}

fn write_chapter_insert_row(
    out: &mut Vec<u8>,
    row: &Row,
    schema: &AdtgSchema,
    marker: ChapterRowMarker,
    options: &AdtgWriteOptions,
) -> Result<()> {
    write_chapter_marker(out, marker, ChapterMarkerKind::Insert);
    let non_null = mask_bytes(schema.fields.len(), false, |index| {
        schema.fields[index].type_code != 136
            && !matches!(row.values[index], Value::Null | Value::Unavailable)
    });
    let nulls = mask_bytes(schema.fields.len(), false, |index| {
        schema.fields[index].type_code != 136 && matches!(row.values[index], Value::Null)
    });
    out.extend_from_slice(&non_null);
    out.extend_from_slice(&nulls);
    write_chapter_present_values(out, row, &schema.fields, options)
}

fn write_chapter_update_values(
    out: &mut Vec<u8>,
    original: &Row,
    updated: &Row,
    fields: &[AdtgField],
    options: &AdtgWriteOptions,
) -> Result<()> {
    let values = fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            if field.type_code == 136 || original.values[index] == updated.values[index] {
                Value::Unavailable
            } else {
                updated.values[index].clone()
            }
        })
        .collect::<Vec<_>>();
    let non_null = mask_bytes(fields.len(), false, |index| {
        !matches!(values[index], Value::Null | Value::Unavailable)
    });
    let nulls = mask_bytes(fields.len(), false, |index| {
        matches!(values[index], Value::Null)
    });
    out.extend_from_slice(&non_null);
    out.extend_from_slice(&nulls);
    for (field, value) in fields.iter().zip(&values) {
        match value {
            Value::Unavailable => {}
            Value::Null if field_allows_null(field) => {}
            Value::Null => bail!(
                "field {} is non-nullable but chapter update contains null",
                field.name
            ),
            value => write_value(out, field, value, options)?,
        }
    }
    Ok(())
}

fn write_chapter_present_values(
    out: &mut Vec<u8>,
    row: &Row,
    fields: &[AdtgField],
    options: &AdtgWriteOptions,
) -> Result<()> {
    for (field_index, field) in fields.iter().enumerate() {
        if field.type_code == 136 {
            continue;
        }
        let value = &row.values[field_index];
        if !matches!(value, Value::Null | Value::Unavailable) {
            write_value(out, field, value, options)?;
        } else if !field_allows_null(field) {
            bail!(
                "field {} is non-nullable but chapter row contains null",
                field.name
            );
        }
    }
    Ok(())
}

fn chapter_recordsets_for_child<'a>(
    recordsets: &[&'a Recordset],
    field_index: usize,
) -> Result<Vec<&'a Recordset>> {
    let mut children = Vec::new();
    for recordset in recordsets {
        for change in &recordset.changes {
            for row_index in &change.row_indices {
                let row = row_at(recordset, *row_index)?;
                match row.values.get(field_index) {
                    Some(Value::Chapter(child)) => children.push(child.as_ref()),
                    Some(Value::Null | Value::Unavailable) => {}
                    Some(other) => bail!(
                        "ADTG chapter field at index {field_index} contained non-chapter value {other:?}"
                    ),
                    None => bail!("ADTG chapter field index {field_index} was missing from row"),
                }
            }
        }
    }
    Ok(children)
}

fn chapter_row_changes<'a>(
    recordsets: &[&'a Recordset],
    fields: &[AdtgField],
) -> Result<Vec<ChapterRowChange<'a>>> {
    let key_indices = chapter_row_key_indices(fields);
    let mut seen = BTreeSet::new();
    let mut changes = Vec::new();
    for recordset in recordsets {
        for change in &recordset.changes {
            let rows = change
                .row_indices
                .iter()
                .map(|row_index| row_at(recordset, *row_index))
                .collect::<Result<Vec<_>>>()?;
            let key = chapter_change_key(change.kind, &rows, &key_indices)?;
            if seen.insert(key) {
                changes.push(ChapterRowChange {
                    kind: change.kind,
                    rows,
                });
            }
        }
    }
    Ok(changes)
}

fn chapter_change_key(
    kind: RowChangeKind,
    rows: &[&Row],
    key_indices: &[usize],
) -> Result<Vec<String>> {
    let mut key = vec![format!("{kind:?}")];
    for row in rows {
        key.extend(chapter_row_key(row, key_indices)?);
    }
    Ok(key)
}

fn chapter_row_key_indices(fields: &[AdtgField]) -> Vec<usize> {
    let key_columns = fields
        .iter()
        .enumerate()
        .filter_map(|(index, field)| {
            (field.type_code != 136 && field.attributes & 0x8000 != 0).then_some(index)
        })
        .collect::<Vec<_>>();
    if !key_columns.is_empty() {
        return key_columns;
    }

    fields
        .iter()
        .enumerate()
        .filter_map(|(index, field)| (field.type_code != 136).then_some(index))
        .collect()
}

fn chapter_row_key(row: &Row, key_indices: &[usize]) -> Result<Vec<String>> {
    key_indices
        .iter()
        .map(|index| {
            row.values
                .get(*index)
                .map(|value| format!("{value:?}"))
                .ok_or_else(|| anyhow!("ADTG chapter row key index {index} was missing"))
        })
        .collect()
}

fn chapter_row_mask_indices(fields: &[AdtgField]) -> Vec<usize> {
    fields
        .iter()
        .enumerate()
        .filter_map(|(index, field)| {
            (field.type_code == 136 || field_allows_null(field)).then_some(index)
        })
        .collect()
}

fn write_rows(
    out: &mut Vec<u8>,
    recordset: &Recordset,
    fields: &[AdtgField],
    options: &AdtgWriteOptions,
) -> Result<()> {
    for change in &recordset.changes {
        match change.kind {
            RowChangeKind::Current => {
                for row_index in &change.row_indices {
                    let row = row_at(recordset, *row_index)?;
                    write_current_or_original_row(out, row, fields, options)?;
                }
            }
            RowChangeKind::Insert => {
                for row_index in &change.row_indices {
                    let row = row_at(recordset, *row_index)?;
                    write_insert_row(out, row, fields, options)?;
                }
            }
            RowChangeKind::Delete => {
                for row_index in &change.row_indices {
                    let row = row_at(recordset, *row_index)?;
                    write_current_or_original_row(out, row, fields, options)?;
                    out.push(0x0c);
                }
            }
            RowChangeKind::Update => {
                if change.row_indices.len() != 2 {
                    bail!("ADTG update change must have original and updated rows");
                }
                let original = row_at(recordset, change.row_indices[0])?;
                let updated = row_at(recordset, change.row_indices[1])?;
                write_current_or_original_row(out, original, fields, options)?;
                out.push(0x0a);
                write_update_values(out, updated, fields, options)?;
            }
        }
    }
    Ok(())
}

fn row_at(recordset: &Recordset, index: usize) -> Result<&Row> {
    recordset
        .rows
        .get(index)
        .ok_or_else(|| anyhow!("ADTG change referenced invalid row {index}"))
}

fn write_current_or_original_row(
    out: &mut Vec<u8>,
    row: &Row,
    fields: &[AdtgField],
    options: &AdtgWriteOptions,
) -> Result<()> {
    out.push(0x07);
    let nullable_indices = nullable_field_indices(fields);
    if !nullable_indices.is_empty() {
        let mask = mask_bytes(nullable_indices.len(), true, |nullable_index| {
            !matches!(
                row.values[nullable_indices[nullable_index]],
                Value::Null | Value::Unavailable
            )
        });
        out.extend_from_slice(&mask);
    }
    for (field, value) in fields.iter().zip(&row.values) {
        if !matches!(value, Value::Null | Value::Unavailable) {
            write_value(out, field, value, options)?;
        } else if !field_allows_null(field) {
            bail!("field {} is non-nullable but row contains null", field.name);
        }
    }
    Ok(())
}

fn write_insert_row(
    out: &mut Vec<u8>,
    row: &Row,
    fields: &[AdtgField],
    options: &AdtgWriteOptions,
) -> Result<()> {
    out.push(0x0d);
    let non_null = mask_bytes(fields.len(), false, |index| {
        !matches!(row.values[index], Value::Null | Value::Unavailable)
    });
    let nulls = mask_bytes(fields.len(), false, |index| {
        matches!(row.values[index], Value::Null)
    });
    out.extend_from_slice(&non_null);
    out.extend_from_slice(&nulls);
    for (field, value) in fields.iter().zip(&row.values) {
        match value {
            Value::Null if field_allows_null(field) => {}
            Value::Null => bail!(
                "field {} is non-nullable but insert contains null",
                field.name
            ),
            Value::Unavailable => {
                bail!("insert row cannot contain unavailable field {}", field.name)
            }
            value => write_value(out, field, value, options)?,
        }
    }
    Ok(())
}

fn write_update_values(
    out: &mut Vec<u8>,
    row: &Row,
    fields: &[AdtgField],
    options: &AdtgWriteOptions,
) -> Result<()> {
    let non_null = mask_bytes(fields.len(), false, |index| {
        !matches!(row.values[index], Value::Null | Value::Unavailable)
    });
    let nulls = mask_bytes(fields.len(), false, |index| {
        matches!(row.values[index], Value::Null)
    });
    out.extend_from_slice(&non_null);
    out.extend_from_slice(&nulls);
    for (field, value) in fields.iter().zip(&row.values) {
        match value {
            Value::Unavailable => {}
            Value::Null if field_allows_null(field) => {}
            Value::Null => bail!(
                "field {} is non-nullable but update contains null",
                field.name
            ),
            value => write_value(out, field, value, options)?,
        }
    }
    Ok(())
}

fn nullable_field_indices(fields: &[AdtgField]) -> Vec<usize> {
    fields
        .iter()
        .enumerate()
        .filter_map(|(index, field)| field_allows_null(field).then_some(index))
        .collect()
}

fn field_allows_null(field: &AdtgField) -> bool {
    field.attributes & (0x20 | 0x40) != 0
}

fn mask_bytes<F>(bit_count: usize, pad_ones: bool, mut bit: F) -> Vec<u8>
where
    F: FnMut(usize) -> bool,
{
    let byte_count = bit_count.div_ceil(8).max(1);
    let mut bytes = vec![if pad_ones { 0xff } else { 0x00 }; byte_count];
    for index in 0..bit_count {
        let mask = 0x80 >> (index % 8);
        if bit(index) {
            bytes[index / 8] |= mask;
        } else {
            bytes[index / 8] &= !mask;
        }
    }
    bytes
}

fn write_value(
    out: &mut Vec<u8>,
    field: &AdtgField,
    value: &Value,
    options: &AdtgWriteOptions,
) -> Result<()> {
    match field.type_code {
        2 => write_i16(
            out,
            integer_range(value, i16::MIN as i64, i16::MAX as i64, &field.name)? as i16,
        ),
        3 => write_i32(
            out,
            integer_range(value, i32::MIN as i64, i32::MAX as i64, &field.name)? as i32,
        ),
        4 => write_single(out, field, float_value(value, &field.name)?)?,
        5 => write_double(out, field, float_value(value, &field.name)?)?,
        6 => write_i64(out, scaled_decimal_i128(value, 4, &field.name)? as i64),
        7 => write_f64(out, ole_datetime_value(value, &field.name)?),
        11 => write_i16(
            out,
            if boolean_value(value, &field.name)? {
                -1
            } else {
                0
            },
        ),
        12 => write_variant(out, value, &field.name)?,
        14 => write_decimal(out, field, value)?,
        16 => write_i8(
            out,
            integer_range(value, i8::MIN as i64, i8::MAX as i64, &field.name)? as i8,
        ),
        17 => write_u8(
            out,
            unsigned_range(value, u8::MAX as u64, &field.name)? as u8,
        ),
        18 => write_u16(
            out,
            unsigned_range(value, u16::MAX as u64, &field.name)? as u16,
        ),
        19 => write_u32(
            out,
            unsigned_range(value, u32::MAX as u64, &field.name)? as u32,
        ),
        20 => write_i64(out, integer_value(value, &field.name)?),
        21 => write_u64(out, unsigned_value(value, &field.name)?),
        64 => write_u64(out, filetime_value(value, &field.name)?),
        72 => out.extend_from_slice(&guid_bytes(value, &field.name)?),
        128 => write_binary(out, field, value)?,
        129 | 130 => write_text(out, field, value, options)?,
        131 => write_numeric(out, field, value)?,
        133 => write_dbdate(out, value, &field.name)?,
        134 => write_dbtime(out, value, &field.name)?,
        135 => write_dbtimestamp(out, value, &field.name)?,
        139 => write_varnumeric(out, field, value)?,
        other => bail!("ADTG writer does not support field type {other}"),
    }
    Ok(())
}

fn write_single(out: &mut Vec<u8>, field: &AdtgField, value: f64) -> Result<()> {
    if field.defined_size < 4 {
        bail!("ADTG adSingle field {} width is too small", field.name);
    }
    let value = value as f32;
    if !value.is_finite() {
        bail!("non-finite ADTG adSingle value for field {}", field.name);
    }
    out.extend_from_slice(&value.to_le_bytes());
    out.extend(std::iter::repeat_n(
        0,
        field.defined_size.saturating_sub(4) as usize,
    ));
    Ok(())
}

fn write_double(out: &mut Vec<u8>, field: &AdtgField, value: f64) -> Result<()> {
    if !value.is_finite() {
        bail!("non-finite ADTG adDouble value for field {}", field.name);
    }
    match field.defined_size {
        4 => out.extend_from_slice(&(value as f32).to_le_bytes()),
        8 => out.extend_from_slice(&value.to_le_bytes()),
        other => bail!(
            "unsupported ADTG adDouble width {other} for field {}",
            field.name
        ),
    }
    Ok(())
}

fn write_binary(out: &mut Vec<u8>, field: &AdtgField, value: &Value) -> Result<()> {
    let Value::BinaryHex(text) = value else {
        bail!("field {} expected binary value", field.name);
    };
    let mut bytes =
        hex::decode(text).with_context(|| format!("field {} had invalid hex", field.name))?;
    for byte in &mut bytes {
        *byte = adtg_binary_preimage(*byte)
            .with_context(|| format!("field {} contains an ADTG-binary byte that cannot roundtrip through the native parser", field.name))?;
    }
    write_variable_bytes(out, field, bytes)
}

fn write_text(
    out: &mut Vec<u8>,
    field: &AdtgField,
    value: &Value,
    options: &AdtgWriteOptions,
) -> Result<()> {
    let Value::String(text) = value else {
        bail!("field {} expected string value", field.name);
    };
    let bytes = if field.type_code == 130 {
        utf16le_bytes(text)
    } else {
        ansi_bytes(text, options.ansi_encoding)
            .with_context(|| format!("failed to encode field {} as ANSI", field.name))?
    };
    write_variable_bytes(out, field, bytes)
}

fn write_variable_bytes(out: &mut Vec<u8>, field: &AdtgField, mut bytes: Vec<u8>) -> Result<()> {
    let is_fixed = field.attributes & 0x10 != 0;
    let is_long = field.attributes & 0x80 != 0;
    let max_value_bytes = if field.type_code == 130 {
        field.defined_size.saturating_mul(2)
    } else {
        field.defined_size
    } as usize;

    if is_fixed && !is_long && max_value_bytes <= 255 {
        if bytes.len() > max_value_bytes {
            bail!(
                "field {} value length {} exceeds fixed width {}",
                field.name,
                bytes.len(),
                max_value_bytes
            );
        }
        if bytes.len() < max_value_bytes {
            bytes.resize(max_value_bytes, 0);
        }
        out.extend_from_slice(&bytes);
        return Ok(());
    }

    if !is_long && bytes.len() > max_value_bytes {
        bail!(
            "field {} value length {} exceeds max width {}",
            field.name,
            bytes.len(),
            max_value_bytes
        );
    }
    if max_value_bytes > 255 {
        write_u32(out, bytes.len() as u32);
    } else {
        write_u8(out, bytes.len() as u8);
    }
    out.extend_from_slice(&bytes);
    Ok(())
}

fn write_decimal(out: &mut Vec<u8>, field: &AdtgField, value: &Value) -> Result<()> {
    let descriptor_scale = (field.scale != 255).then_some(field.scale);
    let descriptor_precision = (field.precision != 255).then_some(field.precision as u8);
    let decimal = decimal_parts(value, descriptor_scale, descriptor_precision, &field.name)?;
    write_decimal_bytes(out, 0, decimal.scale, decimal.negative, decimal.magnitude)
}

fn write_numeric(out: &mut Vec<u8>, field: &AdtgField, value: &Value) -> Result<()> {
    let precision = u8::try_from(field.precision).context("numeric precision exceeded u8")?;
    let scale = field.scale;
    let decimal = decimal_parts(value, Some(scale), Some(precision), &field.name)?;
    out.push(precision);
    out.push(u8::try_from(scale).context("numeric scale exceeded u8")?);
    out.push(if decimal.negative { 0 } else { 1 });
    out.extend_from_slice(&decimal.magnitude.to_le_bytes());
    Ok(())
}

fn write_varnumeric(out: &mut Vec<u8>, field: &AdtgField, value: &Value) -> Result<()> {
    let descriptor_scale = (field.scale <= 38).then_some(field.scale);
    let descriptor_precision = (1..=38)
        .contains(&field.precision)
        .then_some(field.precision as u8);
    let decimal = decimal_parts(value, descriptor_scale, descriptor_precision, &field.name)?;
    let scale = descriptor_scale.unwrap_or(decimal.scale);
    let precision = descriptor_precision.unwrap_or_else(|| decimal_precision(decimal));
    if precision > 38 {
        bail!(
            "field {} varnumeric precision {precision} exceeds 38",
            field.name
        );
    }
    if scale > precision as u32 {
        bail!(
            "field {} varnumeric scale {scale} exceeds precision {precision}",
            field.name
        );
    }
    let mut raw = Vec::with_capacity(19);
    raw.push(precision);
    raw.push(u8::try_from(scale).context("varnumeric scale exceeded u8")?);
    raw.push(if decimal.negative { 0 } else { 1 });
    let magnitude_bytes = decimal.magnitude.to_le_bytes();
    let magnitude_len = magnitude_bytes
        .iter()
        .rposition(|byte| *byte != 0)
        .map(|index| index + 1)
        .unwrap_or(1);
    raw.extend_from_slice(&magnitude_bytes[..magnitude_len]);
    write_variable_bytes(out, field, raw)
}

fn decimal_precision(decimal: DecimalParts) -> u8 {
    let digits = if decimal.magnitude == 0 {
        1
    } else {
        decimal.magnitude.ilog10() + 1
    };
    digits.max(decimal.scale.max(1)) as u8
}

#[derive(Debug, Clone, Copy)]
struct DecimalParts {
    negative: bool,
    magnitude: u128,
    scale: u32,
}

fn decimal_parts(
    value: &Value,
    expected_scale: Option<u32>,
    precision: Option<u8>,
    field_name: &str,
) -> Result<DecimalParts> {
    let text = match value {
        Value::Decimal(text) | Value::String(text) => text,
        Value::Integer(value) => {
            return scaled_integer_parts(*value, expected_scale, precision, field_name)
        }
        Value::UnsignedInteger(value) => {
            return scaled_unsigned_parts(*value, expected_scale, precision, field_name);
        }
        _ => bail!("field {field_name} expected decimal-compatible value"),
    };
    parse_decimal_text(text, expected_scale, precision, field_name)
}

fn scaled_integer_parts(
    value: i64,
    expected_scale: Option<u32>,
    precision: Option<u8>,
    field_name: &str,
) -> Result<DecimalParts> {
    let scale = expected_scale.unwrap_or(0);
    let factor = checked_pow10(scale)?;
    let magnitude = (value as i128)
        .unsigned_abs()
        .checked_mul(factor)
        .ok_or_else(|| anyhow!("field {field_name} decimal magnitude overflow"))?;
    validate_precision(magnitude, precision, field_name)?;
    Ok(DecimalParts {
        negative: value < 0,
        magnitude,
        scale,
    })
}

fn scaled_unsigned_parts(
    value: u64,
    expected_scale: Option<u32>,
    precision: Option<u8>,
    field_name: &str,
) -> Result<DecimalParts> {
    let scale = expected_scale.unwrap_or(0);
    let factor = checked_pow10(scale)?;
    let magnitude = (value as u128)
        .checked_mul(factor)
        .ok_or_else(|| anyhow!("field {field_name} decimal magnitude overflow"))?;
    validate_precision(magnitude, precision, field_name)?;
    Ok(DecimalParts {
        negative: false,
        magnitude,
        scale,
    })
}

fn parse_decimal_text(
    text: &str,
    expected_scale: Option<u32>,
    precision: Option<u8>,
    field_name: &str,
) -> Result<DecimalParts> {
    let text = text.trim();
    let (negative, body) = text
        .strip_prefix('-')
        .map(|body| (true, body))
        .unwrap_or((false, text));
    let body = body.strip_prefix('+').unwrap_or(body);
    let (whole, fraction) = body.split_once('.').unwrap_or((body, ""));
    if whole.is_empty() && fraction.is_empty() {
        bail!("field {field_name} had empty decimal text");
    }
    if !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        bail!("field {field_name} had invalid decimal text {text:?}");
    }
    let actual_scale = fraction.len() as u32;
    let scale = expected_scale.unwrap_or(actual_scale);
    if actual_scale > scale {
        bail!("field {field_name} decimal scale {actual_scale} exceeds descriptor scale {scale}");
    }
    let digits = format!(
        "{whole}{fraction}{:0<pad$}",
        "",
        pad = (scale - actual_scale) as usize
    );
    let magnitude = digits.trim_start_matches('0').parse::<u128>().unwrap_or(0);
    validate_precision(magnitude, precision, field_name)?;
    Ok(DecimalParts {
        negative: negative && magnitude != 0,
        magnitude,
        scale,
    })
}

fn scaled_decimal_i128(value: &Value, scale: u32, field_name: &str) -> Result<i128> {
    let decimal = decimal_parts(value, Some(scale), None, field_name)?;
    let magnitude = i128::try_from(decimal.magnitude)
        .with_context(|| format!("field {field_name} decimal magnitude exceeded i128"))?;
    Ok(if decimal.negative {
        -magnitude
    } else {
        magnitude
    })
}

fn validate_precision(magnitude: u128, precision: Option<u8>, field_name: &str) -> Result<()> {
    if let Some(precision) = precision {
        if magnitude >= checked_pow10(precision as u32)? {
            bail!("field {field_name} decimal magnitude exceeds precision {precision}");
        }
    }
    Ok(())
}

fn checked_pow10(scale: u32) -> Result<u128> {
    10u128
        .checked_pow(scale)
        .ok_or_else(|| anyhow!("decimal scale {scale} is too large"))
}

fn write_decimal_bytes(
    out: &mut Vec<u8>,
    reserved: u16,
    scale: u32,
    negative: bool,
    magnitude: u128,
) -> Result<()> {
    if scale > 28 {
        bail!("ADTG decimal scale {scale} exceeds 28");
    }
    let bytes = magnitude.to_le_bytes();
    out.extend_from_slice(&reserved.to_le_bytes());
    out.push(scale as u8);
    out.push(if negative { 0x80 } else { 0x00 });
    out.extend_from_slice(&bytes[8..12]);
    out.extend_from_slice(&bytes[0..8]);
    Ok(())
}

fn write_variant(out: &mut Vec<u8>, value: &Value, field_name: &str) -> Result<()> {
    let mut raw = Vec::with_capacity(16);
    match value {
        Value::Empty => raw.resize(16, 0),
        Value::Null => {
            raw.extend_from_slice(&1u16.to_le_bytes());
            raw.resize(16, 0);
        }
        Value::Boolean(value) => {
            raw.extend_from_slice(&11u16.to_le_bytes());
            raw.resize(8, 0);
            raw.extend_from_slice(&(if *value { -1i16 } else { 0i16 }).to_le_bytes());
            raw.resize(16, 0);
        }
        Value::DateTime(_) | Value::Date(_) => {
            raw.extend_from_slice(&7u16.to_le_bytes());
            raw.resize(8, 0);
            raw.extend_from_slice(&ole_datetime_value(value, field_name)?.to_le_bytes());
        }
        Value::Integer(value) if i32::try_from(*value).is_ok() => {
            raw.extend_from_slice(&3u16.to_le_bytes());
            raw.resize(8, 0);
            raw.extend_from_slice(&(*value as i32).to_le_bytes());
            raw.resize(16, 0);
        }
        Value::Integer(value) => {
            raw.extend_from_slice(&20u16.to_le_bytes());
            raw.resize(8, 0);
            raw.extend_from_slice(&value.to_le_bytes());
        }
        Value::UnsignedInteger(value) if u32::try_from(*value).is_ok() => {
            raw.extend_from_slice(&19u16.to_le_bytes());
            raw.resize(8, 0);
            raw.extend_from_slice(&(*value as u32).to_le_bytes());
            raw.resize(16, 0);
        }
        Value::UnsignedInteger(value) => {
            raw.extend_from_slice(&21u16.to_le_bytes());
            raw.resize(8, 0);
            raw.extend_from_slice(&value.to_le_bytes());
        }
        Value::Float(value) => {
            if !value.is_finite() {
                bail!("non-finite variant float for field {field_name}");
            }
            raw.extend_from_slice(&5u16.to_le_bytes());
            raw.resize(8, 0);
            raw.extend_from_slice(&value.to_le_bytes());
        }
        Value::Decimal(_) | Value::String(_) => {
            let decimal = decimal_parts(value, None, Some(28), field_name)?;
            write_decimal_bytes(
                &mut raw,
                14,
                decimal.scale,
                decimal.negative,
                decimal.magnitude,
            )?;
        }
        _ => bail!("field {field_name} value cannot be written as fixed ADTG variant"),
    }
    if raw.len() != 16 {
        bail!("internal variant writer produced {} bytes", raw.len());
    }
    out.extend_from_slice(&raw);
    Ok(())
}

fn integer_value(value: &Value, field_name: &str) -> Result<i64> {
    match value {
        Value::Integer(value) => Ok(*value),
        _ => bail!("field {field_name} expected signed integer value"),
    }
}

fn integer_range(value: &Value, min: i64, max: i64, field_name: &str) -> Result<i64> {
    let value = integer_value(value, field_name)?;
    if !(min..=max).contains(&value) {
        bail!("field {field_name} integer value {value} outside range {min}..={max}");
    }
    Ok(value)
}

fn unsigned_value(value: &Value, field_name: &str) -> Result<u64> {
    match value {
        Value::UnsignedInteger(value) => Ok(*value),
        Value::Integer(value) if *value >= 0 => Ok(*value as u64),
        _ => bail!("field {field_name} expected unsigned integer value"),
    }
}

fn unsigned_range(value: &Value, max: u64, field_name: &str) -> Result<u64> {
    let value = unsigned_value(value, field_name)?;
    if value > max {
        bail!("field {field_name} unsigned value {value} exceeds {max}");
    }
    Ok(value)
}

fn float_value(value: &Value, field_name: &str) -> Result<f64> {
    match value {
        Value::Float(value) if value.is_finite() => Ok(*value),
        Value::Float(_) => bail!("field {field_name} had non-finite float"),
        _ => bail!("field {field_name} expected float value"),
    }
}

fn boolean_value(value: &Value, field_name: &str) -> Result<bool> {
    match value {
        Value::Boolean(value) => Ok(*value),
        _ => bail!("field {field_name} expected boolean value"),
    }
}

fn ole_datetime_value(value: &Value, field_name: &str) -> Result<f64> {
    let text = match value {
        Value::DateTime(text) | Value::Date(text) | Value::String(text) => text,
        _ => bail!("field {field_name} expected date/time value"),
    };
    let (date, time) = split_datetime(text)?;
    let days = days_from_civil(date.0 as i128, date.1 as i128, date.2 as i128) + 25_569;
    let seconds = time
        .map(|(hour, minute, second, _)| hour as i128 * 3600 + minute as i128 * 60 + second as i128)
        .unwrap_or(0);
    let fraction = seconds as f64 / 86_400.0;
    if days < 0 && seconds > 0 {
        Ok(days as f64 - fraction)
    } else {
        Ok(days as f64 + fraction)
    }
}

fn filetime_value(value: &Value, field_name: &str) -> Result<u64> {
    let text = match value {
        Value::DateTime(text) | Value::String(text) => text,
        _ => bail!("field {field_name} expected FILETIME date/time value"),
    };
    let (date, time) = split_datetime(text)?;
    let days = days_from_civil(date.0 as i128, date.1 as i128, date.2 as i128);
    let (hour, minute, second, _) = time.unwrap_or((0, 0, 0, 0));
    let unix_seconds = days
        .checked_mul(86_400)
        .and_then(|value| {
            value.checked_add(hour as i128 * 3600 + minute as i128 * 60 + second as i128)
        })
        .ok_or_else(|| anyhow!("field {field_name} FILETIME overflow"))?;
    let filetime_seconds = unix_seconds + 11_644_473_600i128;
    if filetime_seconds < 0 {
        bail!("field {field_name} FILETIME is before 1601-01-01");
    }
    Ok((filetime_seconds as u64) * 10_000_000)
}

fn write_dbdate(out: &mut Vec<u8>, value: &Value, field_name: &str) -> Result<()> {
    let text = match value {
        Value::Date(text) | Value::DateTime(text) | Value::String(text) => text,
        _ => bail!("field {field_name} expected date value"),
    };
    let (date, _) = split_datetime(text)?;
    write_u16(out, date.0);
    write_u16(out, date.1);
    write_u16(out, date.2);
    Ok(())
}

fn write_dbtime(out: &mut Vec<u8>, value: &Value, field_name: &str) -> Result<()> {
    let text = match value {
        Value::Time(text) | Value::String(text) => text,
        _ => bail!("field {field_name} expected time value"),
    };
    let (hour, minute, second, _) = parse_time(text)?;
    write_u16(out, hour);
    write_u16(out, minute);
    write_u16(out, second);
    Ok(())
}

fn write_dbtimestamp(out: &mut Vec<u8>, value: &Value, field_name: &str) -> Result<()> {
    let text = match value {
        Value::DateTime(text) | Value::String(text) => text,
        _ => bail!("field {field_name} expected timestamp value"),
    };
    let (date, time) = split_datetime(text)?;
    let (hour, minute, second, fraction) = time.unwrap_or((0, 0, 0, 0));
    write_u16(out, date.0);
    write_u16(out, date.1);
    write_u16(out, date.2);
    write_u16(out, hour);
    write_u16(out, minute);
    write_u16(out, second);
    write_u32(out, fraction);
    Ok(())
}

type DateParts = (u16, u16, u16);
type TimeParts = (u16, u16, u16, u32);

fn split_datetime(text: &str) -> Result<(DateParts, Option<TimeParts>)> {
    let (date, time) = text
        .split_once('T')
        .or_else(|| text.split_once(' '))
        .map(|(date, time)| (date, Some(time)))
        .unwrap_or((text, None));
    let date = parse_date(date)?;
    let time = time.map(parse_time).transpose()?;
    Ok((date, time))
}

fn parse_date(text: &str) -> Result<(u16, u16, u16)> {
    let parts = text.split('-').collect::<Vec<_>>();
    if parts.len() != 3 {
        bail!("invalid ADTG date text {text:?}");
    }
    let year = parts[0].parse::<u16>()?;
    let month = parts[1].parse::<u16>()?;
    let day = parts[2].parse::<u16>()?;
    validate_date(year, month, day)?;
    Ok((year, month, day))
}

fn parse_time(text: &str) -> Result<(u16, u16, u16, u32)> {
    let (head, fraction) = text.split_once('.').unwrap_or((text, ""));
    let parts = head.split(':').collect::<Vec<_>>();
    if parts.len() != 3 {
        bail!("invalid ADTG time text {text:?}");
    }
    let hour = parts[0].parse::<u16>()?;
    let minute = parts[1].parse::<u16>()?;
    let second = parts[2].parse::<u16>()?;
    validate_time(hour, minute, second)?;
    let fraction = if fraction.is_empty() {
        0
    } else {
        if fraction.len() > 9 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
            bail!("invalid ADTG timestamp fraction {fraction:?}");
        }
        let mut padded = fraction.to_string();
        while padded.len() < 9 {
            padded.push('0');
        }
        padded.parse::<u32>()?
    };
    Ok((hour, minute, second, fraction))
}

fn validate_date(year: u16, month: u16, day: u16) -> Result<()> {
    if !(1..=9999).contains(&year) {
        bail!("invalid ADTG date year {year}");
    }
    let Some(max_day) = gregorian_month_len(year, month) else {
        bail!("invalid ADTG date month {month}");
    };
    if !(1..=max_day).contains(&day) {
        bail!("invalid ADTG date day {day}");
    }
    Ok(())
}

fn validate_time(hour: u16, minute: u16, second: u16) -> Result<()> {
    if hour > 23 || minute > 59 || second > 59 {
        bail!("invalid ADTG time {hour:02}:{minute:02}:{second:02}");
    }
    Ok(())
}

fn days_from_civil(year: i128, month: i128, day: i128) -> i128 {
    let year = year - (month <= 2) as i128;
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let mp = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn guid_bytes(value: &Value, field_name: &str) -> Result<[u8; 16]> {
    let text = match value {
        Value::Guid(text) | Value::String(text) => text,
        _ => bail!("field {field_name} expected GUID value"),
    };
    let text = text.trim_matches(|ch| ch == '{' || ch == '}');
    let parts = text.split('-').collect::<Vec<_>>();
    if parts.len() != 5 {
        bail!("field {field_name} had invalid GUID {text:?}");
    }
    let d1 = u32::from_str_radix(parts[0], 16)?;
    let d2 = u16::from_str_radix(parts[1], 16)?;
    let d3 = u16::from_str_radix(parts[2], 16)?;
    if parts[3].len() != 4 || parts[4].len() != 12 {
        bail!("field {field_name} had invalid GUID {text:?}");
    }
    let mut out = [0u8; 16];
    out[0..4].copy_from_slice(&d1.to_le_bytes());
    out[4..6].copy_from_slice(&d2.to_le_bytes());
    out[6..8].copy_from_slice(&d3.to_le_bytes());
    out[8] = u8::from_str_radix(&parts[3][0..2], 16)?;
    out[9] = u8::from_str_radix(&parts[3][2..4], 16)?;
    for index in 0..6 {
        out[10 + index] = u8::from_str_radix(&parts[4][index * 2..index * 2 + 2], 16)?;
    }
    Ok(out)
}

fn utf16le_bytes(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 2);
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

fn ansi_bytes(text: &str, encoding: &'static Encoding) -> Result<Vec<u8>> {
    if text.is_ascii() {
        return Ok(text.as_bytes().to_vec());
    }
    let (encoded, _encoding, had_errors) = encoding.encode(text);
    if had_errors {
        bail!("text cannot be represented in {}/ANSI", encoding.name());
    }
    Ok(encoded.into_owned())
}

fn adtg_binary_preimage(byte: u8) -> Result<u8> {
    let mapped = match byte {
        0xAC => 0x80,
        0x1A => 0x82,
        0x92 => 0x83,
        0x1E => 0x84,
        0x26 => 0x85,
        0x20 => 0x86,
        0x21 => 0x87,
        0xC6 => 0x88,
        0x30 => 0x89,
        0x60 => 0x8A,
        0x39 => 0x8B,
        0x52 => 0x8C,
        0x7D => 0x8E,
        0x18 => 0x91,
        0x19 => 0x92,
        0x1C => 0x93,
        0x1D => 0x94,
        0x22 => 0x95,
        0x13 => 0x96,
        0x14 => 0x97,
        0xDC => 0x98,
        0x61 => 0x9A,
        0x3A => 0x9B,
        0x53 => 0x9C,
        0x7E => 0x9E,
        0x78 => 0x9F,
        other
            if matches!(
                other,
                0x80 | 0x82
                    | 0x83
                    | 0x84
                    | 0x85
                    | 0x86
                    | 0x87
                    | 0x88
                    | 0x89
                    | 0x8A
                    | 0x8B
                    | 0x8C
                    | 0x8E
                    | 0x91
                    | 0x93
                    | 0x94
                    | 0x95
                    | 0x96
                    | 0x97
                    | 0x98
                    | 0x99
                    | 0x9A
                    | 0x9B
                    | 0x9C
                    | 0x9E
                    | 0x9F
            ) =>
        {
            bail!("byte 0x{other:02X} is normalized by ADTG binary parsing")
        }
        other => other,
    };
    Ok(mapped)
}

fn write_i8(out: &mut Vec<u8>, value: i8) {
    out.push(value as u8);
}

fn write_u8(out: &mut Vec<u8>, value: u8) {
    out.push(value);
}

fn write_i16(out: &mut Vec<u8>, value: i16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_i64(out: &mut Vec<u8>, value: i64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_f64(out: &mut Vec<u8>, value: f64) {
    out.extend_from_slice(&value.to_le_bytes());
}
