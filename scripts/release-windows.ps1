param(
  [string]$Channel = "",
  [string]$UpdateEndpoint = "",
  [string]$KeyPath = "",
  [string]$KeyPassword = "",
  [string]$RepoSlug = "",
  [string]$ReleaseTag = "",
  [string]$AssetUrl = "",
  [string]$MetadataPublishPath = "",
  [string]$NotesPath = "",
  [ValidateSet("nsis", "msi")]
  [string]$Bundle = "nsis",
  [switch]$AllBundles,
  [switch]$SkipBuild,
  [switch]$SkipMetadata,
  [switch]$SkipGitHubRelease
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$defaultRepoSlug = ""

function Write-Step {
  param([string]$Message)
  Write-Host ""
  Write-Host "==> $Message" -ForegroundColor Cyan
}

function Get-JsonValue {
  param(
    [Parameter(Mandatory = $true)]
    [string]$Path
  )

  return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
}

function Get-CargoVersion {
  param(
    [Parameter(Mandatory = $true)]
    [string]$Path
  )

  $content = Get-Content -LiteralPath $Path
  $insidePackage = $false

  foreach ($line in $content) {
    if ($line -match '^\[package\]') {
      $insidePackage = $true
      continue
    }

    if ($insidePackage -and $line -match '^\[') {
      break
    }

    if ($insidePackage -and $line -match '^\s*version\s*=\s*"([^"]+)"') {
      return $Matches[1]
    }
  }

  throw "Could not find the package version in $Path"
}

function Invoke-Checked {
  param(
    [Parameter(Mandatory = $true)]
    [string]$FilePath,

    [Parameter(Mandatory = $true)]
    [string[]]$Arguments
  )

  & $FilePath @Arguments
  if ($LASTEXITCODE -ne 0) {
    throw "Command failed: $FilePath $($Arguments -join ' ')"
  }
}

function Find-SingleFile {
  param(
    [Parameter(Mandatory = $true)]
    [string]$Directory,

    [Parameter(Mandatory = $true)]
    [string]$Filter
  )

  $matches = @(Get-ChildItem -LiteralPath $Directory -Filter $Filter -File -ErrorAction SilentlyContinue)
  if ($matches.Count -eq 0) {
    throw "Could not find a file matching '$Filter' in $Directory"
  }

  if ($matches.Count -gt 1) {
    throw "Expected one file matching '$Filter' in $Directory but found $($matches.Count)"
  }

  return $matches[0]
}

function Get-BundleArtifactInfo {
  param(
    [Parameter(Mandatory = $true)]
    [string]$Directory,

    [Parameter(Mandatory = $true)]
    [ValidateSet("nsis", "msi")]
    [string]$Bundle,

    [Parameter(Mandatory = $true)]
    [string]$PackageVersion
  )

  $installerFilter = if ($Bundle -eq "nsis") { "*$PackageVersion*setup.exe" } else { "*$PackageVersion*.msi" }
  $installerFile = Find-SingleFile -Directory $Directory -Filter $installerFilter
  $updaterSignatureFilter = if ($Bundle -eq "nsis") { "*$PackageVersion*setup.exe.sig" } else { "*$PackageVersion*.msi.sig" }
  $updaterSignature = Find-SingleFile -Directory $Directory -Filter $updaterSignatureFilter
  $updaterArtifactPath = Join-Path $Directory ([System.IO.Path]::GetFileNameWithoutExtension($updaterSignature.Name))
  if (-not (Test-Path -LiteralPath $updaterArtifactPath)) {
    throw "Updater artifact matching $($updaterSignature.Name) was not found at $updaterArtifactPath"
  }

  return [PSCustomObject]@{
    bundle = $Bundle
    installer = $installerFile
    updaterArtifact = (Get-Item -LiteralPath $updaterArtifactPath)
    updaterSignature = $updaterSignature
  }
}

function Test-CommandAvailable {
  param([string]$Name)
  return $null -ne (Get-Command $Name -ErrorAction SilentlyContinue)
}

function Resolve-CommandPath {
  param([string]$Name)

  $command = Get-Command $Name -ErrorAction SilentlyContinue
  if ($null -eq $command) {
    throw "Required command was not found: $Name"
  }

  return $command.Source
}

function Normalize-RepoSlug {
  param([string]$Value)

  if ([string]::IsNullOrWhiteSpace($Value)) {
    return ""
  }

  $trimmed = $Value.Trim()
  if ($trimmed -match '^https?://github\.com/([^/]+)/([^/]+?)(?:\.git)?/?$') {
    return "$($Matches[1])/$($Matches[2])"
  }

  return $trimmed.TrimEnd("/")
}

function Resolve-GhPath {
  if (-not [string]::IsNullOrWhiteSpace($env:DEVNEST_GH_PATH) -and (Test-Path -LiteralPath $env:DEVNEST_GH_PATH)) {
    return $env:DEVNEST_GH_PATH
  }

  $command = Get-Command "gh" -ErrorAction SilentlyContinue
  if ($null -ne $command) {
    return $command.Source
  }

  $candidatePaths = @(
    "C:\Program Files\GitHub CLI\gh.exe",
    "C:\Program Files (x86)\GitHub CLI\gh.exe"
  )

  foreach ($candidate in $candidatePaths) {
    if (Test-Path -LiteralPath $candidate) {
      return $candidate
    }
  }

  return $null
}

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

function Get-GitHubRepoVisibility {
  param(
    [string]$GhPath,
    [string]$RepoSlug
  )

  if ([string]::IsNullOrWhiteSpace($GhPath) -or [string]::IsNullOrWhiteSpace($RepoSlug)) {
    return $null
  }

  $repoJson = & $GhPath repo view $RepoSlug --json visibility 2>$null
  if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($repoJson)) {
    return $null
  }

  return ((ConvertFrom-Json -InputObject $repoJson).visibility)
}

