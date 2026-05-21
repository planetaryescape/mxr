#!/usr/bin/env bash
set -euo pipefail

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
trap 'rm -f "$tmp_files"' EXIT

if [[ "$CHANGED_ONLY" -eq 1 ]]; then
  git diff --name-only "$BASE_REF"...HEAD \
    | { rg '^(crates|tests)/.*\.rs$|^apps/web/src/.*\.test\.(ts|tsx)$|^apps/web/e2e/.*\.spec\.ts$' || true; } \
    | while read -r f; do
        if [[ -f "$f" ]] && rg -q '#\[(tokio::test|test)\]|\b(test|it)\(' "$f"; then
          echo "$f"
        fi
      done \
    | sort -u > "$tmp_files"
else
  search_roots=()
  [[ -d "crates" ]] && search_roots+=("crates")
  [[ -d "tests" ]] && search_roots+=("tests")
  [[ -d "apps/web/src" ]] && search_roots+=("apps/web/src")
  [[ -d "apps/web/e2e" ]] && search_roots+=("apps/web/e2e")
  if [[ "${#search_roots[@]}" -eq 0 ]]; then
    echo "no Rust or web test roots found" >&2
    exit 1
  fi
  { rg -l '#\[(tokio::test|test)\]|\b(test|it)\(' "${search_roots[@]}" || true; } \
    | rg '\.(rs|ts|tsx)$' \
    | rg '^crates/|^tests/|^apps/web/src/.*\.test\.(ts|tsx)$|^apps/web/e2e/.*\.spec\.ts$' \
    | rg -v '/node_modules/' \
    | sort -u > "$tmp_files"
fi

total_files="$(wc -l < "$tmp_files" | tr -d ' ')"

if [[ "$total_files" -eq 0 ]]; then
  echo "no test files found"
  exit 0
fi

csv_escape() {
  local value="${1//\"/\"\"}"
  printf '"%s"' "$value"
}

count_rg() {
  local pattern="$1"
  local file="$2"
  { rg -n "$pattern" "$file" 2>/dev/null || true; } | wc -l | tr -d ' '
}

score_grade() {
  local score="$1"
  if (( score < 12 )); then
    echo "ceremony_heavy"
  elif (( score < 18 )); then
    echo "significant_gaps"
  elif (( score < 24 )); then
    echo "decent_with_gaps"
  else
    echo "high_confidence"
  fi
}

score_action() {
  local score="$1"
  if (( score < 12 )); then
    echo "delete_or_rewrite"
  elif (( score < 18 )); then
    echo "rewrite_or_merge"
  elif (( score < 24 )); then
    echo "targeted_rewrite"
  else
    echo "keep"
  fi
}

