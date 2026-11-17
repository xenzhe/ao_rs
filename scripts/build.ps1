# Compila el servidor en release y arma la carpeta de despliegue (por defecto .\dist).
# Uso: .\scripts\build.ps1 [-Out dist]
# No toca config\ ni data\ si ya existen en el destino (conserva tu configuracion y tu base).
param([string]$Out = "dist")
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

cargo build --release -p server
if ($LASTEXITCODE -ne 0) { throw "cargo build fallo" }

New-Item -ItemType Directory -Force $Out | Out-Null
Copy-Item target\release\server.exe, target\release\ao-ops.exe $Out -Force
foreach ($d in "config", "data") {
    $dest = Join-Path $Out $d
    New-Item -ItemType Directory -Force $dest | Out-Null
    foreach ($f in Get-ChildItem $d -File | Where-Object { $_.Extension -in ".toml", ".ini" }) {
        $target = Join-Path $dest $f.Name
        if (-not (Test-Path $target)) { Copy-Item $f.FullName $target }
    }
}
Copy-Item scripts\*.ps1 $Out -Force
Write-Host "Listo: $Out (server.exe, ao-ops.exe, config\, data\). Arranca con: cd $Out; .\server.exe"
