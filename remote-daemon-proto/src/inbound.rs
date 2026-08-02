//! U6b-1：**流式连接上的入方向**——命令信封的读取、分派与取消。
//!
//! # 这一层归哪
//!
//! §1.1 的第二条线是 `observe/`（读）vs `control/`（做）。入方向有两层，**分开归**：
//!
//! - **传输层**（本文件）：读行、解析信封、长度上限、回显 `id`、管取消登记表。
//!   它跟 `wire.rs` 是同一类东西（协议管道），**不属于读也不属于做**，所以放顶层。
//!   塞进 `control/` 会让「control = 做事」那条线变浑。
//! - **每条命令的处理器**属于 `control/`。本文件只持有命令表、调过去。
//!
//! ⇒ 依赖方向 `inbound → control`，与既有的 `observe → control` 同向，
//! `layering_guard` 的判据不需要放宽。本文件**不许出现任何 `observe::`**（读面的事不归它）。
//!
//! # 为什么需要入方向（实证，不是推测）
//!
//! 没有它已经在制造绕路：
//! - **`--tmux-notify` 存在的唯一理由**就是 tmux hook 子进程没法给正在跑的 daemon 发消息，
//!   只能新起一个进程、校验身份、发信号。
//! - **`--resolve` 为一次极小的 RPC 单开一整条 SSH exec。**
//!
//! 载体是现成的：monitor 那头拿的是 `russh::ChannelStream`，**双工**，
//! 而 `ssh_source.rs` 里 `stdin` 零命中 —— 那半条通道从来没人用过。
//!
//! # 信任边界
//!
//! 出方向 daemon 是唯一写者；入方向它变成**读取不可信输入**的一方。三条硬约束：
//! 单行长度上限（不缓冲、不 OOM）· 未见 Hello 之前不许有 stdin（时序，机检钉住）·
//! 坏行只回错误、**绝不结束进程**。

use crate::wire::{Frame, Request};
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
/// 出方向丢一帧是可恢复的（`Overflow` 会告诉客户端丢了多少，行还在远端 jsonl 里）；
/// **丢一条应答会让客户端永远等下去**。两者混在同一个通道里，实时行的洪峰会把应答挤掉。
/// 所以给应答一条独立的小通道，writer 两边都收。
pub const REPLY_CHANNEL_CAPACITY: usize = 256;

/// U6b-2：本 daemon **接受的命令集**，随 `hello` 上线（`commands` 字段）。
///
/// 能力协商此前只有出方向那一半（`capabilities` 说「我认识哪些流 flag」）。
/// 入方向同样需要：客户端得知道发什么过去才有人接，否则只能试错。
///
/// **这是单一真相源** —— `hello` 从这里取值，`dispatch` 必须恰好处理这些。
/// 两者由 `hello_commands_match_the_dispatch_table` 钉住，不许各写各的。
pub const COMMANDS: &[&str] = &["cancel", "ping"];

/// 在跑的命令登记表：`id` → 取消句柄。
///
/// `id` 是客户端给的**不透明串**——daemon 不解析、不校验格式、只当 map 的键和回显值。
/// 谁生成谁负责唯一。daemon 自己发号的话重连后号段会撞（同 F90「不许拿会变的东西当持久键」）。
type Running = Arc<Mutex<HashMap<String, tokio::task::AbortHandle>>>;

/// 起入方向 reader。
///
/// ★ **必须在 Hello 已 flush 之后调用**——`hello_is_flushed_before_stdin_is_read`
/// 这条机检钉住调用顺序。客户端要先读到 Hello 才知道对面版本与能力；
/// 抢在 Hello 之前写命令，等于在能力协商之前就把命令执行了。
pub fn spawn<R>(stdin: R, replies: mpsc::Sender<Frame>) -> tokio::task::JoinHandle<()>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let running: Running = Arc::new(Mutex::new(HashMap::new()));
        let mut rd = BufReader::new(stdin);
        let mut buf: Vec<u8> = Vec::new();
        // 本行是否已经超限。超限之后**只丢字节、不再往 buf 里塞**（O(1) 内存）。
        let mut overflowed = false;
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
                        "",
                        "line_too_long",
                        &format!("单行超过上限 {MAX_LINE_BYTES} 字节，已整行丢弃"),
                    ),
                )
                .await;
                overflowed = false;
            } else {
                handle_line(&buf, &replies, &running).await;
            }
            buf.clear();
        }
    })
}

