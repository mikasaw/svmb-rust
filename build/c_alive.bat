@echo off
rem C-version (svmb) liveness via its ctl: run svmbctl info in guest
if exist "%~dp0vmenv.bat" call "%~dp0vmenv.bat"
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% runProgramInGuest "%VM_VMX%" "C:\Windows\System32\cmd.exe" "/c C:\Users\wwa\Desktop\driverTest\svmb-test\svmbctl.exe info > C:\Users\wwa\Desktop\svmb_ctl_info.log 2>&1 < NUL"
exit /b %ERRORLEVEL%
