@echo off
rem aliveness probe via Tools channel (admin-context exec, non-interactive)
if exist "%~dp0vmenv.bat" call "%~dp0vmenv.bat"
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% runProgramInGuest "%VM_VMX%" "C:\Windows\System32\cmd.exe" "/c echo ALIVE > %GUEST_DESKTOP%\vm_alive.txt < NUL"
exit /b %ERRORLEVEL%
