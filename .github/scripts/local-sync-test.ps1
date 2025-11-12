<#
.github/scripts/local-sync-test.ps1

# 説明:
# このスクリプトはローカル環境で実行して、
# 1) `tyaro/melsec_mc` を一時ディレクトリにクローン
# 2) workspace の `melsec_mc` からクローン先へコピー（.git を除外）
# 3) ブランチ `sync/melsec_mc` を作成して commit/push
# 4) push の出力をログファイルに保存（必要に応じて GIT_TRACE を有効化可能）
#
# 使い方:
#   # 環境変数 SYNC_PAT に PAT を入れて実行する例
#   $env:SYNC_PAT = 'ghp_xxx'
#   pwsh .\.github\scripts\local-sync-test.ps1
#
# オプション引数:
#   -SyncPat: GitHub の PAT（省略時は環境変数 SYNC_PAT を使用）
#   -Dst: クローン先ディレクトリ（省略時は $env:TEMP に一時フォルダを作成）
#   -EnableTrace: GIT_TRACE を有効にするか（$true/$false、デフォルト $false）
#
# 注意:
# - このスクリプトはローカルでの手動実行用です。実行時に PAT が必要です。
# - 実行すると既存のクローン先ディレクトリは上書きされます。
#
# 出力:
# - ログはスクリプト実行ディレクトリに `local-sync-test-<timestamp>.log` として保存されます。
#
##>

param(
    [string]$SyncPat = $env:SYNC_PAT,
    [string]$Dst = $null,
    [switch]$EnableTrace
)

function Write-Log($s) { $s | Tee-Object -FilePath $global:LogPath -Append }

try {
    $scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
    # repo root is two levels up from .github/scripts
    $repoRoot = (Resolve-Path (Join-Path $scriptDir '..\..')).Path
    $src = Join-Path $repoRoot 'melsec_mc'

    if (-not (Test-Path $src)) {
        Write-Error "ソースパスが見つかりません: $src"
        exit 2
    }

    if (-not $SyncPat) {
        Write-Error "SYNC PAT が指定されていません。引数 -SyncPat か環境変数 SYNC_PAT を設定してください。"
        exit 2
    }

    $ts = (Get-Date).ToString('yyyyMMdd-HHmmss')
    if (-not $Dst) {
        $Dst = Join-Path $env:TEMP "melsec_mc_clone_$ts"
    }

    $global:LogPath = Join-Path $repoRoot "local-sync-test-$ts.log"
    "Local sync test log: $global:LogPath" | Out-File -FilePath $global:LogPath -Encoding utf8

    Write-Log "Repository root: $repoRoot"
    Write-Log "Source: $src"
    Write-Log "Destination: $Dst"

    if (Test-Path $Dst) {
        Write-Log "既存のターゲットを削除します: $Dst"
        Remove-Item -Recurse -Force -LiteralPath $Dst
    }

    Write-Log "→ git clone を実行します"
    $cloneUrl = "https://x-access-token:$SyncPat@github.com/tyaro/melsec_mc.git"
    $cloneCmd = "git clone `"$cloneUrl`" `"$Dst`""
    Write-Log "CMD: $cloneCmd"
    & git clone $cloneUrl $Dst 2>&1 | Tee-Object -FilePath $global:LogPath -Append
    if ($LASTEXITCODE -ne 0) {
        Write-Log "git clone failed with exit $LASTEXITCODE"
        exit 3
    }

    Write-Log "→ ファイルを同期します（.git を除外）"
    # robocopy: source dest /MIR /XD .git
    $robocopyCmd = "robocopy `"$src`" `"$Dst`" /MIR /XD `.git` /R:3 /W:5"
    Write-Log "CMD: $robocopyCmd"
    & robocopy $src $Dst /MIR /XD '.git' /R:3 /W:5 2>&1 | Tee-Object -FilePath $global:LogPath -Append

    Set-Location $Dst

    Write-Log "→ ブランチを作成して checkout"
    & git checkout -B sync/melsec_mc 2>&1 | Tee-Object -FilePath $global:LogPath -Append

    Write-Log "→ 変更をステージしてコミット"
    & git add -A 2>&1 | Tee-Object -FilePath $global:LogPath -Append
    # commit (allow empty so branch exists even if no file changed)
    & git commit -m "sync: update melsec_mc from local melsec_com" --allow-empty 2>&1 | Tee-Object -FilePath $global:LogPath -Append

    if ($EnableTrace) {
        Write-Log "GIT_TRACE を有効化します"
        $env:GIT_TRACE = '1'
        $env:GIT_TRACE_PACKET = '1'
        $env:GIT_CURL_VERBOSE = '1'
    }

    Write-Log "→ push を実行します（詳細はログ参照）"
    $pushUrl = $cloneUrl
    Write-Log "push URL: $pushUrl"
    & git push $pushUrl sync/melsec_mc --force 2>&1 | Tee-Object -FilePath $global:LogPath -Append
    $pushExit = $LASTEXITCODE
    Write-Log "git push exit code: $pushExit"

    if ($pushExit -ne 0) {
        Write-Log "push が失敗しました。ログを確認してください: $global:LogPath"
        exit 4
    }

    Write-Log "push 成功: branch sync/melsec_mc が $Dst に push されました"
    Write-Log "ログファイル: $global:LogPath"
    Write-Output "完了 — ログ: $global:LogPath"
    exit 0

} catch {
    Write-Error $_.Exception.Message
    Write-Log "例外: $($_.Exception.Message)"
    exit 9
}
