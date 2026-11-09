"""Correlate real portal parents with their native Wayland surface lifetimes."""
import json
import re


_EXPORT = re.compile(r'zxdg_exporter_v2[@#]\d+\.export_toplevel\((?:new id )?zxdg_exported_v2[@#](\d+), wl_surface[@#](\d+)\)')
_HANDLE = re.compile(r'zxdg_exported_v2[@#](\d+)\.handle\("((?:[^"\\]|\\.)*)"\)')
_XDG = re.compile(r'xdg_wm_base[@#]\d+\.get_xdg_surface\((?:new id )?xdg_surface[@#](\d+), wl_surface[@#](\d+)\)')


def _parent_export(protocol, parent):
    if not parent.startswith("wayland:") or parent == "wayland:":
        raise RuntimeError("missing actual Wayland portal parent")
    handles = [event for event in _HANDLE.finditer(protocol)
               if json.loads('"' + event.group(2) + '"') == parent.removeprefix("wayland:")]
    if len(handles) != 1:
        raise RuntimeError("portal parent has no unambiguous native exported handle")
    handle = handles[0]
    exports = [event for event in _EXPORT.finditer(protocol, 0, handle.start())
               if event.group(1) == handle.group(1)]
    if not exports:
        raise RuntimeError("native handle has no preceding surface export")
    return exports[-1]


def _toplevel_exists(protocol, creation, end):
    return re.search(r'xdg_surface[@#]' + re.escape(creation.group(1))
                     + r'\.get_toplevel\((?:new id )?xdg_toplevel[@#]\d+\)',
                     protocol[creation.end():end]) is not None


def recreated_main_surfaces(protocol, before_parent, after_parent):
    """Require actual parent-linked main destruction and a new native toplevel.

    Hidden and cursor surfaces are not main-window evidence. Numeric IDs may
    legally be reused after destruction; event order identifies each lifetime.
    """
    before = _parent_export(protocol, before_parent)
    after = _parent_export(protocol, after_parent)
    if before.start() >= after.start():
        raise RuntimeError("native parent exports are out of lifetime order")
    old_id, new_id = before.group(2), after.group(2)
    old = [event for event in _XDG.finditer(protocol, 0, before.start()) if event.group(2) == old_id]
    if not old or not _toplevel_exists(protocol, old[-1], before.start()):
        raise RuntimeError("first actual portal parent is not bound to a native toplevel")
    destroyed = re.search(r'wl_surface[@#]' + re.escape(old_id) + r'\.destroy\(\)',
                          protocol[before.end():after.start()])
    if destroyed is None:
        raise RuntimeError("actual parent-linked main surface was not destroyed")
    end = before.end() + destroyed.end()
    new = [event for event in _XDG.finditer(protocol, end, after.start()) if event.group(2) == new_id]
    if not new or not _toplevel_exists(protocol, new[-1], after.start()):
        raise RuntimeError("actual post-recreation parent has no new native toplevel binding")
    created = re.search(r'wl_compositor[@#]\d+\.create_surface\((?:new id )?wl_surface[@#]'
                        + re.escape(new_id) + r'\)', protocol[end:new[-1].start()])
    if created is None:
        raise RuntimeError("actual post-recreation parent has no new native surface creation")
    return [old_id, new_id]
