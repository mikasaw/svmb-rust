@echo off
rem guest sends a UDP probe to host VMnet8 IP port 50000
if exist "%~dp0vmenv.bat" call "%~dp0vmenv.bat"
"%VMRUN%" -T ws -gu %VM_USER% -gp %VM_PASS% runProgramInGuest "%VM_VMX%" "C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe" "-NoProfile -Command $c=New-Object System.Net.Sockets.UdpClient; $b=[Text.Encoding]::ASCII.GetBytes('KDPING'); $c.Send($b,$b.Length,'%KD_HOST_IP%',50000); $c.Close() < NUL"
exit /b %ERRORLEVEL%
