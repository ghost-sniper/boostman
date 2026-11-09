$ErrorActionPreference = 'Stop'

Push-Location -LiteralPath (Join-Path $PSScriptRoot '..')
try {
    $npm = if ([Environment]::OSVersion.Platform -eq 'Win32NT') { 'npm.cmd' } else { 'npm' }
    & $npm run build:local
    exit $LASTEXITCODE
} finally {
    Pop-Location
}
