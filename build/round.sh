#!/bin/bash
# single A/B round: reset -> deploy -> idle 150 -> fire -> observe 4min
# usage: round.sh <label>   (result appended to build/logs/rounds.txt)
LABEL="$1"
B=/c/Users/www/AiCode/svmb-rust/build
VMRUN="/c/Program Files (x86)/VMware/VMware Workstation/vmrun.exe"
VMX="C:\\Users\\www\\Documents\\Virtual Machines\\Windows 10 x64\\Windows 10 x64.vmx"
cd "$B" || exit 1

"$VMRUN" -T ws reset "C:\Users\www\Documents\Virtual Machines\Windows 10 x64\Windows 10 x64.vmx" hard >/dev/null 2>&1
for i in $(seq 1 16); do cmd.exe //c vm_alive.bat >/dev/null 2>&1 && break; sleep 15; done
cmd.exe //c vm_svc_clean.bat >/dev/null 2>&1
cmd.exe //c vm_push_rust.bat >/dev/null 2>&1 || { echo "$LABEL PUSH_FAIL" >> logs/rounds.txt; exit 1; }
cmd.exe //c vm_svc_create.bat >/dev/null 2>&1
cmd.exe //c vm_svc_start.bat >/dev/null 2>&1 || { echo "$LABEL SVC_FAIL" >> logs/rounds.txt; exit 1; }
"$VMRUN" -T ws -gu wwa -gp 123 CopyFileFromGuestToHost "C:\Users\www\Documents\Virtual Machines\Windows 10 x64\Windows 10 x64.vmx" "C:\Users\wwa\Desktop\svmb_rust_steps.log" "$B\logs\svc_last.log" >/dev/null 2>&1
if ! grep -q "RUNNING" logs/svc_last.log 2>/dev/null; then
  echo "$LABEL LOAD_FAIL_svc_start (see logs/svc_last.log)" >> logs/rounds.txt
  exit 1
fi
sleep 150
FIRE_TS=$(date +%H:%M:%S)
cmd.exe //c "vm_ctl_run.bat start" >/dev/null 2>&1
for i in $(seq 1 8); do
  sleep 30
  cmd.exe //c "vm_ctl_run.bat info" >/dev/null 2>&1
  "$VMRUN" -T ws -gu wwa -gp 123 CopyFileFromGuestToHost "C:\Users\www\Documents\Virtual Machines\Windows 10 x64\Windows 10 x64.vmx" "C:\Users\wwa\Desktop\svmb_rust_ctl_info.log" "$B\\logs\\svmb_rust_ctl_info.log" >/dev/null 2>&1
  LINE=$(grep -c "hv_state" logs/svmb_rust_ctl_info.log 2>/dev/null)
  CTR=$(grep exit_counter logs/svmb_rust_ctl_info.log 2>/dev/null | grep -oE "[0-9]+" | head -1)
  if [ "$LINE" != "1" ]; then
    echo "$LABEL CRASHED_AT T+~$((i*30))s (fired $FIRE_TS)" >> logs/rounds.txt
    exit 0
  fi
done
echo "$Label_SURVIVED" >> /dev/null
echo "$LABEL SURVIVED T+240s ctr=$CTR (fired $FIRE_TS)" >> logs/rounds.txt
