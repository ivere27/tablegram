//! Shared in-memory representation of an ADO `Recordset`.
//!
//! The model is intentionally close to ADO persistence concepts: fields carry
//! ADO type metadata, rows carry their materialized values, and `changes`
//! preserve current/insert/update/delete groups needed by XML updategrams and
//! ADTG row-state streams.

use serde::Serialize;

/// Parsed or caller-built ADO `Recordset` data.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Recordset {
    /// Field metadata in visible ordinal order.
    pub fields: Vec<Field>,
    /// Rows in the persisted/materialized order for this recordset.
    pub rows: Vec<Row>,
    /// Row-state groups referenced by [`Row::change_index`].
    pub changes: Vec<RowChange>,
}

/// ADO field metadata.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Field {
    /// Display name exposed by ADO `Field.Name`.
    pub name: String,
    /// XML row-attribute name used in ADO XML persistence.
    pub xml_name: String,
    /// One-based field ordinal when the source stream provided it.
    pub ordinal: Option<usize>,
    /// Raw XML `dt:type` name when available.
    pub data_type: Option<String>,
    /// Raw XML `rs:dbtype` or equivalent provider type metadata.
    pub db_type: Option<String>,
    /// Canonical ADO `DataTypeEnum` name/code.
    pub ado_type: Option<AdoDataType>,
    /// Maximum byte/character width from ADO metadata.
    pub max_length: Option<usize>,
    /// Numeric precision when the field type carries one.
    pub precision: Option<usize>,
    /// Numeric scale when the field type carries one.
    pub scale: Option<i32>,
    /// Whether the field accepts null values.
    pub nullable: bool,
    /// Whether ADO marked the field writable.
    pub writable: bool,
    /// Whether ADO marked the field fixed-length.
    pub fixed_length: bool,
    /// Whether ADO marked the field as a long value.
    pub long: bool,
    /// Whether provider metadata identifies this field as a key column.
    pub key_column: bool,
    /// Provider base catalog, if persisted.
    pub base_catalog: Option<String>,
    /// Provider base schema, if persisted.
    pub base_schema: Option<String>,
    /// Provider base table, if persisted.
    pub base_table: Option<String>,
    /// Provider base column, if persisted.
    pub base_column: Option<String>,
    /// Child field schema for `adChapter` fields.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chapter_fields: Option<Vec<Field>>,
    /// Parent/child key relation for shaped `adChapter` fields.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chapter_relation: Option<ChapterRelation>,
    /// Raw ADO field attributes normalized to known flags.
    pub attributes: Vec<FieldAttribute>,
}

/// Shaped-recordset relation metadata for a chapter field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChapterRelation {
    /// One or more parent/child field ordinal pairs.
    pub pairs: Vec<ChapterRelationPair>,
}

/// A parent/child field ordinal pair in a chapter relation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChapterRelationPair {
    /// One-based ordinal in the parent recordset.
    pub parent_ordinal: usize,
    /// One-based ordinal in the child recordset.
    pub child_ordinal: usize,
}

/// A row plus ADO row-state metadata.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    /// Zero-based row ordinal inside [`Recordset::rows`].
    pub ordinal: usize,
    /// Logical row state used to materialize default/pending views.
    pub state: RowState,
    /// ADO record status flags preserved from or derived for the row.
    pub status_flags: Vec<RecordStatusFlag>,
    /// Index into [`Recordset::changes`].
    pub change_index: Option<usize>,
    /// Field values in [`Recordset::fields`] order.
    pub values: Vec<Value>,
}

/// A logical row change group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RowChange {
    /// Current, insert, update, or delete group kind.
    pub kind: RowChangeKind,
    /// Row indices belonging to this change group.
    pub row_indices: Vec<usize>,
}

/// Kind of row-state group represented by [`RowChange`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowChangeKind {
    /// A current row with no pending change.
    Current,
    /// A newly inserted row.
    Insert,
    /// Original/updated row pair.
    Update,
    /// Deleted row.
    Delete,
}

/// Per-row state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowState {
    /// Current row with no pending change.
    Current,
    /// Original side of an update or delete.
    Original,
    /// Updated side of an update.
    Updated,
    /// Inserted row.
    Inserted,
    /// Deleted row.
    Deleted,
}

/// ADO record status flag normalized to supported states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordStatusFlag {
    /// ADO `adRecOK`.
    Ok,
    /// ADO `adRecNew`.
    New,
    /// ADO `adRecModified`.
    Modified,
    /// ADO `adRecDeleted`.
    Deleted,
    /// ADO `adRecUnmodified`.
    Unmodified,
}

