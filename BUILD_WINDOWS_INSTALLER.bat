@echo off
setlocal EnableExtensions EnableDelayedExpansion
cd /d "%~dp0"

if "%DOKKOMPLEKT_SIDECAR_MANIFEST%"=="" (
  echo ERROR: DOKKOMPLEKT_SIDECAR_MANIFEST is not set.
  echo A production offline installer must be built from a reviewed manifest containing Tesseract rus+eng, Poppler, LibreOffice, SumatraPDF, llama.cpp and a GGUF model.
  exit /b 1
)

where node >nul 2>nul || (echo ERROR: Node.js is missing. & exit /b 2)
where npm >nul 2>nul || (echo ERROR: npm is missing. & exit /b 2)
where cargo >nul 2>nul || (echo ERROR: Rust/Cargo is missing. & exit /b 2)
call scripts\ensure_python_env.bat || exit /b 1
call npm ci || exit /b 1
.venv\Scripts\python.exe scripts\prepare_sidecars.py "%DOKKOMPLEKT_SIDECAR_MANIFEST%" --clean || exit /b 1
.venv\Scripts\python.exe scripts\assert_offline_runtime_ready.py --target windows-x86_64 --require-semantic-model --require-supply-chain --production || exit /b 1
.venv\Scripts\python.exe scripts\probe_offline_runtime.py --target windows-x86_64 || exit /b 1
.venv\Scripts\python.exe scripts\run_python_contracts_sharded.py --report verification\installer\python-contracts.json || exit /b 1
.venv\Scripts\python.exe scripts\verify_starter_content_packs.py || exit /b 1

call scripts\prepackage_rust_gate.bat
if errorlevel 1 exit /b 1
.venv\Scripts\python.exe scripts\assert_release_ready.py || exit /b 1
call npm run typecheck || exit /b 1
call npm run test || exit /b 1
call npm run build || exit /b 1
call npm run tauri:build -- --bundles nsis || exit /b 1
