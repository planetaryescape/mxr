#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "$0")/.." && pwd)"
parent="${TMPDIR:-$repo/dist/staple-dmg-tests}"
mkdir -p "$parent"
fixture="$(mktemp -d "$parent/staple-dmg.XXXXXX")"
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/bin" "$fixture/with spaces"
export STAPLE_DMG_PATH="$fixture/with spaces/mxr release.dmg"
touch "$STAPLE_DMG_PATH"

cat > "$fixture/bin/xcrun" <<'FAKE'
#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 3 && "$1" == stapler && "$3" == "$STAPLE_DMG_PATH" ]] || exit 91
echo "$2" >> "$STAPLE_CASE/calls"
case "$2" in
    staple)
        count="$(cat "$STAPLE_CASE/count")"
        read -r -a statuses <<< "$STAPLE_CODES"
        echo "$((count + 1))" > "$STAPLE_CASE/count"
        exit "${statuses[$count]:-90}"
        ;;
    validate)
        count="$(cat "$STAPLE_CASE/validate-count")"
        read -r -a statuses <<< "$VALIDATE_STATUS"
        echo "$((count + 1))" > "$STAPLE_CASE/validate-count"
        exit "${statuses[$count]:-90}"
        ;;
    *) exit 92 ;;
esac
FAKE
cat > "$fixture/bin/sleep" <<'FAKE'
#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 1 && ( "$1" == 5 || "$1" == 15 ) ]] || exit 93
echo "$1" >> "$STAPLE_CASE/delays"
FAKE
chmod +x "$fixture/bin/xcrun" "$fixture/bin/sleep"

run_case() {
    local name="$1" codes="$2" validation="$3" expected_status="$4"
    local expected_calls="$5" expected_delays="$6" status=0
    export STAPLE_CASE="$fixture/$name" STAPLE_CODES="$codes" VALIDATE_STATUS="$validation"
    mkdir "$STAPLE_CASE"
    echo 0 > "$STAPLE_CASE/count"
    echo 0 > "$STAPLE_CASE/validate-count"
    : > "$STAPLE_CASE/calls"
    : > "$STAPLE_CASE/delays"
    PATH="$fixture/bin:$PATH" bash "$repo/scripts/staple_dmg.sh" "$STAPLE_DMG_PATH" > "$STAPLE_CASE/output" 2>&1 || status=$?
    if [[ "$status" -ne "$expected_status" ]]; then
        cat "$STAPLE_CASE/output" >&2
        echo "$name: expected exit $expected_status, got $status" >&2
        exit 1
    fi
    printf '%s' "$expected_calls" > "$STAPLE_CASE/expected-calls"
    printf '%s' "$expected_delays" > "$STAPLE_CASE/expected-delays"
    diff -u "$STAPLE_CASE/expected-calls" "$STAPLE_CASE/calls"
    diff -u "$STAPLE_CASE/expected-delays" "$STAPLE_CASE/delays"
    echo "staple_dmg_test: $name ok"
}

run_case first-success-spaces '0' 0 0 $'staple\nvalidate\n' ''
run_case transient-68 '68 0' 0 0 $'staple\nstaple\nvalidate\n' $'5\n'
run_case third-attempt-success '68 68 0' 0 0 $'staple\nstaple\nstaple\nvalidate\n' $'5\n15\n'
run_case exhausted-68 '68 68 68' 0 68 $'staple\nstaple\nstaple\n' $'5\n15\n'
run_case nonretry-error '42' 0 42 $'staple\n' ''
run_case cancelled-130 '130' 0 130 $'staple\n' ''
run_case terminated-143 '143' 0 143 $'staple\n' ''
run_case validation-failure '0' 9 9 $'staple\nvalidate\n' ''
run_case validation-transient-68 '0' '68 0' 0 $'staple\nvalidate\nvalidate\n' $'5\n'
run_case validation-exhausted-68 '0' '68 68 68' 68 $'staple\nvalidate\nvalidate\nvalidate\n' $'5\n15\n'
run_case validation-cancelled '0' 130 130 $'staple\nvalidate\n' ''
echo 'staple_dmg_test: ok (fake xcrun/sleep only)'
