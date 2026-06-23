//! ADO-compatible materialized views used by tests and comparison tooling.
//!
//! The persisted model keeps raw row-state groups, while these helpers expose
//! the default, pending, affected, and conflicting views used by the COM oracle.
//! Updated rows overlay `Value::Unavailable` cells with original values to
//! match ADO's visible row contents.

use anyhow::{Context, Result};
use serde::Serialize;

use crate::model::{FieldAttribute, RecordStatusFlag, Recordset, RowChangeKind, RowState, Value};
use crate::util::overlay_unavailable_values;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MaterializedRecordset {
    pub fields: Vec<MaterializedField>,
    pub rows: Vec<MaterializedRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MaterializedField {
    pub name: String,
    pub ado_type_code: Option<u16>,
    pub max_length: Option<usize>,
    pub precision: Option<usize>,
    pub scale: Option<i32>,
    pub attribute_flags: u32,
    pub base_catalog: Option<String>,
    pub base_schema: Option<String>,
    pub base_table: Option<String>,
    pub base_column: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MaterializedRow {
    pub status: RecordStatusFlag,
    pub values: Vec<Value>,
}

pub fn materialize_default_view(recordset: &Recordset) -> MaterializedRecordset {
    materialize_default_view_unchecked(recordset)
}

/// Materializes the default ADO view after validating the Recordset shape.
///
/// Prefer this for untrusted or caller-built `Recordset` values. The
/// non-fallible [`materialize_default_view`] helper is a compatibility
/// convenience for parser-produced values and does not validate its input.
pub fn try_materialize_default_view(recordset: &Recordset) -> Result<MaterializedRecordset> {
    validate_materialize_input(recordset, "default view")?;
    Ok(materialize_default_view_unchecked(recordset))
}

fn materialize_default_view_unchecked(recordset: &Recordset) -> MaterializedRecordset {
    let fields = materialize_fields(recordset);

    let mut rows = Vec::new();
    for change in &recordset.changes {
        match change.kind {
            RowChangeKind::Current => {
                for row_index in &change.row_indices {
                    if let Some(row) = recordset.rows.get(*row_index) {
                        rows.push(MaterializedRow {
                            status: RecordStatusFlag::Unmodified,
                            values: row.values.clone(),
                        });
                    }
                }
            }
            RowChangeKind::Insert => {
                for row_index in &change.row_indices {
                    if let Some(row) = recordset.rows.get(*row_index) {
                        rows.push(MaterializedRow {
                            status: RecordStatusFlag::New,
                            values: row.values.clone(),
                        });
                    }
                }
            }
            RowChangeKind::Update => {
                let original = change
                    .row_indices
                    .iter()
                    .filter_map(|index| recordset.rows.get(*index))
                    .find(|row| row.state == RowState::Original);
                let updated = change
                    .row_indices
                    .iter()
                    .filter_map(|index| recordset.rows.get(*index))
                    .find(|row| row.state == RowState::Updated);

                if let (Some(original), Some(updated)) = (original, updated) {
                    rows.push(MaterializedRow {
                        status: RecordStatusFlag::Modified,
                        values: overlay_unavailable_values(&original.values, &updated.values),
                    });
                }
            }
            RowChangeKind::Delete => {}
        }
    }

    MaterializedRecordset { fields, rows }
}

pub fn materialize_pending_view(recordset: &Recordset) -> MaterializedRecordset {
    materialize_pending_view_unchecked(recordset)
}

/// Materializes the pending-changes ADO view after validating the Recordset shape.
///
/// Prefer this for untrusted or caller-built `Recordset` values. The
/// non-fallible [`materialize_pending_view`] helper is a compatibility
/// convenience for parser-produced values and does not validate its input.
pub fn try_materialize_pending_view(recordset: &Recordset) -> Result<MaterializedRecordset> {
    validate_materialize_input(recordset, "pending view")?;
    Ok(materialize_pending_view_unchecked(recordset))
}

fn materialize_pending_view_unchecked(recordset: &Recordset) -> MaterializedRecordset {
    let fields = materialize_fields(recordset);

    let mut rows = Vec::new();
    for change in &recordset.changes {
        match change.kind {
            RowChangeKind::Current => {}
            RowChangeKind::Insert => {
                for row_index in &change.row_indices {
                    if let Some(row) = recordset.rows.get(*row_index) {
                        rows.push(MaterializedRow {
                            status: RecordStatusFlag::New,
                            values: row.values.clone(),
                        });
                    }
                }
            }
            RowChangeKind::Update => {
                let original = change
                    .row_indices
                    .iter()
                    .filter_map(|index| recordset.rows.get(*index))
                    .find(|row| row.state == RowState::Original);
                let updated = change
                    .row_indices
                    .iter()
                    .filter_map(|index| recordset.rows.get(*index))
                    .find(|row| row.state == RowState::Updated);

                if let (Some(original), Some(updated)) = (original, updated) {
                    rows.push(MaterializedRow {
                        status: RecordStatusFlag::Modified,
                        values: overlay_unavailable_values(&original.values, &updated.values),
                    });
                }
            }
            RowChangeKind::Delete => {
                for row_index in &change.row_indices {
                    if let Some(row) = recordset.rows.get(*row_index) {
                        rows.push(MaterializedRow {
                            status: RecordStatusFlag::Deleted,
                            values: row.values.clone(),
                        });
                    }
                }
            }
        }
    }

    MaterializedRecordset { fields, rows }
}

pub fn materialize_affected_view(recordset: &Recordset) -> MaterializedRecordset {
    MaterializedRecordset {
        fields: materialize_fields(recordset),
        rows: Vec::new(),
    }
}

/// Materializes the affected-records ADO view after validating the Recordset shape.
///
/// Prefer this for untrusted or caller-built `Recordset` values. The
/// non-fallible [`materialize_affected_view`] helper is a compatibility
/// convenience for parser-produced values and does not validate its input.
pub fn try_materialize_affected_view(recordset: &Recordset) -> Result<MaterializedRecordset> {
    validate_materialize_input(recordset, "affected view")?;
    Ok(MaterializedRecordset {
        fields: materialize_fields(recordset),
        rows: Vec::new(),
    })
}

pub fn materialize_conflicting_view(recordset: &Recordset) -> MaterializedRecordset {
    MaterializedRecordset {
        fields: materialize_fields(recordset),
        rows: Vec::new(),
    }
}

/// Materializes the conflicting-records ADO view after validating the Recordset shape.
///
/// Prefer this for untrusted or caller-built `Recordset` values. The
/// non-fallible [`materialize_conflicting_view`] helper is a compatibility
/// convenience for parser-produced values and does not validate its input.
pub fn try_materialize_conflicting_view(recordset: &Recordset) -> Result<MaterializedRecordset> {
    validate_materialize_input(recordset, "conflicting view")?;
    Ok(MaterializedRecordset {
        fields: materialize_fields(recordset),
        rows: Vec::new(),
    })
}

fn validate_materialize_input(recordset: &Recordset, view: &str) -> Result<()> {
    crate::validate_recordset_shape(recordset)
        .with_context(|| format!("cannot materialize ADO Recordset {view}"))
}

fn materialize_fields(recordset: &Recordset) -> Vec<MaterializedField> {
    recordset
        .fields
        .iter()
        .map(|field| MaterializedField {
            name: field.name.clone(),
            ado_type_code: field.ado_type.map(|ty| ty.code),
            max_length: field.max_length,
            precision: field.precision,
            scale: field.scale,
            attribute_flags: FieldAttribute::bits(&field.attributes)
                | if field.key_column { 0x8000 } else { 0 },
            base_catalog: field.base_catalog.clone(),
            base_schema: field.base_schema.clone(),
            base_table: field.base_table.clone(),
            base_column: field.base_column.clone(),
        })
        .collect()
}
