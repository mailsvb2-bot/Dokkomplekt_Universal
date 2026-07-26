@echo off
setlocal EnableExtensions
cd /d "%~dp0"
title Dokkomplekt Universal - full project check
where node >nul 2>nul || goto missing_toolchain
where npm >nul 2>nul || goto missing_toolchain
where cargo >nul 2>nul || goto missing_toolchain
call scripts\ensure_python_env.bat
if errorlevel 1 goto missing_python
call npm ci
if errorlevel 1 goto failed
if not exist Cargo.lock cargo generate-lockfile
if errorlevel 1 goto failed
call .venv\Scripts\python.exe scripts\run_python_contracts_sharded.py --report verification\local\python-contracts.json
if errorlevel 1 goto failed
call .venv\Scripts\python.exe scripts\verify_starter_content_packs.py
if errorlevel 1 goto failed
call scripts\prepackage_rust_gate.bat
if errorlevel 1 goto failed
call npm run typecheck
if errorlevel 1 goto failed
call npm run test
if errorlevel 1 goto failed
call npm run build
if errorlevel 1 goto failed
echo.
echo ALL AVAILABLE PROJECT CHECKS PASSED.
pause
exit /b 0
:missing_toolchain
echo Missing Node/npm/Cargo. Run INSTALL_TOOLCHAIN.bat first.
pause
exit /b 2
:missing_python
echo Missing Python 3 or Python dependencies could not be installed.
pause
exit /b 2
:failed
echo Checks failed. No release artifact is approved.
pause
exit /b 1
