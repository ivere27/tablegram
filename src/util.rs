//! Small shared primitives used by multiple ADO format modules.

use crate::model::Value;

pub(crate) fn gregorian_month_len(year: u16, month: u16) -> Option<u16> {
    Some(match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_gregorian_leap_year(year) => 29,
        2 => 28,
        _ => return None,
    })
}

pub(crate) fn is_gregorian_leap_year(year: u16) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

pub(crate) fn is_valid_gregorian_date(year: u16, month: u16, day: u16) -> bool {
    (1..=9999).contains(&year)
        && gregorian_month_len(year, month).is_some_and(|max_day| (1..=max_day).contains(&day))
}

pub(crate) fn overlay_unavailable_values(original: &[Value], updated: &[Value]) -> Vec<Value> {
    original
        .iter()
        .zip(updated.iter())
        .map(|(old, new)| {
            if matches!(new, Value::Unavailable) {
                old.clone()
            } else {
                new.clone()
            }
        })
        .collect()
}
