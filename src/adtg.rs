//! Parser and inspector for ADO's binary ADTG persistence format.
//!
//! ADTG is undocumented, so this module keeps the binary schema, row groups,
//! chapter materialization, and marker-byte handling close together. The parser
//! preserves MDAC-observed details such as descriptor flags and row-state groups
//! because writer and oracle comparisons depend on those bytes.

use anyhow::{anyhow, bail, Context, Result};
use encoding_rs::{Encoding, EUC_KR};
use serde::Serialize;
use std::path::Path;

use crate::detect::{detect_format, RecordsetFormat};
use crate::model::{
    AdoDataType, ChapterRelation, ChapterRelationPair, Field, FieldAttribute, RecordStatusFlag,
    Recordset, Row, RowChange, RowChangeKind, RowState, Value,
};
use crate::util::{gregorian_month_len, overlay_unavailable_values};
use crate::ResourceLimits;

/// Lightweight ADTG inspection output used by CLI/debug tooling.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AdtgDocument {
    /// Total input length in bytes.
    pub length: usize,
    /// Hex dump of the first bytes of the stream.
    pub header_hex: String,
    /// Hex dump of the last bytes of the stream.
    pub trailer_hex: String,
    /// First little-endian `u32`, when present.
    pub first_u32_le: Option<u32>,
    /// Human-readable strings detected in the binary stream.
    pub detected_strings: Vec<DetectedString>,
}

/// String detected while inspecting ADTG bytes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DetectedString {
    /// Byte offset where the string was detected.
    pub offset: usize,
    /// Encoding heuristic used for the string.
    pub encoding: DetectedEncoding,
    /// Decoded text.
    pub text: String,
}

/// Encoding used for an inspected string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectedEncoding {
    /// ASCII text.
    Ascii,
    /// UTF-16 little-endian text.
    Utf16Le,
    /// ANSI text decoded with the Korean ADTG corpus heuristic.
    KoreanAnsi,
}

pub const SUPPORTED_ADTG_DESCRIPTOR_TYPE_CODES: &[u16] = &[
    2, 3, 4, 5, 6, 7, 11, 12, 14, 16, 17, 18, 19, 20, 21, 64, 72, 128, 129, 130, 131, 133, 134,
    135, 136, 139,
];

pub const SUPPORTED_NATIVE_ADTG_ADO_TYPE_CODES: &[u16] = &[
    2, 3, 4, 5, 6, 7, 11, 12, 14, 16, 17, 18, 19, 20, 21, 64, 72, 128, 129, 130, 131, 133, 134,
    135, 136, 139, 200, 201, 202, 203, 204, 205,
];

/// Options for ADTG parsing.
///
/// MDAC persists `adChar`/`adVarChar`/`adLongVarChar` bytes without an embedded
/// codepage. The default tries UTF-8 first, then decodes with EUC-KR for Korean
/// corpus compatibility.
#[derive(Clone, Copy)]
pub struct AdtgParseOptions {
    /// ANSI codepage used when UTF-8 is disabled or fails.
    pub ansi_encoding: Option<&'static Encoding>,
    /// Whether to try UTF-8 before [`Self::ansi_encoding`] for ANSI fields.
    pub utf8_first_for_ansi: bool,
    /// Resource limits enforced while parsing ADTG.
    pub resource_limits: ResourceLimits,
}

impl Default for AdtgParseOptions {
    fn default() -> Self {
        Self {
            ansi_encoding: Some(EUC_KR),
            utf8_first_for_ansi: true,
            resource_limits: ResourceLimits::default(),
        }
    }
}

impl AdtgParseOptions {
    /// Use a specific ANSI encoding for non-Unicode text fields.
    pub fn with_ansi_encoding(mut self, encoding: &'static Encoding) -> Self {
        self.ansi_encoding = Some(encoding);
        self
    }

    /// Use an [`encoding_rs`] label such as `b"windows-1252"` for ANSI fields.
    pub fn with_ansi_encoding_label(mut self, label: &[u8]) -> Option<Self> {
        self.ansi_encoding = Some(Encoding::for_label(label)?);
        Some(self)
    }

    /// Disable ANSI fallback decoding.
    pub fn without_ansi_encoding(mut self) -> Self {
        self.ansi_encoding = None;
        self
    }

    /// Control whether ANSI byte fields are attempted as UTF-8 first.
    pub fn with_utf8_first_for_ansi(mut self, enabled: bool) -> Self {
        self.utf8_first_for_ansi = enabled;
        self
    }

    /// Replace ADTG parser resource limits.
    pub fn with_resource_limits(mut self, limits: ResourceLimits) -> Self {
        self.resource_limits = limits;
        self
    }
}

pub fn is_supported_adtg_descriptor_type(type_code: u16) -> bool {
    SUPPORTED_ADTG_DESCRIPTOR_TYPE_CODES.contains(&type_code)
}

pub fn adtg_descriptor_type_codes(bytes: &[u8]) -> Result<Vec<u16>> {
    if detect_format(bytes) == RecordsetFormat::Xml {
        bail!("input looks like ADO XML, not ADTG");
    }

    let limits = ResourceLimits::default();
    limits.check_input_bytes(bytes.len(), "ADTG input")?;
    let (descriptors, offset) = parse_field_descriptors(bytes, limits)?;
    let mut type_codes = Vec::new();
    collect_descriptor_type_codes(bytes, offset, &descriptors, &mut type_codes, limits, 0)?;
    Ok(type_codes)
}

fn collect_descriptor_type_codes(
    bytes: &[u8],
    mut offset: usize,
    descriptors: &[FieldDescriptor],
    out: &mut Vec<u16>,
    limits: ResourceLimits,
    depth: usize,
) -> Result<usize> {
    validate_chapter_depth("ADTG descriptor scan", depth)?;
    out.extend(descriptors.iter().map(|descriptor| descriptor.type_code));
    for _ in descriptors
        .iter()
        .filter(|descriptor| descriptor.type_code == 136)
    {
        let Some((child_descriptors, child_offset, _child_block_start)) =
            find_field_descriptor_block(bytes, offset, limits)?
        else {
            bail!("chaptered ADTG child field descriptors were not found");
        };
        offset = collect_descriptor_type_codes(
            bytes,
            child_offset,
            &child_descriptors,
            out,
            limits,
            depth + 1,
        )?;
    }

    Ok(offset)
}

fn validate_chapter_depth(context: &str, depth: usize) -> Result<()> {
    if depth > crate::MAX_RECORDSET_DEPTH {
        bail!(
            "{context}: exceeded maximum ADO Recordset chapter depth {}",
            crate::MAX_RECORDSET_DEPTH
        );
    }
    Ok(())
}

/// Inspect an ADTG byte stream without parsing it into a full [`Recordset`].
pub fn inspect_adtg(bytes: &[u8]) -> Result<AdtgDocument> {
    if detect_format(bytes) == RecordsetFormat::Xml {
        bail!("input looks like ADO XML, not ADTG");
    }

    let header_len = bytes.len().min(64);
    let trailer_len = bytes.len().min(64);
    let first_u32_le = bytes
        .get(..4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));

    Ok(AdtgDocument {
        length: bytes.len(),
        header_hex: hex::encode_upper(&bytes[..header_len]),
        trailer_hex: hex::encode_upper(&bytes[bytes.len().saturating_sub(trailer_len)..]),
        first_u32_le,
        detected_strings: detect_strings(bytes, 160),
    })
}

/// Parses a persisted ADO ADTG Recordset using the native Rust decoder.
///
/// The supported corpus includes flat Recordsets and checked MDAC/MSDataShape
/// chaptered layouts. Use this for new code that already knows the input is
/// ADTG; use `crate::parse_recordset_bytes` when format auto-detection is
/// needed.
/// Parse a native ADTG byte stream with default options.
pub fn parse_adtg_bytes(bytes: &[u8]) -> Result<Recordset> {
    parse_adtg_bytes_with_options(bytes, AdtgParseOptions::default())
}

/// Parse a native ADTG byte stream with explicit options.
pub fn parse_adtg_bytes_with_options(bytes: &[u8], options: AdtgParseOptions) -> Result<Recordset> {
    options
        .resource_limits
        .check_input_bytes(bytes.len(), "ADTG input")?;
    if detect_format(bytes) == RecordsetFormat::Xml {
        bail!("input looks like ADO XML, not ADTG");
    }

    let (descriptors, mut offset) = parse_field_descriptors(bytes, options.resource_limits)?;
    if descriptors.is_empty() {
        bail!("ADTG field descriptors were not found");
    }
    if descriptors
        .iter()
        .any(|descriptor| descriptor.type_code == 136)
    {
        let recordset = parse_chaptered_adtg(bytes, descriptors, offset, &options)?;
        crate::validate_recordset_shape(&recordset)
            .context("parsed native ADTG Recordset shape was inconsistent")?;
        crate::validate_recordset_resource_limits(&recordset, options.resource_limits)
            .context("parsed native ADTG Recordset exceeded resource limits")?;
        return Ok(recordset);
    }
    let fields = descriptors
        .iter()
        .filter(|descriptor| !descriptor.hidden)
        .enumerate()
        .map(|(index, descriptor)| descriptor.to_field(index))
        .collect::<Vec<_>>();

    let mut rows = Vec::new();
    let mut changes = Vec::new();
    while offset < bytes.len() {
        if bytes[offset] == 0x0f {
            break;
        }

        if bytes[offset] == 0x07 {
            offset += 1;
            let present = row_presence_from_mask(bytes, &mut offset, &descriptors)?;
            let values = visible_values(
                read_values(bytes, &mut offset, &descriptors, &present, &options)?,
                &descriptors,
            );

            match bytes.get(offset).copied() {
                Some(0x0c) => {
                    offset += 1;
                    push_change(
                        &mut rows,
                        &mut changes,
                        RowChangeKind::Delete,
                        RowState::Deleted,
                        RecordStatusFlag::Deleted,
                        values,
                    );
                }
                Some(0x0a) => {
                    offset += 1;
                    let updated_values = visible_values(
                        read_update_values(bytes, &mut offset, &descriptors, &options)?,
                        &descriptors,
                    );
                    let change_index = changes.len();
                    let original_index = rows.len();
                    rows.push(Row {
                        ordinal: original_index,
                        state: RowState::Original,
                        status_flags: vec![RecordStatusFlag::Modified],
                        change_index: Some(change_index),
                        values,
                    });
                    let updated_index = rows.len();
                    rows.push(Row {
                        ordinal: updated_index,
                        state: RowState::Updated,
                        status_flags: vec![RecordStatusFlag::Modified],
                        change_index: Some(change_index),
                        values: updated_values,
                    });
                    changes.push(RowChange {
                        kind: RowChangeKind::Update,
                        row_indices: vec![original_index, updated_index],
                    });
                }
                Some(0x0d) => {
                    offset += 1;
                    push_change(
                        &mut rows,
                        &mut changes,
                        RowChangeKind::Current,
                        RowState::Current,
                        RecordStatusFlag::Unmodified,
                        values,
                    );
                }
                _ => {
                    push_change(
                        &mut rows,
                        &mut changes,
                        RowChangeKind::Current,
                        RowState::Current,
                        RecordStatusFlag::Unmodified,
                        values,
                    );
                }
            }
            continue;
        }

        if bytes[offset] == 0x0d {
            offset += 1;
        }
        let present = insert_presence(bytes, &mut offset, &descriptors)?;
        let values = visible_values(
            read_values(bytes, &mut offset, &descriptors, &present, &options)?,
            &descriptors,
        );
        if bytes.get(offset).copied() == Some(0x0f) && offset + 1 == bytes.len() {
            offset += 1;
        }
        push_change(
            &mut rows,
            &mut changes,
            RowChangeKind::Insert,
            RowState::Inserted,
            RecordStatusFlag::New,
            values,
        );
    }

    validate_flat_adtg_end(bytes, offset)?;

    let recordset = Recordset {
        fields,
        rows,
        changes,
    };
    crate::validate_recordset_shape(&recordset)
        .context("parsed native ADTG Recordset shape was inconsistent")?;
    crate::validate_recordset_resource_limits(&recordset, options.resource_limits)
        .context("parsed native ADTG Recordset exceeded resource limits")?;
    Ok(recordset)
}

fn validate_flat_adtg_end(bytes: &[u8], offset: usize) -> Result<()> {
    if offset == bytes.len() {
        return Ok(());
    }
    if bytes.get(offset).copied() == Some(0x0f) && offset + 1 == bytes.len() {
        return Ok(());
    }
    bail!("unexpected trailing bytes in flat ADTG at offset {offset:#x}");
}

/// Reads and parses a persisted ADO ADTG Recordset file using the native Rust
/// decoder.
/// Read and parse a native ADTG file from disk with default options.
pub fn parse_adtg_file(path: impl AsRef<Path>) -> Result<Recordset> {
    parse_adtg_file_with_options(path, AdtgParseOptions::default())
}

/// Read and parse a native ADTG file from disk with explicit options.
pub fn parse_adtg_file_with_options(
    path: impl AsRef<Path>,
    options: AdtgParseOptions,
) -> Result<Recordset> {
    let path = path.as_ref();
    let bytes = crate::read_file_limited(path, options.resource_limits.max_input_bytes)?;
    parse_adtg_bytes_with_options(&bytes, options)
        .with_context(|| format!("failed to parse ADTG {}", path.display()))
}

