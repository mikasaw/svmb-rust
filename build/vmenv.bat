@echo off
rem svmb-rust local VM environment LOADER (committed; contains NO secrets).
rem Real values live in build/vmenv.local.bat (git-ignored). Copy
rem build/vmenv.local.template.bat to vmenv.local.bat and fill it in.
set "VMRUN=C:\Program Files (x86)\VMware\VMware Workstation\vmrun.exe"
set "VM_VMX=C:\Users\YOU\Documents\Virtual Machines\Win10\Win10.vmx"
set "VM_USER=CHANGEME"
set "VM_PASS=CHANGEME"
set "REPO=%~dp0.."
for %%i in ("%~dp0..") do set "REPO=%%~fi"
set "GUEST_TEST_DIR=C:\Users\CHANGEME\Desktop\driverTest\svmb-rust-test"
set "GUEST_DESKTOP=C:\Users\CHANGEME\Desktop"
set "HOST_LOGS=%REPO%\build\logs"
set "KDNET_KEY=net:port=50000,key=SET-ME-IN-vmenv.local.bat"
if exist "%~dp0vmenv.local.bat" call "%~dp0vmenv.local.bat"
