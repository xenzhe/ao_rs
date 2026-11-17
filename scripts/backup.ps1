# Copia de seguridad de la base AHORA (segura con el servidor en marcha) y rotacion.
# Uso: .\scripts\backup.ps1 [config\server.toml]
# Busca ao-ops.exe junto al script, en .\dist o en target\release.
param([string]$Config = "config\server.toml")
$here = $PSScriptRoot
$tool = @("$here\ao-ops.exe", "$here\..\dist\ao-ops.exe", "$here\..\target\release\ao-ops.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $tool) { throw "no encuentro ao-ops.exe: compila con scripts\build.ps1" }
& $tool backup $Config
exit $LASTEXITCODE
