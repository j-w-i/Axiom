#!/usr/bin/env bash
# The app's tests on a CI machine. A run that hangs says how far it came and fails: on Windows the
# runner cannot end a stuck test, waits out the whole job and then keeps no log.
set -u
cd "$(dirname "$0")/../app"
log="$(mktemp)"
flutter test --reporter expanded >"$log" 2>&1 &
pid=$!
for _ in $(seq 480); do
  kill -0 "$pid" 2>/dev/null || break
  sleep 1
done
if kill -0 "$pid" 2>/dev/null; then
  grep -nE '\[E\]|Exception|did not complete' "$log" | head -40
  echo "--- the last lines"
  tail -n 60 "$log"
  if command -v taskkill >/dev/null; then
    for exe in flutter_tester.exe axiom-host.exe; do taskkill //F //T //IM "$exe" 2>&1; done
  fi
  echo "::error::the tests were still running after eight minutes"
  exit 1
fi
wait "$pid"
status=$?
cat "$log"
exit "$status"
