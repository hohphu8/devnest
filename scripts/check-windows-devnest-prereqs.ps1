$ErrorActionPreference = "SilentlyContinue"

function Test-CommandExists {
  param([string]$Name)

  $command = Get-Command $Name
  return $null -ne $command
}

function Get-FirstPath {
  param([string[]]$Candidates)

  foreach ($candidate in $Candidates) {
    if (Test-Path $candidate) {
      return $candidate
    }
  }

  return $null
}

function Write-Result {
  param(
    [string]$Label,
    [bool]$Ok,
    [string]$Details
  )

  $status = if ($Ok) { "OK" } else { "MISSING" }
  Write-Output ("[{0}] {1} - {2}" -f $status, $Label, $Details)
}

$nodeOk = Test-CommandExists "node"
$npmOk = Test-CommandExists "npm"
$cargoOk = Test-CommandExists "cargo"
$rustcOk = Test-CommandExists "rustc"

$nodeDetails = if ($nodeOk) { node --version } else { "node not found" }
$npmDetails = if ($npmOk) { npm --version } else { "npm not found" }
$cargoDetails = if ($cargoOk) { cargo --version } else { "cargo not found" }
$rustcDetails = if ($rustcOk) { rustc --version } else { "rustc not found" }

Write-Result "Node.js" $nodeOk $nodeDetails
Write-Result "npm" $npmOk $npmDetails
Write-Result "cargo" $cargoOk $cargoDetails
Write-Result "rustc" $rustcOk $rustcDetails

$msvcLink = Get-FirstPath @(
  "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Tools\MSVC",
  "C:\Program Files\Microsoft Visual Studio\18\BuildTools\VC\Tools\MSVC",
  "C:\Program Files\Microsoft Visual Studio\18\Professional\VC\Tools\MSVC",
  "C:\Program Files\Microsoft Visual Studio\18\Enterprise\VC\Tools\MSVC",
  "C:\Program Files\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC",
  "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC",
  "C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Tools\MSVC",
  "C:\Program Files\Microsoft Visual Studio\2022\Enterprise\VC\Tools\MSVC",
  "C:\Program Files\Microsoft Visual Studio\17\Community\VC\Tools\MSVC",
  "C:\Program Files\Microsoft Visual Studio\17\BuildTools\VC\Tools\MSVC"
)

$sdkKernelLib = Get-FirstPath @(
  "C:\Program Files (x86)\Windows Kits\10\Lib",
  "C:\Program Files\Windows Kits\10\Lib"
)

$msvcDetails = if ($msvcLink) { $msvcLink } else { "Visual Studio C++ Build Tools not found" }
$sdkDetails = if ($sdkKernelLib) { $sdkKernelLib } else { "Windows SDK libraries not found" }

Write-Result "MSVC toolchain root" ($null -ne $msvcLink) $msvcDetails
Write-Result "Windows SDK lib root" ($null -ne $sdkKernelLib) $sdkDetails

$pathLink = (Get-Command link.exe | Select-Object -ExpandProperty Source -First 1)
$pathLinkDetails = if ($pathLink) { $pathLink } else { "link.exe not found in PATH" }
$repoLinkerConfig = Test-Path "src-tauri/.cargo/msvc-linker.cmd"
$repoLinkerDetails = if ($repoLinkerConfig) { "src-tauri/.cargo/msvc-linker.cmd" } else { "repo linker wrapper not found" }

Write-Result "PATH linker" ($pathLink -and ($pathLink -notmatch "git[\\/]+usr[\\/]+bin[\\/]+link\.exe$")) $pathLinkDetails
Write-Result "Repo linker wrapper" $repoLinkerConfig $repoLinkerDetails

Write-Output ""
Write-Output "Expected minimum native setup for Tauri on Windows:"
Write-Output "1. Visual Studio Build Tools with Desktop C++ workload"
Write-Output "2. Windows 10/11 SDK libraries"
Write-Output "3. Rust MSVC target"

if (-not $msvcLink -or -not $sdkKernelLib) {
  exit 1
}
