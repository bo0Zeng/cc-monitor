# Phase A 计划自审 — 四视角（2026-08-01）

> 用户「全面审计计划」的产物。四个 read-only agent 并行审 `MASTERPLAN.md` v2 本身：
> 技术可行性 / 完整性覆盖面 / 文档工程 / 代码工程。
> **结论已折进 v3；本文只留 v3 里放不下的证据与清单。**
> 标注 ✅ = 主线程亲自复核。

## 0. 最重要的一句

**四份里有三份独立命中同一条：v2 的 U0 前提是错的**（五套 `ccm-*` e2e 早就有地板）。
独立收敛 ⇒ 那不是某个 agent 的误判，是我的事实错误。

## 1. 三个「安慰剂」—— v2 里看起来在守、实际什么都没守的机制

### 1.1 ✅ U0：地板早就全都有

`ci.yml:370-406` 十五条 `run: bash e2e/assert-pass-floor.sh`，五套 `ccm-*` 全在内
（`ccm-cli 44` / `ccm-print-parity 12` / `ccm-acceptance 19` / `ccm-pretrust 13` / `ccm-rbind-title 8`）。
`ci.yml:310-326` 还有一道**元门禁**：`grep -c` 调用行 `>= 15` + 逐对比对 15 个「套件名 地板值」字面。
`assert-pass-floor.sh` 是**运行期** PASS 数地板且 fail-closed（抓不到「合计 PASS=n」也判红）。

**「`ccm-rbind-title` 实测 6」也是错的，实测 8。** 「6」可复现来源：`grep -c '\bok\b'` ——
把 helper 定义行算一次、把 `:52` 那个 4 元 `for clobber` 循环算一次。
同类误差在 `ccm-cli` 更大：静态 34 个直接点，`:190`（helper 被调 5 次）+ `:214`（5 元循环）各展开 5 ⇒ 44。
**静态口径系统性低估。** 按 6 去「核清」= 把地板松 2 条。

⚠ **不用跑就能判**：CI 真跑这套并卡地板 8 且一直绿 ⇒ 运行期 PASS 必然 ≥ 8。

### 1.2 ✅ U2 的 cfg 位置机检抓不到它自己引的那个 bug

v2 说「`pidfd_open` 裸在业务代码里，正是因为没有平台线」，并提机检「`platform/` 之外出现平台 cfg 就红」。
**`pidfd_open` 根本没有 cfg** —— 它是无条件编译的 Linux-only 代码。11 个错**一个 cfg 都不涉及**。

实测补充：`--all-targets` 是 **12 个错**（多 `watcher.rs:2392 libc::getuid`，测试段）
⇒ **U4 的 DoD 必须写 `--all-targets`**，否则「编得过」只覆盖 bin，CI 的 `cargo test` 在 Windows 上照样编不过。

另：全 crate 平台 cfg **42 处、生产段 23 处**；`watcher.rs:1613` 是 `#[cfg_attr(not(target_os="linux"), allow(dead_code))]`
⇒ 朴素 `grep '#\[cfg('` 会漏，机检要连 `cfg_attr` 一起认。

### 1.3 ✅ S11 对 `ccm_cli_has_required_elements` 的诊断是反的

它**已经有**计数自检：`sftp.rs:1180-1182 report.require(10, …)`，而 `structural_scan::require`
（`structural_scan.rs:42-65`）对 `min_checked == 0` **硬失败**、对 `checked < min_checked` 报「扫描器可能失效了」；
另有 `sftp.rs:1176 pin_definition(EXACT_T_DEF, "t=", …)`。
ccm 掏空后是**四路一起红**（11 个 needle · 两条通道 A 字面量 · `pin_definition` · `require(10)`），不是空转变绿。

⇒ 真风险是**重写时把强度降下来**。U1a 的正确做法是**先把强度基线记成对拍表**。

## 2. 三个当下就坏着的东西（不是计划的问题，是仓里的）

### 2.1 ✅ `no_timer_guard` 非递归 —— 而 v2 的硬门槛会替它背书

