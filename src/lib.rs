//! Native Rust parser and writer for persisted ADO `Recordset` data.
//!
//! The crate reads MDAC/ADO XML and ADTG streams into a shared [`Recordset`]
//! model, validates the shape, and can serialize that model back to ADO XML or
//! ADTG. Chaptered/shaped recordsets are represented as [`Value::Chapter`]
//! values containing nested [`Recordset`] instances.
//!
//! ```no_run
//! use tablegram::{parse_recordset_file, write_ado_xml, write_adtg};
//!
//! # fn main() -> anyhow::Result<()> {
//! let recordset = parse_recordset_file("input.adtg")?;
//! let xml_bytes = write_ado_xml(&recordset)?;
//! let adtg_bytes = write_adtg(&recordset)?;
//! # let _ = (xml_bytes, adtg_bytes);
//! # Ok(())
//! # }
//! ```

use std::io::Read;
use std::path::Path;

use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;

pub mod adtg;
pub mod adtg_write;
pub mod compat;
pub mod corpus_policy;
pub mod detect;
pub mod hexdiff;
pub mod model;
pub mod native_compare;
mod util;
pub mod xml;
pub mod xml_write;

pub use adtg::AdtgParseOptions;
pub use adtg_write::{write_adtg, write_adtg_with_options, AdtgWriteOptions};
pub use model::{
    ChapterRelation, ChapterRelationPair, FieldAttribute, RecordStatusFlag, Recordset,
    RowChangeKind, RowState, Value,
};
pub use xml_write::{write_ado_xml, write_ado_xml_string};

/// Maximum nested chapter depth accepted by parsers, validators, and writers.
pub const MAX_RECORDSET_DEPTH: usize = 64;

/// Resource limits enforced by parser entry points.
///
/// The default is unrestricted to keep the library compatibility-oriented.
/// Service-facing callers should set explicit limits with
/// [`RecordsetParseOptions::with_resource_limits`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceLimits {
    /// Maximum input byte length accepted by byte and file parser entry points.
    pub max_input_bytes: usize,
    /// Maximum visible fields allowed in any one `Recordset`.
    pub max_fields_per_recordset: usize,
    /// Maximum rows allowed in any one `Recordset`.
    pub max_rows_per_recordset: usize,
    /// Maximum bytes accepted for one decoded text, decimal, GUID, date/time, or
    /// binary-hex payload.
    pub max_value_bytes: usize,
    /// Maximum aggregate value payload bytes across the parsed `Recordset` tree.
    pub max_total_value_bytes: usize,
    /// Maximum length allowed for XML decimal text after exponent expansion.
    pub max_xml_decimal_expanded_len: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self::unrestricted()
    }
}

impl ResourceLimits {
    /// Disable parser resource limits.
    pub const fn unrestricted() -> Self {
        Self {
            max_input_bytes: usize::MAX,
            max_fields_per_recordset: usize::MAX,
            max_rows_per_recordset: usize::MAX,
            max_value_bytes: usize::MAX,
            max_total_value_bytes: usize::MAX,
            max_xml_decimal_expanded_len: usize::MAX,
        }
    }

    /// Override [`Self::max_input_bytes`].
    pub fn with_max_input_bytes(mut self, value: usize) -> Self {
        self.max_input_bytes = value;
        self
    }

    /// Override [`Self::max_fields_per_recordset`].
    pub fn with_max_fields_per_recordset(mut self, value: usize) -> Self {
        self.max_fields_per_recordset = value;
        self
    }

    /// Override [`Self::max_rows_per_recordset`].
    pub fn with_max_rows_per_recordset(mut self, value: usize) -> Self {
        self.max_rows_per_recordset = value;
        self
    }

    /// Override [`Self::max_value_bytes`].
    pub fn with_max_value_bytes(mut self, value: usize) -> Self {
        self.max_value_bytes = value;
        self
    }

    /// Override [`Self::max_total_value_bytes`].
    pub fn with_max_total_value_bytes(mut self, value: usize) -> Self {
        self.max_total_value_bytes = value;
        self
    }

    /// Override [`Self::max_xml_decimal_expanded_len`].
    pub fn with_max_xml_decimal_expanded_len(mut self, value: usize) -> Self {
        self.max_xml_decimal_expanded_len = value;
        self
    }

    pub(crate) fn check_input_bytes(self, len: usize, label: &str) -> Result<()> {
        if len > self.max_input_bytes {
            bail!(
                "{label} length {len} exceeded maximum input length {}",
                self.max_input_bytes
            );
        }
        Ok(())
    }

    pub(crate) fn check_fields(self, len: usize, label: &str) -> Result<()> {
        if len > self.max_fields_per_recordset {
            bail!(
                "{label} field count {len} exceeded maximum field count {}",
                self.max_fields_per_recordset
            );
        }
        Ok(())
    }

    pub(crate) fn check_rows(self, len: usize, label: &str) -> Result<()> {
        if len > self.max_rows_per_recordset {
            bail!(
                "{label} row count {len} exceeded maximum row count {}",
                self.max_rows_per_recordset
            );
        }
        Ok(())
    }

    pub(crate) fn check_value_bytes(self, len: usize, label: &str) -> Result<()> {
        if len > self.max_value_bytes {
            bail!(
                "{label} value length {len} exceeded maximum value length {}",
                self.max_value_bytes
            );
        }
        Ok(())
    }
}

/// Format-specific parser options used by [`parse_recordset_bytes_with_options`]
/// and [`parse_recordset_file_with_options`].
#[derive(Clone, Copy, Default)]
pub struct RecordsetParseOptions {
    /// ADTG parser options. XML parsing is unaffected by this field.
    pub adtg: AdtgParseOptions,
    /// Resource limits enforced while reading and validating the input.
    ///
    /// Defaults to [`ResourceLimits::unrestricted`].
    pub limits: ResourceLimits,
}

impl RecordsetParseOptions {
    /// Replace the ADTG parser options while preserving future option fields.
    pub fn with_adtg_options(mut self, options: AdtgParseOptions) -> Self {
        self.adtg = options;
        self
    }

    /// Replace the parser resource limits.
    pub fn with_resource_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = limits;
        self.adtg = self.adtg.with_resource_limits(limits);
        self
    }
}

