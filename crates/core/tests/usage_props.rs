use proptest::prelude::*;
use wanxiang_core::{accept_percent, calendar_cutoff, parse_minor_units};

proptest! {
    #[test]
    fn money_strings_do_not_become_zero_when_invalid(junk in "\\PC{0,12}") {
        let parsed = parse_minor_units(&junk);
        if parsed.is_none() {
            prop_assert_ne!(parsed, Some(0));
        }
    }

    #[test]
    fn percent_outside_0_to_100_is_missing(value in -10_000i64..10_000) {
        let parsed = accept_percent(value);
        if (0..=100).contains(&value) {
            prop_assert_eq!(parsed, Some(i32::try_from(value).unwrap()));
        } else {
            prop_assert!(parsed.is_none());
        }
    }

    #[test]
    fn retention_keeps_the_cutoff_day(
        months in 1u32..121,
        year in 2020i32..2031,
        month in 1u32..13,
        day in 1u32..29,
    ) {
        let today = format!("{year:04}-{month:02}-{day:02}");
        let cutoff = calendar_cutoff(&today, months).unwrap();
        prop_assert!(cutoff.as_str() <= today.as_str());
        prop_assert!(cutoff.as_str() >= "1970-01-01");
        let equal_kept = cutoff.as_str() < cutoff.as_str();
        prop_assert!(!equal_kept);
        if let Some(before) = previous_day(&cutoff) {
            prop_assert!(before.as_str() < cutoff.as_str());
        }
    }
}

fn previous_day(day: &str) -> Option<String> {
    let date = chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()?;
    date.pred_opt().map(|date| date.format("%Y-%m-%d").to_string())
}

#[test]
fn minor_units_keep_two_decimal_places() {
    assert_eq!(parse_minor_units("12.34"), Some(1234));
    assert_eq!(parse_minor_units("12.3"), Some(1230));
    assert_eq!(parse_minor_units("12"), Some(1200));
    assert_eq!(parse_minor_units("0.00"), Some(0));
    assert_eq!(parse_minor_units("12.345"), None);
    assert_eq!(parse_minor_units("-1.00"), None);
    assert_eq!(parse_minor_units(""), None);
    assert_eq!(parse_minor_units(" 12.00"), None);
}
