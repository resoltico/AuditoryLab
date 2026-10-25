//! Native timezone database integration in isolated processes; never changes
//! the parent process or system timezone settings.
#![cfg(unix)]
use std::{fs, process::Command};
#[test]
#[ignore = "subprocess fixture, explicitly invoked by native_calendar_uses_actual_timezone_database"]
fn calendar_child() {
    let date = std::env::var("AUDITORYLAB_CALENDAR_TEST_DATE").unwrap();
    let range = auditorylab::calendar::local_range(
        auditorylab::calendar::CalendarField::Created,
        &date,
        &date,
    )
    .unwrap();
    let shown: Vec<_> = [
        "2026-03-28T22:30:00.123456789Z",
        "2026-03-29T01:30:00Z",
        "2026-10-25T00:30:00Z",
        "2026-10-25T01:30:00Z",
    ]
    .into_iter()
    .map(|s| auditorylab::calendar::local_timestamp(s.parse().unwrap()))
    .collect();
    fs::write(std::env::var_os("AUDITORYLAB_CALENDAR_TEST_RESULT").unwrap(),serde_json::to_vec(&serde_json::json!({"from":range.from,"before":range.before,"context":auditorylab::calendar::local_context(),"shown":shown})).unwrap()).unwrap();
}
#[test]
fn native_calendar_uses_actual_timezone_database() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    fs::create_dir_all(&base).unwrap();
    let temp = tempfile::tempdir_in(base).unwrap();
    for (day, from, before, hours) in [
        (
            "2026-03-29",
            "2026-03-28T22:00:00.000000000Z",
            "2026-03-29T21:00:00.000000000Z",
            23,
        ),
        (
            "2026-10-25",
            "2026-10-24T21:00:00.000000000Z",
            "2026-10-25T22:00:00.000000000Z",
            25,
        ),
    ] {
        let result = temp.path().join(format!("{day}.json"));
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "calendar_child", "--ignored", "--nocapture"])
            .env("TZ", "Europe/Riga")
            .env("AUDITORYLAB_CALENDAR_TEST_DATE", day)
            .env("AUDITORYLAB_CALENDAR_TEST_RESULT", &result)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let value: serde_json::Value = serde_json::from_slice(&fs::read(result).unwrap()).unwrap();
        assert_eq!(
            value["from"], from,
            "native calendar tests require Europe/Riga in the host timezone database; install tzdata if missing"
        );
        assert_eq!(value["before"], before);
        assert_eq!(
            value["shown"],
            serde_json::json!([
                "2026-03-29 00:30:00.123456789 UTC+02:00",
                "2026-03-29 04:30:00 UTC+03:00",
                "2026-10-25 03:30:00 UTC+03:00",
                "2026-10-25 03:30:00 UTC+02:00",
            ])
        );
        let start: auditorylab::core::Timestamp = value["from"].as_str().unwrap().parse().unwrap();
        let end: auditorylab::core::Timestamp = value["before"].as_str().unwrap().parse().unwrap();
        assert_eq!(end.unix_seconds() - start.unix_seconds(), hours * 3600);
    }
}
