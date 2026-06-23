//! Native comparison helpers for XML, ADTG, and MDAC-resaved recordsets.
//!
//! Comparisons operate on materialized ADO views instead of raw rows so pending
//! insert/update/delete groups and shaped chapters are checked the same way ADO
//! exposes them. MDAC resave mode applies only documented provider
//! normalizations.

use crate::compat::{
    try_materialize_default_view, try_materialize_pending_view, MaterializedField,
    MaterializedRecordset, MaterializedRow,
};
use crate::model::{Recordset, Value};
use encoding_rs::EUC_KR;

pub fn compare_native_recordsets(xml: &Recordset, adtg: &Recordset) -> Vec<String> {
    compare_recordsets_with_options(xml, adtg, NativeCompareOptions::strict())
}

pub fn compare_mdac_resaved_recordsets(left: &Recordset, right: &Recordset) -> Vec<String> {
    compare_recordsets_with_options(left, right, NativeCompareOptions::mdac_resave())
}

#[derive(Debug, Clone, Copy)]
struct NativeCompareOptions {
    mdac_resave_normalization: bool,
}

impl NativeCompareOptions {
    fn strict() -> Self {
        Self {
            mdac_resave_normalization: false,
        }
    }

    fn mdac_resave() -> Self {
        Self {
            mdac_resave_normalization: true,
        }
    }
}

fn compare_recordsets_with_options(
    xml: &Recordset,
    adtg: &Recordset,
    options: NativeCompareOptions,
) -> Vec<String> {
    compare_recordsets_with_options_with_depth(xml, adtg, options, 0)
}

fn compare_recordsets_with_options_with_depth(
    xml: &Recordset,
    adtg: &Recordset,
    options: NativeCompareOptions,
    depth: usize,
) -> Vec<String> {
    if depth > crate::MAX_RECORDSET_DEPTH {
        return vec![format!(
            "recordset comparison exceeded maximum ADO Recordset chapter depth {}",
            crate::MAX_RECORDSET_DEPTH
        )];
    }

    let mut mismatches = Vec::new();
    if let Err(err) = crate::validate_recordset_shape(xml) {
        mismatches.push(format!("left recordset shape invalid: {err:#}"));
        return mismatches;
    }
    if let Err(err) = crate::validate_recordset_shape(adtg) {
        mismatches.push(format!("right recordset shape invalid: {err:#}"));
        return mismatches;
    }

    let xml_default = match try_materialize_default_view(xml) {
        Ok(view) => view,
        Err(err) => {
            mismatches.push(format!("left default view materialization failed: {err:#}"));
            return mismatches;
        }
    };
    let adtg_default = match try_materialize_default_view(adtg) {
        Ok(view) => view,
        Err(err) => {
            mismatches.push(format!(
                "right default view materialization failed: {err:#}"
            ));
            return mismatches;
        }
    };
    if !native_fields_match(&xml_default, &adtg_default, options) {
        mismatches.push(format!(
            "default field mismatch: xml={:?} adtg={:?}",
            xml_default.fields, adtg_default.fields
        ));
    }
    if !rows_match_ordered(&xml_default, &adtg_default, options, depth) {
        mismatches.push(format!(
            "default row mismatch: xml={:?} adtg={:?}",
            xml_default.rows, adtg_default.rows
        ));
    }

    let xml_pending = match try_materialize_pending_view(xml) {
        Ok(view) => view,
        Err(err) => {
            mismatches.push(format!("left pending view materialization failed: {err:#}"));
            return mismatches;
        }
    };
    let adtg_pending = match try_materialize_pending_view(adtg) {
        Ok(view) => view,
        Err(err) => {
            mismatches.push(format!(
                "right pending view materialization failed: {err:#}"
            ));
            return mismatches;
        }
    };
    if !native_fields_match(&xml_pending, &adtg_pending, options) {
        mismatches.push(format!(
            "pending field mismatch: xml={:?} adtg={:?}",
            xml_pending.fields, adtg_pending.fields
        ));
    }
    if !rows_match_unordered(&xml_pending, &adtg_pending, options, depth) {
        mismatches.push(format!(
            "pending row mismatch: xml={:?} adtg={:?}",
            xml_pending.rows, adtg_pending.rows
        ));
    }

    mismatches
}

