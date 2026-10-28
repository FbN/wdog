use super::*;

#[test]
fn suffixes() {
    assert_eq!(parse("45").unwrap(), Duration::from_secs(45));
    assert_eq!(parse("45s").unwrap(), Duration::from_secs(45));
    assert_eq!(parse("30m").unwrap(), Duration::from_secs(1800));
    assert_eq!(parse("2h").unwrap(), Duration::from_secs(7200));
    assert_eq!(parse("200ms").unwrap(), Duration::from_millis(200));
    assert_eq!(parse("1.5m").unwrap(), Duration::from_secs(90));
    assert_eq!(parse(" 5s ").unwrap(), Duration::from_secs(5));
}

#[test]
fn errors() {
    assert_eq!(parse(""), Err(DurationError::Empty));
    assert!(matches!(parse("abc"), Err(DurationError::BadNumber(_))));
    assert!(matches!(parse("-5"), Err(DurationError::BadNumber(_))));
    assert!(matches!(parse("5d"), Err(DurationError::BadSuffix(_))));
    assert!(matches!(parse("."), Err(DurationError::BadNumber(_))));
    assert!(matches!(parse("1.2.3"), Err(DurationError::BadNumber(_))));
    assert!(matches!(parse("99999999999h"), Err(DurationError::TooLarge(_))));
}

#[test]
fn short_format() {
    assert_eq!(fmt_short(Duration::from_millis(300)), "0.3s");
    assert_eq!(fmt_short(Duration::from_secs(41)), "41s");
    assert_eq!(fmt_short(Duration::from_secs(59)), "59s");
    assert_eq!(fmt_short(Duration::from_secs(60)), "1m00s");
    assert_eq!(fmt_short(Duration::from_secs(125)), "2m05s");
    assert_eq!(fmt_short(Duration::from_secs(3600)), "1h00m");
    assert_eq!(fmt_short(Duration::from_secs(3720)), "1h02m");
}
