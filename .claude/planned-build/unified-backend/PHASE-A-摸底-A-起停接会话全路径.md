# Phase A 摸底 A — 今天「起 / 停 / 接会话」的全部路径（2026-08-01）

> 四份摸底之一。**这份是「要收编的东西一共有几条」的枚举底账。**
> 标注 ✅ = 我（主线程）亲自复核过代码位置；其余为摸底 agent 所报，未逐条复核。

## 0. 一句话

**19 条路，没有一条经 daemon。** 命令串由 TS 产的 5 条、由 Rust 产的 6 条、由 bash 自己产的 3 条、
三层嵌套产的 1 条、只产计划不执行的 1 条、纯观测的 1 条。

## 1. 路径表

| # | 路 | 入口 | 命令由谁产 | 备注 |
|---|---|---|---|---|
| 1 | 远端 resume（直连无容器） | `remote-launch-run.ts:95` | **TS**（IR 双渲染器） | |
| 2 | 远端 resume（tmux create-or-attach） | `remote-launch-run.ts:129` | **TS** | `session-backend.ts:102` 是唯一 tmux 字面量座 |
| 3 | 远端 resume（send-into 既有 idle tmux） | `remote-launch-run.ts:158` | **TS，恒兜底** | **`ccm` 表达不了的唯一容器形态**；`launch-render-cli.ts:81` 硬性 `ok:false`（§33「诚实放弃」） |
| 4 | 远端 attach | `remote-launch-run.ts:229` | TS | attach 在维度循环**之前** return，不带账号（有意） |
| 5 | 远端开新会话 | `remote-launch-run.ts:204` | TS | `ccmSid: undefined`（`launch-requests.ts:130` 明写 F04 缺口）⇒ 无 `@ccm_sid`、无 rbind 标题 = issue #72 |
| 6 | 本机 Windows resume / 新会话 | `history.rs:876` / `:1243` | **Rust**，一行 IR 不读 | `Get-Command cc` 是目标机现场探测 |
| 7 | 本机 POSIX resume / 新会话 | 同上 → `launch.rs:126` | **Rust** | ⚠ 见 §2-② |
| 8 | 用户敲 `ccm`/`cc`/`cch`/`cct` | `shared/ccm:1` | **bash 自己** | 完全不经 monitor、不经 daemon |
| 9 | 杀远端会话 | `tmux.rs:381` | Rust | 一次性 ssh；三道门折成**一条**原子远端串 |
| 10 | send-keys（`/compact`/`Escape`/`/exit`） | `tmux.rs:440` | Rust | `tmux.rs:430` 头注逐字「走一次性 ssh、**daemon 不参与**（守只读边界）」 |
| 11 | 换号重启（编排） | `account-restart.ts:58` | Rust×3 + TS×1 | 三条独立 ssh + 一次拉起 |
| 12 | cc-bus spawn（GUI） | `cc_bus.rs:436` | **三层**：Rust→cc-spawn(bash)→ccm(bash)→tmux | |
| 13 | 终端里敲 `cc-spawn` | — | bash | 同 12 下半段 |
| 14 | **用量探针（会真起 claude）** | `account_usage.rs:199` | ~~载荷 TS + 编排 Rust~~ ⇒ **U8c-2a 起：载荷 Rust（`launch_core`）+ 编排 Rust** | **独立的第 N 套「建 tmux + send-keys + kill」**，不经 ccm、不经 `SESSION_BACKEND` |
| 15 | SFTP「在此打开终端」 | `sftp/panel.ts:556` | TS | 不起 agent，`INVENTORY §A #7` 有意在范围外 |
| 16 | 设置→账号 `cc-acct-iso` step（含 `/login`） | `accounts-section.ts:187` | TS | 账号是主语不是修饰，有意不收编 |
| 17 | daemon `--resolve` | `resolve_query.rs:109` | **只产计划不执行** | **cc-monitor 侧零消费者**（全仓 `.rs`/`.ts` grep 0 命中） |
| 18 | daemon 自起的 tmux 子进程 | `watcher.rs:416/450`、`tmux_hook.rs:103` | — | **纯观测**；唯一改 server 状态的是装 hook 槽位 |
| 19 | 分叉 fork 起会话 | `fork-start.ts:83` | 转 6/7 或 1/2 | 本机这条**不进 tmux**（有意） |