`no_timer_guard.rs:87-108` 单层 `read_dir` + `extension != "rs"` ⇒ **目录被跳过**。
`readonly_guard.rs:152-156` 里 Phase G 修同一 bug 时留的警示注释还在，**这条没跟**。

失效算术（`:116-120` 的 `files.len() >= 5`）：

| 场景 | 顶层扫到的 .rs | `>= 5` | 结果 |
|---|---|---|---|
| 今天 | 14 | 过（余量 9） | 真在扫 |
| 拆完只留 main/wire/两护栏 | 4 | **失败** | 侥幸 fail-closed，余量 1 个文件 |
| 拆完用 2018 风格（`src/observe.rs` + `src/observe/`） | **7** | **过** | **空转变绿** |

第二种是最可能的布局。此时 `every_duration_use_is_registered_as_non_timer` 会红（total=0 ≠ 1），
而**最省事的弄绿方式是删掉那条登记**（0==0 全绿，两条护栏彻底空转）。
⇒ v2 §4.4④「登记表条数不变」**只查条数，查不出扫描面归零**；红线也只禁「+1」不禁「缩范围」。

### 2.2 ✅ 三个守卫共用一个坏 marker，`main.rs` 测试段今天正被当生产段扫

八处写 `let marker = "\n#[cfg(test)]\nmod tests";`，而 `main.rs:183` 是 `mod stream_flag_tests` ⇒ 不匹配
⇒ `no_timer_guard` / `build_id_guard` / `accounts_query` 的 dispatch 守卫都在扫那 284 行测试代码。

**反向自检结构性地检不出**：`accounts_query.rs:1397` 的 `main_prod.len() < main_raw.len()`
**光靠剥注释就满足**，与测试段有没有剥掉无关。
U6/U8 给 main.rs 加子命令 + 加测试，测试里一出现 `Some("--foo")`，`build_id_guard` 就凭空多数一个子命令。

### 2.3 ✅ 内嵌 daemon 处于「半 bump」

源码 `main.rs:132` = `p1v-attachable`，`embedded-daemons/*.build_id` 两个都是 `p1u-fork-session`。
`build.rs:262-266` 只 `cargo:warning`。影响面限本地手工打包（目录 gitignore，发版 CI 会重编），
但形态正是 `release.yml:95-99` 记的 v2.19–v2.22 事故。**是我 bump 了 BUILD_ID 没 re-zigbuild 造成的。**

### 2.4 `parity_ledger::command_signatures`（`:391`）也非递归

而 `src-tauri/src/adapter/` **今天已经存在**（里面暂无 `#[tauri::command]`，所以还没炸）。U7 重度依赖它做验收面。

## 3. 结构性发现

### 3.1 ✅ `observe/ → control/` 的边今天就存在且语义必需

`watcher.rs:817-822`（`watch_loop` 的 `TmuxObserved` 臂）→ `install_tmux_hooks_best_effort`（`:269-278`）
→ `tmux_hook::install_hooks`（`tmux_hook.rs:104-121`）→ `tmux set-hook -g`。
hook 活在 server 内存里、server 每次重起要重装，而「server 起来了」**只有 observe 知道**。
⇒ v2 的「互不 import 对方内部」第一天就过不去。两种形态，U3 必须选一个：
① `control` 暴露窄接口给 observe 调（措辞要改）；② `watch_loop` 把「server 换 pid」升成上行事件、由 main/control 接管。

### 3.2 三分不够用

`projects_root` 四份逐字相同（`usage_query.rs:48` · `history_query.rs:65` · `search_query.rs:100` · `fork_write.rs:41`），
其中 `fork_write` 属 control、另三个属 observe ⇒ 公共 helper 落在两者之外。
`proc_starttime` crate 内两份（`watcher.rs:1435` vs `accounts_query.rs:311`）。`mtime_ms` 两份。

### 3.3 `session_alive` 是 platform ↔ observe 的**回边**

