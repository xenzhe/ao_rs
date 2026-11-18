@echo off
rem Builds X:\dev\ao20_full: the public ao-org/Recursos (server Dat, extras) with the
rem official Steam client's packed resources on top (all bodies, graphics, maps, sounds).
rem Needs: X:\dev\ao20\Recursos (git clone ao-org/Recursos), Argentum 20 installed from
rem Steam, Python, and openssl (Git for Windows ships one).
setlocal
set STEAM=C:\Program Files (x86)\Steam\steamapps\common\Argentum 20\Recursos
set PUB=X:\dev\ao20\Recursos
set OFF=X:\dev\ao20_official
set OUT=X:\dev\ao20_full
cd /d "%~dp0"
python -I unpack_official.py "%STEAM%" %OFF% || exit /b 1
if exist %OUT% rmdir /s /q %OUT%
robocopy %PUB% %OUT% /E /XD .git /NFL /NDL /NJH /NJS >nul
robocopy %OFF%\Graficos %OUT%\Graficos /NFL /NDL /NJH /NJS >nul
robocopy %OFF%\init %OUT%\init /NFL /NDL /NJH /NJS >nul
robocopy %OFF%\Mapas %OUT%\Mapas /NFL /NDL /NJH /NJS >nul
robocopy %OFF%\MiniMapas %OUT%\Minimapas /NFL /NDL /NJH /NJS >nul
robocopy %OFF%\OGG %OUT%\OGG *.ogg /NFL /NDL /NJH /NJS >nul
robocopy %OFF%\Sounds %OUT%\wav *.wav /NFL /NDL /NJH /NJS >nul
robocopy %OFF%\Midi %OUT%\midi *.mid /NFL /NDL /NJH /NJS >nul
echo Listo: %OUT%
