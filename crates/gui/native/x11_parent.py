"""Bind actual FileChooser requests and native transients to observed X11 windows."""
import json
import re


_CALL = re.compile(r'^method call\b[^\n]*-> destination=org\.freedesktop\.portal\.Desktop\s[^\n]*interface=org\.freedesktop\.portal\.FileChooser;\s*member=OpenFile\s*$', re.MULTILINE)
_ARGUMENT = re.compile(r'^[ \t]+string ("(?:[^"\\]|\\.)*")[ \t]*$', re.MULTILINE)
_XID = re.compile(r'x11:(?:0x)?([0-9a-fA-F]+)')
_TRANSIENT = re.compile(r'WM_TRANSIENT_FOR\(WINDOW\): window id # (0x[0-9a-fA-F]+)\s*')


def _nonzero_xid(value):
    if not 0 < value <= 0xffffffff:
        raise RuntimeError("native X11 window identifier is zero or outside its 32-bit domain")
    return value


def verified_portal_parents(calls, expected_windows):
    """Require each real request's first argument to name its observed main XID.

    Header/options/caption substrings do not establish a parent. The expected
    windows come from the native server before each request, not this log.
    """
    requests = list(_CALL.finditer(calls))
    if not expected_windows or len(requests) != len(expected_windows):
        raise RuntimeError("native X11 FileChooser request count differs from observed actions")
    parents = []
    for index, (request, expected) in enumerate(zip(requests, expected_windows)):
        _nonzero_xid(expected)
        end = requests[index + 1].start() if index + 1 < len(requests) else len(calls)
        # The first body line is the parent. Never skip a malformed first
        # argument to find a convincing decoy farther down the message.
        body = calls[request.end():end].lstrip("\n")
        argument = _ARGUMENT.match(body)
        if argument is None:
            raise RuntimeError("native FileChooser request has no first string parent argument")
        parent = json.loads(argument.group(1))
        xid = _XID.fullmatch(parent)
        if xid is None or _nonzero_xid(int(xid.group(1), 16)) != expected:
            raise RuntimeError("native X11 portal parent does not identify the observed application window")
        parents.append(parent)
    return parents


def verified_transient(property_text, expected):
    """Validate WM_TRANSIENT_FOR when the native toolkit publishes that property."""
    _nonzero_xid(expected)
    if property_text.strip() in ("WM_TRANSIENT_FOR:  not found.", "WM_TRANSIENT_FOR:  no such atom on any window."):
        return None
    match = _TRANSIENT.fullmatch(property_text.strip())
    if match is None or _nonzero_xid(int(match.group(1), 16)) != expected:
        raise RuntimeError("native X11 chooser transient does not identify the observed application window")
    return expected
