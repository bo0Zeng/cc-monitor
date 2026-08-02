# U2 · 抽 `platform/` + `common/`

- 工作区：unified-backend · 主计划 §3 第二梯队 · 任务 #90
- 风险档：**高**（第一个真动结构的；动 `watcher.rs` 这个 1772 生产行的核心）
- 性质：**纯重构，行为逐字不变**。不修 bug、不改判据、不动契约。

## Phase B 复查：主计划对「回边」的判断偏了一个函数

主计划 U2 行写：「⚠ **`session_alive` 是 platform↔observe 的回边**（`spawn_pid_watcher:228` 调它），
先定形态：下沉 platform 还是谓词参数化」。

**复查实况**：`session_alive`（`watcher.rs:1626-1638`）= `pid_alive` + `proc_starttime` +
`is_same_live_process`，三者分别是平台原语、平台原语、**纯函数**。它整条都在 platform 域内，
**没有任何回边**，可以整体下沉。

**真正的回边是 `spawn_pid_watcher` 自己**：它依赖 `PidWatchTarget`（`:192`）与
`WatchEvent`（`:70`）—— 这两个是 **observe 的域类型**（`WatchEvent::Notify` 裹的是
`notify-debouncer` 的事件，`PidWatchTarget::Session{key}` 里的 key 是被观测的 pidfile 路径）。
一个「平台原语」不该知道「醒了要往哪个 channel 发什么帧」。

⇒ **定形态：切开，不参数化谓词。**

| 层 | 函数 | 知道什么 |
|---|---|---|
| `platform/` | `watch_pid_until_exit(pid, expected_start, on_dead)` | pidfd_open · 身份复核 · `poll(2)` · 起线程。**不知道** WatchEvent |
| `observe`（`watcher.rs` 留守） | `spawn_pid_watcher(target, pid, expected_start, tx)` | 把 `on_dead` 实现成 `tx.send(target.death_event(pid))` |

三条「判死」路径（pidfd_open 失败 / 身份复核不符 / poll 醒）都调 `on_dead`；
**「poll 真错误」那条仍然不调**（原注释逐字写着「真错误：**不**报死」）—— 这条语义必须原样保住。

## DoD

| # | 项 | 验收 |
|---|---|---|
| ① | `platform/` 建立，8 个平台原语搬入 | `pidfd_open` · `watch_pid_until_exit` · `pid_alive` · `proc_starttime` · `proc_cmdline` · `parse_btime` · `path_key`(两个 cfg 分支) · `session_alive` + `is_same_live_process` |
| ② | **`proc_starttime` 的两份重复合并** | `accounts_query.rs:311` 那份删掉，改用 `platform::`。变异：改 platform 那份的行为，`accounts_query` 的测试必须跟着变 |
| ③ | `common/` 建立，两组重复合并 | `projects_root`×4（`fork_write` / `history_query` / `search_query` / `usage_query`）· `mtime_ms`×2（`history_query` / `search_query`） |
| ④ | **行为逐字不变** | daemon `cargo test` **194 passed** 不减；wire 输出逐字节对拍（下面第 5 步）；三个 daemon-frames e2e 套件绿 |
| ⑤ | **护栏在新目录结构下真的还在扫** | 这是 U-1 存在的理由：`no_timer_guard` 的字节地板 + 数量相等 + `has_rs_subdir` 那条**休眠中的**回归钉现在必须上岗。变异：往 `platform/` 里放一处 `Duration::from_secs` ⇒ 必须红 |
| ⑥ | 文档面 | `doc/ARCHITECTURE.md` 的 daemon 模块图 · `remote-daemon-proto/` 若有 README · `INVARIANTS` 里点名 `watcher.rs:<行号>` 的地方 |

**不做**：不拆 `observe/`/`control/`（U3）· 不碰 Windows 编译（U4）· 不改 `pid_alive` 的
非 Linux 语义（那是 U4 的 Windows 语义决策，**本功能只搬不改**；主计划 U2 行写的「修 `pid_alive`
地雷」在此**明确推迟到 U4** 并登记）· 不动 `readonly_guard` 的钉法（U1b）。

> **为什么把「修 pid_alive 地雷」推迟**：那个地雷是「非 Linux 恒返回 `true`」。改它 =
> 决定 Windows 上「进程是否存活」怎么答，而那正是 U4 的正题。在 U2 里改等于**在纯重构里
> 夹带一个语义决策**，且 Windows 今天根本编不过（12 个错），改了也无从验证。
> 登记在此，U4 的 DoD 里必须有它。

