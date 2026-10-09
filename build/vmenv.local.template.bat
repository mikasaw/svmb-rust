@echo off
rem COPY to build/vmenv.local.bat (git-ignored) and fill in your values.
rem Only override what differs from the placeholders in vmenv.bat.
set "VM_VMX=C:\Users\you\Documents\Virtual Machines\Windows 10 x64\Windows 10 x64.vmx"
set "VM_USER=your-windows-username"
set "VM_PASS=your-windows-password"
set "GUEST_TEST_DIR=C:\Users\your-windows-username\Desktop\driverTest\svmb-rust-test"
set "GUEST_DESKTOP=C:\Users\your-windows-username\Desktop"
set "KDNET_KEY=net:port=50000,key=your-kdnet-key-from-bcdedit"