**共同远端出口**：路 1–5 + 15 + 16 全部汇到 `launch_remote_terminal`（`launch.rs:289`）
→ `build_remote_ssh_ps_command`（`:163`）→ `launch_powershell_window`（`:223`）。

## 2. 三条被抓出来的现存缺陷

### ① ✅ **Linux 宿主上，远端终端拉起 100% 失败**

`launch_remote_terminal`（`launch.rs:289`）**无条件**调 `launch_powershell_window`（`:304`），
而后者的 `#[cfg(not(windows))]` 分支（`launch.rs:268-271`）直接
`Err("拉起终端窗口仅支持 Windows（v1）")`。
⇒ 路 1–5 + 15 + 16 在 Linux 上全部降级到剪贴板（`remote-launch-run.ts:68` `invokeLaunchOrCopyFallback`）。

**我亲自读了 `launch.rs:260-307` 确认。** 这与 §40 的叙事「远端是 `ssh -t host -- …`，本地就是把 `ssh`
那一跳**去掉**」已经脱节 —— 「去掉 ssh」（`launch_local_posix`，`launch.rs:126`）做了，
**「在 Linux 上也能加上 ssh」没做**。`build_local_posix_argv` 只被 `history.rs:1196` 的本地分支消费。

### ② Linux 本地 resume 默认起的是一个**无 TTY** 的 claude（推测，未实跑）

`launch.rs:117-121` 头注假设命令里有 `ccm --tmux`（「会话容器本来就是 tmux，`ccm --tmux` 自己会建」），
但 `build_local_posix_command`（`history.rs:1157`）**从不加 `--tmux`**，默认 launcher 是空串
（`behavior.ts:66`）。而 `launch_local_posix` 把 stdin/stdout/stderr 全设 null（`launch.rs:137-141`）。
⇒ 默认路径 = 无 TTY、stdio 全 null 的裸 `claude --resume`。**标为未实跑验证的推测**，但代码路径确定。

### ③ ✅ **daemon `--resolve` 发的会话名与 monitor 现实已漂移**

- daemon：`resolve_query.rs:251` `session_name_for` → **`cc-<sid8>`** / `cx-<sid8>`
- monitor：`remote-launch.ts:157` `pickFreshTmuxName` → **`<sid8>-cc>`**（S4b-3b，用户 2026-07-31 反转）

**我亲自读了两处确认。** `resolve_query.rs:85-87` 自陈 sessionName「纯派生不是探测」，
但**外部消费方 aterm 拿它去 attach 会 attach 到一个不存在的名字**。两仓 lockstep 问题。

## 3. tmux 会话名生成器 —— **五个，算法互不相同**

| 生产者 | 规则 |
|---|---|
| `shared/ccm:261` | `sed 's/[^A-Za-z0-9_-]/-/g'` + **`-cc` 后缀** |
| `cc-spawn:81` | `tr -c 'A-Za-z0-9_-' '_'` + **`_cc` 后缀**（**不同算法**） |
| `remote-launch.ts:157` | `<sid8>-cc`，撞名 `-2` |
| `launch-requests.ts:67` | `cc-<sid8>`（遗留默认，生产路径走不到） |
| `resolve_query.rs:251` | `cc-<sid8>` / `cx-<sid8>` |

消费侧 `tmux.rs:482 is_ccm_tmux_name` 被迫同时认 `cc-*` 与 `<X>-cc[-N]` 两族。
**这是「唯一后端」最直接的收益点。**

## 4. 跨语言双写点账本（13 处，⚠ = 无真守卫）