## 与主计划对接（共享面）

- **S3 平台原语** —— 本功能交付它的 daemon 侧。判据 = 跨 target `--all-targets` 编译（U4）。
- **S5 `common/`** —— 本功能交付 `projects_root` / `mtime_ms` 两组；时间换算与 quote 留 U3。
- **S1 护栏扫描面** —— 本功能是它的**第一次真实考验**（U-1 修的递归 + 双判据就是为这一刻）。

## 逐条实现步骤

1. **建 `platform/mod.rs` + `platform/proc.rs` + `platform/pidwatch.rs` + `platform/paths.rs`**，
   函数**逐字搬**（doc 注释一起搬，别重写）。
   *验证*：`cargo test` 194 不减；`git diff` 里搬走的部分应当是纯移动。
2. **切 `spawn_pid_watcher`**：platform 侧 `watch_pid_until_exit`，observe 侧留薄包装。
   *验证*：三条判死路径 + 一条「不报死」路径逐条对照原文。
3. **`accounts_query.rs` 的 `proc_starttime` 去重**。
   *验证*：变异 —— 改 platform 那份让它恒返回 `None`，`accounts_query` 的相关测试必须红。
4. **建 `common/`，合 `projects_root`×4 与 `mtime_ms`×2**。
   *验证*：四处调用点行为不变；`cargo test` 不减。
5. **wire 输出逐字节对拍**：搬家前后各跑一次同一组 daemon 查询子命令，输出 `diff` 必须为空。
   *验证*：这是「行为逐字不变」的硬证据，不能只靠单测。
6. **护栏复验（DoD ⑤）**：跑 `no_timer_guard`，确认 `has_rs_subdir` 那条从休眠转为上岗；
   往 `platform/` 放变异确认会红。
7. **文档面** + 全量门禁。

## 测试策略

- 变异一律退出码判定；`cp -a` 备份还原后 `touch`。**新文件不能用 `git checkout` 还原**（U1a 踩过）。
- Rust 侧核「红得对不对」——rc=101 也可能是编译失败（假红）。
- **纯重构的特有风险是「搬丢了」**：靠 194 这个数 + wire 逐字节对拍两头卡。

## 实现期与计划的偏离

### 偏离①：多搬了一个 `parse_starttime_from_stat`（计划清单里没列）

`proc_starttime` 调它。计划的清单是照主计划抄的，漏了这个被调方。**不搬就编不过**
（`cannot find function parse_starttime_from_stat`）——是漏列不是扩范围，但按纪律登记。

### 偏离②：`watcher.rs` 的 import 拆成「生产段」与 `#[cfg(test)]` 两段

搬走之后，`is_same_live_process` / `parse_btime` / `parse_starttime_from_stat` /
`session_alive` / `USER_HZ` / `pidfd_open` 在 `watcher.rs` 的**生产段已无调用点**
（调用点随函数一起搬走了），只剩测试段还在用。

**没有给整条 `use` 加 `#[cfg(test)]`** —— 那样会让「哪几个是真生产依赖」看不出来。
拆成两条：生产段那条列真在用的四个（`path_key` / `pid_alive` / `proc_cmdline` /
`proc_starttime` / `start_epoch_from_ticks`），测试段那条单列。

### 偏离③：`search_query.rs` 顺带去掉一条失效 import

`mtime_ms` 搬走后 `use std::time::UNIX_EPOCH;` 在该文件成了唯一无用引用（`history_query.rs`
另有别处在用，故保留）。不去掉会留一条 `unused import` 警告。

## 变异 / 对拍验证

| # | 项 | 判据 | 实测 |
|---|---|---|---|
| **1** | **`no_timer_guard` 在新目录下还在扫吗** | 往 `platform/proc.rs` 放 `Duration::from_secs(8)` | **RC=101，两条判据同时红**：`生产段 Duration::from_* 有 2 处，登记表里只有 1 条：[("platform/proc.rs",1),("watcher.rs",1)]` + `生产代码 platform/proc.rs 含周期性唤醒构件`。**诊断按相对路径逐字报出子目录文件** |
| **2** | `proc_starttime` 去重是不是真共用一份 | 改 platform 那份让它恒返回 `None` | **RC=101**，`accounts_query` 的两条测试跟着红（`能读自己的 starttime`） |
| **3** | **wire 输出逐字节对拍** | 搬家前后各跑 6 个查询子命令 | **sha256 完全相同**（`cecd70e6f94f7fd3`），`diff` 无输出 |
| 4 | 扫描面是否真含新目录 | 把地板临时改大读实测值 | 20 个文件 / **121 449 字节**，清单里逐字含 `common/mod.rs` `common/paths.rs` `platform/mod.rs` `platform/pidwatch.rs` `platform/proc.rs` |

