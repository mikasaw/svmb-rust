@echo off
rem watch kd.exe cmdline + UDP50000 binding for 60s, append to log
set "LOG=C:\Users\www\AiCode\svmb-rust\build\logs\kd_watch.txt"
echo ===WATCH START %DATE% %TIME%=== >> "%LOG%"
for /L %%i in (1,1,60) do (
  wmic process where "name='kd.exe'" get ProcessId,CommandLine /format:list 2>nul | findstr /r "CommandLine ProcessId" >> "%LOG%"
  netstat -ano -p UDP 2>nul | findstr ":50000" >> "%LOG%"
  ping -n 2 127.0.0.1 >nul
)
echo ===WATCH END=== >> "%LOG%"