#[derive(Debug, PartialEq, Eq)]
struct NativeFieldIdentity<'a> {
    name: &'a str,
    ado_type_code: Option<u16>,
    max_length: Option<usize>,
    precision: Option<usize>,
    scale: Option<i32>,
    attribute_flags: u32,
    base_catalog: Option<&'a str>,
    base_schema: Option<&'a str>,
    base_table: Option<&'a str>,
    base_column: Option<&'a str>,
}

const FORMAT_SPECIFIC_NATIVE_FIELD_FLAGS: u32 = NEGATIVE_SCALE_FLAG;
const VARIANT_TEXT_STORAGE_FLAGS: u32 = 0x10 | 0x80;
const UPDATABILITY_FLAGS: u32 = 0x04 | 0x08;
const FIXED_LENGTH_FLAG: u32 = 0x10;
const ROW_VERSION_FLAG: u32 = 0x200;
const NEGATIVE_SCALE_FLAG: u32 = 0x4000;
const KEY_COLUMN_FLAG: u32 = 0x8000;

fn native_fields_match(
    left: &MaterializedRecordset,
    right: &MaterializedRecordset,
    options: NativeCompareOptions,
) -> bool {
    left.fields.len() == right.fields.len()
        && left
            .fields
            .iter()
            .zip(right.fields.iter())
            .all(|(left, right)| native_field_matches(left, right, options))
}

fn native_field_matches(
    left: &MaterializedField,
    right: &MaterializedField,
    options: NativeCompareOptions,
) -> bool {
    native_field_identity(left, options) == native_field_identity(right, options)
        || variant_text_field_pair(left, right)
}

fn native_field_identity(
    field: &MaterializedField,
    options: NativeCompareOptions,
) -> NativeFieldIdentity<'_> {
    NativeFieldIdentity {
        name: field.name.as_str(),
        ado_type_code: field.ado_type_code,
        max_length: comparable_native_max_length(field, options),
        precision: comparable_native_precision(field),
        scale: comparable_native_scale(field),
        attribute_flags: comparable_native_attribute_flags(field, options),
        base_catalog: comparable_provider_name(&field.base_catalog, &field.base_column, options),
        base_schema: comparable_provider_name(&field.base_schema, &field.base_column, options),
        base_table: comparable_provider_name(&field.base_table, &field.base_column, options),
        base_column: field.base_column.as_deref(),
    }
}

fn comparable_provider_name<'a>(
    value: &'a Option<String>,
    base_column: &Option<String>,
    options: NativeCompareOptions,
) -> Option<&'a str> {
    if options.mdac_resave_normalization && base_column.is_some() {
        return None;
    }
    value.as_deref()
}

fn comparable_native_max_length(
    field: &MaterializedField,
    options: NativeCompareOptions,
) -> Option<usize> {
    if options.mdac_resave_normalization && mdac_resave_fixed_size_field(field) {
        return None;
    }
    field.max_length
}

fn mdac_resave_fixed_size_field(field: &MaterializedField) -> bool {
    matches!(
        field.ado_type_code,
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
                | 131
                | 133
                | 134
                | 135
                | 136
        )
    )
}

fn comparable_native_attribute_flags(
    field: &MaterializedField,
    options: NativeCompareOptions,
) -> u32 {
    let mut flags = field.attribute_flags & !FORMAT_SPECIFIC_NATIVE_FIELD_FLAGS;
    if options.mdac_resave_normalization {
        flags &= !KEY_COLUMN_FLAG;
    }
    if options.mdac_resave_normalization && flags & UPDATABILITY_FLAGS != 0 {
        flags = (flags & !UPDATABILITY_FLAGS) | UPDATABILITY_FLAGS;
    }
    if options.mdac_resave_normalization {
        flags &= !ROW_VERSION_FLAG;
    }
    flags
}

