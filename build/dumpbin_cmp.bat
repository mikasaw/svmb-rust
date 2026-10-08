@echo off
rem compare link directives + headers of O0 vs O2 C builds
call "C:\Program Files\Microsoft Visual Studio\18\Insiders\VC\Auxiliary\Build\vcvars64.bat" >nul
dumpbin /directives "C:\Users\www\AiCode\svmb\build\logs\svmb_dbg_o0.sys" > "C:\Users\www\AiCode\svmb-rust\build\logs\o0_drectve.txt" 2>&1
dumpbin /directives "C:\Users\www\AiCode\svmb\build\logs\svmb_dbg_o2.sys" > "C:\Users\www\AiCode\svmb-rust\build\logs\o2_drectve.txt" 2>&1
dumpbin /headers "C:\Users\www\AiCode\svmb\build\logs\svmb_dbg_o0.sys" > "C:\Users\www\AiCode\svmb-rust\build\logs\o0_hdr.txt" 2>&1
dumpbin /headers "C:\Users\www\AiCode\svmb\build\logs\svmb_dbg_o2.sys" > "C:\Users\www\AiCode\svmb-rust\build\logs\o2_hdr.txt" 2>&1
echo DONE
