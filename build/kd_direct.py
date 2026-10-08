"""Drive mcp_windbg server directly over stdio (no 30s client limit).

Usage: python kd_direct.py "<kd command>" [...] | --open-only | --base
Each command runs on one KDSession; session is closed (resumed) at the end.
"""
import asyncio
import json
import os
import re
import sys

CONNECTION = "net:port=50000,key=2sazdj33vv3ym.14h4ybb3lujun.aoyihkwsbg1b.2xxysisdr5i28"
SYMS = ("srv*C:\\symbols*https://msdl.microsoft.com/download/symbols;"
        "C:\\Users\\www\\AiCode\\svmb-rust\\x64\\Debug;"
        "C:\\Users\\www\\AiCode\\svmb\\x64\\Debug")


async def run(cmds, open_timeout=120, per_cmd_timeout=60):
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
    async with stdio_client(params) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            result = await session.call_tool(
                "open_kd_session",
                {
                    "connection_string": CONNECTION,
                    "symbols_path": SYMS,
                    "timeout_seconds": open_timeout,
                },
            )
            texts = [c.text for c in result.content if hasattr(c, "text")]
            out = {"open": texts}
            m = re.search(r"session_id:\s*(\S+)", texts[0])
            sid = m.group(1) if m else None
            if sid is None:
                print(json.dumps(out, ensure_ascii=False, indent=1))
                return
            for cmd in cmds:
                r = await asyncio.wait_for(
                    session.call_tool(
                        "run_kd_command",
                        {"session_id": sid, "command": cmd,
                         "timeout_seconds": per_cmd_timeout},
                    ),
                    per_cmd_timeout + 15,
                )
                out[cmd] = [c.text for c in r.content if hasattr(c, "text")]
            close = await asyncio.wait_for(
                session.call_tool("close_kd_session", {"session_id": sid}),
                60,
            )
            out["close"] = [c.text for c in close.content if hasattr(c, "text")]
            print(json.dumps(out, ensure_ascii=False, indent=1))


if __name__ == "__main__":
    args = sys.argv[1:]
    cmds = [] if not args or args[0] == "--open-only" else args
    asyncio.run(run(cmds))
