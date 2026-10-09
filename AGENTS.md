# svmb-rust 项目规则

## 1. 虚拟机调试必须使用 windbg-mcp(2026-10-07 用户指示)

**规则:对 VMware 测试 VM(Windows 10 x64, KDNET)的一切内核调试操作,必须通过
windbg-mcp 工具进行,禁止直接在 shell 里启动裸 `kd.exe` / `cdb.exe` 连接 VM。**

- 连接 VM: `mcp__mcp_windbg__open_kd_session`(connection_string =
  `net:port=50000,key=<your-kdnet-key>`;本机实际值在 git-ignored 的 build/vmenv.local.bat)
- 执行命令: `run_kd_command`;冻结目标: `send_ctrl_break`;等待停机: `wait_for_break`;
  结束必须 `close_kd_session`(释放 UDP 50000,防僵尸进程阻塞下次连接)
- 纯离线静态分析(对 .sys 查符号 RVA、对 .dmp 文件 `cdb -z`)不接触运行中的 VM,
  不受此限;但凡是与运行中 VM 建立调试连接的操作,一律走 windbg-mcp

已知注意事项(windbg-mcp 实战教训):
- 单条工具调用约 30s 硬超时:命令小步发,不要在一条 `run_kd_command` 里串长脚本
- kd 任何命令都会 halt 目标:观测类命令读完立即恢复(`g`)或用 `close_kd_session`
  让目标继续运行,否则 guest 全冻(Tools/vmrun 全灭,表象酷似 VM 死机)
- 会话结束/中断后必须 close;若怀疑残留进程,先 `tasklist | grep kd.exe` 再 taskkill

## 2. 其他既有硬约束(摘要)

- 工具链: LLVM 17.0.6、WDK 28000(26100 无 km 头)、驱动静态 CRT(详见记忆
  svmb-rust-toolchain-constraints)
- guest/宿主 bat 一律纯 ASCII;带引号/分号的调试器命令写成 .bat 再执行
- vmrun exec 命令串一律追加 `< NUL`;覆盖 guest 内 .sys 前先 `sc stop` + 等 3s
- per-core 页不 free(r112 泄漏纪律);每次改码后函数 RVA 漂移,断点前必须重新解析
