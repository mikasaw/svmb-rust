@echo off
if exist "%~dp0vmenv.bat" call "%~dp0vmenv.bat"
rem push the latest signed svmb-rust driver + ctl into the guest test dir
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% CopyFileFromHostToGuest "%VM_VMX%" "%REPO%\x64\Debug\svmb_rust.sys" "%GUEST_TEST_DIR%\svmb_rust.sys"
if errorlevel 1 exit /b 1
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% CopyFileFromHostToGuest "%VM_VMX%" "%REPO%\x64\Debug\svmbctl-rs.exe" "%GUEST_TEST_DIR%\svmbctl-rs.exe"
exit /b %ERRORLEVEL%
