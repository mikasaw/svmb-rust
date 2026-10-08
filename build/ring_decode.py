"""Decode crumbs-ring dq dumps captured by kd_forensic.py --patrol.

Ring layout (crates/svmb-driver/src/svm/exit_handler.rs):
  +0x00 i32 magic0 'SBVC' (0x43564253)   +0x04 i32 magic1 'r701'
  +0x08 i32 w (total posts, monotonic)   +0x10 i32 dropped
  +0x18 entries[128] x 32B:
      +0 i32 tag, +4 i32 cpu, +8 i64 tick(rdtsc), +16 i64 seq, +24 i64 data
  seq is committed LAST -> an entry is valid iff 0 < seq <= w; the tail of
  the seq-sorted entries is the flight recorder trail at freeze time.

Usage: ring_decode.py [kd_forensic.json] [tail_count]
"""
import json
import os
import sys
import struct

LOGDIR = os.path.dirname(os.path.abspath(__file__)) + r"\logs"

TAGS = {
    0x183: "EXIT_EPILOGUE(exit handler done -> asm loop)",
    0x184: "VMRUN_PRE(asm about to VMRUN)",
    0x185: "EXIT_ENTRY(top of Rust exit handler)",
    0x190: "VMRUN_FAIL(code in data)",
    0x191: "LEAVING(state in data)",
    0x192: "TRACE_exit_code(code in data)",
    0x193: "TRACE_rip(rip in data)",
    0x194: "TRACE_nrip(nrip in data)",
    0x195: "SHUTDOWN_CAUGHT(rip in data)->KeBugCheckEx",
}

# u32 exit-code histogram slots (VcpuInfo.exit_hist), only nonzero shown
HIST_NAMES = {
    0x72: "CPUID", 0x7B: "MSR", 0x80: "NPF", 0x81: "EXCP",
    0x400: "SVMB_XP_NPF(slot)", 0x401: "SVMB_XP_OTHER(slot)",
}


def qwords_from_dq(text):
    out = []
    for ln in (text or "").splitlines():
        # lines look like: fffff802`664e1234  43564253`31303752  ...
        parts = ln.split()
        for tok in parts[1:]:
            if "`" in tok and len(tok.replace("`", "")) == 16:
                hi, lo = tok.split("`")
                out.append((int(hi, 16) << 32) | int(lo, 16))
            elif len(tok) == 16 and all(c in "0123456789abcdefABCDEF" for c in tok):
                out.append(int(tok, 16))
    return out


def decode_ring(qw, tail):
    if len(qw) < 4:
        return "  (ring dump too short: %d qwords)" % len(qw)
    magic0, magic1 = struct.unpack("<ii", struct.pack("<qq", qw[0] & 0xFFFFFFFF,
                                                       (qw[0] >> 32) & 0xFFFFFFFF))
    w = qw[1] & 0xFFFFFFFF
    dropped = (qw[2] >> 32) & 0xFFFFFFFF
    lines = ["  magic0=%08x magic1=%08x w(total posts)=%d dropped=%d"
             % (magic0 & 0xFFFFFFFF, magic1 & 0xFFFFFFFF, w, dropped)]
    if (magic0 & 0xFFFFFFFF) != 0x43564253:
        lines.append("  !! magic0 mismatch - not a ring")
        return "\n".join(lines)
    ents = []
    body = qw[3:]  # header is 0x18 = 3 qwords; entries are 4 qwords each
    for i in range(0, len(body) - 3, 4):
        tag, cpu = struct.unpack("<ii", struct.pack("<qq", body[i] & 0xFFFFFFFF,
                                                    (body[i] >> 32) & 0xFFFFFFFF))
        tick = body[i + 1]
        seq = body[i + 2]
        data = body[i + 3]
        if 0 < seq <= w:
            ents.append((seq, tag, cpu, tick, data))
    ents.sort()
    lines.append("  valid entries=%d (committed seq range %s..%s)"
                 % (len(ents), ents[0][0] if ents else "-",
                    ents[-1][0] if ents else "-"))
    for seq, tag, cpu, tick, data in ents[-tail:]:
        name = TAGS.get(tag, "?")
        lines.append("    seq=%-6d tag=%03x %-44s cpu=%d tick=%#x data=%#x"
                     % (seq, tag, name, cpu, tick & 0xFFFFFFFFFFFF, data))
    return "\n".join(lines)


def decode_hist(text):
    qw = qwords_from_dq(text)
    slots = []
    for q in qw:
        slots.append(q & 0xFFFFFFFF)
        slots.append((q >> 32) & 0xFFFFFFFF)
    nz = {i: v for i, v in enumerate(slots) if v}
    if not nz:
        return "  (all hist slots zero)"
    tot = sum(nz.values())
    parts = ["  hist total=%d nonzero=%d:" % (tot, len(nz))]
    for i, v in sorted(nz.items(), key=lambda kv: -kv[1]):
        nm = HIST_NAMES.get(i, "")
        parts.append("    slot[%#x]=%d %s" % (i, v, nm))
    return "\n".join(parts)


def main():
    path = sys.argv[1] if len(sys.argv) > 1 else os.path.join(LOGDIR, "kd_forensic.json")
    tail = int(sys.argv[2]) if len(sys.argv) > 2 else 16
    with open(path, encoding="utf-8") as f:
        t = json.load(f)
    print("mode=%s phase=%s ring_addr=%s base=%s"
          % (t.get("mode"), t.get("phase"), t.get("ring_addr"), t.get("driver_base")))
    for cp in t.get("checkpoints", []):
        print("\n=== checkpoint T+%ss wall=%s status=%s uptime=%s ==="
              % (cp.get("cp_s"), cp.get("wall_s"), cp.get("status"),
                 cp.get("uptime_s")))
        if cp.get("crash"):
            print("  !!! %s" % cp["crash"])
        if cp.get("ring"):
            print(decode_ring(qwords_from_dq(cp["ring"]), tail))
        elif cp.get("ring_err"):
            print("  ring_err=%s" % cp["ring_err"])
        if cp.get("hist0"):
            print(decode_hist(cp["hist0"]))
    for k in ("note", "error"):
        if t.get(k):
            print("\n%s: %s" % (k.upper(), t[k]))


if __name__ == "__main__":
    main()