/// Parse an ADO XML or ADTG byte stream, auto-detecting the persistence format.
///
/// The returned [`Recordset`] has already passed [`validate_recordset_shape`].
pub fn parse_recordset_bytes(bytes: &[u8]) -> Result<Recordset> {
    parse_recordset_bytes_with_options(bytes, RecordsetParseOptions::default())
}

/// Parse an ADO XML or ADTG byte stream with explicit parser options.
///
/// Options control ADTG ANSI text decoding and optional caller-supplied resource
/// limits. XML text decoding remains deterministic from the input document
/// encoding.
pub fn parse_recordset_bytes_with_options(
    bytes: &[u8],
    options: RecordsetParseOptions,
) -> Result<Recordset> {
    options
        .limits
        .check_input_bytes(bytes.len(), "ADO Recordset input")?;
    let recordset = match detect::detect_format(bytes) {
        detect::RecordsetFormat::Xml => xml::parse_ado_xml_bytes_with_limits(bytes, options.limits),
        detect::RecordsetFormat::Adtg => adtg::parse_adtg_bytes_with_options(
            bytes,
            options.adtg.with_resource_limits(options.limits),
        ),
    }?;
    validate_recordset_shape(&recordset).context("parsed ADO Recordset shape was inconsistent")?;
    validate_recordset_resource_limits(&recordset, options.limits)
        .context("parsed ADO Recordset exceeded resource limits")?;
    Ok(recordset)
}

/// Read and parse an ADO XML or ADTG file from disk.
pub fn parse_recordset_file(path: impl AsRef<Path>) -> Result<Recordset> {
    parse_recordset_file_with_options(path, RecordsetParseOptions::default())
}

/// Read and parse an ADO XML or ADTG file from disk with explicit options.
pub fn parse_recordset_file_with_options(
    path: impl AsRef<Path>,
    options: RecordsetParseOptions,
) -> Result<Recordset> {
    let path = path.as_ref();
    let bytes = read_file_limited(path, options.limits.max_input_bytes)?;
    parse_recordset_bytes_with_options(&bytes, options)
        .with_context(|| format!("failed to parse ADO Recordset {}", path.display()))
}

/// Validate internal consistency of a caller-built or parser-produced
/// [`Recordset`].
///
/// This checks row/value counts, row change back-references, chapter schemas,
/// value/type compatibility, depth limits, metadata ranges, and known MDAC
/// payload constraints. Writers call this before serialization.
pub fn validate_recordset_shape(recordset: &Recordset) -> Result<()> {
    validate_recordset_shape_at(recordset, "recordset", 0)
}

/// Validate resource use of an already-built [`Recordset`] tree.
pub fn validate_recordset_resource_limits(
    recordset: &Recordset,
    limits: ResourceLimits,
) -> Result<()> {
    let mut total_value_bytes = 0usize;
    validate_recordset_resource_limits_at(recordset, limits, "recordset", &mut total_value_bytes)
}

fn validate_recordset_resource_limits_at(
    recordset: &Recordset,
    limits: ResourceLimits,
    label: &str,
    total_value_bytes: &mut usize,
) -> Result<()> {
    limits.check_fields(recordset.fields.len(), label)?;
    limits.check_rows(recordset.rows.len(), label)?;
    for (row_index, row) in recordset.rows.iter().enumerate() {
        for (value_index, value) in row.values.iter().enumerate() {
            validate_value_resource_limits(
                value,
                limits,
                &format!("{label}.row{row_index}.field{value_index}"),
                total_value_bytes,
            )?;
        }
    }
    Ok(())
}

fn validate_value_resource_limits(
    value: &Value,
    limits: ResourceLimits,
    label: &str,
    total_value_bytes: &mut usize,
) -> Result<()> {
    match value {
        Value::String(text)
        | Value::Decimal(text)
        | Value::Date(text)
        | Value::Time(text)
        | Value::DateTime(text)
        | Value::Guid(text)
        | Value::BinaryHex(text) => {
            limits.check_value_bytes(text.len(), label)?;
            *total_value_bytes = total_value_bytes
                .checked_add(text.len())
                .ok_or_else(|| anyhow::anyhow!("{label} aggregate value length overflow"))?;
            if *total_value_bytes > limits.max_total_value_bytes {
                bail!(
                    "{label} aggregate value length {} exceeded maximum aggregate value length {}",
                    *total_value_bytes,
                    limits.max_total_value_bytes
                );
            }
        }
        Value::Chapter(recordset) => {
            validate_recordset_resource_limits_at(recordset, limits, label, total_value_bytes)?;
        }
        Value::Null
        | Value::Empty
        | Value::Unavailable
        | Value::Integer(_)
        | Value::UnsignedInteger(_)
        | Value::Float(_)
        | Value::Boolean(_) => {}
    }
    Ok(())
}

pub(crate) fn read_file_limited(path: &Path, max_bytes: usize) -> Result<Vec<u8>> {
    if max_bytes == usize::MAX {
        return std::fs::read(path).with_context(|| format!("failed to read {path:?}"));
    }
    if let Ok(metadata) = std::fs::metadata(path) {
        if metadata.len() > max_bytes as u64 {
            bail!(
                "file {} length {} exceeded maximum input length {max_bytes}",
                path.display(),
                metadata.len()
            );
        }
    }
    let file = std::fs::File::open(path).with_context(|| format!("failed to read {path:?}"))?;
    let mut bytes = Vec::new();
    let max_read = (max_bytes as u64).saturating_add(1);
    file.take(max_read)
        .read_to_end(&mut bytes)
        .with_context(|| format!("failed to read {path:?}"))?;
    if bytes.len() > max_bytes {
        bail!(
            "file {} length exceeded maximum input length {max_bytes}",
            path.display()
        );
    }
    Ok(bytes)
}

