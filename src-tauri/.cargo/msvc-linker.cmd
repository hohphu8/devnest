@echo off
setlocal

set "VS_ROOT="
for %%R in (
  "C:\Program Files\Microsoft Visual Studio\18\Community"
  "C:\Program Files\Microsoft Visual Studio\18\BuildTools"
  "C:\Program Files\Microsoft Visual Studio\18\Professional"
  "C:\Program Files\Microsoft Visual Studio\18\Enterprise"
) do (
  if exist "%%~R\VC\Tools\MSVC" (
    set "VS_ROOT=%%~R"
    goto :vs_root_found
  )
)

echo MSVC installation root not found. 1>&2
exit /b 1

:vs_root_found
set "MSVC_VERSION="
for /f "delims=" %%V in ('dir /b /ad "%VS_ROOT%\VC\Tools\MSVC" ^| sort /r') do (
  set "MSVC_VERSION=%%V"
  goto :msvc_version_found
)

echo MSVC version folder not found under "%VS_ROOT%\VC\Tools\MSVC". 1>&2
exit /b 1

:msvc_version_found
set "SDK_ROOT=C:\Program Files (x86)\Windows Kits\10"
if not exist "%SDK_ROOT%\Lib" (
  echo Windows SDK lib root not found under "%SDK_ROOT%\Lib". 1>&2
  exit /b 1
)

set "SDK_VERSION="
for /f "delims=" %%V in ('dir /b /ad "%SDK_ROOT%\Lib" ^| sort /r') do (
  set "SDK_VERSION=%%V"
  goto :sdk_version_found
)

echo Windows SDK version folder not found under "%SDK_ROOT%\Lib". 1>&2
exit /b 1

:sdk_version_found
set "LINK_EXE=%VS_ROOT%\VC\Tools\MSVC\%MSVC_VERSION%\bin\Hostx64\x64\link.exe"
if not exist "%LINK_EXE%" set "LINK_EXE=%VS_ROOT%\VC\Tools\MSVC\%MSVC_VERSION%\bin\HostX64\x64\link.exe"

if not exist "%LINK_EXE%" (
  echo MSVC link.exe not found under "%VS_ROOT%". 1>&2
  exit /b 1
)

set "LIB=%VS_ROOT%\VC\Tools\MSVC\%MSVC_VERSION%\lib\x64;%SDK_ROOT%\Lib\%SDK_VERSION%\ucrt\x64;%SDK_ROOT%\Lib\%SDK_VERSION%\um\x64;%LIB%"
set "PATH=%VS_ROOT%\VC\Tools\MSVC\%MSVC_VERSION%\bin\Hostx64\x64;%VS_ROOT%\VC\Tools\MSVC\%MSVC_VERSION%\bin\HostX64\x64;%PATH%"

"%LINK_EXE%" %*
