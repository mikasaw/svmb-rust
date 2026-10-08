@echo off
rem C-side reverse bisect round: create svc with HideCpuidBits knob, start
rem usage: c_round.bat <hidebits 0|1>
if exist "%~dp0vmenv.bat" call "%~dp0vmenv.bat"
set "GT=C:\Users\wwa\Desktop\driverTest\svmb-test"
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% runProgramInGuest "%VM_VMX%" "C:\Windows\System32\cmd.exe" "/c sc create svmb type= kernel binPath= %GT%\svmb.sys > nul 2>&1 & reg add HKLM\SYSTEM\CurrentControlSet\Services\svmb\Parameters /v HideCpuidBits /t REG_DWORD /d %1 /f > nul 2>&1 & sc start svmb > C:\Users\wwa\Desktop\c_round.log 2>&1
certutil -hashfile C:\Users\wwa\Desktop\driverTest\svmb-test\svmb.sys SHA256 >> C:\Users\wwa\Desktop\c_round.log 2>&1 < NUL"
exit /b %ERRORLEVEL%
