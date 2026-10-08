@echo off
rem pull recent critical service/bugcheck events + service state
if exist "%~dp0vmenv.bat" call "%~dp0vmenv.bat"
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% runProgramInGuest "%VM_VMX%" "C:\Windows\System32\cmd.exe" "/c wevtutil qe System /q:\"*[System[(EventID=1001 or EventID=41 or EventID=7024 or EventID=7036)]]\" /c:8 /rd:true /f:text > %GUEST_DESKTOP%\kd_evt.log 2>&1 & sc query svmbrust >> %GUEST_DESKTOP%\kd_evt.log 2>&1 & net statistics srv | findstr /C:\"since\" >> %GUEST_DESKTOP%\kd_evt.log 2>&1 < NUL"
exit /b %ERRORLEVEL%
