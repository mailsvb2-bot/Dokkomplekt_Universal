@echo off
setlocal EnableExtensions
cd /d "%~dp0"

echo ============================================================
echo DOKKOMPLEKT UNIVERSAL - FREE UNSIGNED WINDOWS PREVIEW
echo This build is for testing. Windows may show Unknown publisher.
echo ============================================================

where node >nul 2>nul || (echo ERROR: Node.js is missing. & exit /b 2)
where npm >nul 2>nul || (echo ERROR: npm is missing. & exit /b 2)
where cargo >nul 2>nul || (echo ERROR: Rust/Cargo is missing. & exit /b 2)
where powershell >nul 2>nul || (echo ERROR: PowerShell is missing. & exit /b 2)

call npm ci || exit /b 1
call npm run typecheck || exit /b 1
call npm run test || exit /b 1
call npm run build || exit /b 1
call npx tauri build --bundles nsis --config src-tauri\tauri.thin.conf.json || exit /b 1

set "DOKKOMPLEKT_REQUIRE_AUTHENTICODE=0"
powershell -NoProfile -ExecutionPolicy Bypass -File tests\installer\windows_installer_contract.ps1 -TauriConfig src-tauri\tauri.thin.conf.json -ExpectedWebViewMode downloadBootstrapper || exit /b 1

powershell -NoProfile -ExecutionPolicy Bypass -Command "$installer = Get-ChildItem 'target\release\bundle\nsis' -File -Filter '*.exe' ^| Select-Object -First 1; if ($null -eq $installer) { throw 'NSIS installer was not created.' }; $signature = Get-AuthenticodeSignature -FilePath $installer.FullName; if ($signature.Status -ne 'NotSigned') { throw ('Unexpected signature status: ' + $signature.Status) }; Write-Host ('UNSIGNED PREVIEW CREATED: ' + $installer.FullName); Write-Host 'Windows may show Unknown publisher or SmartScreen. This is expected.'" || exit /b 1

echo.
echo FREE UNSIGNED PREVIEW CREATED AND VERIFIED.
echo Folder: target\release\bundle\nsis
echo.
exit /b 0
