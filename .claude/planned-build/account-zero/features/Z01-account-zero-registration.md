# Z01 — 账号 0 登记 + 可见

> 主计划：`../MASTERPLAN.md` §0.1「账号 0 的定义」· §1 Z01 · §3 账本第 1/2/6/8 行
> 前置：**G-B**（`0b297ed`，两道网）· **Z06**（`941e13d`，`NATIVE_IDENTITY` 声明——本轮的判据直接取自它的 `root` 字段）· **Z07**（`e6674d3`，它给 Z01 留的硬提醒就是本轮第一件事）

## 1. 账号 0 是什么（这条定义不许动）

**账号 0 ≡「不设 `CLAUDE_CONFIG_DIR`」这个状态本身。** 凭据在共享库 `~/.claude/.credentials.json`、
状态在 `$HOME/.claude.json`、起它 = **什么都不设**。

它此前是**不可见的幽灵**：manifest 不认、cc-monitor 看不到、用量查不到，但它确实存在
（本机实况 `~/.claude.json` 694 字节，07-28 出现于迁移之后 = 有人裸起过 claude）。

本轮的立论是 **吸收 > 检测 > 禁止**：把它登记成一个正常账号，而不是判它违规。

**为什么不能给它 `configDir = ~/.claude`**（主计划 §0.1 已论证，本轮再确认一次）：那样起它就有两条路
（设与不设），同一个账号会分裂出两份 `.claude.json`（`$HOME` 一份、共享库一份）。

**空串更不行**：`env CLAUDE_CONFIG_DIR="" claude` 设出的是**空值**，空值 ≠ 未设。
⇒ manifest 里 `configDir` 这个**键整个省略**；Rust 侧 `Option<String>`；TS 侧 `string | null`；
所有拼命令的地方都不许 `|| ""`。**本文件里出现四次这个约束，因为它是全部设计的支点。**

## 2. 开工第一件事：Z07 留的硬约束，查出来它的**理由是错的**

Z07 的 D1b 是「secret 项出现在共享库里 ⇒ **vfail**」，理由写的是
「共享集是 `@auto` ⇒ 它会被**自动 symlink 给每个账号** = 静默串号」。

**读源码 + 沙盒实测，这句话不成立**：`share_items()` 的**两个分支**都有
`is_isolate "$base" && continue`，而 secret **全都在隔离集里** ⇒ 隔离项**从不**被 symlink 出去。
沙盒复现：往共享库放回一个 `.credentials.json` 再 `sync --apply`，
**z 保持自己的私有实体（内容不是共享库那份）、b 根本没有、零软链、不串号。**

（D2 那条 600 启发式的**同款说辞仍然成立**——未声明项不在隔离集里，确实会被 symlink。
Z07 是把 D2 的推理整段照搬给了 D1b，而前提没照搬得过去。已回写 Z07 文档 §8 + 主计划 + BACKLOG + README。）

### 2.1 判据换成声明里的 `root` 字段

「是不是 secret」不是正确的判据；**「共享库是不是这个项的原生位置」**才是。而这个答案
Z06 的声明表里本来就有——`root` 字段：

| 项 | root | 共享库里有它 = | Z01 后的档 |
|---|---|---|---|
| `.credentials.json` | `cfg` | 共享库**就是**账号 0 的 config dir ⇒ 它是账号 0 的凭据 | **`ok`「账号 0 已登录」** |
| `.claude.json` | `home` | 账号 0 的住 `$HOME`、隔离账号的住各自 config dir ⇒ **不是任何账号的原生位置**，是迁移残留（且含 `oauthAccount`） | **仍 `vfail`**（混合模式下存在 in-place 账号时 `skip`——那时共享库确实是它的原生位置） |

这不是"给账号 0 开个例外"，是**判据本身之前就选错了**。新加的投影 `ni_is_home_rooted()` 与
`ni_secrets()` 同源，`verify` 与 `acct_zero_logged()` 共用它 ⇒ 两处不会漂。

### 2.2 顺带核实：`sync` 不会把账号 0 登出

一旦承认账号 0 合法，立刻要问：`sync --apply` 的 `ISOLATE` 会不会把
`~/.claude/.credentials.json` 搬走（= 账号 0 被登出 + 内容复制给 z/b = 真串号）？

**读代码确认不会**，两道保险：
1. `_exec_op ISOLATE` 是 **copy-only**：它摘掉的是**账号侧的软链**，共享库那份**保留作模板**；
2. `cmd_add` 显式 `case "$_iso" in .credentials.json|.claude.json) continue`；
   `cmd_sync` 的 ISOLATE 只在**账号已经有一条指向共享库的软链**时才触发。

