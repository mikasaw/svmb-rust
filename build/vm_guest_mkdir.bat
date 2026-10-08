@echo off
if exist "%~dp0vmenv.bat" call "%~dp0vmenv.bat"
rem create the guest test dir if missing (idempotent)
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% runProgramInGuest "%VM_VMX%" "C:\Windows\System32\cmd.exe" "/c if not exist %GUEST_TEST_DIR% mkdir %GUEST_TEST_DIR% < NUL"
exit /b %ERRORLEVEL%
