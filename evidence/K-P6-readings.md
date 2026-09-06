# K-P6 读数 —— `KP6D1` 的全部答案

> **量于**：工作树 `/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-p6` ·
> `HEAD = 55c7fde151b54e1c59b23fc478f9b493c94f6fc7`（分支 `track/k-p6`，基点即 HEAD，本文写作时**零源码改动**）
> **量具**：`evidence/K-P6-russh-ruler.py`（尺子）· `evidence/K-P6-dial-census.py`（扼流点普查）·
> 其余为下文逐条写出的 shell 命令。两份量具的输出各自落在 `.out` 同名文件里。
> **本文里的每一个行号都钉在 `55c7fde` 上，并在旁边抄了那一行的逐字内容**（`brief` 13c）。

---

## ① `§0a①` 复核 —— **四条全对**，另补一条 PM 没量的

| PM 写的 | 我打的命令 | 读数 | 判 |
|---|---|---|---|
| 拨号入口 `ssh_source.rs:573` `client::connect(...)` | `grep -rn 'client::connect' src-tauri/src --include=*.rs` | `573: match client::connect(config, (ep.host.as_str(), ep.port), handler).await {` | ✅ |
| 拨号入口 `:687` `client::connect_stream(...)` | 同上 | `687: let session = client::connect_stream(config, stream, handler)` | ✅ |
| `ssh_source.rs` 全文 8336 行 | `wc -l src-tauri/src/ssh_source.rs` | `8336` | ✅ |
| `remote-daemon-proto/Cargo.toml` 零 `russh` 依赖，提到的两处是注释、说加密后端对齐 | `grep -rn russh remote-daemon-proto/` | `Cargo.toml:91` `#     cargo tree -i ring … ⇒ ring └── russh └── monitor`；`Cargo.toml:100` `# crypto provider 钉 ring（不是默认的 aws-lc-rs）：ring 是 monitor 侧 russh 已经在用的那一棵，` | ✅ 两处都是 `#` 注释、都在讲加密后端 |
| `inbound.rs:22` 逐字「载体是现成的：monitor 那头拿的是 `russh::ChannelStream`」 | `sed -n '22p' remote-daemon-proto/src/inbound.rs` | `//! 载体是现成的：monitor 那头拿的是 \`russh::ChannelStream\`，**双工**，` | ✅ |

**补一条 PM 没量、而本件非知道不可的**（`grep -c '^name = "<pkg>"$' <lock>`）：

| 包 | daemon 那份 `Cargo.lock` | monitor 那份 `Cargo.lock` |
|---|---|---|
| `russh` | **0** | 1 |
| `russh-sftp` | **0** | 1 |
| `ring` | **1** | 1 |
| `aws-lc-rs` | 0 | 0 |

⇒ 「daemon 侧零 `russh`」是**依赖图上的**事实，不只是 `Cargo.toml` 上的。
⇒ 而 **`ring` 已经在 daemon 的依赖图里**（`K-H1` 08-25 那条 `rustls` 带进来的）。这一格很要紧，见 `§④-甲`。

---

## ② `§0a②` 复核 —— **两个数都对，分类那句话要收一格**

```
grep -c 'local_daemon' src-tauri/src/ssh_source.rs   ⇒ 0
grep -n  'ssh_source'  src-tauri/src/local_daemon.rs ⇒ 10 行
```

- `ssh_source.rs` 里 `local_daemon` **零命中** ✅（PM 写的就是这个）
- `local_daemon.rs` 提到 `ssh_source` **10 处** ✅（数对）
- ⚠ PM 写「**10 处全是借协议帧的解析**」——**10 处里 8 处是**，另 **2 处不是**：
  - 帧解析族 **8 处**：`:355` `:808` `:859` `:5202`（`parse_frame`）· `:361` `:864`（`InboundFrame`）· `:831`（`CappedLine, DAEMON_FRAME_LINE_CAP`）· `:838`（`read_capped_line`）
  - **不是解析的 2 处**：`:865` `crate::ssh_source::record_tmux_raw(crate::inbound_client::LOCAL_ORIGIN, raw.clone());` 与
    `:871` `crate::ssh_source::forget_tmux_raw(crate::inbound_client::LOCAL_ORIGIN);`
    —— 那是**帧派生的进程内缓存**（tmux 原文按 origin 存一份），借的是 `ssh_source` 的**状态**，不是它的解析器。