/// Alias for `parse_adtg_bytes`.
///
/// The native decoder accepts flat ADTG and checked chaptered ADTG layouts.
pub fn parse_flat_adtg(bytes: &[u8]) -> Result<Recordset> {
    parse_adtg_bytes(bytes)
}

fn parse_chaptered_adtg(
    bytes: &[u8],
    parent_descriptors: Vec<FieldDescriptor>,
    schema_offset: usize,
    options: &AdtgParseOptions,
) -> Result<Recordset> {
    let mut next_child_group_id = 2;
    let (schema, offset) = parse_chapter_schema(
        bytes,
        parent_descriptors,
        schema_offset,
        None,
        &mut next_child_group_id,
        options.resource_limits,
        0,
    )?;
    if schema.chapter_fields.is_empty() {
        bail!("chaptered ADTG had no chapter descriptor");
    }

    let (raw_rows, next_offset) = read_chapter_row_tree(bytes, offset, &schema, options)?;
    let recordset = materialize_chapter_schema_rows(&schema, &raw_rows, 0)?;
    let offset = next_offset;
    if offset < bytes.len() {
        bail!("unexpected trailing bytes in chaptered ADTG at offset {offset:#x}");
    }

    Ok(recordset)
}

#[derive(Debug, Clone)]
struct ChapterField {
    parent_index: usize,
    relation: ChapterRelation,
}

#[derive(Debug, Clone)]
struct ChapterSchema {
    row_group_id: Option<u32>,
    descriptors: Vec<FieldDescriptor>,
    fields: Vec<Field>,
    chapter_fields: Vec<ChapterField>,
    child_schemas: Vec<ChapterSchema>,
}

#[derive(Debug, Clone)]
struct RawChapterRows {
    rows: Vec<RawChapterRow>,
    child_groups: Vec<RawChapterRows>,
}

#[derive(Debug, Clone)]
struct RawChapterRow {
    kind: RowChangeKind,
    values: Vec<Value>,
    updated_values: Option<Vec<Value>>,
}

