#!/usr/bin/env python3
"""Production desktop qualification through AT-SPI and native compositor input.

Run inside a private D-Bus session with X11 or a real Weston Wayland session.
No application callbacks, offscreen renderer, or audio actions are used.
"""
import argparse
import json
import os
import re
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
    def timed_out(_signal, _frame):
        raise TimeoutError("native qualification exceeded 180 seconds")
    signal.signal(signal.SIGALRM, timed_out)
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
                "XDG_CACHE_HOME": str(scratch / "xdg-cache"),
                "WINIT_UNIX_BACKEND": args.session,
                "SLINT_ACCESSIBILITY": "enable"})
    if args.session == "x11":
        env.pop("WAYLAND_DISPLAY", None)
    else:
        env["WAYLAND_DEBUG"] = "client"
        env["AUDITORYLAB_NATIVE_QA_RECREATE_SURFACE"] = "1"
    # The monitor observes actual portal calls, including their OS parent handles.
    portal_path = scratch / "portal.log"
    with portal_path.open("w") as portal, (scratch / "application.log").open("w") as log:
        monitor = subprocess.Popen(["dbus-monitor", "--session", "interface='org.freedesktop.portal.FileChooser'"], stdout=portal, stderr=portal)
        process = subprocess.Popen([str(args.binary.resolve())], env=env, stdout=log, stderr=log)
        try:
            app = wait_for("production application AT-SPI provider", lambda: app_for(process.pid))
            browse = wait_for("first-frame Browse Library provider action", lambda: named(app, "Browse Library"))
            activate(browse)
            for name in ["Open recipe…", "Workbench"]:
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
            before_parent = None
            surface_ids = None
            if args.session == "wayland":
                activate(named(app, "Open recipe…"))
                cancel = wait_for("native dialog before surface recreation", lambda: named(pyatspi.Registry.getDesktop(0), "Cancel"))
                activate(cancel)
                def first_parent():
                    parents = re.findall(r'string "(wayland:[^"]+)"', portal_path.read_text())
                    return parents[0] if parents else None
                before_parent = wait_for("actual portal parent before recreation", first_parent)
                lifetime = scratch / "surface-lifetime.txt"
                wait_for("observed native GL first frame", lambda: lifetime.exists() and "first-frame-native-gl" in lifetime.read_text())
                (scratch / "surface-parent-confirmed").write_bytes(b"native-parent-confirmed\n")
                wait_for("actual rendered surface after hide/show", lambda: lifetime.exists() and "recreated-frame-native-gl" in lifetime.read_text())
                trace = lifetime.read_text().splitlines()
                if trace != ["first-frame-native-gl", "hidden", "shown", "recreated-frame-native-gl"]:
                    raise RuntimeError("unexpected production surface-lifetime trace: " + repr(trace))
                app = wait_for("OS provider after surface recreation", lambda: app_for(process.pid))
                wait_for("operation names on recreated surface", lambda: named(app, "Workbench"))
            activate(named(app, "Workbench"))
            wait_for("Workbench New sound provider", lambda: named(app, "New sound"))
            notes = wait_for("multiline OS text provider", lambda: next((node for node in nodes(app) if node.name == "Custom recipe notes"), None))
            if not visible(notes):
                if not notes.queryComponent().scrollTo(pyatspi.SCROLL_ANYWHERE):
                    raise RuntimeError("native notes ScrollIntoView refused")
                wait_for("scrolled multiline field", lambda: visible(notes))
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
                activate(wait_for("enabled Open recipe", lambda: named(app, "Open recipe…")))
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
                wait_for("native dialog cancelled and app recovered", lambda: named(app, "Open recipe…"))
            # Reopen and use the real location control to open an isolated recipe.
            activate(named(app, "Open recipe…"))
            wait_for("native chooser reopened", lambda: named(pyatspi.Registry.getDesktop(0), "Cancel"))
            key("ctrl+l")
            type_text(str(args.fixture.resolve()))
            key("Return")
            wait_for("native chooser accepted location", lambda: named(app, "Open recipe…"))
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
                surfaces = list(re.finditer(r'xdg_wm_base[#@]\d+\.get_xdg_surface\((?:new id )?xdg_surface[#@]\d+, wl_surface[#@](\d+)\)', protocol))
                if len(surfaces) < 2:
                    raise RuntimeError("production did not recreate its actual Wayland toplevel surface")
                old_id = surfaces[0].group(1)
                destroyed = re.search(r'wl_surface[#@]' + re.escape(old_id) + r'\.destroy\(\)', protocol[surfaces[0].end():])
                if destroyed is None or not any(item.start() > surfaces[0].end() + destroyed.end() for item in surfaces[1:]):
                    raise RuntimeError("no old native surface destruction followed by a new toplevel")
                parents = re.findall(r'string "(wayland:[^"]+)"', calls)
                if len(parents) < 4 or parents[0] != before_parent or parents[1] == before_parent:
                    raise RuntimeError("recreated native surface did not receive a fresh usable portal parent")
                surface_ids = [old_id, surfaces[-1].group(1)]
            print(json.dumps({"session": args.session, "provider": "AT-SPI", "pid": process.pid,
                              "production": "Winit/FemtoVG", "first_frame_extent": [extent.width, extent.height], "first_frame_colors": colors,
                              "multiline_native_keys": True, "modifier_selection": True,
                              "native_dialog_cancel_reopen": dialogs, "portal_parent": prefix,
                              "native_dialog_open": True, "recreated_wayland_surface_ids": surface_ids, "audio_device_opened": False}))
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