- **PM 那句结论「一处拨号都没有」成立** ✅ —— 上面 10 处没有一处碰 `connect_*`。

---

## ③ `§0a③` 复核 —— 🔴 **PM 的 33 应为 32**；并给出一把更准的尺子

### 甲 · 先把 PM 自己那把尺子重打一遍（口径逐字：子串 `russh`，剔掉 `lstrip()` 后以 `//` `/*` `*` 打头的行）

| 文件 | 总命中行 | 粗·代码行（PM 口径） |
|---|---|---|
| `ssh_source.rs` | 34 | 19 |
| `sftp_pool.rs` | 8 | 5 |
| `sftp.rs` | 8 | 3 |
| `port_forward.rs` | 6 | 2 |
| `mcp.rs` | 1 | 1 |
| `structural_scan.rs` | 1 | 1 |
| `remote_write_registry.rs` | 1 | 1 |
| `launch.rs` / `tmux.rs` / `account_usage.rs` | 1 / 2 / 1 | 0 / 0 / 0 |
| **合计** | **63** | **32** |

🔴 **件文件 `§0a③` 逐字写的是「代码行合计 **33** 处 / 7 份文件」——「7 份文件」对，「33」错。**
PM 自己那张表逐行相加就是 `19+5+3+2+1+1+1 = 32`。**这是本轮顶回的第 1 处。**
（`§0a③` 表里那些**逐份**的数一个都没错，错的只有合计那一步。）

### 乙 · 更准的那把尺子（`KP6D1` 点名要的）

住址 `evidence/K-P6-russh-ruler.py`，输出 `evidence/K-P6-russh-ruler.out`。
它比 PM 那把准在**两处**，而两处都逐处可核（输出【④】栏）：

1. **走 Rust 词法状态机**，每个字符标 `code` / 行注 / 文档注 / 块注 / 字符串 / 字符，
   于是**行尾注释不再算代码**（PM 自己点名的那个粗口径）；
2. **按标识符边界匹配**，于是 **`russh_sftp` 不再算成 `russh`** ——
   那是**另一个 crate**（`src-tauri/Cargo.toml:90` `russh = …` 与 `:95` `russh-sftp = "2"` 两条独立依赖行），
   而 `§2.1` 恰好把 SFTP 划在写区外 ⇒ 混着数，`KP6D3①` 的分母里就装着本件根本不该动的东西。

**读数（严格词 `russh`、代码态）：21 处 / 21 行 / 3 份文件**

| 文件 | code | 行注 | 文档注 | 串 | **代码行** |
|---|---|---|---|---|---|
| `ssh_source.rs` | 19 | 4 | 12 | 0 | **19** |
| `port_forward.rs` | 1 | 1 | 5 | 0 | **1** |
| `sftp.rs` | 1 | 2 | 4 | 0 | **1** |
| 其余 7 份 | 0 | 2 | 5 | 1 | **0** |
| **合计** | **21** | 9 | 26 | 1 | **21** |

另一个 crate `russh_sftp`（**单列，别混**）：`sftp_pool.rs` 5 · `sftp.rs` 2 · `mcp.rs` 1 ⇒ 代码态 **8 处**。

**差额 11 行逐处**（粗尺子算「代码行」而严格尺子不算）：
- 8 行命中的是 `russh_sftp`：`mcp.rs:411` `sftp.rs:32,33` `sftp_pool.rs:131,368,472,473,474`
- 2 行落在**字符串字面量**里：`remote_write_registry.rs:260` `|| prod.contains("russh_sftp")` ·
  `structural_scan.rs:1449` `("lib.rs", "russh-sftp-2.3.0/src/protocol/file_attrs.rs", 29),`
- 1 行落在**行尾注释**里：`port_forward.rs:94` `let session = Arc::new(session); // russh Handle 不 Clone → Arc 共享`