function Get-GitHubRepoInfo {
  param(
    [string]$GhPath,
    [string]$RepoSlug
  )

  if ([string]::IsNullOrWhiteSpace($GhPath) -or [string]::IsNullOrWhiteSpace($RepoSlug)) {
    return $null
  }

  try {
    $repoJson = & $GhPath api "repos/$RepoSlug" 2>$null
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($repoJson)) {
      return $null
    }

    return (ConvertFrom-Json -InputObject $repoJson)
  } catch {
    return $null
  }
}

function Test-GitHubRepoInitialized {
  param(
    [Parameter(Mandatory = $true)]
    [string]$GhPath,

    [Parameter(Mandatory = $true)]
    [string]$RepoSlug
  )

  try {
    $commitsJson = & $GhPath api "repos/$RepoSlug/commits?per_page=1" 2>$null
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($commitsJson)) {
      return $false
    }

    $commits = ConvertFrom-Json -InputObject $commitsJson
    return ($commits.Count -gt 0)
  } catch {
    return $false
  }
}

function Ensure-GitHubReleaseRepoInitialized {
  param(
    [Parameter(Mandatory = $true)]
    [string]$GhPath,

    [Parameter(Mandatory = $true)]
    [string]$RepoSlug
  )

  if (Test-GitHubRepoInitialized -GhPath $GhPath -RepoSlug $RepoSlug) {
    return
  }

  $repoInfo = Get-GitHubRepoInfo -GhPath $GhPath -RepoSlug $RepoSlug
  if ($null -eq $repoInfo) {
    return
  }

  $defaultBranch = if ([string]::IsNullOrWhiteSpace($repoInfo.default_branch)) { "main" } else { [string]$repoInfo.default_branch }
  $bootstrapContent = "# DevNest Release Feed`n`nThis repository hosts public DevNest release artifacts and updater metadata.`n"
  $bootstrapContentBase64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($bootstrapContent))

  Write-Step "Initializing empty GitHub release repository"
  Invoke-Checked -FilePath $GhPath -Arguments @(
    "api",
    "-X", "PUT",
    "repos/$RepoSlug/contents/README.md",
    "-f", "message=Initialize release repo for DevNest updater",
    "-f", "content=$bootstrapContentBase64",
    "-f", "branch=$defaultBranch"
  )
}

