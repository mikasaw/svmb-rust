@echo off
if exist "%~dp0vmenv.bat" call "%~dp0vmenv.bat"
rem diagnostic: list guest test dir + probe both pushed files
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% directoryListingInGuest "%VM_VMX%" "%GUEST_TEST_DIR%"
echo ---RC_LIST=%ERRORLEVEL%---
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% fileExistsInGuest "%VM_VMX%" "%GUEST_TEST_DIR%\svmb_rust.sys"
echo ---RC_SYS=%ERRORLEVEL%---
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% fileExistsInGuest "%VM_VMX%" "%GUEST_TEST_DIR%\svmbctl-rs.exe"
echo ---RC_EXE=%ERRORLEVEL%---
exit /b 0
