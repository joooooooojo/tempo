use crate::commands::tracker::second_buckets;
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