| # | 双写点 | 守卫 |
|---|---|---|
| 1 | `derive_tmux_name` ↔ `deriveTmuxName` | `e2e/ccm-cli.test.sh:199` 真值对拍（`npx tsx` 现跑 TS 实现比对） |
| 2 | tmux `-t` 精确目标 `=<名>:`（**四处同源**） | `sftp.rs:1097-1160` **结构性扫描** + `pin_definition` |
| 3 | ⚠ **agent 画像（三处）**：`ccm:116` · `agent-profile.ts:10` · `adapter/claude_code.rs:20` | `agent-profile-parity.vitest.ts` **只守 Rust↔TS 两侧**；ccm 那份只有一句注释 —— **未登记的第三侧** |
| 4 | ccm 能力集字符串（ccm ↔ `CLI_REQUIRED_CAPS` ↔ cc-spawn） | 运行期协商掩盖静态漂移（半守） |
| 5 | ⚠ POSIX 单引号 quote（**四处**：`ccm:103` / `shell-quote.ts` / `launch.rs:30` / `ssh_source::shell_quote`） | **无统一守卫** |
| 6 | 「基座 = `unset` 而非什么都不加」 | `base-flag-contract-guard.vitest.ts` |
| 7 | ⚠ **tmux 会话名生成规则（五处，见 §3）** | 只有消费侧被迫兼容 |
| 8 | `TMUX_LS_FMT` 六列 + `NO_TMUX` + observation token | 有守卫（双写点测试） |
| 9 | `RemovalCause::Superseded` 线上名 | `ssh_source.rs:3114` |
| 10 | 用量口径 | 注释纪律 + golden 夹具（半守） |
| 11 | `cc-acct-iso shellinit` 围栏串 | 有守卫 |
| 12 | ⚠ cc-bus id 校验判据（`cc_bus.rs:20` 自称「照抄 `shared/ccm:358-362`」） | **引用行号已失效**：那里今天是 attach 分支，真判据在 `ccm:395-399` |
| 13 | 预信任逻辑 | **B02 已消除** |

## 5. 「起会话」今天走的是**第三条传输**

既不是 daemon 流（russh 长连接），也不是 monitor 的 russh 一次性 exec，而是：

- 远端：拼一条 PowerShell 串 `& ssh -t … -- 'bash -lic ''<载荷>'''`（**系统 `ssh` 二进制**）
  → `-EncodedCommand` 穿进 `wt.exe`（`launch.rs:231-237`）。
- POSIX 本地：`bash -lic <cmd>` + `process_group(0)` + stdio 全 null（`launch.rs:126-150`）。
- Windows 本地：`build_local_ps_command`（`history.rs:930`），**不引用任何 IR 类型**。

⇒ 「daemon 变执行者」= **把这第三条传输整个搬进 daemon**，不是给已有通道加个动词。

## 6. 「用哪个二进制起」今天有 **7 个来源**

`remote-config.ts:254 resolveResumeCommand`（per-host 覆盖）· `behavior.resumeCommandRemote` ·
`behavior.resumeCommandLocal` · `--launcher` · `AGENT_PROFILE.defaultLauncher` ·
`adapter.default_launcher()` · `CCSPAWN_LAUNCH`。
`launcher-diagnostics.ts:16` 已经在用启发式提醒「这条命令似乎绕开了 ccm」——**说明这一族已在制造真实困惑**。

## 7. 一条不能合并的分叉（合了会当场回归）

`ccm:264-279`：显式 `--tmux=<名>`（monitor 恒传）→ **不避让、幂等接回**；
无名 `--tmux`（用户敲 `cct`）→ **无条件新建、撞名退 `-2/-3`**。
2026-07-31 用户实测报障后才改成这样。合并 ⇒ 回归「新开终端进同一目录敲 `cct`
被 attach 进别人正在用的窗口」。

## 8. 一个隐藏的 agent×后端耦合

`ccm:602-606`：**只对 codex** 从 `tmux display-message -p '#S'` 派生 `CC_BUS_ID`，
理由是 codex 的 Landlock/seccomp 沙箱够不着 tmux socket（`ccm:128-133`）。
「必须无条件覆盖、不能写 `${CC_BUS_ID:-…}`」（`ccm:596-601`）是身份冒用 + 抢信的防线，
搬进 Rust 时极易丢。
