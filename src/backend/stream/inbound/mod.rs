//! U6b-1：**流式连接上的入方向**——命令信封的读取、分派与取消。
//!
//! # 这一层归哪
//!
//! §1.1 的第二条线是 `observe/`（读）vs `control/`（做）。入方向有两层，**分开归**：
//!
//! - **传输层**（本目录）：读行、解析信封、长度上限、回显 `id`、管取消登记表。
//!   它跟 `wire.rs` 是同一类东西（协议管道），**不属于读也不属于做**，所以放顶层。
//!   塞进 `control/` 会让「control = 做事」那条线变浑。
//! - **每条命令的处理器**都**不在本目录里**。本目录只持有命令表（`registry/` 按族各一份）、调过去。
//!   今天它们住两处：会改变世界的那些住 `control/`；
//!   ⚠〔步 `24f` 第二刀 09-20〕`files-read` 那四条住顶层 `files/` ——
//!   它们整族纯读，塞进 `control/`（=「做事」）会把那条线弄浑，
//!   而 `observe/` 又被下面那条硬约束挡着。理由全文在 `REGISTRY` 上它们那一段。
//!
//! ⇒ 依赖方向 `inbound → control`，与既有的 `observe → control` 同向，
//! `layering_guard` 的判据不需要放宽。本目录**不许出现任何 `observe::`**（读面的事不归它）。
//!
//! # 为什么需要入方向（实证，不是推测）
//!
//! 没有它已经在制造绕路：
//! - **`--tmux-notify` 存在的唯一理由**就是 tmux hook 子进程没法给正在跑的后端发消息，
//!   只能新起一个进程、校验身份、发信号。
//! - **`--resolve` 为一次极小的 RPC 单开一整条 SSH exec。**
//!
//! 载体是现成的：monitor 那头拿的是 `russh::ChannelStream`，**双工**，
//! 而 `stream_source/` 里 `stdin` 零命中 —— 那半条通道从来没人用过。
//!
//! # 信任边界
//!
//! 出方向后端是唯一写者；入方向它变成**读取不可信输入**的一方。三条硬约束：
//! 单行长度上限（不缓冲、不 OOM）· 未见 Hello 之前不许有 stdin（时序，机检钉住）·
//! 坏行只回错误、**绝不结束进程**。

use crate::stream::wire::{Frame, Request};
use copy_core::copy_text;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

mod caps;
mod cli_only;
mod doors;
mod drain;
mod sniff;
pub(crate) mod spec;

pub(crate) use caps::install as install_total;
pub use cli_only::CLI_ONLY_DOCS;
pub use doors::watch_account_mcp;
pub(crate) use doors::LocalFiles;
pub use drain::{exit_after_drain, shutdown_listener, SHUTTING_DOWN};
#[cfg(test)]
use drain::{exit_after_drain_within, Drain};
pub(crate) use drain::{DRAIN, DRAIN_DEADLINE};
use sniff::sniff_id;
pub use sniff::ID_SNIFF_BYTES;
use spec::{BlockingHandler, BoxFut, DataHandler, Fail, Handler, Outcome};
pub(crate) use spec::{CommandSpec, Run};
// 字段的向只有协议文档生成器读（测试档）。
#[cfg(test)]
pub(crate) use spec::Dir;

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
/// 那是假的** —— `session_added`/`session_removed`/`session_state` 是一次差分 / 裁决的结果、
/// 别处不存在（audit-0805 B-3）。那半今天靠 `Overflow.lost` 带身份让客户端重同步；
/// **丢一条应答会让客户端永远等下去**。两者混在同一个通道里，实时行的洪峰会把应答挤掉。
/// 所以给应答一条独立的小通道，writer 两边都收。
pub const REPLY_CHANNEL_CAPACITY: usize = 256;

/// U6b-2：本 backend **接受的命令集**，随 `hello` 上线（`commands` 字段）—— 从 [`REGISTRY`] 取名、按字母排。
///
/// 能力协商此前只有出方向那一半（`capabilities` 说「我认识哪些流 flag」）。
/// 入方向同样需要：客户端得知道发什么过去才有人接，否则只能试错。
/// 名字与处理器绑在同一个值里（[`CommandSpec`]），这里只是取名 —— 没有第二份手抄的名单。
pub fn command_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = REGISTRY.iter().map(|s| s.name).collect();
    names.sort_unstable();
    names
}

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

