@echo off
rem Regenerates X:\dev\tsao_rec (Recursos for Numic AO) from the Tierras Sagradas checkout.
rem Needs: X:\dev\tsao (git clone Comunidad-Winter/Tierras-Sagradas-AO), Python with Pillow,
rem ffmpeg (Shotcut's) for the sounds.
cd /d "%~dp0"
python -I unpack_grh.py X:\dev\tsao\Cliente\Data\GRAFICOS\Grh.tsao X:\dev\tsao_rec\Graficos || exit /b 1
python -I colorkey.py X:\dev\tsao_rec\Graficos || exit /b 1
python -I convert.py X:\dev\tsao X:\dev\tsao_rec || exit /b 1
python -I sounds.py X:\dev\tsao X:\dev\tsao_rec || exit /b 1
python -I levels.py X:\dev\tsao\Servidor\Codigo\Modulo_UsUaRiOs.bas ..\..\tsao\data\levels.toml || exit /b 1
