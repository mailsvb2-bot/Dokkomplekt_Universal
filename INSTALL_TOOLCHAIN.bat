@echo off
setlocal EnableExtensions
cd /d "%~dp0"
title Install toolchain for Dokkomplekt Universal

if not exist launcher_logs mkdir launcher_logs
set "LOG=%~dp0launcher_logs\toolchain_install.log"

echo Dokkomplekt Universal toolchain install > "%LOG%"
echo Started: %DATE% %TIME% >> "%LOG%"

echo.
echo ============================================================
echo  Installing required tools for Dokkomplekt Universal
echo ============================================================
echo.
echo Required for SOURCE launch:
echo - Node.js LTS
 echo - Rustup / exact Rust 1.97.1
 echo - Microsoft Visual Studio Build Tools C++
 echo - Microsoft Edge WebView2 Runtime
 echo.

where winget >nul 2>nul
if errorlevel 1 goto no_winget

echo winget found. Installation will start now.
echo This may take several minutes.
echo.

call winget install --id OpenJS.NodeJS.LTS -e --accept-package-agreements --accept-source-agreements >> "%LOG%" 2>&1
call winget install --id Rustlang.Rustup -e --accept-package-agreements --accept-source-agreements >> "%LOG%" 2>&1
call winget install --id Microsoft.EdgeWebView2Runtime -e --accept-package-agreements --accept-source-agreements >> "%LOG%" 2>&1
call winget install --id Microsoft.VisualStudio.2022.BuildTools -e --accept-package-agreements --accept-source-agreements --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended" >> "%LOG%" 2>&1

call :refresh_path
where rustup >nul 2>nul
if not errorlevel 1 (
  call rustup toolchain install 1.97.1 --profile minimal --component clippy,rustfmt >> "%LOG%" 2>&1
  call rustup override set 1.97.1 >> "%LOG%" 2>&1
)

echo.
echo Installation commands finished.
echo Re-checking tools in this window...
echo.
where node >nul 2>nul && node --version
where npm >nul 2>nul && npm --version
where cargo >nul 2>nul && cargo --version
where rustc >nul 2>nul && rustc --version

echo.
echo If any tool is still missing, close this window and run main.bat again,
echo or restart Windows once.
echo.
echo Log: %LOG%
pause
exit /b 0

:no_winget
echo winget is not available on this Windows.
echo Opening official install pages in browser...
start "" "https://nodejs.org/en/download"
start "" "https://rustup.rs"
start "" "https://developer.microsoft.com/en-us/microsoft-edge/webview2/"
start "" "https://visualstudio.microsoft.com/visual-cpp-build-tools/"
echo.
echo Install Node.js LTS, Rustup, WebView2 Runtime and Visual Studio Build Tools C++ manually.
echo Then run: rustup toolchain install 1.97.1 --profile minimal --component clippy,rustfmt
echo Then restart Windows or reopen Explorer/CMD and run main.bat again.
echo Log: %LOG%
pause
exit /b 3

:refresh_path
set "PATH=%ProgramFiles%\nodejs;%ProgramFiles(x86)%\nodejs;%USERPROFILE%\.cargo\bin;%PATH%"
exit /b 0
