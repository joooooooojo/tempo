param(
  [string]$Version = ""
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$Root = Resolve-Path (Join-Path $ScriptDir "..")

if ([string]::IsNullOrWhiteSpace($Version)) {
  $PackageJson = Get-Content -Raw -Encoding UTF8 (Join-Path $Root "package.json") | ConvertFrom-Json
  $Version = [string]$PackageJson.version
}

$ReleaseDir = Join-Path $Root "src-tauri\target\release"
$Candidates = @(
  (Join-Path $ReleaseDir "tempo.exe"),
  (Join-Path $ReleaseDir "Tempo.exe")
)

$Exe = $Candidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if (-not $Exe) {
  throw "Cannot find release executable in $ReleaseDir"
}

$TargetRoot = Join-Path $Root "src-tauri\target"
$StageDir = Join-Path $TargetRoot "staged-windows"
$ResolvedTargetRoot = (Resolve-Path $TargetRoot).Path

if (Test-Path -LiteralPath $StageDir) {
  $ResolvedStageDir = (Resolve-Path $StageDir).Path
  if (-not $ResolvedStageDir.StartsWith($ResolvedTargetRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to remove path outside target directory: $ResolvedStageDir"
  }
  Remove-Item -LiteralPath $StageDir -Recurse -Force
}

New-Item -ItemType Directory -Path $StageDir | Out-Null
Copy-Item -LiteralPath $Exe -Destination (Join-Path $StageDir "Tempo.exe")

$Output = Join-Path $Root "Tempo_$($Version)_x64-staged.zip"
if (Test-Path -LiteralPath $Output) {
  Remove-Item -LiteralPath $Output -Force
}

Compress-Archive -Path (Join-Path $StageDir "*") -DestinationPath $Output -Force
Write-Output $Output
