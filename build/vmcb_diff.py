import json
import re

def parse_dq(text):
    out = {}
    for ln in text.splitlines():
        m = re.match(r"^([0-9a-fA-F]{8,16})`([0-9a-fA-F]{8,16})\s+(.*)$", ln.strip())
        if not m:
            continue
        addr_hi, addr_lo, rest = m.group(1), m.group(2), m.group(3)
        base = (int(addr_hi, 16) << 32) | int(addr_lo, 16)
        vals = re.findall(r"[0-9a-fA-F]{8,16}`[0-9a-fA-F]{8,16}|(?<![\w`])[0-9a-fA-F]{16}(?![\w`])", rest)
        for i, v in enumerate(vals):
            q = int(v.replace("`", ""), 16)
            out[base + i * 8] = q
    return out

c = json.load(open(r"C:\Users\www\AiCode\svmb-rust\build\logs\c_vmcb_running.json", encoding="utf-8"))
r = json.load(open(r"C:\Users\www\AiCode\svmb-rust\build\logs\kd_forensic.json", encoding="utf-8"))

def get(d, prefix):
    for k, v in d.items():
        if k.startswith(prefix):
            return v[0] if isinstance(v, list) else str(v)
    return ""

CTRL_NAMES = {0x00: "InterceptCr0rd/3CR0wr(两 dword)", 0x08: "OP1|OP2", 0x10: "OP3",
              0x18: "INT_CTRL", 0x20: "?0x20", 0x28: "?0x28", 0x30: "?0x30",
              0x3C: "IOPM_BASE", 0x40: "MSRPM_BASE", 0x48: "TSC_OFFSET",
              0x58: "TLB_CTL@0x58?/ASID", 0x60: "V_TPR", 0x70: "EXITCODE",
              0x78: "EXITINFO1", 0x80: "EXITINFO2", 0x88: "EXITINTINFO",
              0x90: "?0x90", 0xA0: "NP_ENABLE@0xA0?", 0xB0: "NCR3",
              0xC0: "CleanBits", 0xC8: "NRIP(0xC8)", 0xD0: "?0xD0"}
SAVE_NAMES = {0x400: "ES", 0x410: "CS", 0x420: "SS", 0x430: "DS", 0x440: "FS",
              0x450: "GS", 0x460: "LDTR", 0x470: "GDTR", 0x480: "IDTR",
              0x490: "TR", 0x4A0: "?4A0(CPL@4A8)", 0x4B0: "?4B0", 0x4C0: "?4C0",
              0x4D0: "EFER", 0x4E0: "?4E0", 0x4F0: "CR4", 0x500: "CR3",
              0x510: "CR0", 0x520: "DR7", 0x530: "DR6", 0x548: "RFLAGS",
              0x550: "RSP?/CR3@0x550?", 0x578: "RIP", 0x5D8: "RAX?SYSENTER",
              0x600: "STAR", 0x608: "LSTAR", 0x610: "CSTAR", 0x618: "SFMASK",
              0x620: "KERNEL_GS_BASE", 0x628: "SYSENTER_CS", 0x630: "SYSENTER_ESP",
              0x638: "SYSENTER_EIP", 0x640: "GPAT", 0x668: "BR_FROM",
              0x678: "BR_TO", 0x688: "LASTEXCPFROM", 0x698: "LASTEXCPTO"}

for area, ckey, rkey in (("CTRL", "dq poi(0xffffc90df06ed000)+0x0 L40", None),
                         ("SAVE", "dq poi(0xffffc90df06ed000)+0x400 L40", None)):
    cd = parse_dq(get(c, ckey))
    rd = parse_dq(get(r, ("c0_ctrl" if area == "CTRL" else "c0_save")))
    cbase = min(cd) if cd else 0
    rbase = min(rd) if rd else 0
    print(f"===== {area}: C@{cbase:x} R@{rbase:x} =====")
    cofs = {a - cbase: v for a, v in cd.items()}
    rofs = {a - rbase: v for a, v in rd.items()}
    for off in sorted(set(cofs) | set(rofs)):
        cv, rv = cofs.get(off), rofs.get(off)
        name = (CTRL_NAMES if area == "CTRL" else SAVE_NAMES).get(off, "")
        if cv != rv:
            print(f"  +0x{off:03X} {name:24s} C={cv if cv is None else hex(cv):>20s} R={rv if rv is None else hex(rv):>20s}")
        else:
            print(f"  +0x{off:03X} {name:24s} = {hex(cv)}")