fn comparable_native_precision(field: &MaterializedField) -> Option<usize> {
    matches!(field.ado_type_code, Some(14 | 131 | 139))
        .then_some(field.precision?)
        .filter(|precision| *precision != 255)
}

fn comparable_native_scale(field: &MaterializedField) -> Option<i32> {
    let scale = field.scale?;
    if matches!(field.ado_type_code, Some(14 | 139)) && scale == 255 {
        return None;
    }
    matches!(field.ado_type_code, Some(14 | 131 | 135 | 139)).then_some(scale)
}

fn rows_match_ordered(
    left: &MaterializedRecordset,
    right: &MaterializedRecordset,
    options: NativeCompareOptions,
    depth: usize,
) -> bool {
    if depth > crate::MAX_RECORDSET_DEPTH {
        return false;
    }
    left.rows.len() == right.rows.len()
        && left
            .rows
            .iter()
            .zip(right.rows.iter())
            .all(|(left_row, right_row)| {
                rows_match(left, right, left_row, right_row, options, depth)
            })
}

fn rows_match_unordered(
    left: &MaterializedRecordset,
    right: &MaterializedRecordset,
    options: NativeCompareOptions,
    depth: usize,
) -> bool {
    if left.rows.len() != right.rows.len() {
        return false;
    }
    if depth > crate::MAX_RECORDSET_DEPTH {
        return false;
    }

    let mut used = vec![false; right.rows.len()];
    for left_row in &left.rows {
        let Some(index) = right
            .rows
            .iter()
            .enumerate()
            .position(|(index, right_row)| {
                !used[index] && rows_match(left, right, left_row, right_row, options, depth)
            })
        else {
            return false;
        };
        used[index] = true;
    }
    true
}

fn rows_match(
    left_recordset: &MaterializedRecordset,
    right_recordset: &MaterializedRecordset,
    left: &MaterializedRow,
    right: &MaterializedRow,
    options: NativeCompareOptions,
    depth: usize,
) -> bool {
    left.status == right.status
        && left.values.len() == right.values.len()
        && left.values.len() == left_recordset.fields.len()
        && right.values.len() == right_recordset.fields.len()
        && left
            .values
            .iter()
            .zip(right.values.iter())
            .zip(
                left_recordset
                    .fields
                    .iter()
                    .zip(right_recordset.fields.iter()),
            )
            .all(|((left_value, right_value), (left_field, right_field))| {
                values_match_for_fields(
                    left_field,
                    right_field,
                    left_value,
                    right_value,
                    options,
                    depth,
                )
            })
}

fn values_match_for_fields(
    left_field: &MaterializedField,
    right_field: &MaterializedField,
    left: &Value,
    right: &Value,
    options: NativeCompareOptions,
    depth: usize,
) -> bool {
    if options.mdac_resave_normalization
        && fixed_text_field_pair(left_field, right_field)
        && fixed_text_padding_values_match(left, right)
    {
        return true;
    }
    if options.mdac_resave_normalization
        && ansi_text_field_pair(left_field, right_field)
        && ansi_text_mojibake_values_match(left_field, right_field, left, right)
    {
        return true;
    }
    if variant_text_field_pair(left_field, right_field)
        && variant_text_values_match(left, right, depth)
    {
        return true;
    }
    values_match(left, right, options, depth)
}

fn fixed_text_field_pair(left: &MaterializedField, right: &MaterializedField) -> bool {
    left.name == right.name && fixed_text_field(left) && fixed_text_field(right)
}

fn fixed_text_field(field: &MaterializedField) -> bool {
    matches!(
        field.ado_type_code,
        Some(8 | 129 | 130 | 200 | 201 | 202 | 203)
    ) && field.attribute_flags & FIXED_LENGTH_FLAG != 0
}

fn fixed_text_padding_values_match(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::String(left), Value::String(right)) => {
            left.trim_end_matches('\0') == right.trim_end_matches('\0')
        }
        _ => false,
    }
}

