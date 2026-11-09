use super::i18n::tr;
use chrono::{DateTime, Local};

pub(crate) fn stamp(millis: i64) -> String {
    DateTime::from_timestamp_millis(millis)
        .map(|at| at.with_timezone(&Local).format("%m-%d %H:%M").to_string())
        .unwrap_or_default()
}
pub(crate) fn period(millis: u64) -> String {
    let Some(at) = DateTime::from_timestamp_millis(millis as i64) else {
        return tr!("earlier");
    };
    let at = at.with_timezone(&Local);
    let days = Local::now()
        .date_naive()
        .signed_duration_since(at.date_naive())
        .num_days();
    match days {
        ..=0 => tr!("today"),
        1 => tr!("yesterday"),
        2..=6 => tr!("last-week"),
        _ => at.format("%Y-%m").to_string(),
    }
}