> **变异 1 是 U-1 的回报，也是它存在的全部理由。** 修递归之前 `daemon_sources()` 是单层
> `read_dir`（目录没有扩展名 ⇒ 被整个跳过），代码搬进 `platform/` 后它会扫到 0 个子目录文件而
> **静默全绿** —— 整个 U2 就跑在假绿里了，而当时的地板 `files.len() >= 5` 照样满足。

## 合并前的等价性核对（**没有靠「看起来一样」**）

- **`proc_starttime` 两份**：都返回 boot 起的 jiffies；解析都是「`rfind(')')` 之后第 `22-3` 个
  whitespace token」。`accounts_query` 那份内联、`platform` 那份走 `parse_starttime_from_stat`。
  **单位相同才敢合** —— 单位不同的话它们就不是重复，合并就是引 bug。
- **`projects_root` 四份**：函数体都是 `claude_dir.join("projects")`，逐字相同。
- **`mtime_ms` 两份**：连 `unwrap_or(0)` 的退化语义都逐字相同（**该语义原样保留**，
  读失败与「1970 年的文件」被混成同一个值是既有设计，要改得单独立项）。

## 门禁结果

| 项 | 值 |
|---|---|
| daemon `cargo test` | **194 passed**（与搬家前**一字不差**），RC=0 |
| monitor `cargo test --lib` | 663 passed / 3 ignored，RC=0 |
| `cargo fmt --check` 两侧 · `tsc` · `npm test` | OK · RC=0 · 80 文件 1154 例 |
| daemon `cargo clippy` | **0 告警** |
| e2e（**真跑 daemon 二进制**） | `graylight-frames` 12 · `restart-frames` 5 · `resume-frames` 7 · `daemon-fork` 10，全过地板 |

## 代码审计结果（D，高风险档：两视角并行）

### 视角一（架构符合度 + 文档工程）—— 阻塞 2 项，**两条都是我把「规则」写成了「现状」**

| # | 阻塞 | 处置 |
|---|---|---|
| **B1** | **README 同一节里自相矛盾**：`:17` 写「三条解耦线里**第一条已落地**」，而 `:34` 我自己写「真判据只有跨 target 编译，那是 **U4** 的 DoD」。主计划 §1.1-1 对第一条线的判据逐字就是那条编译。审计实跑：**仍 RC=101 / 12 错**。这是新人读 daemon 的第一份文档，「已落地」会让 U3/U4 接手人以为平台线收口了 | 改写成「**有了目录，尚未收口**」，并写明「平台线在 U4 通过那条编译判据之前都不算落地；U2 做的是把 11/12 个错**集中到一个文件**，让 U4 有明确下手点 —— 这是真进展，不是收口」 |
| **B2** | **「`platform/` 是唯一允许平台原语的层」被写成现状，而生产段还有 4 处在外面**，且功能计划的「不做」清单一个字没提 —— **既没收也没登记，文档却已宣布收完** | ① **真收两处**（见下）；② 剩下 4 处**如实列表进 README**，逐条写明处置（`tmux_hook.rs` 的 `libc::kill` 归 U3；`main.rs` 三处是组装根、可辩护为留，**但不能因此说「唯一」**） |

**据 B2 真收进来的两处**（审计说「现在搬成本近零，不搬则 U3 一划层就当场违反 §1.1-1」）：

- `accounts_query.rs::proc_claude_config_dir` —— 读 `/proc/<pid>/environ` + 两个 `target_os` cfg。
  **附带修一处我自己造的悬空引用**：它的头注写着「形状照 `watcher::proc_cmdline`」，
  而 `proc_cmdline` U2 之后在 `watcher.rs` 里已经不存在了。
- `watcher.rs::watch_loop` 里内联的第五处 `claude_dir.join("projects")`（见下 重要-2）。

### 视角一 · 重要（6 项，全部当轮修掉）

