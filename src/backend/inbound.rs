//! U6b-1：**流式连接上的入方向**——命令信封的读取、分派与取消。
//!
//! # 这一层归哪
//!
//! §1.1 的第二条线是 `observe/`（读）vs `control/`（做）。入方向有两层，**分开归**：
//!
//! - **传输层**（本文件）：读行、解析信封、长度上限、回显 `id`、管取消登记表。
//!   它跟 `wire.rs` 是同一类东西（协议管道），**不属于读也不属于做**，所以放顶层。
//!   塞进 `control/` 会让「control = 做事」那条线变浑。
//! - **每条命令的处理器**都**不在本文件里**。本文件只持有命令表、调过去。
//!   今天它们住两处：会改变世界的那些住 `control/`；
//!   ⚠〔步 `24f` 第二刀 09-20〕`files-read` 那四条住顶层 `files/` ——
//!   它们整族纯读，塞进 `control/`（=「做事」）会把那条线弄浑，
//!   而 `observe/` 又被下面那条硬约束挡着。理由全文在 `REGISTRY` 上它们那一段。
//!
//! ⇒ 依赖方向 `inbound → control`，与既有的 `observe → control` 同向，
//! `layering_guard` 的判据不需要放宽。本文件**不许出现任何 `observe::`**（读面的事不归它）。
//!
//! # 为什么需要入方向（实证，不是推测）
//!
//! 没有它已经在制造绕路：
//! - **`--tmux-notify` 存在的唯一理由**就是 tmux hook 子进程没法给正在跑的后端发消息，
//!   只能新起一个进程、校验身份、发信号。
//! - **`--resolve` 为一次极小的 RPC 单开一整条 SSH exec。**
//!
//! 载体是现成的：monitor 那头拿的是 `russh::ChannelStream`，**双工**，
//! 而 `ssh_source.rs` 里 `stdin` 零命中 —— 那半条通道从来没人用过。
//!
//! # 信任边界
//!
//! 出方向后端是唯一写者；入方向它变成**读取不可信输入**的一方。三条硬约束：
//! 单行长度上限（不缓冲、不 OOM）· 未见 Hello 之前不许有 stdin（时序，机检钉住）·
//! 坏行只回错误、**绝不结束进程**。

use crate::wire::{Frame, Request};
use copy_core::copy_text;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

/// 单行上限。超过即整行丢弃 + 回 `line_too_long`。
///
/// 取值与 `control/resolve_query.rs::MAX_RESOLVE_STDIN` 同一量级（1 MiB）——
/// 那条是审计 security-重要② 加的，理由相同：无界读遇超大输入 = 无界堆分配
/// （Pi 级设备 OOM）。命令信封比 `ResumeSpec` 还小，1 MiB 已是极宽松的上限。
///
/// # ★ 上限必须在**读的时候**生效，不能读完再判
///
/// 第一版用 `BufReader::split(b'\n')`，那是**无界 `read_until`**：它先把整段读进内存，
/// `handle_line` 才看 `raw.len()`。D 审计实测：喂 512 MiB 无换行的流 ⇒
/// **RSS 从 6 MiB 涨到 518 MiB**，而它照样回了一条 `line_too_long`「看起来对」。
///
/// 也就是说：常数抄了先例，**机制没抄** —— `resolve_query` 用的是
/// `stdin().take(MAX_RESOLVE_STDIN)`，真·先取上限再读。
/// 而功能自述的验收判据里逐字写着「超长单行 ⇒ 进程存活、**内存不涨**」，
/// 那句当时是**没有任何测试的假声明**。
///
/// 现在是 `fill_buf`/`consume` 手搓：超限之后**只找换行、不再往 buf 里塞字节**，
/// 整行的内存占用与行长无关。
pub const MAX_LINE_BYTES: usize = 1 << 20;

/// 应答通道容量。**刻意与出方向的 `CHANNEL_CAPACITY`（10_000）分开**。
///
/// 出方向丢一条**内容帧**是可恢复的（行还在远端 jsonl 里）；⚠ **这句话此前写成「丢一帧可恢复」，
/// 那是假的** —— `session_added`/`session_removed`/`tmux_session_closed` 是一次差分的结果、
/// 别处不存在（audit-0805 B-3）。那半今天靠 `Overflow.lost` 带身份让客户端重同步；
/// **丢一条应答会让客户端永远等下去**。两者混在同一个通道里，实时行的洪峰会把应答挤掉。
/// 所以给应答一条独立的小通道，writer 两边都收。
pub const REPLY_CHANNEL_CAPACITY: usize = 256;

/// U6b-2：本 backend **接受的命令集**，随 `hello` 上线（`commands` 字段）。
///
/// 能力协商此前只有出方向那一半（`capabilities` 说「我认识哪些流 flag」）。
/// 入方向同样需要：客户端得知道发什么过去才有人接，否则只能试错。
///
/// **这是单一真相源** —— `hello` 从这里取值，`dispatch` 必须恰好处理这些。
/// 两者由 `inbound_structure_guards.rs::the_commands_mirror_matches_the_registry` 钉住，不许各写各的。
/// ⚠ 09-03 订正〔`K-R19` 摸底逮到，PM 自己落 —— 本文件 PM 持有，派不出去〕：
///   这里原先写的是 `hello_commands_match_the_dispatch_table`，**那个符号全仓零定义**
///   （`U8a-2d` 换掉的），而同一份文件的 `mod tests` 里自己写着「上一版是 …」——
///   **一份文件里，一处当现状说，另一处说它是历史。**
pub const COMMANDS: &[&str] = &[
    "accounts-list",
    "accounts-sessions",
    // 〔C4c · 第四波 4B〕换号前的信任预检（替掉最后两条仍逐次拨号的 `--account-trust*`）。
    "accounts-trust",
    // 〔LOC1a · 第四波 4D〕这台机器的 `cc-acct-iso` 两问（本机那两条从 exec 一次性后端改走 `<local>` 长连接）。
    // 〔DUP2 · J4〕一个 cc-acct-iso 步骤在终端里要跑的那一行（预览 · 弹终端都问它；界面零拼 shell 串）。
    "acct-iso-cmd",
    "acct-iso-shellinit",
    "acct-iso-status",
    "apikey-key-set",
    "apikey-read",
    // 〔US1 · 第四波 4D〕界面「这几个号在这台的表里有没有行 · 这台的中转在不在」（成品，界面经 `chan.call` 直接问）。
    "apikey-routing",
    // 〔AS2 · 第四波 4B · V113〕资产目录（后端自有状态，第四层）：现扫 ＋ 记 · 并进别处的整份。
    "assets-catalog",
    "assets-catalog-merge",
    // 〔AS2〕本机常驻后端沿池里那条 SSH 拉 / 并 / 推远端的目录（事件触发：连上 · 看机器页）。
    "assets-sync",
    // 〔GAP1 · `设计/15 §4.7 S1`〕这台后端自己的 stderr 诊断文件（尾部，只读）。**是新命令** ⇒ `build_id_guard` 红是预期的。
    "backend-log",
    "bus-broadcast",
    // 〔SH1 · V136〕只读看一个 agent 收件箱的尾巴（转调 `cc-log`，不推已读位置）。
    "bus-inbox",
    "bus-kill",
    "bus-list",
    "bus-send",
    "bus-spawn",
    "bus-state",
    "cancel",
    "capture-pane",
    // 〔W5-ALIAS · 第五波先行〕别名预览：一条别名的预置参数 → `ccm --print` 那一行（`设计/71 §2.3`）。
    "ccm-print",
    // 〔E2 · `96 §7.2.2`〕这台的 `ccm` 会哪些（与 `ccm --ccm-probe` 同一份）：monitor 远端那一跳改问这里，不再进交互 shell 查 `PATH`。
    "ccm-probe",
    "exit-policy-read",
    "exit-policy-set",
    "files-browse",
    "files-chmod",
    "files-commit-text",
    "files-commit-upload",
    "files-copy",
    "files-create",
    "files-delete",
    "files-delete-session",
    // 〔FILES2 · 第四波〕解压（`设计/60 §6.2` · §7 第 9 条 Q3）。**是新子命令** ⇒ `build_id_guard` 红是预期的。
    "files-extract",
    "files-find",
    "files-home",
    "files-index-rebuild",
    "files-index-status",
    "files-ls",
    "files-mkdir",
    "files-peek",
    "files-put",
    // 〔FILES2 · V152〕读族第十条：按字节寻址分块读回。**是新子命令** ⇒ `build_id_guard` 红是预期的。
    "files-read-chunk",
    "files-read-text",
    "files-rename",
    // 〔W5-FILES · 第五波〕读族第九条：算目录大小（`设计/60 §6.2`）。**是新子命令** ⇒ `build_id_guard` 红是预期的。
    "files-size",
    "files-stage-chunk",
    "files-stat",
    "files-write-text",
    "footprint-probe",
    // 〔C4d · 第四波 4B〕历史注解（星标 / 改名 / 隐藏 / 上次账号）的读写者换成本机常驻后端（第四层；文件原地不动）。
    "history-annotate",
    // 〔STC · `设计/90 §4` 阶段 C〕会话事实出成品（分叉血缘 · 改动文件集 · agent 列表 · 最新 usage）。
    "history-facts",
    "history-find",
    "history-forget",
    "history-index",
    "history-last-accounts",
    // 〔CF2 · 第四波 4B〕按行号取回一段（不依赖骨架索引，`history_query::read_lines`）。
    "history-lines",
    "history-projects",
    "history-read",
    // 〔U4b · 第四波〕这条会话的记录还在不在（resume 一跳先问，`设计/01 §6.2` 最后一条）。
    "history-record",
    "history-search",
    "history-sessions",
    "history-subagents",
    "history-tail",
    "history-user-inputs",
    "kill",
    "launch",
    // 〔US1 · 第四波 4D〕「这个号这一发走哪、注入什么」（上游选择出成品，`设计/20 §3.2` 那张表搬进后端）。
    "launch-endpoint",
    // 〔SR1a〕链路四条（`dial/link.rs`）：本机常驻后端替 monitor 持有并复用到各远端的 SSH 连接。
    "link-close",
    "link-credit",
    "link-data",
    "link-open",
    // 〔SH1 · V137〕MCP 列表出成品（读法住适配层那一格 `agents::Adapter.mcp`）。
    "mcp-read",
    // 〔AS1 · 第四波 4B〕MCP 资产同步的判定（只读；写经文件管理那一面 `files-put`）。
    "mcp-sync-plan",
    // 〔RM1c · 第四波〕代码全景（V108 选 B）：后端经插件口起独立小程序，只说查询语义。
    "panorama",
    "ping",
    "plugins-marketplaces",
    // 〔DEL〕`relay-ensure` / `relay-status` 删了：远端中转住那台的常驻后端里（V139），不再起脱离的 `--relay`。
    // 〔C4d · 第四波 4B〕本机后端的可达表：monitor 在每台远端流握手那一刻交「怎么够到那台」（只登记）。
    "remote-reach",
    "resolve",
    // 〔RESYNC · V149〕手动对齐（`resync_face`；本体 `observe/watcher.rs::resync`）。
    "resync",
    // 〔LOC1a · 第四波 4D〕分叉（`fork_write`，本 crate 唯一的 `O_EXCL` 新建写口）：本机远端同一条长连接。
    "session-fork",
    // 〔AS2〕skill「装到这台」：来源那台读 · 要被写的那一台判（都只读；写经 `files-put`）。
    "skill-install-plan",
    // 〔SU1 · 第四波 4C · V116〕skill 装记录（第四层）：装完记下写了哪几个 · 卸掉的摘掉。
    "skill-install-record",
    // 〔SU1〕这台记着的、从别处装来的 skill · 卸的判定（都只读；删经 `files-delete` 带 `expect`）。
    "skill-installs",
    "skill-read",
    "skill-uninstall-plan",
    "tasks-list",
    // 〔SH1〕列这台的 tmux 会话（原样行；monitor `list_remote_tmux` 那条拨号 shell 退役）。
    "tmux-list",
    // 〔SR1b〕传输四条（`control/transfer.rs`）：传输台住本机常驻后端，SFTP 跟其它 SSH 同一条连接。
    "transfer-download",
    "transfer-start",
    "transfer-stop",
    "transfer-upload",
];

/// 在跑的命令登记表：`id` → 取消句柄。
///
/// `id` 是客户端给的**不透明串**——backend 不解析、不校验格式、只当 map 的键和回显值。
/// 谁生成谁负责唯一。backend 自己发号的话重连后号段会撞（同 F90「不许拿会变的东西当持久键」）。
type Running = Arc<Mutex<HashMap<String, InFlight>>>;

/// 一条在跑的命令。**`cancellable` 不是装饰** —— 见 [`Disposition::SpawnBlocking`]。
struct InFlight {
    abort: tokio::task::AbortHandle,
    cancellable: bool,
}

// ══════════════════════════════════════════════════════════════════════════
//  〔HX1 · 4D〕**退出之前先排空停不下来的那一档**（`调研/第四波记录/HX1.md` §1）
// ══════════════════════════════════════════════════════════════════════════
//
// 🔴 出处：E §E2 ＋ 主会话 D-a「后端收 SIGTERM 先排空在飞写（有上限）再退」。
//   此前三个退出口（stdio 写者断 / 收信号 / 最后一个客户走了且退出行为是「结束」）都是当场 `exit(0)`，
//   把 `spawn_blocking` 里正在写的那一条连线程一起带走 ⇒ 存盘剩半份、递归删删一半、tmux 键入一半。
//
// 🔴 **人群 = 在飞的 `Run::Blocking`，一条不多一条不少**：那一档本来就是「开跑之后停不下来」的类型级声明
//   （[`Disposition::SpawnBlocking`] 头注）——正是它会被做成半截；`Run::Async` 那一档本来就随时可能被 `cancel`
//   打断，每个 await 点上都得是安全的。⇒ 不另立「哪条算写」的分类表（那是一张会漂的第二份真相）。
//
// 🔴 **「有上限」**〔主会话 4D 裁 HX1 拍板项 1，按 `INVARIANTS §48.2`「脱离后不留僵尸」〕：后端**自己**兜一个
//   退出排空期限 [`DRAIN_DEADLINE`]（30 秒）—— 到点仍没排空 ⇒ 记一行说哪几条没做完，然后退。
//   这是后端零定时器（`no_timer_guard`）**唯一**让位的地方，登记在那张表的 `REGISTERED_EXIT_DEADLINE`（恰好一行）。
//   为什么非它不可：远端后端在 SSH 断开那一刻没人叫它退、也没人给上限 —— 阻塞在一个挂死的文件系统上的那一条
//   会把进程无限期留下（一个没有宿主的后端进程）。叫它退的一方仍可以更早：**第二次停机信号 = 立刻退**；
//   机器页「停」等得比它久一点（〔STOP〕一次性子命令 `--resident-stop` 的宽限期 `control/resident.rs::STOP_GRACE_MS`，35 秒），好让后端先把「哪几条没做完」说出来再退。