fn ansi_text_field_pair(left: &MaterializedField, right: &MaterializedField) -> bool {
    left.name == right.name
        && text_field(left)
        && text_field(right)
        && (ansi_text_field(left) || ansi_text_field(right))
}

fn ansi_text_field(field: &MaterializedField) -> bool {
    matches!(field.ado_type_code, Some(129 | 200 | 201))
}

fn text_field(field: &MaterializedField) -> bool {
    matches!(
        field.ado_type_code,
        Some(8 | 129 | 130 | 200 | 201 | 202 | 203)
    )
}

fn ansi_text_mojibake_values_match(
    left_field: &MaterializedField,
    right_field: &MaterializedField,
    left: &Value,
    right: &Value,
) -> bool {
    let (Value::String(left), Value::String(right)) = (left, right) else {
        return false;
    };
    let left = comparable_text_value(left_field, left);
    let right = comparable_text_value(right_field, right);
    left == right
        || mdac_xml_ansi_mojibake(left).as_deref() == Some(right)
        || mdac_xml_ansi_mojibake(right).as_deref() == Some(left)
}

fn comparable_text_value<'a>(field: &MaterializedField, value: &'a str) -> &'a str {
    if fixed_text_field(field) {
        value.trim_end_matches('\0')
    } else {
        value
    }
}

fn mdac_xml_ansi_mojibake(value: &str) -> Option<String> {
    let (encoded, _, had_errors) = EUC_KR.encode(value);
    if had_errors {
        return None;
    }
    Some(encoded.iter().map(|byte| char::from(*byte)).collect())
}

fn values_match(left: &Value, right: &Value, options: NativeCompareOptions, depth: usize) -> bool {
    match (left, right) {
        (Value::Float(left), Value::Float(right)) => float_values_match(*left, *right),
        (Value::Decimal(left), Value::Decimal(right)) => {
            canonical_decimal_text(left) == canonical_decimal_text(right)
        }
        (Value::DateTime(left), Value::DateTime(right)) => {
            canonical_datetime_text(left) == canonical_datetime_text(right)
        }
        (Value::Chapter(left), Value::Chapter(right)) => {
            compare_recordsets_with_options_with_depth(left, right, options, depth + 1).is_empty()
        }
        _ => left == right,
    }
}

fn variant_text_field_pair(left: &MaterializedField, right: &MaterializedField) -> bool {
    left.name == right.name
        && ((left.ado_type_code == Some(203) && right.ado_type_code == Some(12))
            || (left.ado_type_code == Some(12) && right.ado_type_code == Some(203)))
        && variant_text_flags(left) == variant_text_flags(right)
}

fn variant_text_flags(field: &MaterializedField) -> u32 {
    field.attribute_flags & !FORMAT_SPECIFIC_NATIVE_FIELD_FLAGS & !VARIANT_TEXT_STORAGE_FLAGS
}

fn variant_text_values_match(left: &Value, right: &Value, depth: usize) -> bool {
    match (left, right) {
        (Value::String(text), value) | (value, Value::String(text)) => {
            mdac_variant_text_matches_value(text, value)
        }
        (Value::Null, Value::Empty) | (Value::Empty, Value::Null) => true,
        _ => values_match(left, right, NativeCompareOptions::strict(), depth),
    }
}

