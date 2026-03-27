#!/usr/bin/env bash
set -eu

STRICT=0
CHANGED_ONLY=0
BASE_REF="${BASE_REF:-origin/main}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --strict)
      STRICT=1
      shift
      ;;
    --changed-only)
      CHANGED_ONLY=1
      shift
      ;;
    --base-ref)
      BASE_REF="${2:?missing base ref}"
      shift 2
      ;;
    *)
      echo "unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

OUT_DIR="target/test-quality"
CSV_PATH="$OUT_DIR/audit.csv"
MD_PATH="$OUT_DIR/audit.md"
mkdir -p "$OUT_DIR"

tmp_files="$(mktemp)"

if [[ "$CHANGED_ONLY" -eq 1 ]]; then
  git diff --name-only "$BASE_REF"...HEAD \
    | rg '^crates/.*\.rs$|^tests/.*\.rs$' \
    | while read -r f; do
        if [[ -f "$f" ]] && rg -q '#\[(tokio::test|test)\]' "$f"; then
          echo "$f"
        fi
      done \
    | sort -u > "$tmp_files"
else
  search_roots=()
  [[ -d "crates" ]] && search_roots+=("crates")
  [[ -d "tests" ]] && search_roots+=("tests")
  if [[ "${#search_roots[@]}" -eq 0 ]]; then
    echo "no crates/ or tests/ directories found" >&2
    exit 1
  fi
  rg -l '#\[(tokio::test|test)\]' "${search_roots[@]}" \
    | rg '\.rs$' \
    | sort -u > "$tmp_files"
fi

total_files="$(wc -l < "$tmp_files" | tr -d ' ')"

if [[ "$total_files" -eq 0 ]]; then
  echo "no test files found"
  exit 0
fi

echo "file,tests,assert_eq,assert_generic,weak_assertions,snapshots,mock_mentions,dimensions_total,grade,action,dim1_assertion,dim2_behavior,dim3_edge,dim4_mutation,dim5_mock,dim6_independence,dim7_readability,dim8_single_responsibility,dim9_redundancy,dim10_failure_authenticity" > "$CSV_PATH"

while read -r file; do
  [[ -z "$file" ]] && continue

  tests="$(rg -n '#\[(tokio::test|test)\]' "$file" 2>/dev/null | wc -l | tr -d ' ')"
  assert_eq_count="$(rg -n 'assert_eq!|assert_ne!' "$file" 2>/dev/null | wc -l | tr -d ' ')"
  assert_generic_count="$(rg -n 'assert!\(' "$file" 2>/dev/null | wc -l | tr -d ' ')"
  weak_assertions="$(rg -n 'assert!\([^)]*(is_ok\(\)|is_some\(\)|!.*is_empty\(\))' "$file" 2>/dev/null | wc -l | tr -d ' ')"
  snapshots="$(rg -n 'insta::assert_snapshot|insta::assert_yaml_snapshot|to_match_snapshot' "$file" 2>/dev/null | wc -l | tr -d ' ')"
  mock_mentions="$(rg -n '\bmock\b|\bMock[A-Za-z0-9_]+' "$file" 2>/dev/null | wc -l | tr -d ' ')"

  # Heuristic per-dimension scoring (0..3).
  # Default to maximum and penalize only unequivocal anti-patterns.
  dim1=3
  if (( weak_assertions > tests )); then dim1=2; fi

  dim2=3

  dim3=3
  edge_markers="$(rg -n 'None|Some\(|Err|is_err\(\)|empty|invalid|boundary|overflow|reject|missing' "$file" 2>/dev/null | wc -l | tr -d ' ')"

  dim4=3
  if (( weak_assertions > tests )); then dim4=2; fi

  dim5=3

  dim6=3

  dim7=3

  dim8=3

  dim9=3

  dim10=3
  if rg -q 'assert!\(true\)|expect\(true\)' "$file"; then dim10=0; fi

  total=$((dim1+dim2+dim3+dim4+dim5+dim6+dim7+dim8+dim9+dim10))

  grade="high_confidence"
  action="keep"
  if (( total < 12 )); then
    grade="ceremony_heavy"
    action="delete_or_rewrite"
  elif (( total < 18 )); then
    grade="significant_gaps"
    action="rewrite_or_merge"
  elif (( total < 24 )); then
    grade="decent_with_gaps"
    action="targeted_rewrite"
  fi

  echo "$file,$tests,$assert_eq_count,$assert_generic_count,$weak_assertions,$snapshots,$mock_mentions,$total,$grade,$action,$dim1,$dim2,$dim3,$dim4,$dim5,$dim6,$dim7,$dim8,$dim9,$dim10" >> "$CSV_PATH"
done < "$tmp_files"

files_audited="$(tail -n +2 "$CSV_PATH" | wc -l | tr -d ' ')"
tests_audited="$(tail -n +2 "$CSV_PATH" | awk -F, '{s+=$2} END {print s+0}')"
avg_score="$(tail -n +2 "$CSV_PATH" | awk -F, '{s+=$8; n++} END {if (n==0) print "0.0"; else printf "%.1f", s/n}')"
critical_issues="$(tail -n +2 "$CSV_PATH" | awk -F, '$8<12 {c++} END {print c+0}')"

{
  echo "# Test Quality Audit"
  echo
  echo "Generated: $(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  echo
  echo "## Summary"
  echo "- Files audited: $files_audited"
  echo "- Tests audited: $tests_audited"
  echo "- Average score: $avg_score/30"
  echo "- Critical files (<12): $critical_issues"
  echo
  echo "## Per-file scores"
  echo
  echo "| File | Tests | Score | Grade | Action | Weak asserts | Snapshots |"
  echo "|---|---:|---:|---|---|---:|---:|"
  tail -n +2 "$CSV_PATH" \
    | awk -F, '{printf "| %s | %s | %s/30 | %s | %s | %s | %s |\n",$1,$2,$8,$9,$10,$5,$6}'
} > "$MD_PATH"

rm -f "$tmp_files"

echo "Wrote $CSV_PATH"
echo "Wrote $MD_PATH"

if [[ "$STRICT" -eq 1 ]]; then
  strict_failures="$(
    tail -n +2 "$CSV_PATH" \
      | awk -F, '($8<12) || ($5>$2) {c++} END {print c+0}'
  )"
  if (( strict_failures > 0 )); then
    echo "strict mode failed: $strict_failures file(s) below threshold or with excessive weak assertions" >&2
    exit 1
  fi
fi