fn validate_recordset_shape_at(recordset: &Recordset, label: &str, depth: usize) -> Result<()> {
    validate_recordset_depth(label, depth)?;
    validate_fields(recordset, label)?;
    validate_chapter_relations(recordset, label)?;
    validate_chapter_field_schemas(recordset, label, depth)?;

    for (row_index, row) in recordset.rows.iter().enumerate() {
        if row.ordinal != row_index {
            bail!("{label}: row {row_index} ordinal was {}", row.ordinal);
        }
        if row.values.len() != recordset.fields.len() {
            bail!(
                "{label}: row {row_index} had {} values for {} fields",
                row.values.len(),
                recordset.fields.len()
            );
        }
        let Some(change_index) = row.change_index else {
            bail!("{label}: row {row_index} had no change index");
        };
        let Some(change) = recordset.changes.get(change_index) else {
            bail!("{label}: row {row_index} referenced invalid change {change_index}");
        };
        if !change.row_indices.contains(&row_index) {
            bail!("{label}: row {row_index} was missing from change {change_index}");
        }
        let expected_status = expected_status_for_row_state(row.state);
        if row.status_flags != [expected_status] {
            bail!(
                "{label}: row {row_index} status flags were {:?}, expected {:?}",
                row.status_flags,
                [expected_status]
            );
        }

        for (value_index, (field, value)) in
            recordset.fields.iter().zip(row.values.iter()).enumerate()
        {
            if matches!(value, Value::Unavailable) && row.state != RowState::Updated {
                bail!(
                    "{label}: row {row_index} field {value_index} had an unavailable value outside an updated row"
                );
            }
            let field_is_chapter = field.ado_type.map(|ty| ty.code) == Some(136);
            if !matches!(value, Value::Unavailable)
                && field_is_chapter != matches!(value, Value::Chapter(_))
            {
                bail!(
                    "{label}: row {row_index} field {value_index} chapter/value mismatch for field {}",
                    field.name
                );
            }
            if !value_matches_ado_type(value, field.ado_type.map(|ty| ty.code)) {
                bail!(
                    "{label}: row {row_index} field {value_index} value kind did not match ADO type {:?} for field {}",
                    field.ado_type,
                    field.name
                );
            }
            validate_value_payload(label, row_index, value_index, field, value)?;
            if let Value::Chapter(child) = value {
                if let Some(chapter_fields) = &field.chapter_fields {
                    if child.fields != *chapter_fields {
                        bail!(
                            "{label}: row {row_index} field {value_index} chapter schema did not match field {} descriptor",
                            field.name
                        );
                    }
                }
                validate_recordset_shape_at(
                    child,
                    &format!("{label}.row{row_index}.{value_index}"),
                    depth + 1,
                )?;
            }
        }
    }

    for (change_index, change) in recordset.changes.iter().enumerate() {
        if change.row_indices.is_empty() {
            bail!("{label}: change {change_index} had no rows");
        }
        let mut seen_row_indices = BTreeSet::new();
        let mut states = Vec::new();
        for row_index in &change.row_indices {
            if !seen_row_indices.insert(*row_index) {
                bail!("{label}: change {change_index} had duplicate row index {row_index}");
            }
            let Some(row) = recordset.rows.get(*row_index) else {
                bail!("{label}: change {change_index} referenced invalid row {row_index}");
            };
            if row.change_index != Some(change_index) {
                bail!(
                    "{label}: row {row_index} back-reference was {:?}, expected {change_index}",
                    row.change_index
                );
            }
            states.push(row.state);
        }
        validate_change_states(change.kind, &states, label, change_index)?;
    }

    Ok(())
}

fn validate_recordset_depth(label: &str, depth: usize) -> Result<()> {
    if depth > MAX_RECORDSET_DEPTH {
        bail!("{label}: exceeded maximum ADO Recordset chapter depth {MAX_RECORDSET_DEPTH}");
    }
    Ok(())
}

fn validate_chapter_field_schemas(recordset: &Recordset, label: &str, depth: usize) -> Result<()> {
    for (field_index, field) in recordset.fields.iter().enumerate() {
        let Some(chapter_fields) = &field.chapter_fields else {
            continue;
        };
        let schema_recordset = Recordset {
            fields: chapter_fields.clone(),
            rows: Vec::new(),
            changes: Vec::new(),
        };
        validate_recordset_shape_at(
            &schema_recordset,
            &format!("{label}.field{field_index}"),
            depth + 1,
        )?;
    }
    Ok(())
}

fn validate_chapter_relations(recordset: &Recordset, label: &str) -> Result<()> {
    for (field_index, field) in recordset.fields.iter().enumerate() {
        let Some(relation) = &field.chapter_relation else {
            continue;
        };
        let Some(chapter_fields) = &field.chapter_fields else {
            bail!("{label}: field {field_index} chapter relation had no child schema metadata");
        };
        if relation.pairs.is_empty() {
            bail!("{label}: field {field_index} chapter relation had no key pairs");
        }
        let mut parent_ordinals = BTreeSet::new();
        let mut child_ordinals = BTreeSet::new();
        for pair in &relation.pairs {
            let Some(parent_field) = pair
                .parent_ordinal
                .checked_sub(1)
                .and_then(|index| recordset.fields.get(index))
            else {
                bail!(
                    "{label}: field {field_index} chapter relation parent ordinal {} out of range",
                    pair.parent_ordinal
                );
            };
            let Some(child_field) = pair
                .child_ordinal
                .checked_sub(1)
                .and_then(|index| chapter_fields.get(index))
            else {
                bail!(
                    "{label}: field {field_index} chapter relation child ordinal {} out of range",
                    pair.child_ordinal
                );
            };
            if !parent_ordinals.insert(pair.parent_ordinal) {
                bail!(
                    "{label}: field {field_index} chapter relation repeated parent ordinal {}",
                    pair.parent_ordinal
                );
            }
            if !child_ordinals.insert(pair.child_ordinal) {
                bail!(
                    "{label}: field {field_index} chapter relation repeated child ordinal {}",
                    pair.child_ordinal
                );
            }
            if parent_field.ado_type.map(|ty| ty.code) == Some(136) {
                bail!(
                    "{label}: field {field_index} chapter relation parent ordinal {} points to chapter field {}",
                    pair.parent_ordinal,
                    parent_field.name
                );
            }
            if child_field.ado_type.map(|ty| ty.code) == Some(136) {
                bail!(
                    "{label}: field {field_index} chapter relation child ordinal {} points to chapter field {}",
                    pair.child_ordinal,
                    child_field.name
                );
            }
        }
    }
    Ok(())
}