impl RawChapterRow {
    fn relation_values(&self) -> Vec<Value> {
        match (&self.kind, &self.updated_values) {
            (RowChangeKind::Update, Some(updated)) => {
                overlay_unavailable_values(&self.values, updated)
            }
            _ => self.values.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ChapterRowMarker {
    Root,
    Child,
}

impl ChapterRowMarker {
    fn current_marker(self) -> u8 {
        match self {
            ChapterRowMarker::Root => 0x07,
            ChapterRowMarker::Child => 0x87,
        }
    }

    fn insert_marker(self) -> u8 {
        match self {
            ChapterRowMarker::Root => 0x0d,
            ChapterRowMarker::Child => 0x8d,
        }
    }

    fn update_marker(self) -> u8 {
        match self {
            ChapterRowMarker::Root => 0x0a,
            ChapterRowMarker::Child => 0x8a,
        }
    }

    fn delete_marker(self) -> u8 {
        match self {
            ChapterRowMarker::Root => 0x0c,
            ChapterRowMarker::Child => 0x8c,
        }
    }
}

fn parse_chapter_schema(
    bytes: &[u8],
    descriptors: Vec<FieldDescriptor>,
    offset: usize,
    row_group_id: Option<u32>,
    next_child_group_id: &mut u32,
    limits: ResourceLimits,
    depth: usize,
) -> Result<(ChapterSchema, usize)> {
    validate_chapter_depth("ADTG chapter schema", depth)?;
    let chapter_fields = descriptors
        .iter()
        .enumerate()
        .filter_map(|(index, descriptor)| {
            (descriptor.type_code == 136).then_some((index, descriptor))
        })
        .map(|(index, descriptor)| {
            let relation = descriptor
                .chapter_relation
                .clone()
                .ok_or_else(|| anyhow!("chaptered ADTG field had no relation metadata"))?;
            Ok(ChapterField {
                parent_index: index,
                relation,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut fields = descriptors
        .iter()
        .filter(|descriptor| !descriptor.hidden)
        .enumerate()
        .map(|(index, descriptor)| descriptor.to_field(index))
        .collect::<Vec<_>>();

    let mut child_schemas = Vec::new();
    let mut next_offset = offset;
    for chapter_field in &chapter_fields {
        let child_row_group_id = *next_child_group_id;
        *next_child_group_id += 1;
        let Some((child_descriptors, child_offset, _child_block_start)) =
            find_field_descriptor_block(bytes, next_offset, limits)?
        else {
            bail!("chaptered ADTG child field descriptors were not found");
        };
        let (child_schema, after_child_schema) = parse_chapter_schema(
            bytes,
            child_descriptors,
            child_offset,
            Some(child_row_group_id),
            next_child_group_id,
            limits,
            depth + 1,
        )?;
        validate_chapter_relation(
            &chapter_field.relation,
            &descriptors,
            &child_schema.descriptors,
        )?;
        child_schemas.push(child_schema);
        next_offset = after_child_schema;
    }
    for (chapter_field, child_schema) in chapter_fields.iter().zip(&child_schemas) {
        if let Some(field_index) = visible_field_index(&descriptors, chapter_field.parent_index) {
            fields[field_index].chapter_fields = Some(child_schema.fields.clone());
            fields[field_index].chapter_relation = Some(chapter_field.relation.clone());
        }
    }

    Ok((
        ChapterSchema {
            row_group_id,
            descriptors,
            fields,
            chapter_fields,
            child_schemas,
        },
        next_offset,
    ))
}

fn visible_field_index(descriptors: &[FieldDescriptor], descriptor_index: usize) -> Option<usize> {
    let descriptor = descriptors.get(descriptor_index)?;
    if descriptor.hidden {
        return None;
    }
    Some(
        descriptors[..descriptor_index]
            .iter()
            .filter(|descriptor| !descriptor.hidden)
            .count(),
    )
}

fn read_chapter_row_tree(
    bytes: &[u8],
    offset: usize,
    schema: &ChapterSchema,
    options: &AdtgParseOptions,
) -> Result<(RawChapterRows, usize)> {
    let (rows, mut next_offset) =
        read_chapter_row_group(bytes, offset, schema, ChapterRowMarker::Root, options, 0)?;
    if bytes.get(next_offset).copied() == Some(0x0f) {
        next_offset += 1;
    }
    if next_offset == bytes.len() {
        return Ok((rows, next_offset));
    }

    bail!("unexpected trailing bytes in ADTG chapter row tree at offset {next_offset:#x}");
}

fn read_chapter_row_group(
    bytes: &[u8],
    offset: usize,
    schema: &ChapterSchema,
    marker: ChapterRowMarker,
    options: &AdtgParseOptions,
    depth: usize,
) -> Result<(RawChapterRows, usize)> {
    validate_chapter_depth("ADTG chapter row group", depth)?;
    let (rows, mut next_offset) =
        read_current_chapter_rows(bytes, offset, schema, marker, options)?;
    let mut child_groups = Vec::with_capacity(schema.child_schemas.len());
    for child_schema in &schema.child_schemas {
        let (child_group, child_offset) = read_chapter_row_group(
            bytes,
            next_offset,
            child_schema,
            ChapterRowMarker::Child,
            options,
            depth + 1,
        )?;
        child_groups.push(child_group);
        next_offset = child_offset;
    }

    Ok((RawChapterRows { rows, child_groups }, next_offset))
}

fn read_current_chapter_rows(
    bytes: &[u8],
    offset: usize,
    schema: &ChapterSchema,
    marker: ChapterRowMarker,
    options: &AdtgParseOptions,
) -> Result<(Vec<RawChapterRow>, usize)> {
    let mut rows = Vec::new();
    let mut next_offset = offset;

    while let Some((row_offset, row)) =
        read_chapter_row(bytes, next_offset, schema, marker, options)?
    {
        next_offset = row_offset;
        rows.push(row);
    }

    Ok((rows, next_offset))
}

fn read_chapter_row(
    bytes: &[u8],
    offset: usize,
    schema: &ChapterSchema,
    marker: ChapterRowMarker,
    options: &AdtgParseOptions,
) -> Result<Option<(usize, RawChapterRow)>> {
    if let Some(mut next) =
        read_chapter_row_marker_prefix(bytes, offset, schema, marker, marker.current_marker())
    {
        let present = chapter_row_presence_from_mask(bytes, &mut next, &schema.descriptors)?;
        let values =
            read_chaptered_parent_values(bytes, &mut next, &schema.descriptors, &present, options)?;

        return Ok(Some(match bytes.get(next).copied() {
            Some(value) if value == marker.update_marker() => {
                next = read_chapter_row_marker_prefix(bytes, next, schema, marker, value)
                    .ok_or_else(|| anyhow!("invalid ADTG chapter update row marker prefix"))?;
                let updated_values =
                    read_update_values(bytes, &mut next, &schema.descriptors, options)?;
                (
                    next,
                    RawChapterRow {
                        kind: RowChangeKind::Update,
                        values,
                        updated_values: Some(updated_values),
                    },
                )
            }
            Some(value) if value == marker.delete_marker() => {
                next = read_chapter_row_marker_prefix(bytes, next, schema, marker, value)
                    .ok_or_else(|| anyhow!("invalid ADTG chapter delete row marker prefix"))?;
                (
                    next,
                    RawChapterRow {
                        kind: RowChangeKind::Delete,
                        values,
                        updated_values: None,
                    },
                )
            }
            _ => (
                next,
                RawChapterRow {
                    kind: RowChangeKind::Current,
                    values,
                    updated_values: None,
                },
            ),
        }));
    }

    if let Some(mut next) =
        read_chapter_row_marker_prefix(bytes, offset, schema, marker, marker.insert_marker())
    {
        let present = insert_presence(bytes, &mut next, &schema.descriptors)?;
        let values =
            read_chaptered_parent_values(bytes, &mut next, &schema.descriptors, &present, options)?;
        return Ok(Some((
            next,
            RawChapterRow {
                kind: RowChangeKind::Insert,
                values,
                updated_values: None,
            },
        )));
    }

    Ok(None)
}

fn read_chapter_row_marker_prefix(
    bytes: &[u8],
    offset: usize,
    schema: &ChapterSchema,
    marker: ChapterRowMarker,
    expected_marker: u8,
) -> Option<usize> {
    if bytes.get(offset).copied()? != expected_marker {
        return None;
    }

    let next = offset.checked_add(1).filter(|next| *next <= bytes.len())?;
    match marker {
        ChapterRowMarker::Root => Some(next),
        ChapterRowMarker::Child => {
            let actual_group_id = read_u32(bytes, next).ok()?;
            if Some(actual_group_id) != schema.row_group_id {
                return None;
            }
            next.checked_add(4).filter(|next| *next <= bytes.len())
        }
    }
}

fn chapter_row_presence_from_mask(
    bytes: &[u8],
    offset: &mut usize,
    descriptors: &[FieldDescriptor],
) -> Result<Vec<bool>> {
    let mask_field_count = descriptors
        .iter()
        .filter(|descriptor| chapter_row_mask_field(descriptor))
        .count();
    if mask_field_count == 0 {
        return Ok(vec![true; descriptors.len()]);
    }

    let mask = read_mask(bytes, offset, mask_field_count)?;
    validate_row_presence_mask_padding(&mask, mask_field_count, "chapter row presence")?;
    let mut mask_index = 0usize;
    let mut present = Vec::with_capacity(descriptors.len());
    for descriptor in descriptors {
        if chapter_row_mask_field(descriptor) {
            let field_present = mask_bit(&mask, mask_index);
            mask_index += 1;
            present.push(descriptor.type_code == 136 || field_present);
        } else {
            present.push(true);
        }
    }
    Ok(present)
}

fn chapter_row_mask_field(descriptor: &FieldDescriptor) -> bool {
    descriptor.type_code == 136 || descriptor.attributes & (0x20 | 0x40) != 0
}

fn read_chaptered_parent_values(
    bytes: &[u8],
    offset: &mut usize,
    descriptors: &[FieldDescriptor],
    present: &[bool],
    options: &AdtgParseOptions,
) -> Result<Vec<Value>> {
    let mut values = Vec::with_capacity(descriptors.len());
    for (descriptor, present) in descriptors.iter().zip(present.iter().copied()) {
        if descriptor.type_code == 136 {
            values.push(Value::Chapter(Box::new(Recordset {
                fields: Vec::new(),
                rows: Vec::new(),
                changes: Vec::new(),
            })));
        } else if !present {
            values.push(Value::Null);
        } else {
            let start = *offset;
            values.push(
                read_value(bytes, offset, descriptor, options).with_context(|| {
                    format!(
                        "failed to read ADTG chapter parent field {} type {} at offset {start:#x}",
                        descriptor.name, descriptor.type_code
                    )
                })?,
            );
        }
    }
    Ok(values)
}

fn materialize_chapter_schema_rows(
    schema: &ChapterSchema,
    raw: &RawChapterRows,
    depth: usize,
) -> Result<Recordset> {
    validate_chapter_depth("ADTG chapter materialization", depth)?;
    let mut rows = Vec::new();
    let mut changes = Vec::new();
    for row in &raw.rows {
        push_materialized_chapter_change(
            schema,
            row,
            &raw.child_groups,
            &mut rows,
            &mut changes,
            depth,
        )?;
    }

    Ok(Recordset {
        fields: schema.fields.clone(),
        rows,
        changes,
    })
}

fn push_materialized_chapter_change(
    schema: &ChapterSchema,
    raw_row: &RawChapterRow,
    child_groups: &[RawChapterRows],
    rows: &mut Vec<Row>,
    changes: &mut Vec<RowChange>,
    depth: usize,
) -> Result<()> {
    let relation_values = raw_row.relation_values();
    match raw_row.kind {
        RowChangeKind::Current => {
            let values = materialize_chapter_row_values(
                schema,
                raw_row.values.clone(),
                child_groups,
                &relation_values,
                depth,
            )?;
            push_change(
                rows,
                changes,
                RowChangeKind::Current,
                RowState::Current,
                RecordStatusFlag::Unmodified,
                values,
            );
        }
        RowChangeKind::Insert => {
            let values = materialize_chapter_row_values(
                schema,
                raw_row.values.clone(),
                child_groups,
                &relation_values,
                depth,
            )?;
            push_change(
                rows,
                changes,
                RowChangeKind::Insert,
                RowState::Inserted,
                RecordStatusFlag::New,
                values,
            );
        }
        RowChangeKind::Delete => {
            let values = materialize_chapter_row_values(
                schema,
                raw_row.values.clone(),
                child_groups,
                &relation_values,
                depth,
            )?;
            push_change(
                rows,
                changes,
                RowChangeKind::Delete,
                RowState::Deleted,
                RecordStatusFlag::Deleted,
                values,
            );
        }
        RowChangeKind::Update => {
            let updated_values = raw_row
                .updated_values
                .clone()
                .ok_or_else(|| anyhow!("ADTG chapter update row had no updated values"))?;
            let original_values = materialize_chapter_row_values(
                schema,
                raw_row.values.clone(),
                child_groups,
                &relation_values,
                depth,
            )?;
            let updated_values = materialize_chapter_updated_values(
                schema,
                updated_values,
                child_groups,
                &relation_values,
                depth,
            )?;
            push_update_change(rows, changes, original_values, updated_values);
        }
    }
    Ok(())
}

fn push_update_change(
    rows: &mut Vec<Row>,
    changes: &mut Vec<RowChange>,
    original_values: Vec<Value>,
    updated_values: Vec<Value>,
) {
    let change_index = changes.len();
    let original_index = rows.len();
    rows.push(Row {
        ordinal: original_index,
        state: RowState::Original,
        status_flags: vec![RecordStatusFlag::Modified],
        change_index: Some(change_index),
        values: original_values,
    });
    let updated_index = rows.len();
    rows.push(Row {
        ordinal: updated_index,
        state: RowState::Updated,
        status_flags: vec![RecordStatusFlag::Modified],
        change_index: Some(change_index),
        values: updated_values,
    });
    changes.push(RowChange {
        kind: RowChangeKind::Update,
        row_indices: vec![original_index, updated_index],
    });
}

fn materialize_chapter_row_values(
    schema: &ChapterSchema,
    mut values: Vec<Value>,
    child_groups: &[RawChapterRows],
    relation_parent_values: &[Value],
    depth: usize,
) -> Result<Vec<Value>> {
    validate_chapter_depth("ADTG chapter row materialization", depth)?;
    if schema.chapter_fields.len() != schema.child_schemas.len()
        || schema.chapter_fields.len() != child_groups.len()
    {
        bail!("ADTG chapter schema/row group mismatch");
    }

    for ((chapter_field, child_schema), child_group) in schema
        .chapter_fields
        .iter()
        .zip(&schema.child_schemas)
        .zip(child_groups)
    {
        validate_chapter_relation(
            &chapter_field.relation,
            &schema.descriptors,
            &child_schema.descriptors,
        )?;
        let chapter = child_recordset_for_relation(
            child_schema,
            child_group,
            &chapter_field.relation,
            relation_parent_values,
            depth + 1,
        )?;
        values[chapter_field.parent_index] = Value::Chapter(Box::new(chapter));
    }

    Ok(visible_values(values, &schema.descriptors))
}

fn materialize_chapter_updated_values(
    schema: &ChapterSchema,
    mut values: Vec<Value>,
    child_groups: &[RawChapterRows],
    relation_parent_values: &[Value],
    depth: usize,
) -> Result<Vec<Value>> {
    validate_chapter_depth("ADTG chapter updated-row materialization", depth)?;
    if schema.chapter_fields.len() != schema.child_schemas.len()
        || schema.chapter_fields.len() != child_groups.len()
    {
        bail!("ADTG chapter schema/row group mismatch");
    }

    for ((chapter_field, child_schema), child_group) in schema
        .chapter_fields
        .iter()
        .zip(&schema.child_schemas)
        .zip(child_groups)
    {
        if matches!(values[chapter_field.parent_index], Value::Unavailable) {
            continue;
        }
        validate_chapter_relation(
            &chapter_field.relation,
            &schema.descriptors,
            &child_schema.descriptors,
        )?;
        let chapter = child_recordset_for_relation(
            child_schema,
            child_group,
            &chapter_field.relation,
            relation_parent_values,
            depth + 1,
        )?;
        values[chapter_field.parent_index] = Value::Chapter(Box::new(chapter));
    }

    Ok(visible_values(values, &schema.descriptors))
}

fn child_recordset_for_relation(
    schema: &ChapterSchema,
    raw: &RawChapterRows,
    relation: &ChapterRelation,
    parent_values: &[Value],
    depth: usize,
) -> Result<Recordset> {
    validate_chapter_depth("ADTG child Recordset materialization", depth)?;
    let mut rows = Vec::new();
    let mut changes = Vec::new();
    for row in &raw.rows {
        let relation_values = row.relation_values();
        if !chapter_relation_matches(relation, parent_values, &relation_values) {
            continue;
        }
        push_materialized_chapter_change(
            schema,
            row,
            &raw.child_groups,
            &mut rows,
            &mut changes,
            depth,
        )?;
    }

    Ok(Recordset {
        fields: schema.fields.clone(),
        rows,
        changes,
    })
}

fn validate_chapter_relation(
    relation: &ChapterRelation,
    parent_descriptors: &[FieldDescriptor],
    child_descriptors: &[FieldDescriptor],
) -> Result<()> {
    if relation.pairs.is_empty() {
        bail!("ADTG chapter relation had no key pairs");
    }
    let mut seen_parent_ordinals = Vec::new();
    let mut seen_child_ordinals = Vec::new();
    for pair in &relation.pairs {
        let parent_index = pair
            .parent_ordinal
            .checked_sub(1)
            .filter(|index| *index < parent_descriptors.len())
            .ok_or_else(|| anyhow!("ADTG chapter parent relation ordinal out of range"))?;
        let child_index = pair
            .child_ordinal
            .checked_sub(1)
            .filter(|index| *index < child_descriptors.len())
            .ok_or_else(|| anyhow!("ADTG chapter child relation ordinal out of range"))?;
        if seen_parent_ordinals.contains(&pair.parent_ordinal) {
            bail!(
                "ADTG chapter relation repeated parent ordinal {}",
                pair.parent_ordinal
            );
        }
        if seen_child_ordinals.contains(&pair.child_ordinal) {
            bail!(
                "ADTG chapter relation repeated child ordinal {}",
                pair.child_ordinal
            );
        }
        seen_parent_ordinals.push(pair.parent_ordinal);
        seen_child_ordinals.push(pair.child_ordinal);
        if parent_descriptors[parent_index].type_code == 136 {
            bail!(
                "ADTG chapter parent relation ordinal {} points to chapter field {}",
                pair.parent_ordinal,
                parent_descriptors[parent_index].name
            );
        }
        if child_descriptors[child_index].type_code == 136 {
            bail!(
                "ADTG chapter child relation ordinal {} points to chapter field {}",
                pair.child_ordinal,
                child_descriptors[child_index].name
            );
        }
    }
    Ok(())
}

fn chapter_relation_matches(
    relation: &ChapterRelation,
    parent_values: &[Value],
    child_values: &[Value],
) -> bool {
    relation.pairs.iter().all(|pair| {
        let parent_index = pair.parent_ordinal.saturating_sub(1);
        let child_index = pair.child_ordinal.saturating_sub(1);
        match (
            parent_values.get(parent_index),
            child_values.get(child_index),
        ) {
            (Some(parent), Some(child)) => chapter_relation_key_values_match(parent, child),
            _ => false,
        }
    })
}

fn chapter_relation_key_values_match(parent: &Value, child: &Value) -> bool {
    !matches!(parent, Value::Null | Value::Unavailable)
        && !matches!(child, Value::Null | Value::Unavailable)
        && parent == child
}

#[derive(Debug, Clone)]
struct FieldDescriptor {
    name: String,
    source_column: Option<String>,
    source_catalog: Option<String>,
    source_schema: Option<String>,
    source_table: Option<String>,
    ordinal: usize,
    type_code: u16,
    defined_size: u32,
    precision: u32,
    scale: u32,
    attributes: u32,
    provider_projection_marker: Option<u32>,
    hidden: bool,
    chapter_relation: Option<ChapterRelation>,
}

impl FieldDescriptor {
    fn to_field(&self, index: usize) -> Field {
        let attributes = FieldAttribute::from_bits(self.attributes);
        let ado_type = self.ado_type();
        Field {
            name: self.name.clone(),
            xml_name: self.name.clone(),
            ordinal: Some(index + 1),
            data_type: ado_type.map(|ty| adtg_data_type_name(ty.code).to_string()),
            db_type: None,
            ado_type,
            max_length: (self.defined_size != u32::MAX).then_some(self.defined_size as usize),
            precision: Some(self.precision as usize),
            scale: matches!(self.type_code, 14 | 131 | 135 | 136 | 139)
                .then_some(self.scale as i32),
            nullable: attributes.contains(&FieldAttribute::IsNullable)
                || attributes.contains(&FieldAttribute::MayBeNull),
            writable: attributes.contains(&FieldAttribute::Updatable),
            fixed_length: attributes.contains(&FieldAttribute::Fixed),
            long: attributes.contains(&FieldAttribute::Long),
            key_column: self.attributes & 0x8000 != 0,
            base_catalog: self
                .source_catalog
                .as_ref()
                .filter(|source| !source.is_empty())
                .cloned(),
            base_schema: self
                .source_schema
                .as_ref()
                .filter(|source| !source.is_empty())
                .cloned(),
            base_table: self
                .source_table
                .as_ref()
                .filter(|source| !source.is_empty())
                .cloned(),
            base_column: self
                .source_column
                .as_ref()
                .filter(|source| !source.is_empty())
                .cloned(),
            chapter_fields: None,
            chapter_relation: None,
            attributes,
        }
    }

    fn ado_type(&self) -> Option<AdoDataType> {
        let is_fixed = self.attributes & 0x10 != 0;
        let is_long = self.attributes & 0x80 != 0;
        Some(match self.type_code {
            2 => AdoDataType::new("adSmallInt", 2),
            3 => AdoDataType::new("adInteger", 3),
            4 => AdoDataType::new("adSingle", 4),
            5 => AdoDataType::new("adDouble", 5),
            6 => AdoDataType::new("adCurrency", 6),
            7 => AdoDataType::new("adDate", 7),
            11 => AdoDataType::new("adBoolean", 11),
            12 => AdoDataType::new("adVariant", 12),
            14 => AdoDataType::new("adDecimal", 14),
            16 => AdoDataType::new("adTinyInt", 16),
            17 => AdoDataType::new("adUnsignedTinyInt", 17),
            18 => AdoDataType::new("adUnsignedSmallInt", 18),
            19 => AdoDataType::new("adUnsignedInt", 19),
            20 => AdoDataType::new("adBigInt", 20),
            21 => AdoDataType::new("adUnsignedBigInt", 21),
            64 => AdoDataType::new("adFileTime", 64),
            72 => AdoDataType::new("adGUID", 72),
            128 if is_long => AdoDataType::new("adLongVarBinary", 205),
            128 if is_fixed => AdoDataType::new("adBinary", 128),
            128 => AdoDataType::new("adVarBinary", 204),
            129 if is_long => AdoDataType::new("adLongVarChar", 201),
            129 if is_fixed => AdoDataType::new("adChar", 129),
            129 => AdoDataType::new("adVarChar", 200),
            130 if is_long => AdoDataType::new("adLongVarWChar", 203),
            130 if is_fixed => AdoDataType::new("adWChar", 130),
            130 => AdoDataType::new("adVarWChar", 202),
            131 => AdoDataType::new("adNumeric", 131),
            133 => AdoDataType::new("adDBDate", 133),
            134 => AdoDataType::new("adDBTime", 134),
            135 => AdoDataType::new("adDBTimeStamp", 135),
            136 => AdoDataType::new("adChapter", 136),
            139 => AdoDataType::new("adVarNumeric", 139),
            _ => return None,
        })
    }
}

fn parse_field_descriptors(
    bytes: &[u8],
    limits: ResourceLimits,
) -> Result<(Vec<FieldDescriptor>, usize)> {
    let Some((descriptors, next_offset, _block_start)) =
        find_field_descriptor_block(bytes, 0, limits)?
    else {
        bail!("ADTG field descriptor marker was not found");
    };
    Ok((descriptors, next_offset))
}

fn find_field_descriptor_block(
    bytes: &[u8],
    start: usize,
    limits: ResourceLimits,
) -> Result<Option<(Vec<FieldDescriptor>, usize, usize)>> {
    let mut descriptors = Vec::new();
    let mut offset = start;

    while offset.checked_add(20).is_some_and(|end| end < bytes.len()) {
        if let Some((descriptor, next_offset)) = parse_field_descriptor_at(bytes, offset)? {
            let block_start = offset;
            descriptors.push(descriptor);
            limits.check_fields(descriptors.len(), "ADTG descriptor block")?;
            offset = next_offset;
            while let Some((descriptor, next_offset)) = parse_field_descriptor_at(bytes, offset)? {
                descriptors.push(descriptor);
                limits.check_fields(descriptors.len(), "ADTG descriptor block")?;
                offset = next_offset;
            }
            if let Some(shape) = descriptor_shape_at(bytes, offset)? {
                if !is_supported_descriptor_type(shape.type_code) {
                    bail!(
                        "unsupported native ADTG descriptor type {} for field {} at offset {offset:#x}",
                        shape.type_code,
                        shape.name
                    );
                }
            }
            if descriptor_header_at(bytes, offset).is_some() {
                bail!(
                    "unsupported non-flat or chaptered ADTG field descriptor at offset {offset:#x}"
                );
            }
            validate_descriptor_ordinals(&descriptors)?;
            if let Some(provider_source) =
                parse_provider_source_before_descriptor_block(bytes, block_start)?
            {
                apply_provider_source_metadata(&mut descriptors, &provider_source);
            }
            mark_hidden_key_suffix(&mut descriptors);
            return Ok(Some((descriptors, offset, block_start)));
        }
        if let Some(shape) = descriptor_shape_at(bytes, offset)? {
            if !is_supported_descriptor_type(shape.type_code) {
                bail!(
                    "unsupported native ADTG descriptor type {} for field {} at offset {offset:#x}",
                    shape.type_code,
                    shape.name
                );
            }
        }
        offset += 1;
    }

    Ok(None)
}

fn validate_descriptor_ordinals(descriptors: &[FieldDescriptor]) -> Result<()> {
    for (index, descriptor) in descriptors.iter().enumerate() {
        let expected = index + 1;
        if descriptor.ordinal != expected {
            bail!(
                "unexpected ADTG field descriptor ordinal {} for field {} at descriptor {expected}",
                descriptor.ordinal,
                descriptor.name
            );
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct ProviderSourceBlock {
    catalog: Option<String>,
    schema: Option<String>,
    table: Option<String>,
}

fn parse_provider_source_before_descriptor_block(
    bytes: &[u8],
    block_start: usize,
) -> Result<Option<ProviderSourceBlock>> {
    let search_start = block_start.saturating_sub(u8::MAX as usize + 3);
    for offset in search_start..block_start {
        if bytes.get(offset).copied() != Some(0x05)
            || offset.checked_add(3).is_none_or(|end| end > block_start)
        {
            continue;
        }
        let Some(block_end) = offset
            .checked_add(bytes[offset + 1] as usize)
            .and_then(|end| end.checked_add(3))
        else {
            continue;
        };
        if block_end != block_start {
            continue;
        }
        if let Some(source) = parse_provider_source_block_at(bytes, offset, block_end)? {
            return Ok(Some(source));
        }
    }
    Ok(None)
}

fn parse_provider_source_block_at(
    bytes: &[u8],
    offset: usize,
    limit: usize,
) -> Result<Option<ProviderSourceBlock>> {
    if offset.checked_add(7).is_none_or(|end| end > limit)
        || bytes.get(offset).copied() != Some(0x05)
    {
        return Ok(None);
    }

    let mut cursor = offset
        .checked_add(3)
        .ok_or_else(|| anyhow!("ADTG provider source cursor offset overflow"))?;
    let source_count = read_u16(bytes, cursor)?;
    cursor = cursor
        .checked_add(2)
        .ok_or_else(|| anyhow!("ADTG provider source count offset overflow"))?;
    if source_count == 0 {
        return Ok(None);
    }

    let (qualified_name, next) = read_required_utf16_len_string(bytes, cursor, limit)?;
    cursor = next;
    let (rowset_name, next) = read_required_utf16_len_string(bytes, cursor, limit)?;
    cursor = next;
    if cursor.checked_add(6).is_none_or(|end| end > limit) {
        return Ok(None);
    }

    let (catalog, schema, qualified_table) = parse_provider_qualified_name(&qualified_name);
    let table = nonempty_string(rowset_name).or(qualified_table);
    Ok(Some(ProviderSourceBlock {
        catalog,
        schema,
        table,
    }))
}

fn apply_provider_source_metadata(
    descriptors: &mut [FieldDescriptor],
    source: &ProviderSourceBlock,
) {
    for descriptor in descriptors {
        if descriptor.source_column.is_none() {
            continue;
        }
        if descriptor.source_catalog.is_none() {
            descriptor.source_catalog = source.catalog.clone();
        }
        if descriptor.source_schema.is_none() {
            descriptor.source_schema = source.schema.clone();
        }
        if descriptor.source_table.is_none() {
            descriptor.source_table = source.table.clone();
        }
    }
}

fn read_required_utf16_len_string(
    bytes: &[u8],
    offset: usize,
    limit: usize,
) -> Result<(String, usize)> {
    let (value, next) = read_optional_utf16_len_string(bytes, offset, limit)?;
    let Some(value) = value else {
        return Ok((String::new(), next));
    };
    Ok((value, next))
}

fn parse_provider_qualified_name(value: &str) -> (Option<String>, Option<String>, Option<String>) {
    let quoted = quoted_identifier_parts(value);
    if quoted.len() >= 3 {
        return (
            nonempty_string(quoted[0].clone()),
            nonempty_string(quoted[1].clone()),
            nonempty_string(quoted[2].clone()),
        );
    }

    let mut parts = value
        .split('.')
        .map(|part| part.trim().trim_matches('"').to_string())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() >= 3 {
        return (
            nonempty_string(parts.remove(0)),
            nonempty_string(parts.remove(0)),
            nonempty_string(parts.remove(0)),
        );
    }
    (None, None, None)
}

fn quoted_identifier_parts(value: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '"' {
            continue;
        }
        let mut part = String::new();
        while let Some(ch) = chars.next() {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    part.push('"');
                    continue;
                }
                break;
            }
            part.push(ch);
        }
        parts.push(part);
    }
    parts
}

fn nonempty_string(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

struct DescriptorShape {
    header: u8,
    name: String,
    source_column: Option<String>,
    ordinal: usize,
    type_code: u16,
    type_offset: usize,
    next_offset: usize,
}

fn parse_field_descriptor_at(
    bytes: &[u8],
    offset: usize,
) -> Result<Option<(FieldDescriptor, usize)>> {
    let Some(shape) = descriptor_shape_at(bytes, offset)? else {
        return Ok(None);
    };

    if !is_supported_descriptor_type(shape.type_code) {
        return Ok(None);
    }

    let attributes = read_u32(
        bytes,
        shape
            .type_offset
            .checked_add(14)
            .context("ADTG descriptor attribute offset overflow")?,
    )?;
    let extra_offset = shape
        .type_offset
        .checked_add(18)
        .context("ADTG descriptor extra offset overflow")?;
    let chapter_relation = if shape.type_code == 136 {
        parse_chapter_relation(bytes, extra_offset, shape.next_offset)?
    } else {
        None
    };
    let (source_catalog, source_schema) =
        parse_provider_catalog_schema(shape.header, bytes, extra_offset, shape.next_offset)
            .unwrap_or((None, None));
    let provider_projection_marker = (shape.header == 0xf3)
        .then(|| parse_provider_projection_marker(bytes, extra_offset, shape.next_offset))
        .flatten();

    let descriptor = FieldDescriptor {
        name: shape.name,
        source_column: shape.source_column,
        source_catalog,
        source_schema,
        source_table: None,
        ordinal: shape.ordinal,
        type_code: shape.type_code,
        defined_size: read_u32(
            bytes,
            shape
                .type_offset
                .checked_add(2)
                .context("ADTG descriptor defined-size offset overflow")?,
        )?,
        precision: read_u32(
            bytes,
            shape
                .type_offset
                .checked_add(6)
                .context("ADTG descriptor precision offset overflow")?,
        )?,
        scale: read_u32(
            bytes,
            shape
                .type_offset
                .checked_add(10)
                .context("ADTG descriptor scale offset overflow")?,
        )?,
        attributes,
        provider_projection_marker,
        hidden: false,
        chapter_relation,
    };
    validate_field_descriptor_metadata(&descriptor)?;

    Ok(Some((descriptor, shape.next_offset)))
}

fn descriptor_shape_at(bytes: &[u8], offset: usize) -> Result<Option<DescriptorShape>> {
    if offset.checked_add(20).is_none_or(|end| end >= bytes.len()) || bytes[offset] != 0x06 {
        return Ok(None);
    }
    let Some(header) = descriptor_header_at(bytes, offset) else {
        return Ok(None);
    };

    let descriptor_length = bytes[offset + 1] as usize;
    let Some(next_offset) = offset
        .checked_add(descriptor_length)
        .and_then(|value| value.checked_add(3))
    else {
        return Ok(None);
    };
    if next_offset > bytes.len() {
        return Ok(None);
    }

    let ordinal = read_u16(
        bytes,
        offset
            .checked_add(6)
            .context("ADTG descriptor ordinal offset overflow")?,
    )?;
    let name_len = read_u16(
        bytes,
        offset
            .checked_add(8)
            .context("ADTG descriptor name length offset overflow")?,
    )? as usize;
    let Some(name_start) = offset.checked_add(10) else {
        return Ok(None);
    };
    let Some(name_byte_len) = name_len.checked_mul(2) else {
        return Ok(None);
    };
    let Some(name_end) = name_start.checked_add(name_byte_len) else {
        return Ok(None);
    };
    if ordinal == 0 || name_len == 0 || name_end > next_offset {
        return Ok(None);
    }
    let Ok(name) = decode_utf16le(&bytes[name_start..name_end]) else {
        return Ok(None);
    };

    let mut type_offset = name_end;
    let mut source_column = None;
    if descriptor_header_has_source_column(header) {
        if type_offset
            .checked_add(6)
            .is_none_or(|end| end > next_offset)
        {
            return Ok(None);
        }
        let source_name_len = read_u16(
            bytes,
            type_offset
                .checked_add(4)
                .context("ADTG descriptor source-column length offset overflow")?,
        )? as usize;
        let Some(source_name_start) = type_offset.checked_add(6) else {
            return Ok(None);
        };
        let Some(source_name_byte_len) = source_name_len.checked_mul(2) else {
            return Ok(None);
        };
        let Some(source_name_end) = source_name_start.checked_add(source_name_byte_len) else {
            return Ok(None);
        };
        if source_name_end > next_offset {
            return Ok(None);
        }
        source_column = decode_utf16le(&bytes[source_name_start..source_name_end]).ok();
        type_offset = source_name_end;
    }
    if type_offset
        .checked_add(18)
        .is_none_or(|end| end > next_offset)
    {
        return Ok(None);
    }

    let type_code = read_u16(bytes, type_offset)?;

    Ok(Some(DescriptorShape {
        header,
        name,
        source_column,
        ordinal: ordinal as usize,
        type_code,
        type_offset,
        next_offset,
    }))
}

fn parse_chapter_relation(
    bytes: &[u8],
    extra_offset: usize,
    next_offset: usize,
) -> Result<Option<ChapterRelation>> {
    if extra_offset
        .checked_add(12)
        .is_none_or(|end| end > next_offset)
    {
        return Ok(None);
    }

    for relation_len_offset in extra_offset..=next_offset - 12 {
        let relation_len = read_u32(bytes, relation_len_offset)? as usize;
        let Some(relation_offset) = relation_len_offset.checked_add(4) else {
            continue;
        };
        let Some(relation_end) = relation_offset.checked_add(relation_len) else {
            continue;
        };
        if relation_len < 12
            || !relation_len.is_multiple_of(12)
            || relation_len > 64
            || relation_end > next_offset
        {
            continue;
        }
        let mut pairs = Vec::new();
        let mut pair_offset = relation_offset;
        while pair_offset
            .checked_add(8)
            .is_some_and(|end| end <= relation_end)
        {
            let parent_ordinal = read_u32(bytes, pair_offset)? as usize;
            let child_ordinal = read_u32(
                bytes,
                pair_offset
                    .checked_add(4)
                    .context("ADTG chapter relation child ordinal offset overflow")?,
            )? as usize;
            if parent_ordinal == 0 || child_ordinal == 0 {
                break;
            }
            pairs.push(ChapterRelationPair {
                parent_ordinal,
                child_ordinal,
            });
            pair_offset = pair_offset
                .checked_add(12)
                .context("ADTG chapter relation pair offset overflow")?;
        }
        if pairs.is_empty() {
            continue;
        }

        return Ok(Some(ChapterRelation { pairs }));
    }

    Ok(None)
}

fn parse_provider_projection_marker(
    bytes: &[u8],
    extra_offset: usize,
    next_offset: usize,
) -> Option<u32> {
    let catalog_name_len = read_u16(bytes, extra_offset).ok()? as usize;
    let after_catalog = extra_offset
        .checked_add(2)?
        .checked_add(catalog_name_len.checked_mul(2)?)?;
    if after_catalog.checked_add(2)? > next_offset {
        return None;
    }

    let schema_name_len = read_u16(bytes, after_catalog).ok()? as usize;
    let marker_offset = after_catalog
        .checked_add(2)?
        .checked_add(schema_name_len.checked_mul(2)?)?;
    if marker_offset.checked_add(4)? > next_offset {
        return None;
    }

    read_u32(bytes, marker_offset).ok()
}

fn parse_provider_catalog_schema(
    header: u8,
    bytes: &[u8],
    extra_offset: usize,
    next_offset: usize,
) -> Result<(Option<String>, Option<String>)> {
    if header != 0xf3 {
        return Ok((None, None));
    }

    let (catalog, after_catalog) =
        read_optional_utf16_len_string(bytes, extra_offset, next_offset)?;
    let (schema, _) = read_optional_utf16_len_string(bytes, after_catalog, next_offset)?;
    Ok((catalog, schema))
}

fn read_optional_utf16_len_string(
    bytes: &[u8],
    offset: usize,
    limit: usize,
) -> Result<(Option<String>, usize)> {
    if offset.checked_add(2).is_none_or(|end| end > limit) {
        return Ok((None, offset));
    }
    let units = read_u16(bytes, offset)? as usize;
    let start = offset
        .checked_add(2)
        .ok_or_else(|| anyhow!("ADTG provider string start offset overflow"))?;
    let end = start
        .checked_add(
            units
                .checked_mul(2)
                .ok_or_else(|| anyhow!("ADTG provider string length overflow"))?,
        )
        .ok_or_else(|| anyhow!("ADTG provider string offset overflow"))?;
    if end > limit {
        return Ok((None, offset));
    }
    let value = decode_utf16le(&bytes[start..end]).ok();
    Ok((value, end))
}

fn descriptor_header_at(bytes: &[u8], offset: usize) -> Option<u8> {
    if offset.checked_add(20).is_none_or(|end| end >= bytes.len()) || bytes[offset] != 0x06 {
        return None;
    }
    let start = offset.checked_add(2)?;
    let end = offset.checked_add(6)?;
    bytes.get(start..end).and_then(|chunk| {
        // MDAC writes 0x04 here for MSDataShape aggregate, CALC, and
        // NEW columns; scalar/provider fields use 0x00. Server-side
        // optimistic SQL Server cursors can write provider flags after the
        // 0xf3 source-column marker instead of the compact 0x01 0x00 pair.
        // MDAC XML->ADTG resaves can use 0xf0 for the same source-column
        // layout without the provider projection trailer.
        (matches!(chunk, [0x00, 0x80, 0x01, 0x00 | 0x04])
            || matches!(chunk, [0x00, 0xf0 | 0xf3, _, _]))
        .then_some(chunk[1])
    })
}

fn descriptor_header_has_source_column(header: u8) -> bool {
    matches!(header, 0xf0 | 0xf3)
}

fn mark_hidden_key_suffix(descriptors: &mut [FieldDescriptor]) {
    for descriptor in descriptors.iter_mut().rev() {
        if is_hidden_provider_suffix_descriptor(descriptor) {
            descriptor.hidden = true;
        } else {
            break;
        }
    }
}

fn is_hidden_provider_suffix_descriptor(descriptor: &FieldDescriptor) -> bool {
    let source_matches_name = descriptor
        .source_column
        .as_deref()
        .map(|source| source.eq_ignore_ascii_case(&descriptor.name))
        .unwrap_or(false);
    if !source_matches_name {
        return false;
    }

    descriptor.attributes & 0x8000 != 0
        || (descriptor.type_code == 128
            && descriptor.attributes & 0x200 != 0
            && descriptor.provider_projection_marker == Some(0))
}

fn is_supported_descriptor_type(type_code: u16) -> bool {
    is_supported_adtg_descriptor_type(type_code)
}

fn validate_field_descriptor_metadata(descriptor: &FieldDescriptor) -> Result<()> {
    validate_fixed_width_descriptor_metadata(descriptor)?;
    match descriptor.type_code {
        14 => {
            decimal_descriptor_precision_scale(descriptor)?;
        }
        131 => {
            if !(1..=38).contains(&descriptor.precision) {
                bail!(
                    "invalid ADTG adNumeric descriptor precision {} for field {}",
                    descriptor.precision,
                    descriptor.name
                );
            }
            if descriptor.scale > 38 {
                bail!(
                    "invalid ADTG adNumeric descriptor scale {} for field {}",
                    descriptor.scale,
                    descriptor.name
                );
            }
            if descriptor.scale > descriptor.precision {
                bail!(
                    "invalid ADTG adNumeric descriptor scale {} exceeds precision {} for field {}",
                    descriptor.scale,
                    descriptor.precision,
                    descriptor.name
                );
            }
        }
        139 => {
            validate_varnumeric_descriptor_metadata(descriptor)?;
        }
        _ => {}
    }
    Ok(())
}

fn validate_fixed_width_descriptor_metadata(descriptor: &FieldDescriptor) -> Result<()> {
    let exact_width = match descriptor.type_code {
        2 => Some(("adSmallInt", 2)),
        3 => Some(("adInteger", 4)),
        6 => Some(("adCurrency", 8)),
        7 => Some(("adDate", 8)),
        11 => Some(("adBoolean", 2)),
        12 => Some(("adVariant", 16)),
        14 => Some(("adDecimal", 16)),
        16 => Some(("adTinyInt", 1)),
        17 => Some(("adUnsignedTinyInt", 1)),
        18 => Some(("adUnsignedSmallInt", 2)),
        19 => Some(("adUnsignedInt", 4)),
        20 => Some(("adBigInt", 8)),
        21 => Some(("adUnsignedBigInt", 8)),
        64 => Some(("adFileTime", 8)),
        72 => Some(("adGUID", 16)),
        131 => Some(("adNumeric", 19)),
        133 => Some(("adDBDate", 6)),
        134 => Some(("adDBTime", 6)),
        135 => Some(("adDBTimeStamp", 16)),
        136 => Some(("adChapter", 4)),
        _ => None,
    };
    if let Some((type_name, expected_width)) = exact_width {
        require_adtg_width(descriptor, type_name, expected_width)?;
    }

    match descriptor.type_code {
        4 if descriptor.defined_size < 4 => bail!(
            "unsupported ADTG adSingle width {}",
            descriptor.defined_size
        ),
        5 => match descriptor.defined_size {
            4 | 8 => {}
            other => bail!("unsupported ADTG adDouble width {other}"),
        },
        _ => {}
    }

    Ok(())
}

fn decimal_descriptor_precision_scale(descriptor: &FieldDescriptor) -> Result<Option<(u8, u8)>> {
    if descriptor.precision == 255 && descriptor.scale == 255 {
        return Ok(None);
    }
    if !(1..=28).contains(&descriptor.precision) {
        bail!(
            "invalid ADTG adDecimal descriptor precision {} for field {}",
            descriptor.precision,
            descriptor.name
        );
    }
    if descriptor.scale > 28 {
        bail!(
            "invalid ADTG adDecimal descriptor scale {} for field {}",
            descriptor.scale,
            descriptor.name
        );
    }
    if descriptor.scale > descriptor.precision {
        bail!(
            "invalid ADTG adDecimal descriptor scale {} exceeds precision {} for field {}",
            descriptor.scale,
            descriptor.precision,
            descriptor.name
        );
    }
    Ok(Some((descriptor.precision as u8, descriptor.scale as u8)))
}

fn validate_varnumeric_descriptor_metadata(descriptor: &FieldDescriptor) -> Result<()> {
    if descriptor.defined_size < 4 {
        bail!(
            "unsupported ADTG adVarNumeric width {}",
            descriptor.defined_size
        );
    }
    Ok(())
}

fn push_change(
    rows: &mut Vec<Row>,
    changes: &mut Vec<RowChange>,
    kind: RowChangeKind,
    state: RowState,
    status: RecordStatusFlag,
    values: Vec<Value>,
) {
    let change_index = changes.len();
    let row_index = rows.len();
    rows.push(Row {
        ordinal: row_index,
        state,
        status_flags: vec![status],
        change_index: Some(change_index),
        values,
    });
    changes.push(RowChange {
        kind,
        row_indices: vec![row_index],
    });
}

fn row_presence_from_mask(
    bytes: &[u8],
    offset: &mut usize,
    descriptors: &[FieldDescriptor],
) -> Result<Vec<bool>> {
    let nullable_count = descriptors
        .iter()
        .filter(|descriptor| descriptor.attributes & (0x20 | 0x40) != 0)
        .count();
    if nullable_count == 0 {
        return Ok(vec![true; descriptors.len()]);
    }

    let mask = read_mask(bytes, offset, nullable_count)?;
    validate_row_presence_mask_padding(&mask, nullable_count, "row presence")?;
    let mut nullable_index = 0usize;
    let mut present = Vec::with_capacity(descriptors.len());
    for descriptor in descriptors {
        if descriptor.attributes & (0x20 | 0x40) != 0 {
            present.push(mask_bit(&mask, nullable_index));
            nullable_index += 1;
        } else {
            present.push(true);
        }
    }
    Ok(present)
}

fn insert_presence(
    bytes: &[u8],
    offset: &mut usize,
    descriptors: &[FieldDescriptor],
) -> Result<Vec<bool>> {
    let field_count = descriptors.len();
    let non_null = read_mask(bytes, offset, field_count)?;
    let nulls = read_mask(bytes, offset, field_count)?;
    validate_mask_padding(&non_null, field_count, "insert non-null")?;
    validate_mask_padding(&nulls, field_count, "insert null")?;
    validate_value_masks(&non_null, &nulls, descriptors, "insert", true)?;
    Ok((0..field_count)
        .map(|index| mask_bit(&non_null, index) && !mask_bit(&nulls, index))
        .collect())
}

fn read_update_values(
    bytes: &[u8],
    offset: &mut usize,
    descriptors: &[FieldDescriptor],
    options: &AdtgParseOptions,
) -> Result<Vec<Value>> {
    let non_null = read_mask(bytes, offset, descriptors.len())?;
    let nulls = read_mask(bytes, offset, descriptors.len())?;
    validate_mask_padding(&non_null, descriptors.len(), "update non-null")?;
    validate_mask_padding(&nulls, descriptors.len(), "update null")?;
    validate_value_masks(&non_null, &nulls, descriptors, "update", false)?;
    let mut values = vec![Value::Unavailable; descriptors.len()];
    for (index, descriptor) in descriptors.iter().enumerate() {
        if mask_bit(&non_null, index) {
            let start = *offset;
            values[index] = read_value(bytes, offset, descriptor, options).with_context(|| {
                format!(
                    "failed to read ADTG updated field {} type {} at offset {start:#x}",
                    descriptor.name, descriptor.type_code
                )
            })?;
        } else if mask_bit(&nulls, index) {
            values[index] = Value::Null;
        }
    }
    Ok(values)
}

fn validate_value_masks(
    non_null: &[u8],
    nulls: &[u8],
    descriptors: &[FieldDescriptor],
    context: &str,
    require_non_nullable_values: bool,
) -> Result<()> {
    for (index, descriptor) in descriptors.iter().enumerate() {
        if mask_bit(non_null, index) && mask_bit(nulls, index) {
            bail!(
                "conflicting ADTG {context} masks for field {}",
                descriptor.name
            );
        }
        if mask_bit(nulls, index) && !descriptor_allows_null_mask(descriptor) {
            bail!(
                "null ADTG {context} mask for non-nullable field {}",
                descriptor.name
            );
        }
        if require_non_nullable_values
            && !mask_bit(non_null, index)
            && !mask_bit(nulls, index)
            && !descriptor_allows_null_mask(descriptor)
        {
            bail!(
                "missing ADTG {context} value for non-nullable field {}",
                descriptor.name
            );
        }
    }
    Ok(())
}

fn descriptor_allows_null_mask(descriptor: &FieldDescriptor) -> bool {
    descriptor.type_code == 136 || descriptor.attributes & (0x20 | 0x40) != 0
}

fn validate_mask_padding(mask: &[u8], field_count: usize, context: &str) -> Result<()> {
    let used_bits = field_count % 8;
    if used_bits == 0 {
        return Ok(());
    }

    let unused_mask = (1u8 << (8 - used_bits)) - 1;
    if mask.last().copied().unwrap_or(0) & unused_mask != 0 {
        bail!("unused ADTG {context} mask bits set for {field_count} fields");
    }
    Ok(())
}

fn validate_row_presence_mask_padding(
    mask: &[u8],
    field_count: usize,
    context: &str,
) -> Result<()> {
    let used_bits = field_count % 8;
    if used_bits == 0 {
        return Ok(());
    }

    let unused_mask = (1u8 << (8 - used_bits)) - 1;
    if mask.last().copied().unwrap_or(0) & unused_mask != unused_mask {
        bail!("unset unused ADTG {context} mask bits for {field_count} fields");
    }
    Ok(())
}

fn read_mask(bytes: &[u8], offset: &mut usize, field_count: usize) -> Result<Vec<u8>> {
    let byte_count = field_count.div_ceil(8).max(1);
    let end = offset
        .checked_add(byte_count)
        .ok_or_else(|| anyhow!("ADTG field mask offset overflow"))?;
    let mask = bytes
        .get(*offset..end)
        .ok_or_else(|| anyhow!("truncated ADTG field mask"))?
        .to_vec();
    *offset = end;
    Ok(mask)
}

fn mask_bit(mask: &[u8], index: usize) -> bool {
    let byte = mask[index / 8];
    let bit = 0x80 >> (index % 8);
    byte & bit != 0
}

fn read_values(
    bytes: &[u8],
    offset: &mut usize,
    descriptors: &[FieldDescriptor],
    present: &[bool],
    options: &AdtgParseOptions,
) -> Result<Vec<Value>> {
    let mut values = Vec::with_capacity(descriptors.len());
    for (descriptor, present) in descriptors.iter().zip(present.iter().copied()) {
        if present {
            let start = *offset;
            values.push(
                read_value(bytes, offset, descriptor, options).with_context(|| {
                    format!(
                        "failed to read ADTG field {} type {} at offset {start:#x}",
                        descriptor.name, descriptor.type_code
                    )
                })?,
            );
        } else {
            values.push(Value::Null);
        }
    }
    Ok(values)
}

fn visible_values(values: Vec<Value>, descriptors: &[FieldDescriptor]) -> Vec<Value> {
    values
        .into_iter()
        .zip(descriptors)
        .filter_map(|(value, descriptor)| (!descriptor.hidden).then_some(value))
        .collect()
}

fn read_value(
    bytes: &[u8],
    offset: &mut usize,
    descriptor: &FieldDescriptor,
    options: &AdtgParseOptions,
) -> Result<Value> {
    let value = match descriptor.type_code {
        2 => Value::Integer(
            read_i16_width_advance(bytes, offset, descriptor, "adSmallInt", 2)? as i64,
        ),
        3 => Value::Integer(
            read_i32_width_advance(bytes, offset, descriptor, "adInteger", 4)? as i64,
        ),
        4 => Value::Float(read_single_advance(bytes, offset, descriptor)?),
        5 => Value::Float(read_double_advance(bytes, offset, descriptor)?),
        6 => Value::Decimal(format_scaled_i128(
            read_i64_width_advance(bytes, offset, descriptor, "adCurrency", 8)? as i128,
            4,
        )?),
        7 => Value::DateTime(ole_datetime_to_string(read_f64_width_advance(
            bytes, offset, descriptor, "adDate", 8,
        )?)?),
        11 => Value::Boolean(read_boolean_advance(bytes, offset, descriptor)?),
        12 => read_variant_advance(bytes, offset, descriptor, options)?,
        14 => Value::Decimal(read_decimal_advance(bytes, offset, descriptor)?),
        16 => {
            Value::Integer(read_i8_width_advance(bytes, offset, descriptor, "adTinyInt", 1)? as i64)
        }
        17 => Value::UnsignedInteger(read_u8_width_advance(
            bytes,
            offset,
            descriptor,
            "adUnsignedTinyInt",
            1,
        )? as u64),
        18 => Value::UnsignedInteger(read_u16_width_advance(
            bytes,
            offset,
            descriptor,
            "adUnsignedSmallInt",
            2,
        )? as u64),
        19 => Value::UnsignedInteger(read_u32_width_advance(
            bytes,
            offset,
            descriptor,
            "adUnsignedInt",
            4,
        )? as u64),
        20 => Value::Integer(read_i64_width_advance(
            bytes, offset, descriptor, "adBigInt", 8,
        )?),
        21 => Value::UnsignedInteger(read_u64_width_advance(
            bytes,
            offset,
            descriptor,
            "adUnsignedBigInt",
            8,
        )?),
        64 => Value::DateTime(read_filetime_advance(bytes, offset, descriptor)?),
        72 => Value::Guid(read_guid_advance(bytes, offset, descriptor, options)?),
        128 => Value::BinaryHex(read_binary_advance(bytes, offset, descriptor, options)?),
        129 | 130 => Value::String(read_string_advance(bytes, offset, descriptor, options)?),
        131 => Value::Decimal(read_numeric_advance(bytes, offset, descriptor)?),
        139 => Value::Decimal(read_varnumeric_advance(bytes, offset, descriptor, options)?),
        133 => Value::Date(read_dbdate_advance(bytes, offset, descriptor)?),
        134 => Value::Time(read_dbtime_advance(bytes, offset, descriptor)?),
        135 => Value::DateTime(read_dbtimestamp_advance(bytes, offset, descriptor)?),
        other => bail!("unsupported native ADTG field type {other}"),
    };
    Ok(value)
}

fn read_binary_advance(
    bytes: &[u8],
    offset: &mut usize,
    descriptor: &FieldDescriptor,
    options: &AdtgParseOptions,
) -> Result<String> {
    let mut value = read_bytes_advance(bytes, offset, descriptor, options.resource_limits)?;
    normalize_ado_binary_bytes(&mut value);
    let encoded = hex::encode_upper(value);
    options
        .resource_limits
        .check_value_bytes(encoded.len(), &format!("ADTG field {}", descriptor.name))?;
    Ok(encoded)
}

fn read_single_advance(
    bytes: &[u8],
    offset: &mut usize,
    descriptor: &FieldDescriptor,
) -> Result<f64> {
    if descriptor.defined_size < 4 {
        bail!(
            "unsupported ADTG adSingle width {}",
            descriptor.defined_size
        );
    }

    let value = ensure_finite_f32(read_f32_advance(bytes, offset)?, "ADTG adSingle value")? as f64;
    if descriptor.defined_size > 4 {
        let padding_len = (descriptor.defined_size - 4) as usize;
        take(bytes, offset, padding_len)?;
    }
    Ok(value)
}

fn read_double_advance(
    bytes: &[u8],
    offset: &mut usize,
    descriptor: &FieldDescriptor,
) -> Result<f64> {
    match descriptor.defined_size {
        4 => Ok(ensure_finite_f32(
            read_f32_advance(bytes, offset)?,
            "ADTG 4-byte adDouble value",
        )? as f64),
        8 => ensure_finite_f64(read_f64_advance(bytes, offset)?, "ADTG adDouble value"),
        other => bail!("unsupported ADTG adDouble width {other}"),
    }
}

fn read_i8_width_advance(
    bytes: &[u8],
    offset: &mut usize,
    descriptor: &FieldDescriptor,
    type_name: &str,
    expected_width: u32,
) -> Result<i8> {
    require_adtg_width(descriptor, type_name, expected_width)?;
    read_i8_advance(bytes, offset)
}

fn read_u8_width_advance(
    bytes: &[u8],
    offset: &mut usize,
    descriptor: &FieldDescriptor,
    type_name: &str,
    expected_width: u32,
) -> Result<u8> {
    require_adtg_width(descriptor, type_name, expected_width)?;
    read_u8_advance(bytes, offset)
}

fn read_i16_width_advance(
    bytes: &[u8],
    offset: &mut usize,
    descriptor: &FieldDescriptor,
    type_name: &str,
    expected_width: u32,
) -> Result<i16> {
    require_adtg_width(descriptor, type_name, expected_width)?;
    read_i16_advance(bytes, offset)
}

fn read_u16_width_advance(
    bytes: &[u8],
    offset: &mut usize,
    descriptor: &FieldDescriptor,
    type_name: &str,
    expected_width: u32,
) -> Result<u16> {
    require_adtg_width(descriptor, type_name, expected_width)?;
    read_u16_advance(bytes, offset)
}

fn read_i32_width_advance(
    bytes: &[u8],
    offset: &mut usize,
    descriptor: &FieldDescriptor,
    type_name: &str,
    expected_width: u32,
) -> Result<i32> {
    require_adtg_width(descriptor, type_name, expected_width)?;
    read_i32_advance(bytes, offset)
}

fn read_u32_width_advance(
    bytes: &[u8],
    offset: &mut usize,
    descriptor: &FieldDescriptor,
    type_name: &str,
    expected_width: u32,
) -> Result<u32> {
    require_adtg_width(descriptor, type_name, expected_width)?;
    read_u32_advance(bytes, offset)
}

fn read_i64_width_advance(
    bytes: &[u8],
    offset: &mut usize,
    descriptor: &FieldDescriptor,
    type_name: &str,
    expected_width: u32,
) -> Result<i64> {
    require_adtg_width(descriptor, type_name, expected_width)?;
    read_i64_advance(bytes, offset)
}

fn read_u64_width_advance(
    bytes: &[u8],
    offset: &mut usize,
    descriptor: &FieldDescriptor,
    type_name: &str,
    expected_width: u32,
) -> Result<u64> {
    require_adtg_width(descriptor, type_name, expected_width)?;
    read_u64_advance(bytes, offset)
}

fn read_f64_width_advance(
    bytes: &[u8],
    offset: &mut usize,
    descriptor: &FieldDescriptor,
    type_name: &str,
    expected_width: u32,
) -> Result<f64> {
    require_adtg_width(descriptor, type_name, expected_width)?;
    read_f64_advance(bytes, offset)
}

fn read_filetime_advance(
    bytes: &[u8],
    offset: &mut usize,
    descriptor: &FieldDescriptor,
) -> Result<String> {
    require_adtg_width(descriptor, "adFileTime", 8)?;
    filetime_to_string(read_u64_advance(bytes, offset)?)
}

fn read_boolean_advance(
    bytes: &[u8],
    offset: &mut usize,
    descriptor: &FieldDescriptor,
) -> Result<bool> {
    let value = read_i16_width_advance(bytes, offset, descriptor, "adBoolean", 2)?;
    boolean_word_value(value, "ADTG adBoolean value")
}

fn boolean_word_value(value: i16, context: &str) -> Result<bool> {
    match value {
        0 => Ok(false),
        -1 => Ok(true),
        other => bail!("invalid {context} {other}"),
    }
}

fn read_variant_advance(
    bytes: &[u8],
    offset: &mut usize,
    descriptor: &FieldDescriptor,
    options: &AdtgParseOptions,
) -> Result<Value> {
    require_adtg_width(descriptor, "adVariant", 16)?;
    let raw = read_bytes_advance(bytes, offset, descriptor, options.resource_limits)?;
    if raw.len() != 16 {
        bail!("unsupported ADTG adVariant payload length {}", raw.len());
    }

    let vt = u16::from_le_bytes([raw[0], raw[1]]);
    let value = match vt {
        0 => Value::Empty,
        1 => Value::Null,
        2 => Value::Decimal(i16::from_le_bytes([raw[8], raw[9]]).to_string()),
        3 => Value::Decimal(i32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]).to_string()),
        4 => Value::Decimal(
            ensure_finite_f32(
                f32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]),
                "ADTG variant R4 value",
            )?
            .to_string(),
        ),
        5 => Value::Decimal(
            ensure_finite_f64(
                f64::from_le_bytes([
                    raw[8], raw[9], raw[10], raw[11], raw[12], raw[13], raw[14], raw[15],
                ]),
                "ADTG variant R8 value",
            )?
            .to_string(),
        ),
        6 => Value::Decimal(format_scaled_i128(
            i64::from_le_bytes([
                raw[8], raw[9], raw[10], raw[11], raw[12], raw[13], raw[14], raw[15],
            ]) as i128,
            4,
        )?),
        7 => Value::DateTime(ole_datetime_to_string(f64::from_le_bytes([
            raw[8], raw[9], raw[10], raw[11], raw[12], raw[13], raw[14], raw[15],
        ]))?),
        11 => Value::Boolean(boolean_word_value(
            i16::from_le_bytes([raw[8], raw[9]]),
            "ADTG variant VT_BOOL value",
        )?),
        14 => Value::Decimal(read_decimal_bytes(
            &raw[0..16],
            &[14],
            "ADTG variant DECIMAL value",
            None,
            None,
        )?),
        16 => Value::Decimal((raw[8] as i8).to_string()),
        17 => Value::Decimal(raw[8].to_string()),
        18 => Value::Decimal(u16::from_le_bytes([raw[8], raw[9]]).to_string()),
        19 => Value::Decimal(u32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]).to_string()),
        20 => Value::Decimal(
            i64::from_le_bytes([
                raw[8], raw[9], raw[10], raw[11], raw[12], raw[13], raw[14], raw[15],
            ])
            .to_string(),
        ),
        21 => Value::Decimal(
            u64::from_le_bytes([
                raw[8], raw[9], raw[10], raw[11], raw[12], raw[13], raw[14], raw[15],
            ])
            .to_string(),
        ),
        other => bail!("unsupported native ADTG variant subtype {other}"),
    };
    Ok(value)
}

fn ensure_finite_f32(value: f32, context: &str) -> Result<f32> {
    if !value.is_finite() {
        bail!("non-finite {context}");
    }
    Ok(value)
}

fn ensure_finite_f64(value: f64, context: &str) -> Result<f64> {
    if !value.is_finite() {
        bail!("non-finite {context}");
    }
    Ok(value)
}

fn read_string_advance(
    bytes: &[u8],
    offset: &mut usize,
    descriptor: &FieldDescriptor,
    options: &AdtgParseOptions,
) -> Result<String> {
    let raw = read_bytes_advance(bytes, offset, descriptor, options.resource_limits)?;
    options
        .resource_limits
        .check_value_bytes(raw.len(), &format!("ADTG field {}", descriptor.name))?;
    if descriptor.type_code == 130 {
        decode_utf16le(&raw)
    } else {
        Ok(decode_ansi_bytes(&raw, options))
    }
}

fn decode_ansi_bytes(bytes: &[u8], options: &AdtgParseOptions) -> String {
    if options.utf8_first_for_ansi {
        if let Ok(text) = std::str::from_utf8(bytes) {
            return text.to_string();
        }
    }

    if let Some(encoding) = options.ansi_encoding {
        let (decoded, _, had_errors) = encoding.decode(bytes);
        if !had_errors {
            return decoded.into_owned();
        }
    }

    String::from_utf8_lossy(bytes).to_string()
}

fn read_bytes_advance(
    bytes: &[u8],
    offset: &mut usize,
    descriptor: &FieldDescriptor,
    limits: ResourceLimits,
) -> Result<Vec<u8>> {
    let is_fixed = descriptor.attributes & 0x10 != 0;
    let is_long = descriptor.attributes & 0x80 != 0;
    let max_value_bytes = match descriptor.type_code {
        130 => descriptor.defined_size.saturating_mul(2),
        _ => descriptor.defined_size,
    };
    let byte_len = if is_fixed && !is_long && max_value_bytes <= 255 {
        match descriptor.type_code {
            130 => descriptor.defined_size as usize * 2,
            _ => descriptor.defined_size as usize,
        }
    } else if max_value_bytes > 255 {
        read_u32_advance(bytes, offset)? as usize
    } else {
        read_u8_advance(bytes, offset)? as usize
    };
    limits.check_value_bytes(byte_len, &format!("ADTG field {}", descriptor.name))?;
    let max_value_bytes = max_value_bytes as usize;
    if !is_long && byte_len > max_value_bytes {
        bail!(
            "ADTG variable value length {byte_len} exceeds defined byte length {max_value_bytes} for field {}",
            descriptor.name
        );
    }
    let end = offset
        .checked_add(byte_len)
        .ok_or_else(|| anyhow!("ADTG variable value offset overflow"))?;
    let value = bytes
        .get(*offset..end)
        .ok_or_else(|| anyhow!("truncated ADTG variable value"))?
        .to_vec();
    *offset = end;
    Ok(value)
}

fn read_decimal_advance(
    bytes: &[u8],
    offset: &mut usize,
    descriptor: &FieldDescriptor,
) -> Result<String> {
    if descriptor.defined_size != 16 {
        bail!(
            "unsupported ADTG adDecimal width {}",
            descriptor.defined_size
        );
    }

    let raw = take(bytes, offset, 16)?;
    let descriptor_metadata = decimal_descriptor_precision_scale(descriptor)?;
    let (precision, expected_scale) = descriptor_metadata
        .map(|(precision, scale)| (Some(precision), Some(scale)))
        .unwrap_or((None, None));
    read_decimal_bytes(
        raw,
        &[0, 14],
        "ADTG adDecimal value",
        precision,
        expected_scale,
    )
}

fn read_decimal_bytes(
    raw: &[u8],
    expected_reserved: &[u16],
    context: &str,
    precision: Option<u8>,
    expected_scale: Option<u8>,
) -> Result<String> {
    let reserved = u16::from_le_bytes([raw[0], raw[1]]);
    if !expected_reserved.contains(&reserved) {
        bail!("invalid {context} reserved word {reserved}");
    }
    let scale = raw[2] as u32;
    if scale > 28 {
        bail!("invalid {context} scale {scale}");
    }
    let negative = match raw[3] {
        0x00 => false,
        0x80 => true,
        other => bail!("invalid {context} sign byte {other:#04x}"),
    };
    let magnitude = u128::from_le_bytes([
        raw[8], raw[9], raw[10], raw[11], raw[12], raw[13], raw[14], raw[15], raw[4], raw[5],
        raw[6], raw[7], 0, 0, 0, 0,
    ]);
    let scale = if let Some(expected) = expected_scale {
        let expected = expected as u32;
        let mdac_xml_zero_decimal = reserved == 14 && scale == 0 && expected > 0 && magnitude == 0;
        if scale != expected && !mdac_xml_zero_decimal {
            bail!("invalid {context} scale {scale} does not match descriptor scale {expected}");
        }
        if mdac_xml_zero_decimal {
            expected
        } else {
            scale
        }
    } else {
        scale
    };
    if let Some(precision) = precision {
        validate_numeric_magnitude_precision(magnitude, precision, context)?;
    }
    format_scaled_u128(magnitude, scale, negative)
}

fn read_numeric_advance(
    bytes: &[u8],
    offset: &mut usize,
    descriptor: &FieldDescriptor,
) -> Result<String> {
    if descriptor.defined_size != 19 {
        bail!(
            "unsupported ADTG adNumeric width {}",
            descriptor.defined_size
        );
    }

    let raw = take(bytes, offset, 19)?;
    let precision = numeric_precision_value(raw[0], "ADTG adNumeric value")?;
    let scale = numeric_scale_value(raw[1], precision, "ADTG adNumeric value")?;
    let negative = numeric_sign_value(raw[2], "ADTG adNumeric value")?;
    let mut magnitude_bytes = [0u8; 16];
    magnitude_bytes.copy_from_slice(&raw[3..19]);
    let magnitude = u128::from_le_bytes(magnitude_bytes);
    validate_numeric_magnitude_precision(magnitude, precision, "ADTG adNumeric value")?;
    format_scaled_u128(magnitude, scale, negative)
}

fn read_dbdate_advance(
    bytes: &[u8],
    offset: &mut usize,
    descriptor: &FieldDescriptor,
) -> Result<String> {
    require_adtg_width(descriptor, "adDBDate", 6)?;
    let year = read_u16_advance(bytes, offset)?;
    let month = read_u16_advance(bytes, offset)?;
    let day = read_u16_advance(bytes, offset)?;
    format_db_date(year, month, day)
}

fn read_dbtime_advance(
    bytes: &[u8],
    offset: &mut usize,
    descriptor: &FieldDescriptor,
) -> Result<String> {
    require_adtg_width(descriptor, "adDBTime", 6)?;
    let hour = read_u16_advance(bytes, offset)?;
    let minute = read_u16_advance(bytes, offset)?;
    let second = read_u16_advance(bytes, offset)?;
    format_db_time(hour, minute, second)
}

fn read_dbtimestamp_advance(
    bytes: &[u8],
    offset: &mut usize,
    descriptor: &FieldDescriptor,
) -> Result<String> {
    require_adtg_width(descriptor, "adDBTimeStamp", 16)?;
    let year = read_u16_advance(bytes, offset)?;
    let month = read_u16_advance(bytes, offset)?;
    let day = read_u16_advance(bytes, offset)?;
    let hour = read_u16_advance(bytes, offset)?;
    let minute = read_u16_advance(bytes, offset)?;
    let second = read_u16_advance(bytes, offset)?;
    let fraction = read_u32_advance(bytes, offset)?;
    format_db_timestamp(year, month, day, hour, minute, second, fraction)
}

fn read_varnumeric_advance(
    bytes: &[u8],
    offset: &mut usize,
    descriptor: &FieldDescriptor,
    options: &AdtgParseOptions,
) -> Result<String> {
    let raw = read_bytes_advance(bytes, offset, descriptor, options.resource_limits)?;
    if raw.len() < 4 {
        bail!("truncated ADTG varnumeric value");
    }

    let precision = numeric_precision_value(raw[0], "ADTG varnumeric value")?;
    let scale = numeric_scale_value(raw[1], precision, "ADTG varnumeric value")?;
    let negative = numeric_sign_value(raw[2], "ADTG varnumeric value")?;
    let magnitude = raw[3..]
        .iter()
        .enumerate()
        .try_fold(0u128, |acc, (index, byte)| {
            if index >= 16 {
                bail!("ADTG varnumeric magnitude is wider than 128 bits");
            }
            Ok(acc | ((*byte as u128) << (index * 8)))
        })?;
    validate_numeric_magnitude_precision(magnitude, precision, "ADTG varnumeric value")?;
    format_scaled_u128(magnitude, scale, negative)
}

fn numeric_precision_value(value: u8, context: &str) -> Result<u8> {
    if !(1..=38).contains(&value) {
        bail!("invalid {context} precision {value}");
    }
    Ok(value)
}

fn numeric_scale_value(value: u8, precision: u8, context: &str) -> Result<u32> {
    if value > 38 {
        bail!("invalid {context} scale {value}");
    }
    if value > precision {
        bail!("invalid {context} scale {value} exceeds precision {precision}");
    }
    Ok(value as u32)
}

fn numeric_sign_value(value: u8, context: &str) -> Result<bool> {
    match value {
        0 => Ok(true),
        1 => Ok(false),
        other => bail!("invalid {context} sign byte {other:#04x}"),
    }
}

fn validate_numeric_magnitude_precision(
    magnitude: u128,
    precision: u8,
    context: &str,
) -> Result<()> {
    let limit = 10u128
        .checked_pow(precision as u32)
        .ok_or_else(|| anyhow!("unsupported {context} precision {precision}"))?;
    if magnitude >= limit {
        bail!("invalid {context} magnitude exceeds precision {precision}");
    }
    Ok(())
}

fn read_guid_advance(
    bytes: &[u8],
    offset: &mut usize,
    descriptor: &FieldDescriptor,
    options: &AdtgParseOptions,
) -> Result<String> {
    require_adtg_width(descriptor, "adGUID", 16)?;
    let raw = take(bytes, offset, 16)?;
    let d1 = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
    let d2 = u16::from_le_bytes([raw[4], raw[5]]);
    let d3 = u16::from_le_bytes([raw[6], raw[7]]);
    let guid = format!(
        "{{{d1:08X}-{d2:04X}-{d3:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        raw[8], raw[9], raw[10], raw[11], raw[12], raw[13], raw[14], raw[15]
    );
    options
        .resource_limits
        .check_value_bytes(guid.len(), &format!("ADTG field {}", descriptor.name))?;
    Ok(guid)
}

fn format_db_date(year: u16, month: u16, day: u16) -> Result<String> {
    validate_db_date(year, month, day)?;
    Ok(format!("{year:04}-{month:02}-{day:02}"))
}

fn format_db_time(hour: u16, minute: u16, second: u16) -> Result<String> {
    validate_db_time(hour, minute, second)?;
    Ok(format!("{hour:02}:{minute:02}:{second:02}"))
}

fn format_db_timestamp(
    year: u16,
    month: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    fraction: u32,
) -> Result<String> {
    validate_db_date(year, month, day)?;
    validate_db_time(hour, minute, second)?;
    if fraction >= 1_000_000_000 {
        bail!("invalid ADTG DBTIMESTAMP fraction {fraction}");
    }

    let mut value = format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}");
    if fraction != 0 {
        let fraction = format!("{fraction:09}").trim_end_matches('0').to_string();
        value.push('.');
        value.push_str(&fraction);
    }
    Ok(value)
}

fn validate_db_date(year: u16, month: u16, day: u16) -> Result<()> {
    if !(1..=9999).contains(&year) {
        bail!("invalid ADTG DBDATE year {year}");
    }
    let Some(max_day) = gregorian_month_len(year, month) else {
        bail!("invalid ADTG DBDATE month {month}");
    };
    if !(1..=max_day).contains(&day) {
        bail!("invalid ADTG DBDATE day {day} for {year:04}-{month:02}");
    }
    Ok(())
}

fn validate_db_time(hour: u16, minute: u16, second: u16) -> Result<()> {
    if hour > 23 || minute > 59 || second > 59 {
        bail!("invalid ADTG DBTIME {hour:02}:{minute:02}:{second:02}");
    }
    Ok(())
}

fn ole_datetime_to_string(value: f64) -> Result<String> {
    if !value.is_finite() {
        bail!("non-finite ADTG OLE date");
    }

    let has_fraction = (value.fract()).abs() > f64::EPSILON;
    let mut days = if value < 0.0 && has_fraction {
        value.ceil() as i128
    } else {
        value.floor() as i128
    };
    let mut seconds = if value < 0.0 && has_fraction {
        ((days as f64 - value) * 86_400.0).round() as i128
    } else {
        ((value - days as f64) * 86_400.0).round() as i128
    };
    if seconds >= 86_400 {
        days = days
            .checked_add(seconds / 86_400)
            .ok_or_else(|| anyhow!("ADTG OLE date is out of range"))?;
        seconds %= 86_400;
    }
    if !(-1_000_000_000..=1_000_000_000).contains(&days) {
        bail!("ADTG OLE date is out of range");
    }
    let (year, month, day) = civil_from_days(days - 25_569);
    if !(1..=9999).contains(&year) {
        bail!("ADTG OLE date year {year} is out of range");
    }
    let hour = seconds / 3600;
    let minute = (seconds % 3600) / 60;
    let second = seconds % 60;
    Ok(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}"
    ))
}