/// 处理一行。**任何失败都只回一条错误应答，绝不 panic、绝不结束读循环。**
async fn handle_line(raw: &[u8], replies: &mpsc::Sender<Frame>, running: &Running) {
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
    dispatch(req, replies, running).await;
}

/// 命令表。
///
/// ★ **每个处理器都必须离开读循环所在的 task 去跑**（`tokio::spawn`）——
/// `handlers_never_run_on_the_reader_task` 这条机检钉住。
/// 读循环被一条慢命令占住 ⇒ 后续命令全排队，而症状会表现成「远端没反应」，
/// 几乎不可能归因到某一条命令上。同 `run_tmux_ls` 头注那条纪律。
///
/// 只有 `cancel` 例外并且**必须**例外：它就是用来打断别的命令的，
/// 自己再排到那些命令后面去就永远不会生效。它是纯 map 操作，不阻塞。
async fn dispatch(req: Request, replies: &mpsc::Sender<Frame>, running: &Running) {
    match req.cmd.as_str() {
        "cancel" => {
            let target = req
                .args
                .get("target")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let handle = lock(running).remove(&target);
            if let Some(h) = handle {
                h.abort();
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
        }
        "ping" => {
            spawn_handler(req, replies.clone(), running.clone(), |_req| async move {
                Ok(())
            })
            .await;
        }
        other => {
            send(
                replies,
                err(&req.id, "unknown_command", &format!("未知命令 `{other}`")),
            )
            .await;
        }
    }
}

/// 把一条命令交给独立 task 跑，并登记它的取消句柄。
///
/// 登记与摘除都在这里，处理器本身不用管取消 —— 取消靠 `AbortHandle`，
/// 处理器在任何 await 点被打断。
async fn spawn_handler<F, Fut>(req: Request, replies: mpsc::Sender<Frame>, running: Running, f: F)
where
    F: FnOnce(Request) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<(), (String, String)>> + Send,
{
    let id = req.id.clone();

    // ── ★ 拒重复 `id` ─────────────────────────────────────────────────────
    //
    // 旧版直接 `insert` 覆盖。D 审计实测两个后果，都很难被客户端发现：
    // ① 前一条命令的 `AbortHandle` 丢了 ⇒ **永远取消不掉**，而再发一次 cancel 照样回
    //    `ok:true`（"取消不存在的 id 是幂等的"这条规则把它盖住了）；
    // ② 先跑完的那条在 `remove(&id)` 时**把另一条的句柄摘了**。
    //
    // 文档把唯一性推给客户端（"谁生成谁负责唯一"），但 daemon 侧对违约的反应是
    // **静默产生不可取消的僵尸任务 + 回一条看起来成功的应答** —— 那不是客户端的错能兜住的。
    // 现在明确拒绝。
    // 锁只在这一个语句里活着 —— **绝不跨 await**（那会让整个 future 变成 !Send）。
    // 检查与下面的登记之间没有并发：`dispatch` 只在**单一读循环** task 上跑。
    if lock(&running).contains_key(&id) {
        send(
            &replies,
            err(&id, "duplicate_id", "同一个 id 还有命令在跑；换一个"),
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
            Ok(()) => ok(&id_for_task),
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
                    "命令处理器 panic 了；daemon 仍在跑",
                ))
                .await;
        }
        // 被 abort（= cancel 生效）时什么都不补：`Cancelled` 已经发过了。
    });

    lock(&running).insert(id, abort);
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
    m: &Mutex<HashMap<String, tokio::task::AbortHandle>>,
) -> std::sync::MutexGuard<'_, HashMap<String, tokio::task::AbortHandle>> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn ok(id: &str) -> Frame {
    Frame::Reply {
        id: id.to_string(),
        ok: true,
        code: None,
        message: None,
    }
}

