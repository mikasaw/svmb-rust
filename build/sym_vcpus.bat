@echo off
rem offline symbol lookup: dump ALL svmb_rust symbols to a file for grep
set "CDB=C:\Program Files (x86)\Windows Kits\10\Debuggers\x64\cdb.exe"
set "SYS=C:\Users\www\AiCode\svmb-rust\x64\Debug\svmb_rust.sys"
set "SYM=srv*C:\symbols*https://msdl.microsoft.com/download/symbols;C:\Users\www\AiCode\svmb-rust\x64\Debug"
"%CDB%" -z "%SYS%" -y "%SYM%" -logo "C:\Users\www\AiCode\svmb-rust\build\logs\sym_all.txt" -c "x svmb_rust!*; q"
