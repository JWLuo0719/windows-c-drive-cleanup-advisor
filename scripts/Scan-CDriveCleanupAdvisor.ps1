param(
  [string]$Drive = "C",
  [string]$OutputDir = ".",
  [int]$TopCount = 30,
  [int]$LargeFileMB = 200,
  [switch]$IncludeJson
)

$ErrorActionPreference = "SilentlyContinue"

function Test-IsAdmin {
  $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
  $principal = New-Object Security.Principal.WindowsPrincipal($identity)
  return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Get-LocalTreeSize {
  param([string]$Path)
  $total = 0L
  $files = 0L
  $dirs = 0L
  $skipped = 0L
  $stack = New-Object System.Collections.Generic.Stack[string]
  $stack.Push($Path)

  while ($stack.Count -gt 0) {
    $current = $stack.Pop()
    $item = Get-Item -LiteralPath $current -Force -ErrorAction SilentlyContinue
    if ($item -and (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
      $skipped++
      continue
    }
    $dirs++
    try {
      $entries = [System.IO.Directory]::EnumerateFileSystemEntries($current)
    }
    catch {
      continue
    }
    foreach ($entry in $entries) {
      try {
        $attrs = [System.IO.File]::GetAttributes($entry)
        if (($attrs -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
          $skipped++
          continue
        }
        if (($attrs -band [IO.FileAttributes]::Directory) -ne 0) {
          $stack.Push($entry)
        }
        else {
          $info = New-Object System.IO.FileInfo($entry)
          $total += $info.Length
          $files++
        }
      }
      catch {}
    }
  }

  [pscustomobject]@{
    Path = $Path
    SizeGB = [math]::Round($total / 1GB, 2)
    Files = $files
    Dirs = $dirs
    SkippedReparsePoints = $skipped
  }
}

function Get-ChildSizeReport {
  param([string]$Root, [int]$Count)
  if (-not (Test-Path -LiteralPath $Root)) { return @() }
  $items = Get-ChildItem -LiteralPath $Root -Force -ErrorAction SilentlyContinue
  $rows = foreach ($item in $items) {
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { continue }
    if ($item.PSIsContainer) {
      Get-LocalTreeSize -Path $item.FullName
    }
    else {
      [pscustomobject]@{
        Path = $item.FullName
        SizeGB = [math]::Round($item.Length / 1GB, 2)
        Files = 1
        Dirs = 0
        SkippedReparsePoints = 0
      }
    }
  }
  return @($rows | Sort-Object SizeGB -Descending | Select-Object -First $Count)
}

function Get-LargeFiles {
  param([string]$Root, [int64]$ThresholdBytes, [int]$Count)
  $results = New-Object System.Collections.Generic.List[object]
  $stack = New-Object System.Collections.Generic.Stack[string]
  $stack.Push($Root)

  while ($stack.Count -gt 0) {
    $current = $stack.Pop()
    $item = Get-Item -LiteralPath $current -Force -ErrorAction SilentlyContinue
    if ($item -and (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) { continue }
    try {
      $entries = [System.IO.Directory]::EnumerateFileSystemEntries($current)
    }
    catch {
      continue
    }
    foreach ($entry in $entries) {
      try {
        $attrs = [System.IO.File]::GetAttributes($entry)
        if (($attrs -band [IO.FileAttributes]::ReparsePoint) -ne 0) { continue }
        if (($attrs -band [IO.FileAttributes]::Directory) -ne 0) {
          $stack.Push($entry)
        }
        else {
          $info = New-Object System.IO.FileInfo($entry)
          if ($info.Length -ge $ThresholdBytes) {
            $results.Add([pscustomobject]@{
              SizeGB = [math]::Round($info.Length / 1GB, 2)
              LastWriteTime = $info.LastWriteTime
              Path = $info.FullName
            }) | Out-Null
          }
        }
      }
      catch {}
    }
  }
  return @($results | Sort-Object SizeGB -Descending | Select-Object -First $Count)
}

function Add-Table {
  param([System.Text.StringBuilder]$Builder, [string]$Title, [object[]]$Rows)
  [void]$Builder.AppendLine("")
  [void]$Builder.AppendLine("## $Title")
  [void]$Builder.AppendLine("")
  if (-not $Rows -or $Rows.Count -eq 0) {
    [void]$Builder.AppendLine("No data.")
    return
  }
  [void]$Builder.AppendLine("| Size GB | Path | Notes |")
  [void]$Builder.AppendLine("|---:|---|---|")
  foreach ($row in $Rows) {
    $size = $row.SizeGB
    $path = ($row.Path -replace "\|", "/")
    $note = ""
    if ($row.PSObject.Properties.Name -contains "Files") {
      $note = "files=$($row.Files), dirs=$($row.Dirs)"
    }
    elseif ($row.PSObject.Properties.Name -contains "LastWriteTime") {
      $note = "modified=$($row.LastWriteTime)"
    }
    [void]$Builder.AppendLine("| $size | `$path` | $note |")
  }
}

$driveRoot = "$($Drive.TrimEnd(':')):\"
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$reportPath = Join-Path $OutputDir "c-drive-cleanup-advisor-$timestamp.md"
$jsonPath = Join-Path $OutputDir "c-drive-cleanup-advisor-$timestamp.json"

$driveInfo = [System.IO.DriveInfo]::GetDrives() | Where-Object { $_.Name -eq $driveRoot }
$isAdmin = Test-IsAdmin
$top = Get-ChildSizeReport -Root $driveRoot -Count $TopCount

$userProfile = $env:USERPROFILE
$localAppData = $env:LOCALAPPDATA
$appData = $env:APPDATA
$commonRoots = @(
  $userProfile,
  (Join-Path $localAppData "NVIDIA"),
  (Join-Path $localAppData "Microsoft"),
  (Join-Path $localAppData "JianyingPro"),
  (Join-Path $localAppData "Packages"),
  (Join-Path $localAppData "Programs"),
  (Join-Path $appData "Tencent"),
  (Join-Path $appData "kingsoft"),
  (Join-Path $appData "Code"),
  (Join-Path $userProfile ".cache"),
  (Join-Path $userProfile ".vscode"),
  (Join-Path $userProfile "Downloads"),
  (Join-Path $userProfile "Desktop"),
  "C:\ProgramData",
  "C:\Windows"
) | Select-Object -Unique

$drilldowns = @{}
foreach ($root in $commonRoots) {
  if (Test-Path -LiteralPath $root) {
    $drilldowns[$root] = Get-ChildSizeReport -Root $root -Count 20
  }
}

$largeFiles = Get-LargeFiles -Root $driveRoot -ThresholdBytes ($LargeFileMB * 1MB) -Count 80

$pagefile = Get-CimInstance Win32_PageFileUsage -ErrorAction SilentlyContinue |
  Select-Object Name,AllocatedBaseSize,CurrentUsage,PeakUsage
$computer = Get-CimInstance Win32_ComputerSystem -ErrorAction SilentlyContinue |
  Select-Object AutomaticManagedPagefile,@{Name="RAMGB";Expression={[math]::Round($_.TotalPhysicalMemory / 1GB, 2)}}
$hiber = if (Test-Path -LiteralPath "C:\hiberfil.sys") {
  Get-Item -LiteralPath "C:\hiberfil.sys" -Force | Select-Object FullName,@{Name="SizeGB";Expression={[math]::Round($_.Length / 1GB, 2)}}
} else {
  $null
}

$dismAnalyze = $null
if ($isAdmin) {
  $dismAnalyze = (Dism.exe /Online /Cleanup-Image /AnalyzeComponentStore) -join "`n"
}

$builder = New-Object System.Text.StringBuilder
[void]$builder.AppendLine("# Windows C Drive Cleanup Advisor Report")
[void]$builder.AppendLine("")
[void]$builder.AppendLine("Generated: $(Get-Date)")
[void]$builder.AppendLine("Read-only scan: yes")
[void]$builder.AppendLine("Skipped reparse points: yes")
[void]$builder.AppendLine("Running as administrator: $isAdmin")
[void]$builder.AppendLine("")
if ($driveInfo) {
  [void]$builder.AppendLine("Drive $driveRoot total: $([math]::Round($driveInfo.TotalSize / 1GB, 2)) GB")
  [void]$builder.AppendLine("Drive $driveRoot free: $([math]::Round($driveInfo.AvailableFreeSpace / 1GB, 2)) GB")
  [void]$builder.AppendLine("Drive $driveRoot free percent: $([math]::Round(100 * $driveInfo.AvailableFreeSpace / $driveInfo.TotalSize, 1))%")
}

Add-Table -Builder $builder -Title "Top Real C Drive Roots" -Rows $top

foreach ($key in ($drilldowns.Keys | Sort-Object)) {
  Add-Table -Builder $builder -Title "Drilldown: $key" -Rows $drilldowns[$key]
}

Add-Table -Builder $builder -Title "Large Files" -Rows $largeFiles

[void]$builder.AppendLine("")
[void]$builder.AppendLine("## System Managed Items")
[void]$builder.AppendLine("")
[void]$builder.AppendLine("### Pagefile")
[void]$builder.AppendLine("")
if ($pagefile) {
  foreach ($pf in $pagefile) {
    [void]$builder.AppendLine("- $($pf.Name): allocated=$($pf.AllocatedBaseSize) MB, current=$($pf.CurrentUsage) MB, peak=$($pf.PeakUsage) MB")
  }
}
if ($computer) {
  [void]$builder.AppendLine("- RAM: $($computer.RAMGB) GB")
  [void]$builder.AppendLine("- Automatic managed pagefile: $($computer.AutomaticManagedPagefile)")
}
if ($hiber) {
  [void]$builder.AppendLine("- Hibernation file: $($hiber.SizeGB) GB at $($hiber.FullName)")
}
else {
  [void]$builder.AppendLine("- Hibernation file: not found")
}

[void]$builder.AppendLine("")
[void]$builder.AppendLine("### WinSxS")
[void]$builder.AppendLine("")
if ($dismAnalyze) {
  [void]$builder.AppendLine('```text')
  [void]$builder.AppendLine($dismAnalyze)
  [void]$builder.AppendLine('```')
}
else {
  [void]$builder.AppendLine("Run as administrator for DISM AnalyzeComponentStore output:")
  [void]$builder.AppendLine("")
  [void]$builder.AppendLine('```powershell')
  [void]$builder.AppendLine("Dism.exe /Online /Cleanup-Image /AnalyzeComponentStore")
  [void]$builder.AppendLine('```')
}

[void]$builder.AppendLine("")
[void]$builder.AppendLine("## Cleanup Advice")
[void]$builder.AppendLine("")
[void]$builder.AppendLine("- Do not delete anything from WinSxS, Windows Installer, System32, or System Volume Information manually.")
[void]$builder.AppendLine("- Prefer built-in app storage managers for WeChat, QQ, WXWork, WPS, and cloud drives.")
[void]$builder.AppendLine("- GPU shader caches, browser caches, npm/pip/Gradle/Playwright caches, editor installer caches, and app update packages are usually good first-pass cleanup candidates.")
[void]$builder.AppendLine("- Pagefile changes should be made through Windows virtual memory settings unless the user explicitly wants automation.")
[void]$builder.AppendLine("- This tool did not delete, move, or modify files.")

$builder.ToString() | Set-Content -LiteralPath $reportPath -Encoding UTF8

if ($IncludeJson) {
  $data = [pscustomobject]@{
    generated = (Get-Date)
    drive = $driveRoot
    isAdmin = $isAdmin
    top = $top
    drilldowns = $drilldowns
    largeFiles = $largeFiles
    pagefile = $pagefile
    computer = $computer
    hibernation = $hiber
  }
  $data | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $jsonPath -Encoding UTF8
}

Write-Output "[OK] Report written to: $reportPath"
if ($IncludeJson) {
  Write-Output "[OK] JSON written to: $jsonPath"
}
