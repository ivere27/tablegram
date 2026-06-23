//! Documented corpus compatibility boundaries.
//!
//! These tables keep intentional native-only artifacts and COM oracle skips in
//! code so verification commands report known boundaries explicitly instead of
//! silently passing over them.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NativeAdtgOnlyReason {
    ShapeAdtgOnlyPendingChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ComVerificationSkipReason {
    MdacDatetimeTzRoundtripReopensEmpty,
    MdacFloatAliasUnstableOracle,
}

pub const DOCUMENTED_NATIVE_ADTG_ONLY_ARTIFACTS: &[(&str, NativeAdtgOnlyReason)] = &[
    (
        "orders_pending_changes_shape.adtg",
        NativeAdtgOnlyReason::ShapeAdtgOnlyPendingChange,
    ),
    (
        "orders_parent_insert_delete_shape.adtg",
        NativeAdtgOnlyReason::ShapeAdtgOnlyPendingChange,
    ),
    (
        "orders_parent_relation_key_update_shape.adtg",
        NativeAdtgOnlyReason::ShapeAdtgOnlyPendingChange,
    ),
    (
        "orders_child_relation_key_update_shape.adtg",
        NativeAdtgOnlyReason::ShapeAdtgOnlyPendingChange,
    ),
    (
        "orders_composite_parent_relation_key_update_shape.adtg",
        NativeAdtgOnlyReason::ShapeAdtgOnlyPendingChange,
    ),
    (
        "orders_composite_child_relation_key_update_shape.adtg",
        NativeAdtgOnlyReason::ShapeAdtgOnlyPendingChange,
    ),
    (
        "orders_lines_product_pending_shape.adtg",
        NativeAdtgOnlyReason::ShapeAdtgOnlyPendingChange,
    ),
    (
        "orders_lines_product_legacy_pending_shape.adtg",
        NativeAdtgOnlyReason::ShapeAdtgOnlyPendingChange,
    ),
    (
        "orders_calc_new_pending_shape.adtg",
        NativeAdtgOnlyReason::ShapeAdtgOnlyPendingChange,
    ),
];

pub const DOCUMENTED_COM_VERIFICATION_SKIP_ARTIFACTS: &[(&str, ComVerificationSkipReason)] = &[
    // MDAC saves this XML roundtrip from ADTG, but then reopens it as an
    // empty rowset after fixed-length dateTime.tz text gains control bytes.
    (
        "doc_datetime_tz_fallback.roundtrip.xml",
        ComVerificationSkipReason::MdacDatetimeTzRoundtripReopensEmpty,
    ),
    // The documented r8 + dt:maxLength="4" fixture is materialized as a
    // 4-byte double slot. MDAC intermittently reads uninitialized high bytes
    // from this field in source XML, ADTG, and ADO's own XML roundtrip.
    (
        "doc_float_type_aliases.xml",
        ComVerificationSkipReason::MdacFloatAliasUnstableOracle,
    ),
    (
        "doc_float_type_aliases.adtg",
        ComVerificationSkipReason::MdacFloatAliasUnstableOracle,
    ),
    (
        "doc_float_type_aliases.roundtrip.xml",
        ComVerificationSkipReason::MdacFloatAliasUnstableOracle,
    ),
];

pub fn is_documented_native_adtg_only_path(path: &Path) -> bool {
    documented_native_adtg_only_reason_path(path).is_some()
}

pub fn documented_native_adtg_only_reason_path(path: &Path) -> Option<NativeAdtgOnlyReason> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(documented_native_adtg_only_reason_name)
}

pub fn is_documented_native_adtg_only_name(name: &str) -> bool {
    documented_native_adtg_only_reason_name(name).is_some()
}

pub fn documented_native_adtg_only_reason_name(name: &str) -> Option<NativeAdtgOnlyReason> {
    DOCUMENTED_NATIVE_ADTG_ONLY_ARTIFACTS
        .iter()
        .find_map(|(artifact, reason)| (*artifact == name).then_some(*reason))
}

pub fn is_documented_com_verification_skip_path(path: &Path) -> bool {
    documented_com_verification_skip_reason_path(path).is_some()
}

pub fn documented_com_verification_skip_reason_path(
    path: &Path,
) -> Option<ComVerificationSkipReason> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(documented_com_verification_skip_reason_name)
}

pub fn is_documented_com_verification_skip_name(name: &str) -> bool {
    documented_com_verification_skip_reason_name(name).is_some()
}

pub fn documented_com_verification_skip_reason_name(
    name: &str,
) -> Option<ComVerificationSkipReason> {
    DOCUMENTED_COM_VERIFICATION_SKIP_ARTIFACTS
        .iter()
        .find_map(|(artifact, reason)| (*artifact == name).then_some(*reason))
}
