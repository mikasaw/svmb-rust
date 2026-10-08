"""Offline crumbs-ring scanner for a frozen VM's .vmem (r127).

The ring starts with magic0 'SBVC'(0x43564253) + magic1 'r701'(0x31303752)
as two LE u32 -> byte signature "SBVCR701". Layout: +0x08 w(i32 total posts),
+0x10 dropped(i32), +0x18 entries[128] x 32B (tag i32, cpu i32, tick i64,
seq i64, data i64); seq commits last, so valid entries are 0 < seq <= w and
the seq-sorted tail is the flight-recorder trail at freeze time.

Usage: vmem_scan.py <file.vmem> [tail]
"""
import struct
import sys

TAGS = {
    0x183: "EXIT_EPILOGUE",
    0x184: "VMRUN_PRE",
    0x185: "EXIT_ENTRY",
    0x190: "VMRUN_FAIL(code)",
    0x191: "LEAVING(state)",
    0x192: "TRACE_code",
    0x193: "TRACE_rip",
    0x194: "TRACE_nrip",
    0x195: "SHUTDOWN_CAUGHT(rip)",
}
SIG = b"SBVCR701"
ENT = struct.Struct("<iiqqq")  # tag, cpu, tick, seq, data


def main():
    path = sys.argv[1]
    tail = int(sys.argv[2]) if len(sys.argv) > 2 else 24
    hits = []
    with open(path, "rb") as f:
        off = 0
        while True:
            chunk = f.read(1 << 24)  # 16MB
            if not chunk:
                break
            i = chunk.find(SIG)
            while i != -1:
                hits.append(off + i)
                i = chunk.find(SIG, i + 1)
            off += len(chunk)
    print("ring magic hits at:", [hex(h) for h in hits])
    for h in hits:
        with open(path, "rb") as f:
            f.seek(h)
            blob = f.read(0x18 + 128 * 32)
        if len(blob) < 0x18:
            continue
        w, _pad, dropped = struct.unpack("<iii", blob[8:20])
        print("\n=== ring @ %#x  w(total posts)=%d dropped=%d ===" % (h, w, dropped))
        ents = []
        for k in range(128):
            e = ENT.unpack_from(blob, 0x18 + k * 32)
            if 0 < e[3] <= max(w, 1):
                ents.append(e)
        ents.sort(key=lambda e: e[3])
        print("valid=%d committed seq %s..%s"
              % (len(ents), ents[0][3] if ents else "-", ents[-1][3] if ents else "-"))
        for tag, cpu, tick, seq, data in ents[-tail:]:
            print("  seq=%-6d tag=%03x %-18s cpu=%d tick=%#x data=%#x"
                  % (seq, tag, TAGS.get(tag, "?"), cpu, tick & 0xFFFFFFFFFFFF, data))
        # tick deltas of the last few (same-core stall visible as flat ticks)
        if len(ents) >= 2:
            print("  last tick delta=%#x (0 = same-instant burst)" %
                  (ents[-1][4 - 1] - ents[-2][4 - 1]))


if __name__ == "__main__":
    main()
