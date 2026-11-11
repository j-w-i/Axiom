# shellcheck shell=bash
# Source this to run GUI processes on a private, invisible X server ONLY.
# Never shows a window on the user's desktop: Wayland is removed from the
# environment (GTK would otherwise prefer the compositor over $DISPLAY) and
# GTK is forced onto X11 pointed at a fresh Xvfb.
#
#   source apps/desktop-tauri/scripts/headless.sh   # starts Xvfb, exports env
#   ... run things ...
#   headless_stop
_xvfb="${AXIOM_XVFB:-$(command -v Xvfb || true)}"
if [[ ! -x "$_xvfb" ]]; then
  echo "headless.sh: Xvfb not found (apps/desktop-tauri/scripts/sysroot.sh provides one on Fedora); refusing to run GUI code" >&2
  return 1 2>/dev/null || exit 1
fi
_n=$((90 + RANDOM % 9))
while [[ -e "/tmp/.X11-unix/X$_n" ]]; do _n=$((_n + 1)); done
"$_xvfb" ":$_n" -screen 0 1440x900x24 -nolisten tcp ${AXIOM_XVFB_EXTRA:-} >/dev/null 2>&1 &
AXIOM_XVFB_PID=$!
for _ in $(seq 1 50); do [[ -e "/tmp/.X11-unix/X$_n" ]] && break; sleep 0.1; done
unset WAYLAND_DISPLAY WAYLAND_SOCKET
export DISPLAY=":$_n"
export GDK_BACKEND=x11
export XDG_SESSION_TYPE=x11
# Software rendering inside Xvfb (no GPU); keeps WebKitGTK from probing DMABUF.
export WEBKIT_DISABLE_DMABUF_RENDERER=1
export LIBGL_ALWAYS_SOFTWARE=1
headless_stop() { kill "$AXIOM_XVFB_PID" 2>/dev/null || true; }
# Fail closed: the X server must be ours and alive.
if ! kill -0 "$AXIOM_XVFB_PID" 2>/dev/null; then
  echo "headless.sh: Xvfb failed to start" >&2
  return 1 2>/dev/null || exit 1
fi
unset _xvfb _n
