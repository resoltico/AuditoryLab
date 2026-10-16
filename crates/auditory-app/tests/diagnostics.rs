// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Structured facade diagnostics, independent of parser/native implementation crates.
use auditory_app::*;
use std::{
    fs,
    io::{self, Read},
    path::Path,
};
fn origin() -> Origin {
    Origin::Reader("diagnostic consumer".into())
}
fn failure(text: &str) -> AppError {
    let loaded = load_bytes(text.as_bytes(), origin())
        .unwrap_or_else(|error| panic!("invalid structured-diagnostic fixture: {error}"));
    match loaded.inspect(SampleRate::DEFAULT) {
        Err(error) => error,
        Ok(_) => panic!("fixture unexpectedly validated"),
    }
}
#[test]
fn input_categories_carry_limits_schema_and_original_native_queries() {
    let error = load_bytes(&vec![b' '; 65537], origin()).unwrap_err();
    assert!(matches!(
        error.diagnostic(),
        Diagnostic::Input(InputDiagnostic::TooLarge {
            limit: 65536,
            observed_bytes: 65537
        })
    ));
    for (bytes, utf16) in [(&[0xff, 0xfe][..], true), (&[0xff][..], false)] {
        let error = load_bytes(bytes, origin()).unwrap_err();
        assert!(matches!(
            (error.diagnostic(), utf16),
            (Diagnostic::Input(InputDiagnostic::Utf16), true)
                | (Diagnostic::Input(InputDiagnostic::InvalidUtf8), false)
        ));
    }
    let error = load_bytes(b"schema_version=1", origin()).unwrap_err();
    assert!(matches!(
        error.diagnostic(),
        Diagnostic::Input(InputDiagnostic::UnsupportedSchema {
            found: 1,
            supported: 2
        })
    ));
    let query = std::ffi::OsStr::new("unknown\nquery\tΩ");
    let error = load_catalog(query).unwrap_err();
    assert!(
        matches!(error.diagnostic(),Diagnostic::Input(InputDiagnostic::UnknownCatalog{query:original}) if original==query)
    );
    assert_eq!(error.class(), ErrorClass::InvalidInput);
}
#[cfg(unix)]
#[test]
fn unknown_catalog_retains_non_unicode_query_without_lossy_reconstruction() {
    use std::os::unix::ffi::OsStrExt;
    let query = std::ffi::OsStr::from_bytes(b"unknown-\xff");
    let error = load_catalog(query).unwrap_err();
    assert!(
        error.origin().is_none(),
        "a non-Unicode query must not fabricate a lossy built-in identity"
    );
    assert!(
        matches!(error.diagnostic(),Diagnostic::Input(InputDiagnostic::UnknownCatalog{query:original}) if original==query)
    );
}
#[test]
fn format_spans_refer_to_the_exact_multibyte_snapshot_after_file_changes() {
    let source = "schema_version=2\nduration_seconds=1\nfade_seconds=0\n[about]\ntitle='Ω hostile\tvalue'\nunknown=true\n";
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("mutable.toml");
    fs::write(&path, source).unwrap();
    let error = load_file(&path).unwrap_err();
    fs::write(
        &path,
        "schema_version=2\nduration_seconds=0.01\nfade_seconds=0",
    )
    .unwrap();
    let Diagnostic::Input(InputDiagnostic::Format(detail)) = error.diagnostic() else {
        panic!("format diagnostic expected")
    };
    assert_eq!(detail.input(), source);
    assert!(!detail.message().is_empty());
    assert!(
        detail.span().is_some(),
        "this parser fixture must retain an actual source span"
    );
    if let Some(span) = detail.span() {
        assert!(span.end <= source.len());
        assert_eq!(detail.snippet(), source.get(span));
    } else {
        assert!(detail.snippet().is_none());
    }
    assert_eq!(error.origin(), Some(&Origin::File(path)));
    assert!(std::error::Error::source(&error).is_some());
}
#[test]
fn filesystem_input_and_create_commit_phases_keep_native_request_context() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("missing.toml");
    let error = load_file(&missing).unwrap_err();
    assert!(
        matches!(error.diagnostic(),Diagnostic::Filesystem(detail) if detail.phase()==FilesystemPhase::OpenInput&&detail.kind()==io::ErrorKind::NotFound&&detail.requested_path()==Some(missing.as_path())&&detail.stage().is_none())
    );
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "original read evidence",
            ))
        }
    }
    let error = load_reader(Broken, origin()).unwrap_err();
    assert!(
        matches!(error.diagnostic(),Diagnostic::Filesystem(detail) if detail.phase()==FilesystemPhase::ReadInput&&detail.kind()==io::ErrorKind::PermissionDenied&&detail.requested_path().is_none())
    );
    let loaded = load_bytes(
        b"schema_version=2\nduration_seconds=0.01\nfade_seconds=0",
        origin(),
    )
    .unwrap();
    let path = directory.path().join("absent/output.wav");
    let error = loaded
        .prepare_export(SampleRate::DEFAULT, &path)
        .unwrap()
        .execute(&Cancellation::default())
        .unwrap_err();
    assert!(
        matches!(error.diagnostic(),Diagnostic::Filesystem(detail) if detail.phase()==FilesystemPhase::CreateStage&&detail.requested_path()==Some(path.as_path())&&detail.stage().is_none())
    );
    let path = directory.path().join("existing.wav");
    fs::write(&path, b"untouched").unwrap();
    let error = loaded
        .prepare_export(SampleRate::DEFAULT, &path)
        .unwrap()
        .execute(&Cancellation::default())
        .unwrap_err();
    assert!(
        matches!(error.diagnostic(),Diagnostic::Filesystem(detail) if detail.phase()==FilesystemPhase::Commit&&detail.kind()==io::ErrorKind::AlreadyExists&&detail.stage().is_some())
    );
    assert_eq!(error.reason(), Reason::DestinationExists);
    assert_eq!(error.destination(), Some(path.as_path()));
    assert_eq!(fs::read(path).unwrap(), b"untouched");
}
fn component(extra: &str) -> String {
    format!(
        "schema_version=2\nduration_seconds=1\nfade_seconds=0\n[[component]]\nears='both'\npeak_dbfs=-40\n{extra}"
    )
}
#[test]
fn validation_retains_each_parameter_category_and_original_rejected_facts() {
    for (extra, parameter) in [
        ("kind='tone'\nfrequency_hz=-1", Parameter::Frequency),
        (
            "kind='click'\nrate_hz=-1\nwidth_seconds=0.01",
            Parameter::Rate,
        ),
        (
            "kind='click'\nrate_hz=10\nwidth_seconds=0",
            Parameter::Width,
        ),
        (
            "kind='tone'\nfrequency_hz=100\n[component.modulation]\nkind='sinusoidal'\nrate_hz=-1\ndepth=1",
            Parameter::ModulationRate,
        ),
        (
            "kind='tone'\nfrequency_hz=100\n[component.modulation]\nkind='rectangular'\nrate_hz=10\nwidth_seconds=0",
            Parameter::ModulationWidth,
        ),
    ] {
        let error = failure(&component(extra));
        let Diagnostic::Validation(StimulusError::Component {
            number: 1,
            error: detail,
        }) = error.diagnostic()
        else {
            panic!("component diagnostic: {error:?}")
        };
        assert!(
            matches!(detail,ComponentError::Frequency{parameter:actual,sample_rate_hz:48000,..}|ComponentError::PulseWidth{parameter:actual,sample_rate_hz:48000,..} if *actual==parameter)
        );
        assert_eq!(error.operation(), Operation::Inspect);
        assert_eq!(error.class(), ErrorClass::InvalidInput);
        assert!(std::error::Error::source(&error).is_some());
    }
    let error = failure(&component("kind='tone'\nfrequency_hz=1e-320"));
    assert!(matches!(
        error.diagnostic(),
        Diagnostic::Validation(StimulusError::Component {
            error: ComponentError::PhaseStepTooSmall {
                parameter: Parameter::Frequency,
                ..
            },
            ..
        })
    ));
    let error = failure(&component(
        "kind='click'\nrate_hz=1e-320\nwidth_seconds=0.1",
    ));
    assert!(matches!(
        error.diagnostic(),
        Diagnostic::Validation(StimulusError::Component {
            error: ComponentError::PulseRateTooSmall {
                parameter: Parameter::Rate,
                ..
            },
            ..
        })
    ));
    let error = failure(&component(
        "kind='tone'\nfrequency_hz=20000\n[component.modulation]\nkind='sinusoidal'\nrate_hz=5000\ndepth=1",
    ));
    assert!(matches!(
        error.diagnostic(),
        Diagnostic::Validation(StimulusError::Component {
            error: ComponentError::Sideband {
                frequency_hz: 20000.0,
                rate_hz: 5000.0,
                sample_rate_hz: 48000
            },
            ..
        })
    ));
    let error = failure(&component(
        "kind='tone'\nfrequency_hz=100\n[component.modulation]\nkind='sinusoidal'\nrate_hz=5\ndepth=nan",
    ));
    assert!(
        matches!(error.diagnostic(),Diagnostic::Validation(StimulusError::Component{error:ComponentError::Depth{depth},..}) if depth.is_nan())
    );
    let source =
        component("kind='tone'\nfrequency_hz=100").replace("peak_dbfs=-40", "peak_dbfs=inf");
    let error = failure(&source);
    assert!(
        matches!(error.diagnostic(),Diagnostic::Validation(StimulusError::Component{error:ComponentError::PeakDbfs{peak_dbfs},..}) if peak_dbfs.is_infinite())
    );
}
#[test]
fn validation_covers_duration_fade_overlap_component_limits_and_both_ear_peaks() {
    let error = failure("schema_version=2\nduration_seconds=nan\nfade_seconds=0");
    assert!(
        matches!(error.diagnostic(),Diagnostic::Validation(StimulusError::Duration{seconds,sample_rate_hz:48000}) if seconds.is_nan())
    );
    let error = failure("schema_version=2\nduration_seconds=1\nfade_seconds=inf");
    assert!(
        matches!(error.diagnostic(),Diagnostic::Validation(StimulusError::Fade{seconds,..}) if seconds.is_infinite())
    );
    let error = failure("schema_version=2\nduration_seconds=1\nfade_seconds=0.5");
    assert!(matches!(
        error.diagnostic(),
        Diagnostic::Validation(StimulusError::FadesOverlap {
            fade_frames: 24000,
            frames: 48000
        })
    ));
    let mut many = String::from("schema_version=2\nduration_seconds=1\nfade_seconds=0\n");
    for _ in 0..17 {
        many.push_str("[[component]]\nkind='tone'\nears='both'\nfrequency_hz=100\npeak_dbfs=-40\n");
    }
    assert!(matches!(
        failure(&many).diagnostic(),
        Diagnostic::Validation(StimulusError::TooManyComponents { count: 17 })
    ));
    for (route, ear) in [("left", Ear::Left), ("right", Ear::Right)] {
        let mut source = String::from("schema_version=2\nduration_seconds=1\nfade_seconds=0\n");
        for _ in 0..2 {
            source.push_str(&format!(
                "[[component]]\nkind='tone'\nears='{route}'\nfrequency_hz=100\npeak_dbfs=0\n"
            ));
        }
        assert!(
            matches!(failure(&source).diagnostic(),Diagnostic::Validation(StimulusError::Peak{ear:actual,amplitude:2.0}) if *actual==ear)
        );
    }
}
#[test]
fn transferable_diagnostics_do_not_need_native_backend_or_parser_types() {
    fn shared<T: Send + Sync + 'static>() {}
    shared::<AppError>();
    shared::<Diagnostic>();
    shared::<FormatDiagnostic>();
    shared::<DeviceDiagnostic>();
    shared::<FilesystemDiagnostic>();
    shared::<CleanupFailure>();
    let error = load_file(Path::new("definitely-missing-input.toml")).unwrap_err();
    let thread = std::thread::spawn(move || error);
    assert_eq!(thread.join().unwrap().reason(), Reason::Read);
}
