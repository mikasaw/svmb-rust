@echo off
rem svmb-rust local VM environment (git-ignored; never commit credentials)
set "VMRUN=C:\Program Files (x86)\VMware\VMware Workstation\vmrun.exe"
set "VM_VMX=C:\Users\www\Documents\Virtual Machines\Windows 10 x64\Windows 10 x64.vmx"
set "VM_USER=wwa"
set "VM_PASS=123"
set "REPO=%~dp0.."
rem normalize REPO: vmrun file-copy rejects destinations containing ".."
for %%i in ("%~dp0..") do set "REPO=%%~fi"
set "GUEST_TEST_DIR=C:\Users\wwa\Desktop\driverTest\svmb-rust-test"
set "GUEST_DESKTOP=C:\Users\wwa\Desktop"
set "HOST_LOGS=%REPO%\build\logs"
