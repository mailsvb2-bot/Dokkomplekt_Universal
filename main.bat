@echo off
setlocal EnableExtensions EnableDelayedExpansion
cd /d "%~dp0"
title Dokkomplekt Universal - launcher

if not exist launcher_logs mkdir launcher_logs
set "LOG=%~dp0launcher_logs\last_launch.log"
>"%LOG%" echo Dokkomplekt Universal launcher
>>"%LOG%" echo Started: %DATE% %TIME%
>>"%LOG%" echo Root: %CD%

call :launch_prebuilt
if not errorlevel 10 exit /b %errorlevel%

echo.
echo ============================================================
echo  Dokkomplekt Universal - запуск исходного проекта
echo ============================================================
echo.

call :refresh_path
call :check_tools
if errorlevel 1 goto missing_tools

set "CARGO_TARGET_DIR=%LOCALAPPDATA%\Dokkomplekt\cargo-target"
if not exist "%CARGO_TARGET_DIR%" mkdir "%CARGO_TARGET_DIR%"
>>"%LOG%" echo Cargo target: %CARGO_TARGET_DIR%

if not exist "node_modules\.package-lock.json" (
  echo Устанавливаю зависимости интерфейса...
  >>"%LOG%" echo Running npm ci
  call npm ci >>"%LOG%" 2>&1
  if errorlevel 1 goto failed
)

if not exist Cargo.lock (
  echo Создаю Cargo.lock...
  >>"%LOG%" echo Running cargo generate-lockfile
  call cargo generate-lockfile >>"%LOG%" 2>&1
  if errorlevel 1 goto failed
)

echo Запускаю Dokkomplekt Universal...
>>"%LOG%" echo Running npm run tauri:dev
call npm run tauri:dev >>"%LOG%" 2>&1
if errorlevel 1 goto failed
exit /b 0

:launch_prebuilt
for %%E in (
  "%~dp0dokkomplekt-tauri.exe"
  "%~dp0Dokkomplekt Universal.exe"
  "%~dp0target\release\dokkomplekt-tauri.exe"
  "%LOCALAPPDATA%\Programs\Dokkomplekt Universal\dokkomplekt-tauri.exe"
) do (
  if exist "%%~fE" (
    >>"%LOG%" echo Launching prebuilt: %%~fE
    start "" "%%~fE"
    exit /b 0
  )
)
exit /b 10

:refresh_path
set "PATH=%USERPROFILE%\.cargo\bin;%ProgramFiles%\nodejs;%ProgramFiles(x86)%\nodejs;%LOCALAPPDATA%\Programs\nodejs;%PATH%"
exit /b 0

:check_tools
set "MISSING="
for %%T in (node npm cargo rustc) do (
  where %%T >nul 2>nul
  if errorlevel 1 set "MISSING=!MISSING! %%T"
)
if defined MISSING (
  >>"%LOG%" echo Missing tools:!MISSING!
  exit /b 1
)
for /f "delims=" %%V in ('node --version 2^>nul') do >>"%LOG%" echo Node: %%V
for /f "delims=" %%V in ('npm --version 2^>nul') do >>"%LOG%" echo npm: %%V
for /f "delims=" %%V in ('rustc --version 2^>nul') do >>"%LOG%" echo Rust: %%V
for /f "delims=" %%V in ('cargo --version 2^>nul') do >>"%LOG%" echo Cargo: %%V
exit /b 0

:missing_tools
echo.
echo Не найдены инструменты разработки:!MISSING!
echo Этот файл запускает ИСХОДНЫЙ проект. Для обычного пользователя нужен готовый setup.exe.
echo Для разработки сначала запустите INSTALL_TOOLCHAIN.bat, затем снова main.bat.
echo.
echo Журнал: %LOG%
pause
exit /b 2

:failed
echo.
echo ОШИБКА: приложение не запустилось. Последние строки журнала:
echo ------------------------------------------------------------
powershell -NoProfile -ExecutionPolicy Bypass -Command "if (Test-Path -LiteralPath '%LOG%') { Get-Content -LiteralPath '%LOG%' -Tail 60 }"
echo ------------------------------------------------------------
echo Полный журнал: %LOG%
pause
exit /b 1
