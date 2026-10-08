@echo off
rem disasm the function containing the cpuid instruction at +0xd9a7
set "CDB=C:\Program Files (x86)\Windows Kits\10\Debuggers\x64\cdb.exe"
set "SYMR=srv*C:\symbols*https://msdl.microsoft.com/download/symbols;C:\Users\www\AiCode\svmb-rust\x64\Debug"
"%CDB%" -z "C:\Users\www\AiCode\svmb-rust\x64\Debug\svmb_rust.sys" -y "%SYMR%" -logo "C:\Users\www\AiCode\svmb-rust\build\logs\disasm_r3.txt" -c "uf svmb_rust+0xd9a7; q"