`watcher.rs:1632-1651` 是判活策略（observe 语义）但完全由 platform 原语构成，
而 platform 侧的 `spawn_pid_watcher:228` **反向调用它**（挡 `pidfd_open` 之前的 PID 复用）。
出路：下沉 platform（observe 就没判活策略了）或谓词参数化（改签名 → 改四条 pidfd 测试）。U2 先定。

### 3.4 tmux 观测族（~250 行）语义是 observe、手段是起子进程

`watcher.rs:416` / `:456` 各一处 `Command::new("sh")`。
⇒ 「`readonly_guard` 钉死在 `observe/`」在这里名不副实（护栏不认 `Command`）。
**v3 的裁定**：observe 的「只读」= 不改文件系统、不改用户既有数据；只读查询子进程允许且逐处登记；
任何改状态的 tmux 命令归 `control/`。

### 3.5 `watcher.rs` 拆分建议切点（自审给的符号级归属）

```
platform/linux/pidfd.rs   pidfd_open(:156) · spawn_pid_watcher(:211) · PidWatchTarget(:192)
platform/linux/proc.rs    pid_alive(:1417) · proc_starttime(:1435) · proc_cmdline(:1585)
                          · parse_btime(:1539) · start_epoch_from_ticks(:1559)
                          · parse_starttime_from_stat(:1614) ＋ accounts_query.rs:311 那份
platform/windows/…        session_map.rs:451-515 与 bind.rs:704-725 已有可搬实现
platform/paths.rs         path_key(:1763，唯一一处今天已正确 cfg 分家的)
observe/tail.rs           ReadLine · ReadCursor · read_new_lines(:976-1100)        ~125 行
observe/sessions.rs       ReaderState · SessionEntry · process_* · retire · add_time ~500 行
observe/tmux.rs           TmuxObservation 族(:360-541)                              ~250 行
observe/loop.rs           WatchEvent · DebouncerSink · spawn · watch_loop           ~330 行
observe/sink.rs           FrameSink · 路径谓词                                       ~90 行
control/tmux_hook.rs      install_hooks
```

76 条测试按此分家；13 条 `#[cfg(target_os="linux")]` 跟 platform 走，
但 `:2392 libc::getuid` 与 `:2405`/`:2530` 两处 `#[cfg(unix)]` 决定 `--all-targets` 在 Windows 上能不能过。

## 4. 覆盖缺口清单

### 4.1 19 条路里 5 条无归宿（v2）
`send-into`（`ccm` 表达不了的唯一容器形态，渲染器退役后无承接方，全文 0 次）·
**换号重启编排**（`account-restart.ts:58`，横跨起与停，全文 0 次）· SFTP 开终端 · 账号 step（含 `/login`）·
以及摸底 A §2-② 那条现存缺陷（Linux 本地 resume 起一个无 TTY、stdio 全 null 的 claude）。
⇒ v3 收进 **U8d**。

### 4.2 13 个双写点里 5 条断言不成立，且会**新增 4 个**
仍在的：agent 画像三处（消失的恰是守卫本来就没覆盖的那侧）· ccm 能力集字符串 ·
**POSIX 单引号 quote 实测 5 处不是 4 处**（多 `ssh_source.rs:1648` 与 `acct_iso_deploy.rs:81`）·
`TMUX_LS_FMT` · cc-bus id 校验判据（ccm 掏空后 `cc_bus.rs:20` 变孤本，其注释引的行号已失效）。
新增的：**ccm ↔ backend 的上下文/argv 协议（全新 bash↔Rust 契约，零守卫）** ·
monitor 手解 wire ↔ daemon `wire.rs` 的下行帧 · 本机分发链 ↔ 远端部署链 · Windows 判活临时并存。

### 4.3 20 条约束里 8 条零承接
#2 · #6（`remote_active` 唯一写者，本区**新增第五个信号源**）· #7 · #13 · #14 `kind` 排他式 ·
#15 seq 单调 · #16 幂等 · #18 data dir。另 #12/#19/#20 只在散文里。
另：摸底 C 那条「意图 vs 事实」不变量（`INVARIANTS:621-626`）**不在这 20 条里，计划也没接** —— 两张网同时漏。