fn value_matches_ado_type(value: &Value, ado_type_code: Option<u16>) -> bool {
    let Some(code) = ado_type_code else {
        return true;
    };

    match value {
        Value::Null | Value::Unavailable => true,
        Value::Empty => matches!(code, 0 | 12),
        Value::String(_) => matches!(code, 8 | 12 | 129 | 130 | 200 | 201 | 202 | 203),
        Value::Boolean(_) => matches!(code, 11 | 12),
        Value::Integer(_) => matches!(code, 2 | 3 | 12 | 16 | 20),
        Value::UnsignedInteger(_) => matches!(code, 12 | 17 | 18 | 19 | 21),
        Value::Float(_) => matches!(code, 4 | 5 | 12),
        Value::Decimal(_) => matches!(code, 6 | 12 | 14 | 131 | 139),
        Value::Date(_) => matches!(code, 133),
        Value::Time(_) => matches!(code, 134),
        Value::DateTime(_) => matches!(code, 7 | 12 | 64 | 135),
        Value::Guid(_) => matches!(code, 72),
        Value::BinaryHex(_) => matches!(code, 128 | 204 | 205),
        Value::Chapter(_) => matches!(code, 136),
    }
}

fn validate_value_payload(
    label: &str,
    row_index: usize,
    value_index: usize,
    field: &model::Field,
    value: &Value,
) -> Result<()> {
    let ado_type_code = field.ado_type.map(|ty| ty.code);
    match value {
        Value::Integer(value) => {
            validate_signed_integer_range(label, row_index, value_index, ado_type_code, *value)
        }
        Value::UnsignedInteger(value) => {
            validate_unsigned_integer_range(label, row_index, value_index, ado_type_code, *value)
        }
        Value::String(value) => {
            validate_string_payload(label, row_index, value_index, field, value)
        }
        Value::Float(value) if !value.is_finite() => {
            bail!("{label}: row {row_index} field {value_index} had non-finite float {value}");
        }
        Value::Float(value) => validate_float_payload(label, row_index, value_index, field, *value),
        Value::Decimal(value) if !is_decimal_text(value) => {
            bail!("{label}: row {row_index} field {value_index} had invalid decimal {value:?}");
        }
        Value::Decimal(value) if field.ado_type.map(|ty| ty.code) == Some(6) => {
            validate_currency_payload(label, row_index, value_index, value)
        }
        Value::Decimal(value) if field.ado_type.map(|ty| ty.code) == Some(139) => {
            validate_varnumeric_decimal_payload(label, row_index, value_index, field, value)
        }
        Value::Decimal(value) => {
            validate_numeric_decimal_payload(label, row_index, value_index, field, value)
        }
        Value::Date(value) if parse_model_date(value).is_none() => {
            bail!("{label}: row {row_index} field {value_index} had invalid date {value:?}");
        }
        Value::Time(value) if parse_model_time(value).is_none() => {
            bail!("{label}: row {row_index} field {value_index} had invalid time {value:?}");
        }
        Value::DateTime(value) if !is_datetime_text(value) => {
            bail!("{label}: row {row_index} field {value_index} had invalid datetime {value:?}");
        }
        Value::DateTime(value) if field.ado_type.map(|ty| ty.code) == Some(64) => {
            validate_filetime_payload(label, row_index, value_index, value)
        }
        Value::Guid(value) if !is_ado_guid_text(value) => {
            bail!("{label}: row {row_index} field {value_index} had invalid GUID {value:?}");
        }
        Value::Guid(value) if !is_canonical_ado_guid_text(value) => {
            bail!("{label}: row {row_index} field {value_index} had non-canonical GUID {value:?}");
        }
        Value::BinaryHex(value) if !is_even_hex_text(value) => {
            bail!("{label}: row {row_index} field {value_index} had invalid binary hex {value:?}");
        }
        Value::BinaryHex(value) => {
            validate_binary_hex_payload(label, row_index, value_index, field, value)
        }
        _ => Ok(()),
    }
}

fn validate_signed_integer_range(
    label: &str,
    row_index: usize,
    value_index: usize,
    ado_type_code: Option<u16>,
    value: i64,
) -> Result<()> {
    let Some((type_name, min, max)) = (match ado_type_code {
        Some(2) => Some(("adSmallInt", i16::MIN as i64, i16::MAX as i64)),
        Some(3) => Some(("adInteger", i32::MIN as i64, i32::MAX as i64)),
        Some(16) => Some(("adTinyInt", i8::MIN as i64, i8::MAX as i64)),
        _ => None,
    }) else {
        return Ok(());
    };
    if !(min..=max).contains(&value) {
        bail!(
            "{label}: row {row_index} field {value_index} integer {value} outside {type_name} range {min}..={max}"
        );
    }
    Ok(())
}

fn validate_unsigned_integer_range(
    label: &str,
    row_index: usize,
    value_index: usize,
    ado_type_code: Option<u16>,
    value: u64,
) -> Result<()> {
    let Some((type_name, max)) = (match ado_type_code {
        Some(17) => Some(("adUnsignedTinyInt", u8::MAX as u64)),
        Some(18) => Some(("adUnsignedSmallInt", u16::MAX as u64)),
        Some(19) => Some(("adUnsignedInt", u32::MAX as u64)),
        _ => None,
    }) else {
        return Ok(());
    };
    if value > max {
        bail!(
            "{label}: row {row_index} field {value_index} unsigned integer {value} outside {type_name} range 0..={max}"
        );
    }
    Ok(())
}

fn validate_float_payload(
    label: &str,
    row_index: usize,
    value_index: usize,
    field: &model::Field,
    value: f64,
) -> Result<()> {
    match field.ado_type.map(|ty| ty.code) {
        Some(4) => validate_f32_normalized_float(label, row_index, value_index, value, "adSingle"),
        Some(5) if field.max_length == Some(4) => {
            validate_f32_normalized_float(label, row_index, value_index, value, "4-byte adDouble")
        }
        _ => Ok(()),
    }
}

fn validate_f32_normalized_float(
    label: &str,
    row_index: usize,
    value_index: usize,
    value: f64,
    type_name: &str,
) -> Result<()> {
    if value.abs() > f32::MAX as f64 {
        bail!(
            "{label}: row {row_index} field {value_index} float {value} outside {type_name} finite range"
        );
    }
    if ((value as f32) as f64).to_bits() != value.to_bits() {
        bail!(
            "{label}: row {row_index} field {value_index} float {value} was not normalized as {type_name}"
        );
    }
    Ok(())
}

fn validate_string_payload(
    label: &str,
    row_index: usize,
    value_index: usize,
    field: &model::Field,
    value: &str,
) -> Result<()> {
    if field.ado_type.map(|ty| ty.code) == Some(12) {
        return Ok(());
    }
    if field.long {
        return Ok(());
    }
    let Some(max_length) = field.max_length else {
        return Ok(());
    };
    let char_len = value.chars().count();
    if char_len > max_length {
        bail!(
            "{label}: row {row_index} field {value_index} string length {char_len} exceeded max length {max_length}"
        );
    }
    Ok(())
}

