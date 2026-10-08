@echo off
rem disassemble exit-path functions from both O0 and O2 C builds
set "CDB=C:\Program Files (x86)\Windows Kits\10\Debuggers\x64\cdb.exe"
set "SYMC=srv*C:\symbols*https://msdl.microsoft.com/download/symbols;C:\Users\www\AiCode\svmb\x64\Debug"
"%CDB%" -z "C:\Users\www\AiCode\svmb\build\logs\svmb_dbg_o0.sys" -y "%SYMC%" -logo "C:\Users\www\AiCode\svmb-rust\build\logs\o0_exit.txt" -c "uf svmb!SvmbVmExitEntry; uf svmb!svmb::HandleCpuidExit; uf svmb!svmb::EmulateCpuid; q"
"%CDB%" -z "C:\Users\www\AiCode\svmb\build\logs\svmb_dbg_o2.sys" -y "%SYMC%" -logo "C:\Users\www\AiCode\svmb-rust\build\logs\o2_exit.txt" -c "uf svmb!SvmbVmExitEntry; uf svmb!svmb::HandleCpuidExit; uf svmb!svmb::EmulateCpuid; q"