🔴 **对 `KP6D3①` 的直接后果 —— 三条分母校正**：
- `sftp_pool.rs` 的 `russh` 代码行是 **0**，不是 5 ⇒ 它**根本不在**「界面持 russh 句柄」这个人群里；
- `sftp.rs` 是 **1**（`:31` `use russh::client;`），不是 3；
- `port_forward.rs` 是 **1**（`:162` `russh::Disconnect::ByApplication,`），不是 2。
⇒ **`KP6D3①` 那句「那 33 处」应改成「那 21 处 / 3 份文件」**，并把「射程外的逐份写清为什么」
从 4 份（PM 的口径）缩到 **2 份**（`sftp.rs` 1 处 + `port_forward.rs` 1 处）。

### 丙 · ⚠ 这把尺子**保证不了**什么（别读大一格）

- 它答「这个词写在代码位置上」，**不答**「这一行真的持有一个句柄」。
- 它**不**跨文件解析 `use` 别名（`use russh as ssh;`）—— 这正是 `KP6D3` 自己写的
  「判据要断**拨号这件事**，不是断一个词」。本尺子**不**声称堵住它。
- `#[cfg(test)]` 段**不剥**：上表是整份文件的数。要「只生产段」得换 `guard_core::production_code` 那把尺子。

---

## ④ `§0a④` 那四格 —— 逐格答死

### 甲 · **协议今天收不收得下「去拨一个远端」** ⇒ 🔴 **收不下**，而且缺口不在「加个变体」那一档

**盘上的现状（都可重打）**：

- `remote-daemon-proto/src/wire.rs` 的 `pub enum Frame` 今天 **11 个变体**：
  `Hello` `Line` `SessionAdded` `SessionStatus` `SessionRemoved` `TurnEnd`
  `TmuxSessionClosed` `TmuxSessions` `Overflow` `Reply` `Cancelled`。
- 入方向 `inbound::COMMANDS` 今天 **8 条**（`inbound.rs:78-79` 逐字
  `&["bus-kill", "bus-list", "bus-send", "cancel", "kill", "launch", "ping", "resolve"]`），
  全部是 **request/reply** 形状（`Request{args}` → `Frame::Reply`）。
- **11 个变体里没有一个能装一条双工字节流**；8 条命令里没有一条是长连接。

**而「拨号」要的恰恰不是一次 reply，是一条会一直往回吐的流**：
`ssh_source.rs:1304 pub async fn connect_and_exec(` 拿到的是 `russh::ChannelStream`，
之后 `:3763 async fn stream_loop(` 在它上面**一直读远端 daemon 的帧**。
把这件事搬进本机后端 ⇒ 本机后端要把**远端那条帧流**转回界面，于是撞上三堵墙，**每一堵都有机检钉着**：

1. **`single_stream_guard`（daemon 侧，`K-P1 KPY8`）** —— 模块头注第一行逐字：
   「**『多客户端的流』本件明确不做 —— 留一个触发器，不留一句话**」。
   它的 `PINS` 表钉着三个「恰好一个」：`writer_task(` **恰好 2**（stdio 一条 + 常驻一条）·
   `inbound::spawn(` **恰好 2** · `busy.swap(true` **全 crate 恰好 1**（「谁拿到那一条流」由一次原子操作决出）。
   转发一条远端流 = 第二个 `Frame` 生产者 ⇒ 那张表当场红。
2. **`Overflow.lost` 的账要重新定义** —— 同一份头注逐字：
   「分流之后『丢了哪几帧』这本账要**重新定义**。★ **那是语义变更，不是搬运。**」
   而 `§2.4` 逐字「**不改协议既有变体语义**」⇒ **本件的写区里做不了这件事。**
3. **origin 这条维度协议里根本没有** —— 今天「这一帧是哪台机的」由**它从哪条连接进来**决定
   （`local_daemon.rs:865` 拿 `crate::inbound_client::LOCAL_ORIGIN` 硬贴本机标签）。
   一条连接上转发两台机的帧 ⇒ 必须给帧加 origin、给 `Request` 加 target。
   ⚠ **`Hello` 尤其致命**：远端的 `Hello`（`build_id`/`capabilities`/`commands`/`unavailable`）
   若原样出现在本机那条流上，界面会把它读成**本机后端的** hello。

