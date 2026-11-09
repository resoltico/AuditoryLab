#!/usr/bin/env bash
# Run under dbus-run-session and xvfb-run. Wayland is a separate real compositor;
# Xvfb supplies only its outer display and input, never a Wayland substitute.
set -euo pipefail
session=$1
binary=$2
fixture=$3
scratch=$4
mkdir -p "$scratch/runtime"
chmod 700 "$scratch/runtime"
export XDG_RUNTIME_DIR="$scratch/runtime"
export XDG_CONFIG_HOME="$scratch/config-home"
export XDG_DATA_HOME="$scratch/data-home"
export XDG_CACHE_HOME="$scratch/cache-home"
export GSETTINGS_BACKEND=memory
export GTK_A11Y=always
export NO_AT_BRIDGE=0
export LIBGL_ALWAYS_SOFTWARE=1
: "${DBUS_SESSION_BUS_ADDRESS:?private session bus required}"
: "${DISPLAY:?native X11 display required}"
: "${XAUTHORITY:?private X11 authorization required}"
openbox --sm-disable > "$scratch/window-manager.log" 2>&1 &
wm_pid=$!
weston_pid=
portal_pid=
cleanup() {
  if [[ -n "$portal_pid" ]]; then kill "$portal_pid" 2>/dev/null || true; wait "$portal_pid" 2>/dev/null || true; fi
  if [[ -n "$weston_pid" ]]; then kill "$weston_pid" 2>/dev/null || true; wait "$weston_pid" 2>/dev/null || true; fi
  kill "$wm_pid" 2>/dev/null || true
  wait "$wm_pid" 2>/dev/null || true
}
trap cleanup EXIT
if [[ "$session" == wayland ]]; then
  weston --backend=x11-backend.so --socket=auditorylab-wayland --width=1600 --height=1100 \
    --idle-time=0 --no-config --log="$scratch/weston.log" &
  weston_pid=$!
  for attempt in {1..100}; do
    if [[ -S "$XDG_RUNTIME_DIR/auditorylab-wayland" ]]; then break; fi
    if ! kill -0 "$weston_pid" 2>/dev/null; then cat "$scratch/weston.log"; exit 1; fi
    sleep 0.1
  done
  test -S "$XDG_RUNTIME_DIR/auditorylab-wayland"
  export WAYLAND_DISPLAY=auditorylab-wayland
  export XDG_CURRENT_DESKTOP=GNOME
  export XDG_SESSION_TYPE=wayland
  # Native X11 input goes into Weston, which routes native Wayland key events.
  outer=$(timeout 10s xdotool search --sync --onlyvisible --class weston | head -n 1)
  xdotool windowactivate --sync "$outer"
  if ! rg -qi 'GL renderer|GL version|EGL version|gl-renderer' "$scratch/weston.log"; then
    cat "$scratch/weston.log"
    echo 'Weston did not establish the required GL renderer' >&2
    exit 1
  fi
else
  unset WAYLAND_DISPLAY
  export XDG_CURRENT_DESKTOP=GNOME
  export XDG_SESSION_TYPE=x11
fi
# The portal and its GTK implementation must inherit this actual session.
dbus-update-activation-environment DISPLAY XAUTHORITY XDG_RUNTIME_DIR XDG_CURRENT_DESKTOP XDG_SESSION_TYPE XDG_CONFIG_HOME XDG_DATA_HOME XDG_CACHE_HOME
if [[ "$session" == wayland ]]; then dbus-update-activation-environment WAYLAND_DISPLAY; fi
/usr/libexec/xdg-desktop-portal-gtk > "$scratch/portal-provider.log" 2>&1 &
portal_pid=$!
gdbus call --session --dest org.a11y.Bus --object-path /org/a11y/bus \
  --method org.freedesktop.DBus.Properties.Set org.a11y.Status IsEnabled '<true>'
/usr/bin/python3 crates/gui/native/linux.py --binary "$binary" --fixture "$fixture" --session "$session" --scratch "$scratch"
