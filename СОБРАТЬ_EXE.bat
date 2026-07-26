@echo off
setlocal EnableExtensions
cd /d "%~dp0"
title Dokkomplekt Universal - production Windows installer

echo This command builds only the complete offline production installer.
echo It intentionally uses the same fail-closed gates as BUILD_WINDOWS_INSTALLER.bat.
echo.
call BUILD_WINDOWS_INSTALLER.bat
exit /b %errorlevel%