⇒ **答案**：协议**不够用**；补齐它**不是 additive 加一个变体**，而是
①给出方向加 origin 维、②给入方向加 target 维、③重定义丢帧账、④解掉「一条流一个客户端」那三根针。
**这四样每一样都比本件的写区大**，且 ③ 被 `§2.4` 明文排除。

⚠ **诚实边界**：以上说的是「**今天盘上的协议 + 今天钉着的那几根针**」。
我**没有**证明「不存在任何办法」——我证明的是「**在本件写区与 `§2` 的禁令之内没有办法**」。

### 乙 · 那条 `cfg!(target_os = "linux")` 闸的射程 ⇒ 🔴 **PM 的二选一是伪二分**；真答案是第三个，而且闸有**两条**不是一条

PM 问的是「管的是『整个本机后端只在 Linux 起得来』还是只管『脱离运行』那一步」。**两个都不是。**

**闸一 · `local_daemon.rs:1372`**（逐字 `let embedded = if cfg!(target_os = "linux") {`）
— 它只管**内嵌字节要不要用**。理由就写在它上面（`:1352` 逐字
`// ★★ **内嵌的那两份是 musl LINUX 二进制，本机不是 Linux 就一份都不能用**`）。
非 Linux ⇒ `embedded = None`，**仅此而已**。

**闸二 · `local_daemon.rs:902`**（逐字 `cfg!(target_os = "linux"),`，是 `detach_wanted(...)` 的头一个实参）
— 它管的是 **`K-P1` 常驻/脱离那条路**：`:667-670` 逐字
`is_linux && !no_detach_env.is_some_and(|v| !v.trim().is_empty())`。
非 Linux ⇒ `DetachOutcome::NotTaken`，**回落到下面那条 stdio 监护路**。

**第三个答案（PM 两个选项都没有覆盖到的那一个）**：
非 Linux 上本机后端**照样起得来**，走的是 `local_backend::start_or_extract` →
`resolve_beside_this_exe`（`backend/control/local_backend.rs:578`）→ **exe 旁边那份原生 sidecar**。
那份 sidecar 是**真的会打包进去的**：

- `.github/workflows/release.yml:151-153` 逐字 `- name: Build local backend sidecar (native)` /
  `working-directory: remote-daemon-proto` / `run: cargo build --release`
- `:154-162` 把它拷成 `src-tauri/binaries/cc-monitor-remote-$triple.exe`
- `src-tauri/tauri.sidecar.conf.json` 逐字 `"externalBin": ["binaries/cc-monitor-remote"]`
- 同一段的头注逐字给了**真机读数**：
  「daemon 在 Windows 上真编得过（2026-08-04 真机实测 exit=0、release 2.6 MB，
   且跑起来发完整 hello；tmux 那半诚实发 `observation:"unobservable"`）」

🔴 **⇒ 件文件 `§0a④` 那句「**这一格要命**：如果本机后端在 Windows 上根本起不来，
那『拨号归本机后端』在 Windows 上就落不了地」—— 前提不成立，那一格不要命。**
Windows 上本机后端**起得来**（原生 sidecar + stdio 监护），`K25`「每个平台一份原生后端」在这条路上已经兑现。

⚠ **但要换一格担心，而这一格 PM 没提**：Windows 上本机后端只有 **stdio 监护**这一种承载，
**没有** `K-P1` 那条常驻监听口。而用户那句「**为什么现在 ssh 不能独立跑**」要的正是
「界面关了它还在」—— 那件事**只有常驻那条路买得到**。
⇒ **本件在 Windows 上即使搬完，也买不到「独立跑」**，只买到「拨号发生在另一个进程里」。**如实登记，别硬凑**（`§4` 自己要求的）。

### 丙 · SFTP 那两份怎么办 ⇒ **本件不必动**，且 PM 给的分母偏大

- `sftp_pool.rs` 的 `russh` 代码行 = **0**（8 处全是 `russh_sftp`）⇒ 它**根本不在人群里**。
- `sftp.rs` 的 `russh` 代码行 = **1**：`:31` `use russh::client;` —— 只是为了写出 `client::Handle` 这个类型名。
- 两份**都不拨号**：它们的连接来自 `sftp.rs:48` `let (session, _fp) = connect_session(cfg, None, None).await?;`。
⇒ **`§2.1` 的划法是对的，SFTP 不必进写区。**

