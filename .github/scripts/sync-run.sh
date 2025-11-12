#!/usr/bin/env bash
set -euo pipefail

DRY_RUN="${DRY_RUN:-0}"

# if not dry-run, SYNC_PAT is required
if [ "${DRY_RUN}" != "1" ] && [ -z "${SYNC_PAT:-}" ]; then
  echo "ERROR: SYNC_PAT not set"
  exit 1
fi

# clone target repo into 'target' directory
rm -rf target
if [ "${DRY_RUN}" = "1" ]; then
  # read-only clone for dry-run (no token)
  git clone "https://github.com/tyaro/melsec_mc.git" target
else
  git clone "https://x-access-token:${SYNC_PAT}@github.com/tyaro/melsec_mc.git" target
fi
cd target
git fetch origin --prune

# push trace file outside repo
TMP_DIR="${RUNNER_TEMP:-/tmp}"
mkdir -p "${TMP_DIR}"
PUSH_TRACE="$(mktemp "$TMP_DIR/push-trace-XXXXXX.log")"
chmod 600 "$PUSH_TRACE" || true
echo "PUSH_TRACE=$PUSH_TRACE" >> "$GITHUB_ENV"
echo "=== push-trace started: $(date -u) ===" >> "$PUSH_TRACE"

echo "DEBUG: current dir: $(pwd)" >> "$PUSH_TRACE"

# ensure source checkout one level up is not shallow
if git -C .. rev-parse --is-shallow-repository >/dev/null 2>&1; then
  IS_SHALLOW=$(git -C .. rev-parse --is-shallow-repository || true)
  if [ "$IS_SHALLOW" = "true" ]; then
    echo "source repo is shallow; attempting unshallow" >> "$PUSH_TRACE" || true
    git -C .. fetch --prune --unshallow || true
  fi
fi

# sync files
rsync -a --delete --exclude='.github/artifacts' --exclude='*.log' --exclude='*.bak' ../melsec_mc/ .
BRANCH=sync/melsec_mc

git fetch origin main || true
if git show-ref --verify --quiet refs/remotes/origin/main; then
  git checkout -B ${BRANCH} origin/main
else
  git checkout -B ${BRANCH}
fi

UNTRACKED=$(git ls-files --others --exclude-standard || true)
if [ -n "$UNTRACKED" ]; then
  echo "$UNTRACKED" >> "$PUSH_TRACE"
  echo "$UNTRACKED" | grep -E '\.github/artifacts|\.bak|push-trace' >/dev/null 2>&1 && {
    echo "ERROR: dangerous untracked files would be committed. Aborting." | tee -a "$PUSH_TRACE"
    exit 1
  }
fi

if [ -n "$(git status --porcelain)" ]; then
  git add .
  git commit -m "sync: update melsec_mc from tyaro/melsec_com ${GITHUB_SHA}"
  git config --local --unset-all http.https://github.com/.extraheader || true
  git config --local --unset-all credential.helper || true
  for i in 1 2 3 4 5; do
    if [ "${DRY_RUN}" = "1" ]; then
      echo "DRY_RUN=1: skipping actual push (attempt $i)" | tee -a "$PUSH_TRACE"
      echo "(dry-run) would push branch: ${BRANCH}" | tee -a "$PUSH_TRACE"
      sleep 1
      break
    else
      GIT_TRACE=1 GIT_TRACE_PACKET=1 GIT_CURL_VERBOSE=1 \
        git push "https://x-access-token:${SYNC_PAT}@github.com/tyaro/melsec_mc.git" ${BRANCH} --force-with-lease 2>&1 | tee -a "$PUSH_TRACE"
      if [ ${PIPESTATUS[0]} -eq 0 ]; then
        echo "push succeeded" | tee -a "$PUSH_TRACE"
        break
      else
        echo "push failed, retrying ($i)" | tee -a "$PUSH_TRACE"
        sleep 30
      fi
    fi
  done
  if [ "${DRY_RUN}" = "1" ]; then
    echo "PUSHED_BRANCH=${BRANCH}-dry" >> "$GITHUB_OUTPUT"
  else
    echo "PUSHED_BRANCH=${BRANCH}" >> "$GITHUB_OUTPUT"
  fi
else
  # nothing to commit, just ensure branch exists on remote
  git config --local --unset-all http.https://github.com/.extraheader || true
  git config --local --unset-all credential.helper || true
  for i in 1 2 3 4 5; do
    GIT_TRACE=1 GIT_TRACE_PACKET=1 GIT_CURL_VERBOSE=1 \
      git push "https://x-access-token:${SYNC_PAT}@github.com/tyaro/melsec_mc.git" ${BRANCH} --force-with-lease 2>&1 | tee -a "$PUSH_TRACE"
    if [ ${PIPESTATUS[0]} -eq 0 ]; then
      echo "push (forced) succeeded or branch exists" | tee -a "$PUSH_TRACE"
      break
    else
      echo "push failed, retrying ($i)" | tee -a "$PUSH_TRACE"
      sleep 30
    fi
  done
  echo "PUSHED_BRANCH=${BRANCH}" >> "$GITHUB_OUTPUT"
fi

# final note in push trace
echo "=== push-trace finished: $(date -u) ===" >> "$PUSH_TRACE"

exit 0
