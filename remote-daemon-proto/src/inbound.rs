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
pub const MAX_LINE_BYTES: usize = 1 << 20;

/// 应答通道容量。**刻意与出方向的 `CHANNEL_CAPACITY`（10_000）分开**。
///
/// 出方向丢一帧是可恢复的（`Overflow` 会告诉客户端丢了多少，行还在远端 jsonl 里）；
/// **丢一条应答会让客户端永远等下去**。两者混在同一个通道里，实时行的洪峰会把应答挤掉。
/// 所以给应答一条独立的小通道，writer 两边都收。
pub const REPLY_CHANNEL_CAPACITY: usize = 256;

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
        let mut lines = BufReader::new(stdin).split(b'\n');
        loop {
            let raw = match lines.next_segment().await {
                Ok(Some(v)) => v,
                Ok(None) => break, // 客户端关了写半边：正常寿终
                Err(e) => {
                    tracing::warn!("inbound read failed ({e}); stopping reader");
                    break;
                }
            };
            handle_line(&raw, &replies, &running).await;
        }
    })
}

/// 处理一行。**任何失败都只回一条错误应答，绝不 panic、绝不结束读循环。**
async fn handle_line(raw: &[u8], replies: &mpsc::Sender<Frame>, running: &Running) {
    if raw.is_empty() {
        return; // 空行（含 CRLF 的裸 \r 之后）静默跳过
    }
    if raw.len() > MAX_LINE_BYTES {
        // 超长行**不解析**——解析它本身就是被攻击面。`id` 拿不到，回空 id 的错误。
        send(
            replies,
            err(
                "",
                "line_too_long",
                &format!("单行 {} 字节，超过上限 {MAX_LINE_BYTES}", raw.len()),
            ),
        )
        .await;
        return;
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
            let handle = running.lock().expect("running 表被毒化").remove(&target);
            match handle {
                Some(h) => {
                    h.abort();
                    send(replies, Frame::Cancelled { id: target }).await;
                    send(replies, ok(&req.id)).await;
                }
                None => {
                    // 已经跑完 / 从没有过 —— 都不是错误：取消一个不在的东西是幂等的。
                    send(replies, ok(&req.id)).await;
                }
            }
        }
        "ping" => {
            spawn_handler(req, replies.clone(), running.clone(), |_req| async move {
                Ok(())
            });
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
fn spawn_handler<F, Fut>(req: Request, replies: mpsc::Sender<Frame>, running: Running, f: F)
where
    F: FnOnce(Request) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<(), (String, String)>> + Send,
{
    let id = req.id.clone();
    let id_for_task = id.clone();
    let running_for_task = running.clone();
    let task = tokio::spawn(async move {
        let frame = match f(req).await {
            Ok(()) => ok(&id_for_task),
            Err((code, message)) => err(&id_for_task, &code, &message),
        };
        // 先摘登记再回应答：反过来的话，客户端收到应答后立刻发 cancel，
        // 可能命中一个已经跑完但还没摘掉的句柄，白 abort 一个空壳。
        running_for_task
            .lock()
            .expect("running 表被毒化")
            .remove(&id_for_task);
        let _ = replies.send(frame).await;
    });
    running
        .lock()
        .expect("running 表被毒化")
        .insert(id, task.abort_handle());
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
