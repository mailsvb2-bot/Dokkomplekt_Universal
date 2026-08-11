@echo off
setlocal EnableExtensions EnableDelayedExpansion
cd /d "%~dp0"

if "%DOKKOMPLEKT_SIDECAR_MANIFEST%"=="" (
  echo ERROR: DOKKOMPLEKT_SIDECAR_MANIFEST is not set.
  echo The offline installer requires a reviewed manifest for OCR and Office sidecars.
  exit /b 1
)

rem This entrypoint is production-only. Authenticode must use a non-exportable
rem certificate-store key exposed by the approved HSM/KSP/CSP provider.
set "DOKKOMPLEKT_RELEASE_MODE=production"
if /I not "%DOKKOMPLEKT_WINDOWS_SIGNING_BACKEND%"=="certificate-store" (
  echo ERROR: DOKKOMPLEKT_WINDOWS_SIGNING_BACKEND must be certificate-store for production.
  exit /b 1
)
if "%DOKKOMPLEKT_WINDOWS_SIGNING_CERT_THUMBPRINT%"=="" (
  echo ERROR: DOKKOMPLEKT_WINDOWS_SIGNING_CERT_THUMBPRINT is required.
  exit /b 1
)
if "%DOKKOMPLEKT_WINDOWS_SIGNING_ALLOWED_PROVIDER%"=="" (
  echo ERROR: DOKKOMPLEKT_WINDOWS_SIGNING_ALLOWED_PROVIDER is required.
  exit /b 1
)
if "%DOKKOMPLEKT_TIMESTAMP_SERVER%"=="" (
  echo ERROR: DOKKOMPLEKT_TIMESTAMP_SERVER is required for production signing.
  exit /b 1
)
if not "%DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64%"=="" (
  echo ERROR: DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64 is forbidden in production.
  exit /b 1
)
if not "%DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD%"=="" (
  echo ERROR: DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD is forbidden in production.
  exit /b 1
)

where node >nul 2>nul || (echo ERROR: Node.js is missing. & exit /b 2)
where npm >nul 2>nul || (echo ERROR: npm is missing. & exit /b 2)
where cargo >nul 2>nul || (echo ERROR: Rust/Cargo is missing. & exit /b 2)
where powershell >nul 2>nul || (echo ERROR: PowerShell is missing. & exit /b 2)

call scripts\ensure_python_env.bat || exit /b 1
call npm ci || exit /b 1
.venv\Scripts\python.exe scripts\prepare_sidecars.py "%DOKKOMPLEKT_SIDECAR_MANIFEST%" --clean || exit /b 1
.venv\Scripts\python.exe scripts\assert_offline_runtime_ready.py --target windows-x86_64 --require-semantic-model --require-supply-chain --production || exit /b 1
.venv\Scripts\python.exe scripts\verify_windows_runtime_app_parity.py --target windows-x86_64 || exit /b 1
.venv\Scripts\python.exe scripts\probe_offline_runtime.py --target windows-x86_64 || exit /b 1
.venv\Scripts\python.exe scripts\run_python_contracts_sharded.py --report verification\installer\python-contracts.json || exit /b 1
call scripts\prepackage_rust_gate.bat || exit /b 1
.venv\Scripts\python.exe scripts\assert_release_ready.py || exit /b 1
call npm run typecheck || exit /b 1
call npm run test || exit /b 1
call npm run build || exit /b 1

call npx tauri build --no-bundle || exit /b 1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\sign_windows_release.ps1 -ArtifactRoot target\release\dokkomplekt-tauri.exe || exit /b 1
call npx tauri bundle --bundles nsis --config src-tauri\tauri.offline.conf.json || exit /b 1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\sign_windows_release.ps1 -ArtifactRoot target\release\bundle\nsis || exit /b 1
set "DOKKOMPLEKT_REQUIRE_AUTHENTICODE=1"
powershell -NoProfile -ExecutionPolicy Bypass -File tests\installer\windows_installer_contract.ps1 -TauriConfig src-tauri\tauri.offline.conf.json -ExpectedWebViewMode offlineInstaller || exit /b 1

echo SIGNED OFFLINE INSTALLER CREATED AND VERIFIED.
