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

## ★SSH 驱动 Windows 全自动（用户 2026-07-26 定：我从 aya SSH 进 VM 自动测）
用户要「全自动、VM 开 SSH、我进去把能测的都自动测掉」。诚实边界 + 架构：
- **能全自动（SSH 可行）**：DOM 层（WebDriver 走自动化协议非 OS 输入，非交互 SSH 会话大概率能驱）+ 后端会话生命周期（VM app 连 **aya 当远端**=生产拓扑，我在 aya 造 fixture，断言读 VM `[e2e]` 日志 + aya tmux 核）。**覆盖全部真 bug（#75/#76/#68/#69/gray-light）。**
- **SSH 下不可靠（诚实限）**：OS 窗口/native 层（↗ `SetForegroundWindow`/真终端/PowerShell/SFTP/通知）——session-0 无前台。升级路=VM 自动登录 + `schtasks`/`PsExec -i 1` 打进交互会话（覆盖↑但 ↗ 前台仍 flaky+脆）。给用户选：交互会话硬上(带 flaky 警告) 或 留手动 smoke。**不承诺 SetForegroundWindow headless 稳过。**
- **架构**：aya 编排脚本 → SSH `cargo tauri build --debug`(探针内建) → SSH `npm run wdio`(驱 DOM) → aya 造 tmux/@ccm_sid/fake-claude/daemon fixture(VM app 连 aya) → 断言 = WebDriver DOM + SSH `Get-Content` VM `[e2e]` 日志 + aya `tmux has-session`。全程 aya 一条命令，用户不碰。
- **必打 spike（前置，未验证假设）**：WebDriver 能否在**非交互 SSH 会话**起 WebView2 app 并驱 DOM（CI runner 会话 ≠ SSH session-0）。VM 一就绪先跑 hello-world（SSH→tauri-driver+app+一条 trivial DOM 断言），通了才铺全套；不通退交互会话。**别在 spike 前承诺全绿。**
- **VM 一次性前置（用户做）**：OpenSSH server + Node/Rust/WebView2 工具链 + clone repo +（Tier C 才需）自动登录交互会话。

### Tier C「升级到交互会话」recipe（仅 OS-窗口/native 层需要）
SSH 默认落非交互会话（无桌面/前台）→ GUI 自动化要跑进**自动登录的控制台会话(session 1)**。做法：
1. VM 自动登录专用测试用户（netplwiz / `Winlogon.AutoAdminLogon=1`）。
2. 关屏保/空闲锁定/睡眠（`Winlogon.InactivityTimeoutSecs=0`）——锁屏则前台自动化必败。
3. `HKCU\Control Panel\Desktop\ForegroundLockTimeout=0`——松前台锁（↗ 关键缓解）。
4. 预建计划任务 `schtasks /create /tn e2e-run /tr "powershell -File C:\e2e\run.ps1" /rl HIGHEST /it`（**`/it`=只在登录时跑=落交互会话**）；`run.ps1` 读入参文件、跑 wdio/OS-input 套件、写 result.log。
5. aya 驱动：SSH 写入参 → `schtasks /run /tn e2e-run` → 轮询 `Get-Content C:\e2e\result.log`。
**诚实结论**：即便如此 `SetForegroundWindow` 仍会被前台锁/UAC 偶拒（bind.rs 注释已认）→ ↗ 到不了 CI 级稳，只能重试+标 flaky。**性价比判定：↗ 建议留手动 10 秒 smoke（肉眼看终端是否到前台），别为它维护自动登录+计划任务+重试的脆基建。** DOM+后端层不需交互会话（WebDriver 非交互 SSH 即可），交互会话复杂度隔离在真正需要的那一小撮。

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
