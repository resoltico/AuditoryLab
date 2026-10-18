// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! A second consumer of the public application facade, independent of terminal code.
use auditorylab::*;
use std::{
    fs,
    io::{self, Read},
    path::Path,
};
const SILENCE: &[u8] = b"schema_version=2\nduration_seconds=0.01\nfade_seconds=0\n";
fn origin() -> Origin {
    Origin::Reader("consumer".into())
}
fn transferable<T: Send + Sync + 'static>() {}
#[test]
fn owned_values_and_errors_cross_worker_channels() {
    transferable::<LoadedPreset>();
    transferable::<DeclaredDescription>();
    transferable::<ExportView>();
    transferable::<PlaybackView>();
    transferable::<SaveReport>();
    transferable::<ExportReport>();
    transferable::<Facts>();
    transferable::<OutputDescription>();
    transferable::<Exported>();
    transferable::<PlaybackReport>();
    transferable::<AppError>();
    transferable::<Cancellation>();
    let cancel = Cancellation::default();
    let worker_cancel = cancel.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("worker.wav");
    let output = destination.clone();
    let worker = std::thread::spawn(move || {
        let result = load_bytes(SILENCE, origin()).and_then(|p| {
            let facts = p.inspect(SampleRate::DEFAULT)?;
            let prepared = p.prepare_export(SampleRate::DEFAULT, &output)?;
            let view = prepared.view().clone();
            let completed = prepared.execute(&worker_cancel)?;
            Ok((facts, view, completed))
        });
        tx.send((result, worker_cancel.is_requested())).unwrap();
    });
    let (result, _) = rx.recv().unwrap();
    let (facts, view, completed) = result.unwrap();
    assert_eq!(facts.frames(), 480);
    assert_eq!(view.facts().frames(), facts.frames());
    assert_eq!(view.destination(), destination);
    assert_eq!(completed.destination(), destination);
    assert_eq!(completed.origin(), facts.description().origin());
    assert_eq!(completed.exported().frames, facts.frames());
    worker.join().unwrap();
    assert_eq!(fs::metadata(destination).unwrap().len(), 58 + 480 * 8);
    assert!(!cancel.is_requested());
    assert!(!cancel.request());
    assert!(cancel.request());
}
#[test]
fn all_input_adapters_preserve_identity_and_agree() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preset.toml");
    fs::write(&path, SILENCE).unwrap();
    let loaded = load_file(&path).unwrap();
    assert_eq!(loaded.description().origin(), &Origin::File(path));
    let reader = load_reader(SILENCE, origin()).unwrap();
    let bytes = load_bytes(SILENCE, origin()).unwrap();
    assert_eq!(
        reader.inspect(SampleRate::DEFAULT).unwrap().frames(),
        bytes.inspect(SampleRate::DEFAULT).unwrap().frames()
    );
}
#[test]
fn strict_input_limit_encoding_schema_and_reader_failure() {
    let mut exact = SILENCE.to_vec();
    exact.resize(65_536, b' ');
    assert!(load_bytes(&exact, origin()).is_ok());
    assert!(load_reader(exact.as_slice(), origin()).is_ok());
    exact.push(b' ');
    for failure in [
        load_bytes(&exact, origin()),
        load_reader(io::repeat(b' '), origin()),
        load_bytes(&[0xff], origin()),
        load_bytes(&[0xff, 0xfe, 0, 0], origin()),
        load_bytes(b"duration_seconds=1\nfade_seconds=0", origin()),
        load_bytes(b"schema_version=1", origin()),
        load_bytes(
            b"schema_version=2\nduration_seconds=1\nfade_seconds=0\nextra=1",
            origin(),
        ),
        load_bytes(b"schema_version=2\nschema_version=2", origin()),
    ] {
        let e = failure.unwrap_err();
        assert_eq!(e.class(), ErrorClass::InvalidInput);
        assert_eq!(e.reason(), Reason::Preset);
    }
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("reader evidence"))
        }
    }
    let e = load_reader(Broken, origin()).unwrap_err();
    assert_eq!(e.class(), ErrorClass::Operational);
    assert_eq!(e.reason(), Reason::Read);
    assert!(
        std::error::Error::source(&e)
            .unwrap()
            .to_string()
            .contains("reader evidence")
    );
}
#[test]
fn description_does_not_certify_invalid_numeric_values() {
    let p = load_bytes(
        b"schema_version=2\nduration_seconds=-1\nfade_seconds=0\n[about]\ntitle='describable'",
        origin(),
    )
    .unwrap();
    assert_eq!(p.description().about().unwrap().title, "describable");
    for result in [
        p.inspect(SampleRate::DEFAULT).map(|_| ()),
        p.prepare_export(SampleRate::DEFAULT, Path::new("invalid.wav"))
            .map(|_| ()),
    ] {
        let e = result.unwrap_err();
        assert_eq!(e.reason(), Reason::Validation);
        assert_eq!(e.class(), ErrorClass::InvalidInput);
    }
}
#[test]
fn catalog_source_description_save_and_no_clobber_use_one_path() {
    let dir = tempfile::tempdir().unwrap();
    let records = catalog().unwrap();
    assert!(!records.is_empty());
    for description in records {
        let Origin::BuiltIn(name) = description.origin() else {
            panic!("catalog origin must be built-in");
        };
        assert!(description.about().is_some());
        assert!(description.duration_seconds() > 0.0);
        let source = catalog_source(name).unwrap();
        let path = dir.path().join(name);
        let saved = save_recipe(name, &path, &Cancellation::default()).unwrap();
        assert_eq!(saved.origin(), description.origin());
        assert_eq!(saved.destination(), path);
        assert_eq!(fs::read(&path).unwrap(), source.as_bytes());
        assert!(save_recipe(name, &path, &Cancellation::default()).is_err());
    }
    assert_eq!(
        load_catalog("missing").unwrap_err().reason(),
        Reason::UnknownCatalog
    );
}
#[test]
fn exported_header_matches_prepared_facts_and_native_paths() {
    let dir = tempfile::tempdir().unwrap();
    let loaded = load_bytes(SILENCE, origin()).unwrap();
    for hz in [8_000, 44_100, 48_000, 768_000] {
        let rate = SampleRate::new(hz).unwrap();
        let path = dir.path().join(format!("{hz}.wav"));
        let prepared = loaded.prepare_export(rate, &path).unwrap();
        let frames = prepared.view().facts().frames();
        assert_eq!(prepared.view().destination(), path);
        assert!(!path.exists());
        let report = prepared.execute(&Cancellation::default()).unwrap();
        assert_eq!(report.exported().frames, frames);
        assert_eq!(report.exported().sample_rate, rate);
        assert_eq!(report.destination(), path);
        assert_eq!(report.origin(), loaded.description().origin());
        let bytes = fs::read(path).unwrap();
        assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), hz);
        assert_eq!(&bytes[20..24], &[3, 0, 2, 0]);
        assert_eq!(bytes.len() as u64, 58 + 8 * frames);
        assert!(bytes[58..].iter().all(|b| *b == 0));
    }
}
#[test]
fn cancellation_before_start_and_invalid_parent_leave_no_output() {
    let dir = tempfile::tempdir().unwrap();
    let loaded = load_bytes(SILENCE, origin()).unwrap();
    let cancel = Cancellation::default();
    cancel.request();
    let path = dir.path().join("cancelled");
    let e = loaded
        .prepare_export(SampleRate::DEFAULT, &path)
        .unwrap()
        .execute(&cancel)
        .unwrap_err();
    assert_eq!(e.class(), ErrorClass::Cancelled);
    assert!(!path.exists());
    let e = loaded
        .prepare_export(SampleRate::DEFAULT, &dir.path().join("missing/out"))
        .unwrap()
        .execute(&Cancellation::default())
        .unwrap_err();
    assert_eq!(e.class(), ErrorClass::Operational);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}