fn filetime_to_string(value: u64) -> Result<String> {
    let unix_seconds = (value / 10_000_000) as i128 - 11_644_473_600i128;
    let days = unix_seconds.div_euclid(86_400);
    let seconds = unix_seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    if !(1601..=9999).contains(&year) {
        bail!("ADTG FILETIME year {year} is out of range");
    }
    let hour = seconds / 3600;
    let minute = (seconds % 3600) / 60;
    let second = seconds % 60;
    Ok(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}"
    ))
}

fn civil_from_days(days: i128) -> (i128, i128, i128) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + if m <= 2 { 1 } else { 0 }, m, d)
}

fn format_scaled_i128(value: i128, scale: u32) -> Result<String> {
    if value < 0 {
        format_scaled_u128(value.unsigned_abs(), scale, true)
    } else {
        format_scaled_u128(value as u128, scale, false)
    }
}

fn format_scaled_u128(value: u128, scale: u32, negative: bool) -> Result<String> {
    let Some(factor) = 10u128.checked_pow(scale) else {
        bail!("unsupported ADTG decimal scale {scale}");
    };
    let whole = value / factor;
    let fraction = value % factor;
    let formatted = if scale == 0 {
        if negative {
            format!("-{whole}")
        } else {
            whole.to_string()
        }
    } else {
        let sign = if negative { "-" } else { "" };
        format!("{sign}{whole}.{fraction:0width$}", width = scale as usize)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    };
    Ok(formatted)
}

