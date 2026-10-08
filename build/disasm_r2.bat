@echo off
rem disasm the rust helper at +0x4630 (handle_cpuid) and +0x32f0 (set_u64-ish)
set "CDB=C:\Program Files (x86)\Windows Kits\10\Debuggers\x64\cdb.exe"
set "SYMR=srv*C:\symbols*https://msdl.microsoft.com/download/symbols;C:\Users\www\AiCode\svmb-rust\x64\Debug"
"%CDB%" -z "C:\Users\www\AiCode\svmb-rust\x64\Debug\svmb_rust.sys" -y "%SYMR%" -logo "C:\Users\www\AiCode\svmb-rust\build\logs\disasm_r2.txt" -c "uf svmb_rust+0x4630; uf svmb_rust+0x7cf0; q"