#[cfg(unix)]
#[test]
fn non_utf8_paths_remain_native() {
    use std::os::unix::ffi::OsStrExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir
        .path()
        .join(Path::new(std::ffi::OsStr::from_bytes(b"preset-\xff.toml")));
    if let Err(error) = fs::write(&path, SILENCE) {
        assert_eq!(
            error.raw_os_error(),
            Some(92),
            "unexpected native path failure: {error}"
        );
        let failure = load_file(&path).unwrap_err();
        assert_eq!(failure.origin(), Some(&Origin::File(path)));
        return;
    }
    let loaded = load_file(&path).unwrap();
    assert_eq!(loaded.description().origin(), &Origin::File(path));
    let destination = dir
        .path()
        .join(Path::new(std::ffi::OsStr::from_bytes(b"output-\xfe.wav")));
    loaded
        .prepare_export(SampleRate::DEFAULT, &destination)
        .unwrap()
        .execute(&Cancellation::default())
        .unwrap();
    assert!(destination.is_file());
}

#[test]
fn publication_errors_preserve_catalog_origin_destination_and_native_io_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("existing");
    fs::write(&path, b"untouched").unwrap();
    let descriptions = catalog().unwrap();
    let Origin::BuiltIn(name) = descriptions[0].origin() else {
        panic!("catalog identity");
    };
    let error = save_recipe(name, &path, &Cancellation::default()).unwrap_err();
    assert_eq!(error.operation(), Operation::Save);
    assert_eq!(error.reason(), Reason::DestinationExists);
    assert_eq!(error.origin(), Some(&Origin::BuiltIn(name.into())));
    assert_eq!(error.destination(), Some(path.as_path()));
    let source = std::error::Error::source(&error)
        .unwrap()
        .downcast_ref::<io::Error>()
        .unwrap();
    assert_eq!(source.kind(), io::ErrorKind::AlreadyExists);
    assert!(source.raw_os_error().is_some());
    assert_eq!(fs::read(path).unwrap(), b"untouched");
}

