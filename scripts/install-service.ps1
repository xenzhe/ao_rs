# Instala el servidor como servicio de Windows con NSSM (https://nssm.cc), como Administrador.
# Uso: .\install-service.ps1 [-Dir C:\ao\dist] [-Name AOServer] [-Recursos X:\dev\ao20\Recursos]
# Al parar el servicio NSSM manda Ctrl+C: el servidor guarda a todos, hace la copia y sale
# (por eso NO se usa un servicio "crudo" ni schtasks /end, que matan el proceso sin guardar).
param(
    [string]$Dir = (Get-Location).Path,
    [string]$Name = "AOServer",
    [string]$Recursos = ""
)
$ErrorActionPreference = "Stop"
if (-not (Get-Command nssm -ErrorAction SilentlyContinue)) { throw "falta nssm.exe en el PATH (winget install nssm)" }
$exe = Join-Path $Dir "server.exe"
if (-not (Test-Path $exe)) { throw "no existe $exe (compila con scripts\build.ps1)" }

nssm install $Name $exe
nssm set $Name AppDirectory $Dir
nssm set $Name Start SERVICE_AUTO_START
nssm set $Name AppStopMethodSkip 0
nssm set $Name AppStopMethodConsole 60000
nssm set $Name AppStopMethodWindow 0
nssm set $Name AppStopMethodThreads 0
nssm set $Name AppExit Default Restart
nssm set $Name AppRestartDelay 5000
nssm set $Name AppStdout (Join-Path $Dir "server.log")
nssm set $Name AppStderr (Join-Path $Dir "server.log")
nssm set $Name AppRotateFiles 1
nssm set $Name AppRotateBytes 10485760
$envs = "RUST_LOG=info"
if ($Recursos) { $envs += "`nAO_RECURSOS=$Recursos" }
nssm set $Name AppEnvironmentExtra $envs
Write-Host "Servicio $Name instalado. Arrancar: nssm start $Name   Parar (guarda y copia): nssm stop $Name"
