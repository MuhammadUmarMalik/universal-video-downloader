#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BINARY="${ROOT_DIR}/apps/desktop/src-rust/target/release/universal-media-downloader"
FIXTURE="${ROOT_DIR}/scripts/native-recovery-fixture.py"
ASSERT="${ROOT_DIR}/scripts/assert-native-recovery.py"
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/umd-native-recovery.XXXXXX")"
DATA_DIR="${WORK_DIR}/data"
APP_DIR="${DATA_DIR}/com.umd.desktop"
DESTINATION="${APP_DIR}/downloads"
FIRST_LOG="${WORK_DIR}/first-launch.log"
RESTART_LOG="${WORK_DIR}/restart.log"
RPC_FIFO="${WORK_DIR}/rpc.fifo"
mkfifo "${RPC_FIFO}"
exec 3<>"${RPC_FIFO}"

cleanup() {
  if [[ -n "${FIRST_PID:-}" ]] && kill -0 "${FIRST_PID}" 2>/dev/null; then
    kill -TERM "${FIRST_PID}" 2>/dev/null || true
    wait "${FIRST_PID}" 2>/dev/null || true
  fi
  if [[ -n "${RESTART_PID:-}" ]] && kill -0 "${RESTART_PID}" 2>/dev/null; then
    kill -TERM "${RESTART_PID}" 2>/dev/null || true
    wait "${RESTART_PID}" 2>/dev/null || true
  fi
  rm -rf "${WORK_DIR}"
}
trap cleanup EXIT

if [[ ! -x "${BINARY}" ]]; then
  cat >&2 <<EOF
Missing release binary:
  ${BINARY}
Build it first with:
  cargo build --release --manifest-path apps/desktop/src-rust/Cargo.toml
EOF
  exit 2
fi

printf '%s\n' "[1/5] Initializing isolated native application data"
UMD_APP_DATA_DIR="${APP_DIR}" "${BINARY}" --headless <"${RPC_FIFO}" >"${FIRST_LOG}" 2>&1 &
FIRST_PID=$!
sleep 3
if ! kill -0 "${FIRST_PID}" 2>/dev/null; then
  cat "${FIRST_LOG}" >&2
  echo "native binary exited before fixture setup" >&2
  exit 1
fi
kill -KILL "${FIRST_PID}" 2>/dev/null || true
wait "${FIRST_PID}" 2>/dev/null || true

printf '%s\n' "[2/5] Seeding a real interrupted download and .part file"
python3 "${FIXTURE}" "${APP_DIR}/umd.sqlite3" "${DESTINATION}"

printf '%s\n' "[3/5] Relaunching the native binary after forced termination"
UMD_APP_DATA_DIR="${APP_DIR}" "${BINARY}" --headless <"${RPC_FIFO}" >"${RESTART_LOG}" 2>&1 &
RESTART_PID=$!
sleep 3
if ! kill -0 "${RESTART_PID}" 2>/dev/null; then
  cat "${RESTART_LOG}" >&2
  echo "native binary exited during restart" >&2
  exit 1
fi

printf '%s\n' "[4/5] Checking startup recovery telemetry"
grep -F '"event":"headless_startup_recovery_completed"' "${RESTART_LOG}"
grep -F '"requeued":1' "${RESTART_LOG}"

printf '%s\n' "[5/5] Checking durable SQLite state and partial contents"
python3 "${ASSERT}" "${APP_DIR}/umd.sqlite3" "${DESTINATION}/native-recovery.mp4.part"
printf '%s\n' "native packaged recovery smoke test passed"
