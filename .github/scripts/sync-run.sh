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

# helper: create or update PR on target repo
create_or_update_pr() {
  if [ "${DRY_RUN}" = "1" ]; then
    echo "DRY_RUN=1: skipping PR create/update" | tee -a "$PUSH_TRACE"
    return
  fi

  OWNER="tyaro"
  REPO="melsec_mc"
  HEAD_BRANCH="${BRANCH}"
  BASE="main"
  TITLE="sync: update melsec_mc from tyaro/melsec_com ${GITHUB_SHA}"
  BODY="Automated sync from tyaro/melsec_com (${GITHUB_SHA}). See push-trace for details."

  echo "Checking for existing PR (head=${OWNER}:${HEAD_BRANCH}, base=${BASE})" | tee -a "$PUSH_TRACE"

  if command -v jq >/dev/null 2>&1; then
    existing_pr_json=$(curl -sS -H "Authorization: token ${SYNC_PAT}" -H "Accept: application/vnd.github+json" \
      "https://api.github.com/repos/${OWNER}/${REPO}/pulls?head=${OWNER}:${HEAD_BRANCH}&base=${BASE}&state=open" 2>&1 | tee -a "$PUSH_TRACE" ) || true
    existing_pr=$(echo "$existing_pr_json" | jq -r '.[0].number // empty' || true)
  else
    existing_pr_json=$(curl -sS -H "Authorization: token ${SYNC_PAT}" -H "Accept: application/vnd.github+json" \
      "https://api.github.com/repos/${OWNER}/${REPO}/pulls?head=${OWNER}:${HEAD_BRANCH}&base=${BASE}&state=open" 2>&1 | tee -a "$PUSH_TRACE" ) || true
    existing_pr=$(echo "$existing_pr_json" | grep -o '"number": [0-9]*' | head -1 | grep -o '[0-9]*' || true)
  fi

  if [ -n "$existing_pr" ]; then
    echo "Found existing PR #$existing_pr — updating title/body" | tee -a "$PUSH_TRACE"
    if command -v jq >/dev/null 2>&1; then
      curl -sS -X PATCH -H "Authorization: token ${SYNC_PAT}" -H "Accept: application/vnd.github+json" \
        "https://api.github.com/repos/${OWNER}/${REPO}/pulls/${existing_pr}" \
        -d "$(jq -n --arg t "$TITLE" --arg b "$BODY" '{title:$t, body:$b}')" 2>&1 | tee -a "$PUSH_TRACE" || true
    else
      curl -sS -X PATCH -H "Authorization: token ${SYNC_PAT}" -H "Accept: application/vnd.github+json" \
        "https://api.github.com/repos/${OWNER}/${REPO}/pulls/${existing_pr}" \
        -d "{\"title\": \"${TITLE}\", \"body\": \"${BODY}\"}" 2>&1 | tee -a "$PUSH_TRACE" || true
    fi
    echo "PR_NUMBER=${existing_pr}" >> "$GITHUB_OUTPUT" || true
  else
    echo "No existing PR — creating new PR ${HEAD_BRANCH} -> ${BASE}" | tee -a "$PUSH_TRACE"
    if command -v jq >/dev/null 2>&1; then
      pr_json=$(curl -sS -X POST -H "Authorization: token ${SYNC_PAT}" -H "Accept: application/vnd.github+json" \
        "https://api.github.com/repos/${OWNER}/${REPO}/pulls" \
        -d "$(jq -n --arg t "$TITLE" --arg b "$BODY" --arg head "${OWNER}:${HEAD_BRANCH}" --arg base "$BASE" '{title:$t, body:$b, head:$head, base:$base}')" 2>&1 | tee -a "$PUSH_TRACE" ) || true
      pr_url=$(echo "$pr_json" | jq -r '.html_url // empty' || true)
      pr_num=$(echo "$pr_json" | jq -r '.number // empty' || true)
    else
      pr_json=$(curl -sS -X POST -H "Authorization: token ${SYNC_PAT}" -H "Accept: application/vnd.github+json" \
        "https://api.github.com/repos/${OWNER}/${REPO}/pulls" \
        -d "{\"title\": \"${TITLE}\", \"body\": \"${BODY}\", \"head\": \"${OWNER}:${HEAD_BRANCH}\", \"base\": \"${BASE}\"}" 2>&1 | tee -a "$PUSH_TRACE" ) || true
      pr_url=$(echo "$pr_json" | grep -o '"html_url": *"[^"]*"' | head -1 | sed -E 's/"html_url": *"([^"]*)"/\1/' || true)
      pr_num=$(echo "$pr_json" | grep -o '"number": *[0-9]*' | head -1 | grep -o '[0-9]*' || true)
    fi
    echo "Created PR #${pr_num}: ${pr_url}" | tee -a "$PUSH_TRACE"
    [ -n "$pr_url" ] && echo "PR_URL=${pr_url}" >> "$GITHUB_OUTPUT" || true
    [ -n "$pr_num" ] && echo "PR_NUMBER=${pr_num}" >> "$GITHUB_OUTPUT" || true
  fi
}

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
        git push "https://x-access-token:${SYNC_PAT}@github.com/tyaro/melsec_mc.git" ${BRANCH} --force-with-lease 2>&1 | tee -a 
"$PUSH_TRACE"
  if [ ${PIPESTATUS[0]} -eq 0 ]; then
  echo "push succeeded" | tee -a "$PUSH_TRACE"
  # create or update PR after successful push
  create_or_update_pr || true
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
          # ensure PR exists for the branch
          create_or_update_pr || true
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
