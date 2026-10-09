#!/bin/bash
# single round: reset -> deploy -> sc start (== fire via AutoStart) -> observe
# usage: round.sh <label>   (result appended to build/logs/rounds.txt)
# r149+: AutoStart is on by default - service start enters virtualization at
# DriverEntry, so there is NO idle window and NO ctl start step anymore.
LABEL="$1"
PROBES="${2:-10}"   # 30s observation probes (default 10 = ~5min; soak = 30)
B="$(cd "$(dirname "$0")" && pwd)"
VMRUN="/c/Program Files (x86)/VMware/VMware Workstation/vmrun.exe"
# credentials: from git-ignored vmenv.local.sh (see vmenv.local.sh.template)
[ -f "$(dirname "$0")/vmenv.local.sh" ] && . "$(dirname "$0")/vmenv.local.sh"
VM_GU="${VM_GU:-CHANGEME}"; VM_GP="${VM_GP:-CHANGEME}"
# guest desktop in Windows form (single backslashes) for vmrun path args
GUEST_DESK_WIN="${GUEST_DESK_WIN:-C:\\Users\\CHANGEME\\Desktop}"
VMX="${VMX_WIN:-C:\\Users\\CHANGEME\\Documents\\Virtual Machines\\VM\\VM.vmx}"
VLOG="${VM_LOG:-/c/CHANGEME/vmware.log}"
cd "$B" || exit 1

# reset with evidence via count-delta: rc alone has lied (silent reset failure
# left the box frozen; the loop burned 80min on alive timeouts). A boot storm
# writes 500-2000 log lines within 4s, so a tail-window grep can never see
# the reset line - compare the total count instead.
RESETS_BEFORE=$(grep -ac "VMAutomation_Reset" "$VLOG" 2>/dev/null); RESETS_BEFORE=${RESETS_BEFORE:-0}
"$VMRUN" -T ws reset "$VMX" hard >/dev/null 2>&1
sleep 4
RESETS_AFTER=$(grep -ac "VMAutomation_Reset" "$VLOG" 2>/dev/null); RESETS_AFTER=${RESETS_AFTER:-0}
if [ "$RESETS_AFTER" -le "$RESETS_BEFORE" ]; then
  echo "$LABEL RESET_NOT_EVIDENCED (retry manually, do not trust rc)" >> logs/rounds.txt
  exit 1
fi
for i in $(seq 1 16); do timeout 30 cmd.exe //c vm_alive.bat >/dev/null 2>&1 && break; sleep 15; done
cmd.exe //c vm_svc_clean.bat >/dev/null 2>&1
# truncate the guest steps log: it is append-only and never cleared, so a
# stale hash/error from an earlier round would poison this round's gate
timeout 60 "$VMRUN" -T ws -gu "$VM_GU" -gp "$VM_GP" runProgramInGuest "$VMX" "C:\\Windows\\System32\\cmd.exe" "/c type nul > $GUEST_DESK_WIN\\svmb_rust_steps.log < NUL" >/dev/null 2>&1
cmd.exe //c vm_push_rust.bat >/dev/null 2>&1 || { echo "$LABEL PUSH_FAIL" >> logs/rounds.txt; exit 1; }
cmd.exe //c vm_svc_create.bat >/dev/null 2>&1

# sc start == fire (AutoStart). Historical crash window is 2-25s from
# virtualization entry; first aliveness verdict lands after this sleep.
FIRE_TS=$(date +%H:%M:%S)
timeout 60 cmd.exe //c vm_svc_start.bat >/dev/null 2>&1
sleep 8

# gate: best-effort evidence pull. With AutoStart the service reports RUNNING
# only after DriverEntry returns - virtualization starts BEFORE that, and a
# fast death may eat the log entirely. That outcome is crash data, not a
# LOAD_FAIL. Locale-proof pattern: sc query's WIN32_EXIT_CODE field name is
# unlocalized (the guest is GBK; grepping localized words can never match).
timeout 90 "$VMRUN" -T ws -gu "$VM_GU" -gp "$VM_GP" CopyFileFromGuestToHost "$VMX" "$GUEST_DESK_WIN\\svmb_rust_steps.log" "$B\\logs\\svc_last.log" >/dev/null 2>&1
if grep -aqE "WIN32_EXIT_CODE +: +(193|577)" logs/svc_last.log 2>/dev/null; then
  echo "$LABEL LOAD_FAIL_193_or_577 (bad guest file or signature)" >> logs/rounds.txt
  exit 1
fi

# observation: a single slow-Tools blip must not flip a surviving round to
# CRASHED (Tools flapping is documented) - require two consecutive misses.
MISSES=0
for i in $(seq 1 "$PROBES"); do
  sleep 30
  if timeout 20 cmd.exe //c vm_alive.bat >/dev/null 2>&1; then
    MISSES=0
  else
    MISSES=$((MISSES+1))
    if [ "$MISSES" -ge 2 ]; then
      echo "$LABEL CRASHED_AT T+~$((8+i*30))s (fired $FIRE_TS)" >> logs/rounds.txt
      exit 0
    fi
  fi
done
# trailing-miss hardening: a single miss on the last probe must not be
# reported as SURVIVED - confirm once more before declaring survival
if [ "$MISSES" -gt 0 ]; then
  sleep 20
  timeout 20 cmd.exe //c vm_alive.bat >/dev/null 2>&1 || { echo "$LABEL CRASHED_AT T+~330s (trailing miss confirmed; fired $FIRE_TS)" >> logs/rounds.txt; exit 0; }
fi
echo "$LABEL SURVIVED T+$((8+PROBES*30))s (fired $FIRE_TS)" >> logs/rounds.txt
