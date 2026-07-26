# MASTERPLAN — auto-e2e（给真机功能补 e2e 埋点 + 全自动测试）

> Phase A 深度评估已成（2 并行 agent：Windows GUI 驱动可行性 + 逐功能 probe/fixture 方案）。
> **本文件待用户批准后才进 B/C**（planned-build 铁律 #7）。分支 `account-ux`。

## 目标
让本轮真机功能的**端到端行为**可自动验证——现状：纯逻辑有 595 vitest + cargo 单测，但 daemon→帧→emitter→前端灯 / resume→tmux / 换号编排 等**跨进程整链零 e2e**。补 DEV 探针 + 确定性 fixture + 驱动，做成可重复的全自动冒烟。

## 关键发现（决定架构）
1. **驱动分两条、对应两类功能**（agent A）：
   - **DOM 层**（tab 点击 / 自建右键菜单 / resume 按钮 / 快捷键）→ WebdriverIO + `@wdio/tauri-service` + `tauri-driver 2.0.x` 驱 WebView2，**能上 windows-latest CI、Windows 端免 xvfb**。
   - **OS 窗口层**（↗ `SetForegroundWindow` 拉外部终端）→ WebDriver 看不见 + 前台锁 flaky → **仅交互式 Windows VM 半自动/手动**（逻辑已 bind.rs 原生测钉住）。
   - **外部进程层**（真 wt.exe / PowerShell profile / SFTP 传输）→ **保持手动**（README 已登记）。
2. **断言出口 = `[e2e]` 日志**（agent B）：`invoke("frontend_perf_log")` 写 `monitor.<date>.log`，DEV 门控（`import.meta.env.DEV`）。`snapshotSessions()` **已含** `{status,tmuxIdle,origin,account}`——只差一个 emitter，不用重造。
3. **★最大发现：高 ROI 的会话生命周期功能不需要 Windows VM**。gray-light/resume/换号/孤儿 是**后端+SSH+tmux 驱动**，可在 **Linux（aya 或 ubuntu CI 自环）** 上用「loopback SSH 到本机 + daemon(CLAUDE_CONFIG_DIR 隔离) + fake-claude shim + tmux @ccm_sid fixture + `[e2e]` 日志断言」全自动跑，**GH ubuntu runner 本身就够当那台"远端"**。Windows VM 只剩 Win32 层（↗）留手动。

## 三层测试架构（最终形态）
- **Tier 1 — 会话生命周期 e2e（Linux，aya/ubuntu CI 自环，最高 ROI）**：fixture 造确定性远端态 + `[e2e]` 日志断言 + 少量 GUI 触发（xdotool 右键→菜单）。覆盖 gray-light / resume-无孤儿 / 换号编排 / 孤儿清理 / attach。**这是本轮真正要补的核心**，且不依赖 Windows。
- **Tier 2 — Windows DOM 冒烟（windows-latest CI，WebDriver）**：每类 GUI 交互 1–2 条 happy-path（菜单出现/按钮可点/快捷键触发/tab 渲染）。保真 WebView2 渲染 + 接线。**薄**，不重造单测已覆盖的逻辑。
- **Tier 3 — Win32/native（↗ HWND / PowerShell / 真终端 / SFTP）**：交互式 Windows VM 手动 smoke。给一份清单。

