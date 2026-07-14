use crate::commands::tracker::{is_in_night_range, second_buckets};
use chrono::{Local, TimeZone};

#[test]
fn second_buckets_splits_seconds_across_hour_boundary() {
    let now = Local.with_ymd_and_hms(2026, 1, 2, 0, 0, 2).unwrap();

    let buckets = second_buckets(now, 4);

    assert_eq!(buckets.len(), 2);
    assert_eq!(buckets[0].1, 0);
    assert_eq!(buckets[0].2, 2);
    assert_eq!(buckets[1].1, 23);
    assert_eq!(buckets[1].2, 2);
}

#[test]
fn night_range_handles_same_day_and_wrapped_ranges() {
    assert!(is_in_night_range("23:30", "23:00", "06:00"));
    assert!(is_in_night_range("05:30", "23:00", "06:00"));
    assert!(!is_in_night_range("12:00", "23:00", "06:00"));

    assert!(is_in_night_range("14:00", "09:00", "18:00"));
    assert!(!is_in_night_range("20:00", "09:00", "18:00"));
}
