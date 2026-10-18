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
null_handler = HANDLER()
for name in ('FreeConsole', 'AllocConsole'):
    getattr(kernel, name).argtypes = []
    getattr(kernel, name).restype = wintypes.BOOL
kernel.SetConsoleCtrlHandler.argtypes = [HANDLER, wintypes.BOOL]
kernel.SetConsoleCtrlHandler.restype = wintypes.BOOL
kernel.GenerateConsoleCtrlEvent.argtypes = [wintypes.DWORD, wintypes.DWORD]
kernel.GenerateConsoleCtrlEvent.restype = wintypes.BOOL
console_owned = False
phase = 'initialization'
def enter(name):
    global phase
    phase = name
    print('console integration: ' + name, flush=True)
def checked(ok):
    if not ok: raise ctypes.WinError(ctypes.get_last_error())
try:
    enter('allocating private console')
    kernel.FreeConsole()
    checked(kernel.AllocConsole())
    console_owned = True
    enter('normalizing inherited Ctrl+C processing')
    checked(kernel.SetConsoleCtrlHandler(null_handler, True))
    checked(kernel.SetConsoleCtrlHandler(null_handler, False))
    checked(kernel.SetConsoleCtrlHandler(handler, True))
    for event in (0, 1):
        for pre_input in (False, True):
            with tempfile.TemporaryDirectory() as directory:
                directory = pathlib.Path(directory)
                outputs = directory / 'outputs'; outputs.mkdir()
                output = outputs / 'silent.wav'
                preset = directory / 'silence.toml'
                preset.write_text('schema_version=2\nduration_seconds=600\nfade_seconds=0\n', encoding='utf-8')
                process = None
                case = 'event=' + str(event) + ' pre_input=' + str(pre_input)
                try:
                    enter(case + ' spawning in the private console')
                    process = subprocess.Popen([sys.argv[1], 'export', *(['--stdin'] if pre_input else ['--file', str(preset)]), '--output', str(output)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                    deadline = time.monotonic() + 20
                    if pre_input:
                        enter(case + ' holding input open')
                        process.stdin.write(b'schema_version=2\n'); process.stdin.flush()
                        time.sleep(0.2)
                        assert process.poll() is None
                    else:
                        enter(case + ' waiting for stage activity')
                        while not list(outputs.iterdir()):
                            assert time.monotonic() < deadline, 'no stage activity before deadline'
                            assert process.poll() is None, 'export exited before staging'
                            time.sleep(0.005)
                        assert not output.exists(), 'final destination appeared before commit'
                    enter(case + ' sending native console event')
                    checked(kernel.GenerateConsoleCtrlEvent(event, 0))
                    if pre_input:
                        # Keep stdin open until native termination; EOF must not satisfy this oracle.
                        status = process.wait(timeout=20)
                        assert status & 0xffffffff == 0xC000013A, status
                    stdout, stderr = process.communicate(timeout=20)
                    if not pre_input:
                        assert process.returncode == 130, (process.returncode, stderr)
                        assert b'Export cancelled:' in stderr, stderr
                        assert not stdout, stdout
                        assert not list(outputs.iterdir()), 'cooperative cancellation left a stage'
                    assert not output.exists()
                    enter(case + ' verified native termination and output invariants')
                finally:
                    if process is not None:
                        if process.poll() is None: process.kill()
                        stdout, stderr = process.communicate(timeout=20)
                        print('console integration child ' + case + ': exit=' + str(process.returncode) + ' stdout=' + repr(stdout) + ' stderr=' + repr(stderr), file=sys.stderr, flush=True)
except BaseException:
    print('console integration failed during ' + phase, file=sys.stderr, flush=True)
    raise
finally:
    if console_owned and not kernel.FreeConsole():
        print('console integration cleanup: ' + str(ctypes.WinError(ctypes.get_last_error())), file=sys.stderr, flush=True)

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
