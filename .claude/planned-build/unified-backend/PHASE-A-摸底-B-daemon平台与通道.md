# Phase A 摸底 B — daemon 的平台依赖、护栏、以及 monitor↔daemon 通道（2026-08-01）

> ⚠ **本文件是 Phase A 的摸底快照（历史留档）。**
> 其中「wire 类型层面就是单向的」「daemon 流模式从不读 stdin」两条，
> 已被 **U6b-1**（daemon 侧入方向）与 **U8a-2a**（monitor 侧发送端）推翻。
> 保留原文不改写 —— 它记录的是当时的实测，改了就看不出结论是怎么演进的。


> 四份摸底之一。**这份是「daemon 能不能变执行者、能不能上 Windows」的可行性底账。**
> 标注 ✅ = 我（主线程）亲自复跑/复读确认。

## 1. ✅ daemon 今天在 Windows 上**编不过** —— 11 个错误，全在一个小块里

我亲自跑（`set -o pipefail`，**RC=101**）：

```
cd remote-daemon-proto && cargo check --target x86_64-pc-windows-msvc --offline --message-format short
```
```
src/watcher.rs:157:18 E0432 unresolved import `std::os::fd`
src/watcher.rs:235:22 E0432 unresolved import `std::os::fd`
src/watcher.rs:156:53 / 166:26  E0433 cannot find `fd` in `os`
src/watcher.rs:161  E0425 ×3  libc::syscall / SYS_pidfd_open / pid_t
src/watcher.rs:237/239/245×2  E0422/E0425  libc::pollfd / POLLIN / poll
error: could not compile `cc-monitor-remote` due to 11 previous errors
RC=101
```

**全部 11 个错误集中在 `watcher.rs:156-166`（`pidfd_open`）+ `211-262`（poll 循环），其余全过。**
✅ 我另行确认 `pidfd_open`（`:156`）**没有任何 `cfg` 门** —— 文件里第一个 `#[cfg(target_os="linux")]`
出现在 `:1418`。

⚠ `main.rs:8-9` 头注写着「the code is cross-platform and compiles + runs a basic file-watch smoke
on Windows」——**这句今天是假的**，被 P2 的 pidfd 打破。

## 2. ✅ 一颗地雷：`pid_alive` 在非 Linux **恒返回 `true`**

`watcher.rs:1417-1429`（我亲自读）：

```rust
#[cfg(not(target_os = "linux"))]
{
    // Non-Linux (Windows compile/smoke only — not the real target): treat as
    // alive so the cross-platform smoke still exercises the pipeline.
    let _ = pid;
    true
}
```

⇒ 谁把 daemon 编到 Windows 上，会得到「**所有会话永远活着**」的静默错误，不报任何警告。
**任何 Windows daemon 方案的第一步都必须先处置它**（改成 `unimplemented!()` 或真实现）。

## 3. 平台依赖三档

### 档 A：Windows 上**根本不需要**（tmux 一整块，归零而非移植）

`watcher.rs:382-405`（四态分类）· `:415-427`（`run_tmux_ls`）· `:447-474`（`query_tmux_server`）·
`:322`（`TMUX_LS_FMT` 双写点）· `:330-334`（`OBS_*` 双写点）· `:269-277` + `tmux_hook.rs` 整文件（276 行）·
SIGUSR1 通路（`main.rs:341-361`，**存在的唯一理由就是服务 tmux hook**）· wire 帧
`TmuxSessions`/`TmuxSessionClosed`（`wire.rs:178-222`，不发即可）· 差分退休（`watcher.rs:503-540`）。

### 档 B：Windows 上**没有等价物 / 要重写**

| 依赖 | 位置 | Windows | 代价 |
|---|---|---|---|
| `pidfd_open` + `poll(-1)` | `watcher.rs:152-167`, `211-262` | `WaitForSingleObject(hProcess, INFINITE)` | 中，语义**据信**一比一（零轮询 + 绑进程实例免疫 PID 复用）。**摸底 agent 明确标注这条是未查实的推测** ⇒ 真机验证必须进第一批 |
| `/proc/<pid>` 存在性 | `watcher.rs:1417-1429` | `OpenProcess`+`GetExitCodeProcess` —— **仓里已有**：`session_map.rs:460-515` | 低（代码存在，但住 monitor 侧） |
| `/proc/<pid>/stat` starttime（jiffies） | `watcher.rs:1435-1446`, `1614+` | `GetProcessTimes` FILETIME | 中。**格式不同**：Windows 上 CC 写的 `procStart` 是 .NET Local Ticks（`session_map.rs:490-499`，100ms 容差）⇒ `add_time_verdict`（`watcher.rs:1483-1516`）要一份 Windows 变体 |
| `/proc/stat` btime | `watcher.rs:1537-1544` | 无直接等价 | 若 procStart 走 FILETIME 直比则**根本不需要** |
| `/proc/<pid>/cmdline` | `watcher.rs:1585-1600` | WMI / PEB 走查 | 高 |
| **`/proc/<pid>/environ`（抠 `CLAUDE_CONFIG_DIR`）** | `accounts_query.rs:350-360` | **无受支持等价物**（只能 PEB 走查，未文档化、跨 WOW64 会碎） | **最硬的一格**：per-session 账号识别在 Windows 上没有 Linux 那条机制 |

