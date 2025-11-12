#!/usr/bin/env bash
set -euo pipefail

DRY_RUN="${DRY_RUN:-0}"
GITHUB_SHA="${GITHUB_SHA:-unknown}"

# if not dry-run, SYNC_PAT is required
if [ "${DRY_RUN}" != "1" ] && [ -z "${SYNC_PAT:-}" ]; then
  echo "ERROR: SYNC_PAT not set"
  exit 1
fi

# clone target repo into 'target' directory
rm -rf target
if [ "${DRY_RUN}" = "1" ]; then
  git clone "https://github.com/tyaro/melsec_mc.git" target
else
  git clone "https://x-access-token:${SYNC_PAT}@github.com/tyaro/melsec_mc.git" target
fi
cd target
git fetch origin --prune || true

# push trace file outside repo
TMP_DIR="${RUNNER_TEMP:-/tmp}"
mkdir -p "${TMP_DIR}"
PUSH_TRACE="$(mktemp "$TMP_DIR/push-trace-XXXXXX.log")"
chmod 600 "$PUSH_TRACE" || true
echo "PUSH_TRACE=$PUSH_TRACE" >> "$GITHUB_ENV" || true
echo "=== push-trace started: $(date -u) ===" >> "$PUSH_TRACE"
echo "DEBUG: current dir: $(pwd)" >> "$PUSH_TRACE"
echo "DEBUG: created PUSH_TRACE at $PUSH_TRACE" >> "$PUSH_TRACE"