function Test-GitHubReleaseExists {
  param(
    [Parameter(Mandatory = $true)]
    [string]$GhPath,

    [Parameter(Mandatory = $true)]
    [string]$RepoSlug,

    [Parameter(Mandatory = $true)]
    [string]$ReleaseTag
  )

  try {
    & $GhPath release view $ReleaseTag --repo $RepoSlug 1>$null 2>$null
    return ($LASTEXITCODE -eq 0)
  } catch {
    return $false
  }
}

function Stop-LockedBuildProcess {
  param(
    [Parameter(Mandatory = $true)]
    [string]$ExecutablePath
  )

  $normalizedTargetPath = [System.IO.Path]::GetFullPath($ExecutablePath)
  $lockedProcesses = @(
    Get-Process devnest -ErrorAction SilentlyContinue | Where-Object {
      try {
        $_.Path -and ([System.IO.Path]::GetFullPath($_.Path) -eq $normalizedTargetPath)
      } catch {
        $false
      }
    }
  )

  if ($lockedProcesses.Count -eq 0) {
    return
  }

  Write-Step "Stopping running build output"
  foreach ($process in $lockedProcesses) {
    Write-Host "Stopping PID $($process.Id): $($process.Path)"
    Stop-Process -Id $process.Id -Force
  }

  Start-Sleep -Milliseconds 750

  $remainingLocks = @(
    Get-Process devnest -ErrorAction SilentlyContinue | Where-Object {
      try {
        $_.Path -and ([System.IO.Path]::GetFullPath($_.Path) -eq $normalizedTargetPath)
      } catch {
        $false
      }
    }
  )

  if ($remainingLocks.Count -gt 0) {
    throw "Could not stop the running build output at $normalizedTargetPath. Close DevNest processes using this workspace build and try again."
  }
}

$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $projectRoot

$packageJsonPath = Join-Path $projectRoot "package.json"
$tauriConfigPath = Join-Path $projectRoot "src-tauri\\tauri.conf.json"
$cargoTomlPath = Join-Path $projectRoot "src-tauri\\Cargo.toml"
$metadataScriptPath = Join-Path $projectRoot "scripts\\generate-updater-metadata.ps1"
$tauriCliPath = Join-Path $projectRoot "node_modules\\.bin\\tauri.cmd"
$viteCliPath = Join-Path $projectRoot "node_modules\\.bin\\vite.cmd"
$cacheDirectory = Join-Path $projectRoot ".codex-cache\\release-windows"
$tauriBuildConfigOverridePath = Join-Path $cacheDirectory "tauri.build.override.json"
$workspaceReleaseExecutablePath = Join-Path $projectRoot "src-tauri\\target\\release\\devnest.exe"

if (-not (Test-Path -LiteralPath $tauriCliPath)) {
  throw "Tauri CLI wrapper was not found at $tauriCliPath"
}

if (-not (Test-Path -LiteralPath $viteCliPath)) {
  throw "Vite CLI wrapper was not found at $viteCliPath"
}

New-Item -ItemType Directory -Path $cacheDirectory -Force | Out-Null

$packageJson = Get-JsonValue -Path $packageJsonPath
$tauriConfig = Get-JsonValue -Path $tauriConfigPath
$productName = [string]$tauriConfig.productName
$packageVersion = [string]$packageJson.version
$tauriVersion = [string]$tauriConfig.version
$cargoVersion = Get-CargoVersion -Path $cargoTomlPath

if ($packageVersion -ne $tauriVersion -or $packageVersion -ne $cargoVersion) {
  throw "Version mismatch detected. package.json=$packageVersion, tauri.conf.json=$tauriVersion, Cargo.toml=$cargoVersion"
}

$resolvedChannel = if ([string]::IsNullOrWhiteSpace($Channel)) {
  if ([string]::IsNullOrWhiteSpace($env:DEVNEST_RELEASE_CHANNEL)) { "stable" } else { $env:DEVNEST_RELEASE_CHANNEL }
} else {
  $Channel
}