/// 退出排空期限（**唯一**一个会让后端自己醒来的构件，只在收场时装一次）。
///
/// 取值 30 秒：在飞的阻塞写（存盘 ≤ 8 MiB · 递归删 ≤ 10 万条 · 一次 tmux）在正常盘上都是秒内；30 秒是它们的一个量级以上，
/// 超过它多半是卡在挂死的文件系统 / 一个不回话的子进程上，再等也等不来。大文件同机复制（`files-copy`，不可取消）
/// 可能超过它 —— 那一条会被列进「没做完」里说出来。
pub(crate) const DRAIN_DEADLINE: std::time::Duration = std::time::Duration::from_millis(30_000);

/// 退出闸：在飞的阻塞命令（带名字）＋ 关没关。进程里只有一个（[`DRAIN`]）；判据另造实例。
pub(crate) struct Drain {
    /// 锁只在一句里活着，绝不跨 await。
    state: Mutex<DrainState>,
    /// 在飞的归零那一刻叫醒等的人（[`Drain::drained`]）。
    idle: tokio::sync::Notify,
}

struct DrainState {
    /// 下一张票的号。
    next: u64,
    /// 在飞的：票号 → 「命令名（id=…）」—— 到点没排空时要说出是哪几条。
    live: std::collections::BTreeMap<u64, String>,
    closed: bool,
}

/// 一张票：拿着它的阻塞命令还在跑。**票 move 进阻塞闭包**，闭包跑完（成功 / 失败 / panic 展开）才落 ——
/// 外层 async 任务被 `abort` 不影响它（阻塞线程还在跑，票还在它手里）。
pub(crate) struct Ticket(&'static Drain, u64);

impl Drop for Ticket {
    fn drop(&mut self) {
        let now_idle = {
            let mut g = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
            g.live.remove(&self.1);
            g.live.is_empty()
        };
        if now_idle {
            self.0.idle.notify_waiters();
        }
    }
}

impl Drain {
    pub(crate) const fn new() -> Self {
        Drain {
            state: Mutex::new(DrainState {
                next: 0,
                live: std::collections::BTreeMap::new(),
                closed: false,
            }),
            idle: tokio::sync::Notify::const_new(),
        }
    }

    /// 取一张票（`what` = 这一条是什么，到点没排空时说给人听）。闸关了 ⇒ `None`（调用方回 `shutting_down`，一个字节不动）。
    pub(crate) fn enter(&'static self, what: String) -> Option<Ticket> {
        let mut g = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if g.closed {
            return None;
        }
        let key = g.next;
        g.next += 1;
        g.live.insert(key, what);
        Some(Ticket(self, key))
    }

    /// 关闸，回关的那一刻在飞几条。关过再关无害。
    pub(crate) fn close(&self) -> usize {
        let mut g = self.state.lock().unwrap_or_else(|e| e.into_inner());
        g.closed = true;
        g.live.len()
    }

    /// 此刻在飞几条。
    pub(crate) fn in_flight(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .live
            .len()
    }

    /// 此刻在飞的是哪几条（按起跑先后）。
    pub(crate) fn in_flight_names(&self) -> Vec<String> {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .live
            .values()
            .cloned()
            .collect()
    }

    /// 等在飞的归零。先登记等待、再看计数 ⇒ 「看的那一刻还有、紧接着最后一张票落下」那一下不会漏醒。
    pub(crate) async fn drained(&self) {
        loop {
            let woke = self.idle.notified();
            tokio::pin!(woke);
            woke.as_mut().enable();
            if self.in_flight() == 0 {
                return;
            }
            woke.await;
        }
    }
}

/// 本进程的那一个退出闸。
pub(crate) static DRAIN: Drain = Drain::new();

/// 闸关了之后新来的阻塞命令回的**协议级**码（与命令无关、只有本文件判得了 —— 同 `not_cancellable`）。
pub const SHUTTING_DOWN: &str = "shutting_down";

/// 停机信号的监听，**建的那一刻就登记好**（不是等第一次被 poll 才登记 ⇒ 建完之后到的信号不会漏）。
/// 住 `platform::signal`（平台 cfg 只许住那一层）；这里转一手给 `main.rs`。
pub fn shutdown_listener() -> impl std::future::Future<Output = ()> + Send + 'static {
    crate::platform::signal::shutdown_listener()
}

/// 🔴 **流模式后端的收场 —— 三个退出口都走这一个**（`main.rs`：stdio 那条 · 常驻收信号 · 最后一个客户走了且退出行为是「结束」）。
///
/// 关闸 → 说一句在飞几条 → 等它们做完再 `exit(0)`；等的时候**再来一次停机信号 ⇒ 不等了**；
/// **到 [`DRAIN_DEADLINE`] 仍没排空 ⇒ 说出哪几条没做完，退**。
/// `writer`：还在往对端写的那一路（stdio 载体收信号那一形传进来）—— 排空期间照常跑，在飞命令的最终应答照样有机会写回去；
/// 它先结束（对端走了）不影响排空。⚠ 不保证最后一条应答一定写出去了：排空完成那一刻就退（「对面可能已经做了」那一句兜）。
pub async fn exit_after_drain<W>(why: &str, writer: Option<W>) -> !
where
    W: std::future::Future<Output = ()>,
{
    exit_after_drain_within(why, writer, DRAIN_DEADLINE).await
}

/// [`exit_after_drain`] 的本体，期限由参数给（生产恒 [`DRAIN_DEADLINE`]；判据给一个短的，量「到点就退」那一形）。
pub(crate) async fn exit_after_drain_within<W>(
    why: &str,
    writer: Option<W>,
    deadline: std::time::Duration,
) -> !
where
    W: std::future::Future<Output = ()>,
{
    // 先登记第二次信号，再关闸说话 —— 顺序反过来，那句话出去之后、登记之前到的信号就漏了。
    let again = shutdown_listener();
    let left = DRAIN.close();
    if left == 0 {
        tracing::info!("{why} ⇒ 退出（没有在跑的阻塞命令）");
    } else {
        tracing::info!(
            "{why} ⇒ 收尾：还有 {left} 条开跑之后停不下来的命令在跑，等它们做完再退（最多 {} 秒）；\
             新来的这一类命令回 {SHUTTING_DOWN}。再发一次停机信号就不等了",
            deadline.as_secs()
        );
    }
    let writing = async {
        if let Some(w) = writer {
            w.await;
        }
        std::future::pending::<()>().await
    };
    // 〔HX1 · 主会话裁〕后端零定时器唯一让位的一处（`no_timer_guard::REGISTERED_EXIT_DEADLINE`）：只在收场时装这一次。
    let expired = tokio::time::sleep(deadline);
    tokio::select! {
        _ = DRAIN.drained() => {
            if left > 0 {
                tracing::info!("在跑的那几条都做完了 ⇒ 退出");
            }
        }
        _ = again => {
            tracing::warn!(
                "排空时又收到一次停机信号 ⇒ 不等了：还有 {} 条没做完（{}），它们可能只做了一半",
                DRAIN.in_flight(),
                DRAIN.in_flight_names().join("、")
            );
        }
        _ = expired => {
            tracing::warn!(
                "排空期限 {} 秒到了 ⇒ 不等了：还有 {} 条没做完（{}），它们可能只做了一半",
                deadline.as_secs(),
                DRAIN.in_flight(),
                DRAIN.in_flight_names().join("、")
            );
        }
        _ = writing => {}
    }
    // 必须显式 exit（不能让 runtime 自然 drop）：理由全文在 `main.rs::main` 末尾那段 —— stdin 的阻塞读会让 drop 永远等下去。
    std::process::exit(0)
}

/// 起入方向 reader。
///
/// ★ **Hello 必须已经 flush** —— 那不是一条纪律，是一个**参数**：
/// [`crate::wire::HelloFlushed`] 只能由 `wire::write_and_flush_hello` 产出，
/// 拿不到它就调不了本函数。顺序因此**编译期不可表示**。
///
/// U6b-1 曾用一条比较 `main.rs` 里两个字符串字节位置的机检来钉它，
/// D 审计用一次**普通的函数抽取**就绕过了（把调用点包进一个放在文件后段的函数）。
/// 那条机检已随本改动删掉 —— 不可表示之后它是死重量。
pub fn spawn<R>(
    stdin: R,
    replies: mpsc::Sender<Frame>,
    _hello_flushed: crate::wire::HelloFlushed,
) -> tokio::task::JoinHandle<()>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let running: Running = Arc::new(Mutex::new(HashMap::new()));
        // 〔SR1a〕本连接的链路表：随本读循环一起死 ⇒ monitor 走了，它开的链路一条不留
        // （`dial::link::Table` 的 `Drop`）。
        let links = crate::dial::link::Table::new(replies.clone());
        // 〔SR1b〕本连接的传输票表：同上，随本读循环一起死 ⇒ monitor 走了，它开的传输一律撤
        // （`control::transfer::Desk` 的 `Drop`）。
        let xfers = crate::control::transfer::Desk::new(replies.clone());
        let mut rd = BufReader::new(stdin);
        let mut buf: Vec<u8> = Vec::new();
        // 本行是否已经超限。超限之后**只丢字节、不再往 buf 里塞**（O(1) 内存）。
        let mut overflowed = false;
        // 〔F9c · 第四波〕超限那一刻从行首抠出来的 `id`（抠不出 ⇒ 空串，见 [`sniff_id`]）。
        let mut overflow_id = String::new();
        loop {
            let chunk = match rd.fill_buf().await {
                Ok([]) => break, // 客户端关了写半边：正常寿终
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("inbound read failed ({e}); stopping reader");
                    break;
                }
            };
            let (take, done) = match chunk.iter().position(|&c| c == b'\n') {
                Some(i) => (i, true),
                None => (chunk.len(), false),
            };
            if !overflowed {
                if buf.len() + take > MAX_LINE_BYTES {
                    overflowed = true;
                    // 丢之前先看一眼行首（至多 `ID_SNIFF_BYTES`）：buf 此刻就是这一行的开头；
                    // buf 若还是空的（一块就越线），行首就是这一块本身。
                    let head: &[u8] = if buf.is_empty() { &chunk[..take] } else { &buf };
                    overflow_id =
                        sniff_id(&head[..head.len().min(ID_SNIFF_BYTES)]).unwrap_or_default();
                    buf.clear();
                    buf.shrink_to_fit();
                } else {
                    buf.extend_from_slice(&chunk[..take]);
                }
            }
            let consumed = if done { take + 1 } else { take };
            rd.consume(consumed);
            if !done {
                continue;
            }
            if overflowed {
                send(
                    &replies,
                    err(
                        &overflow_id,
                        "line_too_long",
                        &crate::common::contract::malformed(&format!(
                            "request line over {MAX_LINE_BYTES} bytes, dropped whole"
                        )),
                    ),
                )
                .await;
                overflowed = false;
            } else {
                handle_line(&buf, &replies, &running, &links, &xfers).await;
            }
            buf.clear();
        }
    })
}

/// 〔F9c · 第四波〕超长行只看行首这么多字节去找 `id`。
///
/// 整行已经不进内存（[`MAX_LINE_BYTES`] 头注那条「读的时候就生效」），这里多留的只有这一小段，
/// 与行长无关。4 KiB 远够：monitor 发号最长 75 字节、且 `id` 是信封的第一个键
/// （`inbound_client::encode_request` 的字段顺序）；排在它前面的键再长也只是「抠不出 ⇒ 空串」，回到旧行为。
pub const ID_SNIFF_BYTES: usize = 4 * 1024;

/// 〔F9c · 第四波〕从一行**开头的一段**里尽力抠出信封的 `id`（顶层对象里、值是字符串的那一个）。
///
/// 为什么要它：超长行整行丢弃，此前回的 `line_too_long` 带**空** `id` ⇒ 发这一行的调用方等不到
/// 自己的应答，要熬满它自己的预算才超时，看到的是「超时」而不是真原因（`设计/60 §9c.2`）。
///
/// ⚠ 只认**顶层**的 `"id"`：排在前面的键值原样跳过（字符串 / 数 / 嵌套对象与数组都认得），
/// 嵌套对象里的 `"id"` 不算。段不完整、形状不对、`id` 不是字符串 ⇒ `None`（调用方回空串，即旧行为）。
/// ⚠ 它不是 JSON 解析器，也不校验这一行别处合不合法 —— 这一行本来就要被丢弃，只借它的 `id` 回话。
fn sniff_id(head: &[u8]) -> Option<String> {
    // 结构字节按值写（不写成字符字面量）：本仓有几条按文本扫源码的判据，引号与大括号的字面量会搅乱它们的配平。
    const QUOTE: u8 = 0x22;
    const BACKSLASH: u8 = 0x5C;
    const OPEN_OBJ: u8 = 0x7B;
    const CLOSE_OBJ: u8 = 0x7D;
    const OPEN_ARR: u8 = 0x5B;
    const CLOSE_ARR: u8 = 0x5D;
    const COMMA: u8 = 0x2C;
    const COLON: u8 = 0x3A;
    let at = |k: usize| head.get(k).copied();
    let ws = |mut i: usize| {
        while head.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        i
    };
    // 一个字符串（`i` 指在开头那个引号上）⇒ 回收尾引号之后的位置。
    let string_end = |i: usize| -> Option<usize> {
        if at(i) != Some(QUOTE) {
            return None;
        }
        let mut k = i + 1;
        loop {
            match at(k)? {
                BACKSLASH => k += 2,
                QUOTE => return Some(k + 1),
                _ => k += 1,
            }
        }
    };
    // 跳过一个值 ⇒ 回它之后的位置。
    let value_end = |i: usize| -> Option<usize> {
        match at(i)? {
            QUOTE => string_end(i),
            OPEN_OBJ | OPEN_ARR => {
                let mut depth = 0usize;
                let mut k = i;
                loop {
                    match at(k)? {
                        QUOTE => {
                            k = string_end(k)?;
                            continue;
                        }
                        OPEN_OBJ | OPEN_ARR => depth += 1,
                        CLOSE_OBJ | CLOSE_ARR => {
                            depth -= 1;
                            if depth == 0 {
                                return Some(k + 1);
                            }
                        }
                        _ => {}
                    }
                    k += 1;
                }
            }
            _ => {
                let mut k = i;
                while !matches!(at(k)?, COMMA | CLOSE_OBJ | CLOSE_ARR)
                    && !head[k].is_ascii_whitespace()
                {
                    k += 1;
                }
                Some(k)
            }
        }
    };
    let mut i = ws(0);
    if at(i) != Some(OPEN_OBJ) {
        return None;
    }
    i += 1;
    loop {
        i = ws(i);
        let key_end = string_end(i)?;
        let key: String = serde_json::from_slice(&head[i..key_end]).ok()?;
        i = ws(key_end);
        if at(i) != Some(COLON) {
            return None;
        }
        i = ws(i + 1);
        if key == "id" {
            let end = string_end(i)?;
            return serde_json::from_slice(&head[i..end]).ok();
        }
        i = ws(value_end(i)?);
        if at(i) != Some(COMMA) {
            return None;
        }
        i += 1;
    }
}