fn adtg_data_type_name(code: u16) -> &'static str {
    match code {
        2 | 3 | 16 | 20 => "int",
        17 | 18 | 19 | 21 => "uint",
        4 | 5 => "float",
        6 | 14 | 131 | 139 => "number",
        7 | 64 | 135 => "datetime",
        11 => "boolean",
        72 => "uuid",
        128 | 204 | 205 => "bin.hex",
        12 => "variant",
        136 => "chapter",
        133 => "date",
        134 => "time",
        _ => "string",
    }
}

fn normalize_ado_binary_bytes(bytes: &mut [u8]) {
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

fn decode_utf16le(bytes: &[u8]) -> Result<String> {
    if !bytes.len().is_multiple_of(2) {
        bail!("odd-length UTF-16LE text");
    }
    let units = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]));
    char::decode_utf16(units)
        .map(|item| item.map_err(|_| anyhow!("invalid UTF-16LE text")))
        .collect()
}

fn read_i8_advance(bytes: &[u8], offset: &mut usize) -> Result<i8> {
    Ok(read_u8_advance(bytes, offset)? as i8)
}

fn read_u8_advance(bytes: &[u8], offset: &mut usize) -> Result<u8> {
    let value = *bytes
        .get(*offset)
        .ok_or_else(|| anyhow!("truncated ADTG u8"))?;
    *offset += 1;
    Ok(value)
}

