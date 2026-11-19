#!/usr/bin/env bash
# Drive mx4-haptic-daemon in isolation: a private D-Bus session bus, a fake
# hidraw file standing in for the mouse, and a fake notification server.
# See ../SKILL.md for the full recipe. Run with no arguments for usage.
set -euo pipefail

SKILL_DIR="$(cd "$(dirname "$0")/.." && pwd)"
REPO="$(cd "$SKILL_DIR/../../.." && pwd)"
SCRATCH_ROOT="${MX4_VERIFY_SCRATCH:-/tmp/mx4-verify}"
EVIDENCE_ROOT="${MX4_VERIFY_EVIDENCE:-$HOME/mx4-verify-evidence}"
IFACE=com.mx4haptic.CursorTracker
OBJ=/com/mx4haptic/CursorTracker

die() { echo "verify: $*" >&2; exit 1; }

usage() {
  cat <<EOF
usage: verify.sh <command> [args]

  launch                 start an isolated daemon; prints RUN_ID
  doctor                 read-only health check of the current run
  boundary FROM TO X Y   emit BoundaryCrossed (display boundary haptic)
  workspace FROM TO      emit WorkspaceSwitched
  snap TYPE MONITOR      emit WindowSnapped (TYPE: left|right|maximize)
  focus NAME             emit FocusChanged
  notify SUMMARY         send a desktop notification (Notify call)
  packets                decode every HID++ packet the daemon wrote
  evidence [LABEL]       snapshot log + decoded packets into the evidence dir
  test-mode              run 'mx4-haptic-daemon --test' against a fake device
  cleanup                stop what launch started; keeps evidence

Trigger commands accept --repeat N (fire N events back to back) and
--expect K (assert exactly K new packets; default 1). Example, debounce:
  verify.sh boundary 0 1 1920 540 --repeat 2 --expect 1
Env: MX4_RUN=<run id> (default: latest run), MX4_CONFIG=<base config for
     launch> (default: repo config.yaml), MX4_VERIFY_SCRATCH,
     MX4_VERIFY_EVIDENCE, MX4_PY (python with dbus+gi+yaml).
EOF
}

find_py() {
  if [ -n "${MX4_PY:-}" ]; then echo "$MX4_PY"; return; fi
  for p in python3 /usr/bin/python3 /usr/bin/python3.12 python3.12; do
    if command -v "$p" >/dev/null 2>&1 &&
       "$p" -c 'import dbus, yaml; from gi.repository import GLib' 2>/dev/null; then
      command -v "$p"; return
    fi
  done
  die "no python with dbus-python, PyGObject and PyYAML (apt install python3-dbus python3-gi python3-yaml)"
}

run_dir() {
  local id="${MX4_RUN:-}"
  if [ -z "$id" ]; then
    [ -L "$SCRATCH_ROOT/latest" ] || die "no run found; run 'verify.sh launch' first"
    readlink -f "$SCRATCH_ROOT/latest"
  else
    echo "$SCRATCH_ROOT/$id"
  fi
}

load_run() {
  RUN="$(run_dir)"
  [ -f "$RUN/env" ] || die "run dir $RUN has no env file (already cleaned up?)"
  # shellcheck disable=SC1091
  . "$RUN/env"
  export DBUS_SESSION_BUS_ADDRESS="$BUS_ADDR"
}

# A pid is "ours" only if it is alive and its cmdline contains the marker we
# launched it with. Never kill by process name.
is_ours() {
  local pid="$1" marker="$2"
  [ -n "$pid" ] && [ -r "/proc/$pid/cmdline" ] &&
    tr '\0' ' ' <"/proc/$pid/cmdline" | grep -qF -- "$marker"
}

