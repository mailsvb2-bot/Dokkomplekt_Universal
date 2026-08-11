@echo off
setlocal EnableExtensions DisableDelayedExpansion
chcp 65001 >nul 2>&1
title Dokkomplekt Hardware Runner Setup

set "PUBLIC_RAW=https://raw.githubusercontent.com/mailsvb2-bot/Dokkomplekt_Universal/main/scripts"
set "BOOTDIR=%TEMP%\DokkomplektHardwareRunnerSetup"

net session >nul 2>&1
if errorlevel 1 (
  echo [INFO] Запрашиваю права администратора...
  powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "Start-Process -FilePath $env:ComSpec -ArgumentList '/d /s /c ""%~f0"" %*' -Verb RunAs"
  exit /b
)

if exist "%BOOTDIR%" rmdir /s /q "%BOOTDIR%" >nul 2>&1
mkdir "%BOOTDIR%" >nul 2>&1
if errorlevel 1 goto :fail

echo [1/4] Загружаю проверенный bootstrap из Dokkomplekt_Universal...
call :download "%PUBLIC_RAW%/setup_windows_hardware_runner_from_cmd.ps1" "%BOOTDIR%\setup_windows_hardware_runner_from_cmd.ps1"
if errorlevel 1 goto :fail
call :download "%PUBLIC_RAW%/register_windows_hardware_evidence_runner.ps1" "%BOOTDIR%\register_windows_hardware_evidence_runner.ps1"
if errorlevel 1 goto :fail
call :download "%PUBLIC_RAW%/bootstrap_private_windows_runner.ps1" "%BOOTDIR%\bootstrap_private_windows_runner.ps1"
if errorlevel 1 goto :fail

echo [2/4] Проверяю Windows, Word, принтер и необходимые инструменты...
echo [3/4] Откроется страница GitHub. Скопируй временный registration token.
echo [4/4] Вставь token в это окно, когда bootstrap его попросит.
echo.
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%BOOTDIR%\setup_windows_hardware_runner_from_cmd.ps1" %*
set "RC=%ERRORLEVEL%"
if not "%RC%"=="0" goto :failcode

echo.
echo ============================================================
echo ГОТОВО: этот ПК зарегистрирован как dokkomplekt-hardware.
echo Runner запускается автоматически при входе в Windows.
echo ============================================================
echo.
pause
exit /b 0

:download
powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "$ErrorActionPreference='Stop'; [Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; (New-Object Net.WebClient).DownloadFile('%~1','%~2')" >nul
exit /b %ERRORLEVEL%

:failcode
echo.
echo [ERROR] Настройка завершилась с кодом %RC%.
echo Ничего не обходилось и не подменялось. Исправь указанную выше причину и запусти этот CMD повторно.
pause
exit /b %RC%

:fail
echo.
echo [ERROR] Не удалось подготовить bootstrap.
echo Проверь доступ в интернет и повтори запуск.
pause
exit /b 1
