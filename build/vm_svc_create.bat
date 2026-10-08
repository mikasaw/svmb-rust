@echo off
if exist "%~dp0vmenv.bat" call "%~dp0vmenv.bat"
rem create the svmbrust kernel service pointing at the freshly deployed driver
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% runProgramInGuest "%VM_VMX%" "C:\Windows\System32\cmd.exe" "/c sc create svmbrust type= kernel binPath= %GUEST_TEST_DIR%\svmb_rust.sys >> %GUEST_DESKTOP%\svmb_rust_steps.log 2>&1 < NUL"
exit /b %ERRORLEVEL%
