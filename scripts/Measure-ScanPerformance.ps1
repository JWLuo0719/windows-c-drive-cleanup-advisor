param(
  # 真实盘模式：对指定盘符跑原生内核 Quick 扫描并对照产品验收预算。
  [switch]$RealDrive,
  [string]$Drive = "C",
  # 种子树规模（默认 300 目录 × 16 文件，实测量级足够体现数量级差异）。
  [int]$DirCount = 300,
  [int]$FilesPerDir = 16,
  # 性能闸门：原生内核至少快 MinSpeedup 倍，且绝对时长不超过 NativeBudgetSeconds。
  [double]$MinSpeedup = 10,
  [int]$NativeBudgetSeconds = 30,
  # 真实盘 Quick 扫描产品验收预算（秒）。
  [int]$RealDriveBudgetSeconds = 60
)

# 双内核性能闸门（Phase 2）：
#   - 种子树模式（默认）：同一棵树分别跑 PS 兜底内核与原生内核，断言原生 ≥10x 快且绝对时长达标；
#   - 真实盘模式：对真实盘符跑原生 Quick 扫描，对照产品验收预算（AGENT.md：Quick < 60s）。
# 只读边界：仅建/删自管临时产物（$scanRoot/$tempRoot/$outputDir），不触碰用户数据。

$ErrorActionPreference = "Stop"

# 共享助手（Assert-True / Get-FreeDriveLetter）：唯一定义在 scripts/Common.ps1。
. "$PSScriptRoot\Common.ps1"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$ScannerPath = Join-Path $RepoRoot "scripts\Scan-CDriveCleanupAdvisor.ps1"

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

function Get-WcdcaScanBinary {
  # 性能基准必须与产品内核同优化级别（tauri 打包即 release）；debug 构建会低估 5-20 倍。
  $srcTauriDir = Join-Path $RepoRoot "src-tauri"
  Push-Location $srcTauriDir
  try {
    cargo build --release --bin wcdca-scan --quiet
    if ($LASTEXITCODE -ne 0) {
      throw "cargo build --release --bin wcdca-scan failed with exit code $LASTEXITCODE"
    }
  }
  finally {
    Pop-Location
  }
  $exe = Join-Path $srcTauriDir "target\release\wcdca-scan.exe"
  Assert-True (Test-Path -LiteralPath $exe) "release wcdca-scan.exe was not produced by cargo build."
  return $exe
}

