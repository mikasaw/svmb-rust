@echo off
if exist "%~dp0vmenv.bat" call "%~dp0vmenv.bat"
rem run svmbctl-rs <subcommand> in the guest, capture its output to a
rem PER-SUBCOMMAND log (shared name caused later runs to overwrite evidence)
rem usage: vm_ctl_run.bat info | selftest | start | stop
set "SUB=%1"
if "%SUB%"=="" set "SUB=info"
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% runProgramInGuest "%VM_VMX%" "C:\Windows\System32\cmd.exe" "/c %GUEST_TEST_DIR%\svmbctl-rs.exe %* > %GUEST_DESKTOP%\svmb_rust_ctl_%SUB%.log 2>&1 < NUL"
exit /b %ERRORLEVEL%