/// Known ADO field attribute flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldAttribute {
    /// ADO `adFldCacheDeferred`.
    CacheDeferred,
    /// ADO `adFldFixed`.
    Fixed,
    /// ADO `adFldIsChapter`.
    IsChapter,
    /// ADO `adFldIsCollection`.
    IsCollection,
    /// ADO `adFldIsDefaultStream`.
    IsDefaultStream,
    /// ADO `adFldIsNullable`.
    IsNullable,
    /// ADO `adFldIsRowURL`.
    IsRowUrl,
    /// ADO `adFldLong`.
    Long,
    /// ADO `adFldMayBeNull`.
    MayBeNull,
    /// ADO `adFldMayDefer`.
    MayDefer,
    /// ADO `adFldNegativeScale`.
    NegativeScale,
    /// ADO `adFldRowID`.
    RowId,
    /// ADO `adFldRowVersion`.
    RowVersion,
    /// ADO `adFldUnknownUpdatable`.
    UnknownUpdatable,
    /// ADO `adFldUpdatable`.
    Updatable,
}

impl FieldAttribute {
    const BIT_ORDER: [(Self, u32); 15] = [
        (Self::CacheDeferred, 0x1000),
        (Self::Fixed, 0x10),
        (Self::IsChapter, 0x2000),
        (Self::IsCollection, 0x40000),
        (Self::IsDefaultStream, 0x20000),
        (Self::IsNullable, 0x20),
        (Self::IsRowUrl, 0x10000),
        (Self::MayBeNull, 0x40),
        (Self::MayDefer, 0x02),
        (Self::Long, 0x80),
        (Self::NegativeScale, 0x4000),
        (Self::RowId, 0x100),
        (Self::RowVersion, 0x200),
        (Self::UnknownUpdatable, 0x08),
        (Self::Updatable, 0x04),
    ];

    /// Numeric ADO `FieldAttributeEnum` bit for this attribute.
    pub fn bit(self) -> u32 {
        for (attribute, bit) in Self::BIT_ORDER {
            if attribute == self {
                return bit;
            }
        }
        unreachable!("FieldAttribute::BIT_ORDER must include every variant")
    }

    /// Decode known ADO `FieldAttributeEnum` bits in ADTG/COM comparison order.
    pub fn from_bits(bits: u32) -> Vec<Self> {
        Self::BIT_ORDER
            .iter()
            .filter_map(|(attribute, bit)| (bits & bit != 0).then_some(*attribute))
            .collect()
    }

    /// Encode known ADO `FieldAttributeEnum` bits.
    pub fn bits(attributes: &[Self]) -> u32 {
        attributes
            .iter()
            .fold(0u32, |bits, attribute| bits | attribute.bit())
    }
}

/// Canonical ADO `DataTypeEnum` metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AdoDataType {
    /// Symbolic ADO type name, for example `adVarWChar`.
    pub name: &'static str,
    /// Numeric ADO `DataTypeEnum` code.
    pub code: u16,
}

impl AdoDataType {
    /// Construct an ADO type descriptor.
    pub const fn new(name: &'static str, code: u16) -> Self {
        Self { name, code }
    }
}

/// A field value.
///
/// Textual decimal/date/time representations are normalized strings so they
/// can be round-tripped without exposing MDAC's binary encodings to callers.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Value {
    /// ADO `Empty`/variant-empty value.
    Empty,
    /// SQL/ADO null.
    Null,
    /// A value omitted from a pending update payload.
    Unavailable,
    /// Text value.
    String(String),
    /// Boolean value.
    Boolean(bool),
    /// Signed integer value.
    Integer(i64),
    /// Unsigned integer value.
    UnsignedInteger(u64),
    /// Floating-point value. Non-finite values are rejected by validation.
    Float(f64),
    /// Decimal or currency value as a normalized base-10 string.
    Decimal(String),
    /// Date value in `YYYY-MM-DD` form.
    Date(String),
    /// Time value in `HH:MM:SS[.fraction]` form.
    Time(String),
    /// Date-time value in ISO-like form.
    DateTime(String),
    /// GUID value in canonical text form.
    Guid(String),
    /// Binary data encoded as uppercase hexadecimal.
    BinaryHex(String),
    /// Nested recordset value for `adChapter` fields.
    Chapter(Box<Recordset>),
}