fn validate_numeric_decimal_payload(
    label: &str,
    row_index: usize,
    value_index: usize,
    field: &model::Field,
    value: &str,
) -> Result<()> {
    let Some(code @ (14 | 131)) = field.ado_type.map(|ty| ty.code) else {
        return Ok(());
    };
    if code == 14 && field.precision == Some(255) && field.scale == Some(255) {
        return Ok(());
    }

    let Some(precision) = field.precision else {
        return Ok(());
    };
    let Some((digits, scale)) = decimal_precision_scale(value) else {
        bail!(
            "{label}: row {row_index} field {value_index} decimal {value:?} was not a fixed decimal payload"
        );
    };
    if digits > precision {
        bail!(
            "{label}: row {row_index} field {value_index} decimal {value:?} exceeded precision {precision}"
        );
    }
    if let Some(expected_scale) = field.scale {
        if scale as i32 > expected_scale {
            bail!(
                "{label}: row {row_index} field {value_index} decimal {value:?} scale {scale} exceeded declared scale {expected_scale}"
            );
        }
    }

    Ok(())
}

fn validate_varnumeric_decimal_payload(
    label: &str,
    row_index: usize,
    value_index: usize,
    field: &model::Field,
    value: &str,
) -> Result<()> {
    let Some(max_length) = field.max_length else {
        return Ok(());
    };
    let Some(magnitude) = varnumeric_magnitude(value) else {
        bail!(
            "{label}: row {row_index} field {value_index} adVarNumeric decimal {value:?} was not a fixed decimal payload"
        );
    };

    let payload_len = 3 + varnumeric_magnitude_len(magnitude);
    if payload_len <= max_length || (max_length >= 3 && payload_len == max_length + 1) {
        return Ok(());
    }

    bail!(
        "{label}: row {row_index} field {value_index} adVarNumeric payload length {payload_len} exceeded max length {max_length}"
    );
}

fn varnumeric_magnitude(value: &str) -> Option<u128> {
    if value.bytes().any(|byte| matches!(byte, b'e' | b'E')) {
        return None;
    }

    let unsigned = value.strip_prefix('-').unwrap_or(value);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }

    let digits = if fraction.is_empty() {
        let mut digits = whole.to_string();
        while digits.len() > 1 && digits.ends_with('0') {
            digits.pop();
        }
        digits
    } else {
        let fraction = fraction.trim_end_matches('0');
        format!("{whole}{fraction}")
    };
    let digits = digits.trim_start_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    digits.parse::<u128>().ok()
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

fn decimal_precision_scale(value: &str) -> Option<(usize, usize)> {
    let bytes = value.as_bytes();
    if bytes.iter().any(|byte| matches!(byte, b'e' | b'E')) {
        return None;
    }

    let unsigned = value.strip_prefix('-').unwrap_or(value);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let mut digits = String::with_capacity(whole.len() + fraction.len());
    digits.push_str(whole);
    digits.push_str(fraction);
    let digits = digits.trim_start_matches('0').len().max(1);
    Some((digits, fraction.len()))
}

fn validate_currency_payload(
    label: &str,
    row_index: usize,
    value_index: usize,
    value: &str,
) -> Result<()> {
    let Some((digits, scale)) = decimal_precision_scale(value) else {
        bail!(
            "{label}: row {row_index} field {value_index} currency {value:?} was not a fixed decimal payload"
        );
    };
    if scale > 4 {
        bail!(
            "{label}: row {row_index} field {value_index} currency {value:?} scale {scale} exceeded adCurrency scale 4"
        );
    }
    if digits > 19
        || currency_scaled_abs(value, scale).is_none_or(|scaled| {
            let max = if value.starts_with('-') {
                (i64::MAX as u128) + 1
            } else {
                i64::MAX as u128
            };
            scaled > max
        })
    {
        bail!(
            "{label}: row {row_index} field {value_index} currency {value:?} outside adCurrency range"
        );
    }

    Ok(())
}

fn currency_scaled_abs(value: &str, scale: usize) -> Option<u128> {
    let unsigned = value.strip_prefix('-').unwrap_or(value);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    let whole = whole.trim_start_matches('0');
    let whole = if whole.is_empty() { "0" } else { whole };
    let mut scaled = whole.parse::<u128>().ok()?.checked_mul(10_000)?;

    let mut fraction_value = 0u128;
    for byte in fraction.bytes() {
        fraction_value = fraction_value * 10 + u128::from(byte - b'0');
    }
    for _ in scale..4 {
        fraction_value *= 10;
    }
    scaled = scaled.checked_add(fraction_value)?;
    Some(scaled)
}

fn validate_binary_hex_payload(
    label: &str,
    row_index: usize,
    value_index: usize,
    field: &model::Field,
    value: &str,
) -> Result<()> {
    if !is_upper_hex_text(value) {
        bail!(
            "{label}: row {row_index} field {value_index} had non-canonical binary hex {value:?}"
        );
    }
    if field.long {
        return Ok(());
    }
    let Some(max_length) = field.max_length else {
        return Ok(());
    };
    let byte_len = value.len() / 2;
    if byte_len > max_length {
        bail!(
            "{label}: row {row_index} field {value_index} binary payload length {byte_len} exceeded max length {max_length}"
        );
    }
    Ok(())
}

fn validate_filetime_payload(
    label: &str,
    row_index: usize,
    value_index: usize,
    value: &str,
) -> Result<()> {
    let Some((date, time)) = value.split_once('T') else {
        return Ok(());
    };
    let Some((year, _, _)) = parse_model_date(date) else {
        return Ok(());
    };
    if year < 1601 {
        bail!(
            "{label}: row {row_index} field {value_index} adFileTime year {year} is out of range"
        );
    }
    if time.contains('.') {
        bail!(
            "{label}: row {row_index} field {value_index} adFileTime had fractional seconds {value:?}"
        );
    }

    Ok(())
}