fn mdac_variant_text_matches_value(text: &str, value: &Value) -> bool {
    match value {
        Value::Empty | Value::Null => text.is_empty(),
        Value::Unavailable => false,
        Value::String(value) => text == value,
        Value::Boolean(value) => text.eq_ignore_ascii_case(if *value { "true" } else { "false" }),
        Value::Integer(value) => canonical_decimal_text(text) == value.to_string(),
        Value::UnsignedInteger(value) => canonical_decimal_text(text) == value.to_string(),
        Value::Float(value) => text
            .parse::<f64>()
            .map(|text_value| float_values_match(text_value, *value))
            .unwrap_or(false),
        Value::Decimal(value) => canonical_decimal_text(text) == canonical_decimal_text(value),
        Value::DateTime(value) => mdac_datetime_text_matches(text, value),
        _ => false,
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

fn mdac_datetime_text_matches(text: &str, iso: &str) -> bool {
    if canonical_datetime_text(text) == canonical_datetime_text(iso) {
        return true;
    }

    let Some(normalized) = parse_mdac_datetime_text(text) else {
        return false;
    };
    canonical_datetime_text(&normalized) == canonical_datetime_text(iso)
}

fn parse_mdac_datetime_text(text: &str) -> Option<String> {
    let mut parts = text.split_ascii_whitespace();
    let date = parts.next()?;
    let time = parts.next()?;
    let meridiem = parts.next()?;
    if parts.next().is_some() {
        return None;
    }

    let mut date_parts = date.split('/');
    let month = date_parts.next()?.parse::<u32>().ok()?;
    let day = date_parts.next()?.parse::<u32>().ok()?;
    let year = date_parts.next()?.parse::<u32>().ok()?;
    if date_parts.next().is_some() || month == 0 || month > 12 || day == 0 || day > 31 {
        return None;
    }

    let mut time_parts = time.split(':');
    let mut hour = time_parts.next()?.parse::<u32>().ok()?;
    let minute = time_parts.next()?.parse::<u32>().ok()?;
    let second = time_parts.next()?.parse::<u32>().ok()?;
    if time_parts.next().is_some() || hour == 0 || hour > 12 || minute > 59 || second > 59 {
        return None;
    }

    match meridiem.to_ascii_uppercase().as_str() {
        "AM" if hour == 12 => hour = 0,
        "AM" => {}
        "PM" if hour != 12 => hour += 12,
        "PM" => {}
        _ => return None,
    }

    Some(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}"
    ))
}

#[cfg(test)]
mod tests {
    use super::{compare_mdac_resaved_recordsets, compare_native_recordsets};
    use crate::model::{
        AdoDataType, Field, FieldAttribute, RecordStatusFlag, Recordset, Row, RowChange,
        RowChangeKind, RowState, Value,
    };

    #[test]
    fn mdac_resave_compare_accepts_updatable_unknown_updatable_flip() {
        let updatable = recordset_with_updatability(FieldAttribute::Updatable);
        let unknown = recordset_with_updatability(FieldAttribute::UnknownUpdatable);

        assert!(!compare_native_recordsets(&updatable, &unknown).is_empty());
        assert!(compare_mdac_resaved_recordsets(&updatable, &unknown).is_empty());
    }

    #[test]
    fn mdac_resave_compare_accepts_dropped_binary_rowversion_flag() {
        let rowversion = binary_recordset(vec![FieldAttribute::Fixed, FieldAttribute::RowVersion]);
        let resaved = binary_recordset(vec![FieldAttribute::Fixed]);

        assert!(!compare_native_recordsets(&rowversion, &resaved).is_empty());
        assert!(compare_mdac_resaved_recordsets(&rowversion, &resaved).is_empty());
    }

    #[test]
    fn mdac_resave_compare_accepts_dropped_timestamp_rowversion_flag() {
        let rowversion =
            timestamp_recordset(vec![FieldAttribute::Fixed, FieldAttribute::RowVersion]);
        let resaved = timestamp_recordset(vec![FieldAttribute::Fixed]);

        assert!(!compare_native_recordsets(&rowversion, &resaved).is_empty());
        assert!(compare_mdac_resaved_recordsets(&rowversion, &resaved).is_empty());
    }

    #[test]
    fn mdac_resave_compare_accepts_fixed_text_nul_padding() {
        let unpadded = fixed_text_recordset("MTIzNA==");
        let padded = fixed_text_recordset("MTIzNA==\0\0\0\0");

        assert!(!compare_native_recordsets(&unpadded, &padded).is_empty());
        assert!(compare_mdac_resaved_recordsets(&unpadded, &padded).is_empty());
    }

    #[test]
    fn mdac_resave_compare_accepts_ansi_xml_mojibake() {
        let decoded = ansi_text_recordset("r1_p0_\u{d55c}\u{ae00}_<&'>");
        let mdac_xml = ansi_text_recordset("r1_p0_\u{c7}\u{d1}\u{b1}\u{db}_<&'>");

        assert!(!compare_native_recordsets(&mdac_xml, &decoded).is_empty());
        assert!(compare_mdac_resaved_recordsets(&mdac_xml, &decoded).is_empty());
    }

