# Restaura la base desde una copia. PARA EL SERVIDOR ANTES.
# Uso: .\scripts\restore.ps1 [-Backup latest|ruta\ao-AAAAMMDD-HHMMSS.db] [-Config config\server.toml]
# Sin argumentos lista las copias disponibles. La base actual se guarda al lado como ao.db.antes-<fecha>.
param([string]$Backup = "", [string]$Config = "config\server.toml")
$here = $PSScriptRoot
$tool = @("$here\ao-ops.exe", "$here\..\dist\ao-ops.exe", "$here\..\target\release\ao-ops.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $tool) { throw "no encuentro ao-ops.exe: compila con scripts\build.ps1" }
if (-not $Backup) {
    & $tool backups $Config
    Write-Host "`nRestaurar: .\restore.ps1 -Backup latest   (o la ruta de una copia)"
    exit 0
}
& $tool restore $Backup $Config
exit $LASTEXITCODE
