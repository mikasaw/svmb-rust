@echo off
rem dump guest bcdedit dbgsettings + debug state to a log
if exist "%~dp0vmenv.bat" call "%~dp0vmenv.bat"
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% runProgramInGuest "%VM_VMX%" "C:\Windows\System32\cmd.exe" "/c bcdedit /dbgsettings > %GUEST_DESKTOP%\kd_dbgset.log 2>&1 & bcdedit /enum {current} >> %GUEST_DESKTOP%\kd_dbgset.log 2>&1 < NUL"
exit /b %ERRORLEVEL%