fn err(id: &str, code: &str, message: &str) -> Frame {
    Frame::Reply {
        id: id.to_string(),
        ok: false,
        code: Some(code.to_string()),
        message: Some(message.to_string()),
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
mod tests {
    use super::*;

    fn chan() -> (mpsc::Sender<Frame>, mpsc::Receiver<Frame>) {
        mpsc::channel(REPLY_CHANNEL_CAPACITY)
    }

    async fn one_line(input: &str) -> Vec<String> {
        let (tx, mut rx) = chan();
        let h = spawn(std::io::Cursor::new(input.as_bytes().to_vec()), tx);
        h.await.expect("reader task");
        let mut out = Vec::new();
        // 用 `recv().await` 直到 `None`，不用 `try_recv()`：处理器跑在**独立 task** 上
        // （那正是本模块的纪律），`try_recv` 会在它还没跑完时读到空。
        // sender 只有 reader task 与它 spawn 的处理器两处，都结束后 `recv()` 返回 `None`
        // ⇒ 确定性收尾，不需要 sleep、也不需要「空了就重跑一次」那种凑合法
        // （那种测试**可能因为错误的原因通过**）。
        while let Some(f) = rx.recv().await {
            out.push(
                crate::wire::to_line(&f)
                    .expect("serialize")
                    .trim_end()
                    .to_string(),
            );
        }
        out
    }

    #[tokio::test]
    async fn ping_replies_ok() {
        let out = one_line("{\"id\":\"x\",\"cmd\":\"ping\"}\n").await;
        // 逐字节钉死：顺带把「`code`/`message` 为 None 时不上线」也钉住了。
        assert_eq!(out, vec![r#"{"kind":"reply","id":"x","ok":true}"#]);
    }

    /// ★ `id` 是**不透明**的：daemon 不解析、不规范化、只回显。
    #[tokio::test]
    async fn id_is_echoed_back_byte_for_byte() {
        for id in ["🌊-emoji", "0123456789", &"z".repeat(500)] {
            let line = format!(
                "{{\"id\":{},\"cmd\":\"nope\"}}\n",
                serde_json::to_string(id).unwrap()
            );
            let out = one_line(&line).await;
            let want = format!("\"id\":{}", serde_json::to_string(id).unwrap());
            assert!(
                out.iter().any(|l| l.contains(&want)),
                "id {id:?} 没被逐字回显：{out:?}"
            );
        }
    }

    #[tokio::test]
    async fn unknown_command_is_an_error_reply_not_a_crash() {
        let out = one_line("{\"id\":\"a\",\"cmd\":\"rm-rf\"}\n").await;
        assert!(
            out.iter()
                .any(|l| l.contains("unknown_command") && l.contains("\"ok\":false")),
            "{out:?}"
        );
    }

    /// ★ 坏行**不许**拖垮读循环 —— 后面那条好行必须照常处理。
    #[tokio::test]
    async fn a_bad_line_does_not_stop_the_reader() {
        let out = one_line("not json at all\n{\"id\":\"after\",\"cmd\":\"nope\"}\n").await;
        assert!(
            out.iter().any(|l| l.contains("bad_request")),
            "坏行没回 bad_request：{out:?}"
        );
        assert!(
            out.iter().any(|l| l.contains("\"id\":\"after\"")),
            "坏行之后的好行没被处理 —— 读循环被拖垮了：{out:?}"
        );
    }

    /// ★ 超长行只丢它自己，进程活着、后面照常。
    #[tokio::test]
    async fn an_oversized_line_is_rejected_without_killing_the_reader() {
        let huge = format!(
            "{{\"id\":\"big\",\"cmd\":\"ping\",\"pad\":\"{}\"}}\n",
            "p".repeat(MAX_LINE_BYTES)
        );
        let out = one_line(&(huge + "{\"id\":\"after\",\"cmd\":\"nope\"}\n")).await;
        assert!(
            out.iter().any(|l| l.contains("line_too_long")),
            "超长行没被拒：{out:?}"
        );
        assert!(
            out.iter().any(|l| l.contains("\"id\":\"after\"")),
            "超长行之后的好行没被处理：{out:?}"
        );
    }

    /// ★ 喂一条**远超上限**的无换行流，进程内存不许跟着涨。
    ///
    /// 这条测的是 D 审计抓到的那件事：上限如果是「读完再判」，`line_too_long`
    /// 照样回、看起来全对，而 RSS 已经涨了一整条行那么多（实测 512 MiB）。
    /// 判据只能是**内存**，不能是应答内容 —— 应答内容在两种实现下一模一样。
    #[tokio::test]
    async fn an_oversized_line_does_not_grow_memory() {
        /// 懒生成的无换行流：自己不占内存，读多少造多少。
        struct Flood {
            left: usize,
            nl_sent: bool,
        }
        impl tokio::io::AsyncRead for Flood {
            fn poll_read(
                mut self: std::pin::Pin<&mut Self>,
                _cx: &mut std::task::Context<'_>,
                buf: &mut tokio::io::ReadBuf<'_>,
            ) -> std::task::Poll<std::io::Result<()>> {
                if self.left == 0 {
                    if !self.nl_sent {
                        // 末尾补一个换行：不补的话这一整段永远不构成「一行」，
                        // reader 读到 EOF 直接收尾、什么都不回 —— 那是对的行为
                        // （对端没发完整行就走了），但那样就测不到超限应答。
                        self.nl_sent = true;
                        buf.put_slice(b"\n");
                    }
                    return std::task::Poll::Ready(Ok(())); // EOF
                }
                let n = buf.remaining().min(self.left).min(64 * 1024);
                buf.initialize_unfilled_to(n);
                buf.advance(n);
                self.left -= n;
                std::task::Poll::Ready(Ok(()))
            }
        }
        /// 进程 RSS **高水位**（`VmHWM`）—— 内核维护、单调不降。
        ///
        /// 必须用高水位，不能用当前 RSS：
        /// - 读完再量 ⇒ 那块大 buffer 已经 free，glibc 把它 munmap 还给系统，RSS 掉回去了；
        /// - 边跑边采也不行 ⇒ `Flood` 永远 Ready，reader 在**一次 poll 里**读完 256 MiB，
        ///   采样循环根本插不进去。
        ///
        /// 这两种写法我都试过，**都是安慰剂**：把「全程累积、读完再判」这个真正的旧形状
        /// 变异回去，两版都照样绿。
        fn hwm_bytes() -> usize {
            std::fs::read_to_string("/proc/self/status")
                .unwrap_or_default()
                .lines()
                .find_map(|l| l.strip_prefix("VmHWM:"))
                .and_then(|v| v.split_whitespace().next()?.parse::<usize>().ok())
                .unwrap_or(0)
                * 1024
        }

        const FLOOD: usize = 256 * 1024 * 1024; // 256 MiB，是上限的 256 倍
        let before = hwm_bytes();
        assert!(before > 0, "读不到 VmHWM —— 本断言在空转（非 Linux？）");
        let (tx, mut rx) = chan();
        let h = spawn(
            Flood {
                left: FLOOD,
                nl_sent: false,
            },
            tx,
        );
        h.await.expect("reader task");
        let grew = hwm_bytes().saturating_sub(before);
        assert!(
            grew < 16 * 1024 * 1024,
            "喂 {} MiB 无换行的流，RSS 高水位涨了 {} MiB —— 上限是「读完再判」的，\n\
             它在整行进内存之后才生效（D 审计实测涨满 512 MiB）。",
            FLOOD / 1024 / 1024,
            grew / 1024 / 1024
        );
        // 顺带确认它确实**报了**超限，而不是悄悄吞掉。
        let mut saw = false;
        while let Ok(f) = rx.try_recv() {
            if crate::wire::to_line(&f)
                .unwrap_or_default()
                .contains("line_too_long")
            {
                saw = true;
            }
        }
        assert!(saw || FLOOD == 0, "超长流没回 line_too_long");
    }

    fn req(id: &str, cmd: &str) -> Request {
        Request {
            id: id.into(),
            cmd: cmd.into(),
            args: serde_json::Value::Null,
        }
    }

    /// ★ 登记表不许残留空壳。
    ///
    /// 复现 D 审计那条：旧版先 `spawn` 后 `insert`，multi_thread 下 task 可以在 `insert`
    /// 之前就跑完自己的 `remove` ⇒ 那条 `insert` 把已完成任务的句柄永久留在表里。
    /// 实测 20 000 条命令回完应答后**还剩 1964 个**。
    ///
    /// 后果不止泄漏：空壳让 `cancel` **撒谎** —— 对一条早就成功回过 `ok` 的命令发 cancel，
    /// 会收到一条 `cancelled`，客户端把它记成「被取消了」。
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn the_running_table_never_leaks_finished_handles() {
        const N: usize = 5_000;
        let (tx, mut rx) = chan();
        let running: Running = Arc::new(Mutex::new(HashMap::new()));
        for i in 0..N {
            spawn_handler(
                req(&format!("id-{i}"), "ping"),
                tx.clone(),
                running.clone(),
                |_r| async move { Ok(()) },
            )
            .await;
        }
        drop(tx);
        let mut got = 0usize;
        while rx.recv().await.is_some() {
            got += 1;
        }
        assert_eq!(got, N, "应答条数不对 —— 本断言在空转");
        let left = lock(&running).len();
        assert_eq!(
            left, 0,
            "{N} 条命令全部回完应答后，登记表里还剩 {left} 个句柄（空壳）"
        );
    }

    /// ★ 重复 `id` 必须**明确拒绝**，不许静默覆盖。
    ///
    /// 旧版 `insert` 覆盖 ⇒ 前一条永远取消不掉，而再发 cancel 照样回 `ok:true`
    /// （被「取消不存在的 id 是幂等的」那条规则盖住）；且先跑完的那条会把另一条的句柄摘走。
    #[tokio::test]
    async fn a_duplicate_id_is_rejected_instead_of_silently_overwriting() {
        let (tx, mut rx) = chan();
        let running: Running = Arc::new(Mutex::new(HashMap::new()));
        // 第一条：占住 id，直到测试收尾时放行。
        //
        // **不用 `std::future::pending()`**：那样 handler 与它的监督 task 永不结束，
        // 测试断言过了之后进程还挂着不退（实测卡 60s+，整个 test binary 收不了尾）。
        let (release, held) = tokio::sync::oneshot::channel::<()>();
        spawn_handler(
            req("dup", "ping"),
            tx.clone(),
            running.clone(),
            |_r| async move {
                let _ = held.await;
                Ok(())
            },
        )
        .await;
        // 第二条：同一个 id。
        spawn_handler(
            req("dup", "ping"),
            tx.clone(),
            running.clone(),
            |_r| async move { Ok(()) },
        )
        .await;
        drop(tx);
        // **不能排空到 `None`**：占位那条与它的监督 task 各持一个 sender，未放行前不落地。
        // 拒绝那条是**同步**回进通道的（`send` 在未满通道上立即完成），此刻 `try_recv` 必有。
        // 不用 `timeout`：本仓有零定时器纪律，确定性写法本来也比等超时好。
        let first = rx
            .try_recv()
            .expect("通道里没有应答 —— 重复 id 被静默吞了？");
        let line = crate::wire::to_line(&first).expect("serialize");
        assert!(
            line.contains("duplicate_id"),
            "重复 id 没被拒绝，被静默覆盖了：{line}"
        );
        assert_eq!(
            lock(&running).len(),
            1,
            "第一条应当仍在表里、仍可取消（重复的那条不该动它）"
        );
        let _ = release.send(()); // 放行占位那条，让它与监督 task 都收尾
    }

    /// ★ 处理器 panic ⇒ 客户端拿得到错误应答，登记表不泄漏。
    #[tokio::test]
    async fn a_panicking_handler_still_answers_the_client() {
        let (tx, mut rx) = chan();
        let running: Running = Arc::new(Mutex::new(HashMap::new()));
        spawn_handler(
            req("boom", "ping"),
            tx.clone(),
            running.clone(),
            |_r| async {
                panic!("处理器炸了");
            },
        )
        .await;
        drop(tx);
        let mut lines = Vec::new();
        while let Some(f) = rx.recv().await {
            lines.push(crate::wire::to_line(&f).expect("serialize"));
        }
        assert!(
            lines.iter().any(|l| l.contains("handler_panicked")),
            "处理器 panic 之后客户端什么都没收到 —— 它会永远挂着：{lines:?}"
        );
        assert_eq!(lock(&running).len(), 0, "panic 之后登记表泄漏了");
    }

    /// 取消一个不存在的 id 是**幂等**的，不是错误。
    #[tokio::test]
    async fn cancelling_an_unknown_id_is_idempotent_not_an_error() {
        let out =
            one_line("{\"id\":\"c\",\"cmd\":\"cancel\",\"args\":{\"target\":\"ghost\"}}\n").await;
        assert!(
            out.iter()
                .any(|l| l.contains("\"id\":\"c\"") && l.contains("\"ok\":true")),
            "{out:?}"
        );
    }
}

/// U6b-1 的两条**结构性机检**。都不测行为，测的是「代码的形状不许变回去」。
///
/// 为什么这两条必须是机检而不是注释：它们各自是**跨两处才成立**的约束 ——
/// U6a 刚吃过一次亏，PS 握手的顺序在四份文档里错了三份，因为只有注释没有判据。
#[cfg(test)]
mod structure_guards {
    /// 生产段（剥掉测试代码），供两条判据取用。
    fn main_rs() -> String {
        crate::guard_support::production_code(include_str!("main.rs"))
    }

    /// ★ Hello 必须在入方向 reader 起来**之前**就 flush 完。
    ///
    /// 客户端要先读到 Hello 才知道对面是什么版本、有什么能力（U6b-2 还要把
    /// 「接受哪些命令」也放进 Hello）。reader 先起来，就等于在能力协商之前收命令。
    ///
    /// 这是**时序约束**：两边单看都合理，只有合起来看才错 —— 同 U6a 抓到的
    /// PS 握手顺序那一族。取法也照抄那条（比较两处的字节位置）。
    #[test]
    fn hello_is_flushed_before_the_inbound_reader_starts() {
        let src = main_rs();
        let flush = src
            .find("failed to flush hello frame")
            .expect("main.rs 里找不到 Hello 的 flush —— 抽取坏了还是握手改了？");
        let reader = src
            .find("inbound::spawn(")
            .expect("main.rs 里找不到 inbound::spawn —— 入方向被摘了？");
        assert!(
            flush < reader,
            "入方向 reader 起在 Hello flush **之前**了。\n\
             客户端在读到 Hello（版本 / 能力）之前就可能被收走命令 —— \n\
             而 U6b-2 正要把「接受哪些命令」放进 Hello。\n\
             flush@{flush} reader@{reader}"
        );
    }

    /// ★ `hello.commands` 与 `dispatch` 的分派臂必须**完全一致**。
    ///
    /// 声明了却不接 = 客户端发过去石沉大海；接了却不声明 = 客户端不知道能用。
    /// 两边各写一份名单必然漂移 —— 这条让它们只能是同一份。
    #[test]
    fn hello_commands_match_the_dispatch_table() {
        let src = crate::guard_support::production_code(include_str!("inbound.rs"));
        let at = src.find("async fn dispatch(").expect("找不到 dispatch");
        let body_start = src[at..]
            .find('\n')
            .map(|k| at + k + 1)
            .unwrap_or(src.len());
        let mut end = src.len();
        let mut off = body_start;
        for line in src[body_start..].lines() {
            if !line.is_empty() && !line.starts_with(' ') {
                end = off;
                break;
            }
            off += line.len() + 1;
        }
        let marker = "\n        \"";
        let mut arms: Vec<String> = src[body_start..end]
            .split(marker)
            .skip(1)
            .filter_map(|a| a.split_once('"').map(|(n, _)| n.to_string()))
            .collect();
        arms.sort();
        assert!(
            arms.len() >= 2,
            "只切出 {} 条分派臂 —— 抽取坏了，本断言在空转",
            arms.len()
        );
        let mut declared: Vec<String> = super::COMMANDS.iter().map(|s| s.to_string()).collect();
        declared.sort();
        assert_eq!(
            arms, declared,
            "\n`hello.commands`（= inbound::COMMANDS）与 dispatch 的分派臂对不上。\n\
             声明了却不接 ⇒ 客户端发过去石沉大海；接了却不声明 ⇒ 客户端不知道能用。"
        );
    }

    /// 允许在读循环那个 task 里就地跑完的命令。**加进来必须写理由。**
    ///
    /// - `cancel`：它就是用来打断别的命令的。自己再排到那些命令后面去，
    ///   就永远不会生效。而且它是纯 map 操作，不阻塞。
    const MAY_RUN_INLINE: &[&str] = &["cancel"];

    /// ★ 除白名单外，每条命令都必须交给独立 task 跑。
    ///
    /// 读循环被一条慢命令占住 ⇒ 后续命令全排队；而症状会表现成「远端没反应」，
    /// 几乎不可能归因到某一条命令上。同 `run_tmux_ls` 头注那条纪律。
    #[test]
    fn handlers_never_run_on_the_reader_task() {
        let src = crate::guard_support::production_code(include_str!("inbound.rs"));
        let at = src.find("async fn dispatch(").expect("找不到 dispatch");
        // 区间到下一个列 0 的非空行为止（同 protocol_doc_guard 的取法，
        // 刻意不写那个不配对的右大括号字面量 —— 会把 readonly_guard 的括号配平提前收尾）。
        let body_start = src[at..]
            .find('\n')
            .map(|k| at + k + 1)
            .unwrap_or(src.len());
        let mut end = src.len();
        let mut off = body_start;
        for line in src[body_start..].lines() {
            if !line.is_empty() && !line.starts_with(' ') {
                end = off;
                break;
            }
            off += line.len() + 1;
        }
        let body = &src[body_start..end];

        // 分派臂形如 `        "name" =>`（8 空格缩进）。刻意不匹配后面那个左大括号。
        let marker = "\n        \"";
        let arms: Vec<&str> = body.split(marker).skip(1).collect();
        assert!(
            arms.len() >= 2,
            "只切出 {} 条分派臂 —— 抽取坏了，本断言在空转",
            arms.len()
        );

        let mut offenders: Vec<String> = Vec::new();
        for arm in arms {
            let Some((name, rest)) = arm.split_once('"') else {
                continue;
            };
            if MAY_RUN_INLINE.contains(&name) {
                continue;
            }
            if !rest.contains("spawn_handler") {
                offenders.push(name.to_string());
            }
        }
        assert!(
            offenders.is_empty(),
            "这些命令在读循环那个 task 里就地跑：{offenders:?}\n\
             一条慢命令会把后续所有命令堵住，而症状表现成「远端没反应」、\n\
             几乎归因不到具体哪条命令。要么走 `spawn_handler`，\n\
             要么加进 `MAY_RUN_INLINE` **并写明为什么它必须就地跑**。"
        );
    }
}
