#!/usr/bin/env python3
"""Production desktop qualification through AT-SPI and native compositor input.

Run inside a private D-Bus session with X11 or a real Sway Wayland session.
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
from gi.repository import GLib


def pump_events():
    # AT-SPI cache retirement/focus events use the ordinary GLib main context.
    # Bound each drain so an animated application cannot starve the deadline.
    context = GLib.MainContext.default()
    for _ in range(32):
        if not context.pending():
            break
        context.iteration(False)


def wait_for(description, probe, seconds=30):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        pump_events()
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
        except (GLib.Error, RuntimeError, AttributeError):
            continue


def visible(node):
    try:
        state = node.getState()
        return state.contains(pyatspi.STATE_SHOWING) and state.contains(pyatspi.STATE_ENABLED)
    except (GLib.Error, RuntimeError):
        return False


def named(root, name, require_showing=True):
    # Native provider nodes can retire between a child enumeration and its
    # property read when the actual UI changes. Re-scan within the deadline.
    for node in nodes(root):
        try:
            if node.name == name and (not require_showing or visible(node)):
                return node
        except (GLib.Error, RuntimeError):
            continue
    return None


def showing_main_frame(root):
    for node in nodes(root):
        try:
            if node.getRole() == pyatspi.ROLE_FRAME and node.name == "Auditory Lab" and visible(node):
                return node
        except (GLib.Error, RuntimeError):
            continue
    return None


def provider_description(root):
    lines = []
    for node in nodes(root):
        try:
            lines.append(str(node.getRoleName()) + ": " + str(node.name))
        except (GLib.Error, RuntimeError):
            lines.append("<retired native provider node>")
    return "\n".join(lines)[:20000]


def activate(node):
    action = node.queryAction()
    if action.nActions < 1 or not action.doAction(0):
        raise RuntimeError("OS-provider action refused: " + node.name)


def current_chooser():
    cancel = named(pyatspi.Registry.getDesktop(0), "Cancel")
    if cancel is None:
        return None
    try:
        parent = cancel.parent
        while parent.parent is not None and parent.getRole() not in (pyatspi.ROLE_DIALOG, pyatspi.ROLE_FILE_CHOOSER):
            parent = parent.parent
        if parent.getRole() not in (pyatspi.ROLE_DIALOG, pyatspi.ROLE_FILE_CHOOSER):
            return None
        if not cancel.queryComponent().grabFocus():
            return None
        pump_events()
        if not visible(cancel) or not cancel.getState().contains(pyatspi.STATE_FOCUSED):
            return None
        return cancel, parent
    except (GLib.Error, RuntimeError):
        return None


def focused_location(root):
    for node in nodes(root):
        try:
            state = node.getState()
            if node.getRole() == pyatspi.ROLE_TEXT and visible(node) and state.contains(pyatspi.STATE_EDITABLE) and state.contains(pyatspi.STATE_FOCUSED):
                return node
        except (GLib.Error, RuntimeError):
            continue
    return None


def key(*keys):
    subprocess.run(["xdotool", "key", "--clearmodifiers", *keys], check=True, timeout=5)


def type_text(text):
    subprocess.run(["xdotool", "type", "--clearmodifiers", "--delay", "2", "--", text], check=True, timeout=10)


def app_for(pid):
    for app in pyatspi.Registry.getDesktop(0):
        try:
            if app.get_process_id() == pid:
                return app
        except (GLib.Error, RuntimeError):
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
            frame = wait_for("showing native main frame", lambda: showing_main_frame(app))
            extent = frame.queryComponent().getExtents(pyatspi.DESKTOP_COORDS)
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
                cancel, _ = wait_for("focused native dialog before surface recreation", current_chooser)
                activate(cancel)
                wait_for("native dialog dismissed before surface recreation", lambda: not visible(cancel))
                wait_for("main recovered before surface recreation", lambda: named(app, "Open recipe…"))
                def first_parent():
                    parents = re.findall(r'string "(wayland:[^"]+)"', portal_path.read_text())
                    return parents[0] if parents else None
                before_parent = wait_for("actual portal parent before recreation", first_parent)
                lifetime = scratch / "surface-lifetime.txt"
                wait_for("observed native GL first frame", lambda: lifetime.exists() and "first-frame-native-gl" in lifetime.read_text())
                marker = scratch / "surface-parent-confirmed.pending"
                marker.write_bytes(b"native-parent-confirmed\n")
                marker.replace(scratch / "surface-parent-confirmed")
                wait_for("actual rendered surface after hide/show", lambda: lifetime.exists() and "recreated-frame-native-gl" in lifetime.read_text())
                trace = lifetime.read_text().splitlines()
                if trace != ["first-frame-native-gl", "hidden", "shown", "recreated-frame-native-gl"]:
                    raise RuntimeError("unexpected production surface-lifetime trace: " + repr(trace))
                app = wait_for("OS provider after surface recreation", lambda: app_for(process.pid))
                wait_for("operation names on recreated surface", lambda: named(app, "Workbench"))
            activate(named(app, "Workbench"))
            wait_for("Workbench New sound provider", lambda: named(app, "New sound"))
            notes = wait_for("multiline OS text provider", lambda: named(app, "Custom recipe notes", require_showing=False))
            title = wait_for("visible title keyboard starting point", lambda: named(app, "Sound title"))
            if not title.queryComponent().grabFocus():
                raise RuntimeError("title OS-provider focus refused")
            # Slint's native Tab-navigation path scrolls clipped fields into view;
            # its AccessKit adapter does not advertise ScrollIntoView actions.
            for _ in range(80):
                key("Tab")
                time.sleep(0.05)
                if notes.getState().contains(pyatspi.STATE_FOCUSED):
                    break
            wait_for("actual visible multiline keyboard focus", lambda: visible(notes) and notes.getState().contains(pyatspi.STATE_FOCUSED))
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
                cancel, parent = wait_for("current focused native file chooser", current_chooser)
                dialogs.append(parent.name)
                activate(cancel)
                wait_for("native dialog actually dismissed", lambda: not visible(cancel))
                wait_for("native dialog cancelled and app recovered", lambda: named(app, "Open recipe…"))
            # Reopen and use the real location control to open an isolated recipe.
            activate(named(app, "Open recipe…"))
            cancel, chooser = wait_for("current focused native chooser reopened", current_chooser)
            # GTK binds Ctrl+L on its chooser content widget; the Cancel action
            # area is a sibling and intentionally does not own that shortcut.
            files = wait_for("native chooser Files table", lambda: named(chooser, "Files"))
            if files.getRole() != pyatspi.ROLE_TABLE or not files.queryComponent().grabFocus():
                raise RuntimeError("native chooser Files table focus refused")
            wait_for("actual Files table keyboard focus", lambda: files.getState().contains(pyatspi.STATE_FOCUSED))
            key("ctrl+l")
            # GTK3 entries expose editable TEXT, unlike Slint's ENTRY role.
            location = wait_for("actual focused editable GTK location control", lambda: focused_location(chooser))
            key("ctrl+a")
            fixture = str(args.fixture.resolve())
            type_text(fixture)
            wait_for("native location received the exact fixture path", lambda: text_of(location) == fixture)
            # Single-selection xdg-desktop-portal-gtk exposes Select as its
            # actual accept operation; typing a location alone is not acceptance.
            accept = wait_for("native chooser Select control", lambda: named(chooser, "Select"))
            activate(accept)
            wait_for("native chooser accepted and dismissed", lambda: not visible(cancel))
            wait_for("native chooser accepted location", lambda: named(app, "Open recipe…"))
            wait_for("opened fixture selected in production provider", lambda: named(app, "My 3 Hz pair"))
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
                              "native_dialog_open": "My 3 Hz pair", "recreated_wayland_surface_ids": surface_ids, "audio_device_opened": False}))
        except BaseException:
            app = app_for(process.pid)
            if app is not None:
                (scratch / "failure-provider.txt").write_text(provider_description(app))
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
