// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Real document admission, with bounded subprocesses for potentially blocking inputs.
use auditorylab::{
    Diagnostic, ErrorClass, InputDiagnostic, Origin, Reason, SampleRate, load_file, load_reader,
};
use std::{
    fs,
    io::Read,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
const TEXT: &[u8] = b"schema_version=2\nduration_seconds=0.01\nfade_seconds=0\n";

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn child_output(child: &mut OwnedChild) -> String {
    let mut output = Vec::new();
    if let Some(mut stdout) = child.0.stdout.take() {
        stdout
            .read_to_end(&mut output)
            .unwrap_or_else(|error| panic!("child stdout: {error}"));
    }
    if let Some(mut stderr) = child.0.stderr.take() {
        stderr
            .read_to_end(&mut output)
            .unwrap_or_else(|error| panic!("child stderr: {error}"));
    }
    String::from_utf8_lossy(&output).into_owned()
}
fn rejected_without_wait(path: &Path, typed: bool) {
    let mut child = OwnedChild(
        Command::new(
            std::env::current_exe().unwrap_or_else(|error| panic!("test executable: {error}")),
        )
        .args(["--exact", "document_admission_child", "--nocapture"])
        .env("AUDITORY_DOCUMENT_CHILD_PATH", path)
        .env(
            "AUDITORY_DOCUMENT_CHILD_TYPED",
            if typed { "1" } else { "0" },
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("admission child spawn: {error}")),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child
            .0
            .try_wait()
            .unwrap_or_else(|error| panic!("admission child wait: {error}"))
        {
            let output = child_output(&mut child);
            assert!(
                status.success(),
                "admission child failed: {path:?}: {status}\n{output}"
            );
            return;
        }
        if Instant::now() >= deadline {
            let _ = child.0.kill();
            let _ = child.0.wait();
            panic!(
                "document admission blocked: {path:?}\n{}",
                child_output(&mut child)
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn document_admission_child() {
    let Some(path) = std::env::var_os("AUDITORY_DOCUMENT_CHILD_PATH") else {
        return;
    };
    let error = load_file(&path).unwrap_err();
    assert_eq!(error.reason(), Reason::Read);
    assert_eq!(error.origin(), Some(&Origin::File(path.into())));
    if std::env::var_os("AUDITORY_DOCUMENT_CHILD_TYPED").unwrap() == "1" {
        assert!(matches!(
            error.diagnostic(),
            Diagnostic::Input(InputDiagnostic::NotRegularFile)
        ));
        assert_eq!(error.class(), ErrorClass::InvalidInput);
        assert!(error.to_string().contains("regular file"));
        assert!(error.to_string().contains("--stdin"));
    }
}
#[test]
fn regular_documents_and_explicit_streams_retain_limits_and_identity() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("recipe.toml");
    fs::write(&path, TEXT).unwrap();
    let loaded = load_file(&path).unwrap();
    assert_eq!(loaded.description().origin(), &Origin::File(path.clone()));
    assert_eq!(loaded.inspect(SampleRate::DEFAULT).unwrap().frames(), 480);
    #[cfg(unix)]
    {
        let link = dir.path().join("linked.toml");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert_eq!(
            load_file(&link).unwrap().description().origin(),
            &Origin::File(link)
        );
    }
    let mut exact = TEXT.to_vec();
    exact.resize(65536, b' ');
    fs::write(&path, &exact).unwrap();
    assert!(load_file(&path).is_ok());
    exact.push(b' ');
    fs::write(&path, &exact).unwrap();
    assert!(matches!(
        load_file(&path).unwrap_err().diagnostic(),
        Diagnostic::Input(InputDiagnostic::TooLarge {
            limit: 65536,
            observed_bytes: 65537
        })
    ));
    fs::write(&path, [0xff]).unwrap();
    assert!(matches!(
        load_file(&path).unwrap_err().diagnostic(),
        Diagnostic::Input(InputDiagnostic::InvalidUtf8)
    ));
    assert!(load_reader(TEXT, Origin::Reader("intentional finite stream".into())).is_ok());
    rejected_without_wait(dir.path(), cfg!(unix));
}
#[cfg(unix)]
#[test]
fn fifo_symlink_and_devices_reject_before_read_without_a_writer() {
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("recipe.pipe");
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let link = dir.path().join("pipe-link");
    std::os::unix::fs::symlink(&fifo, &link).unwrap();
    for path in [&fifo, &link, Path::new("/dev/null"), Path::new("/dev/zero")] {
        rejected_without_wait(path, true);
    }
}

#[cfg(windows)]
fn wait_for_pipe_marker(server: &mut OwnedChild, marker: &Path, phase: &str, deadline: Instant) {
    while !marker.exists() {
        if let Some(status) = server
            .0
            .try_wait()
            .unwrap_or_else(|error| panic!("pipe server wait: {error}"))
        {
            panic!(
                "pipe server exited during {phase}: {status}\n{}",
                child_output(server)
            );
        }
        if Instant::now() >= deadline {
            let _ = server.0.kill();
            let _ = server.0.wait();
            panic!(
                "pipe server deadline during {phase}\n{}",
                child_output(server)
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(windows)]
#[test]
fn connected_owned_pipe_sending_no_bytes_and_native_devices_reject() {
    let dir = tempfile::tempdir().unwrap();
    let name = format!("auditory-document-{}", std::process::id());
    let ready = dir.path().join("ready");
    let connected = dir.path().join("connected");
    // The server owns only this unique pipe and deliberately sends no bytes. If
    // admission accidentally reads it, the bounded child check fails and reaps it.
    // Post the connection wait before advertising readiness, so even a client
    // that rejects and closes immediately cannot outrun the server's wait.
    let script = r#"
$ErrorActionPreference='Stop'
[Console]::Error.WriteLine('pipe server: constructing')
$p=[System.IO.Pipes.NamedPipeServerStream]::new($env:AUDITORY_PIPE_NAME,[System.IO.Pipes.PipeDirection]::Out,1,[System.IO.Pipes.PipeTransmissionMode]::Byte,[System.IO.Pipes.PipeOptions]::Asynchronous)
try {
    $waiting=$p.WaitForConnectionAsync()
    [Console]::Error.WriteLine('pipe server: connection wait posted')
    [IO.File]::WriteAllText($env:AUDITORY_PIPE_READY,'ready')
    $waiting.GetAwaiter().GetResult()
    [Console]::Error.WriteLine('pipe server: connected')
    [IO.File]::WriteAllText($env:AUDITORY_PIPE_CONNECTED,'connected')
    Start-Sleep -Seconds 60
} finally {
    $p.Dispose()
}
"#;
    let mut server = OwnedChild(
        Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("AUDITORY_PIPE_NAME", &name)
            .env("AUDITORY_PIPE_READY", &ready)
            .env("AUDITORY_PIPE_CONNECTED", &connected)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|error| panic!("admission child spawn: {error}")),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    wait_for_pipe_marker(&mut server, &ready, "Ready", deadline);
    rejected_without_wait(Path::new(&format!(r"\\.\pipe\{name}")), true);
    wait_for_pipe_marker(&mut server, &connected, "Connected", deadline);
    for device in [r"\\.\NUL", r"\\.\CONIN$"] {
        rejected_without_wait(Path::new(device), false);
    }
}