## 3. 做了什么

### 3.1 bash 侧（上游 `~/.claude/skills/cc-acct-iso/`，再 lockstep 同步）

- `ACCOUNT_ZERO_NAME="0"` · `acct_zero_logged()`（**谓词**）· `acct_zero_logged_json()` ·
  `acct_zero_email()`（现读 `$LEGACY_HOME_DIR/.claude.json`）· `acct_zero_json(with_live)`
- 账号 0 **在输出时合成，刻意不进 `MF`**：`MF` 的每条都被 `sync`/`verify`/`mf_set_default`
  当作「有真目录、可 chmod、可建软链」来用，塞进去会四处炸
- **读回时静默过滤**（`manifest_load`）：它是写时合成的，不是坏数据 ⇒ **不 warn**
- **追加在数组末尾**：放首位会让所有按下标取账号的地方整体错位（初版放首位，既有套件当场红 15 条）
- `verify` 改判 + `list`（表 + `--json`）+ `manifest_render` 都认它
- `"0"` 设为**保留名**（`add 0` 拒）· `rm 0` 给出说得清的拒绝理由 ·
  `run 0` = `exec env -u CLAUDE_CONFIG_DIR`（**`-u` 不是 `=""`**；调用者环境里很可能已经有这个变量）
- `which`（未设时）从「裸起模式 + rc=1」改成「**账号 0** + 如实报登录态 + **rc=0**」
- `shellinit` 多产一行 `0cc() { env -u CLAUDE_CONFIG_DIR command claude "$@"; }`
  —— 那句 `export CLAUDE_CONFIG_DIR=<默认>` 是**全局**的，不给逃生口就再也回不到账号 0

### 3.2 daemon（`accounts_query.rs`）

- `RawAccount.config_dir: Option<String>`：**结构性**判据（键在不在），**不认名字**——Rust 里不硬编码 `"0"`
- `accounts-meta`：账号 0 出 `configDir: null` · `exists: true` · `loggedIn` 探 `sharedStore/.credentials.json`
  （`sharedStore` 缺失 ⇒ `false` = 「不知道」，**不假装已登录**）
- **`--session-accounts`：裸起会话现在归属账号 0**（此前 `account: null` + `bare: true`）。
  归属表的 key 是 `Option<String>`，`None` 那条就是账号 0 ⇒ 仍然是「归属来自 manifest」，
  有反向用例钉住（manifest 里没有账号 0 时，裸起会话仍 `account: null`）
- **新动词 `--account-trust-zero <cwd>`**：账号 0 的 `.claude.json` 在 `$HOME`（声明里 root=home），
  路径**写死在代码里、不收路径参数** ⇒ 它连「任意文件读」的面都没有。
  `--account-trust` 与它共用 `trust_of_claude_json()`（避免第二份实现）
- 能力标记 `"accountZeroAware": true`：**旧 daemon 不会出这个键** ⇒ 消费侧能**明说**降级

### 3.3 cc-monitor

- `RemoteAccount.config_dir: Option<String>` · `AccountsMeta.account_zero_aware` ·
  `AccountsResult.notice`
- **`degraded_notice()`：绝不静默降级**，且两种旧法分开说（用户要做的事不一样）：
  - 旧 **daemon**（无能力标记）⇒「更新远端 daemon」
  - 旧 **cc-acct-iso**（有标记但列表里没有 configDir 缺席的那条）⇒「跑一次 `sync --apply`」
  - `enabled:false` ⇒ **不出噪音**
- `check_account_trust(config_dir: Option<String>)` → `trust_args()` 纯函数分流（拼命令行是注入面，必须能直接断言）
- TS：`Account.configDir: string | null` · `isAccountZero()`（结构性，不认名字）·
  `deriveUi` 透传 notice · 设置里的账号表把路径列渲染成「（不设 `CLAUDE_CONFIG_DIR`）」·
  用量单元格**明说**「账号 0 暂不支持」而不是留白或发一次注定失败的探测 · 降级说明渲染成显眼的一条

### 3.4 刻意**不做**的一件事：账号 0 暂不可选

`isSelectable()` 要求 `mode === "isolated"` ⇒ 账号 0（`mode:"bare"`）**天然落选**，这正是当前想要的：

> ~~从 UI 起它需要「显式 unset `CLAUDE_CONFIG_DIR`」这条注入路径，而 `launch-plan` 今天只会 `export`。~~

### ★ 3.4bis 这段理由**是错的，Z02 已订正**

