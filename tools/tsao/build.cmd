@echo off
rem Regenerates the Numic AO resources (default X:\dev\tsao_rec, or the folder given as the
rem first argument) from the Tierras Sagradas checkout.
rem Needs: X:\dev\tsao (git clone Comunidad-Winter/Tierras-Sagradas-AO), Python with Pillow,
rem ffmpeg (Shotcut's) for the sounds.
setlocal
set OUT=%~1
if "%OUT%"=="" set OUT=X:\dev\tsao_rec
cd /d "%~dp0"
python -I unpack_grh.py X:\dev\tsao\Cliente\Data\GRAFICOS\Grh.tsao %OUT%\Graficos || exit /b 1
python -I colorkey.py %OUT%\Graficos || exit /b 1
python -I convert.py X:\dev\tsao %OUT% || exit /b 1
python -I wrapfix.py %OUT% || exit /b 1
python -I sounds.py X:\dev\tsao %OUT% || exit /b 1
python -I progress.py X:\dev\tsao %OUT% || exit /b 1
python -I levels.py X:\dev\tsao\Servidor\Codigo\Modulo_UsUaRiOs.bas ..\..\tsao\data\levels.toml || exit /b 1
