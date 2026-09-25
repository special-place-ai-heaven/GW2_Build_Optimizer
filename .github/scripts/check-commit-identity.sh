#!/usr/bin/env bash
# Metadata hygiene gate: blocks agent-added Co-authored-by trailers, non-admin
# identities and [skip ci] markers on incoming commits. It reads self-asserted
# git fields, so it is NOT cryptographic proof of authorship. The owner-side
# control is branch protection: require the `verify` check, PRs only into
# main, optionally signed commits.
# Author / person-committer / Co-authored-by must be special-place-administrator
# with one of:
#   197073597+special-place-administrator@users.noreply.github.com
#   special-place-administrator@users.noreply.github.com (legacy no-ID form)
#   administrator@special-place.online
# Committer may also be GitHub|web-flow <noreply@github.com>.
# Usage: check-commit-identity.sh <base...HEAD> | <commit>
# SELFTEST=1: clean admin passes; Cursor Co-authored-by and [skip ci] fail.
set -euo pipefail

ADMIN_NAME=special-place-administrator
EMAIL_A='197073597+special-place-administrator@users.noreply.github.com'
EMAIL_B='administrator@special-place.online'
EMAIL_C='special-place-administrator@users.noreply.github.com'
BOT_EMAIL='noreply@github.com'

admin_email() {
  case "$1" in
    "$EMAIL_A"|"$EMAIL_B"|"$EMAIL_C") return 0 ;;
    *) return 1 ;;
  esac
}

failed=0
fail() {
  printf '%s %s\n' "$1" "$2" >&2
  failed=1
}

