param(
  [ValidateSet('Run','Build','Gate')]
  [string]$Mode = 'Run'
)
$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath (Split-Path -Parent $PSScriptRoot)
if ($Mode -eq 'Run') {
  cmd.exe /c main.bat
} elseif ($Mode -eq 'Build') {
  cmd.exe /c "BUILD_EXE.bat"
} else {
  cmd.exe /c "CHECK_PROJECT.bat"
}
exit $LASTEXITCODE