decode() {
  "$PY" - "$1" <<'EOF'
import sys
names = {0x00:"SHARP_STATE_CHANGE",0x01:"DAMP_STATE_CHANGE",0x02:"SHARP_COLLISION",
 0x03:"DAMP_COLLISION",0x04:"SUBTLE_COLLISION",0x05:"HAPPY_ALERT",0x06:"ANGRY_ALERT",
 0x07:"COMPLETED",0x08:"SQUARE",0x09:"WAVE",0x0A:"FIREWORK",0x0B:"MAD",0x0C:"KNOCK",
 0x0D:"JINGLE",0x0E:"RINGING",0x1B:"WHISPER_COLLISION"}
data = open(sys.argv[1], "rb").read()
if len(data) % 20:
    print(f"WARNING: {len(data)} bytes is not a multiple of 20")
for i in range(0, len(data) - len(data) % 20, 20):
    p = data[i:i+20]
    ok = p[0] == 0x11 and p[3] == 0x41 and not any(p[5:])
    print(f"#{i//20+1:03d} {p.hex(' ')}  dev=0x{p[1]:02X} feat={p[2]} "
          f"wave=0x{p[4]:02X} {names.get(p[4], '?')}{'' if ok else '  MALFORMED'}")
EOF
}

packet_count() { echo $(( $(stat -c %s "$FAKE_DEV") / 20 )); }

cmd_launch() {
  PY="$(find_py)"
  mkdir -p "$SCRATCH_ROOT"
  local id; id="run-$(date +%Y%m%d-%H%M%S)-$$"
  RUN="$SCRATCH_ROOT/$id"
  EVID="$EVIDENCE_ROOT/$id"
  mkdir -p "$RUN" "$EVID"
  FAKE_DEV="$RUN/fake-hidraw"
  : >"$FAKE_DEV"
  CONFIG="$RUN/config.yaml"
  # Base config (repo's, or MX4_CONFIG) with the device swapped for the fake
  # file. The real /dev/hidraw* must never be the target of an automated run.
  local base="${MX4_CONFIG:-$REPO/config.yaml}"
  [ -f "$base" ] || die "config $base not found"
  grep -q "^device_path:" "$base" || die "config $base needs a top-level device_path: line"
  sed "s|^device_path:.*|device_path: $FAKE_DEV|" "$base" >"$CONFIG"
  grep -q "^device_path: $FAKE_DEV\$" "$CONFIG" || die "could not point config at fake device"

  local out; out="$(dbus-daemon --session --fork --print-address=1 --print-pid=1)"
  BUS_ADDR="$(sed -n 1p <<<"$out")"; BUS_PID="$(sed -n 2p <<<"$out")"
  export DBUS_SESSION_BUS_ADDRESS="$BUS_ADDR"

  "$PY" "$SKILL_DIR/scripts/fake-notifyd.py" "$EVID/notifyd.log" \
    >"$RUN/notifyd.out" 2>&1 &
  NOTIFYD_PID=$!

  "$PY" "$REPO/mx4-haptic-daemon" --config "$CONFIG" --verbose \
    >"$EVID/daemon.log" 2>&1 &
  DAEMON_PID=$!

  cat >"$RUN/env" <<EOF
RUN_ID=$id
PY=$PY
REPO=$REPO
FAKE_DEV=$FAKE_DEV
CONFIG=$CONFIG
EVID=$EVID
BUS_ADDR='$BUS_ADDR'
BUS_PID=$BUS_PID
NOTIFYD_PID=$NOTIFYD_PID
DAEMON_PID=$DAEMON_PID
GIT_REV=$(git -C "$REPO" rev-parse --short HEAD 2>/dev/null || echo unknown)$(git -C "$REPO" diff --quiet 2>/dev/null || echo -dirty)
EOF
  ln -sfn "$RUN" "$SCRATCH_ROOT/latest"

  # Ready = the daemon logged its start line AND the notification name is owned.
  for _ in $(seq 1 50); do
    if grep -q "mx4-haptic-daemon started" "$EVID/daemon.log" &&
       grep -q "fake-notifyd ready" "$RUN/notifyd.out"; then
      echo "RUN_ID=$id"
      echo "evidence: $EVID"
      return 0
    fi
    if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
      cat "$EVID/daemon.log" >&2; die "daemon exited during launch (run cleanup)"
    fi
    sleep 0.1
  done
  cat "$EVID/daemon.log" >&2
  die "daemon not ready after 5s (run cleanup)"
}