check_one() {
  local sha="$1" an ae cn ce line val name email lines msg
  an=$(git log -1 --format='%an' "$sha")
  ae=$(git log -1 --format='%ae' "$sha")
  cn=$(git log -1 --format='%cn' "$sha")
  ce=$(git log -1 --format='%ce' "$sha")

  [[ "$an" == "$ADMIN_NAME" ]] || fail "$sha" author-name
  admin_email "$ae" || fail "$sha" author-email

  if [[ "$cn" == "GitHub" || "$cn" == "web-flow" ]]; then
    [[ "$ce" == "$BOT_EMAIL" ]] || fail "$sha" committer-email
  else
    [[ "$cn" == "$ADMIN_NAME" ]] || fail "$sha" committer-name
    admin_email "$ce" || fail "$sha" committer-email
  fi

  # a skip marker suppresses the run that would check this commit
  msg=$(git log -1 --format='%B' "$sha")
  if grep -qiE '\[(skip ci|ci skip|no ci|skip actions|actions skip)\]|^skip-checks: ?true' <<< "$msg"; then
    fail "$sha" skip-ci
  fi

  # ponytail: any Co-authored-by line, not only the trailer block — GitHub
  # attributes the trailer, a mid-message line is the same claim.
  lines=$(grep -i -E '^[[:space:]]*co-authored-by:' <<< "$msg" || true)
  while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    line=${line%$'\r'}
    val=${line#*:}
    val=${val#"${val%%[![:space:]]*}"}
    val=${val%"${val##*[![:space:]]}"}
    if [[ "$val" =~ ^(.+)[[:space:]]\<([^[:space:]<>]+)\>$ ]]; then
      name=${BASH_REMATCH[1]}
      email=${BASH_REMATCH[2]}
      name=${name%"${name##*[![:space:]]}"}
      [[ "$name" == "$ADMIN_NAME" ]] && admin_email "$email" && continue
    fi
    fail "$sha" co-authored-by
  done <<< "$lines"
}

check_range() {
  local range="$1" shas sha n=0
  if [[ "$range" == *...* ]]; then
    # ponytail: --right-only is the incoming side of base...HEAD / before...after
    shas=$(git rev-list --reverse --right-only "$range")
  elif [[ "$range" == *..* ]]; then
    echo "range must be triple-dot (base...HEAD), got $range" >&2
    exit 2
  else
    shas=$(git rev-parse --verify "${range}^{commit}")
  fi
  for sha in $shas; do
    check_one "$sha"
    n=$((n + 1))
  done
  if [[ "$failed" -ne 0 ]]; then
    exit 1
  fi
  echo "ok $range ($n)"
}

run_selftest() {
  local script dir base good bot_gh bot_wf bad prev
  script=$(cd "$(dirname "$0")" && pwd)/$(basename "$0")
  # Git Bash with MSYS_NO_PATHCONV=1 hands git.exe /tmp/x as C:/tmp/x while
  # cd uses the MSYS /tmp: the selftest would split across two dirs.
  unset MSYS_NO_PATHCONV
  dir=$(mktemp -d)
  # shellcheck disable=SC2064 # expand now: $dir is local, EXIT fires later
  trap "rm -rf '$dir'" EXIT
  git init -q -b main "$dir"
  git -C "$dir" config commit.gpgsign false

  commit_as() {
    GIT_AUTHOR_NAME=$ADMIN_NAME \
    GIT_AUTHOR_EMAIL=$1 \
    GIT_COMMITTER_NAME=${3:-$ADMIN_NAME} \
    GIT_COMMITTER_EMAIL=${4:-$1} \
    git -C "$dir" -c commit.gpgsign=false commit --allow-empty -q -m "$2"
  }
  expect_ok() {
    local rc=0
    (cd "$dir" && env -u SELFTEST bash "$script" "$1") || rc=$?
    if [[ "$rc" -ne 0 ]]; then
      echo "SELFTEST expected pass: $1" >&2
      exit 1
    fi
  }

  commit_as "$EMAIL_A" "clean A"
  base=$(git -C "$dir" rev-parse HEAD)
  expect_ok "$base"

  commit_as "$EMAIL_B" "clean B"
  good=$(git -C "$dir" rev-parse HEAD)
  expect_ok "$base...$good"

  commit_as "$EMAIL_A" "github web" "GitHub" "$BOT_EMAIL"
  bot_gh=$(git -C "$dir" rev-parse HEAD)
  expect_ok "$bot_gh"

  commit_as "$EMAIL_A" "web-flow" "web-flow" "$BOT_EMAIL"
  bot_wf=$(git -C "$dir" rev-parse HEAD)
  expect_ok "$bot_wf"
  prev=$bot_wf

  commit_as "$EMAIL_A" "$(printf 'cursor\n\nCo-authored-by: Cursor <cursoragent@cursor.com>\n')"
  bad=$(git -C "$dir" rev-parse HEAD)
  local out rc=0
  out=$(cd "$dir" && env -u SELFTEST bash "$script" "$prev...$bad" 2>&1) || rc=$?
  if [[ "$rc" -eq 0 || "$out" != "$bad co-authored-by" ]]; then
    echo "SELFTEST expected '$bad co-authored-by' (rc=$rc) got: $out" >&2
    exit 1
  fi

  commit_as "$EMAIL_C" "$(printf 'legacy noreply\n\nCo-authored-by: %s <%s>\n' "$ADMIN_NAME" "$EMAIL_C")"
  expect_ok "$(git -C "$dir" rev-parse HEAD)"

  commit_as "$EMAIL_A" "quiet [Skip CI]"
  bad=$(git -C "$dir" rev-parse HEAD)
  rc=0
  out=$(cd "$dir" && env -u SELFTEST bash "$script" "$bad" 2>&1) || rc=$?
  if [[ "$rc" -eq 0 || "$out" != "$bad skip-ci" ]]; then
    echo "SELFTEST expected '$bad skip-ci' (rc=$rc) got: $out" >&2
    exit 1
  fi
  echo "SELFTEST ok"
}

if [[ "${SELFTEST:-}" == "1" ]]; then
  run_selftest
  exit 0
fi

if [[ $# -ne 1 ]]; then
  echo "usage: check-commit-identity.sh <base...HEAD>" >&2
  exit 2
fi
check_range "$1"