🔴 **但有一条连带，PM 没写、必须报上来**：`sftp.rs:48` 是 `connect_session` 的**调用点**。
一旦本件真把 `connect_session` 搬走，这一处（以及丁里那一处）**必然连带**，
届时 `sftp.rs` / `port_forward.rs` 各要改 **1 处调用**。**那时请 PM 划写区，我不自己扩。**

### 丁 · 端口转发同理 ⇒ **本件不必动**，同样有一处连带

- `port_forward.rs` 的 `russh` 代码行 = **1**：`:162` `russh::Disconnect::ByApplication,`（主动断连，不是拨号）。
- 它的连接同样来自 `:91` `let (session, _fp) = ssh_source::connect_session(&cfg, None, None)`。
- 模块头注 `:2` 自陈逐字「cc-monitor 已有 SSH 连接隧道(复用 `connect_session` → 自动继承 F45 竞速/F56 跳板 +」。
⇒ 不必动；连带同丙。

---

## ⑤ 🔴 **本件真正的分母**（`§0a` 一个字都没写，而它决定本件有多大）

件文件 `§0a①` 只点了**两个**拨号入口。那是**最底下**那两条 russh 原语，**不是**「谁在拨号」。
量具 `evidence/K-P6-dial-census.py`（输出 `.out`）逐处普查了整条链：

```
client::connect(:573) / client::connect_stream(:687)      ← §0a① 点的两处
  └── connect_session(:701)          调用 7 处 / 3 份文件
        ├── connect_and_exec(:1304)          调用  1 处 /  1 份
        ├── connect_and_exec_cmd(:2078)      调用 18 处 / 11 份   ★ 全仓最宽
        ├── connect_and_exec_capture(:2252)  调用  1 处 /  1 份
        └── sftp::connect_sftp(:47)          调用 14 处 /  4 份
```

**层1 + 层2 的调用点散在 14 份文件里**：
`account_usage.rs`、`acct_iso_deploy.rs`、`cc_bus.rs`、`ccm_probe.rs`、`hooks_diag.rs`、
`mcp.rs`、`port_forward.rs`、`pubkey.rs`、`remote_branch.rs`、`remote_history.rs`、
`sftp.rs`、`sftp_pool.rs`、`ssh_source.rs`、`tmux.rs`。

⚠ **这 14 份里，`russh` 代码态命中是 0 的有 11 份**（`russh` 代码态 >0 的只有 3 份：
`ssh_source.rs` / `port_forward.rs` / `sftp.rs`）；11 份里更有 **7 份连 `russh` 这个子串都没有**
（`acct_iso_deploy.rs` · `cc_bus.rs` · `ccm_probe.rs` · `hooks_diag.rs` · `pubkey.rs` ·
`remote_branch.rs` · `remote_history.rs`），另 4 份只在注释/字符串里提过
（`account_usage.rs` · `mcp.rs` · `sftp_pool.rs` · `tmux.rs`）。
⇒ 只看 `russh` 这个词的判据（`KP6D3①`）**看不见这 11 份**，而它们每一个都在往外拨。
这正是 `KP6D3` 自己写的「**判据要断拨号这件事，不是断一个词**」那句话的实测形状。
（分母 = 上面那 14 份；口径 = 量具② 的「调用」档 ∩ 量具① 的严格代码态，两把尺子的 `--json` 现打对拍。）

⇒ **`KP6D3①` 的判据面必须是「拨号扼流点的调用图」（本量具那张表），不是「`russh` 的词频」。**
`russh` 归零可以靠把 `connect_session` 包一层实现（`KP6D3` 自己点名的第一条失效路径），
而扼流点调用图**包一层就会多一个名字**，跑一遍就看得见。

---

## ⑥ 另外两条：判据落地前必须先修的题面

### 甲 · `KP6D3②` 按字面**今天就红**（写在主干上，不是本件引入的）

`KP6D3②` 逐字：「不新开第二条私路（**不加旁路 socket、不共享文件、不读对方进程的内存**）」。
而**界面与本机后端今天就在共享文件、并读对方进程的内核元数据**，全部是 `K-P1` 常驻那条路的**会合面**：

- `local_daemon.rs:169` `fn token_path(dir: &std::path::Path) -> std::path::PathBuf {`（attach token，`0600`）
- `local_daemon.rs:174` `fn pid_path(dir: &std::path::Path, port: u16) -> std::path::PathBuf {`（谁在听这个口）
- `local_daemon.rs:336` `fn read_listen_owner(dir: &std::path::Path, port: u16) -> Option<(u32, std::path::PathBuf)> {`
- `local_daemon.rs:1165` `let exe = std::fs::read_link(format!("/proc/{pid}/exe"))`（杀之前核身份，防 pid 复用）

⇒ 判据若照字面写，**落地那一刻就红在既有实现上**，而那不是本件的账。
**建议题面收一格**（PM 裁）：「**控制面与数据面**只走协议帧；**会合面**（口 / token / pid / 身份核对）
今天在文件上，是 `K-P1` 的既有形态，本件不新增第二条**数据**私路」。
按 `brief` 17「题目本身…比该做的宽了一格 ⇒ 不许照字面做完，交回」。**这是我顶回的第 2 处。**

### 乙 · `KP6D3③` 今天是**空真**，而它已经有家了

`KP6D3③`：「`remote-daemon-proto` 不许 `use` 任何 `src-tauri` 的类型」。
现状：`remote-daemon-proto` **不依赖** monitor 那个 crate（依赖里只有 `src-tauri/crates/*` 那 7 个共享 crate + dev-dep `guard-core`）
⇒ `use` 一个 `src-tauri/src` 的类型**在编译期就不可表示**，判据无论怎么写都恒绿。
而这条性质**已经有住址**：`src-tauri/src/cross_half_edge_registry.rs`（`F20`），它钉两件事——
①每条跨半边（`include_str!`）都要登记；②**没有一条边长在生产段**（模块头注逐字
「生产段一旦出现跨半边的 `include_str!`，`cargo build` 就真的咬住了，本条当场红」）。
⇒ **别再写第二份**（`brief` 13b：一个闭集只许有一个住址）。`KP6D3③` 应改写成
「本件不新增跨半边**生产段**依赖，读数落 `cross_half_edge_registry`」。

---

## ⑦ `KP6D4` 的文案面基线（本轮零变化，先把分母钉下来）

拨号那条路上**用户看得见的词汇是一个闭集**，且它已经有一个机器盯着的住址：

- Rust 侧 `src-tauri/src/ssh_source.rs:422 pub enum ConnectStage {` —— **6 个变体**
  `Dialing` / `HostKey` / `Failed` / `Won` / `Auth` / `Established`
- 阶段粗分类 `:442 pub fn classify_stage(err: &str) -> &'static str {` —— **4 个标签** `tcp` / `timeout` / `hostkey` / `other`
- 生成物 `src/generated/ConnectStage.ts`（`ts_rs` 导出）⇒ **门禁第二格 `generated` 直接盯着它**
- 前端消费点 `src/settings/machine-card.ts:121 export function describeStage(st: ConnectStage): {`，
  其中 `:141` 逐字注释「穷尽性兜底——未来新增 ConnectStage 变体时编译期(never)即报错」

⇒ **`KP6D4` 的「文案面读数」不必另造判据**：改动这 6+4 个之中任何一个，
`generated` 那一格与 TS 的 `never` 穷尽性检查**两道**都会响。本轮**这十项逐字未动**（零源码改动，见 `⑧`）。

---

## ⑧ 死值验 —— 「这把尺子更准」这句话自己也被切了六刀

`KP6D1` 的 acceptor 是 `实测`。我的核心主张是「严格尺子比粗尺子准，准在**标识符边界**与**词法状态**两处」，
那句话本身是个读数 ⇒ 也要有刀。量具 `evidence/K-P6-ruler-mutations.py`，输出 `.out`。

**台子**：把那 10 份含 `russh` 的文件拷进一次性临时 git 仓（`/tmp/K-P6-ruler-mutations-<pid>`），
**每一刀一份新副本**（`brief` 12c），**被测树一个字节不改**（跑完 `git status` 只见 `evidence/` 的新增）。
⚠ 台子只装 10 份文件 ⇒ 台子上 `connect_and_exec_cmd` 调用数是 **9**，全树是 **18**：**分母不同，不是漂移**。

**基线（台子上）**：严格 code=21 · 严格代码行=21 · 粗代码行=32 · `russh_sftp` code=8 · 调用=9

| id | 锚点 · 命中 · 切第几处 | 真实 Δ | 判 |
|---|---|---|---|
| R1 `sftp_pool.rs` 一处 `russh_sftp::protocol`→`russh::protocol` | 锚点 `russh_sftp::protocol::OpenFlags::READ` 命中 **1**，切第 1 处 | 严格code **+1** · 严格代码行 +1 · **粗代码行 0** · sftp_code **−1** | ✅ **单独证明标识符边界那一格有牙** |
| R2 `ssh_source.rs` `use russh::client;` 挪进字符串 | 锚点 `use russh::client;` 命中 **1**，切第 1 处 | 严格code **−1** · 严格代码行 −1 · **粗代码行 0** | ✅ **单独证明字符串态那一格有牙** |
| R3 `port_forward.rs:94` **行尾注释**里 `russh`→`ssh` | 锚点整行命中 **1**，切第 1 处（**一个代码字节没动**） | **粗代码行 −1** · 严格三格全 0 | ✅ **单独证明行尾注释正是两把尺子分岔处**（PM 自己点名的那个粗口径） |
| R4 `mcp.rs` `&russh_sftp::client::SftpSession` 整行注释掉 | 锚点整行命中 **1**，切第 1 处 | 粗代码行 **−1** · sftp_code **−1** · 严格 `russh` **0** | ✅ 粗尺子的对照 |
| R5 `tmux.rs` 末尾加一句代码态 `russh::Disconnect` | 锚点 0 → 1（追加） | 严格code **+1** · 严格代码行 +1 · 粗代码行 **+1** | ✅ **反向控制**：两把都不是恒定值、没有空转 |
| R6 `tmux.rs` 同时加一处**注释态** + 一处**代码态** `connect_and_exec_cmd(` | 追加 2 处字面命中 | 调用数 9 → **10**（Δ+1） | ✅ 注释那处**确实没被数到** |

**六刀里不符 0 刀 · 本轮零 CRASH**（六刀的判定行都在，没有一刀是解析炸了或异常退出）。
**每一刀都断言了「未点名的格子一格都不许动」** —— 初版的期望表漏点了 `strict_code_lines`，
量具**当场把自己判红 4 刀**；那正是这条规矩要买的东西，如实留在量具的头注里。

**`7u`（把实现整个退掉，还有多少条新断言仍绿）**：本轮**没有生产实现可退**（零源码改动）。
退掉的话，能退的只有这四份量具，而它们一退，本文里的读数**一条都不剩** ⇒ `7u` 在本轮**不适用**，
不是「退了还绿」。**如实写，不冒充一次 `7u` 通过。**

---

## ⑨ 本轮盘上状态

- **源码零改动**：本轮只新增 `evidence/` 下 9 份文件 —— 4 份 `.py` 量具
  （`K-P6-russh-ruler.py` 尺子 · `K-P6-dial-census.py` 扼流点普查 ·
   `K-P6-lineno-pins.py` 行号校验位 · `K-P6-ruler-mutations.py` 死值验）
  ＋ 各自的 `.out` ＋ 本文。**`evidence/` 里零 `.sh`**。
- **门禁**（`PB_WS=backend-consolidation .claude/devbox/gate <本树> k-p6`，沙箱，量于本文写作前）：
  `cargo 1438 · generated ok · daemon 587 · npm 1590 · e2e 12/8/264/72 · pb FAIL=0 BROKEN=0 ⇒ GATE: OK`
  —— **与 PM 给的基线九格逐格相同**（本轮没有源码改动，本该相同；贴出来是为了证明这棵树是干净的）。
- **主干可用性**：`track/k-p6` 与基点 `55c7fde` 在**所有源码文件上逐字节相同** ⇒ 主干**完全可用**。
