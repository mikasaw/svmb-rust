@echo off
rem C-version flow: %1 = create | start | fire
if exist "%~dp0vmenv.bat" call "%~dp0vmenv.bat"
set "GT=C:\Users\wwa\Desktop\driverTest\svmb-test"
if "%1"=="create" (
  "%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% runProgramInGuest "%VM_VMX%" "C:\Windows\System32\cmd.exe" "/c sc create svmb type= kernel binPath= %GT%\svmb.sys > C:\Users\wwa\Desktop\c_flow.log 2>&1 & sc start svmb >> C:\Users\wwa\Desktop\c_flow.log 2>&1 < NUL"
) else if "%1"=="fire" (
  "%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% runProgramInGuest "%VM_VMX%" "C:\Windows\System32\cmd.exe" "/c %GT%\svmbctl.exe start > C:\Users\wwa\Desktop\svmb_ctl_start.log 2>&1 < NUL"
) else (
  echo usage: c_flow.bat create^|fire
)
exit /b %ERRORLEVEL%
