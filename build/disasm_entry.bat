@echo off
rem disasm C SvmbVmExitEntry full body for side-by-side with Rust disasm_r.txt
set "CDB=C:\Program Files (x86)\Windows Kits\10\Debuggers\x64\cdb.exe"
set "SYMC=srv*C:\symbols*https://msdl.microsoft.com/download/symbols;C:\Users\www\AiCode\svmb\x64\Debug"
"%CDB%" -z "C:\Users\www\AiCode\svmb\x64\Debug\svmb.sys" -y "%SYMC%" -logo "C:\Users\www\AiCode\svmb-rust\build\logs\disasm_entry_c.txt" -c "uf svmb!SvmbVmExitEntry; q"
