param(
  [Parameter(Mandatory = $true)]
  [string]$Version,

  [Parameter(Mandatory = $true)]
  [string]$AssetUrl,

  [Parameter(Mandatory = $true)]
  [string]$SignaturePath,

  [Parameter(Mandatory = $true)]
  [string]$OutputPath,

  [string]$NotesPath,

  [string]$Platform = "windows-x86_64",

  [string]$PubDate = ""
)

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

$resolvedSignaturePath = Resolve-Path -LiteralPath $SignaturePath
$signature = (Get-Content -LiteralPath $resolvedSignaturePath -Raw).Trim()

if ([string]::IsNullOrWhiteSpace($signature)) {
  throw "Signature file is empty: $SignaturePath"
}

$notes = $null
if (-not [string]::IsNullOrWhiteSpace($NotesPath)) {
  $resolvedNotesPath = Resolve-Path -LiteralPath $NotesPath
  $notes = (Get-Content -LiteralPath $resolvedNotesPath -Raw).Trim()
  if ([string]::IsNullOrWhiteSpace($notes)) {
    $notes = $null
  }
}

if ([string]::IsNullOrWhiteSpace($PubDate)) {
  $PubDate = [DateTimeOffset]::UtcNow.ToString("o")
}

$metadata = [ordered]@{
  version = $Version
  notes = $notes
  pub_date = $PubDate
  platforms = [ordered]@{
    $Platform = [ordered]@{
      signature = $signature
      url = $AssetUrl
    }
  }
}

$outputDirectory = Split-Path -Parent $OutputPath
if (-not [string]::IsNullOrWhiteSpace($outputDirectory) -and -not (Test-Path -LiteralPath $outputDirectory)) {
  New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null
}

$json = $metadata | ConvertTo-Json -Depth 8
Write-Utf8NoBom -Path $OutputPath -Value $json

Write-Output "Updater metadata written to $OutputPath"