| # | 发现 | 处置 |
|---|---|---|
| A1 | **`USER_HZ` 是未使用的 import**，`cargo test`/`clippy` 各报一条 warning。「CI 的 clippy 是 advisory ⇒ **不会红**」—— 与 U1a Phase D 的 A1 是**同一族的静默退化** | 删掉 |
| A2 | **`projects` 这个目录名是五处不是四处**：第五处是 `watcher.rs::watch_loop` 里**内联**的（`grep fn projects_root` 找不到它）。不收的话「合并去重」承诺的性质（改布局只改一处）**根本没拿到** | 收进 `common::paths::projects_root`。并在注释里写明「生产段现在单一来源；**测试夹具里那批刻意不收** —— 测试自己搭目录该写字面量，走生产 helper 就成了拿自己对自己断言」 |
| A3 | **计划要建的 `platform/paths.rs` 没建**，`path_key` 被塞进了自称「`/proc` 与进程身份这一族」的 `proc.rs` —— 它既不是 `/proc` 也不是进程身份，是 NTFS 路径语义。**而且这条偏离没登记** | 建 `platform/paths.rs` 并搬过去。审计的理由更要紧：**U4 给 Windows 补第二套时路径语义与进程语义会各自长分支，现在分开零成本，到时候再搬就是第二次搬同一段** —— 铁律 6 的典型 |
| A4 | **`common/` 的门槛第一天就被自己放进去的函数破了**：门槛写「**纯**（不 I/O）」，而 `mtime_ms` 第一行就是 `std::fs::metadata`。门槛是防「杂物间」的唯一机制，第一条自相矛盾会让它失去约束力 | 门槛改写成「**平台无关**」而不是「纯」。真正要挡的从来不是 I/O，是**平台知识**：`mtime_ms` 做 I/O 但对 OS 一无所知（`std::fs` 在哪都一样）；`proc_starttime` 也做 I/O，但它认识 `/proc` 的字段布局 |
| A5 | **`mtime_ms` 的「≥2 个上层用」按层口径不成立** —— 两个调用点同属 observe | **登记而不粉饰**：写进 `common/mod.rs`，并写死「**U3 的 DoD 里必须复查这一条**」。现在不动是因为 `observe/` 还不存在，硬划等于凭空造一个层 |
| A6 | README 模块清单**漏列 `USER_HZ`**；`ARCHITECTURE.md` 引 `Cargo.toml:7-9` **off-by-one**（实际 6-8） | 都改；顺带把 `Cargo.toml` 的引用写成仓库相对路径 |

### 我自己在修 B1 时又差点抄错一个数

改 README 时我把「12 个错、11 在 pidwatch + 1 个 `getuid`」照搬了审计与主计划的记录。
**自己断言的自己核**：`cargo check --all-targets --target x86_64-pc-windows-msvc` 实跑 —— 
`grep -c '^error'` 给出 **14**，吓一跳；逐条数 `error[EXXXX]` 才是 **12**（另 2 行是 `error: could not compile` 汇总）。
按错误逐条归位：**11 个 `platform/pidwatch.rs` + 1 个 `watcher.rs:2182 libc::getuid`**，
且已核实 `getuid` 在测试段（其上最近的 `#[cfg(test)]` 在 :2028）。**README 的数是对的**，
但差一点就成了又一处「抄来的数字」。

### 补跑的两条护栏变异（审计要求的第二条 + 新目录复验）

| 变异 | 实测 |
|---|---|
| 往新建的 `platform/paths.rs` 放 `Duration::from_secs(3)` | **RC=101**，诊断逐字含 `platform/paths.rs` |
| 往 `common/paths.rs` 放 `std::fs::write`（`readonly_guard`，此前只验过 `no_timer_guard`） | **RC=101**：`daemon 写盘护栏违规（红线 I7 默认层）：生产代码 …/common/paths.rs 含 fs::write` |

### 视角一 · 登记待办（不在 U2 范围）

- **`readonly_guard.rs:205` 的白名单按裸文件名匹配**（`name == WRITE_WHITELIST_MODULE`）。
  U2 引入子目录后，「同名文件在不同目录」**第一次成为可能** ⇒ 将来任何 `*/fork_write.rs`
  都会被当白名单放行。**U3 把 `fork_write.rs` 搬进 `control/` 时必须改成路径判定。**
  `no_timer_guard.rs:265-272` 已经处理过同类问题（basename 后缀匹配 + 写了理由），照抄即可。
  U2 的「不做」清单明写不动 `readonly_guard` 的钉法 ⇒ 留 U1b/U3。