    #[test]
    fn strict_compare_rejects_provider_metadata_mismatch() {
        let orders = provider_recordset(
            Some("AdoRecordsetSales"),
            Some("dbo"),
            Some("SalesOrders"),
            Some("OrderId"),
        );
        let customers = provider_recordset(
            Some("AdoRecordsetSales"),
            Some("dbo"),
            Some("SalesCustomers"),
            Some("OrderId"),
        );

        assert!(!compare_native_recordsets(&orders, &customers).is_empty());
    }

    #[test]
    fn strict_compare_rejects_key_column_mismatch() {
        let keyed = provider_recordset(
            Some("AdoRecordsetSales"),
            Some("dbo"),
            Some("SalesOrders"),
            Some("OrderId"),
        );
        let mut unkeyed = keyed.clone();
        unkeyed.fields[0].key_column = false;

        assert!(!compare_native_recordsets(&keyed, &unkeyed).is_empty());
        assert!(compare_mdac_resaved_recordsets(&keyed, &unkeyed).is_empty());
    }

    #[test]
    fn mdac_resave_compare_accepts_dropped_provider_catalog_schema_table() {
        let source = provider_recordset(
            Some("AdoRecordsetSales"),
            Some("dbo"),
            Some("SalesOrders"),
            Some("OrderId"),
        );
        let resaved = provider_recordset(None, None, None, Some("OrderId"));

        assert!(!compare_native_recordsets(&source, &resaved).is_empty());
        assert!(compare_mdac_resaved_recordsets(&source, &resaved).is_empty());
    }

    fn recordset_with_updatability(updatability: FieldAttribute) -> Recordset {
        Recordset {
            fields: vec![Field {
                name: "ReviewNote".to_string(),
                xml_name: "ReviewNote".to_string(),
                ordinal: Some(1),
                data_type: Some("string".to_string()),
                db_type: None,
                ado_type: Some(AdoDataType::new("adVarWChar", 202)),
                max_length: Some(40),
                precision: None,
                scale: None,
                nullable: true,
                writable: matches!(updatability, FieldAttribute::Updatable),
                fixed_length: false,
                long: false,
                key_column: false,
                base_catalog: None,
                base_schema: None,
                base_table: None,
                base_column: None,
                chapter_fields: None,
                chapter_relation: None,
                attributes: vec![FieldAttribute::MayBeNull, updatability],
            }],
            rows: vec![Row {
                ordinal: 0,
                state: RowState::Current,
                status_flags: vec![RecordStatusFlag::Unmodified],
                change_index: Some(0),
                values: vec![Value::String("ready".to_string())],
            }],
            changes: vec![RowChange {
                kind: RowChangeKind::Current,
                row_indices: vec![0],
            }],
        }
    }

    fn provider_recordset(
        base_catalog: Option<&str>,
        base_schema: Option<&str>,
        base_table: Option<&str>,
        base_column: Option<&str>,
    ) -> Recordset {
        Recordset {
            fields: vec![Field {
                name: "OrderId".to_string(),
                xml_name: "OrderId".to_string(),
                ordinal: Some(1),
                data_type: Some("int".to_string()),
                db_type: None,
                ado_type: Some(AdoDataType::new("adInteger", 3)),
                max_length: Some(4),
                precision: None,
                scale: None,
                nullable: false,
                writable: true,
                fixed_length: true,
                long: false,
                key_column: true,
                base_catalog: base_catalog.map(str::to_string),
                base_schema: base_schema.map(str::to_string),
                base_table: base_table.map(str::to_string),
                base_column: base_column.map(str::to_string),
                chapter_fields: None,
                chapter_relation: None,
                attributes: vec![FieldAttribute::Fixed, FieldAttribute::Updatable],
            }],
            rows: vec![Row {
                ordinal: 0,
                state: RowState::Current,
                status_flags: vec![RecordStatusFlag::Unmodified],
                change_index: Some(0),
                values: vec![Value::Integer(1001)],
            }],
            changes: vec![RowChange {
                kind: RowChangeKind::Current,
                row_indices: vec![0],
            }],
        }
    }

