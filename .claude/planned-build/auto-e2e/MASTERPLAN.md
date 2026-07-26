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
Tier1 = bash 套件（仿 f40-suite）跑 Linux dev app（xvfb）+ 断言 `[e2e]` 日志；能进 ubuntu-latest CI（自环）。Tier2 = wdio 跑 windows-latest CI 或 VM session-1 hop。每步 tsc+vitest 绿；探针改动不破 f40-suite 既有断言。

---

# ★综合测试主计划（2026-07-26 用户定「都搞·自动全做·第三方测·全边界」）

## 三条硬要求（本轮驱动）
1. **planned-build 自动全做**：一个功能走 B→C→D→E→F，全部做完 Phase G，遇硬阻塞才停回问。
2. **第三方测（非主线程一直自测）**：每个功能的**建测试+实跑**委托独立 worktree agent；**主线程只做编排 + 独立复核 + 集成**（审 diff、回盘看真结果、ff 并入、D/E/F 签收）。这满足"独立验证"——建者与验者分离。
3. **全边界**：每功能列**边界矩阵**，逐条覆盖（不只 happy-path）。

## 委托可靠性纪律（从两次 agent 夭折学的）
两次 F-Vwin 委托死于**外因**（①账号 session limit；②sandbox flag `cd`+管道），非任务本身。故每个委托 agent prompt 必含：
- **STEP 0 先 `git merge --ff-only account-ux`**（worktree 起点是 stale 祖先，缺计划文件+当前源）→ 确认能读到 `features/<NN>.md` 再动手。
- **禁 `cd`+管道**：用绝对路径 + `--manifest-path` + 结果重定向到文件（sandbox 会拦 cd+pipe）。
- **VM 测试走 session-1 hop**（`schtasks /it`）：session-0 建不出可见窗口 / 某些 GUI 不可驱；compile 可在 session-0 `--no-run` 先查错。
- **撞 rate-limit / API-limit → 立即停+如实报，别 thrash**。
- **如实回报真结果**（对齐仓库反伪造纪律）：贴 `test result:` 原行，不自报绿。
- 红线全含（daemon 零改 / 不 push·bump / 不碰 ~/.bashrc / 无 emoji）。

## 执行模型（自动 loop）
主线程编排：`委托 agent 建+跑一个功能 → 收 → 独立复核（审 diff + 回盘核实真结果 + 门禁）→ ff 并入 account-ux → D/E/F 签收 → 委托下一个`。**串行**（共享面：fixture、tabs.ts、session-1 hop runner——并行 worktree 会撞）。全部完 → Phase G（/full-audit + 端到端 + 汇报）。

## 已完成
- **F-E0 基建** + **F-E1 gray-light 全链**（Linux 自环，daemon-frame 5/0 + 全链 3/0）✅ 并入。
- **F-Vwin #74/#41 真 Windows 验证**（bind.rs native 测试，VM session-1：1 passed + 全套 363 passed/0 failed）✅ 并入 9ac0615。

## ★功能边界矩阵（本轮待委托实现的）
### F-E2 resume idle 就地复用（#75/#76）— Linux 自环 fixture
| 边界 | 断言 |
|---|---|
| 远端 archived + idle-tmux(灰) → Resume（tmux）| `mode=idle-reuse name=cc-<sid8>`（**无 `-N` 孤儿**，治 #76）+ argv.log + 复活清灰 + `tmux has-session` 孤儿数=0 |
| 远端 archived（无 tmux）→ Resume（直连）| 新 session + argv 正确 CLAUDE_CONFIG_DIR |
| 带账号 pin「用账号 X resume」| argv `CLAUDE_CONFIG_DIR` = X 的目录 |
| 不带 pin | 默认全局当前账号目录（治 #75 主因） |
| 本地 archived Resume | 复活 archived→live，`[e2e] tab-state` |
| 重复 resume / tmux 已消失 / 会话仍 live | 幂等/回退/守卫不误动 |

### F-E3 换号重启编排（#68/#69）— Linux 自环
| 边界 | 断言 |
|---|---|
| restart 账号 X + compactFirst | jsonl compact 记录驱动 compact→exit→kill→resume 序列 + argv 新 `CLAUDE_CONFIG_DIR` |
| restart 无 compact | 直接 exit→kill→resume |
| 检测到 mismatch（账号 chip ⚠k）| restart 后 mismatch 清零 |
| 批量对齐 alignAll：idle vs busy 会话 | 可注入 confirm 分别处理，busy 走确认 |
| 取消 confirm | no-op 不动 |

### F-E4 孤儿清理（F05）— Linux 自环，**前置=可注入 confirm seam（唯一动生产码，行为等价）**
| 边界 | 断言 |
|---|---|
| 无 tab 的 `cc-*` tmux | scan 计入 + cleanup 真删（`has-session` 消失）|
| 非 `cc-*` 用户会话 | **不误伤**（存活）|
| `<project>_cc`（cc-bus 资产）| **不误伤** |
| confirm 接受 vs 拒绝 | 接受才删；拒绝 no-op |
| 零孤儿 / 混合 | 计数准确、no-op |

### F-E5 Tier2 Windows DOM（session-1 hop，wdio）
| 边界 | 断言 |
|---|---|
| 裸壳（无 fixture）| `#app`/`#tab-bar`/`#status-bar` 存在；`.status-msg`("等待活跃")/`.status-count`("活跃 0")/`.empty-state`；6 顶栏钮+`.status-cmdk` 可点 |
| overlay 快捷键（物理码）| `KeyH`→历史、`KeyG`→全景、`Ctrl+KeyK`→命令栏、`KeyT`→Tasks 各开；`Escape` 关 |
| 会话相关（E5b，VM app 连 aya + F-E1 fixture）| `.tab`/`.tab-title`/`.live-dot` 状态类；右键 `.tab-context-menu` 项随状态；archived 有 resume 项；`.status-account` chip |
| confirm 阻塞 | spec 内 `window.confirm` 桩掉（不动生产码）|

### F-E1 灰灯边界补（搭车，已有基座）
| 边界 | 断言 |
|---|---|
| 多会话各自独立变灰 | 每 sid 独立 `[e2e] tab-state tmuxIdle` |
| 空 backend 最后会话卡灰（已知残留）| 断言这个**已知行为**（daemon 哨兵，红线外）|
| 复活只清对应 tab 的灰 | 不误清别的 |

## 不做（Tier3 手动 / 边界外）
↗ `SetForegroundWindow` 真拉前（前台锁，肉眼/`GetForegroundWindow==hwnd` smoke）· 真终端/PowerShell/SFTP/系统通知 · 真 claude 真出内容（hard-to-fixture）。灰灯 WebView2 渲染看一眼并进 F-E5。

## 执行序（ROI）
F-E4 confirm seam（前置，其余搭 seam）→ F-E2 resume → F-E3 换号 → F-E1 灰灯边界补 → F-E5 Tier2（含 E5b）→ Phase G。