/// 起入方向 reader。
///
/// ★ **Hello 必须已经 flush** —— 那不是一条纪律，是一个**参数**：
/// [`crate::stream::wire::HelloFlushed`] 只能由 `wire::write_and_flush_hello` 产出，
/// 拿不到它就调不了本函数。顺序因此**编译期不可表示**。
///
/// U6b-1 曾用一条比较 `main.rs` 里两个字符串字节位置的机检来钉它，
/// D 审计用一次**普通的函数抽取**就绕过了（把调用点包进一个放在文件后段的函数）。
/// 那条机检已随本改动删掉 —— 不可表示之后它是死重量。
pub fn spawn<R>(
    stdin: R,
    replies: mpsc::Sender<Frame>,
    _hello_flushed: crate::stream::wire::HelloFlushed,
) -> tokio::task::JoinHandle<()>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let running: Running = Arc::new(Mutex::new(HashMap::new()));
        // 本连接的链路表：随本读循环一起死 ⇒ monitor 走了，它开的链路一条不留
        // （`dial::link::Table` 的 `Drop`）。
        let links = crate::dial::link::Table::new(replies.clone());
        // 本连接的传输票表：同上，随本读循环一起死 ⇒ monitor 走了，它开的传输一律撤
        // （`control::transfer::Desk` 的 `Drop`）。
        let xfers = crate::control::transfer::Desk::new(replies.clone());
        // 本连接的终端订阅票表：同上，随本读循环一起死 ⇒ monitor 走了，它订的终端画面一律退订
        // （`control::terminal_follow::Desk` 的 `Drop`）。
        let follows = crate::control::terminal_follow::Desk::new(replies.clone());
        let mut rd = BufReader::new(stdin);
        let mut buf: Vec<u8> = Vec::new();
        // 本行是否已经超限。超限之后**只丢字节、不再往 buf 里塞**（O(1) 内存）。
        let mut overflowed = false;
        // 超限那一刻从行首抠出来的 `id`（抠不出 ⇒ 空串，见 [`sniff_id`]）。
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
                    Frame::err(
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
                handle_line(&buf, &replies, &running, &links, &xfers, &follows).await;
            }
            buf.clear();
        }
    })
}