test_area() {
  local file="$1"
  case "$file" in
    crates/*) cut -d/ -f2 <<< "$file" ;;
    apps/web/src/*) echo "web-src" ;;
    apps/web/e2e/*) echo "web-e2e" ;;
    tests/*) echo "workspace-tests" ;;
    *) echo "other" ;;
  esac
}

echo "file,area,tests,assertions,weak_assertions,snapshots,mock_mentions,broad_mocks,broad_lint_allows,behavior_markers,edge_markers,score,grade,action,dim1_assertion,dim2_behavior,dim3_edge,dim4_mutation,dim5_mock,dim6_independence,dim7_readability,dim8_single_responsibility,dim9_redundancy,dim10_failure_authenticity" > "$CSV_PATH"

while read -r file; do
  [[ -z "$file" ]] && continue

  area="$(test_area "$file")"
  if [[ "$file" == *.rs ]]; then
    tests="$(count_rg '#\[(tokio::test|test)\]' "$file")"
  else
    tests="$(count_rg '\b(test|it)\(' "$file")"
  fi
  if (( tests == 0 )); then
    continue
  fi

  assertions="$(count_rg 'assert(_eq|_ne)?!|assert_matches!|expect\(|to(Be|Equal|Have|Contain|Match|BeVisible|HaveText|HaveCount|BeTruthy|BeFalsy|BeDisabled|BeEnabled)' "$file")"
  weak_assertions="$(count_rg 'assert!\([^)]*(is_ok\(\)|is_some\(\)|!.*is_empty\(\))|toBeTruthy\(\)|toBeFalsy\(\)' "$file")"
  snapshots="$(count_rg 'insta::assert_snapshot|insta::assert_yaml_snapshot|assert_json_snapshot|toMatchSnapshot|to_match_snapshot' "$file")"
  mock_mentions="$(count_rg '\bmock\b|\bMock[A-Za-z0-9_]+|vi\.mock|jest\.mock|mockResolvedValue|mockImplementation|toHaveBeenCalled' "$file")"
  broad_mocks="$(count_rg 'vi\.mock\("|jest\.mock\("' "$file")"
  broad_lint_allows="$(count_rg '#!\[allow\(clippy::(panic|unwrap_used)|#\[allow\(clippy::(panic|unwrap_used)' "$file")"
  behavior_markers="$(count_rg '\b(Given|When|Then|Regression|dry-run|roundtrip|round-trip|persist|revert|reject|error|fails|fallback|restores|reflects|observable|user|daemon|CLI|JSON|provider|sync|search)\b' "$file")"
  edge_markers="$(count_rg '\b(None|Some\(|Err|is_err\(\)|empty|invalid|missing|duplicate|overflow|boundary|timeout|disabled|expired|stale|failure|fallback|reject|unauthorized|offline|reconnect|malformed)\b' "$file")"
  vacuous_assertions="$(count_rg 'assert!\(true\)|expect\(true\)|toBe\(true\).*//.*placeholder' "$file")"
  call_assertions="$(count_rg 'toHaveBeenCalled|mock\.calls|\.calls\[' "$file")"
  sleeps_or_globals="$(count_rg 'thread::sleep|tokio::time::sleep|set_var|remove_var|process\.env|Date\.now|Math\.random' "$file")"
  line_count="$(wc -l < "$file" | tr -d ' ')"
  avg_lines=$(( (line_count + tests - 1) / tests ))

  dim1=3
  if (( assertions == 0 )); then dim1=1; fi
  if (( weak_assertions > 0 && weak_assertions * 2 >= assertions )); then dim1=$(( dim1 > 1 ? dim1 - 1 : dim1 )); fi
  if (( vacuous_assertions > 0 )); then dim1=0; fi

  dim2=3
  if (( behavior_markers == 0 && tests > 2 )); then dim2=2; fi
  if (( call_assertions > 0 && call_assertions * 2 >= assertions )); then dim2=$(( dim2 > 1 ? dim2 - 1 : dim2 )); fi

  dim3=3
  if (( edge_markers == 0 && tests > 3 )); then dim3=1; fi
  if (( edge_markers == 0 && tests <= 3 )); then dim3=2; fi

  dim4=3
  if (( weak_assertions > 0 || snapshots > tests )); then dim4=2; fi
  if (( vacuous_assertions > 0 )); then dim4=0; fi

  dim5=3
  if (( mock_mentions > tests )); then dim5=2; fi
  if (( mock_mentions > tests * 3 || broad_mocks > tests )); then dim5=1; fi
  if (( assertions == 0 && mock_mentions > 0 )); then dim5=0; fi

  dim6=3
  if (( sleeps_or_globals > 0 )); then dim6=2; fi
  if (( sleeps_or_globals > tests )); then dim6=1; fi

  dim7=3
  if (( behavior_markers == 0 && assertions < tests )); then dim7=2; fi
  if (( assertions == 0 )); then dim7=1; fi

  dim8=3
  if (( avg_lines > 90 )); then dim8=2; fi
  if (( avg_lines > 150 )); then dim8=1; fi

  dim9=3
  if (( snapshots > tests / 2 && tests > 5 )); then dim9=2; fi
  if (( snapshots > tests )); then dim9=1; fi

  dim10=3
  if (( assertions == 0 || vacuous_assertions > 0 )); then dim10=0; fi
  if (( weak_assertions > tests )); then dim10=$(( dim10 > 1 ? dim10 - 1 : dim10 )); fi
  if (( vacuous_assertions > 0 )); then
    dim1=0
    dim2=0
    dim3=0
    dim4=0
    dim7=0
    dim8=1
    dim9=1
    dim10=0
  fi

  total=$((dim1+dim2+dim3+dim4+dim5+dim6+dim7+dim8+dim9+dim10))
  grade="$(score_grade "$total")"
  action="$(score_action "$total")"

  {
    csv_escape "$file"; printf ","
    csv_escape "$area"; printf ","
    printf "%s,%s,%s,%s,%s,%s,%s,%s,%s,%s," \
      "$tests" "$assertions" "$weak_assertions" "$snapshots" "$mock_mentions" \
      "$broad_mocks" "$broad_lint_allows" "$behavior_markers" "$edge_markers" "$total"
    csv_escape "$grade"; printf ","
    csv_escape "$action"; printf ","
    printf "%s,%s,%s,%s,%s,%s,%s,%s,%s,%s\n" \
      "$dim1" "$dim2" "$dim3" "$dim4" "$dim5" "$dim6" "$dim7" "$dim8" "$dim9" "$dim10"
  } >> "$CSV_PATH"
done < "$tmp_files"

files_audited="$(tail -n +2 "$CSV_PATH" | wc -l | tr -d ' ')"
tests_audited="$(tail -n +2 "$CSV_PATH" | awk -F, '{s+=$3} END {print s+0}')"
avg_score="$(tail -n +2 "$CSV_PATH" | awk -F, '{s+=$12; n++} END {if (n==0) print "0.0"; else printf "%.1f", s/n}')"
critical_issues="$(tail -n +2 "$CSV_PATH" | awk -F, '$12<12 {c++} END {print c+0}')"
strict_issues="$(tail -n +2 "$CSV_PATH" | awk -F, '($12<18) || ($9>0) {c++} END {print c+0}')"

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
  echo "- Strict issues (<18 or broad lint allows): $strict_issues"
  echo
  echo "## Area summary"
  echo
  echo "| Area | Files | Tests | Avg score | Weak asserts | Snapshots | Mock mentions |"
  echo "|---|---:|---:|---:|---:|---:|---:|"
  tail -n +2 "$CSV_PATH" \
    | awk -F, '{files[$2]++; tests[$2]+=$3; score[$2]+=$12; weak[$2]+=$5; snaps[$2]+=$6; mocks[$2]+=$7} END {for (area in files) printf "| %s | %d | %d | %.1f/30 | %d | %d | %d |\n", area, files[area], tests[area], score[area]/files[area], weak[area], snaps[area], mocks[area]}' \
    | sort
  echo
  echo "## Lowest scores"
  echo
  echo "| File | Area | Tests | Score | Grade | Action | Weak | Snapshots | Mocks |"
  echo "|---|---|---:|---:|---|---|---:|---:|---:|"
  tail -n +2 "$CSV_PATH" \
    | sort -t, -k12,12n -k1,1 \
    | head -40 \
    | awk -F, '{printf "| %s | %s | %s | %s/30 | %s | %s | %s | %s | %s |\n",$1,$2,$3,$12,$13,$14,$5,$6,$7}'
} > "$MD_PATH"

echo "Wrote $CSV_PATH"
echo "Wrote $MD_PATH"

if [[ "$STRICT" -eq 1 ]]; then
  if (( strict_issues > 0 )); then
    echo "strict mode failed: $strict_issues file(s) below threshold or with broad lint allows" >&2
    exit 1
  fi
fi
