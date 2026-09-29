# 共享测试/发布助手：被 Invoke-ProjectChecks 生态下的脚本以 dot-source 方式引入。
# 去重来源：Assert-True 原 4 份、Get-FreeDriveLetter 原 3 份、Get-Sha256Hex 原 2 份
#（scripts/Test-SafetyBoundary.ps1、Test-ScannerContract.ps1、Measure-ScanPerformance.ps1、
#  Test-PortablePackage.ps1、New-ReleaseChecksum.ps1）。修改助手实现只改这一个文件。

function Assert-True {
  param([bool]$Condition, [string]$Message)
  if (-not $Condition) {
    throw $Message
  }
}

function Get-FreeDriveLetter {
  $used = [System.IO.DriveInfo]::GetDrives() | ForEach-Object { $_.Name.Substring(0, 1).ToUpperInvariant() }
  foreach ($letter in @("Z", "Y", "X", "W", "V", "U", "T")) {
    if ($used -notcontains $letter) {
      return $letter
    }
  }
  throw "No temporary drive letter is available."
}

function Get-Sha256Hex {
  param(
    [Parameter(Mandatory = $true)]
    [string]$LiteralPath
  )

  $stream = [System.IO.File]::OpenRead($LiteralPath)
  $sha256 = [System.Security.Cryptography.SHA256]::Create()
  try {
    $hashBytes = $sha256.ComputeHash($stream)
    return [System.BitConverter]::ToString($hashBytes).Replace("-", "")
  }
  finally {
    $sha256.Dispose()
    $stream.Dispose()
  }
}