### 档 C：已经跨平台，零代价

`notify`/`notify-debouncer-mini`（inotify → `ReadDirectoryChangesW`，`cargo tree --target …msvc`
显示 `windows-sys 0.48` 已在依赖图）· Windows 路径大小写折叠（`watcher.rs:1763-1770` 已写）·
`%USERPROFILE%` 回退（`main.rs:424-428`）· 停机信号（`main.rs:435-465`）。
其余 unix-only 调用（symlink / `getuid` / `PermissionsExt` / `Command::new("sleep")`）**全在 `#[cfg(test)]` 里**。

**两个容易漏的 feature 缺口**（`remote-daemon-proto/Cargo.toml:24-34`）：
tokio features 没有 **`process`**（今天用 `std::process`）、没有 **`net`**（loopback 监听要它，
或改 named pipe）。而 `Cargo.toml:15-19` 明确说版本钉死是为了离线缓存 ⇒ 动 features 要动 lock。

## 4. 护栏逐条

### 4.1 ✅ `readonly_guard` **拦不到「起进程」** —— 不是推理，是既成事实

模式表（`readonly_guard.rs:61-73`）**纯 fs 命名空间锚定**，
无任何 `Command`/`process::`/`spawn` 模式。而 daemon 生产代码**今天就在起进程且护栏是绿的**：
`watcher.rs:416`（`sh -c`）· `watcher.rs:450` · `tmux_hook.rs:107`（`tmux`）。

⇒ **这道门今天就是开的。** 与 STATUS.md 里记的 E49 用户更正一致。

### 4.2 ★ 但有一条护栏**照不到**的语义缺口 —— **要用户裁决**

`doc/INVARIANTS.md:1211` 现措辞：「**daemon 不许改动用户既有数据**；新增文件须 `O_EXCL` 且限于白名单模块。」

`readonly_guard` 是**源码子串扫描**，只看得见 daemon 自己 `fs::write`。但 daemon 一旦起
`ccm`/`claude`，**用户既有数据一定会被改**（CC 写 jsonl、重写 pidfile；`shared/ccm:44-46` 头注明写
`--tmux` 会顺带写 `~/.claude.json` / `~/.codex/config.toml`）。

⇒ **机器护栏放行，散文铁律的字面意思不放行，CI 不会红。**
必须显式裁决「间接写算不算」并改掉其中一份文档 ——
否则这条铁律退化成「只要隔一层 fork/exec 就绕过了」。

### 4.3 `no_timer_guard` —— 对「执行者」是**真**约束

禁用表（`no_timer_guard.rs:46-57`，运行时拼字符串防自指）：`thread::sleep` · `time::sleep` ·
`recv_timeout` · `time::interval` · `Instant::now` · `Duration::from_secs`。
外加更狠的一条（`:137-155`）：生产段 `Duration::from_*` 的**处数必须恰好等于登记表条数**，
✅ 我复读确认**登记表今天只有 1 条**（`watcher.rs` 的 `Duration::from_millis(DEBOUNCE_MS)`，`:63-68`）。

⇒ `tokio::time::timeout(Duration::from_secs(5), child.wait())` **同时踩两条**；
换 `from_millis(5000)` 偷渡照样红。**重试退避、限流、「起了会话之后等它出现」一条都写不了。**

**既有先例给了一条不改护栏的路**：`run_tmux_ls` 头注（`watcher.rs:412-414`）明写「**无超时**：
`output()` 是无超时阻塞调用……故**只能在一次性后台线程里调用**，绝不可直接跑在 `watch_loop` 线程上」。
执行者照此纪律（无超时 + 一次性线程 + 主循环只收结果）可不动护栏 ——
**但那意味着执行动作没有超时兜底。**

### 4.4 `build_id_guard` —— 加子命令**必然**触发

`build_id_guard.rs:49-60` 的只追加表 + `:108-142` 的两条断言 ⇒ 加任何执行子命令 ⇒ 指纹变新 ⇒ 红
⇒ 唯一出路「bump BUILD_ID + 追加一行」。配套纪律（`main.rs:57-62`、`INVARIANTS:1240-1243`）：
bump 必须与 **re-zigbuild 内嵌二进制 + 更新 `embedded-daemons/*.build_id`** 一套做，否则是「半 bump」（更糟）。