cmd_doctor() {
  load_run
  local bad=0
  check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; bad=1; fi; }
  echo "run $RUN_ID  rev $GIT_REV  evidence $EVID"
  check "private bus pid $BUS_PID alive" "is_ours $BUS_PID 'dbus-daemon --session'"
  check "daemon pid $DAEMON_PID alive with our config" "is_ours $DAEMON_PID '$CONFIG'"
  check "fake notifyd pid $NOTIFYD_PID alive" "is_ours $NOTIFYD_PID fake-notifyd.py"
  check "daemon holds the fake device open (not a real hidraw)" \
    "ls -l /proc/$DAEMON_PID/fd 2>/dev/null | grep -qF -- '$FAKE_DEV'"
  check "daemon has no real /dev/hidraw open" \
    "! ls -l /proc/$DAEMON_PID/fd 2>/dev/null | grep -q '/dev/hidraw'"
  check "bus answers and owns org.freedesktop.Notifications" \
    "gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
       --method org.freedesktop.DBus.NameHasOwner org.freedesktop.Notifications 2>/dev/null | grep -q true"
  check "daemon logged its start line" "grep -q 'mx4-haptic-daemon started' '$EVID/daemon.log'"
  echo "packets written so far: $(packet_count)"
  return $bad
}

# trigger <log-regex> <cmd...>: run cmd REPEAT times back to back, then wait
# for exactly EXPECT new packets (default 1) and, if EXPECT>0, a matching log
# line. Prints the new log lines and decoded packets.
trigger() {
  local pattern="$1"; shift
  local before; before="$(packet_count)"
  local log_before; log_before="$(wc -l <"$EVID/daemon.log")"
  local i
  for ((i = 0; i < REPEAT; i++)); do "$@" >/dev/null; done
  local want=$((before + EXPECT)) deadline=$((SECONDS + 3))
  while [ "$(packet_count)" -lt "$want" ] && [ $SECONDS -lt $deadline ]; do sleep 0.05; done
  sleep 0.5   # let any extra (unexpected) packet land before counting
  local got=$(( $(packet_count) - before ))
  tail -n +"$((log_before + 1))" "$EVID/daemon.log"
  decode "$FAKE_DEV" | tail -n +"$((before + 1))"
  [ "$got" -eq "$EXPECT" ] || die "expected $EXPECT new packet(s) after $REPEAT event(s), got $got"
  if [ "$EXPECT" -gt 0 ]; then
    tail -n +"$((log_before + 1))" "$EVID/daemon.log" | grep -qE "$pattern" ||
      die "packet written but log line /$pattern/ missing"
  fi
  echo "ok: $REPEAT event(s) -> $got packet(s), total $(packet_count)"
}

emit() { gdbus emit --session --object-path "$OBJ" --signal "$IFACE.$1" -- "${@:2}"; }

parse_expect() {
  REPEAT=1; EXPECT=1; ARGS=()
  while [ $# -gt 0 ]; do
    case "$1" in
      --repeat) REPEAT="$2"; shift 2 ;;
      --expect) EXPECT="$2"; shift 2 ;;
      *) ARGS+=("$1"); shift ;;
    esac
  done
}