/// 处理一行。**任何失败都只回一条错误应答，绝不 panic、绝不结束读循环。**
async fn handle_line(
    raw: &[u8],
    replies: &mpsc::Sender<Frame>,
    running: &Running,
    links: &crate::dial::link::Table,
    xfers: &crate::control::transfer::Desk,
    follows: &crate::control::terminal_follow::Desk,
) {
    if raw.is_empty() {
        return; // 空行（含 CRLF 的裸 \r 之后）静默跳过
    }
    let mut req: Request = match serde_json::from_slice(raw) {
        Ok(r) => r,
        Err(e) => {
            // 坏 JSON 时 `id` 无从得知 —— 回空 id，客户端按「上一条没应答」超时处理。
            send(replies, Frame::err("", "bad_request", &e.to_string())).await;
            return;
        }
    };
    // 发起方期限从收到这一行起算：减余量换成截止时刻，这条命令里装总期限的各处都收紧到它。
    req.until = caps::until_of(req.within_ms);
    match dispatch(req, replies, running, links, xfers, follows) {
        Disposition::Done => {}
        Disposition::Reply(f) => send(replies, f).await,
        Disposition::Spawn(req, run) => {
            let fut = move |r: Request| async move { run(r).await.map_err(Fail::from) };
            spawn_handler(req, replies.clone(), running.clone(), fut, true).await
        }
        Disposition::SpawnData(req, run) => {
            spawn_handler(req, replies.clone(), running.clone(), run, true).await
        }
        // ★ 同步阻塞处理器：进 `spawn_blocking` 的专用线程池，**不占 tokio worker**。
        //   `cancellable: false` —— `spawn_blocking` 起的活 abort 不了，说实话。
        Disposition::SpawnBlocking(req, run) => {
            // 取票在起跑之前：闸关了（进程在收场）⇒ 一个字节不动、回协议级 `shutting_down`。
            let Some(ticket) = DRAIN.enter(format!("{}（id={}）", req.cmd, req.id)) else {
                send(
                    replies,
                    Frame::err(
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
                    let _total = caps::install(&r);
                    run(r)
                })
                .await
                {
                    Ok(res) => res,
                    Err(e) => Err(Fail::from((
                        "handler_panicked".to_string(),
                        copy_text("beInbound.handleLine.internal", &[("e", &e.to_string())]),
                    ))),
                }
            };
            spawn_handler(req, replies.clone(), running.clone(), fut, false).await
        }
    }
}

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
    /// 同 [`Disposition::Spawn`]，失败可带 `data`。
    SpawnData(Request, DataHandler),
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
    follows: &crate::control::terminal_follow::Desk,
) -> Disposition {
    match req.cmd.as_str() {
        // 链路四条：要碰**本连接的链路表**与应答通道 ⇒ 与 `cancel` 同一档（硬臂、就地做完）。
        // ★ `link-data` **必须就地**（不 `spawn`）：同一条链路的上行块按到达顺序进队，
        //   交给独立 task 就不再保序。它成功时的应答由上行泵在写进管子之后发（背压）。
        "link-open" => Disposition::Reply(links.open(&req.id, &req.args)),
        "link-data" => match links.data(&req.id, &req.args) {
            Some(f) => Disposition::Reply(f),
            None => Disposition::Done,
        },
        "link-credit" => Disposition::Reply(links.credit(&req.id, &req.args)),
        "link-close" => Disposition::Reply(links.close(&req.id, &req.args)),
        // 传输四条：要碰**本连接的票表**与应答通道（进度帧走应答通道）⇒ 同一档硬臂。
        //   开单 / 起跑 / 撤都是就地做完的记账（起跑那一下 `spawn` 两个任务，不 await）。
        // 测试连接：进度格走**本连接的应答通道**（不丢、与应答同序）⇒ 与传输四条同一档硬臂；
        //   本体照旧是真异步、`cancel` 能在 await 点打断（交给通用的 spawn 那一路登记）。
        "remote-probe" => {
            let tx = replies.clone();
            Disposition::Spawn(
                req,
                Box::new(move |r: Request| -> BoxFut {
                    Box::pin(async move {
                        crate::dial::probe::answer_probe(&r.args, &tx)
                            .await
                            .map(|()| None)
                            .map_err(|(c, m)| (c.to_string(), m))
                    })
                }),
            )
        }
        "transfer-upload" | "transfer-download" | "transfer-start" | "transfer-stop" => {
            Disposition::Reply(crate::control::transfer::Desk::answer_wire(
                xfers, &req.cmd, &req.id, &req.args,
            ))
        }
        // 终端实时预览三条：要碰**本连接的订阅票表**与应答通道（画面帧走应答通道）⇒ 同一档硬臂。
        //   订上那一下要起几个 tmux（名单 · 版本 · 控制模式客户端）⇒ 异步档里挪进阻塞线程池、带一份票表过去；
        //   回执与退订是就地做完的记账。
        crate::control::terminal_follow::FOLLOW => {
            let desk = follows.clone();
            Disposition::Spawn(
                req,
                Box::new(move |r: Request| -> BoxFut {
                    Box::pin(async move {
                        desk.follow_off_worker(r.args)
                            .await
                            .map(|()| None)
                            .map_err(|(c, m)| (c.to_string(), m))
                    })
                }),
            )
        }
        crate::control::terminal_follow::FOLLOW_ACK | crate::control::terminal_follow::UNFOLLOW => {
            Disposition::Reply(follows.answer_wire(&req.cmd, &req.id, &req.args))
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
                let _ = replies.try_send(Frame::err(
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
            let _ = replies.try_send(Frame::ok(&req.id));
            Disposition::Done
        }
        // U8a-2d：其余命令**一律查注册表**，不再是一串手写臂。
        //
        // 这一步换掉的是 `hello_commands_match_the_dispatch_table` —— 那条机检扫的是
        // 分派臂的**文本**（按 8 空格缩进切），既对 rustfmt 脆，又只能事后比对。
        // 现在名字与处理器绑在**同一个值**里 ⇒ 「声明了却不接」「接了却不声明」
        // 在注册表这一侧不可表示；`hello.commands` 也从注册表派生（[`command_names`]），没有第二份名单。
        other => match lookup(other) {
            Some(spec) => match spec.run {
                Run::Async(f) => Disposition::Spawn(req, Box::new(f)),
                Run::AsyncData(f) => Disposition::SpawnData(req, Box::new(f)),
                Run::Blocking(f) => {
                    Disposition::SpawnBlocking(req, Box::new(move |r| f(r).map_err(Fail::from)))
                }
                Run::BlockingData(f) => Disposition::SpawnBlocking(req, Box::new(f)),
                // `cancel` 在上面那条硬臂里处理完了，走不到这儿。
                Run::Builtin => Disposition::Reply(Frame::err(
                    &req.id,
                    "unknown_command",
                    &copy_text(
                        "beInbound.dispatch.unhandled",
                        &[("other", &other.to_string())],
                    ),
                )),
            },
            None => Disposition::Reply(Frame::err(
                &req.id,
                "unknown_command",
                // 说这句的就是那台不认的后端，它不知道 monitor 怎么称呼它 ⇒「该机」。
                &copy_core::backend_old(&copy_core::peer_machine()),
            )),
        },
    }
}

/// 撤不动的那几条：阻塞档（`Run::Blocking`）开跑之后打不断，`cancel` 命中回 `not_cancellable`。
/// hello 的 `uncancellable` 就是它 —— 与 `dispatch` 按档位分流读的是同一张表。
/// 按字母排：它是一个集合，次序与命令登记在哪一族无关。
pub fn uncancellable() -> Vec<String> {
    let mut names: Vec<String> = REGISTRY
        .iter()
        .filter(|s| matches!(s.run, Run::Blocking(_) | Run::BlockingData(_)))
        .map(|s| s.name.to_string())
        .collect();
    names.sort();
    names
}

/// 命令表按族分住 `registry/` 下各一份（每份一张 `SPECS`），在编译期拼回一张 [`REGISTRY`]。
pub(crate) mod registry {
    pub(super) mod accounts;
    pub(super) mod aliases;
    pub(super) mod assets;
    pub(super) mod bus;
    // 文件管理那一族不叫 `files`：那是后端顶层文件读面的模块名，族名与它同名会让「谁够得到 `files::`」那条边界判据认错人。
    pub(super) mod file_manager;
    pub(super) mod history;
    pub(super) mod link;
    pub(super) mod machine;
    pub(super) mod terminals;

    /// 族的全集：`registry/` 下每一份文件恰好一行（`inbound_structure_guards` 两向钉住）。次序就是 [`super::REGISTRY`] 里的次序。
    pub(crate) const FAMILIES: &[&[super::CommandSpec]] = &[
        link::SPECS,
        file_manager::SPECS,
        accounts::SPECS,
        history::SPECS,
        assets::SPECS,
        aliases::SPECS,
        bus::SPECS,
        terminals::SPECS,
        machine::SPECS,
    ];
}

/// 各族条数之和（[`REGISTRY`] 的长度）。
const fn registry_len(families: &[&[CommandSpec]]) -> usize {
    let mut n = 0;
    let mut i = 0;
    while i < families.len() {
        n += families[i].len();
        i += 1;
    }
    n
}

/// 按族的次序把各族的登记拷进一张定长表。
const fn concat_families<const N: usize>(families: &[&[CommandSpec]]) -> [CommandSpec; N] {
    let mut out = [families[0][0]; N];
    let mut k = 0;
    let mut i = 0;
    while i < families.len() {
        let mut j = 0;
        while j < families[i].len() {
            out[k] = families[i][j];
            k += 1;
            j += 1;
        }
        i += 1;
    }
    out
}

/// **单一事实源。** `dispatch` 从它查，`hello` 的 `commands` / `uncancellable` 从它派生。
pub const REGISTRY: &[CommandSpec] =
    &concat_families::<{ registry_len(registry::FAMILIES) }>(registry::FAMILIES);

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
    Fut: std::future::Future<Output = Outcome> + Send,
{
    let id = req.id.clone();
    let cmd = req.cmd.clone();
    let args = req.args.clone();

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
            Frame::err(
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
                detail: None,
                data,
            },
            Err(f) => f.into_reply(id_for_task.clone(), &cmd, &args),
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
                .send(Frame::err(
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
#[path = "../../../../tests/backend/stream/inbound_tests.rs"]
mod tests;

// 阻塞档命令的总期限：信封里的发起方期限 · 后端登记的上限。
#[cfg(test)]
#[path = "../../../../tests/backend/stream/caps_tests.rs"]
mod caps_tests;

// 退出排空的判据（闸本体 · 真子进程 ＋ 真信号 · 接线）。
// 〔p3q 门禁 winchk-backend 红后补〕判据里用了 `std::os::unix` / `libc::kill` / FIFO —— 只在 unix 上编；Windows 那一形的排空没量（HX1.md 买不到）。
#[cfg(all(test, unix))]
#[path = "../../../../tests/backend/stream/drain_tests.rs"]
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
/// - Hello 顺序 ⇒ [`crate::stream::wire::HelloFlushed`] 见证（拿不到就调不了 `spawn`）；
/// - 处理器不许跑在读循环上 ⇒ `dispatch` **非 async**，分派臂里没有 `.await` 可写。
///
/// 不可表示之后那两条机检是死重量，删掉。**这是审计给的方向**：
/// 别再往判据上加正则，让违规不可表示。
#[cfg(test)]
#[path = "../../../../tests/backend/stream/inbound_structure_guards.rs"]
mod structure_guards;