### 4.5 §26 死循环护栏 —— 加 flag 也有机检

`main.rs:190-215` `every_capability_token_is_strippable`：`CAPABILITIES` 每个 token 必须有
`split_stream_flags`（`:172-177`）的剥离分支，否则 CI 红。
**这是埋死循环的地方**（flag 落进 query 分支 ⇒ daemon 打印结果退出 ⇒ monitor 无 hello 死循环）。

### 4.6 ★ 一条盲区（对形态乙很关键）

`readonly_guard.rs:230` 与 `no_timer_guard.rs:88` **都只扫 `env!("CARGO_MANIFEST_DIR")/src`**
（= 只扫 `remote-daemon-proto/src/`）。
⇒ **把执行器逻辑挪进共享 crate，两条 daemon 护栏对它完全是瞎的。**
这既是形态乙的成本优势，也是它的风险 —— 要么把护栏扩过去，要么明确承认那份代码不受这两条铁律管。

## 5. monitor ↔ daemon 通道

### 5.1 wire：9 种帧，**类型层面就是单向的**

`wire.rs:36` —— `#[derive(Debug, Clone, Serialize)]`，**只有 `Serialize`，没有 `Deserialize`**。
9 帧：`Hello` · `Line` · `SessionAdded` · `SessionStatus` · `SessionRemoved` · `TurnEnd` ·
`TmuxSessionClosed` · `TmuxSessions` · `Overflow`。
monitor 侧刻意**不 import** 这个 crate，用 `serde_json::Value` + 读 `kind` 的 schema-agnostic 解析
（`ssh_source.rs:1654-1658`）。

### 5.2 **没有下行命令通道** —— 三条独立证据

1. daemon 流模式**从不读 stdin**：全仓 `stdin` 命中里唯一读的是 `resolve_query.rs:111`
   （一次性 `--resolve`）；`main.rs:302-377` 流模式只有 `BufWriter::new(stdout())`。
2. monitor **从不往那条 channel 写**：`ssh_source.rs`（5170 行）grep `write_all|\.write|send_data|
   channel.data` **零命中**；`AsyncWriteExt` 唯一用处是 `:3843` 的 `shutdown()`
   ——那是 `probe_daemon` 的**半关闭发 EOF**，不是发命令。
3. 流式连接就是一次 exec 读 stdout：`connect_and_exec`（`:892-909`）。

### 5.3 一次性查询：**每条都新开一整条 SSH 连接**（不是新 channel）

`run_list_query`（`remote_history.rs:45`）→ `connect_and_exec_cmd`（`ssh_source.rs:1540`）→
`connect_session`（`:605`）：完整 TCP + 握手 + host key + 鉴权，每次都做。**没有 exec 连接池**
（唯一复用 `exec_on_session` `:2699` 只服务 daemonless 轮询）。

今天在用的一次性子命令：`--search`（`remote_history.rs:94`）· `--usage`（`:155`）·
`--list-accounts`（`accounts.rs:229`）· `--fork-session`（`remote_branch.rs:76`，走 capture 变体）。
**`--resolve` 在 cc-monitor 里 0 个调用点。**

### 5.4 唯一已被证明的下行载荷形态

`--resolve` 的 exec 模型（`resolve_query.rs:13`）：
> **exec 模型**：1 exec = 1 请求 1 响应 1 退出、天然 1:1，**无 request-id**；超时 = 客户端杀 exec。

它天然回避了「零定时器」（超时在客户端）和「背压」（一次一条）。
但 `resolve_query.rs:15` 也自称 **「advisory not owning（§5④）：只返命令串、daemon 零 handle、
绝不执行后端」**，且 caps 四个名**逐字复用 aterm 的 `SessionCapabilities`**、契约锁死在
`daemon-协议-v1 §3`（2026-07-18 冻结）。⇒ **让 daemon 执行 = 对 ADR-01 的显式反转 + 两仓 lockstep。**

## 6. 「会话容器是一个维度」—— 已经是一等字段，不是要新造

`src/launch-plan.ts:22-26`：
```ts
export type TmuxMode = "create-or-attach" | "send-into" | "attach-only";
export type LaunchContainer =
  | { kind: "none" }
  | { kind: "tmux"; name: string; nameQuoting: "raw" | "quoted"; mode: TmuxMode };
```
**`{kind:"none"}` 不是理论值，`launch-requests.ts:45` 今天就在产。**
兜底渲染器（`launch-render-fallback.ts:78-110`）对 `none` 渲染 `<envOps>cd '<cwd>' && <argv>`
——**「没有容器，就是一个进程」这条路今天就通**。

