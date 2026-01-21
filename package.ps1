param(
  [string]$OutputPath = "dist\\ToastMCP.zip",
  [switch]$SkipBuild,
  [switch]$IncludeSounds
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$exePath = Join-Path $repoRoot "target\\release\\toastmcp.exe"

if (-not $SkipBuild) {
  Write-Host "Building release binary..."
  & cargo build --release
}

if (-not (Test-Path $exePath)) {
  throw "Release binary not found at $exePath"
}

$outputFullPath = Join-Path $repoRoot $OutputPath
$outputDir = Split-Path -Parent $outputFullPath
$stagingDir = Join-Path $outputDir "staging"

New-Item -ItemType Directory -Force -Path $stagingDir | Out-Null

Copy-Item -Path $exePath -Destination (Join-Path $stagingDir "toastmcp.exe")
Copy-Item -Path (Join-Path $repoRoot "icons") -Destination (Join-Path $stagingDir "icons") -Recurse
Copy-Item -Path (Join-Path $repoRoot "res") -Destination (Join-Path $stagingDir "res") -Recurse

if ($IncludeSounds) {
  Copy-Item -Path (Join-Path $repoRoot "sounds") -Destination (Join-Path $stagingDir "sounds") -Recurse
}

if (Test-Path $outputFullPath) {
  Remove-Item -Force $outputFullPath
}

Compress-Archive -Path (Join-Path $stagingDir "*") -DestinationPath $outputFullPath

Remove-Item -Recurse -Force $stagingDir

Write-Host "Created package: $outputFullPath"