### 4.4 未对账的 BACKLOG / issue / 工作区
- BACKLOG：**E40**（「L1 里 daemon 可能与 tmux 同锅，届时探针会被一起端，**不许继承 zero-poll-liveness 的结论**」——
  U5 的本机 backend 正是 L1 形态）· **E39**（`notify-debouncer-mini` 静默吞 inotify 溢出，两端都中招，「绝不补定时器」）·
  E42（`/usage` 解析从未真机验证）· E3 · E52 · E54 · E55 · E71 · E72 · E73 · E74 · E77 · E78 · E79 · E82 · E64①⑤⑥
- issue：**#58**（5 个单向门，与 U6 同族）· #76 · #72 · #75 · #43 · #74 · #41 · #81 · #60 · #73 · #63 · #68
- 工作区：**`branch-anywhere/`**（只剩发版，下一步给 aterm 发 `--fork-session` 契约冻结通报，U11 会动它）·
  **`account-onboarding/`**（红线「daemon 起会话机制零改」+ 待做 F6/F7，与 U8/U9/U10 正面冲突，**是否仍活着待确认**）·
  **`auto-e2e/`**（5 套 e2e 全部断言现行架构）

## 5. 文档面（摸底 D 漏掉的 8 条，全部集中在 `IPC-PROTOCOL.md`）

