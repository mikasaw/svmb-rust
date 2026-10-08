@echo off
rem one-shot kd: resolve svmbrust base via !drvobj (brief halt, qd detaches)
set "KD=C:\Program Files (x86)\Windows Kits\10\Debuggers\x64\kd.exe"
set "KEY=net:port=50000,key=2sazdj33vv3ym.14h4ybb3lujun.aoyihkwsbg1b.2xxysisdr5i28"
set "SYM=srv*C:\symbols*https://msdl.microsoft.com/download/symbols;C:\Users\www\AiCode\svmb-rust\x64\Debug"
"%KD%" -k "%KEY%" -y "%SYM%" -logo "C:\Users\www\AiCode\svmb-rust\build\logs\kd_out.txt" -c "!drvobj svmbrust 2; qd"
