@echo off
rem dream-os launcher. Windows and dream-os (native on Windows, and the Linux side on WSL2) can run at the same time.
rem   dream-os status : check the runtime (GPU, DXBC to SPIR-V, Flash Attention)
rem   dream-os raid   : check RAID6/Z2 parity and self-heal
rem   dream-os dx     : check open-directx (DXBC to SPIR-V)
rem   dream-os fonts  : check the bundled Japanese font (IPAGothic, open source)
rem   dream-os linux  : open the Linux side (WSL2 Ubuntu); Windows stays usable
rem   dream-os both   : open the Linux side and show the Windows-side status at the same time
set D=%~dp0
if "%1"=="status" "%D%dream_os_status.exe" & goto :eof
if "%1"=="raid" "%D%raid6_bridge_benchmark.exe" & goto :eof
if "%1"=="dx" "%D%directx_bridge_benchmark.exe" & goto :eof
if "%1"=="fonts" "%D%font_check.exe" & goto :eof
if "%1"=="linux" wsl.exe & goto :eof
if "%1"=="both" (start "dream-os (Linux)" wsl.exe & "%D%dream_os_status.exe") & goto :eof
echo usage: dream-os status ^| raid ^| dx ^| fonts ^| linux ^| both