cmd_boundary()  { load_run; parse_expect "$@"; [ ${#ARGS[@]} -eq 4 ] || die "boundary FROM TO X Y"
  trigger "Boundary crossed" emit BoundaryCrossed "${ARGS[@]}"; }
cmd_workspace() { load_run; parse_expect "$@"; [ ${#ARGS[@]} -eq 2 ] || die "workspace FROM TO"
  trigger "Workspace switched" emit WorkspaceSwitched "${ARGS[@]}"; }
cmd_snap()      { load_run; parse_expect "$@"; [ ${#ARGS[@]} -eq 2 ] || die "snap TYPE MONITOR"
  trigger "Window snapped" emit WindowSnapped "'${ARGS[0]}'" "${ARGS[1]}"; }
cmd_focus()     { load_run; parse_expect "$@"; [ ${#ARGS[@]} -eq 1 ] || die "focus NAME"
  trigger "Focus changed" emit FocusChanged "'${ARGS[0]}'"; }
cmd_notify()    { load_run; parse_expect "$@"; [ ${#ARGS[@]} -eq 1 ] || die "notify SUMMARY"
  trigger "Notification received" gdbus call --session \
    --dest org.freedesktop.Notifications --object-path /org/freedesktop/Notifications \
    --method org.freedesktop.Notifications.Notify -- \
    mx4-verify 0 '' "${ARGS[0]}" 'sent by verify.sh' '[]' '{}' 3000; }

cmd_packets() { load_run; decode "$FAKE_DEV"; }

cmd_evidence() {
  load_run
  local label="${1:-snapshot}"
  decode "$FAKE_DEV" >"$EVID/packets-$label.txt"
  cp "$FAKE_DEV" "$EVID/fake-hidraw-$label.bin"
  cp "$CONFIG" "$EVID/config.yaml"
  { echo "run=$RUN_ID rev=$GIT_REV label=$label at=$(date -Is)"; } >>"$EVID/INDEX"
  echo "$EVID/packets-$label.txt"
}

cmd_test_mode() {
  PY="$(find_py)"
  mkdir -p "$SCRATCH_ROOT"
  local id; id="test-$(date +%Y%m%d-%H%M%S)-$$"
  local dir="$SCRATCH_ROOT/$id" evid="$EVIDENCE_ROOT/$id"
  mkdir -p "$dir" "$evid"
  FAKE_DEV="$dir/fake-hidraw"; : >"$FAKE_DEV"
  sed "s|^device_path:.*|device_path: $FAKE_DEV|" "$REPO/config.yaml" >"$dir/config.yaml"
  set +e
  "$PY" "$REPO/mx4-haptic-daemon" --config "$dir/config.yaml" --test >"$evid/test-mode.out" 2>&1
  local rc=$?
  set -e
  echo "exit=$rc" >>"$evid/test-mode.out"
  cat "$evid/test-mode.out"
  decode "$FAKE_DEV" | tee "$evid/packets-test-mode.txt"
  rm -rf "$dir"
  echo "evidence: $evid"
  [ $rc -eq 0 ]
}

cmd_cleanup() {
  RUN="$(run_dir)"
  if [ ! -f "$RUN/env" ]; then echo "nothing to clean at $RUN"; return 0; fi
  # shellcheck disable=SC1091
  . "$RUN/env"
  # Preserve evidence before scratch goes away.
  if [ -f "$FAKE_DEV" ]; then
    PY="$PY" decode "$FAKE_DEV" >"$EVID/packets-final.txt" || true
    cp "$FAKE_DEV" "$EVID/fake-hidraw-final.bin" || true
    cp "$CONFIG" "$EVID/config.yaml" || true
  fi
  # Stop the daemon first, so it never outlives its fake device and falls
  # back to scanning real /dev/hidraw*.
  for pair in "$DAEMON_PID:$CONFIG" "$NOTIFYD_PID:fake-notifyd.py" "$BUS_PID:dbus-daemon --session"; do
    local pid="${pair%%:*}" marker="${pair#*:}"
    if is_ours "$pid" "$marker"; then
      kill "$pid" 2>/dev/null || true
      for _ in $(seq 1 30); do kill -0 "$pid" 2>/dev/null || break; sleep 0.1; done
      is_ours "$pid" "$marker" && kill -9 "$pid" 2>/dev/null || true
      echo "stopped pid $pid ($marker)"
    fi
  done
  rm -rf "$RUN"
  [ "$(readlink -f "$SCRATCH_ROOT/latest" 2>/dev/null)" = "$RUN" ] && rm -f "$SCRATCH_ROOT/latest"
  echo "evidence kept at $EVID"
  ls "$EVID"
}

case "${1:-}" in
  launch) cmd_launch ;;
  doctor) cmd_doctor ;;
  boundary|workspace|snap|focus|notify|packets|evidence|cleanup)
    c="$1"; shift; "cmd_$c" "$@" ;;
  test-mode) cmd_test_mode ;;
  *) usage; [ -n "${1:-}" ] && exit 2 || exit 0 ;;
esac
