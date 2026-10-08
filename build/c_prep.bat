@echo off
rem check C-version test dir + service state in guest
if exist "%~dp0vmenv.bat" call "%~dp0vmenv.bat"
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% runProgramInGuest "%VM_VMX%" "C:\Windows\System32\cmd.exe" "/c dir C:\Users\wwa\Desktop\driverTest\svmb-test > C:\Users\wwa\Desktop\c_prep.log 2>&1 & sc query svmb >> C:\Users\wwa\Desktop\c_prep.log 2>&1 < NUL"
exit /b %ERRORLEVEL%