$resolvedKeyPath = if ([string]::IsNullOrWhiteSpace($KeyPath)) {
  if ([string]::IsNullOrWhiteSpace($env:DEVNEST_UPDATER_KEY_PATH)) {
    Join-Path $HOME ".tauri\\devnest-updater.key"
  } else {
    $env:DEVNEST_UPDATER_KEY_PATH
  }
} else {
  $KeyPath
}

$resolvedKeyPassword = if ([string]::IsNullOrWhiteSpace($KeyPassword)) {
  $env:DEVNEST_UPDATER_KEY_PASSWORD
} else {
  $KeyPassword
}

$resolvedRepoSlug = if ([string]::IsNullOrWhiteSpace($RepoSlug)) {
  if ([string]::IsNullOrWhiteSpace($env:DEVNEST_GITHUB_REPO)) { $defaultRepoSlug } else { $env:DEVNEST_GITHUB_REPO }
} else {
  $RepoSlug
}
$resolvedRepoSlug = Normalize-RepoSlug -Value $resolvedRepoSlug
$ghPath = Resolve-GhPath
$resolvedReleaseTag = if ([string]::IsNullOrWhiteSpace($ReleaseTag)) { "v$packageVersion" } else { $ReleaseTag }
$resolvedMetadataPublishPath = if ([string]::IsNullOrWhiteSpace($MetadataPublishPath)) {
  $env:DEVNEST_METADATA_PUBLISH_PATH
} else {
  $MetadataPublishPath
}
$resolvedUpdateEndpoint = if ([string]::IsNullOrWhiteSpace($UpdateEndpoint)) {
  if ([string]::IsNullOrWhiteSpace($env:DEVNEST_UPDATE_ENDPOINT)) {
    if ([string]::IsNullOrWhiteSpace($resolvedRepoSlug)) {
      ""
    } else {
      "https://github.com/$resolvedRepoSlug/releases/latest/download/$resolvedChannel.json"
    }
  } else {
    $env:DEVNEST_UPDATE_ENDPOINT
  }
} else {
  $UpdateEndpoint
}

$defaultVersionNotesPath = Join-Path $projectRoot "release-notes\\$packageVersion.md"
$resolvedNotesPath = if ([string]::IsNullOrWhiteSpace($NotesPath)) {
  if (Test-Path -LiteralPath $defaultVersionNotesPath) {
    $defaultVersionNotesPath
  } else {
    ""
  }
} else {
  $NotesPath
}

$keyDirectory = Split-Path -Parent $resolvedKeyPath
if (-not [string]::IsNullOrWhiteSpace($keyDirectory) -and -not (Test-Path -LiteralPath $keyDirectory)) {
  New-Item -ItemType Directory -Path $keyDirectory -Force | Out-Null
}

Write-Step "Ensuring updater signing keypair"
if (-not (Test-Path -LiteralPath $resolvedKeyPath)) {
  $signerArgs = @("signer", "generate", "--ci", "-w", $resolvedKeyPath)
  if (-not [string]::IsNullOrWhiteSpace($resolvedKeyPassword)) {
    $signerArgs += @("-p", $resolvedKeyPassword)
  }
  Invoke-Checked -FilePath $tauriCliPath -Arguments $signerArgs
}

$publicKeyPath = "$resolvedKeyPath.pub"
if (-not (Test-Path -LiteralPath $publicKeyPath)) {
  throw "Updater public key file was not found at $publicKeyPath"
}

$publicKey = (Get-Content -LiteralPath $publicKeyPath -Raw).Trim()
if ([string]::IsNullOrWhiteSpace($publicKey)) {
  throw "Updater public key file is empty: $publicKeyPath"
}
$privateKey = (Get-Content -LiteralPath $resolvedKeyPath -Raw).Trim()
if ([string]::IsNullOrWhiteSpace($privateKey)) {
  throw "Updater private key file is empty: $resolvedKeyPath"
}

