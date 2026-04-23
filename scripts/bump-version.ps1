param(
  [Parameter(Mandatory = $true, Position = 0)]
  [string]$Version
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Write-Utf8NoBom {
  param(
    [Parameter(Mandatory = $true)]
    [string]$Path,

    [Parameter(Mandatory = $true)]
    [string]$Value
  )

  $encoding = New-Object System.Text.UTF8Encoding($false)
  $writer = New-Object System.IO.StreamWriter($Path, $false, $encoding)
  try {
    $writer.Write($Value)
  } finally {
    $writer.Dispose()
  }
}

function Update-JsonVersionFile {
  param(
    [Parameter(Mandatory = $true)]
    [string]$Path,

    [Parameter(Mandatory = $true)]
    [string]$Version
  )

  $content = Get-Content -LiteralPath $Path -Raw
  $updated = [regex]::Replace(
    $content,
    '(?m)^(\s*"version"\s*:\s*")[^"]+(".*)$',
    ('${1}' + $Version + '${2}'),
    1
  )

  if ($updated -eq $content) {
    throw "Could not find a top-level JSON version field in $Path"
  }

  Write-Utf8NoBom -Path $Path -Value $updated
}

function Update-CargoPackageVersion {
  param(
    [Parameter(Mandatory = $true)]
    [string]$Path,

    [Parameter(Mandatory = $true)]
    [string]$Version
  )

  $lines = Get-Content -LiteralPath $Path
  $insidePackage = $false
  $updated = $false

  for ($index = 0; $index -lt $lines.Count; $index++) {
    $line = $lines[$index]

    if ($line -match '^\[package\]') {
      $insidePackage = $true
      continue
    }

    if ($insidePackage -and $line -match '^\[') {
      break
    }

    if ($insidePackage -and $line -match '^\s*version\s*=\s*"([^"]+)"') {
      $lines[$index] = [regex]::Replace($line, '(^\s*version\s*=\s*")[^"]+(".*$)', ('${1}' + $Version + '${2}'))
      $updated = $true
      break
    }
  }

  if (-not $updated) {
    throw "Could not find the [package] version field in $Path"
  }

  $newLine = if ($lines.Count -gt 0 -and $lines[0] -match "`r") { "`r`n" } else { "`n" }
  Write-Utf8NoBom -Path $Path -Value (($lines -join $newLine) + $newLine)
}

function Ensure-ReleaseNotesFile {
  param(
    [Parameter(Mandatory = $true)]
    [string]$ProjectRoot,

    [Parameter(Mandatory = $true)]
    [string]$Version
  )

  $releaseNotesDirectory = Join-Path $ProjectRoot "release-notes"
  if (-not (Test-Path -LiteralPath $releaseNotesDirectory)) {
    New-Item -ItemType Directory -Path $releaseNotesDirectory -Force | Out-Null
  }

  $releaseNotesPath = Join-Path $releaseNotesDirectory "$Version.md"
  if (-not (Test-Path -LiteralPath $releaseNotesPath)) {
    $template = @"
# DevNest $Version

## Highlights

- Summarize the main user-facing changes in this release.

## Added

- Describe new capabilities here.

## Improved

- Describe fixes, cleanup, and performance work here.

## Notes

- Add upgrade notes, migration notes, or anything users should know before updating.
"@
    Write-Utf8NoBom -Path $releaseNotesPath -Value ($template.TrimStart("`r", "`n") + "`r`n")
  }

  return $releaseNotesPath
}

if ($Version -notmatch '^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$') {
  throw "Version '$Version' is not a valid semver string."
}

$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $projectRoot

$packageJsonPath = Join-Path $projectRoot "package.json"
$tauriConfigPath = Join-Path $projectRoot "src-tauri\\tauri.conf.json"
$cargoTomlPath = Join-Path $projectRoot "src-tauri\\Cargo.toml"

Update-JsonVersionFile -Path $packageJsonPath -Version $Version
Update-JsonVersionFile -Path $tauriConfigPath -Version $Version
Update-CargoPackageVersion -Path $cargoTomlPath -Version $Version
$releaseNotesPath = Ensure-ReleaseNotesFile -ProjectRoot $projectRoot -Version $Version

Write-Host "Updated DevNest version to $Version" -ForegroundColor Green
Write-Host " - package.json"
Write-Host " - src-tauri/tauri.conf.json"
Write-Host " - src-tauri/Cargo.toml"
Write-Host " - release-notes/$Version.md"
