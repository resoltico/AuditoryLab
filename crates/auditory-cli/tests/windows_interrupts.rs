// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Native Windows console events, tested in an isolated console-owning harness.
#![cfg(windows)]
use std::process::Command;

#[test]
fn console_ctrl_c_and_ctrl_break_cancel_silent_export_and_pre_input_wait() {
    // ctypes is confined to a child Python process. Every child has a bounded
    // wait and is killed/reaped even if event delivery or an assertion fails.
    let script = r#"
import ctypes, pathlib, subprocess, sys, tempfile, time
from ctypes import wintypes
kernel = ctypes.WinDLL('kernel32', use_last_error=True)
HANDLER = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.DWORD)
handler = HANDLER(lambda event: True)
kernel.SetConsoleCtrlHandler.argtypes = [HANDLER, wintypes.BOOL]
kernel.GenerateConsoleCtrlEvent.argtypes = [wintypes.DWORD, wintypes.DWORD]
def checked(ok):
    if not ok: raise ctypes.WinError(ctypes.get_last_error())
for event in (0, 1):
    for pre_input in (False, True):
        with tempfile.TemporaryDirectory() as directory:
            directory = pathlib.Path(directory)
            outputs = directory / 'outputs'; outputs.mkdir()
            output = outputs / 'silent.wav'
            preset = directory / 'silence.toml'
            preset.write_text('schema_version=2\nduration_seconds=600\nfade_seconds=0\n', encoding='utf-8')
            process = subprocess.Popen([sys.argv[1], 'export', *(['--stdin'] if pre_input else ['--file', str(preset)]), '--output', str(output)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, creationflags=subprocess.CREATE_NEW_CONSOLE)
            try:
                kernel.FreeConsole()
                deadline = time.monotonic() + 20
                while not kernel.AttachConsole(process.pid):
                    if time.monotonic() >= deadline: raise AssertionError('child console unavailable')
                    if process.poll() is not None: raise AssertionError('child exited before console attach')
                    time.sleep(0.005)
                checked(kernel.SetConsoleCtrlHandler(handler, True))
                if pre_input:
                    process.stdin.write(b'schema_version=2\n'); process.stdin.flush()
                    time.sleep(0.2)
                    assert process.poll() is None
                else:
                    while not list(outputs.iterdir()):
                        assert time.monotonic() < deadline, 'no stage activity before deadline'
                        assert process.poll() is None, 'export exited before staging'
                        time.sleep(0.005)
                    assert not output.exists(), 'final destination appeared before commit'
                checked(kernel.GenerateConsoleCtrlEvent(event, 0))
                if pre_input:
                    # Do not close stdin: an EOF/parser failure must not satisfy this signal oracle.
                    status = process.wait(timeout=20)
                    assert status & 0xffffffff == 0xC000013A, status
                stdout, stderr = process.communicate(timeout=20)
                if not pre_input:
                    assert process.returncode == 130, (process.returncode, stderr)
                    assert b'Export cancelled:' in stderr, stderr
                    assert not stdout, stdout
                    assert not list(outputs.iterdir()), 'cooperative cancellation left a stage'
                assert not output.exists()
            finally:
                if process.poll() is None: process.kill()
                process.wait(timeout=20)
                kernel.FreeConsole()
"#;
    let result = Command::new("python")
        .args(["-c", script, env!("CARGO_BIN_EXE_auditorylab")])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}