但：**`INVARIANTS §38`（`:926-988`）已决策 `container` 维持硬编码一等字段、不进
`LAUNCH_DIMENSIONS`**，判据第 1 条正是「两个渲染器的主体控制流本身要长出新分支 ⇒ 不该进注册表」。
⇒ 加 Windows 那格 = 往判别联合加 variant + 两个渲染器各加分支，**这条路是设计好的**。

⚠ 但 `SessionBackend`（`session-backend.ts:45-71`）三个方法全是 tmux 形状，
其头注（`:15-19`）自陈撑不住阶段②：
> abduco/dtach **没有 `send-keys`**，本接口 `createRunAttach({quotedPayload})` 的「打字载荷」模型是
> tmux 特有的……阶段② daemon 在场后，取命令方式从「同步返回 shell 串的 builder」转成「问 daemon 的
> RPC 句柄」（异步、错误面变化）——**调用点届时要按 RPC 重塑，不是零改动换 const**。

**这个接口要重设计，不是加实现。**

## 7. 三种形态的结构性代价

### 形态甲：Windows 原生 daemon，本机 loopback
- 修 11 个编译错误（中）· Windows 判活（低，代码已存在于 monitor 侧）· procStart 两种格式（中）·
  **per-session 账号识别无解** · 修 `pid_alive` 地雷（必做）· 传输层要 loopback/named pipe（中）·
  **部署链整条断**（`sftp.rs:408-476` 只走 SFTP；`is_safe_remote_daemon_path` 要求 `p.starts_with('/')`
  `:538-551`；`build.rs::embed_daemons` 硬编码 musl 两 arch `:225-276`）。
- **要撤销「daemon 刻意不在 workspace」这条决定** —— `remote-daemon-proto/Cargo.toml:7-9` 逐字：
  「a workspace would pull this Linux-only daemon into the Windows CI `cargo test --all` and break
  the build」；`branch-core/src/lib.rs:14-20` 又写一遍；`ci.yml:148-150` 按这条分了 job。
- 三条 daemon 护栏**扫全文、不认 `cfg`** ⇒ `#[cfg(windows)]` 里的代码照样被扫。

### 形态乙：daemon 只在 Linux；Windows 由 monitor Rust 侧执行；共享「计划内核」crate
- **仓里已有跑通的先例**：`src-tauri/crates/branch-core`（`lib.rs:1-24`）——
  crate 住 monitor workspace 内，daemon 单向 path dep 依赖它，「依赖不会反向制造 workspace 成员关系」。
  头注还逐条论证了为什么不用「复制 + 漂移守卫」。
- 要动 `INVARIANTS §40` 的「Windows 例外」段（`:1077-1084`）：收编还是继续挂例外，要表态。
- **盲区**：共享 crate 不在两条 daemon 护栏扫描范围内（§4.6）。

### 形态丙（三个变体）
- **丙1**：`daemonless` per-host 模式**已存在并已发布**（`ssh_source.rs:95-100` + `:2607-2870`，
  有明写降级能力集 `:2812`）。这是「必须装 daemon」要处置的**存量**。
- **丙2**：daemon 继续当 advisor，把 `--resolve` 扩到 create/attach/kill 的完整 CommandPlan，
  执行永远在客户端。**零护栏冲突**（不加执行 ⇒ 不碰间接写、不需要超时 ⇒ 不碰零定时器）。
- **丙4**：把「Windows 那格」当 **transport** 而非 container。
  `LaunchPlan.transport` 今天已是 `{kind:"local"}|{kind:"ssh"}`（`launch-plan.ts:97`），
  `tryRenderCli` 用它做第一道闸（`launch-render-cli.ts:76`）。两条轴的正交性是类型事实。

### 丙3（不是形态，是冲突预警）
**issue #82**（`tmux -C` 控制模式住进 daemon）与本方向**争夺同一片 daemon 表面**，
且方向相反（它是**加深** tmux↔daemon 耦合，本计划是把容器抽象出去）。
它自陈「需解封「daemon 零改」约束」。⚠ 其正文引用的 `TMUX_EMIT_INTERVAL = 8s`
**已在 P5 被删**（`INVARIANTS:454`）—— **该 issue 的前提部分已过期，先更新再决策。**

## 8. issue #48 owner 已钉死的两条不可逆决策（2026-07-13）

1. 会话注册表若发持久编号并被引用/落盘 → **必须 opaque + 稳定 id**（否则重蹈 `enc(cwd)`）。
2. **`session.*` 协议现在就要盖 `version` 字段 + 能力协商**，别等远端部署后再补。

与 `INVARIANTS §28`（自造持久身份护栏，`:513-556`）同族。**这是单向门，事后改不回来。**
