@echo off
rem Numic AO con mapas, graficos, NPCs, objetos y hechizos de Tierras Sagradas.
rem Base de datos propia (tsao\data\ao.db). Cerrar la ventana del servidor lo apaga.
cd /d "%~dp0.."
set AO_RECURSOS=X:\dev\tsao_rec
set AO_GM_SEED=Numic:admin
if not exist X:\dev\tsao_rec\Mapas call tools\tsao\build.cmd
cargo build --profile play -p server -p client || pause
start "Numic AO (TSAO) - servidor" cmd /k target\play\server.exe tsao\server.toml
timeout /t 3 /nobreak >nul
start "" target\play\client.exe
