@echo off
rem Numic AO - arranca el servidor local (con el admin de pruebas) y el cliente.
rem Doble clic. Cerrar la ventana del servidor lo apaga (guarda a todos).
cd /d "%~dp0.."
set AO_RECURSOS=X:\dev\ao20\Recursos
set AO_GM_SEED=Numic:admin
if not exist target\debug\server.exe (
  echo Compilando por primera vez, tarda unos minutos...
  cargo build -p server -p client || pause
)
start "Numic AO - servidor" cmd /k target\debug\server.exe
timeout /t 3 /nobreak >nul
start "" target\debug\client.exe
