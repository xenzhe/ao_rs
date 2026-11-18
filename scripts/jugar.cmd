@echo off
rem Numic AO - arranca el servidor local (con el admin de pruebas) y el cliente.
rem Doble clic. Cerrar la ventana del servidor lo apaga (guarda a todos).
rem Usa el perfil "play" (optimizado, compila rapido): mucho mas fps que debug.
cd /d "%~dp0.."
rem Recursos completos: repo publico + cliente oficial de Steam (tools\ao20\build_full.cmd).
set AO_RECURSOS=X:\dev\ao20_full
set AO_GM_SEED=Numic:admin
cargo build --profile play -p server -p client || pause
start "Numic AO - servidor" cmd /k target\play\server.exe
timeout /t 3 /nobreak >nul
start "" target\play\client.exe
