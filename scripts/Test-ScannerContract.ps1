param()

$ErrorActionPreference = "Stop"

# 共享助手（Assert-True / Get-FreeDriveLetter）：唯一定义在 scripts/Common.ps1。
. "$PSScriptRoot\Common.ps1"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$ScannerPath = Join-Path $RepoRoot "scripts\Scan-CDriveCleanupAdvisor.ps1"
$RustScannerPath = Join-Path $RepoRoot "src-tauri\src\scanner.rs"
$StagesTsPath = Join-Path $RepoRoot "src\scanStages.ts"
$FrontendSrcDir = Join-Path $RepoRoot "src"

function Get-ProtocolMap {
  # 从源文本提取协议表：先截取表块，再逐条提取 键=值。
  param([string]$Text, [string]$BlockPattern, [string]$EntryPattern, [string]$Label)
  $blockMatch = [regex]::Match($Text, $BlockPattern)
  Assert-True $blockMatch.Success "$Label protocol table block not found."
  $map = @{}
  foreach ($m in [regex]::Matches($blockMatch.Groups[1].Value, $EntryPattern)) {
    $map[$m.Groups[1].Value] = $m.Groups[2].Value
  }
  Assert-True ($map.Count -gt 0) "$Label protocol table is empty."
  return $map
}

function Assert-MapsEqual {
  param([hashtable]$Expected, [hashtable]$Actual, [string]$Label)
  foreach ($key in $Expected.Keys) {
    Assert-True ($Actual.ContainsKey($key)) "$Label is missing protocol key: $key"
    Assert-True ($Actual[$key] -eq $Expected[$key]) "$Label drifted on $key : expected $($Expected[$key]) , got $($Actual[$key])"
  }
  foreach ($key in $Actual.Keys) {
    Assert-True ($Expected.ContainsKey($key)) "$Label has extra protocol key: $key"
  }
}

Assert-True (Test-Path -LiteralPath $ScannerPath) "Scanner script is missing."
Assert-True (Test-Path -LiteralPath $RustScannerPath) "Rust scanner module is missing."
Assert-True (Test-Path -LiteralPath $StagesTsPath) "scanStages.ts is missing."

# ==== 静态断言：进度协议三方一致性 ====
# 协议表必须在发射端（PS 脚本）、解析端（scanner.rs）、消费端（scanStages.ts）三处保持一致。
$scannerText = Get-Content -Raw -Encoding UTF8 -LiteralPath $ScannerPath
$rustText = Get-Content -Raw -Encoding UTF8 -LiteralPath $RustScannerPath
$stagesText = Get-Content -Raw -Encoding UTF8 -LiteralPath $StagesTsPath

$psPct = Get-ProtocolMap -Text $scannerText `
  -BlockPattern '(?s)\$ProgressPercent\s*=\s*@\{(.*?)\}' `
  -EntryPattern '"([A-Z_]+)"\s*=\s*(\d+)' `
  -Label "Scanner \$ProgressPercent"
$rustPct = @{}
foreach ($m in [regex]::Matches($rustText, 'PROGRESS_PCT_([A-Z_]+):\s*u8\s*=\s*(\d+);')) {
  $rustPct[$m.Groups[1].Value] = $m.Groups[2].Value
}
Assert-True ($rustPct.Count -gt 0) "Rust PROGRESS_PCT_* constants not found."
$tsPct = Get-ProtocolMap -Text $stagesText `
  -BlockPattern '(?s)PROGRESS_PCT\s*=\s*\{(.*?)\}\s*as\s*const' `
  -EntryPattern '([A-Z_]+):\s*(\d+)' `
  -Label "scanStages.ts PROGRESS_PCT"

Assert-MapsEqual -Expected $psPct -Actual $rustPct -Label "scanner.rs percent table"
Assert-MapsEqual -Expected $psPct -Actual $tsPct -Label "scanStages.ts percent table"

