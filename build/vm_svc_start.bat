@echo off
if exist "%~dp0vmenv.bat" call "%~dp0vmenv.bat"
rem start svmbrust (fires DriverEntry) then record the service state
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% runProgramInGuest "%VM_VMX%" "C:\Windows\System32\cmd.exe" "/c sc start svmbrust >> %GUEST_DESKTOP%\svmb_rust_steps.log 2>&1 & sc query svmbrust >> %GUEST_DESKTOP%\svmb_rust_steps.log 2>&1 & certutil -hashfile %GUEST_TEST_DIR%\svmb_rust.sys SHA256 >> %GUEST_DESKTOP%\svmb_rust_steps.log 2>&1 < NUL"
exit /b %ERRORLEVEL%
