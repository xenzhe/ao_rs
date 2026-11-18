@echo off
rem Regenerates the Recursos for Numic AO from the Tierras Sagradas checkout.
rem Usage: build.cmd [output folder]   (default X:\dev\tsao_rec)
rem Needs: X:\dev\tsao (git clone Comunidad-Winter/Tierras-Sagradas-AO), Python with Pillow,
rem ffmpeg (Shotcut's) for the sounds.
cd /d "%~dp0"
set OUT=%~1
if "%OUT%"=="" set OUT=X:\dev\tsao_rec
python -I unpack_grh.py X:\dev\tsao\Cliente\Data\GRAFICOS\Grh.tsao %OUT%\Graficos || exit /b 1
python -I colorkey.py %OUT%\Graficos || exit /b 1
python -I convert.py X:\dev\tsao %OUT% || exit /b 1
python -I wrapfix.py %OUT% || exit /b 1
python -I sounds.py X:\dev\tsao %OUT% || exit /b 1
python -I levels.py X:\dev\tsao\Servidor\Codigo\Modulo_UsUaRiOs.bas ..\..\tsao\data\levels.toml || exit /b 1
