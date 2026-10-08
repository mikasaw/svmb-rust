@echo off
rem r121 post-host-reboot unattended baseline test (SYSTEM task)
rem result appended to build\logs\rounds.txt via c_round.sh
set LOG=C:\Users\www\AiCode\svmb-rust\build\logs\after_reboot_host.log
echo === AFTER-REBOOT START %date% %time% === >> %LOG%
set VRUN=C:\Program Files (x86)\VMware\VMware Workstation\vmrun.exe
set VMX=C:\Users\www\Documents\Virtual Machines\Windows 10 x64\Windows 10 x64.vmx

rem let host services settle
timeout /t 60 /nobreak >nul

"%VRUN%" -T ws start "%VMX%" nogui >> %LOG% 2>&1

rem wait for Tools (up to 6 min)
set N=0
:waitloop
timeout /t 15 /nobreak >nul
"%VRUN%" -T ws -gu wwa -gp 123 runProgramInGuest "%VMX%" "C:\Windows\System32\cmd.exe" "/c echo A > C:\Users\wwa\Desktop\ab.txt < NUL" >nul 2>&1
if %errorlevel%==0 goto ready
set /a N+=1
if %N% lss 24 goto waitloop
echo TOOLS NEVER CAME UP >> %LOG%
goto end

:ready
echo TOOLS READY %time% >> %LOG%
"C:\Program Files\Git\bin\bash.exe" -lc "cd /c/Users/www/AiCode/svmb-rust/build && bash c_round.sh POSTREBOOT_C_BASE_R1 1" >> %LOG% 2>&1
"C:\Program Files\Git\bin\bash.exe" -lc "cd /c/Users/www/AiCode/svmb-rust/build && bash c_round.sh POSTREBOOT_C_BASE_R2 1" >> %LOG% 2>&1
echo === AFTER-REBOOT DONE %date% %time% === >> %LOG%

:end
