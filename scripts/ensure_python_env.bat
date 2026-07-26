@echo off
setlocal EnableExtensions
cd /d "%~dp0\.."

if exist ".venv\Scripts\python.exe" goto install
where py >nul 2>nul && (
  py -3 -m venv .venv || exit /b 1
  goto install
)
where python >nul 2>nul || (
  echo ERROR: Python 3 is not installed or not in PATH.
  exit /b 2
)
python -m venv .venv || exit /b 1

:install
.venv\Scripts\python.exe -m pip install --disable-pip-version-check --no-input -r requirements-dev.txt || exit /b 1
exit /b 0