fn read_i16_advance(bytes: &[u8], offset: &mut usize) -> Result<i16> {
    let value = i16::from_le_bytes(take_array(bytes, offset)?);
    Ok(value)
}

fn read_u16_advance(bytes: &[u8], offset: &mut usize) -> Result<u16> {
    let value = u16::from_le_bytes(take_array(bytes, offset)?);
    Ok(value)
}

fn read_i32_advance(bytes: &[u8], offset: &mut usize) -> Result<i32> {
    let value = i32::from_le_bytes(take_array(bytes, offset)?);
    Ok(value)
}

fn read_u32_advance(bytes: &[u8], offset: &mut usize) -> Result<u32> {
    let value = u32::from_le_bytes(take_array(bytes, offset)?);
    Ok(value)
}

fn read_i64_advance(bytes: &[u8], offset: &mut usize) -> Result<i64> {
    let value = i64::from_le_bytes(take_array(bytes, offset)?);
    Ok(value)
}

fn read_u64_advance(bytes: &[u8], offset: &mut usize) -> Result<u64> {
    let value = u64::from_le_bytes(take_array(bytes, offset)?);
    Ok(value)
}

fn read_f32_advance(bytes: &[u8], offset: &mut usize) -> Result<f32> {
    let value = f32::from_le_bytes(take_array(bytes, offset)?);
    Ok(value)
}

