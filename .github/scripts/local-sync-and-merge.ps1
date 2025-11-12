<#
local-sync-and-merge.ps1

Usage (example):
  $env:TARGET_PAT = Read-Host -AsSecureString | ConvertFrom-SecureString
  $env:TARGET_PAT = '<your token>'
  $env:SRC_PAT = '<your token for source push if needed>'
  pwsh ./.github/scripts/local-sync-and-merge.ps1 -Branch sync/melsec_mc -Message "ci: sync melsec_mc"

What it does:
 - Ensures working tree is clean (or optionally commits staged changes)
 - Creates a branch (by default `sync/melsec_mc` or a timestamped variant)
 - Pushes that branch to source repo (origin) and to target repo (`tyaro/melsec_mc`) in parallel
 - Creates or updates an open PR on the target repo from that branch into `main`
 - Optionally merges the PR automatically

Requirements:
 - PowerShell (pwsh)
 - `git` available in PATH
 - `curl` (built-in on Windows 10+) or Invoke-RestMethod
 - Environment variables set: TARGET_PAT (token with repo push + PR permissions on tyaro/melsec_mc)
   Optionally SRC_PAT (for pushing to source over HTTPS) if your `origin` isn't already auth'd.

Notes:
 - This script uses the GitHub REST API for PR creation/merge using TARGET_PAT so it does not rely on `gh auth` state.
 - It will not overwrite `main` on either repo; it creates a branch and opens a PR.
 - Use with care. Review the created PR before merge if you want manual control.
#>

param(
    [string]$Branch = 'sync/melsec_mc',
    [string]$Message = "ci: sync melsec_mc",
    [switch]$ForceBranch,
    [switch]$AutoMerge
)

function Abort($msg) {
    Write-Error $msg
    exit 1
}

# 1) Safety checks
$cwd = (Get-Location).Path
Write-Output "Working directory: $cwd"
# Ensure inside a git repo
if (-not (git rev-parse --is-inside-work-tree 2>$null)) {
    Abort "Not inside a git repository. cd to your repo root and re-run."
}

# Ensure clean index or at least staged/committed
$status = git status --porcelain
if ($status) {
    Write-Output "Working tree has changes:\n$status"
    Write-Output "Please commit or stash changes before running this script, or stage and set -Message to commit them now." 
    Read-Host -Prompt "Press Enter to abort (or Ctrl+C to cancel)"
    Abort "Aborting due to uncommitted changes."
}

# 2) Prepare branch
$timestamp = Get-Date -Format 'yyyyMMdd-HHmmss'
if ($Branch -eq 'sync/melsec_mc') {
    # make branch deterministic (but include timestamp as optional)
    $branchName = $Branch
} else {
    $branchName = $Branch
}

# Create or update branch locally
Write-Output "Creating/updating branch $branchName locally"
# Checkout main first to ensure up-to-date
git fetch origin --prune
git checkout main
git pull --ff-only origin main

# create branch (or move it)
if (git rev-parse --verify $branchName 2>$null) {
    Write-Output "Branch $branchName exists locally; resetting it to HEAD"
    git branch -f $branchName HEAD
} else {
    git checkout -b $branchName
}

# Ensure branch points to current HEAD
git checkout $branchName

# 3) Push to both remotes in parallel
# Source push: origin by default (use SRC_PAT if needed)
$originUrl = (git remote get-url origin) -replace '\s+$',''
Write-Output "Origin URL: $originUrl"

# Prepare target remote URL using TARGET_PAT
if (-not $env:TARGET_PAT) {
    Write-Output "Environment variable TARGET_PAT is not set. The script cannot push to the target repo without it."
    Read-Host -Prompt "Press Enter to abort"
    Abort "Missing TARGET_PAT"
}
$targetRepo = 'tyaro/melsec_mc'
$targetUrl = "https://x-access-token:$($env:TARGET_PAT)@github.com/$targetRepo.git"