$env:DEVNEST_RELEASE_CHANNEL = $resolvedChannel
$env:DEVNEST_UPDATE_ENDPOINT = $resolvedUpdateEndpoint
$env:DEVNEST_UPDATER_PUBLIC_KEY = $publicKey
$env:TAURI_SIGNING_PRIVATE_KEY = $privateKey
$env:TAURI_SIGNING_PRIVATE_KEY_PATH = $resolvedKeyPath

if ([string]::IsNullOrWhiteSpace($resolvedKeyPassword)) {
  Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD -ErrorAction SilentlyContinue
} else {
  $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $resolvedKeyPassword
}

$primaryBundle = $Bundle
$bundlesToProcess = if ($AllBundles) {
  @($primaryBundle) + @(@("nsis", "msi") | Where-Object { $_ -ne $primaryBundle })
} else {
  @($primaryBundle)
}
$secondaryBundles = @($bundlesToProcess | Where-Object { $_ -ne $primaryBundle })

Write-Step "Release inputs"
Write-Host "Version: $packageVersion"
Write-Host "Channel: $resolvedChannel"
Write-Host "Primary bundle: $primaryBundle"
Write-Host "Bundles to process: $($bundlesToProcess -join ', ')"
Write-Host "GitHub repo: $resolvedRepoSlug"
Write-Host "Update endpoint: $resolvedUpdateEndpoint"
Write-Host "Key path: $resolvedKeyPath"
Write-Host "Public key path: $publicKeyPath"

$resolvedRepoVisibility = Get-GitHubRepoVisibility -GhPath $ghPath -RepoSlug $resolvedRepoSlug
if (-not [string]::IsNullOrWhiteSpace($resolvedRepoVisibility)) {
  Write-Host "GitHub repo visibility: $resolvedRepoVisibility"
}

$usesGitHubReleaseEndpoint = -not [string]::IsNullOrWhiteSpace($resolvedRepoSlug) -and (
  $resolvedUpdateEndpoint -like "https://github.com/$resolvedRepoSlug/*" -or
  $resolvedUpdateEndpoint -like "https://github.com/$resolvedRepoSlug"
)

$effectiveSkipMetadata = [bool]$SkipMetadata
if (
  -not $effectiveSkipMetadata -and
  [string]::IsNullOrWhiteSpace($AssetUrl) -and
  [string]::IsNullOrWhiteSpace($env:DEVNEST_RELEASE_ASSET_URL) -and
  [string]::IsNullOrWhiteSpace($resolvedRepoSlug)
) {
  Write-Warning "No GitHub release repo or asset URL was configured. Skipping updater metadata generation for this local release. Set -RepoSlug, DEVNEST_GITHUB_REPO, -AssetUrl, or DEVNEST_RELEASE_ASSET_URL to publish updater metadata."
  $effectiveSkipMetadata = $true
}

if ($usesGitHubReleaseEndpoint -and $resolvedRepoVisibility -eq "PRIVATE") {
  throw "The configured release repository '$resolvedRepoSlug' is private. DevNest updater endpoints and release assets must be publicly reachable over HTTPS, so make the repository public or point DEVNEST_UPDATE_ENDPOINT / DEVNEST_RELEASE_ASSET_URL at a public host."
}

if (-not $SkipBuild) {
  Write-Step "Building frontend assets"
  Invoke-Checked -FilePath $viteCliPath -Arguments @("build")

  Stop-LockedBuildProcess -ExecutablePath $workspaceReleaseExecutablePath

  Write-Step "Building Windows $Bundle release"
  $updaterEndpoints = @(if (-not [string]::IsNullOrWhiteSpace($resolvedUpdateEndpoint)) {
    $resolvedUpdateEndpoint
  })
  $tauriBuildConfigOverride = @{
    build = @{
      beforeBuildCommand = $null
    }
    plugins = @{
      updater = @{
        endpoints = $updaterEndpoints
        pubkey = $publicKey
      }
    }
  }
  $tauriBuildConfigOverride = $tauriBuildConfigOverride | ConvertTo-Json -Depth 4 -Compress
  Write-Utf8NoBom -Path $tauriBuildConfigOverridePath -Value $tauriBuildConfigOverride
  foreach ($bundleName in $bundlesToProcess) {
    Write-Step "Building Windows $bundleName release"
    Invoke-Checked -FilePath $tauriCliPath -Arguments @(
      "build",
      "--ci",
      "--bundles", $bundleName,
      "--config", $tauriBuildConfigOverridePath
    )
  }
}

