#!/usr/bin/env bash
set -eu

# TAP verification script for gh-create-history generated repositories.
# Usage: verify.sh <repo-path> --commits N --branches B --size S --oldest D
#
# Outputs TAP (Test Anything Protocol) format — 14 checks.
# Exit 0 if all pass; exit 1 on any failure.

REPO=""
COMMITS=0
BRANCHES=0
SIZE=""
OLDEST=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --commits)  COMMITS="$2";  shift 2 ;;
    --branches) BRANCHES="$2"; shift 2 ;;
    --size)     SIZE="$2";     shift 2 ;;
    --oldest)   OLDEST="$2";   shift 2 ;;
    *)
      if [[ -z "$REPO" ]]; then
        REPO="$1"; shift
      else
        echo "Unknown argument: $1" >&2; exit 2
      fi
      ;;
  esac
done

if [[ -z "$REPO" || "$COMMITS" -eq 0 || "$BRANCHES" -eq 0 || -z "$SIZE" || -z "$OLDEST" ]]; then
  echo "Usage: verify.sh <repo-path> --commits N --branches B --size S --oldest D" >&2
  exit 2
fi

# Parse --size into bytes
parse_size() {
  local raw
  raw=$(echo "$1" | tr '[:upper:]' '[:lower:]')
  local num="${raw%%[a-z]*}"
  local suffix="${raw##*[0-9]}"
  case "$suffix" in
    b)  echo "$num" ;;
    kb) echo $((num * 1024)) ;;
    mb) echo $((num * 1024 * 1024)) ;;
    gb) echo $((num * 1024 * 1024 * 1024)) ;;
    *)  echo "$num" ;;
  esac
}

# Parse --oldest into seconds
parse_oldest() {
  local raw
  raw=$(echo "$1" | tr '[:upper:]' '[:lower:]')
  local num="${raw%%[a-z]*}"
  local suffix="${raw##*[0-9]}"
  case "$suffix" in
    yr|year|years)     echo $((num * 365 * 86400)) ;;
    mo|month|months)   echo $((num * 30 * 86400)) ;;
    w|week|weeks)      echo $((num * 7 * 86400)) ;;
    d|day|days)        echo $((num * 86400)) ;;
    *)                 echo $((num * 86400)) ;;
  esac
}

SIZE_BYTES=$(parse_size "$SIZE")
WINDOW_SECS=$(parse_oldest "$OLDEST")
EXPECTED_BRANCHES=$((BRANCHES + 1))
EXPECTED_MIN_COMMITS=$((COMMITS * EXPECTED_BRANCHES))

PASS=0
FAIL=0
TOTAL=14

echo "TAP version 13"
echo "1..$TOTAL"

tap() {
  local n="$1" ok="$2" desc="$3"
  if [[ "$ok" == "true" ]]; then
    echo "ok $n - $desc"
    PASS=$((PASS + 1))
  else
    echo "not ok $n - $desc"
    FAIL=$((FAIL + 1))
  fi
}

tap_skip() {
  local n="$1" desc="$2" reason="$3"
  echo "ok $n - $desc # SKIP $reason"
  PASS=$((PASS + 1))
}

cd "$REPO"

# 1. Branch count
ACTUAL_BRANCHES=$(git branch --list | wc -l | tr -d ' ')
tap 1 "$([ "$ACTUAL_BRANCHES" -eq "$EXPECTED_BRANCHES" ] && echo true || echo false)" \
  "branch count == $EXPECTED_BRANCHES (got $ACTUAL_BRANCHES)"

# 2. Total commits
TOTAL_COMMITS=$(git rev-list --all | wc -l | tr -d ' ')
tap 2 "$([ "$TOTAL_COMMITS" -ge "$EXPECTED_MIN_COMMITS" ] && echo true || echo false)" \
  "total commits >= $EXPECTED_MIN_COMMITS (got $TOTAL_COMMITS)"

# 3. Max file size
MAX_BLOB=0
while IFS= read -r sha; do
  bsize=$(git cat-file -s "$sha" 2>/dev/null || echo 0)
  if [ "$bsize" -gt "$MAX_BLOB" ]; then
    MAX_BLOB="$bsize"
  fi
done <<< "$(git rev-list --all --objects | awk '{print $1}' | while read -r oid; do
  otype=$(git cat-file -t "$oid" 2>/dev/null)
  if [ "$otype" = "blob" ]; then echo "$oid"; fi
done)"
tap 3 "$([ "$MAX_BLOB" -le "$SIZE_BYTES" ] && echo true || echo false)" \
  "max blob size <= ${SIZE_BYTES}b (got ${MAX_BLOB}b)"