# Add temporary remote 'sync-target' (unique name) if not present
$syncRemote = "sync-target"
if (git remote get-url $syncRemote 2>$null) {
    git remote remove $syncRemote
}
git remote add $syncRemote $targetUrl

# Push commands
$pushOriginCmd = {
    param($branch)
    Write-Output "Pushing to origin -> $branch"
    git push origin "HEAD:refs/heads/$branch" --force-with-lease
}
$pushTargetCmd = {
    param($syncRemote, $branch)
    Write-Output "Pushing to target remote ($syncRemote) -> $branch"
    git push $syncRemote "HEAD:refs/heads/$branch" --force
}

# Start both pushes in background jobs
Write-Output "Starting parallel pushes..."
$jobOrigin = Start-Job -ScriptBlock $pushOriginCmd -ArgumentList $branchName
$jobTarget = Start-Job -ScriptBlock $pushTargetCmd -ArgumentList $syncRemote,$branchName

# Wait for both
Wait-Job -Job $jobOrigin,$jobTarget | Out-Null
$originRes = Receive-Job $jobOrigin -ErrorAction SilentlyContinue
$targetRes = Receive-Job $jobTarget -ErrorAction SilentlyContinue

Write-Output "Origin push result:`n$originRes"
Write-Output "Target push result:`n$targetRes"

# Check exit status via git ls-remote to ensure branch exists on target
$ref = git ls-remote $syncRemote refs/heads/$branchName 2>$null
if (-not $ref) {
    Abort "Push to target failed or branch not found on target. Review the push output above."
}

# 4) Create or update PR on target via GitHub REST API
$apiBase = "https://api.github.com/repos/$targetRepo"
$headers = @{ Authorization = "token $($env:TARGET_PAT)"; Accept = 'application/vnd.github+json' }

# Check for existing open PR with this head
$owner = 'tyaro'
# If we pushed directly to target repo, head is simply the branch name
$headSpec = $branchName

Write-Output "Checking for existing PR on $targetRepo with head=$headSpec"
$existing = Invoke-RestMethod -Uri "$apiBase/pulls?state=open&head=$owner`:$headSpec" -Headers $headers -Method Get

if ($existing -and $existing.Count -gt 0) {
    $pr = $existing[0]
    Write-Output "Found existing PR #$($pr.number): $($pr.html_url)"
    # Optionally update title/body
    $prNumber = $pr.number
} else {
    Write-Output "Creating new PR on $targetRepo"
    $body = @{ title = $Message; head = $headSpec; base = 'main'; body = "Automated sync from melsec_com: $Message" }
    $json = $body | ConvertTo-Json -Depth 6
    try {
        $pr = Invoke-RestMethod -Uri "$apiBase/pulls" -Headers $headers -Method Post -Body $json
    $prNumber = $pr.number
    Write-Output "Created PR #$($prNumber): $($pr.html_url)"
    } catch {
        Abort "Failed creating PR: $($_.Exception.Message)"
    }
}

# 5) Optionally merge the PR
if ($AutoMerge) {
    Write-Output "Merging PR #$prNumber"
    $mergeBody = @{ commit_title = "Merge: sync $branchName"; merge_method = "merge" } | ConvertTo-Json
    try {
        $mergeRes = Invoke-RestMethod -Uri "$apiBase/pulls/$prNumber/merge" -Headers $headers -Method Put -Body $mergeBody
        if ($mergeRes.merged -eq $true) {
            Write-Output "PR #$prNumber merged successfully (message: $($mergeRes.message))"
            # Optionally delete branch on remote
            Write-Output "Deleting branch $branchName on target remote"
            git push $syncRemote --delete $branchName
        } else {
            Write-Output "PR not merged: $($mergeRes.message)"
        }
    } catch {
        Write-Error "Merge failed: $($_.Exception.Message)"
    }
} else {
    Write-Output "Auto-merge not requested. Please review PR #$($prNumber): $($pr.html_url)"
}

Write-Output "Done. Clean up: removing temporary remote $syncRemote"
git remote remove $syncRemote

return @{ pr = $prNumber; prUrl = $pr.html_url }
