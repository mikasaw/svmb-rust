#!/bin/bash
# single C-side round: reset -> deploy C svc (HideCpuidBits=$1) -> observe 4min
# usage: c_round.sh <label> <hidebits>   (result appended to logs/rounds.txt)
LABEL="$1"; HIDE="$2"
B=/c/Users/www/AiCode/svmb-rust/build
VMRUN="/c/Program Files (x86)/VMware/VMware Workstation/vmrun.exe"
cd "$B" || exit 1

"$VMRUN" -T ws reset "C:\Users\www\Documents\Virtual Machines\Windows 10 x64\Windows 10 x64.vmx" hard >/dev/null 2>&1
for i in $(seq 1 16); do cmd.exe //c vm_alive.bat >/dev/null 2>&1 && break; sleep 15; done
cmd.exe //c c_round.bat "$HIDE" >/dev/null 2>&1
"$VMRUN" -T ws -gu wwa -gp 123 CopyFileFromGuestToHost "C:\Users\www\Documents\Virtual Machines\Windows 10 x64\Windows 10 x64.vmx" "C:\Users\wwa\Desktop\c_round.log" "$B\logs\c_round_last.log" >/dev/null 2>&1
if grep -q "193" logs/c_round_last.log 2>/dev/null || grep -q " 577" logs/c_round_last.log 2>/dev/null; then
  echo "$LABEL LOAD_FAIL_193_or_577 (bad guest file or signature)" >> logs/rounds.txt
  exit 1
fi
sleep 8
FIRE_TS=$(date +%H:%M:%S)
for i in $(seq 1 8); do
  sleep 30
  cmd.exe //c c_alive.bat >/dev/null 2>&1
  OK=$?
  "$VMRUN" -T ws -gu wwa -gp 123 CopyFileFromGuestToHost "C:\Users\www\Documents\Virtual Machines\Windows 10 x64\Windows 10 x64.vmx" "C:\Users\wwa\Desktop\svmb_ctl_info.log" "$B\\logs\\svmb_ctl_info.log" >/dev/null 2>&1
  LINE=$(grep -c "hv state" logs/svmb_ctl_info.log 2>/dev/null)
  if [ "$LINE" != "1" ] && [ $OK -ne 0 ]; then
    echo "$LABEL CRASHED_AT T+~$((i*30))s (fired $FIRE_TS)" >> logs/rounds.txt
    "$VMRUN" -T ws -gu wwa -gp 123 runProgramInGuest "C:\Users\www\Documents\Virtual Machines\Windows 10 x64\Windows 10 x64.vmx" "C:\Windows\System32\cmd.exe" "/c sc delete svmb > nul 2>&1 < NUL" >/dev/null 2>&1
    exit 0
  fi
done
echo "$LABEL SURVIVED T+240s (fired $FIRE_TS)" >> logs/rounds.txt
"$VMRUN" -T ws -gu wwa -gp 123 runProgramInGuest "C:\Users\www\Documents\Virtual Machines\Windows 10 x64\Windows 10 x64.vmx" "C:\Windows\System32\cmd.exe" "/c C:\Users\wwa\Desktop\driverTest\svmb-test\svmbctl.exe stop > nul 2>&1 & sc stop svmb > nul 2>&1 & sc delete svmb > nul 2>&1 < NUL" >/dev/null 2>&1
