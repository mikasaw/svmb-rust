@echo off
if exist "%~dp0vmenv.bat" call "%~dp0vmenv.bat"
rem stop + delete old svmbrust service (1062 not-started is fine, delete result
rem is what matters); waits 3s so the kernel releases the .sys file lock
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% runProgramInGuest "%VM_VMX%" "C:\Windows\System32\cmd.exe" "/c sc stop svmbrust >> %GUEST_DESKTOP%\svmb_rust_steps.log 2>&1 & ping -n 3 127.0.0.1 >nul & sc delete svmbrust >> %GUEST_DESKTOP%\svmb_rust_steps.log 2>&1 < NUL"
exit /b %ERRORLEVEL%