/// 处理一行。**任何失败都只回一条错误应答，绝不 panic、绝不结束读循环。**
async fn handle_line(
    raw: &[u8],
    replies: &mpsc::Sender<Frame>,
    running: &Running,
    links: &crate::dial::link::Table,
    xfers: &crate::control::transfer::Desk,
) {
    if raw.is_empty() {
        return; // 空行（含 CRLF 的裸 \r 之后）静默跳过
    }
    let req: Request = match serde_json::from_slice(raw) {
        Ok(r) => r,
        Err(e) => {
            // 坏 JSON 时 `id` 无从得知 —— 回空 id，客户端按「上一条没应答」超时处理。
            send(replies, err("", "bad_request", &e.to_string())).await;
            return;
        }
    };
    match dispatch(req, replies, running, links, xfers) {
        Disposition::Done => {}
        Disposition::Reply(f) => send(replies, f).await,
        Disposition::Spawn(req, run) => {
            spawn_handler(req, replies.clone(), running.clone(), run, true).await
        }
        // ★ 同步阻塞处理器：进 `spawn_blocking` 的专用线程池，**不占 tokio worker**。
        //   `cancellable: false` —— `spawn_blocking` 起的活 abort 不了，说实话。
        Disposition::SpawnBlocking(req, run) => {
            // 〔HX1〕取票在起跑之前：闸关了（进程在收场）⇒ 一个字节不动、回协议级 `shutting_down`。
            let Some(ticket) = DRAIN.enter(format!("{}（id={}）", req.cmd, req.id)) else {
                send(
                    replies,
                    err(
                        &req.id,
                        SHUTTING_DOWN,
                        &copy_text("beInbound.drain.shuttingDown", &[]),
                    ),
                )
                .await;
                return;
            };
            let fut = move |r: Request| async move {
                // 票跟着阻塞闭包走：闭包跑完才落（外层被 abort 也照样数着）。
                match tokio::task::spawn_blocking(move || {
                    let _ticket = ticket;
                    run(r)
                })
                .await
                {
                    Ok(res) => res,
                    Err(e) => Err((
                        "handler_panicked".to_string(),
                        copy_text("beInbound.handleLine.internal", &[("e", &e.to_string())]),
                    )),
                }
            };
            spawn_handler(req, replies.clone(), running.clone(), fut, false).await
        }
    }
}

/// 处理器：拿走 [`Request`]，返回一个可以在**独立 task** 上跑的 future。
type Handler = Box<dyn FnOnce(Request) -> BoxFut + Send>;
/// 同步阻塞处理器（见 [`Disposition::SpawnBlocking`]）。
type BlockingHandler = Box<dyn FnOnce(Request) -> CmdResult + Send>;
type CmdResult = Result<Option<serde_json::Value>, (String, String)>;
type BoxFut = std::pin::Pin<Box<dyn std::future::Future<Output = CmdResult> + Send + 'static>>;

/// 一条命令**怎么跑**。
///
/// # 为什么是这个形状（U6b-3，据 D 审计重做）
///
/// 上一版 `dispatch` 是 `async fn`，纪律「处理器不许跑在读循环上」靠一条**扫分派臂文本**
/// 的机检钉。D 审计用三种**普通写法**把它绕过去了，全部 211 passed：
/// 尾随注释（`production_code` 只剥整行注释）· 同一条臂里既 `spawn_handler` 又就地 `.await` ·
/// 或模式 `"cancel" | "drain-everything" =>` 把白名单外的命令一起吞进白名单。
/// 而它们真的堵住了读循环 —— 实测「排在后面的 ping 永远收不到应答」。
///
/// 结论是审计给的：**别再往判据上加正则，让违规不可表示。**
/// `dispatch` 现在是**非 async** 的 ⇒ **分派臂里根本没有 `.await` 可写**。
/// 要跑活，只能交出一个 future 让调用方 spawn。
///
/// 三档的分工：
/// - [`Disposition::Done`]：已经在 `dispatch` 里**同步**做完了。非 async 意味着它做不了会阻塞的事。
/// - [`Disposition::Reply`]：立刻回这一帧，**由调用方 await 发送**（保住背压，不像 `try_send` 会丢）。
/// - [`Disposition::Spawn`]：交给独立 task。绝大多数命令走这里。
enum Disposition {
    Done,
    Reply(Frame),
    Spawn(Request, Handler),
    /// **同步阻塞**的处理器（起进程、扫全库）。走 `tokio::task::spawn_blocking`。
    ///
    /// # 为什么必须与 [`Disposition::Spawn`] 分开（D 设计审计 · 视角 A · P5）
    ///
    /// `launch` 的处理器是同步的，起 tmux 进程会真的阻塞。放在 `tokio::spawn` 上就是
    /// **占住一个 worker**；`main` 是裸 `#[tokio::main]`（worker 数 = 可用并行度），
    /// 单核机器（Pi 那一档，正是本后端的目标机型）上一条在跑的 `launch` 就会占住
    /// **唯一**的 worker —— 而 `writer_task`（出方向帧的唯一出口）和入方向 reader 都在
    /// 同一个 runtime 上。症状是「远端还活着但一句话不说」，极难归因
    /// （观测 watcher 在 `std::thread` 上，不受影响，所以看起来更像网络问题）。
    ///
    /// 分开还有第二个作用：`spawn_blocking` 起的活**abort 不了**。
    /// 这一档因此同时是「这条命令不可取消」的类型级声明，`cancel` 据此回 `not_cancellable`
    /// 而不是撒一条 `cancelled` 的谎。
    SpawnBlocking(Request, BlockingHandler),
}

/// 命令表。**非 async —— 见 [`Disposition`]。**
fn dispatch(
    req: Request,
    replies: &mpsc::Sender<Frame>,
    running: &Running,
    links: &crate::dial::link::Table,
    xfers: &crate::control::transfer::Desk,
) -> Disposition {
    match req.cmd.as_str() {
        // 〔SR1a〕链路四条：要碰**本连接的链路表**与应答通道 ⇒ 与 `cancel` 同一档（硬臂、就地做完）。
        // ★ `link-data` **必须就地**（不 `spawn`）：同一条链路的上行块按到达顺序进队，
        //   交给独立 task 就不再保序。它成功时的应答由上行泵在写进管子之后发（背压）。
        "link-open" => Disposition::Reply(links.open(&req.id, &req.args)),
        "link-data" => match links.data(&req.id, &req.args) {
            Some(f) => Disposition::Reply(f),
            None => Disposition::Done,
        },
        "link-credit" => Disposition::Reply(links.credit(&req.id, &req.args)),
        "link-close" => Disposition::Reply(links.close(&req.id, &req.args)),
        // 〔SR1b〕传输四条：要碰**本连接的票表**与应答通道（进度帧走应答通道）⇒ 同一档硬臂。
        //   开单 / 起跑 / 撤都是就地做完的记账（起跑那一下 `spawn` 两个任务，不 await）。
        "transfer-upload" | "transfer-download" | "transfer-start" | "transfer-stop" => {
            Disposition::Reply(crate::control::transfer::Desk::answer_wire(
                xfers, &req.cmd, &req.id, &req.args,
            ))
        }
        "cancel" => {
            let target = req
                .args
                .get("target")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            // ★ **不可取消的命令要说实话，不许回一条撒谎的 `cancelled`。**
            //
            // D 设计审计（视角 A · P4）实测出的谎：`launch` 的处理器是**同步阻塞**的，
            // `AbortHandle::abort()` 只在 await 点生效，对 `spawn_blocking` 起的活更是空操作
            // ⇒ 客户端收到 `Cancelled`、`CallError::Cancelled`，而**远端的 tmux 会话照样建出来、
            // 载荷照样键入**。那不是措辞问题，是控制面在骗调用方。
            //
            // 处置：登记表记住每条命令可不可取消；不可取消的**留在表里**（它还在跑），
            // 回一条 `not_cancellable`，让调用方知道「这条停不下来，去查它的最终应答」。
            let handle = {
                let mut g = lock(running);
                match g.get(&target) {
                    Some(f) if !f.cancellable => None,
                    _ => g.remove(&target),
                }
            };
            if handle.is_none() && lock(running).contains_key(&target) {
                let _ = replies.try_send(err(
                    &req.id,
                    "not_cancellable",
                    &copy_text("beInbound.dispatch.cannotCancel", &[]),
                ));
                return Disposition::Done;
            }
            if let Some(h) = handle {
                h.abort.abort();
                // ★ `cancel` 的应答一律 `try_send`，**绝不 await**。
                //
                // 它被放进 `MAY_RUN_INLINE` 的理由原本写的是"纯 map 操作，不阻塞"——
                // 那是错的：它要 `send(...).await` 两次。D 审计实测应答通道满时
                // `dispatch` 300ms 都回不来，读循环整个停摆 ⇒ **后面的 cancel 连解析都轮不到**。
                //
                // 那是**控制面被数据面背压堵死**的优先级反转，恰好是"给应答开独立通道"
                // 想解决的那件事。丢一条 cancel 应答，远比堵死读循环便宜。
                let _ = replies.try_send(Frame::Cancelled { id: target });
            }
            // 取消一个不存在的 id 是幂等的、不是错误。
            let _ = replies.try_send(ok(&req.id));
            Disposition::Done
        }
        // U8a-2d：其余命令**一律查注册表**，不再是一串手写臂。
        //
        // 这一步换掉的是 `hello_commands_match_the_dispatch_table` —— 那条机检扫的是
        // 分派臂的**文本**（按 8 空格缩进切），既对 rustfmt 脆，又只能事后比对。
        // 现在名字与处理器绑在**同一个值**里 ⇒ 「声明了却不接」「接了却不声明」
        // 在注册表这一侧不可表示；剩下的只是 `COMMANDS` 那面镜子，由**数据对数据**的
        // `the_commands_mirror_matches_the_registry` 钉住。
        other => match lookup(other) {
            Some(spec) => match spec.run {
                Run::Async(f) => Disposition::Spawn(req, Box::new(f)),
                Run::Blocking(f) => Disposition::SpawnBlocking(req, Box::new(f)),
                // `cancel` 在上面那条硬臂里处理完了，走不到这儿。
                Run::Builtin => Disposition::Reply(err(
                    &req.id,
                    "unknown_command",
                    &copy_text(
                        "beInbound.dispatch.unhandled",
                        &[("other", &other.to_string())],
                    ),
                )),
            },
            None => Disposition::Reply(err(
                &req.id,
                "unknown_command",
                &copy_text(
                    "beInbound.dispatch.unknown",
                    &[("other", &other.to_string())],
                ),
            )),
        },
    }
}

/// 一条命令**怎么跑**。三档，缺一不可：
///
/// - [`Run::Async`]：真异步，有 await 点 ⇒ `cancel` 能在那儿把它打断。
/// - [`Run::Blocking`]：**同步阻塞**（起进程 / 扫全库）⇒ 进 `spawn_blocking` 的专用线程池。
///   ⚠ 它**开跑之后打不断** —— 这一档不是「修好了取消」，是**停止假装能取消**：
///   `cancel` 命中它时回 `not_cancellable`，而不是撒一条 `cancelled` 的谎。
/// - [`Run::Builtin`]：`dispatch` 里的硬臂（`cancel` ＋ 〔SR1a〕链路四条）。它要 `replies`/`running`，
///   与别的命令签名不同 —— 硬塞进统一签名等于给每条命令都递上「自己发帧 / 碰登记表」的能力，
///   而那条性质今天是成立的，不该为了整齐拆掉。**但它仍要在注册表里占一行**，
///   否则「镜子 == 注册表」覆盖不到它。
pub(crate) enum Run {
    Async(fn(Request) -> BoxFut),
    Blocking(fn(Request) -> CmdResult),
    Builtin,
}