- **账本 S5 的「时间换算 · quote」在 daemon crate 内没有可合的逐字副本**（审计逐条核过）：
  时间换算三处语义/单位各不相同（`history_query.rs::created_ms_or_mtime` created 优先 ·
  `watcher.rs::file_mtime_epoch` 单位是**秒**不是毫秒 · `platform/proc.rs::start_epoch_from_ticks` ticks→epoch）；
  quote 在 daemon 内只有 `tmux_hook.rs::sq` 一份，多份在 monitor 侧、**跨 crate，`common/` 收不了**。
  ⇒ **S5 的措辞要改**（F 阶段落账）。
- **`is_same_live_process` 留在 `proc.rs` 是对的**（审计用我自己写的 `common/` 门槛判的：
  唯一生产调用点在同文件；整段 doc 讲的是 `/proc` 的域知识）。**但 U4 埋一条伏笔**：
  Windows 判活要复用这张判定表，届时应上提到 `platform/liveness.rs`，而不是留在按 `/proc` 命名的模块里。

### 视角二（有没有搬丢/搬错）—— **阻塞 0**，用了比我更硬的手段

它没采信我的自证，自己重做了一遍且更狠：

- **10 项逐字对比**（去掉可见性前缀后 `difflib` 精确比对）→ 全 OK。
  更强的一条**不依赖它挑对 item 边界**：把 `git diff -U0` 里 `watcher.rs` 的**全部 230 行删除行**
  拿出来逐行在 `platform/*.rs` 里找对应，**只有 3 行找不到** —— 正是切分点那三行
  （两处 `tx.send(target.death_event(pid))` → `on_dead()`，一处 `session_alive` 加了模块前缀）。
  **其余 227 行一字不差。**
- **`proc_starttime` 去重的等价性用差分 fuzz 证**：两份实现原样贴进一个 fuzz 程序，
  手挑边角（空串 / 无 `)` / `((odd))` / 多字节 comm / u64::MAX / 溢出 / 负数）+ **60 万条生成输入**
  ⇒ `600015 inputs, 39708 produced Some(..), **0 mismatches**`。
- **wire 对拍它做到 14 条子命令**（我只跑了 6 条），且 **stdout + stderr + 退出码三样**全 diff 为空
  （rc 覆盖 0 和 2）。**外加流模式**：真杀掉被看守 PID 触发 pidfd 判死，帧按 PID 归一化后逐字节相同
  —— **三条判死路径里最难保住的那条（③ poll 醒）实测保住了**。
- **`FnOnce` 不可能被调两次是编译期保证的**：三个调用点里有一个在 `loop` 内，
  只要存在「调完还能再调」的路径，rustc 会给 `E0382 use of moved value ... in previous iteration of loop`。
  **它编过了 ⇒ 已证**（这条比任何测试都硬）。
- 行数/函数计数两头对账：`3837 → 3625`（Δ −212）= `230 删 − 18 加` ✓；
  顶层 `fn` `42 → 31`（Δ −11）= 新层 12 个 − 1 个新写的包装 ✓。

### 视角二 · 重要 / 建议（4 项，全部当轮处置）