#[test]
fn descriptions_report_canonical_assignments_and_device_free_listening_policy() {
    for (ears, routes, headphones) in [
        (vec![], vec![], false),
        (vec!["both"], vec![Ears::Both], false),
        (vec!["left"], vec![Ears::Left], true),
        (vec!["right"], vec![Ears::Right], true),
        (
            vec!["right", "both", "left", "right"],
            vec![Ears::Both, Ears::Left, Ears::Right],
            true,
        ),
        // Equal frequencies separately assigned remain separate routing.
        (vec!["right", "left"], vec![Ears::Left, Ears::Right], true),
    ] {
        let mut source = String::from_utf8(SILENCE.to_vec()).unwrap();
        for ears in ears {
            source.push_str(&format!(
                "[[component]]\nkind='tone'\nears='{ears}'\nfrequency_hz=100\npeak_dbfs=-30\n"
            ));
        }
        let loaded = load_bytes(source.as_bytes(), origin()).unwrap();
        let description = loaded.description();
        assert_eq!(description.origin(), &origin());
        assert!(description.about().is_none());
        assert_eq!(description.duration_seconds(), 0.01);
        assert_eq!(description.fade_seconds(), 0.0);
        assert_eq!(description.routes(), routes);
        assert_eq!(
            description.notices(),
            if headphones {
                vec![Notice::LowStartingVolume, Notice::HeadphonesForSeparateEars]
            } else {
                vec![Notice::LowStartingVolume]
            }
        );
    }
}
#[test]
fn invalid_nonfinite_and_extreme_declarations_stay_describable() {
    for value in ["-1", "nan", "inf", "-inf", "1e300", "1e-300"] {
        let source = format!(
            "schema_version=2\nduration_seconds={value}\nfade_seconds=0\n[about]\ntitle='declared context'\n"
        );
        let loaded = load_bytes(source.as_bytes(), origin()).unwrap();
        let description = loaded.description();
        let declared = value.parse::<f64>().unwrap();
        if declared.is_nan() {
            assert!(description.duration_seconds().is_nan());
        } else {
            assert_eq!(description.duration_seconds(), declared);
        }
        assert_eq!(description.about().unwrap().title, "declared context");
        assert_eq!(description.notices(), [Notice::LowStartingVolume]);
        assert_eq!(
            loaded.inspect(SampleRate::DEFAULT).unwrap_err().class(),
            ErrorClass::InvalidInput
        );
    }
}
#[test]
fn rounded_session_facts_retain_exact_declarations_and_match_export() {
    let loaded = load_bytes(
        b"schema_version=2\nduration_seconds=1.00001\nfade_seconds=0.00001",
        origin(),
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    for hz in [8000, 48000] {
        let rate = SampleRate::new(hz).unwrap();
        let inspected = loaded.inspect(rate).unwrap();
        assert_eq!(inspected.description().duration_seconds(), 1.00001);
        assert_eq!(inspected.description().fade_seconds(), 0.00001);
        assert_eq!(inspected.frames(), u64::from(hz));
        assert_eq!(inspected.duration_seconds(), 1.0);
        assert_eq!(inspected.fade_frames(), 0);
        assert_eq!(inspected.fade_seconds(), 0.0);
        let path = dir.path().join(hz.to_string());
        let prepared = loaded.prepare_export(rate, &path).unwrap();
        assert_eq!(prepared.view().facts().frames(), inspected.frames());
        let exported = prepared.execute(&Cancellation::default()).unwrap();
        assert_eq!(exported.exported().frames, inspected.frames());
        assert_eq!(
            fs::metadata(path).unwrap().len(),
            58 + 8 * inspected.frames()
        );
    }
}
#[test]
fn export_binds_exact_requested_path_before_execution_and_reports_its_own_context() {
    let dir = tempfile::tempdir().unwrap();
    let loaded = load_bytes(SILENCE, origin()).unwrap();
    let original = dir.path().join("./bound.wav");
    let mut request = original.clone();
    let prepared = loaded
        .prepare_export(SampleRate::DEFAULT, &request)
        .unwrap();
    let view = prepared.view().clone();
    assert_eq!(view.destination().as_os_str(), original.as_os_str());
    assert_eq!(view.facts().description().origin(), &origin());
    assert!(!original.exists());
    request.set_file_name("other.wav");
    let report = prepared.execute(&Cancellation::default()).unwrap();
    assert!(original.is_file());
    assert!(!request.exists());
    assert_eq!(report.destination().as_os_str(), original.as_os_str());
    assert_eq!(report.origin(), &origin());
    assert_eq!(report.exported().frames, view.facts().frames());
}

#[test]
fn unknown_catalog_diagnostic_quotes_inline_controls_without_changing_typed_identity_or_source() {
    let name = "unknown\nforged\tlabel";
    let error = load_catalog(name).unwrap_err();
    assert_eq!(error.origin(), Some(&Origin::BuiltIn(name.into())));
    assert_eq!(error.reason(), Reason::UnknownCatalog);
    assert_eq!(error.class(), ErrorClass::InvalidInput);
    let rendered = error.to_string();
    assert!(!rendered.contains('\n'));
    assert!(!rendered.contains('\t'));
    assert!(rendered.starts_with("\"unknown\\nforged\\tlabel\" (built-in): "));
    let source = std::error::Error::source(&error).unwrap();
    assert_eq!(
        source.to_string(),
        format!("no built-in preset is named {name:?}")
    );
}