fn is_decimal_text(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut cursor = usize::from(bytes.first() == Some(&b'-'));
    if cursor == bytes.len() {
        return false;
    }

    let whole_start = cursor;
    while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
        cursor += 1;
    }
    if cursor == whole_start {
        return false;
    }

    if bytes.get(cursor) == Some(&b'.') {
        cursor += 1;
        let fraction_start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        if cursor == fraction_start {
            return false;
        }
    }

    if matches!(bytes.get(cursor), Some(b'e' | b'E')) {
        cursor += 1;
        if matches!(bytes.get(cursor), Some(b'+' | b'-')) {
            cursor += 1;
        }
        let exponent_start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        if cursor == exponent_start {
            return false;
        }
    }

    cursor == bytes.len()
}

fn is_datetime_text(value: &str) -> bool {
    let Some((date, time)) = value.split_once('T') else {
        return false;
    };
    parse_model_date(date).is_some() && parse_model_time_with_optional_fraction(time)
}

fn parse_model_date(value: &str) -> Option<(u16, u16, u16)> {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year = parse_fixed_digits(bytes, 0, 4)?;
    let month = parse_fixed_digits(bytes, 5, 2)?;
    let day = parse_fixed_digits(bytes, 8, 2)?;
    validate_model_date(year, month, day).then_some((year, month, day))
}

fn parse_model_time(value: &str) -> Option<(u16, u16, u16)> {
    let bytes = value.as_bytes();
    if bytes.len() != 8 || bytes[2] != b':' || bytes[5] != b':' {
        return None;
    }
    parse_model_time_from_bytes(bytes)
}

fn parse_model_time_with_optional_fraction(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() < 8 || bytes[2] != b':' || bytes[5] != b':' {
        return false;
    }
    if parse_model_time_from_bytes(bytes).is_none() {
        return false;
    }
    if bytes.len() == 8 {
        return true;
    }
    bytes[8] == b'.'
        && !bytes[9..].is_empty()
        && bytes[9..].len() <= 9
        && bytes[9..].iter().all(u8::is_ascii_digit)
}

fn parse_model_time_from_bytes(bytes: &[u8]) -> Option<(u16, u16, u16)> {
    let hour = parse_fixed_digits(bytes, 0, 2)?;
    let minute = parse_fixed_digits(bytes, 3, 2)?;
    let second = parse_fixed_digits(bytes, 6, 2)?;
    (hour <= 23 && minute <= 59 && second <= 59).then_some((hour, minute, second))
}

fn parse_fixed_digits(bytes: &[u8], offset: usize, len: usize) -> Option<u16> {
    let slice = bytes.get(offset..offset + len)?;
    if !slice.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(
        slice
            .iter()
            .fold(0u16, |acc, byte| acc * 10 + u16::from(byte - b'0')),
    )
}

fn validate_model_date(year: u16, month: u16, day: u16) -> bool {
    util::is_valid_gregorian_date(year, month, day)
}

fn is_ado_guid_text(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 38
        && bytes[0] == b'{'
        && bytes[37] == b'}'
        && [9, 14, 19, 24]
            .into_iter()
            .all(|index| bytes[index] == b'-')
        && bytes
            .iter()
            .enumerate()
            .filter(|(index, _)| !matches!(index, 0 | 9 | 14 | 19 | 24 | 37))
            .all(|(_, byte)| byte.is_ascii_hexdigit())
}

fn is_canonical_ado_guid_text(value: &str) -> bool {
    is_ado_guid_text(value)
        && value
            .bytes()
            .enumerate()
            .filter(|(index, _)| !matches!(index, 0 | 9 | 14 | 19 | 24 | 37))
            .all(|(_, byte)| byte.is_ascii_digit() || matches!(byte, b'A'..=b'F'))
}

fn is_even_hex_text(value: &str) -> bool {
    value.len().is_multiple_of(2) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_upper_hex_text(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || matches!(byte, b'A'..=b'F'))
}

fn validate_fields(recordset: &Recordset, label: &str) -> Result<()> {
    let mut ordinals = BTreeSet::new();
    for (field_index, field) in recordset.fields.iter().enumerate() {
        if field.name.is_empty() {
            bail!("{label}: field {field_index} had an empty name");
        }
        if field.xml_name.is_empty() {
            bail!("{label}: field {field_index} had an empty XML name");
        }
        if matches!(field.data_type.as_deref(), Some("")) {
            bail!("{label}: field {field_index} had an empty data type");
        }
        if matches!(field.db_type.as_deref(), Some("")) {
            bail!("{label}: field {field_index} had an empty DB type");
        }
        if matches!(field.base_catalog.as_deref(), Some("")) {
            bail!("{label}: field {field_index} had an empty base catalog");
        }
        if matches!(field.base_schema.as_deref(), Some("")) {
            bail!("{label}: field {field_index} had an empty base schema");
        }
        if matches!(field.base_table.as_deref(), Some("")) {
            bail!("{label}: field {field_index} had an empty base table");
        }
        if matches!(field.base_column.as_deref(), Some("")) {
            bail!("{label}: field {field_index} had an empty base column");
        }
        if let Some(ordinal) = field.ordinal {
            if ordinal == 0 {
                bail!("{label}: field {field_index} had zero ordinal");
            }
            if ordinal > recordset.fields.len() {
                bail!(
                    "{label}: field {field_index} ordinal {ordinal} exceeded field count {}",
                    recordset.fields.len()
                );
            }
            if !ordinals.insert(ordinal) {
                bail!("{label}: duplicate field ordinal {ordinal}");
            }
        }
        if let Some(ado_type) = field.ado_type {
            let Some(expected_name) = ado_type_name_for_code(ado_type.code) else {
                bail!(
                    "{label}: field {field_index} had unknown ADO type code {}",
                    ado_type.code
                );
            };
            if ado_type.name != expected_name {
                bail!(
                    "{label}: field {field_index} ADO type code {} was named {}, expected {expected_name}",
                    ado_type.code,
                    ado_type.name
                );
            }
            if ado_type_requires_fixed_length(ado_type.code) && !field.fixed_length {
                bail!(
                    "{label}: field {field_index} ADO type {} should be fixed-length",
                    ado_type.name
                );
            }
            if field.fixed_length && !ado_type_allows_fixed_length(ado_type.code) {
                bail!(
                    "{label}: field {field_index} non-fixed ADO type {} had fixed-length metadata",
                    ado_type.name
                );
            }
            if ado_type_requires_long(ado_type.code) && !field.long {
                bail!(
                    "{label}: field {field_index} ADO type {} should be long",
                    ado_type.name
                );
            }
            if field.long && !ado_type_allows_long(ado_type.code) {
                bail!(
                    "{label}: field {field_index} non-long ADO type {} had long metadata",
                    ado_type.name
                );
            }
            validate_field_metadata(label, field_index, field, ado_type.code)?;
        }
        let mut attributes = Vec::new();
        for attribute in &field.attributes {
            if attributes.contains(attribute) {
                bail!(
                    "{label}: field {field_index} had duplicate attribute {:?}",
                    attribute
                );
            }
            attributes.push(*attribute);
        }
        let is_nullable_attribute = field.attributes.contains(&FieldAttribute::IsNullable)
            || field.attributes.contains(&FieldAttribute::MayBeNull);
        if field.nullable != is_nullable_attribute {
            bail!("{label}: field {field_index} nullable flag disagreed with attributes");
        }
        if field.writable != field.attributes.contains(&FieldAttribute::Updatable) {
            bail!("{label}: field {field_index} writable flag disagreed with attributes");
        }
        if field.fixed_length != field.attributes.contains(&FieldAttribute::Fixed) {
            bail!("{label}: field {field_index} fixed-length flag disagreed with attributes");
        }
        if field.long != field.attributes.contains(&FieldAttribute::Long) {
            bail!("{label}: field {field_index} long flag disagreed with attributes");
        }
        let field_is_chapter = field.ado_type.map(|ty| ty.code) == Some(136);
        if field_is_chapter != field.attributes.contains(&FieldAttribute::IsChapter) {
            bail!("{label}: field {field_index} chapter flag disagreed with attributes");
        }
        if !field_is_chapter && field.chapter_fields.is_some() {
            bail!("{label}: field {field_index} non-chapter field had chapter schema metadata");
        }
        if !field_is_chapter && field.chapter_relation.is_some() {
            bail!("{label}: field {field_index} non-chapter field had chapter relation metadata");
        }
        if let Some(chapter_fields) = &field.chapter_fields {
            if chapter_fields.is_empty() {
                bail!("{label}: field {field_index} chapter schema had no fields");
            }
        }
    }
    if !ordinals.is_empty() && ordinals.len() != recordset.fields.len() {
        bail!(
            "{label}: field ordinals were incomplete: {} of {} fields had ordinals",
            ordinals.len(),
            recordset.fields.len()
        );
    }
    Ok(())
}