| # | 位置 | 文档说 | 实际 |
|---|---|---|---|
| X-13 | `:490-493` 握手时序图 | 「先写 `ps-await` → 再设 WindowTitle」 | `cc.ps1.tpl:59-65` 逐字写的是**相反顺序**及理由：「v2 竞态修复……旧顺序下 v2.21 实测每个新 shell 首次 cc 固定烧满超时」⇒ **文档在教一个已判定为 bug 的时序** |
| | `:507/:515/:89` | 忙等超时 800ms | 3000ms（`cc.ps1.tpl:77`） |
| | `:505-513` | 成功判据 = 检测到 await 删除 | 实际是读到 `ps-registry` 且指纹匹配（`cc.ps1.tpl:81`），注释明写「不依赖 await 删除这一种信号」 |
| X-14 | `:492`/`:520` | bind debounce 100ms | 50ms（`bind.rs:160`） |
| X-15 | `:229-237` | `history-metadata.json` 是裸 sid map | 实际 `HistoryMetadata{version, entries}` 两层；字段写盘是 `customTitle`；另有未提的 `updatedAt` / `lastAccount` |
| X-16 | `:302` + `INVARIANTS:147` | `procStart` = `.NET ToFileTime()`（1601 纪元） | 实际 `NetTicks` = `.NET DateTime.Ticks`（**0001-01-01 Local**），要经 `to_net_local_ticks()` 转。`:124` 说 `ps_proc_start` 与它「同语义」**是假的**，与 `INVARIANTS §18:310` 自相矛盾 |
| X-17 | `:364` | `SessionActivityPayload` 驼峰 | snake_case（`bridge.rs:232` 没有 `rename_all`；生成物 `session_id/status/waiting_for`） |
| X-18 | `:368-395` 帧字段表 | — | **漏 8 个线上字段**：`hello` 漏 `codex_dir`/`kinds`/`emits` · `line` 漏 `byte_offset` · `session_added` 漏 `agent_kind`/`liveness_confidence`/**`attachable`** · `session_status` 漏 `liveness_confidence`。⚠ `attachable` 有整节 §9.3 且标「契约冻结」，**帧表里却没有它** |
| X-19 | `:396-411` 一次性查询表 | 9 条 | **漏 5 条**：`--tmux-notify` · `--resolve` · `--fork-session` · **`--account-trust-zero`** · `--read-session-from-offset`。⚠ `main.rs:283-287` 记着 **v3.4.0 出过事故正是因为 `--account-trust-zero` 没被登记** —— 现在文档又漏一遍 |
| X-20 | `:407` | `accounts-meta` 首行字段 | 漏 `accountZeroAware`（Z01 能力标记） |

⇒ **`IPC-PROTOCOL.md` 563 行里七处在说谎**（§2/§3/§7/§9/§10/§10.1/§11 + 时序图）。
拿它当 U6 的冻结基线 = 把错误固化进新协议 ⇒ **v3 把它并进 U6，先修再冻结**，
并把「wire 字段名双向 `include_str!` 对拍」从建议升为必需（X-2 + X-18 + X-19 有那条测试一条都发不出来）。

### 5.1 另一处：`INVARIANTS §40` 的平价缺口表三行已被代码推翻
`:1116`「多账号 本地无 —— 最大的一处欠账」→ **已有**（`lib.rs:992-993` 的 E79）·
`:1118`「本地不注入任何 env」→ **假**（`history.rs:1047/1068` 的 `config_dir_prefix_ps`）·
`:1120`「`ccm` 全套修饰 本地无」→ POSIX 本地已落地。
⇒ **v2 的 D2/E15 建立在「本地不注入 env」这个已不成立的前提上**；
`history.rs:869-870` 逐字写着已经选了「**显式** `unset`（不是什么都不加 —— 那会被 shell rc 顶掉 = 静默串号）」
⇒ **D2 不是开放问题**。

## 6. CI / 构建：新增 crate / target / 子命令 / e2e 套件要同步改的地方

仓里**无 justfile / Makefile / `.cargo/config.toml` / `rust-toolchain.toml` / `clippy.toml` / `rustfmt.toml`**。
入口只有 `.github/workflows/{ci,release}.yml` · `package.json::scripts` · `scripts/run.ps1` · `src-tauri/build.rs`。

- **新增 crate**：`ci.yml:47` · `:74`/`:81`（每 crate 一条 `-p`）· `:88`/`:91`（fmt+clippy 各一条 `--manifest-path`，
  `:83-86` 注释明写「三样都要补」，branch-core 就漏过）· 五处 rust-cache `workspaces:` ·
  两个 `Cargo.toml` · `eslint.config.js:17` · `RELEASING.md:20` · 两个 README（「CI 七 job」写了 4-5 份副本）· `CONTRIBUTING.md:82`
- **新增 target/arch**：`release.yml:23/30-32/34-37/44-45/54-55/56-58/104/114/227/270-273` ·
  `build.rs:241/242/248/254/264/268` · `sftp.rs:503-517`（两个 `static DaemonBinary` + `include_bytes!`）·
  `sftp.rs:380-394`（`uname -m`，Windows 没有）· **`sftp.rs:1252-1263`（arch 表 + `assert_eq!(…, b"\x7fELF")`）** ·
  `REMOTE-PHASE0-DEPLOY.md:22-33` · `tauri.conf.json:31` + `release.yml:143/240` · `release.yml:149/248`
- **新增子命令**：`main.rs:132` bump · `:149` CAPABILITIES · `:172-177` + `:198-205` ·
  `build_id_guard.rs:49-60` 追加一行 · `accounts_query.rs:1379-1433`（含 `assert_eq!(subs.len(), 4)` 要手改）·
  `build.rs:175-180`/`:203-218` · `release.yml:56`/`:113` · `ssh_source.rs:1861`/`:1875` · 三份文档
- **新增 e2e 套件**：`package.json` · `ci.yml` 调用行 · **`ci.yml:316` 套数地板** · **`:318-321` 逐对自检** ·
  `:258-262` shellcheck 清单 + `:270` 文件数地板 39 · `e2e/README.md` · `e2e/exec-bit-guard.sh` 三处

### 6.1 `build.rs` 的静默失败点
`:162-165` 硬编码 `../remote-daemon-proto/src/main.rs`，**读不到 → `"unknown"` 不 fail**；
`:166` 同路径的 `rerun-if-changed`，**写错不报错、只是不再触发重编**；
`:250-256`/`:270-276` **缺 `.build_id` = 空串不 fail；缺二进制 = 不置 cfg，连 warn 都没有**。
capabilities 那侧有 `ssh_source.rs:882-889` 兜着，**build_id 没有任何等价断言** ⇒ v3 的 S8。

### 6.2 CI 结构缺口
daemon job（`ci.yml:150-175`）只跑 ubuntu-latest native gnu，**无 `--target`、无 musl、无 Windows、无 `--locked`**。
交叉编译只在 tag 时跑一次，而 `release.yml:164-183` 自承 `build-linux` job「**从未真正跑过**」。

## 7. 测试与 lint 现状（实测）

| 项 | 数 |
|---|---|
| `remote-daemon-proto` 测试 | 186（`#[tokio::test]` 0） |
| `src-tauri` lib 测试 | 663 编译出 = 593 人写（Linux）+ 70 ts-rs 生成 |
| `branch-core` | 8 |
| vitest | 1148 例 / 79 文件 |
| tsx node `*.test.ts` | 241 例 / 471 断言 / 16 文件 —— **既无断言地板又被 `coverage.exclude` 排掉，双重不设防**（E64①） |
| e2e 有地板 | **16 套**（U8a-2a 起，原 15） |
| e2e 无地板 | `graylight-suite`（**有论证的排除**，要 GUI+Xvfb）· `exec-bit-guard`（收尾格式不同，自带覆盖面自检）· **`f40-suite`（孤儿：不在 package.json、不在 CI）** · tier2 WDIO |
| 覆盖率门禁 | 阻断式，S52/B43/F46/L53，实测 S54.14/B45.15/F48.73/L55.64，裕度 2.1–2.7 点 |

**lint**：clippy 全仓 **advisory，无 `-D warnings`、无 `clippy.toml`、无 crate 级 `deny`** ⇒
本区 17 个功能全程唯一硬门是 `cargo fmt --check`。
shellcheck 覆盖 39 文件（含 `shared/ccm` 与 vendored，都有论证）；
**`src-tauri/scripts/cc.ps1.tpl` 102 行 PowerShell 零静态检查**却被 `include_str!` 进二进制（E64⑥）。

## 8. 依赖管理（实测结论：不构成障碍）

不是「钉死」是「lock + offline」：声明全是宽松 semver，起作用的是提交在仓里的
`remote-daemon-proto/Cargo.lock`（67 个包）+ `--offline`。
Windows 支持**不需要新增依赖**（lock 里已有 `windows-sys 0.48/0.61` + `windows-targets` + `mio`）；
`windows-sys` 加 feature 不下载新 crate。
**唯一要动 lock 的是 tokio `net`**（拉 `socket2`，daemon lock 里没有，但 `src-tauri/Cargo.lock` 有、本地 cache 有）。
⚠ 真风险不在离线，在 **CI 联网解析**：daemon job 既无 `--locked` 也无 `--offline`。

## 9. 四份审计明确未核实的

- 未跑任何 e2e（只读纪律）；`ccm-rbind-title` 的 8 由两条独立路径印证，其余 10 套是「静态 == 地板」的推定。
- 未跑 `cargo test`，「今天全绿 / 护栏今天真在叫」无一手证据。
- `WaitForSingleObject` ≡ `poll(pidfd)` 未验（计划自己登记为开放-1）。
- windows-latest 上 daemon 能否 `cargo test`（需 link.exe）未验；确证的只有「`cargo check` 不链接，ubuntu 上就能跑 Windows target」。
- `account-onboarding/` 是否仍活着 —— 其 STATUS 顶部说「已被接管」，下半截又在报 F5/F1 签收与「下一个 F2」。
- `shared/cc-bus/scripts/cc-spawn` 是否 vendored 外部工具（文件在本仓，但 unify-launch 与摸底 D 都写「不在本仓」）。
- `sftp.rs:1262` 的 ELF 断言在 `#[cfg(embedded_daemons)]` 下 ⇒ CI 上不编译，「会打红」只在本地/release 构建成立。
