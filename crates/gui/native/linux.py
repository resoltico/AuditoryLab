#!/usr/bin/env python3
"""Production desktop qualification through AT-SPI and native compositor input.

Run inside a private D-Bus session with X11 or a real Weston Wayland session.
No application callbacks, offscreen renderer, or audio actions are used.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import signal
import time

import pyatspi
from PIL import ImageGrab


def wait_for(description, probe, seconds=30):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        result = probe()
        if result is not None and result is not False:
            return result
        time.sleep(0.1)
    raise RuntimeError("timed out: " + description)


def nodes(root):
    pending = [root]
    count = 0
    while pending:
        node = pending.pop(0)
        count += 1
        if count > 12000:
            raise RuntimeError("AT-SPI tree exceeded bounded inspection budget")
        yield node
        try:
            pending.extend(child for child in node if child is not None)
        except (RuntimeError, AttributeError):
            continue


def visible(node):
    try:
        state = node.getState()
        return state.contains(pyatspi.STATE_SHOWING) and state.contains(pyatspi.STATE_ENABLED)
    except RuntimeError:
        return False


def named(root, name):
    return next((node for node in nodes(root) if node.name == name and visible(node)), None)


def activate(node):
    action = node.queryAction()
    if action.nActions < 1 or not action.doAction(0):
        raise RuntimeError("OS-provider action refused: " + node.name)


def key(*keys):
    subprocess.run(["xdotool", "key", "--clearmodifiers", *keys], check=True, timeout=5)


def type_text(text):
    subprocess.run(["xdotool", "type", "--clearmodifiers", "--delay", "2", "--", text], check=True, timeout=10)


def app_for(pid):
    for app in pyatspi.Registry.getDesktop(0):
        try:
            if app.get_process_id() == pid:
                return app
        except RuntimeError:
            pass
    return None


def text_of(node):
    text = node.queryText()
    return text.getText(0, text.characterCount)


def main():
    signal.alarm(180)
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--fixture", required=True, type=Path)
    parser.add_argument("--session", choices=["x11", "wayland"], required=True)
    parser.add_argument("--scratch", required=True, type=Path)
    args = parser.parse_args()
    scratch = args.scratch.resolve()
    scratch.mkdir(parents=True, exist_ok=True)
    if args.session == "wayland" and not os.environ.get("WAYLAND_DISPLAY"):
        raise RuntimeError("required Wayland compositor socket is unavailable")
    if not os.environ.get("DISPLAY"):
        raise RuntimeError("required native input/display surface is unavailable")
    env = os.environ.copy()
    env.update({"AUDITORYLAB_LIBRARY_DIR": str(scratch / "library"),
                "AUDITORYLAB_NATIVE_QA_DIRECTORY": str(scratch),
                "XDG_CONFIG_HOME": str(scratch / "xdg-config"),
                "XDG_DATA_HOME": str(scratch / "xdg-data"),
                "WINIT_UNIX_BACKEND": args.session,
                "SLINT_ACCESSIBILITY": "enable"})
    if args.session == "x11":
        env.pop("WAYLAND_DISPLAY", None)
    else:
        env["WAYLAND_DEBUG"] = "client"
    # The monitor observes actual portal calls, including their OS parent handles.
    portal_path = scratch / "portal.log"
    with portal_path.open("w") as portal, (scratch / "application.log").open("w") as log:
        monitor = subprocess.Popen(["dbus-monitor", "--session", "interface='org.freedesktop.portal.FileChooser'"], stdout=portal, stderr=portal)
        process = subprocess.Popen([str(args.binary.resolve())], env=env, stdout=log, stderr=log)
        try:
            app = wait_for("production application AT-SPI provider", lambda: app_for(process.pid))
            browse = wait_for("first-frame Browse Library provider action", lambda: named(app, "Browse Library"))
            activate(browse)
            for name in ["New sound", "Open recipe", "Workbench"]:
                wait_for("operation provider name " + name, lambda name=name: named(app, name))
            # Inspect real surface geometry after first-frame provider construction.
            frames = [node for node in nodes(app) if node.getRole() == pyatspi.ROLE_FRAME]
            if not frames:
                raise RuntimeError("application exported no native top-level frame")
            extent = frames[0].queryComponent().getExtents(pyatspi.DESKTOP_COORDS)
            if extent.width <= 100 or extent.height <= 100:
                raise RuntimeError("first-frame native geometry is unusable")
            pixels = ImageGrab.grab(bbox=(extent.x, extent.y, extent.x + extent.width, extent.y + extent.height), xdisplay=os.environ["DISPLAY"])
            colors = len(set(pixels.resize((100, 100)).getdata()))
            if colors < 5:
                pixels.save(scratch / "failure-first-frame.png")
                raise RuntimeError("no rendered production first-frame pixels")
            activate(named(app, "Workbench"))
            notes = wait_for("multiline OS text provider", lambda: named(app, "Custom recipe notes"))
            if not notes.queryComponent().grabFocus():
                raise RuntimeError("multiline OS-provider focus refused")
            type_text("first native line\nsecond native line")
            wait_for("native multiline input", lambda: text_of(notes) == "first native line\nsecond native line")
            key("ctrl+a")
            wait_for("native modifier selection", lambda: notes.queryText().getNSelections() > 0 and notes.queryText().getSelection(0) == (0, len(text_of(notes))))
            # Leave no dirty-document continuation ahead of the real file chooser.
            type_text("")
            key("BackSpace")
            wait_for("cleared multiline input", lambda: text_of(notes) == "")
            activate(named(app, "Library"))
            dialogs = []
            for attempt in range(2):
                activate(wait_for("enabled Open recipe", lambda: named(app, "Open recipe")))
                desktop = pyatspi.Registry.getDesktop(0)
                cancel = wait_for("native portal file chooser Cancel", lambda: named(desktop, "Cancel"))
                # Actual toolkit file chooser must expose editable location/name UI.
                parent = cancel.parent
                while parent.parent is not None and parent.getRole() not in (pyatspi.ROLE_DIALOG, pyatspi.ROLE_FILE_CHOOSER):
                    parent = parent.parent
                if parent.getRole() not in (pyatspi.ROLE_DIALOG, pyatspi.ROLE_FILE_CHOOSER):
                    raise RuntimeError("Cancel did not belong to a native file chooser")
                dialogs.append(parent.name)
                activate(cancel)
                wait_for("native dialog cancelled and app recovered", lambda: named(app, "Open recipe"))
            # Reopen and use the real location control to open an isolated recipe.
            activate(named(app, "Open recipe"))
            wait_for("native chooser reopened", lambda: named(pyatspi.Registry.getDesktop(0), "Cancel"))
            key("ctrl+l")
            type_text(str(args.fixture.resolve()))
            key("Return")
            wait_for("native chooser accepted location", lambda: named(app, "Open recipe"))
            time.sleep(0.2)
            portal.flush()
            calls = portal_path.read_text()
            prefix = "wayland:" if args.session == "wayland" else "x11:"
            if calls.count(prefix) < 3:
                raise RuntimeError("native portal did not receive a usable " + prefix + " parent on every opening")
            if process.poll() is not None:
                raise RuntimeError("production application exited during native qualification")
            if args.session == "wayland":
                log.flush()
                protocol = (scratch / "application.log").read_text()
                for message in ["wl_surface", "xdg_wm_base"]:
                    if message not in protocol:
                        raise RuntimeError("no actual Wayland production surface evidence: " + message)
            print(json.dumps({"session": args.session, "provider": "AT-SPI", "pid": process.pid,
                              "production": "Winit/FemtoVG", "first_frame_extent": [extent.width, extent.height], "first_frame_colors": colors,
                              "multiline_native_keys": True, "modifier_selection": True,
                              "native_dialog_cancel_reopen": dialogs, "portal_parent": prefix,
                              "native_dialog_open": True, "audio_device_opened": False}))
        except BaseException:
            app = app_for(process.pid)
            if app is not None:
                (scratch / "failure-provider.txt").write_text("\n".join(str(node.getRoleName()) + ": " + str(node.name) for node in nodes(app))[:20000])
            raise
        finally:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
            monitor.terminate()
            monitor.wait(timeout=5)


if __name__ == "__main__":
    main()