fn validate_field_metadata(
    label: &str,
    field_index: usize,
    field: &model::Field,
    ado_type_code: u16,
) -> Result<()> {
    if let Some(max_length) = field.max_length {
        if max_length == 0 {
            bail!("{label}: field {field_index} had zero max length");
        }
        if let Some((type_name, minimum_width)) = ado_type_minimum_width(ado_type_code) {
            if max_length < minimum_width {
                bail!(
                    "{label}: field {field_index} max length {max_length} was below {type_name} minimum width {minimum_width}"
                );
            }
        }
    }
    if let Some(scale) = field.scale {
        if scale < 0 {
            bail!("{label}: field {field_index} had negative scale {scale}");
        }
    }

    match ado_type_code {
        7 => validate_temporal_field_width(label, field_index, field, "adDate", &[8, 16]),
        64 => validate_temporal_field_width(label, field_index, field, "adFileTime", &[8, 16]),
        5 => validate_double_field_metadata(label, field_index, field),
        6 => validate_exact_field_width(label, field_index, field, "adCurrency", 8),
        11 => validate_exact_field_width(label, field_index, field, "adBoolean", 2),
        12 => validate_variant_field_metadata(label, field_index, field),
        14 => {
            validate_exact_field_width(label, field_index, field, "adDecimal", 16)?;
            validate_decimal_field_metadata(label, field_index, field)
        }
        72 => validate_exact_field_width(label, field_index, field, "adGUID", 16),
        133 => validate_temporal_field_width(label, field_index, field, "adDBDate", &[6]),
        134 => validate_temporal_field_width(label, field_index, field, "adDBTime", &[6]),
        131 => {
            validate_exact_field_width(label, field_index, field, "adNumeric", 19)?;
            validate_numeric_field_metadata(label, field_index, field)
        }
        135 => validate_timestamp_field_metadata(label, field_index, field),
        136 => validate_chapter_field_metadata(label, field_index, field),
        139 => validate_varnumeric_field_metadata(label, field_index, field),
        _ => Ok(()),
    }
}

fn validate_double_field_metadata(
    label: &str,
    field_index: usize,
    field: &model::Field,
) -> Result<()> {
    if let Some(max_length) = field.max_length {
        if !matches!(max_length, 4 | 8) {
            bail!(
                "{label}: field {field_index} adDouble max length {max_length} was not MDAC width 4 or 8"
            );
        }
    }

    Ok(())
}

fn validate_exact_field_width(
    label: &str,
    field_index: usize,
    field: &model::Field,
    type_name: &str,
    expected_width: usize,
) -> Result<()> {
    if let Some(max_length) = field.max_length {
        if max_length != expected_width {
            bail!(
                "{label}: field {field_index} {type_name} max length {max_length} was not MDAC width {expected_width}"
            );
        }
    }

    Ok(())
}

fn validate_temporal_field_width(
    label: &str,
    field_index: usize,
    field: &model::Field,
    type_name: &str,
    expected_widths: &[usize],
) -> Result<()> {
    if let Some(max_length) = field.max_length {
        if !expected_widths.contains(&max_length) {
            bail!(
                "{label}: field {field_index} {type_name} max length {max_length} was not MDAC width {}",
                describe_widths(expected_widths)
            );
        }
    }

    Ok(())
}

fn describe_widths(widths: &[usize]) -> String {
    match widths {
        [width] => width.to_string(),
        [left, right] => format!("{left} or {right}"),
        _ => widths
            .iter()
            .map(|width| width.to_string())
            .collect::<Vec<_>>()
            .join(", "),
    }
}

fn validate_variant_field_metadata(
    label: &str,
    field_index: usize,
    field: &model::Field,
) -> Result<()> {
    if let Some(max_length @ 1..=10) = field.max_length {
        bail!(
            "{label}: field {field_index} adVariant max length {max_length} below MDAC minimum 11"
        );
    }

    Ok(())
}

