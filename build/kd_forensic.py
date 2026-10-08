"""svmb-rust KDNET forensic pipeline via mcp_windbg server over stdio.

Modes:
  --linkcheck          open -> vertarget -> close (target left running)
  --forensic           open -> lm(base) -> scan .data for VCPUS ->
                       .writemem all core Info pages -> vertarget -> g -> close
  --arm                boot-break release -> gate READY -> wait fire -> one
                       T+8s freeze + full dump_seq
  --patrol             r126b: boot release -> gate READY -> wait fire ->
                       repeated freezes (T+8s then every +45s), each reads the
                       crumbs ring (last committed entries = death trail),
                       core0 exit histogram and uptime; a shrinking uptime
                       means the box triple-faulted since the previous
                       checkpoint -> record crash window and stop.
Dumps land in build/logs/hist_core*.bin; JSON transcript in logs/kd_forensic.json
(transcript is persisted after every checkpoint, even when a step raises).
"""
import asyncio
import json
import os
import re
import sys

CONNECTION = "net:port=50000,key=2sazdj33vv3ym.14h4ybb3lujun.aoyihkwsbg1b.2xxysisdr5i28"
SYMS = ("srv*C:\\symbols*https://msdl.microsoft.com/download/symbols;"
        r"C:\Users\www\AiCode\svmb-rust\crates\svmb-driver\target\debug;"
        r"C:\Users\www\AiCode\svmb-rust\crates\svmb-driver\target\debug\deps;"
        r"C:\Users\www\AiCode\svmb-rust\x64\Debug")
LOGDIR = r"C:\Users\www\AiCode\svmb-rust\build\logs"
DATA_RVA = 0x1E000          # legacy fallback only - real value comes from
                            # data_rva() below (the section has moved between
                            # builds: 0x1E000 -> 0x17000 broke two sessions)
DATA_LEN = 0x280
DRIVER_FILE = r"C:\Users\www\AiCode\svmb-rust\x64\Debug\svmb_rust.sys"


def data_rva():
    """PE-parse the .data VirtualAddress from the current driver build.

    Hardcoded RVAs drifted twice already; always prefer the real section
    table, fall back to the constant only if the file is unreadable.
    """
    import struct
    try:
        with open(DRIVER_FILE, "rb") as f:
            blob = f.read(0x2000)
        pe = struct.unpack_from("<I", blob, 0x3C)[0]
        nsec = struct.unpack_from("<H", blob, pe + 6)[0]
        opt = struct.unpack_from("<H", blob, pe + 20)[0]
        if pe + 24 + opt + nsec * 40 > len(blob):
            raise ValueError("section table beyond read window")
        for i in range(nsec):
            off = pe + 24 + opt + i * 40
            name = blob[off:off + 8].rstrip(b"\x00").decode("ascii", "replace")
            if name == ".data":
                return struct.unpack_from("<I", blob, off + 12)[0]
    except Exception:
        pass
    return DATA_RVA

INFO_OFF = 0x3000           # VcpuInfo slot inside blob
RING_QWORDS = 0x203         # 0x18 header + 128*0x20 entries = 0x1018 bytes


class Sess:
    def __init__(self, session, sid):
        self.session = session
        self.sid = sid

    async def cmd(self, c, timeout=150):
        r = await asyncio.wait_for(
            self.session.call_tool(
                "run_kd_command",
                {"session_id": self.sid, "command": c, "timeout_seconds": timeout},
            ),
            timeout + 20,
        )
        return "\n".join(x.text for x in r.content if hasattr(x, "text"))


async def open_session(session, timeout=120):
    r = await asyncio.wait_for(
        session.call_tool(
            "open_kd_session",
            {"connection_string": CONNECTION, "symbols_path": SYMS,
             "timeout_seconds": timeout},
        ),
        timeout + 30,
    )
    text = "\n".join(x.text for x in r.content if hasattr(x, "text"))
    m = re.search(r"session_id:\s*(\S+)", text)
    if not m:
        raise RuntimeError("open failed:\n" + text[:2000])
    return m.group(1), text


