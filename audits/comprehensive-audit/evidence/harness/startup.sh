#!/usr/bin/env bash
# Release shell (audit copy with AUDIT_STARTUP hooks) + release axiomd, private KWin at 125 %.
set -uo pipefail
root="$(cd "$(dirname "$0")/../../../.." && pwd)"; S="$1"; runs="${2:-6}"
bundle="$S/app/build/linux/x64/release/bundle/axiom"; daemon="$root/target/release/axiomd"
source "$root/scripts/env.sh"
sock="axiom-audit-start-$$"
kwin_wayland --virtual --no-lockscreen --no-global-shortcuts --socket "$sock" --width 2560 --height 1440 --scale 1.25 >/dev/null 2>&1 &
kwin=$!; trap 'kill $kwin 2>/dev/null || true' EXIT
for _ in $(seq 80); do [[ -S "$XDG_RUNTIME_DIR/$sock" ]] && break; sleep 0.1; done
unset DISPLAY; export WAYLAND_DISPLAY="$sock" GDK_BACKEND=wayland AXIOM_WINDOW_SIZE=2040x1120 AUDIT_STARTUP=1
ws=$(mktemp -d); mkdir -p $ws/src; for i in $(seq 200); do echo "fn f$i() {}" > $ws/src/f$i.rs; done; printf 'fn main() {\n    println!("hi");\n}\n' > $ws/src/main.rs
warmcache=$(mktemp -d); warmdata=$(mktemp -d)
one() { # kind cache data
  local t0 out; t0=$(date +%s%N)
  out=$(XDG_CACHE_HOME="$2" AUDIT_OPEN=$ws AUDIT_FILE=src/main.rs AUDIT_HOLD=500 "$bundle" --daemon "$daemon" --data-dir "$3" 2>/dev/null)
  local t0us=$((t0/1000))
  local mainus ff ready folder sd
  mainus=$(grep -o 'main_entered_us=[0-9]*' <<<"$out" | cut -d= -f2); ff=$(grep -o 'first_frame_us=[0-9]*' <<<"$out" | cut -d= -f2)
  ready=$(grep -o 'ready_us=[0-9]*' <<<"$out" | cut -d= -f2); folder=$(grep -o 'folder_us=[0-9]*' <<<"$out" | cut -d= -f2); sd=$(grep -o 'shutdown_ms=[0-9.]*' <<<"$out" | cut -d= -f2)
  echo "$1 exec->main $(( (mainus-t0us)/1000 )) ms, exec->first_frame $(( (ff-t0us)/1000 )) ms, exec->ready $(( (ready-t0us)/1000 )) ms, exec->folder_listed $(( (folder-t0us)/1000 )) ms, shutdown $sd ms"
}
for i in $(seq $runs); do
  c=$(mktemp -d); d=$(mktemp -d); one cold "$c" "$d"; rm -rf "$c" "$d"
  one warm "$warmcache" "$warmdata"
done
# Idle: folder + document, wait 15 s, sample 10 s.
out=$(mktemp); XDG_CACHE_HOME="$warmcache" AUDIT_OPEN=$ws AUDIT_FILE=src/main.rs AUDIT_HOLD=30000 "$bundle" --daemon "$daemon" --data-dir "$warmdata" > "$out" 2>/dev/null &
app=$!; sleep 15
dpid=$(grep -o 'daemon=[0-9]*' "$out" | cut -d= -f2)
tk() { awk '{print $14+$15}' /proc/$1/stat; }
pss() { awk '/^Pss:/{print $2}' /proc/$1/smaps_rollup; }
a0=$(tk $app); d0=$(tk $dpid); sleep 10; a1=$(tk $app); d1=$(tk $dpid)
echo "idle (folder + document, 10 s after 15 s): shell CPU $(( (a1-a0) ))% , daemon CPU $(( (d1-d0) ))% (ticks/10s == percent), shell PSS $(( $(pss $app)/1024 )) MiB, daemon PSS $(( $(pss $dpid)/1024 )) MiB, threads shell $(ls /proc/$app/task | wc -l)"
wait $app
rm -rf "$ws" "$warmcache" "$warmdata" "$out"
