@echo off
if exist "%~dp0vmenv.bat" call "%~dp0vmenv.bat"
rem guest-side SHA256 of the pushed binaries into a FRESH step log.
rem one vmrun exec per hash (compound && chains proved fragile right after push)
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% fileExistsInGuest "%VM_VMX%" "%GUEST_TEST_DIR%\svmb_rust.sys"
echo ---PROBE_SYS=%ERRORLEVEL%---
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% fileExistsInGuest "%VM_VMX%" "%GUEST_TEST_DIR%\svmbctl-rs.exe"
echo ---PROBE_EXE=%ERRORLEVEL%---
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% runProgramInGuest "%VM_VMX%" "C:\Windows\System32\cmd.exe" "/c certutil -hashfile %GUEST_TEST_DIR%\svmb_rust.sys SHA256 > %GUEST_DESKTOP%\svmb_rust_steps.log 2>&1 < NUL"
echo ---HASH_SYS=%ERRORLEVEL%---
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% runProgramInGuest "%VM_VMX%" "C:\Windows\System32\cmd.exe" "/c certutil -hashfile %GUEST_TEST_DIR%\svmbctl-rs.exe SHA256 >> %GUEST_DESKTOP%\svmb_rust_steps.log 2>&1 < NUL"
echo ---HASH_EXE=%ERRORLEVEL%---
exit /b 0
