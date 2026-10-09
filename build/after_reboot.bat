@echo off
if exist "%~dp0vmenv.bat" call "%~dp0vmenv.bat"
rem r121 post-host-reboot unattended baseline test (SYSTEM task)
rem result appended to build\logs\rounds.txt via c_round.sh
set LOG=%REPO%\build\logs\after_reboot_host.log
echo === AFTER-REBOOT START %date% %time% === >> %LOG%
set VRUN=C:\Program Files (x86)\VMware\VMware Workstation\vmrun.exe
set VMX=%VM_VMX%

rem let host services settle
timeout /t 60 /nobreak >nul

"%VRUN%" -T ws start "%VMX%" nogui >> %LOG% 2>&1

rem wait for Tools (up to 6 min)
set N=0
:waitloop
timeout /t 15 /nobreak >nul
"%VRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% runProgramInGuest "%VMX%" "C:\Windows\System32\cmd.exe" "/c echo A > %GUEST_DESKTOP%\ab.txt < NUL" >nul 2>&1
if %errorlevel%==0 goto ready
set /a N+=1
if %N% lss 24 goto waitloop
echo TOOLS NEVER CAME UP >> %LOG%
goto end

:ready
echo TOOLS READY %time% >> %LOG%
"C:\Program Files\Git\bin\bash.exe" -lc "cd $REPO_UNIX/build && bash round.sh POSTREBOOT_R1" >> %LOG% 2>&1
"C:\Program Files\Git\bin\bash.exe" -lc "cd $REPO_UNIX/build && bash round.sh POSTREBOOT_R2" >> %LOG% 2>&1
echo === AFTER-REBOOT DONE %date% %time% === >> %LOG%

:end