/// 一条入方向命令的登记。**名字与处理器绑在同一个值里。**
///
/// `doc_anchor` / `codes` / `fields` **只被护栏读**（`protocol_doc_guard` 与本文件的
/// `structure_guards`）—— 那正是它们存在的理由：把「这条命令的契约面」写成**数据**，
/// 好让机检对着它比。非测试构建里它们确实没有读者，故精确 allow 而不是给整个类型开口子。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct CommandSpec {
    /// 线上命令名。
    pub name: &'static str,
    /// 它在 `src/doc/IPC-PROTOCOL.md` §10 里那一小节的标题**逐字**；`None` = 没有自己的小节
    /// （只要求名字出现在「入方向」节里）。**有 `fields` 就必须有小节** —— 由机检钉住。
    ///
    /// ⚠ **这是约定不是事实**：护栏只能查「标题在、字段名在它下面出现」，查不了写得对不对。
    /// 这不是新增局限，是把 `protocol_doc_guard` 早就登记过的那条局限**局部化**
    /// （从「§10 全节任意反引号」收到「本命令那一小节」），强度只升不降。
    pub(crate) doc_anchor: Option<&'static str>,
    /// 本命令**自己**可能回的 code。**协议级 code 不许出现在这里**（由 R4 的零命中钉住）。
    pub codes: &'static [&'static str],
    /// 本命令 `args` / `data` 的字段名。空 = 无载荷（如 `ping`）。
    ///
    /// ⚠ 它是**手写镜子**，本身就是一个新的漂移源 —— 所以必须再钉一层：
    /// 与真正的解析器/输出构造器实测对拍（`launch_fields_match_its_parser_and_output`）。
    /// 用一个手写清单去证明另一个手写清单是没有意义的。
    pub(crate) fields: &'static [&'static str],
    /// 这条命令**收不收入方向载荷**（CLI 面据此决定读不读 stdin）。
    ///
    /// # ★★ 为什么这是显式的，而不是从 `fields` 派生〔P4f 08-13 实测〕
    ///
    /// 原来 `cli_control::reads_stdin` 写成 `!fields.is_empty()`。那是个**代用品**：
    /// `fields` 的定义是「`args` **和** `data` 的字段名」，而 `kill`/`launch`/`resolve`
    /// 恰好都有输入、`ping` 恰好零字段 ⇒ 代用品当时全对。
    ///
    /// `bus-list` 是第一条**无输入、却有输出字段**的命令 ⇒ 代用品判它要读 stdin
    /// ⇒ **它挂住等一个永远不来的输入**。实测：`--ping` 120ms 回，`--bus-list` 6 秒
    /// 被掐死、一个字都没输出。而 CLI 面正是给第三方 skill 调的。
    ///
    /// ⚠ 这条病仓里**修过一次**（`--ping` 第一版无条件读 stdin，`cli_control` 的头注逐字：
    /// 「问『你活着吗』的那条命令，答案是挂住 —— 所有失败里最坏的一种」）。
    /// 它换了扇门回来，因为守它的判据是**恒真**的（`fields.is_empty()` ⟺ `!reads_stdin`
    /// 两边是同一个表达式，两个分支都不可能红）。
    ///
    /// ⇒ 改成每条命令自己说。真不真由**行为**判据验（`tests/e2e/backend-cc-bus.sh`：
    /// 声明无输入的命令，在 stdin 不关时必须秒回）。
    pub(crate) takes_input: bool,
    pub(crate) run: Run,
}

/// 〔NET2〕撤不动的那几条：阻塞档（`Run::Blocking`）开跑之后打不断，`cancel` 命中回 `not_cancellable`。
/// hello 的 `uncancellable` 就是它 —— 与 `dispatch` 按档位分流读的是同一张表。
pub fn uncancellable() -> Vec<String> {
    REGISTRY
        .iter()
        .filter(|s| matches!(s.run, Run::Blocking(_)))
        .map(|s| s.name.to_string())
        .collect()
}