# 4. Oldest commit within window (120s tolerance)
NOW=$(date +%s)
OLDEST_TS=$(git log --all --format='%at' --reverse | head -1)
EXPECTED_OLDEST=$((NOW - WINDOW_SECS - 120))
tap 4 "$([ "$OLDEST_TS" -ge "$EXPECTED_OLDEST" ] && echo true || echo false)" \
  "oldest commit within window (120s tolerance)"

# 5. Newest commit within 10% of window or 14 days
NEWEST_TS=$(git log --all --format='%at' | head -1)
TOLERANCE=$((WINDOW_SECS / 10))
MAX_TOLERANCE=$((14 * 86400))
if [ "$TOLERANCE" -gt "$MAX_TOLERANCE" ]; then
  TOLERANCE="$MAX_TOLERANCE"
fi
NEWEST_LOWER=$((NOW - TOLERANCE))
tap 5 "$([ "$NEWEST_TS" -ge "$NEWEST_LOWER" ] && echo true || echo false)" \
  "newest commit within 10% of window or 14 days"

# 6. Merge commits > 0
MERGE_COUNT=$(git rev-list --all --merges | wc -l | tr -d ' ')
tap 6 "$([ "$MERGE_COUNT" -gt 0 ] && echo true || echo false)" \
  "merge commits > 0 (got $MERGE_COUNT)"

# 7. Octopus merges > 0
OCTOPUS_COUNT=$(git rev-list --all --min-parents=3 | wc -l | tr -d ' ')
tap 7 "$([ "$OCTOPUS_COUNT" -gt 0 ] && echo true || echo false)" \
  "octopus merges > 0 (got $OCTOPUS_COUNT)"

# 8. Tags > 0
TAG_COUNT=$(git tag --list | wc -l | tr -d ' ')
tap 8 "$([ "$TAG_COUNT" -gt 0 ] && echo true || echo false)" \
  "tags > 0 (got $TAG_COUNT)"

# 9. File renames > 0
RENAME_LINES=$(git log --all --diff-filter=R --summary --oneline | grep -c 'rename' || true)
tap 9 "$([ "$RENAME_LINES" -gt 0 ] && echo true || echo false)" \
  "file renames > 0 (got $RENAME_LINES)"

# 10. File deletes > 0
DELETE_LINES=$(git log --all --diff-filter=D --summary --oneline | grep -c 'delete' || true)
tap 10 "$([ "$DELETE_LINES" -gt 0 ] && echo true || echo false)" \
  "file deletes > 0 (got $DELETE_LINES)"

# 11. Conflict resolution commits (skip if < 20 merges)
if [ "$MERGE_COUNT" -lt 20 ]; then
  tap_skip 11 "conflict resolution commits" "fewer than 20 merges ($MERGE_COUNT)"
else
  CONFLICT_MARKERS=$(git log --all --oneline --grep='[Cc]onflict\|[Rr]esolv' | wc -l | tr -d ' ')
  tap 11 "$([ "$CONFLICT_MARKERS" -gt 0 ] && echo true || echo false)" \
    "conflict resolution commits > 0 (got $CONFLICT_MARKERS)"
fi

# 12. git fsck clean
FSCK_OUT=$(git fsck --no-progress 2>&1 || true)
FSCK_ERRORS=$(echo "$FSCK_OUT" | grep -c -E '^(error|fatal)' || true)
tap 12 "$([ "$FSCK_ERRORS" -eq 0 ] && echo true || echo false)" \
  "git fsck clean (errors: $FSCK_ERRORS)"

# 13. Commits exist (sanity)
tap 13 "$([ "$TOTAL_COMMITS" -gt 0 ] && echo true || echo false)" \
  "commits exist (got $TOTAL_COMMITS)"

# 14. Time spread >= 50% of window
if [ "$OLDEST_TS" -gt 0 ] && [ "$NEWEST_TS" -gt 0 ]; then
  ACTUAL_SPAN=$((NEWEST_TS - OLDEST_TS))
  HALF_WINDOW=$((WINDOW_SECS / 2))
  tap 14 "$([ "$ACTUAL_SPAN" -ge "$HALF_WINDOW" ] && echo true || echo false)" \
    "time spread >= 50% of window (span=${ACTUAL_SPAN}s, need=${HALF_WINDOW}s)"
else
  tap 14 "false" "time spread >= 50% of window (missing timestamps)"
fi

echo ""
echo "# Passed: $PASS / $TOTAL"
if [ "$FAIL" -gt 0 ]; then
  echo "# FAILED: $FAIL"
  exit 1
fi
exit 0