def parse_uptime(text):
    m = re.search(r"System Uptime:\s*(\d+) days (\d+):(\d+):(\d+)", text or "")
    if not m:
        return None
    d, h, mi, s = map(int, m.groups())
    return d * 86400 + h * 3600 + mi * 60 + s


async def main(mode):
    from mcp import ClientSession, StdioServerParameters
    from mcp.client.stdio import stdio_client

    env = dict(os.environ)
    env["_NT_SYMBOL_PATH"] = (
        "SRV*C:\\Users\\www\\AiCode\\windbg-test\\sym*https://msdl.microsoft.com/download/symbols"
    )
    params = StdioServerParameters(
        command=r"C:\Python314\python.exe",
        args=["-m", "mcp_windbg", "--timeout", "180"],
        env=env,
    )
    transcript = {"mode": mode}

    def persist():
        with open(os.path.join(LOGDIR, "kd_forensic.json"), "w", encoding="utf-8") as f:
            json.dump(transcript, f, ensure_ascii=False, indent=1)

    async def dump_seq(s, transcript):
        # warm-up: first command after a freeze sits behind the KDNET
        # "Refreshing KD connection" exchange and can be very slow
        try:
            transcript["warmup"] = await s.cmd("vertarget", timeout=150)
        except Exception as e:
            transcript["warmup_error"] = repr(e)
        lmout = await s.cmd("lm m svmb_rust")
        transcript["lm"] = lmout
        # module table row: "fffff802`664c0000 fffff802`664e4000 svmb_rust"
        row = next((ln for ln in lmout.splitlines() if "svmb_rust" in ln
                    and "`" in ln), None)
        if not row:
            transcript["note"] = "svmb_rust module not found:\n" + lmout[:800]
            transcript["vertarget"] = await s.cmd("vertarget")
            return
        m = re.match(r"\s*([0-9a-fA-F]{8,16})`([0-9a-fA-F]{8,16})", row)
        base = (int(m.group(1), 16) << 32) | int(m.group(2), 16)
        transcript["driver_base"] = hex(base)
        data_va = base + data_rva()
        dq = await s.cmd(f"dq {data_va:x} L{DATA_LEN // 8:x}")
        transcript["data_dump"] = dq
        qwords = [int(x.replace("`", ""), 16) for x in
                  re.findall(r"[0-9a-fA-F]{8,16}`[0-9a-fA-F]{8,16}|(?<![\w`])[0-9a-fA-F]{16}(?![\w`])", dq)]
        blobs = [q for q in qwords
                 if 0xFFFF800000000000 <= q < 0xFFFFFFFFFFFFFFFF
                 and (q & 0xFFF) == 0]
        blobs = sorted(set(blobs))
        transcript["blobs"] = [hex(b) for b in blobs]
        if not blobs:
            transcript["note"] = ("no page-aligned kernel pointers in .data - "
                                  "VCPUS cleared (stop ran?)")
        dumps = {}
        for i, b in enumerate(blobs[:6]):  # first 6 = VCPUS entries
            start = b + INFO_OFF + 0x48  # VcpuInfo.exit_hist
            dqh = await s.cmd(f"dq {start:x} L102")
            dumps[f"core{i}"] = {"blob": hex(b), "hist_dq": dqh}
        transcript["dumps"] = dumps
        # guest VMCB running-state dump for cross-implementation diff
        # (C side captured via symbols in logs/c_vmcb_running.json)
        if blobs:
            b0 = blobs[0]
            try:
                transcript["c0_vmcb_ptr"] = await s.cmd(f"dq {b0:x} L1")
                transcript["c0_save"] = await s.cmd(f"dq poi({b0:x})+0x400 L40")
                transcript["c0_ctrl"] = await s.cmd(f"dq poi({b0:x}) L40")
            except Exception as e:
                transcript["vmcb_dump_error"] = repr(e)
        # crumbs ring: magic0 lives at ring+0 as a DWORD (magic1 'r701' sits
        # next to it, so a qword search for 43564253 can never match)
        try:
            srch = await s.cmd(f"s -d {base:x} L8000 43564253", timeout=150)
            transcript["ring_search"] = srch
            hits = re.findall(r"([0-9a-fA-F]{8,16})`([0-9a-fA-F]{8,16})", srch)
            ring_addrs = []
            for h in hits:
                if len(h[1]) > 8:
                    ring_addrs.append((int(h[0], 16) << 32) | int(h[1], 16))
                elif len(h[0]) <= 8 and len(h[1]) <= 8:
                    v = int(h[0] + h[1].zfill(8)[-8:] if False else h[0] + h[1], 16)
                    ring_addrs.append(v)
            if ring_addrs:
                ring = ring_addrs[0]
                transcript["ring_addr"] = hex(ring)
                # ring: 0x18 header + 128*32B entries
                for off in range(0, RING_QWORDS * 8, 0x400):
                    transcript[f"ring_{off:x}"] = await s.cmd(
                        f"dq {ring + off:x} L{(0x80 if off == 0 else 0x100):x}",
                        timeout=150)
            else:
                transcript["ring_note"] = "magic not found in image range"
        except Exception as e:
            transcript["ring_error"] = repr(e)
        # host-side live register/MSR snapshot (frozen mid-virtualization);
        # kd rejects semicolon chains - one command per call
        for rc in ("r cr0", "r cr3", "r cr4", "r cr8",
                   "rdmsr c0000080", "rdmsr c0000082", "rdmsr c0000084",
                   "rdmsr 277", "rdmsr c0000100", "rdmsr c0000101",
                   "rdmsr c0000102", "rdmsr 174", "rdmsr 175", "rdmsr 176",
                   "rdmsr c0010117"):
            key = "hr_" + rc.replace(" ", "_")
            try:
                transcript[key] = await s.cmd(rc, timeout=150)
            except Exception as e:
                transcript[key] = repr(e)
        transcript["vertarget"] = await s.cmd("vertarget")

    async def patrol(s, session, sid, transcript):
        """r126b: repeated freeze-snapshots after fire; ring trail per snapshot,
        uptime regression == triple fault since previous checkpoint."""
        # persistent kd log: driver DbgPrint lands on host disk in real time,
        # surviving even a triple-fault reset (the log file is host-side)
        try:
            transcript["logopen"] = await s.cmd(
                ".logopen " + os.path.join(LOGDIR, "kd_patrol.log"), timeout=30)
        except Exception as e:
            transcript["logopen_err"] = repr(e)
        try:
            await asyncio.wait_for(s.cmd("g", timeout=20), 30)
            transcript["boot_go"] = "sent"
        except Exception as e:
            transcript["boot_go_note"] = str(e)
        with open(os.path.join(LOGDIR, "arm_gate.txt"), "w") as f:
            f.write("READY\n")
        fire_path = os.path.join(LOGDIR, "fire_signal.txt")
        loop = asyncio.get_event_loop()
        deadline = loop.time() + 3000
        fired = None
        last_guard = loop.time()
        while loop.time() < deadline:
            if os.path.exists(fire_path):
                fired = open(fire_path).read().strip()
                break
            # guard: if the target rebooted mid-wait it sits frozen at a boot
            # break nobody releases -> deployment would die. A periodic g
            # releases it; against a running target it just errors out.
            if loop.time() - last_guard > 15:
                last_guard = loop.time()
                try:
                    await asyncio.wait_for(s.cmd("g", timeout=10), 15)
                except Exception:
                    pass
            await asyncio.sleep(0.5)
        if not fired:
            transcript["note"] = "no fire signal within 3000s"
            return
        transcript["fired_at"] = fired
        persist()
        cps = [8] + [8 + 45 * i for i in range(1, 24)]
        t0 = loop.time()
        state = {"base": None, "ring": None, "blob0": None,
                 "prev_up": None, "misses": 0}
        checkpoints = []
        transcript["checkpoints"] = checkpoints
        for cp in cps:
            entry = {"cp_s": cp}
            checkpoints.append(entry)
            now = loop.time() - t0
            if now < cp:
                await asyncio.sleep(cp - now)
            entry["wall_s"] = round(loop.time() - t0, 1)
            # --- freeze (2 attempts per checkpoint) ---
            landed = False
            for att in range(2):
                try:
                    await asyncio.wait_for(
                        session.call_tool("send_ctrl_break",
                                          {"session_id": sid}), 30)
                    wfb = await asyncio.wait_for(
                        session.call_tool(
                            "wait_for_break",
                            {"session_id": sid, "timeout_seconds": 45}), 60)
                    txt = "\n".join(x.text for x in wfb.content
                                    if hasattr(x, "text"))
                    low = txt.lower()
                    if "did not stop" in low or "timed out" in low or "timeout" in low:
                        entry[f"break{att}"] = txt[-160:]
                        continue
                    landed = True
                    entry[f"break{att}"] = "LANDED"
                    break
                except Exception as e:
                    entry[f"break{att}_err"] = repr(e)
            if not landed:
                entry["status"] = "no-freeze"
                state["misses"] += 1
                persist()
                if state["misses"] >= 3:
                    transcript["phase"] = "unreachable x3 - patrol stopped"
                    return
                continue
            state["misses"] = 0
            # --- reads while frozen ---
            up = None
            try:
                vt = await s.cmd("vertarget", timeout=60)
                up = parse_uptime(vt)
                entry["vt_line"] = next((ln for ln in vt.splitlines()
                                         if "Uptime" in ln or "uptime" in ln), "")
            except Exception as e:
                entry["vt_err"] = repr(e)
            entry["uptime_s"] = up
            if up is not None and state["prev_up"] is not None \
                    and up < state["prev_up"] - 60:
                entry["crash"] = ("REBOOT: uptime %s -> %s -> triple fault "
                                  "since previous checkpoint"
                                  % (state["prev_up"], up))
                transcript["phase"] = "CRASH between cp %s and %s" % (
                    checkpoints[-2]["cp_s"] if len(checkpoints) > 1 else 0, cp)
                try:
                    entry["lm_after"] = await s.cmd("lm m svmb_rust", timeout=60)
                except Exception as e:
                    entry["lm_after_err"] = repr(e)
                persist()
                return
            if up is not None:
                state["prev_up"] = up
            # one-time module base
            if state["base"] is None:
                try:
                    lmout = await s.cmd("lm m svmb_rust", timeout=60)
                    row = next((ln for ln in lmout.splitlines()
                                if "svmb_rust" in ln and "`" in ln), None)
                    if row:
                        m = re.match(r"\s*([0-9a-fA-F]{8,16})`([0-9a-fA-F]{8,16})", row)
                        state["base"] = (int(m.group(1), 16) << 32) | int(m.group(2), 16)
                        transcript["driver_base"] = hex(state["base"])
                except Exception as e:
                    entry["lm_err"] = repr(e)
            # one-time ring address: PDB symbol first, dword magic scan second
            if state["base"] and state["ring"] is None:
                try:
                    await s.cmd(".reload /f svmb_rust.sys", timeout=90)
                    xo = await s.cmd("x svmb_rust!*CRUMB_RING*", timeout=60)
                    entry["xsym"] = xo.strip()[:200]
                    m = re.search(r"([0-9a-fA-F]{8,16})`([0-9a-fA-F]{8,16})", xo)
                    if m:
                        state["ring"] = ((int(m.group(1), 16) << 32)
                                         | int(m.group(2), 16))
                except Exception as e:
                    entry["x_err"] = repr(e)
                if state["ring"] is None:
                    try:
                        sr = await s.cmd(
                            "s -d %x L8000 43564253" % state["base"], timeout=120)
                        entry["ssearch"] = sr.strip()[:300]
                        m = re.search(r"([0-9a-fA-F]{8,16})`([0-9a-fA-F]{8,16})", sr)
                        if m:
                            state["ring"] = ((int(m.group(1), 16) << 32)
                                             | int(m.group(2), 16))
                    except Exception as e:
                        entry["s_err"] = repr(e)
                if state["ring"] is not None:
                    transcript["ring_addr"] = hex(state["ring"])
            # ring dump: header w + full 128 entries (0x1018 bytes)
            if state["ring"] is not None:
                try:
                    entry["ring"] = await s.cmd(
                        "dq %x L%x" % (state["ring"], RING_QWORDS), timeout=150)
                except Exception as e:
                    entry["ring_err"] = repr(e)
            # core0 blob discovery (page-aligned kernel ptrs in .data)
            if state["base"] and state["blob0"] is None:
                try:
                    dq = await s.cmd(
                        "dq %x L%x" % (state["base"] + data_rva(), DATA_LEN // 8),
                        timeout=120)
                    qwords = [int(x.replace("`", ""), 16) for x in
                              re.findall(r"[0-9a-fA-F]{8,16}`[0-9a-fA-F]{8,16}", dq)]
                    blobs = [q for q in qwords
                             if 0xFFFF800000000000 <= q < 0xFFFFFFFFFFFFFFFF
                             and (q & 0xFFF) == 0]
                    if blobs:
                        state["blob0"] = sorted(set(blobs))[0]
                        transcript["blobs"] = [hex(b) for b in sorted(set(blobs))]
                except Exception as e:
                    entry["data_err"] = repr(e)
            if state["blob0"] is not None:
                try:
                    entry["hist0"] = await s.cmd(
                        "dq %x L102" % (state["blob0"] + INFO_OFF + 0x48),
                        timeout=120)
                except Exception as e:
                    entry["hist_err"] = repr(e)
            entry["status"] = "ok"
            persist()
            # resume the target (virtualization time only advances between g's)
            try:
                await asyncio.wait_for(s.cmd("g", timeout=20), 30)
            except Exception as e:
                entry["go_note"] = str(e)
        transcript["phase"] = "patrol finished, no crash within window"

    async def watch(s, session, sid, transcript):
        """r127: passive death watch. logopen captures everything kd receives;
        after fire we send NOTHING (any command would break/perturb the live
        box). Poll the host-side log file for the driver's 0xDEADDEAD
        self-report (KeBugCheckEx from the VMEXIT_SHUTDOWN handler) or a
        silent-reset banner, then dump while the box is halted at bugcheck."""
        logpath = os.path.join(LOGDIR, "kd_patrol.log")
        try:
            transcript["logopen"] = await s.cmd(
                ".logopen " + logpath, timeout=30)
        except Exception as e:
            transcript["logopen_err"] = repr(e)
        try:
            await asyncio.wait_for(s.cmd("g", timeout=20), 30)
            transcript["boot_go"] = "sent"
        except Exception as e:
            transcript["boot_go_note"] = str(e)
        with open(os.path.join(LOGDIR, "arm_gate.txt"), "w") as f:
            f.write("READY\n")
        fire_path = os.path.join(LOGDIR, "fire_signal.txt")
        loop = asyncio.get_event_loop()
        deadline = loop.time() + 3000
        fired = None
        last_guard = loop.time()
        while loop.time() < deadline:
            if os.path.exists(fire_path):
                fired = open(fire_path).read().strip()
                break
            if loop.time() - last_guard > 15:
                last_guard = loop.time()
                try:
                    await asyncio.wait_for(s.cmd("g", timeout=10), 15)
                except Exception:
                    pass
            await asyncio.sleep(0.5)
        if not fired:
            transcript["note"] = "no fire signal within 3000s"
            return
        transcript["fired_at"] = fired
        persist()
        watch_until = loop.time() + 900
        while loop.time() < watch_until:
            await asyncio.sleep(3)
            tail = ""
            try:
                with open(logpath, "r", errors="replace") as f:
                    tail = f.read()[-4000:]
            except OSError:
                continue
            low = tail.lower()
            if "0xdeaddead" in low or "bugcheck" in low or "*** stop" in low:
                transcript["phase"] = "BUGCHECK CAPTURED"
                transcript["death_banner"] = tail[-1500:]
                persist()
                # target is halted at the bugcheck: read everything directly
                await asyncio.sleep(5)  # let kd finish printing the report
                try:
                    await dump_seq(s, transcript)
                    transcript["dump"] = "ok"
                except Exception as e:
                    transcript["dump_err"] = repr(e)
                persist()
                return
            if ("restarted without notifying" in low
                    or "transport link disconnected" in low):
                transcript["phase"] = "SILENT RESET (no self-report)"
                transcript["death_banner"] = tail[-1500:]
                persist()
                return
        transcript["phase"] = "survived 900s watch window"
        persist()

    async def run_all():
        async with stdio_client(params) as (read, write):
            async with ClientSession(read, write) as session:
                await session.initialize()
                sid, banner = await open_session(session)
                transcript["open"] = banner
                s = Sess(session, sid)
                try:
                    if mode == "--linkcheck":
                        transcript["vertarget"] = await s.cmd("vertarget")
                    elif mode == "--watch":
                        await watch(s, session, sid, transcript)
                    elif mode == "--patrol":
                        await patrol(s, session, sid, transcript)
                    elif mode == "--arm":
                        transcript["vertarget_boot"] = await s.cmd("vertarget")
                        # release the boot break so the guest can reach desktop
                        try:
                            await asyncio.wait_for(s.cmd("g", timeout=20), 30)
                            transcript["boot_go"] = "sent"
                        except Exception as e:
                            transcript["boot_go_note"] = str(e)
                        with open(os.path.join(LOGDIR, "arm_gate.txt"), "w") as f:
                            f.write("READY\n")
                        fire_path = os.path.join(LOGDIR, "fire_signal.txt")
                        deadline = asyncio.get_event_loop().time() + 3000
                        fired = None
                        while asyncio.get_event_loop().time() < deadline:
                            if os.path.exists(fire_path):
                                fired = open(fire_path).read().strip()
                                break
                            await asyncio.sleep(0.5)
                        if not fired:
                            transcript["note"] = "no fire signal within 3000s"
                        else:
                            transcript["fired_at"] = fired
                            # T+8s freeze: zeroing-fix build crashes inside
                            # ~10-30s of fire; freeze before it with some exits
                            await asyncio.sleep(8)
                            transcript["phase"] = "sending ctrl-break"
                            try:
                                brk = await asyncio.wait_for(
                                    session.call_tool("send_ctrl_break",
                                                      {"session_id": sid}), 30)
                                transcript["break_req"] = "\n".join(
                                    x.text for x in brk.content
                                    if hasattr(x, "text"))
                                wfb = await asyncio.wait_for(
                                    session.call_tool(
                                        "wait_for_break",
                                        {"session_id": sid,
                                         "timeout_seconds": 45}), 60)
                                transcript["wait_for_break"] = "\n".join(
                                    x.text for x in wfb.content
                                    if hasattr(x, "text"))
                                transcript["phase"] = "dumping"
                                await dump_seq(s, transcript)
                                transcript["phase"] = "dumped-ok"
                            except Exception as e:
                                transcript["break_error"] = repr(e)
                            try:
                                await asyncio.wait_for(s.cmd("g", timeout=20), 30)
                            except Exception as e:
                                transcript["final_go_note"] = str(e)
                    else:
                        await dump_seq(s, transcript)
                    if mode not in ("--patrol", "--watch"):
                        try:
                            await asyncio.wait_for(s.cmd("g", timeout=20), 30)
                        except Exception as e:
                            transcript["go_note"] = f"go not awaited ({e}) - close will resume"
                finally:
                    try:
                        close = await asyncio.wait_for(
                            session.call_tool("close_kd_session",
                                              {"session_id": sid}), 60)
                        transcript["close"] = "\n".join(
                            x.text for x in close.content
                            if hasattr(x, "text"))
                    except Exception as e:
                        transcript["close_error"] = str(e)
                    if mode in ("--arm", "--patrol", "--watch"):
                        with open(os.path.join(LOGDIR, "done.txt"), "w") as f:
                            f.write("DONE\n")

    try:
        await run_all()
    except Exception as e:
        transcript["error"] = repr(e)
    persist()
    print(json.dumps({k: v for k, v in transcript.items()
                      if k in ("mode", "driver_base", "ring_addr", "blobs",
                               "phase", "note", "error", "close",
                               "close_error")},
                     ensure_ascii=False, indent=1))


if __name__ == "__main__":
    mode = sys.argv[1] if len(sys.argv) > 1 else "--linkcheck"
    asyncio.run(main(mode))
