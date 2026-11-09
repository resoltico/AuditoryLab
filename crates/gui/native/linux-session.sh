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
compositor_pid=
portal_pid=
cleanup() {
  if [[ -n "$portal_pid" ]]; then kill "$portal_pid" 2>/dev/null || true; wait "$portal_pid" 2>/dev/null || true; fi
  if [[ -n "$compositor_pid" ]]; then kill "$compositor_pid" 2>/dev/null || true; wait "$compositor_pid" 2>/dev/null || true; fi
  kill "$wm_pid" 2>/dev/null || true
  wait "$wm_pid" 2>/dev/null || true
}
trap cleanup EXIT
if [[ "$session" == wayland ]]; then
  cat > "$scratch/sway.conf" <<'CONFIG'
xwayland disable
focus_follows_mouse no
output * mode 1600x1100
CONFIG
  WLR_BACKENDS=x11 WLR_RENDERER=pixman \
    sway --unsupported-gpu --debug --config "$scratch/sway.conf" > "$scratch/compositor.log" 2>&1 &
  compositor_pid=$!
  socket=
  for attempt in {1..100}; do
    for candidate in "$XDG_RUNTIME_DIR"/wayland-*; do
      if [[ -S "$candidate" ]]; then socket=${candidate##*/}; break; fi
    done
    if [[ -n "$socket" ]]; then break; fi
    if ! kill -0 "$compositor_pid" 2>/dev/null; then cat "$scratch/compositor.log"; exit 1; fi
    sleep 0.1
  done
  test -n "$socket"
  export WAYLAND_DISPLAY="$socket"
  export XDG_CURRENT_DESKTOP=sway
  export XDG_SESSION_TYPE=wayland
  export GDK_BACKEND=wayland
  # Native X11 input goes into the compositor's real nested output surface.
  outer=
  for attempt in {1..100}; do
    mapfile -t windows < <(xwininfo -root -tree | sed -n 's/^[[:space:]]*\(0x[[:xdigit:]]*\).*/\1/p')
    for window in "${windows[@]}"; do
      # wlroots publishes the UTF-8 _NET_WM_NAME, not a legacy WM_NAME.
      if xprop -id "$window" _NET_WM_NAME 2>/dev/null | rg -q '"wlroots - ' \
        && xwininfo -id "$window" 2>/dev/null | rg -q 'Map State: IsViewable'; then
        outer=$window
        break
      fi
    done
    if [[ -n "$outer" ]]; then break; fi
    sleep 0.1
  done
  if [[ -z "$outer" ]]; then
    xwininfo -root -tree > "$scratch/failure-x11-tree.txt"
    echo 'No visible native compositor output with the expected _NET_WM_NAME' >&2
    exit 1
  fi
  xdotool windowactivate --sync "$outer"
  if ! rg -qi 'Creating pixman renderer|pixman' "$scratch/compositor.log"; then
    cat "$scratch/compositor.log"
    echo 'Wayland compositor did not establish its requested Pixman composition backend' >&2
    exit 1
  fi
  # Hosted Xvfb supplies no DRM render FD. Pixman composes the real native
  # surfaces; AuditoryLab itself must still produce NativeOpenGL frames through
  # Winit/FemtoVG, independently asserted by the application's rendering notifier.
  timeout 5s wayland-info > "$scratch/compositor-protocols.txt"
  for protocol in zxdg_exporter_v2 zxdg_importer_v2; do
    if ! rg -q "$protocol" "$scratch/compositor-protocols.txt"; then
      echo "Wayland compositor lacks required native parent protocol $protocol" >&2
      exit 1
    fi
  done
else
  unset WAYLAND_DISPLAY
  export XDG_CURRENT_DESKTOP=GNOME
  export XDG_SESSION_TYPE=x11
  export GDK_BACKEND=x11
fi
# The portal and its GTK implementation must inherit this actual session.
mkdir -p "$XDG_CONFIG_HOME/xdg-desktop-portal"
printf '[preferred]\ndefault=gtk\norg.freedesktop.impl.portal.FileChooser=gtk\n' > "$XDG_CONFIG_HOME/xdg-desktop-portal/portals.conf"
dbus-update-activation-environment DISPLAY XAUTHORITY GDK_BACKEND XDG_RUNTIME_DIR XDG_CURRENT_DESKTOP XDG_SESSION_TYPE XDG_CONFIG_HOME XDG_DATA_HOME XDG_CACHE_HOME
if [[ "$session" == wayland ]]; then dbus-update-activation-environment WAYLAND_DISPLAY; fi
/usr/libexec/xdg-desktop-portal-gtk > "$scratch/portal-provider.log" 2>&1 &
portal_pid=$!
gdbus call --session --dest org.a11y.Bus --object-path /org/a11y/bus \
  --method org.freedesktop.DBus.Properties.Set org.a11y.Status IsEnabled '<true>'
/usr/bin/python3 -B crates/gui/native/wayland_surface_test.py
/usr/bin/python3 -B crates/gui/native/linux.py --binary "$binary" --fixture "$fixture" --session "$session" --scratch "$scratch"