create_or_update_pr() {
  echo "DEBUG: entering create_or_update_pr" | tee -a "$PUSH_TRACE"
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

  if command -v gh >/dev/null 2>&1; then
    echo "Using gh CLI for PR operations" | tee -a "$PUSH_TRACE"
    echo "$SYNC_PAT" | gh auth login --with-token 2>&1 | tee -a "$PUSH_TRACE" || true
    gh --version 2>&1 | tee -a "$PUSH_TRACE" || true
    gh auth status 2>&1 | tee -a "$PUSH_TRACE" || true

    existing_pr=$(gh pr list --repo ${OWNER}/${REPO} --head ${OWNER}:${HEAD_BRANCH} --base ${BASE} --state open --json number -q '.[0].number' 2>/dev/null || true)
    if [ -n "$existing_pr" ]; then
      echo "Found existing PR #${existing_pr} via gh — editing" | tee -a "$PUSH_TRACE"
      gh pr edit ${existing_pr} --repo ${OWNER}/${REPO} --title "$TITLE" --body "$BODY" 2>&1 | tee -a "$PUSH_TRACE" || true
      pr_url=$(gh pr view ${existing_pr} --repo ${OWNER}/${REPO} --json url -q '.url' 2>/dev/null || true)
      echo "PR_NUMBER=${existing_pr}" >> "$GITHUB_OUTPUT" || true
      [ -n "$pr_url" ] && echo "PR_URL=${pr_url}" >> "$GITHUB_OUTPUT" || true
    else
      echo "No existing PR found via gh — creating" | tee -a "$PUSH_TRACE"
      pr_json=$(gh pr create --repo ${OWNER}/${REPO} --title "$TITLE" --body "$BODY" --base ${BASE} --head ${OWNER}:${HEAD_BRANCH} --json url,number 2>&1 | tee -a "$PUSH_TRACE" || true)
      pr_url=$(echo "$pr_json" | jq -r '.url // empty' 2>/dev/null || true)
      pr_num=$(echo "$pr_json" | jq -r '.number // empty' 2>/dev/null || true)
      if [ -n "$pr_url" ]; then
        echo "Created PR: ${pr_url} (#${pr_num})" | tee -a "$PUSH_TRACE"
        echo "PR_URL=${pr_url}" >> "$GITHUB_OUTPUT" || true
        echo "PR_NUMBER=${pr_num}" >> "$GITHUB_OUTPUT" || true
      else
        echo "gh pr create did not return a URL — falling back to API" | tee -a "$PUSH_TRACE"
      fi
    fi
  else
    echo "gh not available; will try curl-based API" | tee -a "$PUSH_TRACE"
  fi

  # Fallback / final attempt via API
  if [ -z "${pr_url:-}" ]; then
    echo "Attempting curl-based PR check/create (fallback)" | tee -a "$PUSH_TRACE"
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
        pr_url=$(echo "$pr_json" | grep -o '"html_url": *"[^"]*"' | head -1 | sed -E 's/"html_url": *"([^\"]*)"/\1/' || true)
        pr_num=$(echo "$pr_json" | grep -o '"number": *[0-9]*' | head -1 | grep -o '[0-9]*' || true)
      fi
      echo "Created PR #${pr_num}: ${pr_url}" | tee -a "$PUSH_TRACE"
      [ -n "$pr_url" ] && echo "PR_URL=${pr_url}" >> "$GITHUB_OUTPUT" || true
      [ -n "$pr_num" ] && echo "PR_NUMBER=${pr_num}" >> "$GITHUB_OUTPUT" || true
    fi
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

# sync files (exclude artifact directories and known trace files)
rsync -a --delete \
  --exclude='.github/artifacts/**' \
  --exclude='**/push-trace*' \
  --exclude='*.log' \
  --exclude='*.bak' ../melsec_mc/ .
echo "DEBUG: completed rsync, current dir $(pwd)" >> "$PUSH_TRACE"

# Safety scan: ensure we don't commit push-trace backups or token-like strings
echo "DEBUG: scanning workspace for push-trace files or token-like strings" | tee -a "$PUSH_TRACE"
# If any literal push-trace files exist in the tree, abort (print paths)
found_push_trace_files=$(find . -path './.git' -prune -o -type f -name 'push-trace*' -print | sed -n '1,5p' || true)
if [ -n "$found_push_trace_files" ]; then
  echo "ERROR: found candidate push-trace files in workspace; refusing to continue." | tee -a "$PUSH_TRACE"
  find . -path './.git' -prune -o -type f -name 'push-trace*' -print | tee -a "$PUSH_TRACE" || true
  exit 1
fi

# Grep for token-like patterns (ghp_, ghs_, x-access-token:, GITHUB_TOKEN, SYNC_PAT)
# Exclude binary/media and .git and artifact dirs from the scan
if grep -RIn --binary-files=without-match -E 'ghp_[A-Za-z0-9_]{5,}|ghs_[A-Za-z0-9_]{5,}|x-access-token:|GITHUB_TOKEN|SYNC_PAT' . \
     --exclude-dir=.git --exclude-dir=.github/artifacts --exclude='*.png' --exclude='*.jpg' --exclude='*.zip' >/dev/null 2>&1; then
  echo "ERROR: potential token-like strings found in workspace; aborting. See details below:" | tee -a "$PUSH_TRACE"
  grep -RIn --binary-files=without-match -E 'ghp_[A-Za-z0-9_]{5,}|ghs_[A-Za-z0-9_]{5,}|x-access-token:|GITHUB_TOKEN|SYNC_PAT' . \
    --exclude-dir=.git --exclude-dir=.github/artifacts --exclude='*.png' --exclude='*.jpg' --exclude='*.zip' | tee -a "$PUSH_TRACE" || true
  exit 1
fi

echo "DEBUG: completed rsync, current dir $(pwd)" >> "$PUSH_TRACE"
BRANCH=sync/melsec_mc

git fetch origin main || true
if git show-ref --verify --quiet refs/remotes/origin/main; then
  git checkout -B ${BRANCH} origin/main
else
  git checkout -B ${BRANCH}
fi

echo "DEBUG: running git status (porcelain) and listing changed files" >> "$PUSH_TRACE"
git status --porcelain >> "$PUSH_TRACE" 2>&1 || true
git diff --name-only origin/main...HEAD >> "$PUSH_TRACE" 2>&1 || true
UNTRACKED=$(git ls-files --others --exclude-standard || true)
echo "DEBUG: UNTRACKED=[$UNTRACKED]" >> "$PUSH_TRACE"
if [ -n "$UNTRACKED" ]; then
  echo "$UNTRACKED" >> "$PUSH_TRACE"
  echo "$UNTRACKED" | grep -E '\.github/artifacts|\.bak|push-trace' >/dev/null 2>&1 && {
    echo "ERROR: dangerous untracked files would be committed. Aborting." | tee -a "$PUSH_TRACE"
    exit 1
  }
fi

if [ -n "$(git status --porcelain)" ]; then
  echo "DEBUG: detected changes to commit" >> "$PUSH_TRACE"
  git add . 2>&1 | tee -a "$PUSH_TRACE" || true
  # ensure commit identity is set in the cloned target repo
  echo "DEBUG: configuring git user identity for commit" | tee -a "$PUSH_TRACE"
  git config user.name "github-actions[bot]" 2>&1 | tee -a "$PUSH_TRACE" || true
  git config user.email "41898282+github-actions[bot]@users.noreply.github.com" 2>&1 | tee -a "$PUSH_TRACE" || true
  echo "ABOUT TO COMMIT" | tee -a "$PUSH_TRACE"
  # commit safely even when GITHUB_SHA is unset
  set +e
  git commit -m "sync: update melsec_mc from tyaro/melsec_com ${GITHUB_SHA:-unknown}" 2>&1 | tee -a "$PUSH_TRACE"
  commit_status=${PIPESTATUS[0]:-0}
  set -e
  echo "COMMIT_EXIT=${commit_status}" >> "$PUSH_TRACE"
  if [ "$commit_status" -ne 0 ]; then
    echo "NOTICE: commit exited with status ${commit_status} (may be nothing to commit)" | tee -a "$PUSH_TRACE"
  fi
  git config --local --unset-all http.https://github.com/.extraheader || true
  git config --local --unset-all credential.helper || true
  # before attempting push, fetch remote branch and attempt to rebase to avoid stale-info rejects
  echo "DEBUG: fetching origin/${BRANCH} for rebase attempt" | tee -a "$PUSH_TRACE"
  git fetch origin ${BRANCH} || true
  if git rev-parse --verify --quiet refs/remotes/origin/${BRANCH} >/dev/null 2>&1; then
    echo "DEBUG: origin/${BRANCH} exists — attempting to rebase onto origin/${BRANCH}" | tee -a "$PUSH_TRACE"
    set +e
    git rebase origin/${BRANCH} 2>&1 | tee -a "$PUSH_TRACE"
    rebase_status=${PIPESTATUS[0]:-1}
    set -e
    if [ ${rebase_status} -ne 0 ]; then
      echo "ERROR: git rebase onto origin/${BRANCH} failed (status=${rebase_status}); aborting. See push-trace for details." | tee -a "$PUSH_TRACE"
      git rebase --abort >/dev/null 2>&1 || true
      # exit non-zero so CI run records failure and push is not attempted
      exit 1
    fi
  else
    echo "DEBUG: origin/${BRANCH} does not exist — skipping rebase" | tee -a "$PUSH_TRACE"
  fi

  for i in 1 2 3 4 5; do
    if [ "${DRY_RUN}" = "1" ]; then
      echo "DRY_RUN=1: skipping actual push (attempt $i)" | tee -a "$PUSH_TRACE"
      echo "(dry-run) would push branch: ${BRANCH}" | tee -a "$PUSH_TRACE"
      sleep 1
      break
    else
      set +eA
      GIT_TRACE=1 GIT_TRACE_PACKET=1 GIT_CURL_VERBOSE=1 \
        git push "https://x-access-token:${SYNC_PAT}@github.com/tyaro/melsec_mc.git" ${BRANCH} --force-with-lease 2>&1 | tee -a "$PUSH_TRACE"
      push_status=${PIPESTATUS[0]:-1}
      set -e
      if [ ${push_status} -eq 0 ]; then
        echo "push succeeded" | tee -a "$PUSH_TRACE"
        # Extra diagnostics before attempting PR creation
        echo "DEBUG: about to call create_or_update_pr (post-push)" | tee -a "$PUSH_TRACE"
        echo "DEBUG: gh exists: $(command -v gh || echo 'no')" | tee -a "$PUSH_TRACE"
        if command -v gh >/dev/null 2>&1; then
          gh --version 2>&1 | tee -a "$PUSH_TRACE" || true
          gh auth status 2>&1 | tee -a "$PUSH_TRACE" || true
        fi
        if [ -n "${SYNC_PAT:-}" ]; then
          echo "DEBUG: SYNC_PAT is set (length=${#SYNC_PAT})" >> "$PUSH_TRACE" 2>&1 || true
        else
          echo "DEBUG: SYNC_PAT is NOT set" | tee -a "$PUSH_TRACE"
        fi
        create_or_update_pr |& tee -a "$PUSH_TRACE" || true
        break
      else
        echo "push failed, retrying ($i)" | tee -a "$PUSH_TRACE"
        sleep 30
      fi
    fi
  done
  # if all retries failed in the commit-path, try a final controlled --force fallback
  if [ ${push_status:-1} -ne 0 ] && [ "${DRY_RUN}" != "1" ]; then
    echo "All --force-with-lease attempts failed after commit; attempting final fallback: git push --force" | tee -a "$PUSH_TRACE"
    set +e
    GIT_TRACE=1 GIT_TRACE_PACKET=1 GIT_CURL_VERBOSE=1 \
      git push "https://x-access-token:${SYNC_PAT}@github.com/tyaro/melsec_mc.git" ${BRANCH} --force 2>&1 | tee -a "$PUSH_TRACE"
    push_status=${PIPESTATUS[0]:-1}
    set -e
    if [ ${push_status} -eq 0 ]; then
      echo "force push succeeded" | tee -a "$PUSH_TRACE"
      create_or_update_pr |& tee -a "$PUSH_TRACE" || true
    else
      echo "final force-push ALSO failed (status=${push_status}); leaving as-is" | tee -a "$PUSH_TRACE"
    fi
  fi
  if [ "${DRY_RUN}" = "1" ]; then
    echo "PUSHED_BRANCH=${BRANCH}-dry" >> "$GITHUB_OUTPUT"
  else
    echo "PUSHED_BRANCH=${BRANCH}" >> "$GITHUB_OUTPUT"
  fi
else
  echo "DEBUG: no changes to commit path taken" >> "$PUSH_TRACE"
  # nothing to commit, only push/PR if our local branch differs from origin/${BRANCH}
  git config --local --unset-all http.https://github.com/.extraheader || true
  git config --local --unset-all credential.helper || true

  remote_sha=""
  if git rev-parse --verify --quiet refs/remotes/origin/${BRANCH} >/dev/null 2>&1; then
    remote_sha=$(git rev-parse origin/${BRANCH} 2>/dev/null || true)
  fi
  local_sha=$(git rev-parse HEAD 2>/dev/null || true)
  if [ -n "${remote_sha}" ] && [ "${local_sha}" = "${remote_sha}" ]; then
    echo "Local branch equals origin/${BRANCH}; skipping push and PR" | tee -a "$PUSH_TRACE"
    echo "PUSHED_BRANCH=${BRANCH}" >> "$GITHUB_OUTPUT"
  else
    # attempt push with lease; if it keeps failing, try a final force-push (logged)
    push_status=1
    for i in 1 2 3 4 5; do
      set +e
      GIT_TRACE=1 GIT_TRACE_PACKET=1 GIT_CURL_VERBOSE=1 \
        git push "https://x-access-token:${SYNC_PAT}@github.com/tyaro/melsec_mc.git" ${BRANCH} --force-with-lease 2>&1 | tee -a "$PUSH_TRACE"
      push_status=${PIPESTATUS[0]:-1}
      set -e
      if [ ${push_status} -eq 0 ]; then
        echo "push (forced-with-lease) succeeded or branch exists" | tee -a "$PUSH_TRACE"
        echo "DEBUG: about to call create_or_update_pr (branch-exists path)" | tee -a "$PUSH_TRACE"
        echo "DEBUG: gh exists: $(command -v gh || echo 'no')" | tee -a "$PUSH_TRACE"
        if command -v gh >/dev/null 2>&1; then
          gh --version 2>&1 | tee -a "$PUSH_TRACE" || true
          gh auth status 2>&1 | tee -a "$PUSH_TRACE" || true
        fi
        if [ -n "${SYNC_PAT:-}" ]; then
          echo "DEBUG: SYNC_PAT is set (length=${#SYNC_PAT})" >> "$PUSH_TRACE" 2>&1 || true
        else
          echo "DEBUG: SYNC_PAT is NOT set" | tee -a "$PUSH_TRACE"
        fi
        create_or_update_pr |& tee -a "$PUSH_TRACE" || true
        break
      else
        echo "push failed, retrying ($i)" | tee -a "$PUSH_TRACE"
        sleep 30
      fi
    done

    # if all retries failed, attempt one last force-push (as a controlled fallback)
    if [ ${push_status:-1} -ne 0 ] && [ "${DRY_RUN}" != "1" ]; then
      echo "All --force-with-lease attempts failed; attempting final fallback: git push --force" | tee -a "$PUSH_TRACE"
      set +e
      GIT_TRACE=1 GIT_TRACE_PACKET=1 GIT_CURL_VERBOSE=1 \
        git push "https://x-access-token:${SYNC_PAT}@github.com/tyaro/melsec_mc.git" ${BRANCH} --force 2>&1 | tee -a "$PUSH_TRACE"
      push_status=${PIPESTATUS[0]:-1}
      set -e
      if [ ${push_status} -eq 0 ]; then
        echo "force push succeeded" | tee -a "$PUSH_TRACE"
        create_or_update_pr |& tee -a "$PUSH_TRACE" || true
      else
        echo "final force-push ALSO failed (status=${push_status}); leaving as-is" | tee -a "$PUSH_TRACE"
      fi
    fi
    echo "PUSHED_BRANCH=${BRANCH}" >> "$GITHUB_OUTPUT"
  fi
fi

# final note in push trace
echo "=== push-trace finished: $(date -u) ===" >> "$PUSH_TRACE"

exit 0