fn validate_decimal_field_metadata(
    label: &str,
    field_index: usize,
    field: &model::Field,
) -> Result<()> {
    if field.precision == Some(255) && field.scale == Some(255) {
        return Ok(());
    }

    let Some(precision) = field.precision else {
        return Ok(());
    };
    if !(1..=28).contains(&precision) {
        bail!("{label}: field {field_index} adDecimal precision {precision} outside 1..=28");
    }

    let Some(scale) = field.scale else {
        return Ok(());
    };
    if scale > 28 {
        bail!("{label}: field {field_index} adDecimal scale {scale} outside 0..=28");
    }
    if scale as usize > precision {
        bail!("{label}: field {field_index} adDecimal scale {scale} exceeds precision {precision}");
    }

    Ok(())
}

fn validate_numeric_field_metadata(
    label: &str,
    field_index: usize,
    field: &model::Field,
) -> Result<()> {
    let Some(precision) = field.precision else {
        bail!("{label}: field {field_index} adNumeric missing precision");
    };
    if !(1..=38).contains(&precision) {
        bail!("{label}: field {field_index} adNumeric precision {precision} outside 1..=38");
    }

    let Some(scale) = field.scale else {
        bail!("{label}: field {field_index} adNumeric missing scale");
    };
    if scale > 38 {
        bail!("{label}: field {field_index} adNumeric scale {scale} outside 0..=38");
    }
    if scale as usize > precision {
        bail!("{label}: field {field_index} adNumeric scale {scale} exceeds precision {precision}");
    }

    Ok(())
}

fn validate_timestamp_field_metadata(
    label: &str,
    field_index: usize,
    field: &model::Field,
) -> Result<()> {
    validate_temporal_field_width(label, field_index, field, "adDBTimeStamp", &[16])?;
    if let Some(scale) = field.scale {
        if scale > 9 {
            bail!("{label}: field {field_index} adDBTimeStamp scale {scale} outside 0..=9");
        }
    }

    Ok(())
}

fn validate_varnumeric_field_metadata(
    label: &str,
    field_index: usize,
    field: &model::Field,
) -> Result<()> {
    let Some(max_length) = field.max_length else {
        bail!("{label}: field {field_index} adVarNumeric missing max length");
    };
    if max_length < 3 {
        bail!(
            "{label}: field {field_index} adVarNumeric max length {max_length} below MDAC XML minimum 3"
        );
    }

    Ok(())
}

fn validate_chapter_field_metadata(
    label: &str,
    field_index: usize,
    field: &model::Field,
) -> Result<()> {
    if field.max_length != Some(4) {
        bail!(
            "{label}: field {field_index} adChapter max length was {:?}, expected 4",
            field.max_length
        );
    }
    if !matches!(field.precision, None | Some(0) | Some(255)) {
        bail!(
            "{label}: field {field_index} adChapter precision was {:?}, expected 0, 255, or absent",
            field.precision
        );
    }
    if !matches!(field.scale, None | Some(0) | Some(255)) {
        bail!(
            "{label}: field {field_index} adChapter scale was {:?}, expected 0, 255, or absent",
            field.scale
        );
    }

    Ok(())
}

fn ado_type_name_for_code(code: u16) -> Option<&'static str> {
    Some(match code {
        0 => "adEmpty",
        2 => "adSmallInt",
        3 => "adInteger",
        4 => "adSingle",
        5 => "adDouble",
        6 => "adCurrency",
        7 => "adDate",
        8 => "adBSTR",
        9 => "adIDispatch",
        10 => "adError",
        11 => "adBoolean",
        12 => "adVariant",
        13 => "adIUnknown",
        14 => "adDecimal",
        16 => "adTinyInt",
        17 => "adUnsignedTinyInt",
        18 => "adUnsignedSmallInt",
        19 => "adUnsignedInt",
        20 => "adBigInt",
        21 => "adUnsignedBigInt",
        64 => "adFileTime",
        72 => "adGUID",
        128 => "adBinary",
        129 => "adChar",
        130 => "adWChar",
        131 => "adNumeric",
        132 => "adUserDefined",
        133 => "adDBDate",
        134 => "adDBTime",
        135 => "adDBTimeStamp",
        136 => "adChapter",
        138 => "adPropVariant",
        139 => "adVarNumeric",
        200 => "adVarChar",
        201 => "adLongVarChar",
        202 => "adVarWChar",
        203 => "adLongVarWChar",
        204 => "adVarBinary",
        205 => "adLongVarBinary",
        _ => return None,
    })
}

fn ado_type_requires_fixed_length(code: u16) -> bool {
    matches!(
        code,
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
            | 136
    )
}

fn ado_type_allows_fixed_length(code: u16) -> bool {
    ado_type_requires_fixed_length(code)
}

fn ado_type_requires_long(code: u16) -> bool {
    matches!(code, 201 | 203 | 205)
}

fn ado_type_allows_long(code: u16) -> bool {
    matches!(code, 12 | 201 | 203 | 205)
}

fn ado_type_minimum_width(code: u16) -> Option<(&'static str, usize)> {
    Some(match code {
        2 => ("adSmallInt", 2),
        3 => ("adInteger", 4),
        4 => ("adSingle", 4),
        5 => ("adDouble", 4),
        6 => ("adCurrency", 8),
        7 => ("adDate", 8),
        11 => ("adBoolean", 2),
        14 => ("adDecimal", 16),
        16 => ("adTinyInt", 1),
        17 => ("adUnsignedTinyInt", 1),
        18 => ("adUnsignedSmallInt", 2),
        19 => ("adUnsignedInt", 4),
        20 => ("adBigInt", 4),
        21 => ("adUnsignedBigInt", 4),
        64 => ("adFileTime", 8),
        72 => ("adGUID", 16),
        131 => ("adNumeric", 19),
        133 => ("adDBDate", 6),
        134 => ("adDBTime", 6),
        135 => ("adDBTimeStamp", 16),
        136 => ("adChapter", 4),
        _ => return None,
    })
}

fn validate_change_states(
    kind: RowChangeKind,
    states: &[RowState],
    label: &str,
    change_index: usize,
) -> Result<()> {
    match kind {
        RowChangeKind::Current if states.iter().all(|state| *state == RowState::Current) => Ok(()),
        RowChangeKind::Insert if states.iter().all(|state| *state == RowState::Inserted) => Ok(()),
        RowChangeKind::Delete if states.iter().all(|state| *state == RowState::Deleted) => Ok(()),
        RowChangeKind::Update if states == [RowState::Original, RowState::Updated] => Ok(()),
        _ => bail!("{label}: change {change_index} {kind:?} had row states {states:?}"),
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