$releaseRoot = Join-Path $projectRoot "dist\\release\\windows\\$packageVersion"
$releaseAssetsDirectory = Join-Path $releaseRoot "assets"
$releaseMetadataDirectory = Join-Path $releaseRoot "metadata"
New-Item -ItemType Directory -Path $releaseAssetsDirectory -Force | Out-Null
New-Item -ItemType Directory -Path $releaseMetadataDirectory -Force | Out-Null

Write-Step "Collecting release artifacts"
$copiedPublicKey = Join-Path $releaseRoot "updater-public-key.txt"
$copiedPublicKeyPub = Join-Path $releaseRoot "updater-public-key.pub"
$bundleResults = @()

foreach ($bundleName in $bundlesToProcess) {
  $bundleDirectory = Join-Path $projectRoot "src-tauri\\target\\release\\bundle\\$bundleName"
  if (-not (Test-Path -LiteralPath $bundleDirectory)) {
    throw "Bundle directory not found: $bundleDirectory"
  }

  $bundleArtifactInfo = Get-BundleArtifactInfo -Directory $bundleDirectory -Bundle $bundleName -PackageVersion $packageVersion
  $copiedInstaller = Join-Path $releaseAssetsDirectory $bundleArtifactInfo.installer.Name
  $copiedUpdaterArtifact = Join-Path $releaseAssetsDirectory $bundleArtifactInfo.updaterArtifact.Name
  $copiedUpdaterSignature = Join-Path $releaseAssetsDirectory $bundleArtifactInfo.updaterSignature.Name

  Copy-Item -LiteralPath $bundleArtifactInfo.installer.FullName -Destination $copiedInstaller -Force
  Copy-Item -LiteralPath $bundleArtifactInfo.updaterArtifact.FullName -Destination $copiedUpdaterArtifact -Force
  Copy-Item -LiteralPath $bundleArtifactInfo.updaterSignature.FullName -Destination $copiedUpdaterSignature -Force

  $bundleResults += [PSCustomObject]@{
    bundle = $bundleName
    installer = $copiedInstaller
    updaterArtifact = $copiedUpdaterArtifact
    updaterSignature = $copiedUpdaterSignature
  }
}

Set-Content -LiteralPath $copiedPublicKey -Value $publicKey -Encoding UTF8
Copy-Item -LiteralPath $publicKeyPath -Destination $copiedPublicKeyPub -Force

$primaryBundleResult = @($bundleResults | Where-Object { $_.bundle -eq $primaryBundle } | Select-Object -First 1)
if ($primaryBundleResult.Count -eq 0) {
  throw "Primary bundle artifacts were not collected for '$primaryBundle'."
}
$primaryBundleResult = $primaryBundleResult[0]

$resolvedAssetUrl = if (-not [string]::IsNullOrWhiteSpace($AssetUrl)) {
  $AssetUrl
} elseif (-not [string]::IsNullOrWhiteSpace($env:DEVNEST_RELEASE_ASSET_URL)) {
  $env:DEVNEST_RELEASE_ASSET_URL
} elseif (-not [string]::IsNullOrWhiteSpace($resolvedRepoSlug)) {
  "https://github.com/$resolvedRepoSlug/releases/download/$resolvedReleaseTag/$([System.IO.Path]::GetFileName($primaryBundleResult.updaterArtifact))"
} else {
  ""
}

