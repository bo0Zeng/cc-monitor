//! 流模式收场：退出之前先排空开跑之后停不下来的那一档（闸 · 票 · 期限 · 三个退出口共用的收场）。

use std::sync::Mutex;

//
// ══════════════════════════════════════════════════════════════════════════
// 退出之前先排空停不下来的那一档
// ══════════════════════════════════════════════════════════════════════════
//
// 后端收 SIGTERM 先排空在飞写（有上限）再退：当场 `exit(0)` 会把 `spawn_blocking` 里正在写的那一条连线程一起带走
// ⇒ 存盘剩半份、递归删删一半、tmux 键入一半。三个退出口（stdio 写者断 / 收信号 / 最后一个客户走了且退出行为是「结束」）都走这里。
//
// 人群 = 在飞的 `Run::Blocking`，一条不多一条不少：那一档本来就是「开跑之后停不下来」的类型级声明（[`Disposition::SpawnBlocking`] 头注）；
// `Run::Async` 那一档随时可能被 `cancel` 打断，每个 await 点上都得是安全的 ⇒ 不另立「哪条算写」的分类表。
//
// 有上限（`INVARIANTS §48.2`「脱离后不留僵尸」）：后端自己兜一个退出排空期限 [`DRAIN_DEADLINE`]（30 秒），到点仍没排空 ⇒
// 记一行说哪几条没做完，然后退。这是后端零定时器让位的地方之一（登记在 `REGISTERED_DEADLINE_WAKES`）：远端后端在 SSH 断开那一刻没人叫它退、
// 也没人给上限，阻塞在挂死文件系统上的那一条会把进程无限期留下。第二次停机信号 = 立刻退；机器页「停」等得比它久一点
// （`--resident-stop` 的宽限期 `control/resident.rs::STOP_GRACE_MS`，35 秒），好让后端先把「哪几条没做完」说出来。

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

/// 闸关了之后新来的阻塞命令回的**协议级**码（与命令无关、只有入方向判得了 —— 同 `not_cancellable`）。
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
    // 后端零定时器让位的一处（`no_timer_guard::REGISTERED_DEADLINE_WAKES`）：只在收场时装这一次。
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
