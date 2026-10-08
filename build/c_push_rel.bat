@echo off
rem push C Release (production) sys into the guest C test dir
if exist "%~dp0vmenv.bat" call "%~dp0vmenv.bat"
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% CopyFileFromHostToGuest "%VM_VMX%" "C:\Users\www\AiCode\svmb\x64\Release\svmb.sys" "C:\Users\wwa\Desktop\driverTest\svmb-test\svmb.sys"
exit /b %ERRORLEVEL%
