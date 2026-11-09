# Native graphical qualification

These scripts drive the production debug GUI through OS accessibility providers and native keyboard input. They do not call application callbacks or open audio. Each process uses an isolated library and `AUDITORYLAB_NATIVE_QA_DIRECTORY`; that private debug-only seam isolates onboarding configuration and is absent from distributed builds. Initialize the isolated library with the ordinary CLI before launching the harness.

Linux requires a private D-Bus session, AT-SPI registry, GTK desktop portal, X11 display, `xdotool`, Openbox and system Python with `pyatspi` and Pillow. The Wayland case additionally starts a real Weston compositor with its GL renderer, forces the application onto its Wayland socket and checks actual Wayland surface protocol. The outer X11 display supplies Weston input and output; it is not the Wayland implementation. Run separate cases:

```sh
scratch="$PWD/target/native-x11"
mkdir -p "$scratch"
target/debug/auditorylab --library-dir "$scratch/library" library init
timeout 240s dbus-run-session -- xvfb-run --auto-servernum --server-args='-screen 0 1920x1200x24 +extension GLX' bash crates/gui/native/linux-session.sh x11 "$PWD/target/debug/auditorylab-gui" "$PWD/crates/core/tests/fixtures/recipes/custom-derived-pair.toml" "$scratch"
```

Use `wayland` and a distinct scratch directory for the second case. The harness verifies provider operation names, first-frame geometry/pixels, multiline native input/modifier selection, repeated native chooser cancellation/reopening, actual file opening and nonempty protocol-specific portal parent handles. The Wayland case opens a native dialog before and after a debug-only one-shot hide/show of the production main window. It checks rendered-frame traces, old Wayland surface destruction followed by a new toplevel, fresh provider operation names and a fresh exported portal parent. The seam runs once after an observed native GL frame and a private parent-confirmation marker written by the OS harness after its first real chooser is cancelled. The debug-only gate expires after thirty seconds and supports only this fixed hide/show witness.

Windows requires an interactive desktop, UI Automation and native keyboard access. In PowerShell, invoke `windows.ps1` with `-Binary`, `-Fixture` and `-Scratch` absolute paths. It additionally checks the actual Win32 owner of each common file dialog. Missing sessions/providers/controls fail; there is no offscreen fallback or skip. Successful JSON output reports only the cases actually exercised.

Native waits have deadlines and CI must also bound the outer process to five minutes. Failures retain bounded provider-tree text and first-frame image evidence under the supplied scratch directory; portal/protocol logs stay there for diagnosis. Remove passing scratch after retaining the requested evidence ledger.

These scripts are prepared qualification inputs. Syntax checks do not qualify their native behavior. CI must execute them on the delivered source. macOS AppKit repeated Quit Save/Discard/Cancel requires its own observed cases in the final acceptance ledger.