**unset 的注入形态早就有了**，两条渲染路各一份：

| 渲染路 | 怎么 unset |
|---|---|
| CLI | `ACCOUNT_DIMENSION.cliFlags` 对非 `account` 态吐 `--base`；`shared/ccm` 收到会 `unset CLAUDE_CONFIG_DIR`（**两处落点**：载荷行 `:572` + 会话级 env `:598`） |
| 兜底 | `ENV_RESET_DIMENSION` 推 `{kind:"unset-config-dir"}` op |

**我错在哪**：只查了 `ACCOUNT_DIMENSION.apply`（那里确实只 push `export-config-dir`），
**没往下查 `cliFlags` 与 `ccm`**。与 Z07 把 D2 的推理照搬给 D1b 同类：
**一个层面看到的事实被当成了整条链路的事实。**

**真正卡住的是选择链路**：① `accountConfigDir()` 返回 null ⇒ `resolveAccount` 说不出
「显式选了账号 0」 ② 菜单没有这个选项 ③ **`tabs.ts:2283`** 那个三元加变体**不报编译错**地走错分支。
只有 ③ 卡红线。⇒ 详见 `Z02-PARTIAL.md`；`isSelectable()` 上方的注释已改写。

那条 `--base` 契约现已由 `src/base-flag-contract-guard.vitest.ts` 钉住（Z02 交付）。

## 4. 三个坑（都被门禁当场接住）

1. **`chk 'cmd | grep -q'` 在 `pipefail` 下恒红**：套件开了 `set -uo pipefail`，
   左侧 `die` 非零 ⇒ **整条管线判失败**，`grep` 命中了也报红。改成落文件再 grep。
   （Z06 栽的是 `while … cond && printf`，这是同一个坑的**新装扮**——第三次了。）
2. **`acct_zero_logged` 初版 `printf 'true'/'false'`**：于是 `if acct_zero_logged; then` **恒真**，
   而且那个 `false` 字符串被漏进了 `which` 和 `run` 的 stdout（实测输出 `0\nfalse未设…` / `falseCFG=<unset>`）。
   改成**纯谓词**，要字符串的另给 `_json` 版；加断言钉「它不往 stdout 吐东西」。
3. **`init --apply` 会把凭据搬进账号 d** ⇒ 想验「账号 0 已登录」必须在沙盒里**显式造**一份共享库凭据
   （这恰恰就是真实场景：全迁之后有人没设 `CLAUDE_CONFIG_DIR` 又起了一次 claude）。

另外：**Rust 侧那条 Z06 双写点守卫在本轮红了一次，且红得对**——我把
`dir.join(".credentials.json").exists()` 改名成了 `probe_dir…`，守卫锚点当场失配。
锚点已放宽到 `.join(".credentials.json").exists()`（仍然钉住文件名，不钉变量名）。

## 5. 变异验收（Phase D）

**如实标注**：planned-build 铁律 8 要求并行多 agent 审计；本会话有常驻指令「除非用户要求不开 agent」
⇒ 主线程变异 + 全门禁 + 沙盒验收代替。**这是欠了铁律 8 的账，不是强度裁剪。**
每条变异先确认**编译/解析通过**（`bash -n` / `cargo build` / `tsc`），否则判色无效。

| 变异 | 做了什么 | 结果 |
|---|---|---|
| **A** | bash：`acct_zero_json` 输出 `"configDir": ""` | **成立**：红 3 条（没有 configDir 键 / manifest 不出现空串 / `list --json` 同上） |
| **B** | daemon：`config_dir.as_deref().filter(\|c\| !c.is_empty())`（把空串也当账号 0） | **成立**：红 `empty_config_dir_is_not_account_zero` |
| **C** | monitor：`degraded_notice` 对旧 daemon 恒返回 `None` | **成立**：红 `old_daemon_gets_an_explicit_notice` |
| **D** | TS：`isAccountZero` 改成 `a.name === "0"`（认名字而不是认结构） | **成立**：红「判据是结构性的，不认名字」 |
| **E** | TS：路径列回落成 `a.configDir ?? ""`（账号 0 那行留白） | **成立**：红「路径列说的是它的真实含义」 |

五条全部成立，逐条回盘后全绿。

## 6. 工程审计（Phase E）

### 6.1 账本对账