fn read_f64_advance(bytes: &[u8], offset: &mut usize) -> Result<f64> {
    let value = f64::from_le_bytes(take_array(bytes, offset)?);
    Ok(value)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    let end = offset
        .checked_add(2)
        .ok_or_else(|| anyhow!("ADTG u16 offset overflow at offset {offset:#x}"))?;
    let chunk = bytes
        .get(offset..end)
        .ok_or_else(|| anyhow!("truncated ADTG u16 at offset {offset:#x}"))?;
    Ok(u16::from_le_bytes([chunk[0], chunk[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| anyhow!("ADTG u32 offset overflow at offset {offset:#x}"))?;
    let chunk = bytes
        .get(offset..end)
        .ok_or_else(|| anyhow!("truncated ADTG u32 at offset {offset:#x}"))?;
    Ok(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
}

fn take<'a>(bytes: &'a [u8], offset: &mut usize, len: usize) -> Result<&'a [u8]> {
    let end = offset
        .checked_add(len)
        .ok_or_else(|| anyhow!("ADTG value offset overflow"))?;
    let chunk = bytes
        .get(*offset..end)
        .ok_or_else(|| anyhow!("truncated ADTG value"))?;
    *offset = end;
    Ok(chunk)
}

fn take_array<const N: usize>(bytes: &[u8], offset: &mut usize) -> Result<[u8; N]> {
    let chunk = take(bytes, offset, N)?;
    let mut out = [0u8; N];
    out.copy_from_slice(chunk);
    Ok(out)
}

fn require_adtg_width(
    descriptor: &FieldDescriptor,
    type_name: &str,
    expected_width: u32,
) -> Result<()> {
    if descriptor.defined_size != expected_width {
        bail!(
            "unsupported ADTG {type_name} width {}",
            descriptor.defined_size
        );
    }
    Ok(())
}

pub fn detect_strings(bytes: &[u8], limit: usize) -> Vec<DetectedString> {
    let mut strings = Vec::new();
    strings.extend(scan_ascii(bytes, limit));
    strings.extend(scan_korean_ansi(bytes, limit.saturating_sub(strings.len())));
    strings.extend(scan_utf16le(bytes, limit.saturating_sub(strings.len())));
    strings.sort_by_key(|item| item.offset);
    strings.truncate(limit);
    strings
}

fn scan_ascii(bytes: &[u8], limit: usize) -> Vec<DetectedString> {
    let mut out = Vec::new();
    let mut offset = 0;

    while offset < bytes.len() && out.len() < limit {
        if !is_ascii_text_byte(bytes[offset]) {
            offset += 1;
            continue;
        }

        let start = offset;
        while offset < bytes.len() && is_ascii_text_byte(bytes[offset]) {
            offset += 1;
        }

        if offset - start >= 4 {
            let text = String::from_utf8_lossy(&bytes[start..offset]).to_string();
            out.push(DetectedString {
                offset: start,
                encoding: DetectedEncoding::Ascii,
                text,
            });
        }
    }

    out
}

fn scan_korean_ansi(bytes: &[u8], limit: usize) -> Vec<DetectedString> {
    let mut out = Vec::new();
    let mut offset = 0;

    while offset < bytes.len() && out.len() < limit {
        if !is_ansi_text_byte(bytes[offset]) || bytes[offset].is_ascii() {
            offset += 1;
            continue;
        }

        let start = offset;
        while offset < bytes.len() && is_ansi_text_byte(bytes[offset]) {
            offset += 1;
        }

        if offset - start >= 4 {
            let slice = &bytes[start..offset];
            let (decoded, _, had_errors) = EUC_KR.decode(slice);
            if !had_errors && !decoded.is_ascii() {
                out.push(DetectedString {
                    offset: start,
                    encoding: DetectedEncoding::KoreanAnsi,
                    text: decoded.into_owned(),
                });
            }
        }
    }

    out
}

fn scan_utf16le(bytes: &[u8], limit: usize) -> Vec<DetectedString> {
    let mut out = Vec::new();
    let mut offset = 0usize;

    while offset.checked_add(1).is_some_and(|end| end < bytes.len()) && out.len() < limit {
        let mut pos = offset;
        let mut code_units = Vec::new();

        while pos.checked_add(1).is_some_and(|end| end < bytes.len()) {
            let unit = u16::from_le_bytes([bytes[pos], bytes[pos + 1]]);
            if unit == 0 {
                break;
            }
            if is_bad_utf16_text_unit(unit) {
                break;
            }
            code_units.push(unit);
            pos += 2;
        }

        if code_units.len() >= 3 {
            let decoded: String = char::decode_utf16(code_units.iter().copied())
                .map(|item| item.unwrap_or(char::REPLACEMENT_CHARACTER))
                .collect();
            if decoded
                .chars()
                .all(|ch| !ch.is_control() || matches!(ch, '\t' | '\r' | '\n'))
            {
                out.push(DetectedString {
                    offset,
                    encoding: DetectedEncoding::Utf16Le,
                    text: decoded,
                });
                offset = pos.saturating_add(2);
                continue;
            }
        }

        offset += 1;
    }

    out
}

fn is_ascii_text_byte(byte: u8) -> bool {
    matches!(byte, b' '..=b'~' | b'\t')
}

fn is_ansi_text_byte(byte: u8) -> bool {
    matches!(byte, b' '..=b'~') || byte >= 0x80
}

fn is_bad_utf16_text_unit(unit: u16) -> bool {
    matches!(unit, 0x0001..=0x0008 | 0x000b | 0x000c | 0x000e..=0x001f)
        || (0xd800..=0xdfff).contains(&unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_chapter_row_tree_handles_many_sibling_prefixes() {
        let child_count = 12u32;
        let rows_per_child = 8usize;
        let child_schemas = (0..child_count)
            .map(|index| test_chapter_schema(Some(index + 2), Vec::new()))
            .collect::<Vec<_>>();
        let schema = test_chapter_schema(None, child_schemas);

        let mut bytes = vec![ChapterRowMarker::Root.current_marker()];
        for group_id in 2..(child_count + 2) {
            for _ in 0..rows_per_child {
                bytes.push(ChapterRowMarker::Child.current_marker());
                bytes.extend_from_slice(&group_id.to_le_bytes());
            }
        }
        bytes.push(0x0f);

        let (raw, offset) = read_chapter_row_tree(&bytes, 0, &schema, &AdtgParseOptions::default())
            .expect("deterministic chapter row parser should not enumerate prefixes");

        assert_eq!(offset, bytes.len());
        assert_eq!(raw.rows.len(), 1);
        assert_eq!(raw.child_groups.len(), child_count as usize);
        for child_group in raw.child_groups {
            assert_eq!(child_group.rows.len(), rows_per_child);
            assert!(child_group.child_groups.is_empty());
        }
    }

    #[test]
    fn deterministic_chapter_row_tree_handles_nested_prefix_patterns() {
        let root_child_count = 8u32;
        let rows_per_child = 6usize;
        let grand_rows_per_child = 5usize;
        let child_schemas = (0..root_child_count)
            .map(|index| {
                let child_group_id = 2 + (index * 2);
                let grandchild_group_id = child_group_id + 1;
                test_chapter_schema(
                    Some(child_group_id),
                    vec![test_chapter_schema(Some(grandchild_group_id), Vec::new())],
                )
            })
            .collect::<Vec<_>>();
        let schema = test_chapter_schema(None, child_schemas);

        let mut bytes = vec![ChapterRowMarker::Root.current_marker()];
        for index in 0..root_child_count {
            let child_group_id = 2 + (index * 2);
            let grandchild_group_id = child_group_id + 1;
            for _ in 0..rows_per_child {
                bytes.push(ChapterRowMarker::Child.current_marker());
                bytes.extend_from_slice(&child_group_id.to_le_bytes());
            }
            for _ in 0..(rows_per_child.saturating_mul(grand_rows_per_child)) {
                bytes.push(ChapterRowMarker::Child.current_marker());
                bytes.extend_from_slice(&grandchild_group_id.to_le_bytes());
            }
        }
        bytes.push(0x0f);

        let (raw, offset) = read_chapter_row_tree(&bytes, 0, &schema, &AdtgParseOptions::default())
            .expect("deterministic chapter row parser should not enumerate nested prefixes");

        assert_eq!(offset, bytes.len());
        assert_eq!(raw.rows.len(), 1);
        assert_eq!(raw.child_groups.len(), root_child_count as usize);
        for child_group in raw.child_groups {
            assert_eq!(child_group.rows.len(), rows_per_child);
            assert_eq!(child_group.child_groups.len(), 1);
            assert_eq!(
                child_group.child_groups[0].rows.len(),
                rows_per_child.saturating_mul(grand_rows_per_child)
            );
        }
    }

    #[test]
    fn primitive_readers_reject_overflowing_offsets() {
        let bytes = [0u8; 8];

        let u16_err = read_u16(&bytes, usize::MAX).expect_err("u16 offset should overflow");
        let u32_err = read_u32(&bytes, usize::MAX).expect_err("u32 offset should overflow");

        assert!(
            format!("{u16_err:#}").contains("ADTG u16 offset overflow"),
            "{u16_err:#}"
        );
        assert!(
            format!("{u32_err:#}").contains("ADTG u32 offset overflow"),
            "{u32_err:#}"
        );
    }

    #[test]
    fn descriptor_shape_rejects_overflowing_offset() {
        let bytes = [0u8; 32];

        let shape = descriptor_shape_at(&bytes, usize::MAX)
            .expect("overflowing descriptor offset should not error");

        assert!(
            shape.is_none(),
            "overflowing descriptor offset should not produce a shape"
        );
    }

    fn test_chapter_schema(
        row_group_id: Option<u32>,
        child_schemas: Vec<ChapterSchema>,
    ) -> ChapterSchema {
        ChapterSchema {
            row_group_id,
            descriptors: Vec::new(),
            fields: Vec::new(),
            chapter_fields: Vec::new(),
            child_schemas,
        }
    }
}