$metadataOutputPath = Join-Path $releaseMetadataDirectory "$resolvedChannel.json"
if (-not $effectiveSkipMetadata) {
  if ([string]::IsNullOrWhiteSpace($resolvedAssetUrl)) {
    throw "Cannot generate updater metadata without an asset URL. Set -AssetUrl, DEVNEST_RELEASE_ASSET_URL, or DEVNEST_GITHUB_REPO."
  }

  Write-Step "Generating updater metadata"
  $metadataArgs = @{
    Version = $packageVersion
    AssetUrl = $resolvedAssetUrl
    SignaturePath = $primaryBundleResult.updaterSignature
    OutputPath = $metadataOutputPath
  }

  if (-not [string]::IsNullOrWhiteSpace($resolvedNotesPath) -and (Test-Path -LiteralPath $resolvedNotesPath)) {
    $metadataArgs["NotesPath"] = $resolvedNotesPath
  }

  & $metadataScriptPath @metadataArgs

  if (-not [string]::IsNullOrWhiteSpace($resolvedMetadataPublishPath)) {
    $metadataPublishDirectory = Split-Path -Parent $resolvedMetadataPublishPath
    if (-not [string]::IsNullOrWhiteSpace($metadataPublishDirectory)) {
      New-Item -ItemType Directory -Path $metadataPublishDirectory -Force | Out-Null
    }
    Copy-Item -LiteralPath $metadataOutputPath -Destination $resolvedMetadataPublishPath -Force
  }
} else {
  Write-Step "Skipping updater metadata generation"
}

$archiveProductName = if ([string]::IsNullOrWhiteSpace($productName)) { "DevNest" } else { $productName }
$portableArchiveName = "$archiveProductName-v$packageVersion-portable.zip"
$portableArchivePath = Join-Path $releaseAssetsDirectory $portableArchiveName
$portableArchiveStagingDirectory = Join-Path $releaseRoot "portable-package"
$portableArchiveFiles = @()
foreach ($bundleResult in $bundleResults) {
  $portableArchiveFiles += @($bundleResult.installer, $bundleResult.updaterArtifact, $bundleResult.updaterSignature)
}
if (-not $effectiveSkipMetadata) {
  $portableArchiveFiles += $metadataOutputPath
}
$portableArchiveFiles = @($portableArchiveFiles | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -Unique)

if ($portableArchiveFiles.Count -eq 0) {
  throw "Portable archive staging list is empty."
}

Write-Step "Creating portable release archive"
if (Test-Path -LiteralPath $portableArchiveStagingDirectory) {
  Remove-Item -LiteralPath $portableArchiveStagingDirectory -Recurse -Force
}
New-Item -ItemType Directory -Path $portableArchiveStagingDirectory -Force | Out-Null

foreach ($archiveFile in $portableArchiveFiles) {
  $archiveFileItem = Get-Item -LiteralPath $archiveFile
  Copy-Item -LiteralPath $archiveFileItem.FullName -Destination (Join-Path $portableArchiveStagingDirectory $archiveFileItem.Name) -Force
}

if (Test-Path -LiteralPath $portableArchivePath) {
  Remove-Item -LiteralPath $portableArchivePath -Force
}

Compress-Archive -Path (Join-Path $portableArchiveStagingDirectory "*") -DestinationPath $portableArchivePath -CompressionLevel Optimal -Force