$psCode = Get-ProtocolMap -Text $scannerText `
  -BlockPattern '(?s)\$ProgressCode\s*=\s*@\{(.*?)\}' `
  -EntryPattern '"([A-Z_]+)"\s*=\s*"([^"]*)"' `
  -Label "Scanner \$ProgressCode"
$rustCode = @{}
foreach ($m in [regex]::Matches($rustText, 'PROGRESS_CODE_([A-Z_]+):\s*&str\s*=\s*"([^"]*)";')) {
  $rustCode[$m.Groups[1].Value] = $m.Groups[2].Value
}
Assert-True ($rustCode.Count -gt 0) "Rust PROGRESS_CODE_* constants not found."
Assert-MapsEqual -Expected $psCode -Actual $rustCode -Label "scanner.rs code table"

# 标记行格式对齐：PS 发射 `[WCDCA_PROGRESS] {percent}|{code}`，Rust 以前缀+分隔符解析同一格式。
Assert-True ($scannerText.Contains('[WCDCA_PROGRESS] $Percent|$Code')) "Scanner must emit the frozen marker format."
$prefixMatch = [regex]::Match($rustText, 'PROGRESS_MARKER_PREFIX:\s*&str\s*=\s*"([^"]*)";')
Assert-True $prefixMatch.Success "Rust PROGRESS_MARKER_PREFIX not found."
Assert-True ($prefixMatch.Groups[1].Value -eq "[WCDCA_PROGRESS] ") "Rust marker prefix drifted from '[WCDCA_PROGRESS] '."
$separatorMatch = [regex]::Match($rustText, "PROGRESS_MARKER_SEPARATOR:\s*char\s*=\s*'([^']*)';")
Assert-True $separatorMatch.Success "Rust PROGRESS_MARKER_SEPARATOR not found."
Assert-True ($separatorMatch.Groups[1].Value -eq "|") "Rust marker separator drifted from '|'."

# 消费端禁止文案耦合：UI 只能按 percent 分段，不得解析 message 推断阶段。
$frontendFiles = Get-ChildItem -LiteralPath $FrontendSrcDir -Include "*.ts", "*.tsx" -Recurse -File
Assert-True ($frontendFiles.Count -gt 0) "Frontend src directory has no .ts/.tsx files."
foreach ($file in $frontendFiles) {
  $text = Get-Content -Raw -Encoding UTF8 -LiteralPath $file.FullName
  Assert-True ($text -notmatch 'message\.includes') "$($file.Name) must not derive scan stage from message text."
}
Assert-True ($stagesText -match 'PROGRESS_PCT\.DRILLDOWN_START') "scanStages.ts must use PROGRESS_PCT.DRILLDOWN_START for the deep-drilldown boundary."

# ==== 运行时断言：实际扫描输出遵守协议表 ====
# 双内核 A/B 种子树设计（Phase 2）：
#   - 各锚点 SizeGB 两位可区分且互不相同，规避 PS 平手排序不保证导致 top-N 截断差异；
#   - 硬链接对：两内核均按链接计数（std file_index 不稳定，未做去重）；
#   - junction：两内核均跳过并计入 SkippedReparsePoints；
#   - ACL 拒绝 ListDirectory 的目录：错误记账口径注入源（原生每路径 1 次；PS 快扫两遍共 2 次）。
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("wcdca-scanner-contract-" + [guid]::NewGuid().ToString("N"))
$reportDir = Join-Path $tempRoot "reports"
$nativeReportDir = Join-Path $tempRoot "native-reports"
$driveLetter = Get-FreeDriveLetter
$driveName = "${driveLetter}:"
$substCreated = $false

function New-SparseFile {
  param([string]$Path, [long]$Bytes)
  $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::Create, [System.IO.FileAccess]::Write)
  try {
    $stream.SetLength($Bytes)
  }
  finally {
    $stream.Dispose()
  }
}

function Assert-ProtocolLines {
  param([object[]]$Lines, [hashtable]$PercentTable, [string]$Label)
  # 逐行校验协议：固定阶段码的百分比必须与常量表一致；心跳码必须落在所属阶段区间内。
  $emittedFixed = @{}
  $sawLargeFilesHeartbeat = $false
  foreach ($line in $Lines) {
    $marker = [regex]::Match([string]$line, '^\[WCDCA_PROGRESS\] (\d+)\|(.+)$')
    if (-not $marker.Success) { continue }
    $pct = [int]$marker.Groups[1].Value
    $code = $marker.Groups[2].Value
    if ($code -match '^([A-Z_]+):') {
      $heartbeat = $Matches[1]
      if ($heartbeat -eq "LARGE_FILES_SCAN") {
        $sawLargeFilesHeartbeat = $true
        Assert-True ($pct -ge $PercentTable["LARGE_FILES"] -and $pct -lt $PercentTable["SYSTEM_INFO"]) "$Label LARGE_FILES_SCAN heartbeat percent $pct fell outside the protocol band."
      }
      continue
    }
    if ($PercentTable.ContainsKey($code)) {
      $emittedFixed[$code] = $pct
      Assert-True ($pct -eq $PercentTable[$code]) "$Label marker $code emitted at $pct but protocol table says $($PercentTable[$code])."
    }
  }
  foreach ($code in @("DRIVE_INFO", "TOP_ROOTS", "LARGE_FILES", "SYSTEM_INFO", "REPORT", "JSON")) {
    Assert-True ($emittedFixed.ContainsKey($code)) "$Label did not emit fixed stage code $code."
  }
  Assert-True $sawLargeFilesHeartbeat "$Label LARGE_FILES_SCAN heartbeat was not observed."
}

try {
  New-Item -ItemType Directory -Force -Path $tempRoot | Out-Null
  New-Item -ItemType Directory -Force -Path (Join-Path $tempRoot "CacheRoot") | Out-Null
  New-Item -ItemType Directory -Force -Path (Join-Path $tempRoot "DataRoot") | Out-Null
  New-Item -ItemType Directory -Force -Path (Join-Path $tempRoot "JunctionRoot") | Out-Null
  New-Item -ItemType Directory -Force -Path (Join-Path $tempRoot "LockRoot\lockbox") | Out-Null
  New-Item -ItemType Directory -Force -Path $reportDir | Out-Null
  New-Item -ItemType Directory -Force -Path $nativeReportDir | Out-Null

  # 锚点体量：CacheRoot 55MB / DataRoot 66MB(33+33 硬链接) / root-file 44MB / LockRoot 22MB / JunctionRoot 11MB
  $largeFile = Join-Path $tempRoot "CacheRoot\large-cache.bin"
  New-SparseFile -Path $largeFile -Bytes (55MB)
  $baseFile = Join-Path $tempRoot "DataRoot\base.bin"
  New-SparseFile -Path $baseFile -Bytes (33MB)
  $baseLink = Join-Path $tempRoot "DataRoot\base-link.bin"
  cmd /c mklink /H "$baseLink" "$baseFile" | Out-Null
  if ($LASTEXITCODE -ne 0) { throw "mklink /H failed for $baseLink" }
  New-SparseFile -Path (Join-Path $tempRoot "JunctionRoot\junction-file.bin") -Bytes (11MB)
  $junctionLink = Join-Path $tempRoot "JunctionRoot\link"
  cmd /c mklink /J "$junctionLink" (Join-Path $tempRoot "CacheRoot") | Out-Null
  if ($LASTEXITCODE -ne 0) { throw "mklink /J failed for $junctionLink" }
  New-SparseFile -Path (Join-Path $tempRoot "LockRoot\outside.bin") -Bytes (22MB)
  $rootFile = Join-Path $tempRoot "root-file.bin"
  New-SparseFile -Path $rootFile -Bytes (44MB)

  # 错误注入：对 lockbox 拒绝 ListDirectory（不拒 ReadAttributes/WriteDac，保证可恢复）。
  # 双方枚举都会因此失败（实测：.NET EnumerateFileSystemEntries 与 Rust read_dir 均被拒；
  # 独占句柄注入无效，两种枚举都绕过共享检查）。finally 里先恢复 ACL 再清理目录。
  $lockbox = Join-Path $tempRoot "LockRoot\lockbox"
  $denySid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
  $denyRule = New-Object System.Security.AccessControl.FileSystemAccessRule(
    $denySid,
    [System.Security.AccessControl.FileSystemRights]::ListDirectory,
    [System.Security.AccessControl.AccessControlType]::Deny)
  # Use the .NET ACL API: hosts with both PowerShell 7 and Windows PowerShell
  # module paths can fail to autoload Get-Acl in the nested verification process.
  $lockboxInfo = [System.IO.DirectoryInfo]::new($lockbox)
  $lockAcl = $lockboxInfo.GetAccessControl()
  $lockAcl.AddAccessRule($denyRule)
  $lockboxInfo.SetAccessControl($lockAcl)

  subst $driveName $tempRoot
  if ($LASTEXITCODE -ne 0) {
    throw "subst failed for $driveName"
  }
  $substCreated = $true

  $scanOutput = & powershell -NoProfile -ExecutionPolicy Bypass -File $ScannerPath `
    -Drive $driveLetter `
    -OutputDir $reportDir `
    -TopCount 5 `
    -LargeFileMB 1 `
    -IncludeJson `
    -SkipCommonRoots

  Assert-True (($scanOutput -join "`n") -match "\[WCDCA_PROGRESS\]") "Scanner did not emit progress markers."
  Assert-True (($scanOutput -join "`n") -match "TOP_ROOTS_SCAN") "Scanner did not emit top-root heartbeat markers."
  Assert-True (($scanOutput -join "`n") -match "LARGE_FILES_SCAN") "Scanner did not emit large-file heartbeat markers."
  Assert-True (($scanOutput -join "`n") -match "\[OK\] Report written") "Scanner did not report Markdown output."
  Assert-True (($scanOutput -join "`n") -match "\[OK\] JSON written") "Scanner did not report JSON output."
  Assert-True (Test-Path -LiteralPath $largeFile) "Scanner changed or removed the source file."

  Assert-ProtocolLines -Lines $scanOutput -PercentTable $psPct -Label "PS scanner"

  $jsonPath = Get-ChildItem -LiteralPath $reportDir -Filter "*.json" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
  $markdownPath = Get-ChildItem -LiteralPath $reportDir -Filter "*.md" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
  Assert-True ($null -ne $jsonPath) "Scanner did not create a JSON report."
  Assert-True ($null -ne $markdownPath) "Scanner did not create a Markdown report."

  $json = Get-Content -Raw -Encoding UTF8 -LiteralPath $jsonPath.FullName | ConvertFrom-Json
  Assert-True ($json.drive -eq "${driveLetter}:\") "JSON report drive does not match temporary drive."
  $largeFiles = @($json.largeFiles)
  Assert-True ($largeFiles.Count -ge 1) "JSON report did not include the seeded large file."
  $seededLargeFiles = @($largeFiles | Where-Object {
    ($_.Path -like "$driveName*") -or ($_.Path -like "$tempRoot*")
  })
  Assert-True ($seededLargeFiles.Count -ge 1) "Large file path is not under the temporary scan root."
  $drilldownProperties = @($json.drilldowns.PSObject.Properties | Where-Object { $_.MemberType -eq "NoteProperty" })
  Assert-True ($drilldownProperties.Count -eq 0) "SkipCommonRoots should avoid common-root drilldowns."

  $markdown = Get-Content -Raw -Encoding UTF8 -LiteralPath $markdownPath.FullName
  Assert-True ($markdown.Contains("Read-only scan: yes")) "Markdown report does not state read-only scan."
  Assert-True ($markdown.Contains("This tool did not delete, move, or modify files.")) "Markdown report does not state no file changes."

  # ==== 双内核 A/B 等价断言（Phase 2）====
  # 同一种子树跑原生内核（wcdca-scan 测试二进制），与上方 PS 内核结果逐字段比对。
  Assert-True (Test-Path -LiteralPath $largeFile) "PS scan changed or removed the source file."
  $psJson = $json

  $srcTauriDir = Join-Path $RepoRoot "src-tauri"
  Push-Location $srcTauriDir
  try {
    cargo build --bin wcdca-scan --quiet
    if ($LASTEXITCODE -ne 0) { throw "cargo build --bin wcdca-scan failed" }
  }
  finally {
    Pop-Location
  }
  $wcdcaScanExe = Join-Path $srcTauriDir "target\debug\wcdca-scan.exe"
  Assert-True (Test-Path -LiteralPath $wcdcaScanExe) "wcdca-scan.exe was not produced by cargo build."

  $nativeOutput = & $wcdcaScanExe --root "${driveName}\" --output $nativeReportDir --top 5 --large-mb 1 --skip-common-roots
  Assert-True ($LASTEXITCODE -eq 0) "wcdca-scan exited with code $LASTEXITCODE"
  Assert-True (($nativeOutput -join "`n") -match "\[OK\] Report written") "Native kernel did not report Markdown output."
  Assert-True (($nativeOutput -join "`n") -match "\[OK\] JSON written") "Native kernel did not report JSON output."
  Assert-ProtocolLines -Lines $nativeOutput -PercentTable $psPct -Label "Native kernel"

  $nativeJsonPath = Get-ChildItem -LiteralPath $nativeReportDir -Filter "*.json" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
  $nativeMarkdownPath = Get-ChildItem -LiteralPath $nativeReportDir -Filter "*.md" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
  Assert-True ($null -ne $nativeJsonPath) "Native kernel did not create a JSON report."
  Assert-True ($null -ne $nativeMarkdownPath) "Native kernel did not create a Markdown report."
  $nativeJson = Get-Content -Raw -Encoding UTF8 -LiteralPath $nativeJsonPath.FullName | ConvertFrom-Json
  $nativeMarkdown = Get-Content -Raw -Encoding UTF8 -LiteralPath $nativeMarkdownPath.FullName
  Assert-True ($nativeMarkdown.Contains("Read-only scan: yes")) "Native Markdown report does not state read-only scan."
  Assert-True ($nativeMarkdown.Contains("This tool did not delete, move, or modify files.")) "Native Markdown report does not state no file changes."
  Assert-True (Test-Path -LiteralPath $largeFile) "Native kernel changed or removed the source file."
  Assert-True (Test-Path -LiteralPath $rootFile) "Native kernel changed or removed the root-level file."

  # A/B 1：drilldowns 口径（SkipCommonRoots 时两内核都必须为空）。
  $nativeDrilldownProperties = @($nativeJson.drilldowns.PSObject.Properties | Where-Object { $_.MemberType -eq "NoteProperty" })
  Assert-True ($nativeDrilldownProperties.Count -eq 0) "Native kernel: SkipCommonRoots should avoid common-root drilldowns."

  # A/B 2：top 行逐字段等价（Path → SizeGB/Files/Dirs/SkippedReparsePoints，忽略次序）。
  function Get-RowMap {
    param([object[]]$Rows)
    $map = @{}
    foreach ($row in $Rows) {
      $map[[string]$row.Path] = "{0}|{1}|{2}|{3}" -f $row.SizeGB, $row.Files, $row.Dirs, $row.SkippedReparsePoints
    }
    return $map
  }
  $psTop = Get-RowMap -Rows @($psJson.top)
  $nativeTop = Get-RowMap -Rows @($nativeJson.top)
  Assert-True ($psTop.Count -eq 5) "PS kernel top row count is $($psTop.Count), expected 5 (unique anchor sizes)."
  Assert-True ($nativeTop.Count -eq 5) "Native kernel top row count is $($nativeTop.Count), expected 5 (unique anchor sizes)."
  foreach ($key in $psTop.Keys) {
    Assert-True ($nativeTop.ContainsKey($key)) "Native kernel top is missing row: $key"
    Assert-True ($nativeTop[$key] -eq $psTop[$key]) "A/B mismatch on top row $key : PS $($psTop[$key]) , native $($nativeTop[$key])"
  }
  foreach ($key in $nativeTop.Keys) {
    Assert-True ($psTop.ContainsKey($key)) "PS kernel top is missing row: $key"
  }

  # A/B 3：大文件行等价（Path → SizeGB；LastWriteTime 表示法不同，见 KERNEL_MIGRATION.md）。
  $psLarge = Get-RowMap -Rows @($psJson.largeFiles | ForEach-Object {
    [pscustomobject]@{ Path = $_.Path; SizeGB = $_.SizeGB; Files = 0; Dirs = 0; SkippedReparsePoints = 0 }
  })
  $nativeLarge = Get-RowMap -Rows @($nativeJson.largeFiles | ForEach-Object {
    [pscustomobject]@{ Path = $_.Path; SizeGB = $_.SizeGB; Files = 0; Dirs = 0; SkippedReparsePoints = 0 }
  })
  Assert-True ($psLarge.Count -ge 5) "PS kernel large-file rows: expected at least 5 seeded files, got $($psLarge.Count)."
  Assert-True ($nativeLarge.Count -eq $psLarge.Count) "A/B large-file row count mismatch: PS $($psLarge.Count) , native $($nativeLarge.Count)"
  foreach ($key in $psLarge.Keys) {
    Assert-True ($nativeLarge.ContainsKey($key)) "Native kernel large files are missing: $key"
    Assert-True ($nativeLarge[$key] -eq $psLarge[$key]) "A/B mismatch on large file $key : PS $($psLarge[$key]) , native $($nativeLarge[$key])"
  }

  # A/B 4：硬链接按链接计数（两内核 Files 都把每个链接算作一个文件，总量相等）。
  $psDataRoot = $psTop["$driveName\DataRoot"]
  $nativeDataRoot = $nativeTop["$driveName\DataRoot"]
  Assert-True ($psDataRoot -eq "0.06|2|1|0") "PS hardlink accounting drifted (expected SizeGB=0.06 Files=2 Dirs=1 Skipped=0): $psDataRoot"
  Assert-True ($nativeDataRoot -eq "0.06|2|1|0") "Native hardlink accounting drifted (expected SizeGB=0.06 Files=2 Dirs=1 Skipped=0): $nativeDataRoot"

  # A/B 5：junction 跳过计数等价。
  $psJunction = $psTop["$driveName\JunctionRoot"]
  $nativeJunction = $nativeTop["$driveName\JunctionRoot"]
  Assert-True ($psJunction -eq "0.01|1|1|1") "PS junction accounting drifted (expected Skipped=1): $psJunction"
  Assert-True ($nativeJunction -eq "0.01|1|1|1") "Native junction accounting drifted (expected Skipped=1): $nativeJunction"

  # A/B 6：scanErrors 记账口径。
  #   原生：单次遍历，每个失败路径记 1 次（scanErrorTotal 为截断前总数，列表上限 200）。
  #   PS：快扫两遍（TreeSize* + LargeFiles*），同一路径最多记 2 次；JSON 暂无 total 字段
  #   （Phase 4 脚本卫生补总数），小种子树下列表长度即总数。
  #   注入源 lockbox 被 ACL 拒绝 ListDirectory：两种内核枚举都必失败，差异只在计数次数
  #   （PS 的 Get-LocalTreeSize 与 Get-LargeFiles 各记一次；原生单次遍历记一次）。
  $psErrorRows = @($psJson.scanErrors)
  $nativeErrorRows = @($nativeJson.scanErrors)
  $nativeErrorTotal = [int]$nativeJson.scanErrorTotal
  Assert-True ($nativeErrorRows.Count -ge 1) "Native kernel did not record the locked-directory scan error."
  Assert-True ($nativeErrorTotal -eq $nativeErrorRows.Count) "Native scanErrorTotal must equal the row count when below the limit: total $nativeErrorTotal , rows $($nativeErrorRows.Count)"
  $nativeErrorPaths = @($nativeErrorRows | ForEach-Object { [string]$_.Path })
  Assert-True (($nativeErrorPaths | Where-Object { $_ -like "*lockbox*" }).Count -ge 1) "Native scanErrors do not mention the locked path."
  if ($psErrorRows.Count -gt 0) {
    Assert-True ($psErrorRows.Count -eq (2 * $nativeErrorTotal)) "PS/native scan-error totals must follow the 2:1 dual-walk ratio when both record errors: PS $($psErrorRows.Count) , native $nativeErrorTotal"
    $psErrorPaths = @($psErrorRows | ForEach-Object { [string]$_.Path })
    Assert-True (($psErrorPaths | Where-Object { $_ -like "*lockbox*" }).Count -ge 1) "PS scanErrors do not mention the locked path."
    Write-Output "[OK] A/B scan-error ratio verified (PS 2 : native 1 per path)"
  }
  else {
    Write-Output "[NOTE] PS kernel recorded 0 scan errors while native recorded $nativeErrorTotal (both should fail on the ACL-denied path). Documented in KERNEL_MIGRATION.md."
  }
  Assert-True ($nativeJson.scanErrorLimit -eq 200) "Native scanErrorLimit drifted from 200."
  # PS 侧新增 scanErrorTotal（截断前总数）：未超上限时必须等于行数，与原生语义一致。
  $psErrorTotal = [int]$psJson.scanErrorTotal
  Assert-True ($psErrorTotal -ge $psErrorRows.Count) "PS scanErrorTotal must be at least the row count: total $psErrorTotal , rows $($psErrorRows.Count)"
  if ($psErrorRows.Count -lt [int]$psJson.scanErrorLimit) {
    Assert-True ($psErrorTotal -eq $psErrorRows.Count) "PS scanErrorTotal must equal the row count when below the limit: total $psErrorTotal , rows $($psErrorRows.Count)"
  }
}
finally {
  if ($null -ne $denyRule -and (Test-Path -LiteralPath $lockbox)) {
    # 恢复 lockbox ACL（deny 规则不拒 WriteDac/ReadControl，可无损还原）。
    $restoreInfo = [System.IO.DirectoryInfo]::new($lockbox)
    $restoreAcl = $restoreInfo.GetAccessControl()
    $restoreAcl.RemoveAccessRule($denyRule) | Out-Null
    $restoreInfo.SetAccessControl($restoreAcl)
  }
  if ($substCreated) {
    subst $driveName /D | Out-Null
  }
  if (Test-Path -LiteralPath $tempRoot) {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force
  }
}

Write-Output "[OK] Scanner contract checks passed"
