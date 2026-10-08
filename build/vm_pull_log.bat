@echo off
if exist "%~dp0vmenv.bat" call "%~dp0vmenv.bat"
rem pull the guest step + per-subcommand ctl logs back to the host (best-effort)
if not exist "%HOST_LOGS%" mkdir "%HOST_LOGS%"
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% CopyFileFromGuestToHost "%VM_VMX%" "%GUEST_DESKTOP%\svmb_rust_steps.log" "%HOST_LOGS%\svmb_rust_steps.log"
echo ---PULL_STEPS=%ERRORLEVEL%---
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% CopyFileFromGuestToHost "%VM_VMX%" "%GUEST_DESKTOP%\svmb_rust_ctl_start.log" "%HOST_LOGS%\svmb_rust_ctl_start.log"
echo ---PULL_START=%ERRORLEVEL%---
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% CopyFileFromGuestToHost "%VM_VMX%" "%GUEST_DESKTOP%\svmb_rust_ctl_info.log" "%HOST_LOGS%\svmb_rust_ctl_info.log"
echo ---PULL_INFO=%ERRORLEVEL%---
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% CopyFileFromGuestToHost "%VM_VMX%" "%GUEST_DESKTOP%\svmb_rust_ctl.log" "%HOST_LOGS%\svmb_rust_ctl.log"
echo ---PULL_CTL=%ERRORLEVEL%---
exit /b 0
