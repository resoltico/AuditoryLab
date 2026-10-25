//! Independent identity/calendar/timezone boundary checks.
use auditorylab_core::{DocumentUuid, Timestamp};
#[test]
fn instants_normalize_offsets_without_losing_nanoseconds_or_unknown_dates() {
    let a: Timestamp = "2026-10-24T09:00:00.123456789+03:00".parse().unwrap();
    let b: Timestamp = "2026-10-24T06:00:00.123456789Z".parse().unwrap();
    assert_eq!(a, b);
    assert_eq!(a.to_string(), "2026-10-24T06:00:00.123456789Z");
    assert!(a > "2026-10-24T06:00:00.123456788Z".parse().unwrap());
    assert_eq!(
        "2000-02-29T00:00:00Z"
            .parse::<Timestamp>()
            .unwrap()
            .to_string(),
        "2000-02-29T00:00:00.000000000Z"
    );
    for text in [
        "2026-02-29T00:00:00Z",
        "1900-02-29T00:00:00Z",
        "0000-01-01T00:00:00Z",
        "0000-12-31T23:59:00-01:00",
        "10000-01-01T00:00:00Z",
        "2026-10-24t00:00:00Z",
        "2026-10-24T00:00:00z",
        "2026-10-24T00:00:00-00:00",
        "2026-10-24T00:00:60Z",
        "2026-10-24T00:00:00",
        "2026-10-24T00:00:00.1234567890Z",
        "2026-10-24T00:00:00.Z",
    ] {
        assert!(text.parse::<Timestamp>().is_err(), "{text}");
    }
}
#[test]
fn uuid_shape_is_strict_but_does_not_claim_provenance() {
    let id = "00000000-0000-4000-8000-000000000001";
    assert_eq!(id.parse::<DocumentUuid>().unwrap().to_string(), id);
    for invalid in [
        "00000000-0000-1000-8000-000000000001",
        "00000000-0000-4000-c000-000000000001",
        "00000000000040008000000000000001",
        "{00000000-0000-4000-8000-000000000001}",
        "aaaaaaaa-AAAA-4aaa-8aaa-aaaaaaaaaaaa",
    ] {
        assert!(invalid.parse::<DocumentUuid>().is_err(), "{invalid}");
    }
}