## ★共享面账本（≥2 功能改到 / 需先落地）
| 面 | 最终形态 | 谁用 |
|---|---|---|
| **e2e 基建**（一次性，挡在所有功能埋点前）：loopback-remote 配置 + daemon wrapper（`CLAUDE_CONFIG_DIR=/tmp/e2e-remote-claude` 隔离，防与本地 ~/.claude 双 tab）+ `fake-claude` shim（记 argv+env→argv.log、写 pidfile 喂 daemon 判活、追 jsonl、sleep 常驻）+ `gen-idle-tmux.sh`（`tmux new -d -s cc-<x8>` + `set-option @ccm_sid`）。放 `e2e/` 与 `gen-fork-session.py` 同级。 | Tier1 全部功能 |
| **探针汇总**（DEV 门控，`[e2e]` 日志）：新 `TabManager.debugSessionsSnapshot()`（复用 snapshotSessions + 派生 mismatch，**不改** `debugSnapshot()` 形状以免破 f40-suite）+ `e2e-probe.ts` 加 Ctrl+Alt+F10/中键 account-chip 触发 + 状态转移 emitter（tab-state/resume/attach-idle/restart/orphan/mismatch-count 各真值点）+ 统一 `e2eLog()` 小工具。 | Tier1 全部 + Tier2 |
| **可注入 confirm**（唯一动生产代码处，行为等价 seam）：`cleanupOrphanTmux`/`killRemoteTmux` 现用裸 `window.confirm`（headless 卡死）→ 加 `opts.confirm` 注入（对齐 `account-restart.ts` 已有范式），DEV 下自动接受。 | F05 孤儿、Tier2 |
| WebDriver harness：`wdio.conf.ts` + `@wdio/tauri-service` + 锁版本 + test build（纯 DOM 路径不需 `withGlobalTauri:true`）。 | Tier2 |

## 功能清单 + 依赖顺序（按 ROI）
- **F-E0 e2e 基建**（前置，必先）：loopback-remote + daemon wrapper + fake-claude shim + gen-idle-tmux + `e2eLog()`/`debugSessionsSnapshot()` + 可注入 confirm。**这是地基，先做。**
- **F-E1 gray-light 全链**（最高 ROI）：live→(kill proc)→tmuxIdle=1→(kill-session,另留一个无关 tmux 防空 backend 卡灰)→archived 的 `[e2e] tab-state` 序列。跨进程整链、单测碰不到。
- **F-E2 resume idle 就地复用**（#75/#76）：右键 resume→`mode=idle-reuse name=cc-<sid8>`（无 `-N` 孤儿）+ argv.log 验命令 + 复活清灰 + `tmux has-session` 无孤儿。
- **F-E3 换号重启编排**（#68/#69）：jsonl 追 compact 记录驱动 compact→exit→kill→resume 序列 + argv.log 验新 `CLAUDE_CONFIG_DIR`。
- **F-E4 孤儿清理**（F05，前置=可注入 confirm）：造无 tab 的 `cc-*` + 非 cc-* 用户会话 → scan/cleanup 计数 + tmux 真删 + 不误伤。
- **F-E5 Tier2 Windows DOM 冒烟**（WebDriver）：右键菜单/resume 按钮/快捷键/tab 渲染 薄 happy-path。
- **F-E6 attach-idle + account-badge**（低 ROI，搭车）：一条 send-keys/菜单文案浅断言 + `[e2e] sessions` 加 mismatch 字段。单测已够，不建专用 fixture。
- **不做**：↗ HWND / PowerShell / 真终端 / SFTP 传输 / 系统通知 = Tier3 手动清单（Windows VM）。真 claude 真 resume 出内容 = hard-to-fixture，不追。

## 安全 / 红线 / 不做什么
- **daemon 零行为改动**：只加测试 fixture（外部 wrapper/shim，不改 daemon 源）+ 前端 DEV 探针。
- **探针生产零包含**：全 `import.meta.env.DEV` 门控（同 e2e-probe.ts）。
- **唯一动生产代码 = 可注入 confirm**（行为等价、加 DEV seam，不改默认交互）。
- **不 push/发版/bump · 不改 TMUX_LS_FMT · 不碰 ~/.bashrc · 不要 emoji**。
- 版本脆：`@wdio/tauri-service`(next) + msedgedriver 精确匹配 → 锁版本、可接受升级返工。

## 测试约定
Tier1 = bash 套件（仿 f40-suite）跑 Linux dev app（xvfb）+ 断言 `[e2e]` 日志；能进 ubuntu-latest CI（自环）。Tier2 = wdio 跑 windows-latest CI。每步 tsc+vitest 绿；探针改动不破 f40-suite 既有断言。
