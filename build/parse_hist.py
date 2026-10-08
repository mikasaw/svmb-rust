import json
import re
from collections import defaultdict

NAMES = {
    0x40: "EXCP0 #DE", 0x41: "EXCP1 #DB", 0x43: "EXCP3 #BP", 0x44: "EXCP4 #OF",
    0x45: "EXCP5 #BR", 0x46: "EXCP6 #UD", 0x47: "EXCP7 #NM", 0x48: "EXCP8 #DF",
    0x4A: "EXCP10 #TS", 0x4B: "EXCP11 #NP", 0x4C: "EXCP12 #SS", 0x4D: "EXCP13 #GP",
    0x4E: "EXCP14 #PF", 0x52: "EXCP18 #MC", 0x55: "EXCP21 #CP",
    0x60: "INTR", 0x61: "NMI", 0x62: "SMI", 0x63: "INIT", 0x64: "VINTR",
    0x72: "CPUID", 0x78: "HLT", 0x7A: "INVLPGA", 0x7B: "IOIO", 0x7C: "MSR",
    0x7F: "SHUTDOWN", 0x80: "VMRUN", 0x81: "VMMCALL", 0x82: "VMLOAD",
    0x83: "VMSAVE", 0x84: "STGI", 0x85: "CLGI", 0x87: "RDTSCP", 0x89: "WBINVD",
    0x8A: "MONITOR", 0x8B: "MWAIT", 0x8D: "XSETBV", 0x8F: "EFER_WRITE",
}
NPF_SLOT, OTHER_SLOT = 0x100, 0x101

d = json.load(open(r"C:\Users\www\AiCode\svmb-rust\build\logs\kd_forensic.json",
                   encoding="utf-8"))
total = defaultdict(int)
per_core = {}
for ci in range(6):
    txt = d["dumps"][f"core{ci}"]["hist_dq"]
    qwords = []
    for ln in txt.splitlines():
        m = re.match(r"^[0-9a-fA-F]{8,16}`[0-9a-fA-F]{8,16}\s+(.*)$", ln.strip())
        if not m:
            continue
        vals = m.group(1).split()
        for v in vals:
            if re.fullmatch(r"[0-9a-fA-F]{8,16}`[0-9a-fA-F]{8,16}", v):
                qwords.append(int(v.replace("`", ""), 16))
    per_core[ci] = qwords
    for idx, val in enumerate(qwords):
        if val:
            total[idx] += val

grand = sum(total.values())
print(f"total exits (sum of hist, all 6 cores): {grand}")
for ci in range(6):
    print(f"  core{ci}: {sum(per_core[ci]):>6}")
print("\nnonzero slots (slot=count, code>0x100 except NPF aggregated):")
for idx in sorted(total, key=lambda i: -total[i]):
    if idx == NPF_SLOT:
        name = "NPF(0x400) [slot 0x100]"
    elif idx == OTHER_SLOT:
        name = "OTHER(>0x100) [slot 0x101]"
    elif idx in NAMES:
        name = f"{NAMES[idx]} (0x{idx:02x})"
    else:
        name = f"code 0x{idx:02x}"
    print(f"  {name:<28} {total[idx]:>8}")
