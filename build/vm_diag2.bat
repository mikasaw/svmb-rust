@echo off
if exist "%~dp0vmenv.bat" call "%~dp0vmenv.bat"
rem re-run the hash check alone and probe the guest step log afterwards
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% runProgramInGuest "%VM_VMX%" "C:\Windows\System32\cmd.exe" "/c certutil -hashfile %GUEST_TEST_DIR%\svmb_rust.sys SHA256 > %GUEST_DESKTOP%\svmb_rust_steps.log 2>&1 < NUL"
echo ---RC_HASH=%ERRORLEVEL%---
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% fileExistsInGuest "%VM_VMX%" "%GUEST_DESKTOP%\svmb_rust_steps.log"
echo ---RC_LOG=%ERRORLEVEL%---
exit /b 0
