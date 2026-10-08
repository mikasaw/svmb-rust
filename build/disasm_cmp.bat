@echo off
rem offline disasm of the CPUID tamper path in both drivers
set "CDB=C:\Program Files (x86)\Windows Kits\10\Debuggers\x64\cdb.exe"
set "SYMR=srv*C:\symbols*https://msdl.microsoft.com/download/symbols;C:\Users\www\AiCode\svmb-rust\x64\Debug"
set "SYMC=srv*C:\symbols*https://msdl.microsoft.com/download/symbols;C:\Users\www\AiCode\svmb\x64\Debug"
"%CDB%" -z "C:\Users\www\AiCode\svmb\x64\Debug\svmb.sys" -y "%SYMC%" -logo "C:\Users\www\AiCode\svmb-rust\build\logs\disasm_c.txt" -c "uf svmb!svmb::HandleCpuidExit; q"
"%CDB%" -z "C:\Users\www\AiCode\svmb-rust\x64\Debug\svmb_rust.sys" -y "%SYMR%" -logo "C:\Users\www\AiCode\svmb-rust\build\logs\disasm_r.txt" -c "uf svmb_rust!svmb_vm_exit_entry; q"