| 账本行 | 本功能做了什么 | 状态 |
|---|---|---|
| **1 账号 0 进 manifest** | 写时合成、追加末尾、`configDir` 键省略、`mode:"bare"`、读回静默过滤 | ✅ |
| **2 `list-accounts` 报出来** | 人表 + `--json`（带 `exists`/`loggedIn`）+ daemon 帧 + 设置里的表 | ✅ |
| **6 `verify` 从违规改判为状态** | 判据换成声明的 `root` 字段（顺带订正 Z07 的错误理由） | ✅ |
| **8 cc-monitor 列表多一行** | 是；但**不可选**（§3.4 论证），且这条限制在代码里写死了理由 | ⚠ 部分，如实记 |
| **7 lockstep** | 6 文件同步 + `.vendor_id` `3416ab2260e55d74` → **`bf3d3a798d095162`**；逐字 `diff -q` 一致；`build.rs` 不 warn | ✅ |
| **9 原生身份声明** | 消费它：新增 `ni_is_home_rooted()` 投影，`verify` 与 `acct_zero_logged` 同源 | ✅ 复用 + 一处扩充 |

### 6.2 门禁数字

| 门禁 | 前 | 后 |
|---|---|---|
| vendored `run-tests.sh` | 231 | **268**（+37；脚本内 `MIN_ASSERTS` 与 `ci.yml` 双保险同步上调） |
| shellcheck | 36 文件零告警 | 36 文件零告警（地板不变） |
| daemon `cargo test` | 141 | **149** |
| monitor `cargo test --all` | 611 | **618** |
| vitest | 837 / 56 files | **847** / 56 files |
| tsc · eslint · fmt · clippy | 0 · 7 基线 · clean · — | 0 · **7 基线不变** · clean · **clippy 增量 0**（lib 36 / lib+tests 50，与 HEAD 逐一对拍） |
| 生成物 / check:types | 67 | 67 |

### 6.3 用户真实数据

- `~/.claude-accts/accounts.json`：mtime **2026-07-26 17:41:02**、size **413** —— **未变**
- z/b 的 `settings.json`：仍是 07-26 那两个软链 —— **未变**
- `~/.claude/.credentials.json` / `~/.claude/.claude.json`：**都不存在**（⇒ 本机账号 0 当前未登录，
  且没有 home 根 secret 泄漏进共享库）
- `~/.claude.json`（694 B，账号 0 的状态文件）：**只读观察，未改**
- ⚠ **如实记**：`~/.claude-accts/b/` 的 `.claude.json`/`.credentials.json`/`backups` mtime 是本轮期间的 11:01
  —— 那是**用户另一个正在跑的 claude 实例**（token 刷新 + 状态写入 + 备份轮转），**不是本轮改动**。
  本轮所有 bash 执行都在 `HOME=$SB` 沙盒里；红线三项逐条核过。
- 用户实况 daemon 进程（`~/.cc-monitor/bin/`，已跑 4h）**未碰**

### 6.4 本轮没有做的

- **账号 0 不可选**（§3.4）——放开要 unset 注入形态，动 `launch-plan` + `tabs.ts`（红线）
- 账号 0 的**用量查询**（要起隐藏会话，同上）——UI 上已**明说**不支持，不是留白
- 上游备份 `scratchpad/z01-upstream-backup/`；变异备份 `scratchpad/mut{A..E}.bak`

## 7. 给后续的登记项

| 项 | 内容 | 归属 |
|---|---|---|
| **Z01-follow**〔**Z02 已订正**〕 | 账号 0 可选。~~需要 `unset-config-dir` 注入形态~~ —— **那个早就有**（见 §3.4bis）。真正要做的是**选择链路**：`resolveAccount` 增 `account0` 出口 · 菜单加选项 · **`tabs.ts:2283` 三元改穷尽判别（必须最先做）** | `tabs.ts` 红线松开后，按 `Z02-PARTIAL.md` §6 七步 |
| **Z01-usage** | 账号 0 的用量查询（依赖上一条） | 同上 |
| **Z07-补** | `verify` 对 `.claude.json` 的致命判据仍在，但**混合模式下会 skip**——in-place 逃生口本身该不该保留是另一个问题 | 已在 Z07 文档记 |

## 8. 签收

- [x] 通过代码审计（五条变异全部成立 + 既有 231/141/611/837 四套先验不破 + G-B 两道网 + 全门禁）
- [x] 通过工程审计（账本 1/2/6/7/9 到位，第 8 行**如实记为部分**并写清了为什么不能硬上）
- [x] **订正了 Z07 的一句事实错误**，四份文档同步（features/Z07 §8 · MASTERPLAN · BACKLOG E37 · README 6d）
- [x] 用户真实数据零改动（逐条核过，含一条「不是我改的」的如实说明）