/// **单一事实源。** `COMMANDS` 是它的镜子，`dispatch` 从它查。
pub const REGISTRY: &[CommandSpec] = &[
    // P4f：cc-bus 的两条基础命令。**转调本机的 cc-bus 命令**，不在后端里重实现总线
    //（用户 08-13 逐字：「细节先按原本的就行」「后面我可能要改ccbus」）。
    // ⚠ 刻意**没有** `bus-recv`：`cc-recv` 会推进已读位置，backend 代读等于把消息从人那里
    //   偷走。「有没有新的」由 `bus-list` 的待读数回答（只读、不消费）。理由全文在 `control/cc_bus.rs`。
    CommandSpec {
        name: "bus-list",
        doc_anchor: Some("#### `bus-list`"),
        codes: &["not_installed", "timed_out", "failed"],
        fields: &["agents", "ccm_sid", "id", "live", "target", "unread"],
        takes_input: false,
        run: Run::Blocking(|_r| crate::control::cc_bus::list_for_inbound().map(Some)),
    },
    // 〔C4e · 第四波 4C〕广播：列名单（同 `bus-list` 那一个函数）→ 挑在线的 → 逐个投递（同 `bus-send` 那一处起进程）。
    //   原是 monitor 里的组合；界面改经通道直接说后端（`src/cc-bus-control.ts`），组合收进这一侧（业务解释只有一个家）。
    //   起子进程并等它们退出 ⇒ 阻塞档，同下面几条。部分投递失败**不整条回错**（成品里逐个列），
    //   只有「一条都还没发」的那一步（列名单）失败才回码。
    CommandSpec {
        name: "bus-broadcast",
        doc_anchor: Some("#### `bus-broadcast`"),
        // 〔DUP3〕`bad_id`：给的 `from` 形状过不了 `shell_quote_core::bus_id_ok`（交给 `cc-send` 之前先判，一个人都没发）。
        codes: &[
            "invalid_args",
            "not_installed",
            "timed_out",
            "failed",
            "bad_id",
        ],
        fields: &[
            "detail",
            "error",
            "failed",
            "from",
            "id",
            "liveness_unknown",
            "sent",
            "skipped_offline",
            "text",
        ],
        takes_input: true,
        run: Run::Blocking(|r| crate::control::cc_bus::broadcast_for_inbound(&r.args).map(Some)),
    },
    CommandSpec {
        name: "bus-kill",
        doc_anchor: Some("#### `bus-kill`"),
        codes: &[
            "invalid_args",
            "bad_id",
            "not_installed",
            "timed_out",
            "failed",
        ],
        fields: &["id", "killed", "stale_only"],
        takes_input: true,
        run: Run::Blocking(|r| crate::control::cc_bus::kill_for_inbound(&r.args).map(Some)),
    },
    CommandSpec {
        name: "bus-send",
        doc_anchor: Some("#### `bus-send`"),
        codes: &[
            "invalid_args",
            // 〔DUP2 · J12〕收件人的形状在交给 `cc-send` 之前就过不了（`INVARIANTS §47` ①）。
            "bad_id",
            "not_installed",
            "rejected",
            "timed_out",
            "too_long",
            "failed",
        ],
        fields: &["from", "live", "registered", "sent", "to"],
        takes_input: true,
        run: Run::Blocking(|r| crate::control::cc_bus::send_for_inbound(&r.args).map(Some)),
    },
    // 〔BS1b 09-24〕派生一个协作 agent（转调 `cc-spawn`）。起子进程并等它退出 ⇒ 阻塞档，
    // 同上面那三条。**会起一个真 agent 进程（烧额度）** —— 这一跳不重试由调用方负责，
    // 超时那一档的说法里明写「可能已经起来了」（`control::cc_bus::classify_spawn`）。
    CommandSpec {
        name: "bus-spawn",
        doc_anchor: Some("#### `bus-spawn`"),
        codes: &[
            "invalid_args",
            "bad_id",
            "not_installed",
            "timed_out",
            "failed",
        ],
        fields: &["id", "said", "spawned"],
        takes_input: true,
        run: Run::Blocking(|r| crate::control::cc_bus::spawn_for_inbound(&r.args).map(Some)),
    },
    // `K-R113`（09-13）：**一次回全的具名读命令。**
    //
    // 它不是 `bus-list` 的超集写法 —— `agents` 那一半**就是** `bus-list` 那一半
    //（同一个 `agents_via_cc_list`，由 `cc_bus::tests::bus_state_answers_both_halves_from_one_call`
    // 钉住），多出来的是 spawn 台账。⚠ 两半必须在**同一条命令**里回：总线名单与 spawn 台账
    // 互相引用，分两条命令取回来的两份是两个时刻的，拼出来的状态盘上从没存在过。
    CommandSpec {
        name: "bus-state",
        doc_anchor: Some("#### `bus-state`"),
        codes: &["not_installed", "timed_out", "failed"],
        // 〔SH1 · V136〕多了 `registered_at` · `spawned_at` · `skipped`（cc-bus 的 `--tsv` 形答）。
        fields: &[
            "agents",
            "ccm_sid",
            "dir",
            "id",
            "live",
            "registered_at",
            "skipped",
            "spawned",
            "spawned_at",
            "target",
            "task",
            "unread",
        ],
        takes_input: false,
        run: Run::Blocking(|_r| crate::control::cc_bus::state_for_inbound().map(Some)),
    },
    // 〔SH1 · V136〕驾驶舱读收件箱：转调 `cc-log`（只读，不推已读位置 —— 不是 `bus-recv`，`设计/95 §3.3`）。阻塞档。
    CommandSpec {
        name: "bus-inbox",
        doc_anchor: Some("#### `bus-inbox`"),
        codes: &[
            "invalid_args",
            "bad_id",
            "not_installed",
            "timed_out",
            "failed",
        ],
        fields: &[
            "class",
            "from",
            "id",
            "lines",
            "messages",
            "skipped",
            "text",
            "truncated",
            "ts",
        ],
        takes_input: true,
        run: Run::Blocking(|r| crate::control::cc_bus::inbox_for_inbound(&r.args).map(Some)),
    },
    CommandSpec {
        name: "cancel",
        doc_anchor: None,
        codes: &[],
        fields: &["target"],
        takes_input: true,
        run: Run::Builtin,
    },
    // `K-R104`（09-13）：**把 `K-R86` / `K-R87` 那两条原语搬上帧面。**
    //
    // 🔴 为什么非搬不可（这是**结构**，不是性能取舍）：那两条此前**只有 CLI 面**，
    // 而 CLI 面每调一次就是一次 SSH 握手 —— 用量探针两段轮询上限 12+20 轮
    // ⇒ 单次探测最多 **36** 次握手，撑破 `EXEC_TIMEOUT_SECS = 25`。
    // 帧面是**一条长连接上多次往返**，握手恒 1 次。读数与三条候选的比价住
    // `.claude/planned-build/backend-consolidation/features/K-R101-…#§8`。
    //
    // ⚠ 两条都**只做一次**：抓一屏就返回、起一个会话就返回。
    // 「隔多久再抓一次」留在调用方（`K37`：后端只给机制，不给偏好），
    // backend 侧由 `no_timer_guard` 零容忍地钉着。
    CommandSpec {
        name: "capture-pane",
        doc_anchor: Some("#### `capture-pane`"),
        codes: &[
            "invalid_args",
            "no_tmux",
            "no_server",
            "no_such_session",
            "capture_failed",
        ],
        fields: &["name", "screen"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::capture_pane::capture_for_inbound(&r.args).map(Some)
        }),
    },
    // ── 〔步 `24f` 第二刀 09-20〕`files-read` 这一族上线 ────────────────────────────
    //
    // 🔴 **处理器不住 `control/`，这是本仓第一条** —— 而且它必须不住那里：
    //   `control/` 的定义是「**会改变世界**」（`§1.1` 第二条线），而这一族整族纯读
    //   （`设计/96 §2.9` 边界①，`readonly_guard` 4259 行一行没动）；
    //   而读面 `observe/` 又被本文件头注那条硬约束挡着（`inbound` 不许出现 `observe::`，
    //   `inbound_structure_guards::inbound_never_reaches_into_the_observe_layer` 在钉）。
    //   ⇒ 它住顶层 `files/`（`lib.rs` 一条 `pub mod`，`readonly_guard::BACKEND_CORE_MODULES`
    //   里也是一条独立登记）。本文件头注那句「每条命令的处理器属于 `control/`」
    //   由此收窄成「**不在本文件里实现**」——那才是它真正保的东西。
    //
    // 🔴 **四条的 `run` 长得一模一样，那是刻意的**：命令名从 `r.cmd` 来，
    //   而 `r.cmd` 正是 `lookup()` 用来选中这条 spec 的那个串
    //   （CLI 面同理，`cli_control` 拿 `spec.name` 填它）⇒ 「登记的名字」与
    //   「真正被调的能力」**在类型上是同一个值**，抄错一条能力名这件事不可表示。
    //   翻译（`-` → `.`）只有 `files::answer_wire` 一处，理由整段在那个函数的头注。
    //
    // ⚠ 四条全在 `Run::Blocking`：`files::answer` 是**同步**函数，前两条真的做文件系统
    //   I/O，`files-find` 在 64 万条量纲上的现打外推是 20–50 ms（`设计/60 §3.5.3`）——
    //   那是不该占住 worker 的时长；而把「哪条够快可以走 `Run::Async`」拆成两档，
    //   等于给同一个同步入口记两份账。⇒ 一族一档。
    //   代价如实写：它们因此**取消不掉**，`cancel` 命中时回 `not_cancellable`（不撒谎）。
    // ── 〔步 `24f` 第三刀 09-21〕`设计/96 §2.9` 裁出来的第五、第六条 ──────────────
    //
    // 🔴 **它们补的是那两段「机制」的线上面** —— `设计/60 §3.5.2` 那张三段表
    //   （建索引 / 保鲜 / 查询）里，第二刀只把「查」那一段接上了线。
    //   在这两条之前，`files::index::rebuild_once` 与 `files::browse_watch::set_browsing`
    //   **零生产调用方** ⇒ 真机上 `files-find` 恒回 `index_missing: true`。
    //
    // 🔴 **节拍仍然不归后端**，一个字没松：这两条与 `capture-pane` 那两条**同一形** ——
    //   「**只做一次**……『隔多久再做一次』留在调用方」（`K37`：后端只给机制，不给偏好），
    //   `no_timer_guard` 在后端侧零容忍地钉着。
    //   ⚠ **别把这一刀读成「`设计/60 §3.5.2a` 那个缺口填上了」**：调用方不发这条命令，
    //   索引照旧永远不会自己变新，而「调用方到底发不发」后端这棵树的判据钉不住。
    //
    // ⚠ 两条的 `run` 与同族那四条逐字同形，理由同上一段（名字从 `r.cmd` 来 ⇒
    //   「登记的名字」与「真被调的能力」在类型上是同一个值）。
    // ── 〔波 5 ㈠ · 2026-09-23〕`设计/60 §8.6` **第 2 步**：把那份零消费者的写原语接上 ──
    //
    // 🔴 **这一条是本族第一条会往盘上写的命令，而它不花用户那句「允许」**：
    //   `control/files_write.rs` 自 09-19 起就在 `readonly_guard` 的写白名单上，
    //   射程逐字是「`O_EXCL` 新建一份**此前不存在**的文件；不删、不改名、不覆盖、不建目录」
    //   ⇒ 接上它**没有放宽任何一层判据**（`readonly_guard` 这一拍一行没动）。
    //   它此前的形状与 `files.ls` 同一形：**能力在那儿，没人接**（那份文件头注逐字
    //   「它今天没有调用方」）。本条就是那个调用方。
    //
    // 🔴 **处理器住 `control/`，与 `files-read` 那六条刻意不同层**：`control/` 的定义是
    //   「**会改变世界**」（`§1.1` 第二条线）—— 这一条真的改变世界，所以它回到了那一层；
    //   而 `files/` 那一族整族纯读（`设计/96 §2.9` 边界①），**一个字节都不许被这一条带脏**。
    //   ⇒ 两面分家：`files-*` 这个线上前缀底下从此有两族，
    //   由 `inbound_structure_guards` 那两条**互不相交**的相等断言各钉一族。
    //
    // ⚠ **CLI 面是自动来的，不是选的**：`cli_control::cli_exposed` = 非 `Run::Builtin`
    //   ⇒ 这一条同拍上了 `lib.rs::SUBCOMMANDS`（不加就当未知 flag、静默进流模式）。
    //   ⇒ 「入口窄」这件事不靠命令面，靠 `readonly_guard` 第三层那条
    //   「**谁引用得到 `control/files_write`**」。理由整段在那个模块的命令面那一节。
    // 〔B2 · 条 66 · `设计/01 §3.3b`〕「退出行为」那个值的两条命令 —— 值住**后端所在那台机器**
    //   （`~/.cc-monitor/backend.json`），前端要读要改都经这两条，**前端从不碰那个文件**。
    //   ⚠ 两条都在阻塞档：同步文件 I/O，开跑之后打不断 ⇒ `cancel` 命中回 `not_cancellable`。
    //   ⚠ `exit-policy-read` **没有错误码**：「读不出来」是一个**状态**（`state: "unreadable"` ＋ `reason`），
    //     不是一次失败 —— 调用方要的就是那一句「读不出来，按默认办」（`§3.3b ⑤`）。
    //   ⚠ CLI 面同样是派生的必然（`cli_control::cli_exposed`），理由同下面 `files-create` 那一段。
    // 〔W5-ALIAS · 第五波先行〕**别名预览**（`设计/71 §2.3`「生成器旁边显示这条别名实际会执行什么，是真验证，
    //   不是前端拼串」）：与 `ccm --print` 同一个计划函数，环境是「这台机器家目录里的一个新终端」。
    //   只读（不起进程、不写盘），阻塞档（读账号库 manifest ＋ 问会话快照）。
    CommandSpec {
        name: "ccm-print",
        doc_anchor: Some("#### `ccm-print`"),
        codes: &["bad_args", "refused"],
        fields: &["args", "line"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::ccm::answer_print(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔E2 · `96 §7.2.2`〕**这台的 `ccm` 会哪些**：`ccm` 就是这台后端本身（V28），「PATH 上那个是谁」退化成问它自己。
    //   纯函数（拼 `--ccm-probe` 那几行），不起进程、不碰盘 ⇒ 不进阻塞档（同 `ping` / `acct-iso-cmd` 那一形）。
    CommandSpec {
        name: "ccm-probe",
        doc_anchor: Some("#### `ccm-probe`"),
        codes: &[],
        fields: &["probe"],
        takes_input: false,
        run: Run::Async(|_r| {
            Box::pin(async move { Ok(Some(crate::control::ccm::answer_probe())) })
        }),
    },
    CommandSpec {
        name: "exit-policy-read",
        doc_anchor: Some("#### `exit-policy-read`"),
        codes: &[],
        fields: &["killOnExit", "path", "reason", "state"],
        takes_input: false,
        run: Run::Blocking(|_r| Ok(Some(crate::control::exit_policy::answer_read()))),
    },
    CommandSpec {
        name: "exit-policy-set",
        doc_anchor: Some("#### `exit-policy-set`"),
        codes: &["bad_args", "io_failed"],
        fields: &["killOnExit", "path", "reason", "state"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::exit_policy::answer_set(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔RM1a · 第四波〕**上游选择**那份凭据文件在**这台机器上**的读写口 —— 上游选择自己的状态，
    //   不是用户文件（判清全文 `调研/第四波记录/RM1a.md §1`）⇒ 写口登记在 `readonly_guard` 第四层，
    //   **只从这里一扇门进来**。远端账号页配的 key 从此落在会话跑的那台机器上。
    //   ⚠ 明文只在 `apikey-key-set` 的 `args.key` 里（帧面：长连接入方向；派生 CLI 面：stdin），
    //     **不进 argv / env / 日志**；两条的应答都只有掩码。
    //   ⚠ 两条都在阻塞档：同步文件 I/O，开跑之后打不断。
    //   ⚠ 它们**不起中转**、中转那几条也**不碰凭据**（「账号就账号, 中转就中转」）。
    CommandSpec {
        name: "apikey-key-set",
        doc_anchor: Some("#### `apikey-key-set`"),
        codes: &["bad_args", "bad_file", "io_failed"],
        // 〔HX2 · 4D〕入 `configDir`（账号 id 由后端推）· 出 `account`（推出来的那个）。
        fields: &["account", "baseUrl", "configDir", "key", "masked", "path"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::accounts::upstream::file_face::answer_set(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "apikey-read",
        doc_anchor: Some("#### `apikey-read`"),
        codes: &[],
        // 〔US1〕`rows` 退出线上：「表里有哪几行」只在这台后端里用（`file_face::rows_at`，三处读者同一份）。
        fields: &["configured", "masked", "notice", "path", "problem"],
        takes_input: false,
        run: Run::Blocking(|_r| {
            crate::accounts::upstream::file_face::answer_read()
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔US1 · 第四波 4D〕上游选择出的两份成品（`accounts/upstream/endpoint.rs`）。
    //   阻塞档：读一次凭据文件、装一次表；「中转在不在」读本进程的监听状态（中转住这里）。
    //   〔DEL 续〕只上流面（`cli_control::STREAM_ONLY`）：一次性进程里没有中转，答 `listening:false` 是假话。
    CommandSpec {
        name: "launch-endpoint",
        doc_anchor: Some("#### `launch-endpoint`"),
        codes: &["bad_args"],
        fields: &["account", "baseUrl", "listening", "whenDown"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::accounts::upstream::endpoint::answer_launch(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "apikey-routing",
        doc_anchor: Some("#### `apikey-routing`"),
        codes: &["bad_args"],
        fields: &["routed", "running"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::accounts::upstream::endpoint::answer_routing(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔DEL〕这里原是 `relay-status` / `relay-ensure`（这台机器上脱离的 `--relay` 在不在 · 起一个）：
    //   中转只住常驻后端进程里（本机远端同形，V139），那一族随回落一形删了。
    // 〔RM1a · 第四波〕「足迹」的这台机器那一半：只交**路径事实**（环境 · stat · 有没有某几个字样），
    //   哪一行属于哪个工具、存在 / 缺失 / 查不动怎么分，**只住 monitor 的 `config_surface`**。只读，阻塞档。
    CommandSpec {
        name: "footprint-probe",
        doc_anchor: Some("#### `footprint-probe`"),
        codes: &["bad_args", "too_large"],
        fields: &["env", "hooks", "notices", "stat"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::footprint::answer(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔AS1 · 第四波 4B〕**MCP 资产同步的判定**（`设计/96` 的 B，用户 09-24 V111 · V112）：两份原文进、
    //   差异四态 ＋ 可疑项（带这台机器的事实）＋「写哪几条」出。由**要被写的那一台**跑（事实是那台的）。
    //   只读：原文由 monitor 经 `files-peek` 读来，写经 `files-put`（CAS）—— 本条一个字节都不落盘。阻塞档（`stat`）。
    CommandSpec {
        name: "mcp-sync-plan",
        doc_anchor: Some("#### `mcp-sync-plan`"),
        codes: &["bad_args", "bad_file", "needs_consent"],
        fields: &["overwrite", "rows", "source", "take", "target", "write"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::mcp_sync::answer(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔AS2 · 第四波 4B · V113〕**资产目录**：这台现扫一次 skill 与项目级 MCP、记进后端自有的
    //   `~/.cc-monitor/assets-catalog.json`（第四层，变了才写）、回整份目录 ＋「这台缺什么」的判定。
    //   `-merge` 那条再把另一台后端的整份并进来（同一台取 `gen` 大的整份）。一个用户文件都不写。阻塞档（扫盘）。
    CommandSpec {
        name: "assets-catalog",
        doc_anchor: Some("#### `assets-catalog`"),
        codes: &["catalog_unreadable", "io_failed"],
        fields: &["changed", "machines", "path", "problems", "rows", "self"],
        takes_input: false,
        run: Run::Blocking(|r| {
            crate::asset_catalog::answer_catalog(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "assets-catalog-merge",
        doc_anchor: Some("#### `assets-catalog-merge`"),
        codes: &["bad_args", "catalog_unreadable", "io_failed"],
        fields: &[
            "catalog", "changed", "machines", "path", "problems", "rows", "self",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::asset_catalog::answer_merge(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔AS2〕**资产目录的自动同步**：本机常驻后端沿池里那条 SSH 连接（多开一个 exec 通道，零新连接）
    //   拉远端的目录、并进本机、把远端缺的推过去。写口（`answer_merge`）由这扇门递进去 —— `asset_sync.rs`
    //   自己不直呼它（`readonly_guard` 第四层 ④：写口只从 `inbound.rs` 进来）。真异步（拨号 / 等远端）。
    CommandSpec {
        name: "assets-sync",
        doc_anchor: Some("#### `assets-sync`"),
        codes: &["bad_args", "io_failed"],
        fields: &["dial", "origin", "reach", "self", "synced"],
        takes_input: true,
        run: Run::Async(|r| {
            Box::pin(async move {
                let fold: crate::asset_sync::Fold =
                    std::sync::Arc::new(crate::asset_catalog::answer_merge);
                crate::asset_sync::answer(&r.args, fold, &crate::remote_ask::DialRemote)
                    .await
                    .map(Some)
                    .map_err(|(c, m)| (c.to_string(), m))
            })
        }),
    },
    // 〔C4d · 第四波 4B〕**可达表登记**：monitor（宿主，只交事实）在每台远端流握手成功那一刻交「怎么够到那台」
    //   （拨号请求 ＋ 那台后端的路径），本机后端记进内存可达表（`remote_ask`，后端重启就空）。**只登记，不拨号**：
    //   之后「本机后端问远端后端」的两路（资产目录同步 · 历史跨机 join）都查这张表。老远端也登记（历史问它的是老子命令）。
    CommandSpec {
        name: "remote-reach",
        doc_anchor: Some("#### `remote-reach`"),
        codes: &["bad_args"],
        fields: &["dial", "origin", "reach"],
        takes_input: true,
        // 纯内存（一把锁、插一行）⇒ 不进阻塞档，同 `ping` / `resolve`。
        run: Run::Async(|r| {
            Box::pin(async move {
                crate::remote_ask::answer_reach(&r.args)
                    .map(Some)
                    .map_err(|(c, m)| (c.to_string(), m))
            })
        }),
    },
    // 〔C4d · 第四波 4B〕**历史注解**（星标 / 改名 / 隐藏 / 上次用哪个号起）的读写者换成本机常驻后端 ——
    //   主会话 09-25 裁「文件留在原处、同一路径，不迁移、一条不丢」：路径由 monitor 起本机后端时显式交（`CCM_HISTORY_METADATA`），
    //   写是第四层（`history_annotations.rs`，读不懂就拒写、只改那一条、认不出的键原样留着）。三条都是阻塞档（读写一份小文件）。
    CommandSpec {
        name: "history-annotate",
        doc_anchor: Some("#### `history-annotate`"),
        codes: &[
            "annotations_unreadable",
            "bad_args",
            "io_failed",
            "no_annotations",
        ],
        fields: &[
            "customTitle",
            "entry",
            "hidden",
            "lastAccount",
            "patch",
            "sid",
            "starred",
            "updatedAt",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::history_annotations::answer_annotate(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "history-forget",
        doc_anchor: Some("#### `history-forget`"),
        codes: &[
            "annotations_unreadable",
            "bad_args",
            "io_failed",
            "no_annotations",
        ],
        fields: &["removed", "sid"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::history_annotations::answer_forget(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "history-last-accounts",
        doc_anchor: Some("#### `history-last-accounts`"),
        codes: &["annotations_unreadable", "no_annotations"],
        fields: &["accounts"],
        takes_input: false,
        run: Run::Blocking(|r| {
            crate::history_annotations::last_accounts(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔AS2 · 第四波 4B · V113〕**skill「装到这台」**：`skill-read` 在来源那台读出这个 skill 的全部文件（原文 ＋ 执行位）；
    //   `skill-install-plan` 在要被写的那一台判 —— 差异四态与「不同的要显式说盖」那道闸原样用 AS1 的 `mcp_sync::{diff, plan}`，
    //   可疑项（可执行 · 二进制 · 绝对路径 · `#!` 要的命令）带那台的事实。两条都只读；写经 `files-put`（CAS）。阻塞档（扫盘）。
    CommandSpec {
        name: "skill-read",
        doc_anchor: Some("#### `skill-read`"),
        codes: &["bad_args", "io_failed", "not_found", "too_large"],
        fields: &["dir", "files", "name", "root", "skipped"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::skill_install::answer_read(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "skill-install-plan",
        doc_anchor: Some("#### `skill-install-plan`"),
        codes: &[
            "bad_args",
            "bad_file",
            "io_failed",
            "needs_consent",
            "too_large",
        ],
        fields: &[
            "base",
            "dir",
            "name",
            "overwrite",
            "prefix",
            "root",
            "rows",
            "source",
            "take",
            "target",
            "write",
            // 〔SU1〕给了 `take` 才有：真要写的那几个的摘要 ＋ 装之前在不在（装完原样交回 `skill-install-record`）。
            "ledger",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::skill_install::answer_plan(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔SU1 · 第四波 4C · V116〕**skill 卸**（「要，只删装时写进去的文件」）：
    //   `skill-install-record` 是装记录 `~/.cc-monitor/skill-installs.json` 的写口（第四层；装完记 `add` · 卸掉的摘 `drop`），
    //   `skill-installs` 列这台记着的 · `skill-uninstall-plan` 在被卸的那一台判（逐文件四态 ＋ 要不要问 ＋ 删哪几个）。
    //   后两条只读；删经 `files-delete`（CAS）。三条都是阻塞档（读写一份小文件 · 逐个读盘比摘要）。
    CommandSpec {
        name: "skill-install-record",
        doc_anchor: Some("#### `skill-install-record`"),
        codes: &[
            "bad_args",
            "io_failed",
            "ledger_unreadable",
            "not_found",
            "too_large",
        ],
        fields: &[
            "changed",
            "dir",
            "files",
            "name",
            "op",
            "paths",
            "remaining",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::skill_ledger::answer_record(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "skill-installs",
        doc_anchor: Some("#### `skill-installs`"),
        codes: &["io_failed", "ledger_unreadable"],
        fields: &["installs"],
        takes_input: false,
        run: Run::Blocking(|r| {
            crate::skill_install::answer_installs(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "skill-uninstall-plan",
        doc_anchor: Some("#### `skill-uninstall-plan`"),
        codes: &[
            "bad_args",
            "io_failed",
            "ledger_unreadable",
            "needs_consent",
            "not_found",
        ],
        fields: &[
            "confirm", "delete", "dir", "forget", "name", "rows", "seen", "take",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::skill_install::answer_uninstall_plan(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-create",
        doc_anchor: Some("#### `files-create`"),
        codes: &["bad_args", "bad_path", "io_failed", "refused"],
        fields: &["bytes", "content", "path", "rel", "root"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // ── 〔波 5 ㈡ · 2026-09-23〕`设计/60 §8.6` **第 3 步**：改动既有数据的那五条 ──────────
    //
    // 🔴 **这五条才花掉用户那句话**：「现在只允许后端的文件管理部分写文件」。
    //   处理器同住 `control/files_write.rs`（`readonly_guard` 第三层唯一登记的模块），
    //   而本文件是那一层登记的**唯一一扇门** —— 后端生产树里别处引用那个模块 ⇒ 红。
    //   ⚠ 本段五条与上面 `files-create` 的 `run` 逐字同形（名字从 `r.cmd` 来）。
    // ⚠ 全在阻塞档：同步文件系统 I/O（外加围栏那几次 `canonicalize`），开跑之后打不断
    //   ⇒ `cancel` 命中时回 `not_cancellable`，不撒谎。
    CommandSpec {
        name: "files-mkdir",
        doc_anchor: Some("#### `files-mkdir`"),
        codes: &["bad_args", "bad_path", "io_failed", "refused"],
        fields: &["path", "rel", "root"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-rename",
        doc_anchor: Some("#### `files-rename`"),
        codes: &["bad_args", "bad_path", "io_failed", "refused"],
        fields: &["from", "path", "root", "to"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-delete",
        doc_anchor: Some("#### `files-delete`"),
        codes: &["bad_args", "bad_path", "io_failed", "refused", "stale"],
        // 〔FW5〕`recursive`（入）· `removed`（出）：显式才删整棵树，逐条目过围栏。
        // 〔RM1e〕`expect`（入）：给了 ⇒ 盘上逐字节等于它才删一份普通文件，否则 `stale`。
        fields: &["expect", "path", "recursive", "rel", "removed", "root"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-chmod",
        doc_anchor: Some("#### `files-chmod`"),
        // 〔FW5〕`no_unix_mode`：这个平台没有 unix 权限位（target 轴从这一格现推 Windows 那一格）。
        codes: &[
            "bad_args",
            "bad_path",
            "io_failed",
            "no_unix_mode",
            "refused",
        ],
        fields: &["mode", "path", "rel", "root"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔F7a · 第三波 09-24〕写面第七条：同根内复制（`设计/60 §13`）。与上面五条同住一个模块、
    //   同一扇门、同档（同步文件 I/O，取消不掉）。它**不给第三层添动词**：由 `O_EXCL` 新建 ＋
    //   换名 ＋ 删自己刚建的那一份拼出来（理由住 `control/files_write.rs::copy_entry`）。
    CommandSpec {
        name: "files-copy",
        doc_anchor: Some("#### `files-copy`"),
        codes: &["bad_args", "bad_path", "io_failed", "refused"],
        // 〔W5-FILES〕`recursive`（入）· `files` / `dirs`（出）：显式才复制目录。
        fields: &[
            "bytes",
            "dirs",
            "files",
            "from",
            "links",
            "overwrite",
            "path",
            "recursive",
            "root",
            "to",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔FILES2 · 第四波 09-27〕解压（`设计/60 §6.2` · `§7` 第 9 条 Q3，主会话按通行做法裁）：处理器住
    //   `control/files_extract.rs`（第三层第四个登记的模块），本文件照旧是那一层唯一的门。阻塞档（同步读包 ＋ 落盘）。
    CommandSpec {
        name: "files-extract",
        doc_anchor: Some("#### `files-extract`"),
        codes: &[
            "bad_args",
            "bad_path",
            "exists",
            "io_failed",
            "refused",
            "unsupported",
        ],
        fields: &[
            "bytes", "dirs", "files", "fresh", "links", "path", "rel", "root",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_extract::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-write-text",
        doc_anchor: Some("#### `files-write-text`"),
        codes: &["bad_args", "bad_path", "io_failed", "refused", "stale"],
        fields: &[
            "bytes", "content", "expect", "path", "rel", "root", "sha256",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // ── 〔RW1 · 第四波 · 2026-09-24〕用户文件的读改写 ＋ 删历史会话 ─────────────────────────
    //
    // 🔴 用户裁「只允许后端的文件管理部分写文件」**只管用户的文件、本机也管** ⇒ monitor 进程
    //   不再直接写用户文件；本机与远端都经这三条（`call(origin, …)`，同一条路）。处理器同住
    //   `control/files_write.rs`（第三层），本文件照旧是那一层唯一的门。阻塞档（同步文件 I/O）。
    //   `files-delete-session` 只收 sid（理由住那个模块的 `delete_session`）。
    //   〔AR1 · V119〕上一版说它是「会话文件围栏**唯一的例外**」—— FN1 之后文件管理写面已不设会话文件围栏，
    //   这一条「只许删会话形状那一份」的限制是它自己的，不是谁的例外。
    CommandSpec {
        name: "files-peek",
        doc_anchor: Some("#### `files-peek`"),
        codes: &[
            "bad_args",
            "bad_path",
            "io_failed",
            "not_text",
            "refused",
            "too_large",
        ],
        fields: &["exists", "path", "rel", "root", "text"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-put",
        doc_anchor: Some("#### `files-put`"),
        codes: &["bad_args", "bad_path", "io_failed", "refused", "stale"],
        fields: &[
            "backup", "bytes", "changed", "content", "created", "expect", "parents", "path", "rel",
            "root",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-delete-session",
        doc_anchor: Some("#### `files-delete-session`"),
        codes: &["bad_args", "io_failed", "refused"],
        fields: &["path", "sid"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // ── 〔F7c · 第三波 · 2026-09-24〕`设计/60 §13`：上传的**提交** ──────────────────────
    //
    // 🔴 SFTP 缩成只做传输之后，上传只写暂存区（`~/.cc-monitor/staging/<key>.part`）；
    //   把它挪进用户目标的**那一下**在这里 —— 用户逐字「现在只允许后端的文件管理部分写文件」。
    //   处理器住 `control/files_commit.rs`（`readonly_guard` 第三层第二个登记的模块），
    //   本文件照旧是那一层唯一的门。阻塞档：同步文件系统 I/O（围栏的 `canonicalize` ＋ 改名）。
    // ── 〔F9c · 第四波〕存盘装不进一条请求行时：逐块进暂存区 ＋ 读回拼起来原地覆盖 ──────────
    //   同住 `control/files_commit.rs`（第三层第二个模块），阻塞档理由同上一条。
    CommandSpec {
        name: "files-stage-chunk",
        doc_anchor: Some("#### `files-stage-chunk`"),
        codes: &["bad_args", "io_failed", "refused"],
        fields: &["bytes", "content", "key", "seq"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_commit::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-commit-text",
        doc_anchor: Some("#### `files-commit-text`"),
        codes: &["bad_args", "bad_path", "io_failed", "refused", "stale"],
        fields: &[
            "bytes", "chunks", "expect", "key", "path", "rel", "root", "sha256",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_commit::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-commit-upload",
        doc_anchor: Some("#### `files-commit-upload`"),
        codes: &["bad_args", "bad_path", "io_failed", "refused", "stale"],
        fields: &[
            "bytes",
            "chunks",
            "expect",
            "key",
            "overwrite",
            "path",
            "rel",
            "root",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::files_commit::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-browse",
        doc_anchor: Some("#### `files-browse`"),
        codes: &["bad_args", "bad_path"],
        // 〔W5-FILES〕+`watching` · `watch_failed` · `watch_error`（进程里那一个监听器跟上名单，`设计/60 §3.7`）。
        fields: &[
            "added",
            "browse_watch_cap",
            "dirs",
            "rejected",
            "removed",
            "watch_error",
            "watch_failed",
            "watching",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-index-rebuild",
        doc_anchor: Some("#### `files-index-rebuild`"),
        // `already_rebuilding`〔2026-09-21〕：非阻塞互斥抢不到那个位。
        codes: &["already_rebuilding", "bad_path", "unreadable"],
        fields: &[
            "entries",
            "path",
            "resident_bytes",
            "skipped_mounts",
            "truncated",
            "unreadable_dirs",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-find",
        doc_anchor: Some("#### `files-find`"),
        codes: &["bad_args"],
        fields: &[
            "hits",
            "ignore_ascii_case",
            "index_age_secs",
            "index_missing",
            "limit",
            "needle",
            "scanned",
            "total_hits",
            "truncated",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-index-status",
        doc_anchor: Some("#### `files-index-status`"),
        codes: &[],
        fields: &[
            "age_secs",
            "browse_watch_cap",
            "browse_watches",
            "cold_first_build_secs",
            "entries",
            "index_missing",
            "resident_bytes",
            "rewalk_interval_secs",
            "skipped_mounts",
            "stale",
            "truncated",
            "unreadable_dirs",
        ],
        takes_input: false,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-ls",
        doc_anchor: Some("#### `files-ls`"),
        codes: &["bad_path", "unreadable"],
        fields: &[
            "entries",
            "kind",
            "limit",
            "mtime_secs",
            "path",
            "size",
            "truncated",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-stat",
        doc_anchor: Some("#### `files-stat`"),
        codes: &["bad_path", "unreadable"],
        // 〔GP1 · 第四波〕+`mode`（能力 `files.stat` 同拍加的那一格；非 unix 缺席）。
        fields: &["kind", "mode", "mtime_secs", "path", "readonly", "size"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // ── 〔F7a · 第三波 · 2026-09-24〕`设计/60 §13`：窗口换走通道的那两问 ────────────────
    //
    // 🔴 **同一族（`files-read`）的第七、第八条，整族照旧纯读**：编辑器读一份文本 ·
    //   开窗前「那台机器的 home 在哪」。此前窗口为这两问各拨一条 SFTP（`设计/60 §12.3`
    //   那张欠账表），现在经通道问后端 —— 窗口进程够后端**只剩通道**这一条路。
    // ⚠ `run` 与同族那六条逐字同形（名字从 `r.cmd` 来）；同在 `Run::Blocking`、同样取消不掉。
    CommandSpec {
        name: "files-read-text",
        doc_anchor: Some("#### `files-read-text`"),
        codes: &[
            "bad_args",
            "bad_path",
            "not_text",
            "too_large",
            "unreadable",
        ],
        fields: &["bytes", "max_bytes", "path", "sha256", "text"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔FILES2 · V152〕读族第十条：按字节寻址分块读回（非 UTF-8 名的下载）。同族同形、同在阻塞档。
    CommandSpec {
        name: "files-read-chunk",
        doc_anchor: Some("#### `files-read-chunk`"),
        codes: &["bad_args", "bad_path", "not_text", "unreadable"],
        fields: &["content", "eof", "len", "offset", "path", "size"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔W5-FILES · 第五波〕读族第九条：算目录大小（`设计/60 §6.2`）。与同族那几条逐字同形、同在阻塞档。
    CommandSpec {
        name: "files-size",
        doc_anchor: Some("#### `files-size`"),
        codes: &["bad_path", "unreadable"],
        fields: &[
            "bytes",
            "dirs",
            "files",
            "links",
            "other",
            "path",
            "skipped_mounts",
            "unreadable_dirs",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-home",
        doc_anchor: Some("#### `files-home`"),
        codes: &["no_home"],
        fields: &["path"],
        takes_input: false,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // ── 〔`C1` · 2026-09-24〕只读查询面上线 —— `设计/15 §3.2` 层 1 ＋ `99 §4.19.2 ⑥` ──────────
    //
    // 🔴 **这八条此前全是一次性子命令**：monitor 每问一次就新拨一条 SSH（握手 ＋ 鉴权 ＋ exec），
    //   而这条长连接明明已经在那儿。账号那两条还被一个 10 秒的轮询按台数翻倍。
    //   ⇒ 登记上来，monitor 改走已有的 `inbound_client`，那些逐次拨号与轮询一起删。
    //
    // 🔴 **处理器住顶层 `read_face`，与 `files/` 同一个理由**：本文件不许出现 `observe::`
    //   （`inbound_never_reaches_into_the_observe_layer`），而查询本体住 `observe/`。
    //   `read_face` 只做换壳 —— 每条都调 CLI 那一臂同一个函数，`out` 从 stdout 换成内存。
    //
    // ⚠ **名字刻意不与 CLI 那几条同名**（`history-projects` 而不是 `list-projects`）：
    //   CLI 面是从本表**自动派生**的（`cli_control::cli_exposed`）—— 同名就会把
    //   `--list-projects` 从 `history_query::run` 手里抢走、改印一行 JSON，
    //   而本机 monitor 正在 exec 那条读它的逐行输出。⇒ 代价如实登记：这八条同拍多出
    //   八个 CLI 面（`--history-projects` …），已进 `lib.rs::SUBCOMMANDS`（不进就静默进流模式）。
    //
    // ⚠ 全在 `Run::Blocking`：它们都做文件 I/O（`history-search` 扫全库）。代价同 `files-*`：
    //   `cancel` 命中时回 `not_cancellable`（不撒谎）。
    // 〔C4d · 第四波 4B〕这两条**出成品**：历史跨机 join 的唯一的家（`history_join.rs`）—— 这台（记录树 ＋ 合成历史 ＋ pidfile 判活）
    //   或可达表里的那一台（`remote_ask` 问它的 CLI 老子命令 `--list-projects` / `--list-sessions`），并上这台的注解。
    //   真异步（远端那一跳要等）；本机扫盘那一段挪到阻塞线程池（`history_join::blocking`）。
    CommandSpec {
        name: "history-projects",
        doc_anchor: Some("#### `history-projects`"),
        codes: &["bad_args", "failed", "too_large", "unreachable"],
        fields: &["notice", "origin", "rows"],
        takes_input: true,
        run: Run::Async(|r| {
            Box::pin(async move {
                crate::history_join::answer_projects(r.args)
                    .await
                    .map(Some)
                    .map_err(|(c, m)| (c.to_string(), m))
            })
        }),
    },
    CommandSpec {
        name: "history-sessions",
        doc_anchor: Some("#### `history-sessions`"),
        codes: &["bad_args", "failed", "too_large", "unreachable"],
        fields: &["notice", "origin", "project_dir", "rows"],
        takes_input: true,
        run: Run::Async(|r| {
            Box::pin(async move {
                crate::history_join::answer_sessions(r.args)
                    .await
                    .map(Some)
                    .map_err(|(c, m)| (c.to_string(), m))
            })
        }),
    },
    CommandSpec {
        name: "history-search",
        doc_anchor: Some("#### `history-search`"),
        codes: &["bad_args", "failed", "too_large"],
        fields: &[
            "after_ms",
            "include_tools",
            "limit",
            "lines",
            "query",
            "scope",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "history-subagents",
        doc_anchor: Some("#### `history-subagents`"),
        codes: &[
            "bad_args",
            "bad_parent",
            "path_refused",
            "too_large",
            "write_failed",
        ],
        fields: &["lines", "parent"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔U4b · 第四波〕resume 之前问「这条会话的记录还在不在」。同族同档（一次目录枚举 ⇒ 阻塞档）、
    // 同一个只读宿主。**只收 sid**（找文件那一步与分叉 / 删会话同一份 `branch_core::find_session_file`）。
    CommandSpec {
        name: "history-record",
        doc_anchor: Some("#### `history-record`"),
        codes: &["bad_args"],
        // 〔GP1 · 第四波〕+`configDir`（可选入参：这次 resume 要用的账号根）。
        fields: &["configDir", "present", "root", "sid"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔CF2 · 第四波 4B〕按**行号**取回（没接骨架的会话丢掉的正文从这里要回来）。同族同档、同一个只读宿主。
    CommandSpec {
        name: "history-lines",
        doc_anchor: Some("#### `history-lines`"),
        codes: &["bad_args", "failed", "oversized_line", "refused"],
        fields: &["eof", "from", "lines", "next", "path", "until"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "history-read",
        doc_anchor: Some("#### `history-read`"),
        codes: &["bad_args", "failed", "oversized_line", "refused"],
        fields: &["eof", "next", "offset", "path", "text", "until"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔SR1a · 2026-09-24〕骨架索引与大纲清单上帧面（此前它们在远端走逐次拨号 —— `STILL_DIALED` 那两行）。
    // 同族同档（同步文件 I/O ⇒ 阻塞档）、同一个只读宿主（`read_face::answer`）。
    // 〔SR1a × SE2〕会话内查找上帧面（此前走逐次拨号 —— `STILL_DIALED` 那一行）。同族同档。
    // 〔STC · `设计/90 §4` 阶段 C〕会话事实（`read_face.rs` 那一臂 ＋ `observe/facts_query.rs`）。同族同档、同一个只读宿主。
    //   `prior` 是调用方上一次拿到的应答原样（续传令牌）；应答五格即成品。
    CommandSpec {
        name: "history-facts",
        doc_anchor: Some("#### `history-facts`"),
        codes: &["bad_args", "failed", "too_large"],
        fields: &[
            "agents",
            "end",
            "forkedFrom",
            "path",
            "prior",
            "touchedFiles",
            "usage",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "backend-log",
        doc_anchor: Some("#### `backend-log`"),
        codes: &["bad_args", "failed"],
        fields: &["maxBytes", "path", "size", "text", "truncated"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "history-find",
        doc_anchor: Some("#### `history-find`"),
        codes: &["bad_args", "failed", "too_large"],
        fields: &["hits", "include_tools", "limit", "path", "query", "total"], // 〔C4b〕应答出成品：`lines` ⇒ `total` / `hits`
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "history-index",
        doc_anchor: Some("#### `history-index`"),
        codes: &["bad_args", "failed", "too_large"],
        fields: &["end", "from", "offset", "path", "rows", "until"], // 〔C4b〕应答出成品：`lines` ⇒ `from` / `end` / `rows`
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "history-user-inputs",
        doc_anchor: Some("#### `history-user-inputs`"),
        codes: &["bad_args", "failed", "too_large"],
        fields: &["end", "entries", "from", "path"], // 〔C4b〕应答出成品：`lines` ⇒ `from` / `end` / `entries`
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "history-tail",
        doc_anchor: Some("#### `history-tail`"),
        codes: &["bad_args", "failed"],
        fields: &["end", "n", "path", "split_at", "tail_from", "total"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔C4c · 第四波 4B〕出成品：`{meta, accounts, notice}`，并上这台机器自己那份 apikey 表；`agent` 随请求带（必填）。
    CommandSpec {
        name: "accounts-list",
        doc_anchor: Some("#### `accounts-list`"),
        codes: &["bad_args", "too_large"],
        fields: &["accounts", "agent", "meta", "notice"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "accounts-sessions",
        doc_anchor: Some("#### `accounts-sessions`"),
        codes: &["too_large"],
        fields: &["lines"],
        takes_input: false,
        run: Run::Blocking(|r| {
            crate::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔C4c · 第四波 4B〕换号前的信任预检：`configDir`（缺席 / null = 账号 0）＋ `cwd` → `{trusted, known}`。
    //   同族同档（读一份 manifest ＋ 一份 `.claude.json` ⇒ 阻塞档）、同一个只读宿主。
    CommandSpec {
        name: "accounts-trust",
        doc_anchor: Some("#### `accounts-trust`"),
        codes: &[
            "bad_args",
            "failed",
            "manifest_unavailable",
            "no_home",
            "unknown_config_dir",
            "unsafe_config_dir",
        ],
        fields: &["configDir", "cwd", "known", "trusted"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // ── 〔RM1b · 第四波〕功能侧只读查询 —— 远端会话的任务列表（`parity_ledger` `session.tasks`）──
    //
    // 🔴 此前只有 monitor 直读**本机** `tasks/<sid>/` 那一条路，远端 tab 永远拿不到任务。
    //   本机后端与远端后端是同一个二进制 ⇒ 读法搬到这里，monitor 按 origin 问（本机也走这里）。
    // ⚠ 宿主是 `feature_face`，**不是** `read_face`：monitor 侧有一条两向判据数的正是
    //   「交给 `read_face::answer` 的 == `C1` 那八条」，本族不在其中（理由全文在 `feature_face` 头注）。
    // ⚠ 阻塞档：读一个目录 ＋ 每个任务文件各一次。`cancel` 命中回 `not_cancellable`（不撒谎）。
    // 〔RM1b · 第四波〕同族第二条：插件市场只读枚举（`parity_ledger` `plugins.marketplaces`）。
    //   从 monitor `plugins.rs`（`P8a`）原样搬来，三条出口不变；〔C4b〕应答 = 整份 survey（成品，不再裹成一行）。
    CommandSpec {
        name: "plugins-marketplaces",
        doc_anchor: Some("#### `plugins-marketplaces`"),
        codes: &["failed", "too_large"],
        fields: &["entries", "file_absent"],
        takes_input: false,
        run: Run::Blocking(|r| {
            crate::feature_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔LOC1a · 第四波 4D〕这台机器的 `cc-acct-iso` 两问 —— 与 CLI `--acct-iso-status` / `--acct-iso-shellinit`
    //   同一份本体（`accounts/iso.rs`）。`设计/05 §14.6`：本机那几问从「exec 一次性本机后端」改走 `<local>` 长连接。
    //   `shellinit` 要起一次 `cc-acct-iso`（插件口）⇒ 两条都进阻塞档（`status` 只看文件在不在，同档省一份理由）。
    CommandSpec {
        name: "acct-iso-status",
        doc_anchor: Some("#### `acct-iso-status`"),
        codes: &[],
        fields: &["installed", "looked", "path"],
        takes_input: false,
        run: Run::Blocking(|_r| {
            crate::accounts::iso::answer_wire_status()
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔DUP2 · 主会话 09-26 裁 J4〕**一个 cc-acct-iso 步骤在终端里要跑的那一行**（`设计/01 §1.1`「命令串……都不在前端」·
    //   `设计/90 §3` 判据 2；先例 `ccm-print`）。纯函数：校验 ＋ 唯一的 quote，**不起进程、不碰盘** ⇒ 不进阻塞档（同 `ping` / `resolve`
    //   那一形，在 runtime 上当场答完）。跑它的是用户面前那个终端（DESIGN §6）。
    CommandSpec {
        name: "acct-iso-cmd",
        doc_anchor: Some("#### `acct-iso-cmd`"),
        codes: &["bad_args", "refused"],
        fields: &["cmd", "credFile", "name", "step"],
        takes_input: true,
        run: Run::Async(|r| {
            Box::pin(async move {
                crate::accounts::iso::answer_wire_cmd(&r.args)
                    .map(Some)
                    .map_err(|(c, m)| (c.to_string(), m))
            })
        }),
    },
    CommandSpec {
        name: "acct-iso-shellinit",
        doc_anchor: Some("#### `acct-iso-shellinit`"),
        codes: &["not_installed", "not_run", "timed_out", "tool_failed"],
        fields: &["snippet"],
        takes_input: false,
        run: Run::Blocking(|_r| {
            crate::accounts::iso::answer_wire_shellinit()
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔LOC1a · 第四波 4D〕分叉：与 CLI `--fork-session` 同一个本体（`control/fork_write.rs::run_inner`，
    //   读 → `branch-core` 变换 → `O_EXCL` 新建）。本机远端同一条长连接；读整份 jsonl ⇒ 阻塞档。
    //   ⚠ 名字刻意不是 `fork-session`：自动派生的 CLI 面会与对 aterm 冻结的 `--fork-session`（argv 形）撞名。
    CommandSpec {
        name: "session-fork",
        doc_anchor: Some("#### `session-fork`"),
        codes: &["bad_args", "fork_failed"],
        fields: &["jsonlPath", "sessionId", "sid", "uuid"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::fork_face::answer(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔SH1 · V137〕MCP 列表成品：user / local / project 三段 ＋ 用过的项目目录 ＋ 读不出来的那几份。阻塞档（同步文件 I/O）。
    CommandSpec {
        name: "mcp-read",
        doc_anchor: Some("#### `mcp-read`"),
        codes: &["bad_args", "too_large"],
        fields: &[
            "dirs",
            "entries",
            "name",
            "problems",
            "projectDir",
            "scope",
            "server",
            "sourcePath",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::feature_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔RESYNC · V149 · `设计/15 §4.1b`〕手动对齐：整机（或 `sid` 只对一个会话）重跑起步那套对齐，回差异。阻塞档：等每份 watcher 做完。
    CommandSpec {
        name: "resync",
        doc_anchor: Some("#### `resync`"),
        codes: &["bad_args"],
        fields: &[
            "added",
            "removed",
            "retagged",
            "sid",
            "uncancellable",
            "unavailable",
            "watchers",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::resync_face::answer(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔SH1〕列这台的 tmux 会话：`{installed, lines}`（原样 `tmux ls -F` 行，与流里推的那份同一个格式串）。阻塞档（起一次 `sh` ＋ `tmux`）。
    CommandSpec {
        name: "tmux-list",
        doc_anchor: Some("#### `tmux-list`"),
        codes: &["unobservable", "too_large"],
        fields: &["installed", "lines"],
        takes_input: false,
        run: Run::Blocking(|r| {
            crate::feature_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "tasks-list",
        doc_anchor: Some("#### `tasks-list`"),
        codes: &["bad_args", "failed", "too_large"],
        // 〔LOC1a · 第四波 4D · C4e 批 4〕应答换成成品 `{tasks: [...]}`（原是原样对象的 `lines`）⇒ 后端行为变更，合并那拍 bump。
        fields: &[
            "activeForm",
            "blockedBy",
            "blocks",
            "description",
            "id",
            "sid",
            "status",
            "subject",
            "tasks",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::feature_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // ── 〔RM1c · 第四波〕代码全景（用户 09-24 V108 选 B）────────────────────────────
    //
    // 后端**不链**引擎：经插件通用调用口起那个只装引擎的独立小程序（`control/panorama.rs`），
    // 解析发生在被起的那个进程里；索引落这台机器上后端自己的数据目录。
    // ⚠ 只说查询语义：`op` 只许 `control::panorama::OPS` 里的词（`protocol_doc_guard` 那条 `P7c-2` 约束）。
    // 〔RM1f〕**异步档**：起进程走 `plugin::invoke::run_abortable`（异步等子进程）⇒ `cancel` 命中时
    //   处理器 future 被丢、小程序那一组子进程被杀、回 `cancelled` —— 建索引（可到分钟级）打得断了。
    //   〔墓碑 —— RM1c 那一版是阻塞档：「起一个进程、等它退出。`cancel` 命中回 `not_cancellable`（不撒谎）」。〕
    CommandSpec {
        name: "panorama",
        doc_anchor: Some("#### `panorama`"),
        codes: &[
            "bad_args",
            "not_installed",
            "unsupported",
            "timed_out",
            "too_large",
            "failed",
        ],
        fields: &["args", "op", "repo", "result"],
        takes_input: true,
        run: Run::Async(|r| {
            Box::pin(async move { crate::control::panorama::answer(&r.args).await.map(Some) })
        }),
    },
    // F04a：**第一条破坏性命令。** 三道门在 `control/gate::admit_destructive`，
    // 对句柄下手不对名字。⚠ monitor 侧改走这条路是 **F04b**（定框 C6 的顺序）。
    CommandSpec {
        name: "kill",
        doc_anchor: Some("#### `kill`"),
        codes: &[
            "invalid_args",
            "no_tmux",
            "no_such_session",
            "wrong_owner",
            "too_many_windows",
            "kill_failed",
        ],
        fields: &["killed", "name", "session"],
        takes_input: true,
        run: Run::Blocking(|r| crate::control::kill::kill_for_inbound(&r.args).map(Some)),
    },
    CommandSpec {
        name: "launch",
        doc_anchor: Some("#### `launch`"),
        // 〔TL2 · C4e 问 2〕+`wrong_owner`：`send-into` / `send-keys-raw` 过 `gate::admit`（§34 Gate 2），
        // 它真会回这个码，登记表原先漏了。由 `gate_tests.rs::every_command_that_passes_the_gate_lists_the_gates_codes` 从 gate.rs 源码派生钉住。
        codes: &[
            "invalid_args",
            "no_tmux",
            "no_such_session",
            "wrong_owner",
            "create_failed",
            "typed_unconfirmed",
        ],
        // 〔`K-P2` `D` 阶段第三拍 09-03〕8 → 11：`agent` / `width` / `height`。
        // 那三个是「ccm 的 `--tmux` 真的改走这条路」逼出来的 —— 本地那条编排里
        // `@ccm_agent` 与 `-x/-y` 一直都在，这一侧此前没有字段能表达它们
        // ⇒ 不补就是**静默丢修饰**。⚠ `avoid_collision` **不加**：撞名避让住在要搬的那一块
        // **之外**，而「撞了」这件事后端已经用 `created:false` 表达完了（`§15 裁五`）。
        fields: &[
            "agent", "ccm_sid", "created", "cwd", "height", "mode", "name", "payload", "session",
            "typed", "width",
        ],
        takes_input: true,
        run: Run::Blocking(|r| crate::control::launch::launch_for_inbound(&r.args).map(Some)),
    },
    // 〔SR1a · 2026-09-24〕**链路四条** —— 用户裁「改成单一常驻后端」：本机只常驻一个后端，
    // 到各远端的 SSH 连接由它持有、按拨号身份复用（`dial/pool.rs`）；monitor 经这条流开「链路」，
    // 链路上的字节与 C2 那个 `--dial` 子进程的 stdout 逐字节同形（`dial/mod.rs` 头注）。
    // 四条都是 `Run::Builtin`：要碰本连接的链路表 ⇒ **只在帧面**，CLI 面不派生（一次性进程没有「连接」可言）。
    CommandSpec {
        name: "link-open",
        doc_anchor: Some("#### `link-open`"),
        codes: &[
            "invalid_args",
            "unsupported_use",
            "duplicate_link",
            "too_many_links",
        ],
        fields: &["dial", "link", "window"],
        takes_input: true,
        run: Run::Builtin,
    },
    CommandSpec {
        name: "link-data",
        doc_anchor: Some("#### `link-data`"),
        codes: &["invalid_args", "no_such_link", "link_busy", "link_closed"],
        fields: &["data", "link"],
        takes_input: true,
        run: Run::Builtin,
    },
    CommandSpec {
        name: "link-credit",
        doc_anchor: Some("#### `link-credit`"),
        codes: &["invalid_args", "no_such_link"],
        fields: &["bytes", "link"],
        takes_input: true,
        run: Run::Builtin,
    },
    CommandSpec {
        name: "link-close",
        doc_anchor: Some("#### `link-close`"),
        codes: &["invalid_args"],
        fields: &["link"],
        takes_input: true,
        run: Run::Builtin,
    },
    // 〔SR1b · 2026-09-24〕**传输四条** —— 用户 V89「SFTP 进本机常驻后端，只写暂存区」：传输台从 monitor 搬进
    // 本机常驻后端（`设计/60 §4.6`）。四条都是 `Run::Builtin`：要碰本连接的票表与应答通道 ⇒ **只在帧面**。
    CommandSpec {
        name: "transfer-upload",
        doc_anchor: Some("#### `transfer-upload`"),
        codes: &["bad_args", "io_failed", "busy", "too_many_transfers"],
        fields: &["dial", "id", "key", "local_path"],
        takes_input: true,
        run: Run::Builtin,
    },
    CommandSpec {
        name: "transfer-download",
        doc_anchor: Some("#### `transfer-download`"),
        codes: &["bad_args", "refused", "too_many_transfers"],
        fields: &["dial", "id", "local_path", "remote_path"],
        takes_input: true,
        run: Run::Builtin,
    },
    CommandSpec {
        name: "transfer-start",
        doc_anchor: Some("#### `transfer-start`"),
        codes: &["bad_args", "no_such_transfer", "already_started"],
        fields: &["id"],
        takes_input: true,
        run: Run::Builtin,
    },
    CommandSpec {
        name: "transfer-stop",
        doc_anchor: Some("#### `transfer-stop`"),
        codes: &["bad_args"],
        fields: &["id"],
        takes_input: true,
        run: Run::Builtin,
    },
    CommandSpec {
        name: "ping",
        doc_anchor: None,
        codes: &[],
        fields: &[],
        takes_input: false,
        run: Run::Async(|_r| Box::pin(async move { Ok(None) })),
    },
    // U6b-3：第一条**真业务命令**。
    // 一次性 `--resolve` 那条路**逐字不动** —— 契约与仓外 aterm 冻结在 2026-07-18，
    // 两条路复用同一个纯函数。⚠ 它的命令级错误码今天仍叫 `bad_request`（与协议级同名），
    // **刻意不改**：改它会破坏那份冻结的契约。如实登记。
    CommandSpec {
        name: "resolve",
        doc_anchor: Some("#### `resolve`"),
        // 〔V126 · TL2〕原来只列两个，而 `resolve_from_json` 还会回 `invalid_session_id` /
        // `unsafe_launch_candidate`（B2 两道校验）⇒ 登记表比真回的少两个。补齐；
        // 与跨仓承诺的码全集两向相等由 `resolve_query_tests.rs` 〔V126〕那一族钉着
        //（`stdin_read_failed` 只有一次性那条会出，不在这里）。
        codes: &[
            "bad_request",
            "invalid_session_id",
            "unsafe_launch_candidate",
            "serialize_failed",
        ],
        fields: &[],
        takes_input: true,
        run: Run::Async(|r| {
            Box::pin(async move {
                let input = serde_json::to_string(&r.args)
                    .map_err(|e| ("bad_request".to_string(), e.to_string()))?;
                crate::control::resolve_query::resolve_json_for_inbound(&input)
                    .map(Some)
                    .map_err(|(c, m)| (c.to_string(), m))
            })
        }),
    },
];

fn lookup(name: &str) -> Option<&'static CommandSpec> {
    REGISTRY.iter().find(|s| s.name == name)
}

/// 把一条命令交给独立 task 跑，并登记它的取消句柄。
///
/// 登记与摘除都在这里，处理器本身不用管取消 —— 取消靠 `AbortHandle`，
/// 处理器在任何 await 点被打断。
async fn spawn_handler<F, Fut>(
    req: Request,
    replies: mpsc::Sender<Frame>,
    running: Running,
    f: F,
    cancellable: bool,
) where
    F: FnOnce(Request) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = CmdResult> + Send,
{
    let id = req.id.clone();

    // ── ★ 拒重复 `id` ─────────────────────────────────────────────────────
    //
    // 旧版直接 `insert` 覆盖。D 审计实测两个后果，都很难被客户端发现：
    // ① 前一条命令的 `AbortHandle` 丢了 ⇒ **永远取消不掉**，而再发一次 cancel 照样回
    //    `ok:true`（"取消不存在的 id 是幂等的"这条规则把它盖住了）；
    // ② 先跑完的那条在 `remove(&id)` 时**把另一条的句柄摘了**。
    //
    // 文档把唯一性推给客户端（"谁生成谁负责唯一"），但后端侧对违约的反应是
    // **静默产生不可取消的僵尸任务 + 回一条看起来成功的应答** —— 那不是客户端的错能兜住的。
    // 现在明确拒绝。
    // 锁只在这一个语句里活着 —— **绝不跨 await**（那会让整个 future 变成 !Send）。
    // 检查与下面的登记之间没有并发：`dispatch` 只在**单一读循环** task 上跑。
    if lock(&running).contains_key(&id) {
        send(
            &replies,
            err(
                &id,
                "duplicate_id",
                &crate::common::contract::malformed("a command with this id is still running"),
            ),
        )
        .await;
        return;
    }

    // ── ★ 登记闸门：task 起来但**不许开跑**，直到句柄登记完成 ────────────────
    //
    // 旧版先 `spawn` 后 `insert`。生产是 multi_thread runtime，task 可以在 `insert`
    // 之前就跑完它自己的 `remove` ⇒ 那条 `insert` 把一个**已完成任务的空壳句柄**
    // 永久留在表里。D 审计实测：20 000 条命令全部回完应答后，表里还剩 **1964 个句柄**。
    //
    // 后果不止泄漏 —— 空壳让 `cancel` **撒谎**：对一条早就成功回过 `ok` 的命令
    // 发 cancel，会收到一条 `cancelled`，客户端把它记成"被取消了"。
    let (gate_tx, gate_rx) = tokio::sync::oneshot::channel::<()>();
    let id_for_task = id.clone();
    let running_for_task = running.clone();
    let id_sup = id.clone();
    let running_sup = running.clone();
    let replies_sup = replies.clone();
    let task = tokio::spawn(async move {
        let _ = gate_rx.await;
        let frame = match f(req).await {
            Ok(data) => Frame::Reply {
                id: id_for_task.clone(),
                ok: true,
                code: None,
                message: None,
                data,
            },
            Err((code, message)) => err(&id_for_task, &code, &message),
        };
        // 先摘登记再回应答：反过来的话，客户端收到应答后立刻发 cancel，
        // 可能命中一个已经跑完但还没摘掉的句柄，白 abort 一个空壳。
        lock(&running_for_task).remove(&id_for_task);
        let _ = replies.send(frame).await;
    });
    let abort = task.abort_handle();

    // ★ 监督 task：**处理器 panic 不许让客户端永远挂着。**
    //
    // 上面那个 task 里 `remove` 与 `send` 都在 `f(req).await` **之后**，panic 时两句
    // 都不执行 ⇒ 客户端等不到任何应答、登记表泄漏一个句柄。今天只有 `ping` 所以不可达，
    // 但本文件的定位就是「后面每条命令都骑上来的骨架」，U6b-3 接第一条真业务命令就活了。
    // 而模块头注写的是「任何失败都只回一条错误应答」。
    //
    // 用监督而不是 `catch_unwind`：后者要 `futures_util`，不值得为这个加一个依赖。
    // 监督还顺带兜住被 `cancel` abort 时的登记泄漏（`remove` 是幂等的）。
    tokio::spawn(async move {
        let outcome = task.await;
        lock(&running_sup).remove(&id_sup);
        if outcome.as_ref().is_err_and(|e| e.is_panic()) {
            let _ = replies_sup
                .send(err(
                    &id_sup,
                    "handler_panicked",
                    &copy_text("beInbound.spawnHandler.crashed", &[]),
                ))
                .await;
        }
        // 被 abort（= cancel 生效）时什么都不补：`Cancelled` 已经发过了。
    });

    lock(&running).insert(id, InFlight { abort, cancellable });
    let _ = gate_tx.send(()); // 登记落地之后才放行
}

/// 拿 `running` 的锁。**毒化了也继续用。**
///
/// 三处旧版写的是 `.expect("running 表被毒化")`。D 审计确认今天毒化不了
/// （临界区都是语句级、不跨 await），但一旦哪天有人在里面多写一句会 panic 的东西，
/// **读循环 task 会直接死，而且没有任何协议层信号** —— `main` 只 `abort()` 那个
/// JoinHandle、从不 `await` 它，只有默认 panic hook 往 stderr 打一行。
///
/// 这张表丢一致性无所谓（最坏是某条命令取消不掉），**读循环活着更重要**。
fn lock(
    m: &Mutex<HashMap<String, InFlight>>,
) -> std::sync::MutexGuard<'_, HashMap<String, InFlight>> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn ok(id: &str) -> Frame {
    Frame::Reply {
        id: id.to_string(),
        ok: true,
        code: None,
        message: None,
        data: None,
    }
}

fn err(id: &str, code: &str, message: &str) -> Frame {
    Frame::Reply {
        id: id.to_string(),
        ok: false,
        code: Some(code.to_string()),
        message: Some(message.to_string()),
        data: None,
    }
}

/// 发一帧应答。通道满 ⇒ 记一条 warn 就算了。
///
/// **不用 `try_send` 丢弃**：应答通道是独立的小通道（见 [`REPLY_CHANNEL_CAPACITY`]），
/// 它满意味着客户端连应答都读不过来，这时候阻塞住入方向**正是想要的**——
/// 让背压顶回去，而不是把应答丢掉让客户端空等。
async fn send(replies: &mpsc::Sender<Frame>, frame: Frame) {
    if replies.send(frame).await.is_err() {
        tracing::debug!("reply channel closed; client gone");
    }
}

#[cfg(test)]
#[path = "../../tests/backend/inbound_tests.rs"]
mod tests;

// 〔HX1〕退出排空的判据（闸本体 · 真子进程 ＋ 真信号 · 接线）。
// 〔p3q 门禁 winchk-backend 红后补〕判据里用了 `std::os::unix` / `libc::kill` / FIFO —— 只在 unix 上编；Windows 那一形的排空没量（HX1.md 买不到）。
#[cfg(all(test, unix))]
#[path = "../../tests/backend/drain_tests.rs"]
mod drain_tests;

/// 结构性机检：不测行为，测「代码的形状不许变回去」。
///
/// # U6b-3 删掉了这里原有的两条
///
/// `hello_is_flushed_before_the_inbound_reader_starts`（比较 `main.rs` 里两个字符串的字节位置）
/// 与 `handlers_never_run_on_the_reader_task`（扫 `dispatch` 的分派臂文本）
/// **都被 D 审计用普通重构击穿**：前者被一次函数抽取绕过，后者被尾随注释 / 同臂混用 / 或模式绕过。
///
/// 两条约束现在是**编译期不可表示**的：
/// - Hello 顺序 ⇒ [`crate::wire::HelloFlushed`] 见证（拿不到就调不了 `spawn`）；
/// - 处理器不许跑在读循环上 ⇒ `dispatch` **非 async**，分派臂里没有 `.await` 可写。
///
/// 不可表示之后那两条机检是死重量，删掉。**这是审计给的方向**：
/// 别再往判据上加正则，让违规不可表示。
#[cfg(test)]
#[path = "../../tests/backend/inbound_structure_guards.rs"]
mod structure_guards;