if (-not $SkipGitHubRelease) {
  if ([string]::IsNullOrWhiteSpace($resolvedRepoSlug)) {
    Write-Warning "Skipping GitHub Release upload because DEVNEST_GITHUB_REPO / -RepoSlug was not provided."
  } else {
    if ([string]::IsNullOrWhiteSpace($ghPath)) {
    Write-Warning "Skipping GitHub Release upload because the GitHub CLI (gh) is not installed."
    } else {
    Ensure-GitHubReleaseRepoInitialized -GhPath $ghPath -RepoSlug $resolvedRepoSlug
    Write-Step "Uploading artifacts to GitHub Release $resolvedReleaseTag"
    $releaseExists = Test-GitHubReleaseExists -GhPath $ghPath -RepoSlug $resolvedRepoSlug -ReleaseTag $resolvedReleaseTag
    $releaseNotesArgs = if (-not [string]::IsNullOrWhiteSpace($resolvedNotesPath) -and (Test-Path -LiteralPath $resolvedNotesPath)) {
      @("--notes-file", $resolvedNotesPath)
    } else {
      @("--notes", "DevNest $packageVersion")
    }

    if (-not $releaseExists) {
      $ghCreateArgs = @(
        "release", "create", $resolvedReleaseTag,
        "--repo", $resolvedRepoSlug,
        "--title", $resolvedReleaseTag
      ) + $releaseNotesArgs
      Invoke-Checked -FilePath $ghPath -Arguments $ghCreateArgs
    } elseif ($releaseNotesArgs[0] -eq "--notes-file") {
      $ghEditArgs = @(
        "release", "edit", $resolvedReleaseTag,
        "--repo", $resolvedRepoSlug,
        "--title", $resolvedReleaseTag
      ) + $releaseNotesArgs
      Invoke-Checked -FilePath $ghPath -Arguments $ghEditArgs
    }

    $uploadFiles = @($portableArchivePath)
    foreach ($bundleResult in $bundleResults) {
      $uploadFiles += @($bundleResult.installer, $bundleResult.updaterArtifact, $bundleResult.updaterSignature)
    }
    if (-not $effectiveSkipMetadata) {
      $uploadFiles += $metadataOutputPath
    }
    $uploadFiles = @($uploadFiles | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -Unique)
    $ghUploadArgs = @(
      "release", "upload", $resolvedReleaseTag,
      "--repo", $resolvedRepoSlug,
      "--clobber"
    ) + $uploadFiles
    Invoke-Checked -FilePath $ghPath -Arguments $ghUploadArgs
    }
  }
} else {
  Write-Step "Skipping GitHub Release upload"
}

$summary = [ordered]@{
  version = $packageVersion
  channel = $resolvedChannel
  releaseTag = $resolvedReleaseTag
  primaryBundle = $primaryBundle
  bundles = $bundleResults
  secondaryBundles = $secondaryBundles
  allBundles = [bool]$AllBundles
  updateEndpoint = $resolvedUpdateEndpoint
  repoSlug = $resolvedRepoSlug
  installer = $primaryBundleResult.installer
  updaterArtifact = $primaryBundleResult.updaterArtifact
  updaterSignature = $primaryBundleResult.updaterSignature
  portableArchive = $portableArchivePath
  publicKeyFile = $copiedPublicKey
  metadataFile = if ($effectiveSkipMetadata) { $null } else { $metadataOutputPath }
  metadataPublishPath = if ([string]::IsNullOrWhiteSpace($resolvedMetadataPublishPath)) { $null } else { $resolvedMetadataPublishPath }
  assetUrl = if ([string]::IsNullOrWhiteSpace($resolvedAssetUrl)) { $null } else { $resolvedAssetUrl }
}

$summaryPath = Join-Path $releaseRoot "release-summary.json"
$summaryJson = $summary | ConvertTo-Json -Depth 6
Write-Utf8NoBom -Path $summaryPath -Value $summaryJson

Write-Step "Windows release prepared"
Write-Host "Primary installer: $($primaryBundleResult.installer)"
Write-Host "Primary updater artifact: $($primaryBundleResult.updaterArtifact)"
Write-Host "Primary updater signature: $($primaryBundleResult.updaterSignature)"
if ($secondaryBundles.Count -gt 0) {
  Write-Host "Secondary bundles: $($secondaryBundles -join ', ')"
}
Write-Host "Portable archive: $portableArchivePath"
Write-Host "Public key: $copiedPublicKey"
if (-not $effectiveSkipMetadata) {
  Write-Host "Metadata: $metadataOutputPath"
  if (-not [string]::IsNullOrWhiteSpace($resolvedMetadataPublishPath)) {
    Write-Host "Published metadata copy: $resolvedMetadataPublishPath"
  }
}
Write-Host "Summary: $summaryPath"