if ($RealDrive) {
  # ==== 真实盘模式：原生内核 Quick 扫描对照产品验收预算 ====
  $wcdcaScanExe = Get-WcdcaScanBinary
  # 清理对象统一为自管 $tempRoot（安全边界白名单变量）。
  $tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("wcdca-perf-real-" + [guid]::NewGuid().ToString("N"))
  $outputDir = Join-Path $tempRoot "reports"
  New-Item -ItemType Directory -Force -Path $outputDir | Out-Null
  $driveRoot = "$($Drive.TrimEnd(':')):\"
  try {
    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    & $wcdcaScanExe --root $driveRoot --output $outputDir --top 5 --large-mb 200 --skip-common-roots | Out-Null
    $stopwatch.Stop()
    if ($LASTEXITCODE -ne 0) {
      throw "wcdca-scan exited with code $LASTEXITCODE"
    }
    $seconds = [math]::Round($stopwatch.Elapsed.TotalSeconds, 1)
    Write-Output "[PERF] real-drive quick drive=$driveRoot native=${seconds}s budget=${RealDriveBudgetSeconds}s"
    Assert-True ($seconds -le $RealDriveBudgetSeconds) "Real-drive quick scan took ${seconds}s, exceeding the ${RealDriveBudgetSeconds}s acceptance budget."
    Write-Output "[OK] Real-drive performance gate passed"
  }
  finally {
    if (Test-Path -LiteralPath $tempRoot) {
      Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
  }
  return
}

# ==== 种子树模式：双内核 A/B 计时 ====
$wcdcaScanExe = Get-WcdcaScanBinary
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("wcdca-perf-seed-" + [guid]::NewGuid().ToString("N"))
$scanRoot = Join-Path $tempRoot "tree"
$psOutputDir = Join-Path $tempRoot "ps-reports"
$nativeOutputDir = Join-Path $tempRoot "native-reports"
$driveLetter = Get-FreeDriveLetter
$driveName = "${driveLetter}:"
$substCreated = $false

try {
  New-Item -ItemType Directory -Force -Path $scanRoot | Out-Null
  New-Item -ItemType Directory -Force -Path $psOutputDir | Out-Null
  New-Item -ItemType Directory -Force -Path $nativeOutputDir | Out-Null

  # 固定种子树：每目录 FilesPerDir 个 1KB 稀疏文件 + 少量大文件候选。
  $totalFiles = 0
  for ($dirIndex = 0; $dirIndex -lt $DirCount; $dirIndex++) {
    $dirPath = Join-Path $scanRoot ("dir-{0:D4}" -f $dirIndex)
    New-Item -ItemType Directory -Force -Path $dirPath | Out-Null
    for ($fileIndex = 0; $fileIndex -lt $FilesPerDir; $fileIndex++) {
      New-SparseFile -Path (Join-Path $dirPath ("file-{0:D3}.bin" -f $fileIndex)) -Bytes 1024
      $totalFiles++
    }
    if (($dirIndex % 50) -eq 0) {
      # 大文件候选仅参与 Get-LargeFiles/walk 的阈值判断路径；体积不进 largeFiles（阈值 200MB），
      # 遍历开销与真实大文件一致。2MB 避免 SetLength 预分配撑爆临时盘。
      New-SparseFile -Path (Join-Path $dirPath "large-cache.bin") -Bytes (2MB)
      $totalFiles++
    }
  }

  subst $driveName $scanRoot
  if ($LASTEXITCODE -ne 0) {
    throw "subst failed for $driveName"
  }
  $substCreated = $true

  # On a clean Windows runner, first execution of the freshly built unsigned
  # test binary can include process/AV startup cost. Measure that separately
  # against the absolute budget, then compare warmed walks on the same tree.
  # Warm both kernels so the ratio is about traversal rather than first launch.
  & powershell -NoProfile -ExecutionPolicy Bypass -File $ScannerPath `
    -Drive $driveLetter `
    -OutputDir $psOutputDir `
    -TopCount 5 `
    -LargeFileMB 200 `
    -IncludeJson `
    -SkipCommonRoots | Out-Null
  if ($LASTEXITCODE -ne 0) {
    throw "PS scanner warm-up exited with code $LASTEXITCODE"
  }

  $firstNativeStopwatch = [System.Diagnostics.Stopwatch]::StartNew()
  & $wcdcaScanExe --root "${driveName}\" --output $nativeOutputDir --top 5 --large-mb 200 --skip-common-roots | Out-Null
  $firstNativeStopwatch.Stop()
  if ($LASTEXITCODE -ne 0) {
    throw "wcdca-scan first-run exited with code $LASTEXITCODE"
  }
  $firstNativeSeconds = [math]::Round($firstNativeStopwatch.Elapsed.TotalSeconds, 2)
  Write-Output "[PERF] seed-tree first-run native=${firstNativeSeconds}s budget=${NativeBudgetSeconds}s"
  Assert-True ($firstNativeSeconds -le $NativeBudgetSeconds) "Native first-run seed-tree scan took ${firstNativeSeconds}s, exceeding the ${NativeBudgetSeconds}s budget."

  $psStopwatch = [System.Diagnostics.Stopwatch]::StartNew()
  & powershell -NoProfile -ExecutionPolicy Bypass -File $ScannerPath `
    -Drive $driveLetter `
    -OutputDir $psOutputDir `
    -TopCount 5 `
    -LargeFileMB 200 `
    -IncludeJson `
    -SkipCommonRoots | Out-Null
  $psStopwatch.Stop()
  if ($LASTEXITCODE -ne 0) {
    throw "PS scanner exited with code $LASTEXITCODE"
  }

  $nativeStopwatch = [System.Diagnostics.Stopwatch]::StartNew()
  & $wcdcaScanExe --root "${driveName}\" --output $nativeOutputDir --top 5 --large-mb 200 --skip-common-roots | Out-Null
  $nativeStopwatch.Stop()
  if ($LASTEXITCODE -ne 0) {
    throw "wcdca-scan exited with code $LASTEXITCODE"
  }

  # 原生内核常在百毫秒级：断言走毫秒精度，展示保留 2 位小数秒。
  $psMs = $psStopwatch.Elapsed.TotalMilliseconds
  $nativeMs = $nativeStopwatch.Elapsed.TotalMilliseconds
  Assert-True ($nativeMs -ge 1) "Native kernel timing collapsed below 1ms; increase seed tree size."
  $speedup = [math]::Round($psMs / $nativeMs, 1)
  $psSeconds = [math]::Round($psMs / 1000, 2)
  $nativeSeconds = [math]::Round($nativeMs / 1000, 2)

  Write-Output "[PERF] seed-tree warmed dirs=$DirCount files=$totalFiles native=${nativeSeconds}s ps=${psSeconds}s speedup=${speedup}x gates=10x,${NativeBudgetSeconds}s"

  Assert-True ($speedup -ge $MinSpeedup) "Native kernel is only ${speedup}x faster than PS (gate: ${MinSpeedup}x). If the order-of-magnitude gate fails, keep the PS kernel and reposition the product per AGENT.md."
  Assert-True ($nativeSeconds -le $NativeBudgetSeconds) "Native seed-tree scan took ${nativeSeconds}s, exceeding the ${NativeBudgetSeconds}s budget."
  Write-Output "[OK] Seed-tree performance gate passed"
}
finally {
  if ($substCreated) {
    subst $driveName /D | Out-Null
  }
  if (Test-Path -LiteralPath $tempRoot) {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force
  }
}