| # | 发现 | 处置 |
|---|---|---|
| R3 | **`tracing` 事件的 target 变了**（`cc_monitor_remote::watcher` → `::platform::pidwatch`）—— 级别与文案逐字未变，但 `tracing` 的 target 默认取 `module_path!()`。审计查过影响面**为零**（全仓无 `cc_monitor_remote::` 命中、无 `RUST_LOG`、无脚本 grep 这四条文案） | **不改，但如实记**：这是「行为逐字不变」唯一的例外，签收时不粉饰 |
| S1 | **`PidWatchTarget` 头上那 20 行权威说明文不对题** —— 它逐条描述「三条判据 + poll 真错误刻意不发」，而那些代码已全在 `pidwatch.rs`。**这段错位 U2 之前就有**（当年就贴错了 item，隔着 enum + impl 才到 `spawn_pid_watcher`），U2 把它从「贴错 item」升级成「跨文件悬空」 | 搬到 `pidwatch.rs` 头注，它真正描述的代码旁边，并注明这段错位的来历 |
| S2 | **`no_timer_guard.rs` 的实测基线注释又过期了** —— 写着「15 个文件 121_131 字节」，而 U2 之后是 20 个。**而那段注释自己第一句就是「别手抄这个数」** | **这次不改数字，改做法**：删掉快照值，只留复测办法。教训写进注释：「注释里但凡出现一个会随代码变的数字，光写『别手抄』挡不住任何人 —— 要么给复测办法，要么根本别记」 |
| S3 | **「poll 真错误不判死」这条路径零测试覆盖**（既有缺口，非本轮回归） | 它**没法用普通测试触发**（要让 `poll(2)` 真出错，需伪造坏 fd 或注入 syscall 失败）。⇒ 按本仓惯用形式加**结构性钉子**：扫源码，EINTR 之后那段里不许出现 `on_dead`。**变异验证**：把那条也改成判死（= 「顺手统一成都发」这个真实失败模式）⇒ **RC=101** |
| Q8 | **`REGISTERED_DURATION_USES` 的 `ends_with` 分支被执行了却恒为 false** —— 「返回 true」那一侧**零覆盖**，U3 搬 `watcher.rs` 进 `observe/` 时才第一次真生效；而它一旦写错，报出来是「登记表在腐烂」这条**指向完全错误方向**的诊断 | 抽成 `matches_registered(rel, file)` + 真值表单测（含 `observe/watcher.rs` ⇒ true，以及 `notwatcher.rs` / `watcher.rs.bak` / `watcher.rsx` ⇒ false）。三行成本，现在钉住 |

## 工程审计结果（E，主线程对账）

- **主计划仍自洽**，但账本两处要改（F 阶段落账）：
  - **S3 平台原语**：U2 交付的是「**目标归属地 + 把 11/12 个 Windows 编译错集中到一个文件**」，
    **不是收口**。生产段还有 4 处在外（`tmux_hook.rs` 的 `libc::kill` 归 U3；`main.rs` 三处是组装根）。
  - **S5 `common/`**：「时间换算 · quote」这两项在 daemon crate 内**没有可合的逐字副本**
    （审计逐条核过：时间换算三处语义/单位各不相同，其中 `watcher.rs::file_mtime_epoch` 单位是**秒**；
    quote 在 daemon 内只有一份，多份在 monitor 侧、**跨 crate，`common/` 收不了**）。S5 措辞要改。
- **给 U3 的清单**（铁律 6：现在写死，别到时候只做一半）：
  1. `tmux_hook.rs` 的 `libc::kill` —— §1.1 已裁定 `tmux_hook` 归 `control/`，搬层时连它一起处理，
     否则 `control/` 里带一个裸 libc 原语。
  2. **`readonly_guard.rs:205` 的白名单按裸文件名匹配** —— U2 引入子目录后「同名文件在不同目录」
     **第一次成为可能**，将来任何 `*/fork_write.rs` 都会被当白名单放行。照 `no_timer_guard` 的
     `matches_registered` 改成路径判定。
  3. **复查 `common/mod.rs` 里 `mtime_ms` 的「≥2 个上层」** —— 今天两个调用点同属 observe，
     `observe/` 一建出来就要重判。
  4. `is_same_live_process` 在 U4 要上提到 `platform/liveness.rs`（Windows 判活复用同一张判定表）。

## 签收

- [x] 过代码审计（D，两视角并行）—— 视角一阻塞 2 + 重要 6，视角二阻塞 0 + 重要/建议 5，**全部当轮处置**
- [x] 过工程审计（E，主线程对账）—— 账本 S3/S5 措辞要改；给 U3 写死四条清单
- [x] 主计划已更新（F）

## 最终门禁

| 项 | 值 |
|---|---|
| daemon `cargo test` | **196 passed**（基线 194，+2 = `matches_registered` 真值表 + poll 结构性钉子），RC=0 |
| monitor `cargo test --lib` | 663 passed / 3 ignored，RC=0 |
| daemon `cargo clippy --all-targets` | **0 告警**（与 `4901757` 基线一致） |
| `cargo fmt --check` 两侧 · `tsc` · `npm test` | OK · RC=0 · 1154 例 |
| **wire 逐字节对拍** | 与搬家前 **sha256 相同**（改到最后一版仍相同） |
| e2e（真跑 daemon 二进制） | `graylight-frames` 12 · `restart-frames` 5 · `resume-frames` 7 · `daemon-fork` 10 |
| Windows 跨 target | 仍 **12 错**（与主计划记录一致），11 个已集中到 `platform/pidwatch.rs` + 1 个 `watcher.rs:2182` 测试段 `libc::getuid` |