    fn binary_recordset(attributes: Vec<FieldAttribute>) -> Recordset {
        Recordset {
            fields: vec![Field {
                name: "LegacyRowVersion".to_string(),
                xml_name: "LegacyRowVersion".to_string(),
                ordinal: Some(1),
                data_type: Some("bin.hex".to_string()),
                db_type: None,
                ado_type: Some(AdoDataType::new("adBinary", 128)),
                max_length: Some(8),
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
                attributes,
            }],
            rows: vec![Row {
                ordinal: 0,
                state: RowState::Current,
                status_flags: vec![RecordStatusFlag::Unmodified],
                change_index: Some(0),
                values: vec![Value::BinaryHex("00000000000007D1".to_string())],
            }],
            changes: vec![RowChange {
                kind: RowChangeKind::Current,
                row_indices: vec![0],
            }],
        }
    }

    fn timestamp_recordset(attributes: Vec<FieldAttribute>) -> Recordset {
        Recordset {
            fields: vec![Field {
                name: "RowVersionTimestamp".to_string(),
                xml_name: "RowVersionTimestamp".to_string(),
                ordinal: Some(1),
                data_type: Some("dateTime".to_string()),
                db_type: None,
                ado_type: Some(AdoDataType::new("adDBTimeStamp", 135)),
                max_length: Some(16),
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
                attributes,
            }],
            rows: vec![Row {
                ordinal: 0,
                state: RowState::Current,
                status_flags: vec![RecordStatusFlag::Unmodified],
                change_index: Some(0),
                values: vec![Value::DateTime("2026-06-15T01:02:03".to_string())],
            }],
            changes: vec![RowChange {
                kind: RowChangeKind::Current,
                row_indices: vec![0],
            }],
        }
    }

    fn fixed_text_recordset(value: &str) -> Recordset {
        Recordset {
            fields: vec![Field {
                name: "FixedText".to_string(),
                xml_name: "FixedText".to_string(),
                ordinal: Some(1),
                data_type: Some("string".to_string()),
                db_type: None,
                ado_type: Some(AdoDataType::new("adWChar", 130)),
                max_length: Some(12),
                precision: None,
                scale: None,
                nullable: true,
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
                attributes: vec![FieldAttribute::MayBeNull, FieldAttribute::Fixed],
            }],
            rows: vec![Row {
                ordinal: 0,
                state: RowState::Current,
                status_flags: vec![RecordStatusFlag::Unmodified],
                change_index: Some(0),
                values: vec![Value::String(value.to_string())],
            }],
            changes: vec![RowChange {
                kind: RowChangeKind::Current,
                row_indices: vec![0],
            }],
        }
    }

    fn ansi_text_recordset(value: &str) -> Recordset {
        Recordset {
            fields: vec![Field {
                name: "AnsiText".to_string(),
                xml_name: "AnsiText".to_string(),
                ordinal: Some(1),
                data_type: Some("string".to_string()),
                db_type: None,
                ado_type: Some(AdoDataType::new("adVarChar", 200)),
                max_length: Some(80),
                precision: None,
                scale: None,
                nullable: true,
                writable: false,
                fixed_length: false,
                long: false,
                key_column: false,
                base_catalog: None,
                base_schema: None,
                base_table: None,
                base_column: None,
                chapter_fields: None,
                chapter_relation: None,
                attributes: vec![FieldAttribute::MayBeNull],
            }],
            rows: vec![Row {
                ordinal: 0,
                state: RowState::Current,
                status_flags: vec![RecordStatusFlag::Unmodified],
                change_index: Some(0),
                values: vec![Value::String(value.to_string())],
            }],
            changes: vec![RowChange {
                kind: RowChangeKind::Current,
                row_indices: vec![0],
            }],
        }
    }
}
