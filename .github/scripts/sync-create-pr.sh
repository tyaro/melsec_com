#!/usr/bin/env bash
set -euo pipefail

if [ -z "${SYNC_PAT:-}" ]; then
  echo "ERROR: SYNC_PAT not set" >&2
  exit 1
fi
if [ -z "${PUSHED_BRANCH:-}" ]; then
  echo "ERROR: PUSHED_BRANCH not set" >&2
  exit 1
fi

OWNER=tyaro
REPO=melsec_mc
BRANCH=${PUSHED_BRANCH}
TITLE="Sync melsec_mc from tyaro/melsec_com ${GITHUB_SHA::7}"
BODY="Automated sync of the melsec_mc directory from tyaro/melsec_com commit ${GITHUB_SHA}."

# ensure branch exists on the remote
STATUS=$(curl -s -o /dev/null -w "%{http_code}" -H "Authorization: token ${SYNC_PAT}" "https://api.github.com/repos/${OWNER}/${REPO}/git/ref/heads/${BRANCH}")
if [ "${STATUS}" -ne 200 ]; then
  echo "Branch ${BRANCH} does not exist on ${OWNER}/${REPO}, skipping PR creation." && exit 0
fi

# fetch open PRs from this head branch
PR_JSON=$(curl -s -H "Authorization: token ${SYNC_PAT}" -H "Accept: application/vnd.github+json" "https://api.github.com/repos/${OWNER}/${REPO}/pulls?state=open&head=${OWNER}:${BRANCH}")
PR_NUMBER=$(echo "$PR_JSON" | python3 -c 'import sys,json; d=sys.stdin.read(); j=json.loads(d) if d.strip() else []; print(j[0]["number"] if j else "")')

if [ -n "${PR_NUMBER}" ]; then
  echo "Found existing PR #${PR_NUMBER}, updating title/body."
  PAYLOAD=$(mktemp)
  printf '%s' "{\"title\":\"${TITLE}\",\"body\":\"${BODY}\"}" > ${PAYLOAD}
  curl -s -X PATCH -H "Authorization: token ${SYNC_PAT}" -H "Accept: application/vnd.github+json" "https://api.github.com/repos/${OWNER}/${REPO}/pulls/${PR_NUMBER}" --data-binary @${PAYLOAD}
  echo "Updated PR #${PR_NUMBER}."
  rm -f ${PAYLOAD}
else
  echo "No existing PR found, creating a new PR: ${TITLE} -> ${OWNER}/${REPO}:main"
  PAYLOAD=$(mktemp)
  printf '%s' "{\"title\":\"${TITLE}\",\"head\":\"${BRANCH}\",\"base\":\"main\",\"body\":\"${BODY}\"}" > ${PAYLOAD}
  curl -s -X POST -H "Authorization: token ${SYNC_PAT}" -H "Accept: application/vnd.github+json" "https://api.github.com/repos/${OWNER}/${REPO}/pulls" --data-binary @${PAYLOAD}
  rm -f ${PAYLOAD}
fi
