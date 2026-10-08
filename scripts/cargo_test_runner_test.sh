#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TASK_TEMP="$(mktemp -d)"
trap 'rm -rf "$TASK_TEMP"' EXIT
mkdir "$TASK_TEMP/bin"
cat > "$TASK_TEMP/bin/cargo" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$CARGO_TARGET_DIR" "$CARGO_BUILD_JOBS" "$RUST_TEST_THREADS" "$@" > "$RUNNER_RESULT"
exit 23
EOF
cat > "$TASK_TEMP/bin/pgrep" <<'EOF'
#!/usr/bin/env bash
touch "$REAPER_CALLED"
exit 0
EOF
chmod +x "$TASK_TEMP/bin/cargo" "$TASK_TEMP/bin/pgrep"
export RUNNER_RESULT="$TASK_TEMP/result" REAPER_CALLED="$TASK_TEMP/reaper"
set +e
env -u CARGO_BUILD_JOBS -u RUST_TEST_THREADS PATH="$TASK_TEMP/bin:$PATH" "$ROOT/scripts/cargo-test" -p mxr-store --lib 'filter with spaces'
code=$?
set -e
[[ "$code" == 23 ]]
[[ ! -e "$REAPER_CALLED" ]]
printf '%s\n' "$ROOT/target-cli" 4 4 test -p mxr-store --lib 'filter with spaces' > "$TASK_TEMP/expected"
diff -u "$TASK_TEMP/expected" "$RUNNER_RESULT"
echo 'Cargo runner preserves arguments/exit code, bounds default workers and never invokes the global reaper.'
