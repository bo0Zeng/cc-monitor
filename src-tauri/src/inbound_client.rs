//! U8a-2a：monitor 侧的**入方向发送端** —— 往那条长连接的写半边发命令、按 `id` 收应答。
//!
//! # 它补上的是哪一半
//!
//! U6b-1/2/3 在 daemon 侧建了完整的入方向：信封、`id` 回显、取消、单行上限、能力协商。
//! 但 U8a-2 摸底实测出一件事：**monitor 侧一个字节都没往那条流的 stdin 写过**
//! （`ssh_source.rs` 里写半边的唯一用法是 `probe_daemon` 的 `shutdown()`，零数据字节）。
//! ⇒ 那条通道在生产里**不可达**。本模块就是那条缺失的发送端。
//!
//! # 「Hello 之前不许写」做成不可表示（对称于 daemon 的 `wire::HelloFlushed`）
//!
//! ```text
//! connect_and_exec → ChannelStream
//!         └── split_and_park(stream) ─→ (ReadHalf, ParkedWriter<WriteHalf>)
//!                     │                            ▲ 身上没有任何写方法
//!                     │                            └── .into_client(hello: DaemonHello)
//!                     └── ReadHalf → 既有 reader task      ▲ 只能由 InboundFrame::Hello 换来
//! ```
//!
//! 收到 Hello 之前，`stream_loop` 手里只有一个 [`ParkedWriter`]，**它没有可调的写**。
//! 这比「注释 + 扫文本的机检」强一档：U6b-3 的 D 审计用一次普通的函数抽取就绕过了那种机检。
//!
//! **诚实边界（据 D 审计订正，别再吹成「唯一」）**：
//! - 切分与停放是**同一个函数**（[`split_and_park`]）⇒ 生产代码里**不存在**可写的裸
//!   `WriteHalf` 窗口。第一版是 `split()` 之后再 `park()`，审计实测在那个窗口里
//!   插一句 `w.write(b"early\n")` ⇒ 两条护栏全绿，而那就是一次 Hello 之前的写。
//! - 剩下的绕过方式：自己造一个假的 `InboundFrame::Hello` 去换见证（调用点显眼的胡来），
//!   或者绕开本模块直接拿 `tokio::io::split` —— 后者由 `ssh_source` 的
//!   `write_half_guard::ssh_source_never_splits_a_stream_itself` 拦（**零命中型**判据，
//!   尾随注释绕不动）。
//!
//! # 超时归客户端
//!
//! 主计划已定「超时一律推给客户端」（daemon 的零定时器铁律不改，登记表仍 1 条）。
//! 所以 [`InboundClient::call`] 自带超时，且超时后**补发一条 `cancel`**，让 daemon 别白跑 ——
//! 这顺带让 U6b-1 写好的 `cancel` 命令第一次有真调用方。

use crate::ssh_source::InboundFrame;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};

/// 同一条连接上**同时在等应答**的命令数上限。
///
/// 超时**不摘登记**（见 [`InboundClient::call`]），所以一个死掉但没断连的 daemon
/// 会让登记表只涨不落。这条上限把它变成「新命令快速失败」而不是「内存无界增长」。
/// 取值与 daemon 侧应答通道容量同量级（`remote-daemon-proto/src/inbound.rs` 的
/// `REPLY_CHANNEL_CAPACITY = 256`）—— 那头一次也只缓 256 条应答。
pub const MAX_PENDING: usize = 256;

/// 待写队列容量。满了 [`InboundClient::call`] 会**等**（背压），不丢命令 ——
/// 丢一条命令的后果是调用方永远等不到应答，比慢一点糟得多。
pub const WRITE_QUEUE_CAPACITY: usize = 64;

/// 一次调用失败的原因。**每一档都要能让调用方分辨「该重试」还是「别重试」。**
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallError {
    /// daemon 在 `hello.commands` 里没声明这条命令（含旧 daemon：无该字段 ⇒ 空集）。
    /// 客户端侧直接拒，省一次往返 + 一次超时。**别重试**。
    Unsupported { cmd: String, offered: Vec<String> },
    /// 同时在等的命令已达 [`MAX_PENDING`]。**可稍后重试**。
    TooManyPending,
    /// 连接（或写任务）已经没了。**重连后重试**。
    Disconnected,
    /// daemon 回了 `{"kind":"cancelled"}`。
    Cancelled,
    /// 本地超时。已补发 `cancel`（best-effort）。
    Timeout { after: Duration },
    /// daemon 回了 `ok:false`。`code`/`message` 原样透出（形状对齐 `--resolve` 的错误契约）。
    Remote { code: String, message: String },
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallError::Unsupported { cmd, offered } => write!(
                f,
                "远端 daemon 不支持入方向命令 `{cmd}`（它声明的是 {offered:?}）—— 多半是旧版本，请重装该机器的 daemon"
            ),
            CallError::TooManyPending => write!(
                f,
                "同时在等的入方向命令已达上限 {MAX_PENDING} —— 远端可能没在回应答"
            ),
            CallError::Disconnected => write!(f, "入方向通道已断开"),
            CallError::Cancelled => write!(f, "命令已被取消"),
            CallError::Timeout { after } => {
                write!(f, "等应答超时（{}ms）", after.as_millis())
            }
            CallError::Remote { code, message } => write!(f, "远端拒绝（{code}）：{message}"),
        }
    }
}

/// 一条命令的结局（由收帧侧路由过来）。
#[derive(Debug, Clone, PartialEq)]
enum Outcome {
    Reply {
        ok: bool,
        code: Option<String>,
        message: Option<String>,
        data: Option<Value>,
    },
    Cancelled,
}

/// 交给 writer task 的活。
///
/// 之所以不是纯 `String`：**关写半边**必须是一条显式指令，不能是「写任务结束时顺手做的事」。
/// 关掉写半边 = 让 daemon 的入方向 reader 见 EOF 寿终；一次性探测想要这个收尾，
/// 长连接不想要（那头之后还要能发命令）。两种语义必须分开表达。
///
/// ⚠ **不要以为关写半边就能让 daemon 退出。** e2e 实测（`e2e/inbound-daemon-frames.sh` 第 9 条）：
/// stdin EOF 只结束 daemon 的入方向 reader **task**，进程照活。daemon 只在
/// ① `writer_task` 结束（stdout 关了）或 ② 收到停机信号 时退出（见其 `main.rs` 的 select）。
/// `ssh_source::probe_daemon` 里那句「daemon 看到 EOF 自行退出」的老注释是错的 ——
/// 它真正的收尾靠的是整条 SSH channel 被 drop。
enum WriteJob {
    Line(String),
    CloseWrite,
}

/// **Hello 见证**：拿到它 = 已经真的收到过 daemon 的 `hello` 帧。
///
/// 唯一构造入口是 [`DaemonHello::from_hello_frame`]，它只对 `InboundFrame::Hello` 返回 `Some`。
/// 字段私有 ⇒ 外部造不出来。
#[derive(Debug, Clone)]
pub struct DaemonHello {
    commands: Vec<String>,
}

impl DaemonHello {
    /// 从一个真的 Hello 帧换见证。非 Hello 帧 → `None`。
    pub fn from_hello_frame(frame: &InboundFrame) -> Option<Self> {
        match frame {
            InboundFrame::Hello { commands, .. } => Some(Self {
                commands: commands.clone(),
            }),
            _ => None,
        }
    }
}

/// **停在手里的写半边** —— 身上没有任何写方法。
///
/// 唯一出口是 [`ParkedWriter::into_client`]，而它要一个 [`DaemonHello`]。
/// 这就是「Hello 之前不许写」在 monitor 侧的落点。
pub struct ParkedWriter<W> {
    inner: W,
}

/// 把流切成两半，并且**在同一个表达式里**把写半边停住。
///
/// # ★ 为什么必须是这一个函数，而不是「`split` 之后记得 `park`」
///
/// D 审计对上一版做了两次变异，两次都全绿：
/// - 在 `split()` 那行加一句**尾随注释**提到 `park`，写半边交给别的函数 —— 机检看窗口里
///   有 `park` 就放行（`production_code` 只剥**行首**注释，行尾的原样留在扫描面里）；
/// - `split()` 之后先 `w.write(b"early\n").await` 再 `park(w)` —— 那就是一次
///   **Hello 之前的写**，而两条护栏都没话说。
///
/// 也就是说：`split()` 与 `park()` 之间存在一个**可写的裸 `WriteHalf` 窗口**，
/// 类型系统在那里保护不了任何东西，兜底的只是一条能被普通写法绕过的文本机检。
///
/// 处置按本区的第 6 条纪律 ——「判据被绕过时，先问能不能让它不可表示」：
/// 让那个窗口**根本不存在**。调用方拿不到裸 `WriteHalf`，只能拿到 [`ParkedWriter`]。
/// 于是护栏也从「每处 split 后面要有 park」变成「生产段里**不许出现** `tokio::io::split(`」——
/// 零命中型判据，尾随注释绕不动。
pub fn split_and_park<S>(
    stream: S,
) -> (
    tokio::io::ReadHalf<S>,
    ParkedWriter<tokio::io::WriteHalf<S>>,
)
where
    S: tokio::io::AsyncRead + AsyncWrite + Send + 'static,
{
    let (r, w) = tokio::io::split(stream);
    (r, ParkedWriter { inner: w })
}

/// 把一个已经拿在手里的写半边停下。
///
/// **只在测试期存在**（`#[cfg(test)]`）—— 生产代码一律走 [`split_and_park`]。
/// 留它是因为 `tokio::io::duplex` 造的内存管道两端本来就是分开的，没有可切的双工流；
/// 把它 gate 在 test 下，等于让「生产代码里有可写的裸 WriteHalf」这件事**编译期不可表示**，
/// 不用再靠 `write_half_guard` 去扫（那条护栏留着挡 `tokio::io::split` 直接调用）。
#[cfg(test)]
pub fn park<W>(w: W) -> ParkedWriter<W>
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    ParkedWriter { inner: w }
}

impl<W> ParkedWriter<W>
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    /// 收到 Hello 之后，把停着的写半边换成一个能发命令的客户端。
    ///
    /// 写半边被移进一个独立的 writer task —— 之后没有任何人还能直接碰它。
    pub fn into_client(self, hello: DaemonHello) -> Arc<InboundClient> {
        let (tx, mut rx) = mpsc::channel::<WriteJob>(WRITE_QUEUE_CAPACITY);
        let mut w = self.inner;
        tauri::async_runtime::spawn(async move {
            while let Some(job) = rx.recv().await {
                let line = match job {
                    WriteJob::Line(l) => l,
                    // 只有明确要求时才关写半边 —— 见 `InboundClient::close_write`。
                    WriteJob::CloseWrite => {
                        let _ = w.shutdown().await;
                        break;
                    }
                };
                if let Err(e) = w.write_all(line.as_bytes()).await {
                    tracing::warn!("inbound_client 写失败（{e}）；停写");
                    break;
                }
                // 逐行 flush：命令是交互式的，攒批只会让应答莫名其妙地晚到。
                if let Err(e) = w.flush().await {
                    tracing::warn!("inbound_client flush 失败（{e}）；停写");
                    break;
                }
            }
            // 走到这里 = 通道关了或写崩了。**不隐式 shutdown**：关掉写半边 =
            // daemon 的入方向 reader 寿终 ⇒ 这条连接**再也发不出命令**。
            // 长连接上那是不可接受的，所以关不关由调用方用 `CloseWrite` 明说。
        });
        Arc::new(InboundClient {
            commands: hello.commands,
            nonce: connection_nonce(),
            seq: AtomicU64::new(0),
            writes: tx,
            pending: Mutex::new(HashMap::new()),
        })
    }
}

/// 一条连接上的入方向客户端。
pub struct InboundClient {
    commands: Vec<String>,
    /// 本连接的号段前缀。**每连接一套** —— 重连后的 `id` 与上一条连接不撞。
    nonce: String,
    seq: AtomicU64,
    writes: mpsc::Sender<WriteJob>,
    pending: Mutex<HashMap<String, oneshot::Sender<Outcome>>>,
}

impl InboundClient {
    /// daemon 声明接受这条命令吗。
    pub fn accepts(&self, cmd: &str) -> bool {
        self.commands.iter().any(|c| c == cmd)
    }

    /// 本连接内唯一的请求 `id`。
    ///
    /// 形状 `<连接 nonce>-<单调序号>`：nonce 让重连后的号段不撞，序号在连接内唯一
    /// ⇒ daemon 的 `duplicate_id` 拒绝路径在正常情况下打不到。
    fn next_id(&self) -> String {
        let n = self.seq.fetch_add(1, Ordering::Relaxed);
        format!("{}-{n}", self.nonce)
    }

    /// 发一条命令并等应答。
    ///
    /// # ★ 超时覆盖**写入 + 等应答**两段，不只是后者
    ///
    /// 第一版把 `writes.send(..).await` 放在 `timeout` 外面。D 审计给出了完整的死锁链，
    /// 而且每一环都是本仓自己写下来的事实：
    ///
    /// ```text
    /// monitor 读侧一停（stream_loop 卡在 flush_lines）
    ///   → daemon stdout 反压
    ///   → daemon 应答通道满（IPC-PROTOCOL 第 4 条：「满时阻塞入方向正是想要的」）
    ///   → daemon 停读 stdin
    ///   → monitor 的 write_all 永久 pending（MASTERPLAN 逐字记着这条）
    ///   → 写队列（64）填满
    ///   → call() **无视自己的 timeout 永久挂起**
    /// ```
    ///
    /// 而 `doc/IPC-PROTOCOL.md` 把「超时归客户端」写成了契约。所以两段共用**一个 deadline**。
    ///
    /// # 超时为什么**不摘登记**（只对「等应答」那一段成立）
    ///
    /// 摘掉的话，晚到的 `reply`/`cancelled` 会落进「未登记的 id」，每次超时刷一条 warn ——
    /// 而那恰恰是**预期内**的事。所以登记条目留着，由路由侧摘：路由发现
    /// `oneshot::send` 失败即知调用方已走，记 debug 而不是 warn。
    ///
    /// **写入那一段超时则相反**：那条命令根本没入队（`send` 只在有空位时才完成），
    /// 不会有任何应答回来 ⇒ 必须当场摘掉，否则就是纯泄漏。
    pub async fn call(
        &self,
        cmd: &str,
        args: Value,
        timeout: Duration,
    ) -> Result<Option<Value>, CallError> {
        if !self.accepts(cmd) {
            return Err(CallError::Unsupported {
                cmd: cmd.to_string(),
                offered: self.commands.clone(),
            });
        }
        let id = self.next_id();
        let rx = self.register(&id).ok_or(CallError::TooManyPending)?;
        let deadline = tokio::time::Instant::now() + timeout;
        let line = WriteJob::Line(encode_request(&id, cmd, &args));
        match tokio::time::timeout_at(deadline, self.writes.send(line)).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => {
                self.take_pending(&id);
                return Err(CallError::Disconnected);
            }
            Err(_elapsed) => {
                // 没入队 ⇒ 不会有应答 ⇒ 摘掉，也不用补 cancel（daemon 没见过这条命令）。
                self.take_pending(&id);
                return Err(CallError::Timeout { after: timeout });
            }
        }
        match tokio::time::timeout_at(deadline, rx).await {
            Ok(Ok(Outcome::Reply { ok: true, data, .. })) => Ok(data),
            Ok(Ok(Outcome::Reply { code, message, .. })) => Err(CallError::Remote {
                code: code.unwrap_or_else(|| "unspecified".to_string()),
                message: message.unwrap_or_default(),
            }),
            Ok(Ok(Outcome::Cancelled)) => Err(CallError::Cancelled),
            // 登记条目被摘掉/连接没了 ⇒ 发送端 drop。
            Ok(Err(_)) => Err(CallError::Disconnected),
            Err(_elapsed) => {
                self.fire_and_forget_cancel(&id);
                Err(CallError::Timeout { after: timeout })
            }
        }
    }

    /// 收到 `{"kind":"reply",…}` 时调。返回是否找到了对应的等待者。
    pub fn route_reply(
        &self,
        id: &str,
        ok: bool,
        code: Option<String>,
        message: Option<String>,
        data: Option<Value>,
    ) -> bool {
        self.deliver(
            id,
            Outcome::Reply {
                ok,
                code,
                message,
                data,
            },
        )
    }

    /// 收到 `{"kind":"cancelled",…}` 时调。返回是否找到了对应的等待者。
    pub fn route_cancelled(&self, id: &str) -> bool {
        self.deliver(id, Outcome::Cancelled)
    }

    /// **显式关掉写半边** —— daemon 的入方向 reader 见 EOF 后寿终。
    ///
    /// 只有一次性探测该调（`ssh_source::probe_daemon`：探完就不再发命令了）。
    /// 长连接上调它 = 之后**再也发不出任何命令**，而连接看起来一切正常。
    /// 之所以做成一条要主动发的指令而不是「writer task 结束时顺手做」，就是为了让这个
    /// 区别在调用点显形。
    ///
    /// **它不会让 daemon 退出**（e2e 第 9 条实测钉住）—— 见 [`WriteJob`] 的说明。
    pub fn close_write(&self) {
        if self.writes.try_send(WriteJob::CloseWrite).is_err() {
            // 队列满 / 写任务已退。**说出来** —— 调用方会以为已经关了。
            tracing::debug!("inbound_client close_write 未能入队（写队列满或写任务已退）");
        }
    }

    /// 断连：叫醒所有还在等的调用方（它们会拿到 [`CallError::Disconnected`]）。
    pub fn shutdown(&self) {
        let n = {
            let mut p = lock(&self.pending);
            let n = p.len();
            p.clear();
            n
        };
        if n > 0 {
            tracing::debug!("inbound_client 断连，叫醒 {n} 个等待中的命令");
        }
    }

    fn deliver(&self, id: &str, outcome: Outcome) -> bool {
        let Some(tx) = self.take_pending(id) else {
            // ★ 归不到任何命令头上的应答。**最要紧的是别把 code/message 丢了** ——
            // 它们往往是唯一能说清「为什么那条命令没反应」的东西。
            //
            // daemon 对**协议级**错误（坏 JSON / 超长单行）回的 `id` 是**空串**
            // （它那时还不知道 id 是什么），所以空 id 不是「回显错了」，是这一类。
            let detail = match &outcome {
                Outcome::Reply {
                    ok: false,
                    code,
                    message,
                    ..
                } => format!(
                    "（错误应答 code={} message={}）",
                    code.as_deref().unwrap_or("-"),
                    message.as_deref().unwrap_or("-")
                ),
                _ => String::new(),
            };
            if id.is_empty() {
                tracing::warn!(
                    "远端 daemon 回了一条**协议级**错误应答（id 为空，归不到具体命令）{detail} —— \
                     多半是上一条命令的 JSON 坏了或超过单行上限；那条命令会走本地超时"
                );
            } else {
                tracing::warn!(
                    "入方向应答的 id `{id}` 没有登记{detail} —— 要么 daemon 回显错了 id，\
                     要么登记表满时补发的 cancel 回来了"
                );
            }
            return false;
        };
        if tx.send(outcome).is_err() {
            // 正常：调用方已超时走人（或那是一条 fire-and-forget 的 cancel）。
            tracing::debug!("入方向应答 `{id}` 晚到，调用方已走");
        }
        true
    }

    /// 登记一个等待者。表满 → `None`。
    ///
    /// # ★ 满之前先回收「调用方已走」的登记
    ///
    /// 「超时不摘登记」那条设计有个前提：晚到的应答终会把登记摘掉。
    /// D 审计指出这个前提在**背压路径上不成立** —— daemon 侧 cancel 的两条应答都是
    /// `try_send`（`inbound.rs`），应答通道满时**静默丢弃**，被 abort 的命令也不补应答。
    /// 那条 id 就永远等不到任何帧，是真泄漏；每次超时消耗 2 格，128 次封死 256 格，
    /// **而且 daemon 恢复之后也不会自愈**。
    ///
    /// 回收判据用 `oneshot::Sender::is_closed()`：接收端已 drop = 调用方早走了，
    /// 这条登记留着只是为了「让晚到的应答别刷 warn」，满的时候它显然不值那个价。
    /// 不需要定时器，只在真要满的那一刻扫一次。
    fn register(&self, id: &str) -> Option<oneshot::Receiver<Outcome>> {
        let (tx, rx) = oneshot::channel();
        let mut p = lock(&self.pending);
        if p.len() >= MAX_PENDING {
            let before = p.len();
            p.retain(|_, waiter| !waiter.is_closed());
            let reclaimed = before - p.len();
            if reclaimed > 0 {
                tracing::debug!("入方向登记表满，回收了 {reclaimed} 条调用方已走的登记");
            }
            if p.len() >= MAX_PENDING {
                return None;
            }
        }
        p.insert(id.to_string(), tx);
        Some(rx)
    }

    /// 当前登记数（测试用；生产没有读者，别拿它做判断）。
    #[cfg(test)]
    fn pending_len(&self) -> usize {
        lock(&self.pending).len()
    }

    fn take_pending(&self, id: &str) -> Option<oneshot::Sender<Outcome>> {
        lock(&self.pending).remove(id)
    }

    /// 超时后补一条 `cancel`，让 daemon 别白跑。**不等它的应答。**
    ///
    /// 它自己那条应答也登记（接收端立刻丢掉）——这样路由到它时走的是
    /// 「调用方已走」的 debug 路径，而不是「未登记的 id」的 warn。
    ///
    /// **登记不上就不发**：登记表满时硬发出去，那条应答回来一定落进 unknown-id 的 warn，
    /// 而那正是登记它想避免的噪声 —— 在表已经满、日志最该干净的时候刷。
    fn fire_and_forget_cancel(&self, target: &str) {
        if !self.accepts("cancel") {
            return;
        }
        let id = self.next_id();
        let Some(_keep_quiet) = self.register(&id) else {
            tracing::debug!("登记表满，跳过超时补发的 cancel：target={target}");
            return;
        };
        let line = encode_request(&id, "cancel", &serde_json::json!({ "target": target }));
        // `try_send`：这是 best-effort 的收尾，绝不为它阻塞调用方。
        if self.writes.try_send(WriteJob::Line(line)).is_err() {
            self.take_pending(&id);
            tracing::debug!("超时补发 cancel 未能入队（队列满或已断连）：target={target}");
        }
    }
}

/// 请求信封的线上形状。**用结构体而不是 `json!` map** —— 结构体字段顺序由 serde 保证，
/// 与 `serde_json` 有没有开 `preserve_order` 无关。跨轨对拍要的是**逐字节确定**。
#[derive(Serialize)]
struct RequestLine<'a> {
    id: &'a str,
    cmd: &'a str,
    args: &'a Value,
}

/// 把一条命令编成线上的一行（含行尾 `\n`）。**纯函数。**
///
/// 对侧是 `remote-daemon-proto/src/wire.rs::Request`（`{id, cmd, args}`，`args` 可缺省）。
///
/// # 为什么可以 `expect`
///
/// `serde_json::Value` 在类型上就装不下会让序列化失败的东西：`Number` 不可能是 NaN/Inf，
/// `Map` 的键恒为 `String`。三个字段全是 `&str`/`&Value` ⇒ 这个 `to_string` 不可失败。
pub fn encode_request(id: &str, cmd: &str, args: &Value) -> String {
    let mut s = serde_json::to_string(&RequestLine { id, cmd, args })
        .expect("RequestLine 只含 &str/&Value，序列化不可失败");
    s.push('\n');
    s
}

/// U8a-2b：`launch` 命令的**参数构造器**（monitor 这一侧的契约面）。
///
/// # 它今天有没有生产调用方 —— 没有，如实说
///
/// 生产路径还没切过来：tauri 命令 `launch_remote_terminal(origin, remote_cmd)` 收到的
/// 已经是一条**渲染好的 shell 串**，拆不回结构化计划。切换要等前端改成发结构化请求
/// （U8c 的两个 TS 渲染器 + IR 退役），登记为 **U8a-2c**。
///
/// 那为什么现在就写：**它是契约**。字段名一旦与 daemon 的解析器漂开，症状是
/// 「命令发出去了、daemon 回 `bad_request` 说缺字段」，而两边各自看都「对」。
/// `launch_args_field_names_match_the_daemon_parser` 把这件事变成编译期就会红的对拍。
///
/// `mode` 只有两种取值 —— **没有 `attach-only`**：attach 是平面 ③，daemon 在远端，
/// 开不了你面前的窗（见 daemon `control/launch.rs` 头注）。
// U8a-2c-1：**它有生产调用方了** —— `backend::control::daemon_launch::daemon_send_into`。
// 在那之前这里挂着 `#[allow(dead_code)]`（编码器早写好、零调用方，正是复盘点名的「方向偏移」形状）。
pub fn launch_args(
    mode: &str,
    name: &str,
    payload: &str,
    cwd: Option<&str>,
    ccm_sid: Option<&str>,
) -> Value {
    let mut m = serde_json::Map::new();
    m.insert("mode".into(), Value::String(mode.to_string()));
    m.insert("name".into(), Value::String(name.to_string()));
    m.insert("payload".into(), Value::String(payload.to_string()));
    if let Some(c) = cwd {
        m.insert("cwd".into(), Value::String(c.to_string()));
    }
    if let Some(s) = ccm_sid {
        m.insert("ccm_sid".into(), Value::String(s.to_string()));
    }
    Value::Object(m)
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// 每条连接一个前缀。用进程内单调计数 + 起始时间戳：**不需要密码学随机**，
/// 只需要「同一个 daemon 进程看到的两条连接不会用同一个号段」。
fn connection_nonce() -> String {
    static CONN: AtomicU64 = AtomicU64::new(0);
    let n = CONN.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("m{t:x}.{n}")
}

// ============================================================================
// 每台远端主机一个客户端：origin → 当前连接的客户端
// ============================================================================

/// 形状同 `ssh_source::announced_registry`：origin 是那台机器的稳定身份。
/// 写者 = 各主机的 `stream_loop`（收到 hello 时登记、连接退出时摘除）。
fn registry() -> &'static Mutex<HashMap<String, Arc<InboundClient>>> {
    static R: std::sync::OnceLock<Mutex<HashMap<String, Arc<InboundClient>>>> =
        std::sync::OnceLock::new();
    R.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 登记本连接的客户端。同 origin 的旧条目会被替换（重连）。
pub fn register(origin: &str, client: Arc<InboundClient>) {
    // **先放锁再 shutdown** —— 与 `unregister` 对称。今天两者都不会死锁（没有任何路径是
    // pending → registry），但两处写法不一致就是给后来人埋的坑。
    let old = lock(registry()).insert(origin.to_string(), client);
    if let Some(old) = old {
        old.shutdown();
    }
}

/// 摘除 —— **只摘自己那条**。重连时新连接可能已经登记上来了，
/// 拿旧的 `Arc` 比一下指针，不是自己的就别动（否则会把新连接摘掉）。
pub fn unregister(origin: &str, mine: &Arc<InboundClient>) {
    let mut r = lock(registry());
    let is_mine = r.get(origin).is_some_and(|cur| Arc::ptr_eq(cur, mine));
    if is_mine {
        r.remove(origin);
    }
    drop(r);
    mine.shutdown();
}

/// 取某台远端主机当前的入方向客户端。没连上/还没收到 hello → `None`。
///
/// **今天只有测试在读**：注册表由 `stream_loop` 填，第一个生产读者是 U8a-2b 的 `launch`
/// （起会话要在长连接上发命令）。这一条如实登记，不假装它已经在线上被用。
#[allow(dead_code)]
pub fn client_for(origin: &str) -> Option<Arc<InboundClient>> {
    lock(registry()).get(origin).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncBufReadExt;

    fn hello_frame(commands: &[&str]) -> InboundFrame {
        InboundFrame::Hello {
            v: 1,
            build_id: "test".into(),
            host_arch: "x86_64".into(),
            claude_dir: "/tmp".into(),
            capabilities: vec![],
            commands: commands.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// ★ **一律带超时地读**。D 审计变异 MU16（`encode_request` 不 push `\n`）时，
    /// 两条断言确实 FAILED，**但整个 `cargo test --lib` 900s 没返回** —— 5 处裸
    /// `read_line().await` 收不到换行就永远悬着。CI 上那表现为 job 超时，不是失败列表，
    /// 排查成本天差地别。
    async fn next_line(peer: &mut tokio::io::BufReader<tokio::io::DuplexStream>) -> String {
        let mut line = String::new();
        tokio::time::timeout(Duration::from_secs(5), peer.read_line(&mut line))
            .await
            .expect("等对端读一行超时（5s）—— 让它干净变红，别让 cargo test 挂死")
            .expect("读一行");
        line
    }

    fn client_on_duplex(
        commands: &[&str],
    ) -> (
        Arc<InboundClient>,
        tokio::io::BufReader<tokio::io::DuplexStream>,
    ) {
        let (mine, theirs) = tokio::io::duplex(64 * 1024);
        let hello = DaemonHello::from_hello_frame(&hello_frame(commands)).expect("是 Hello 帧");
        (
            park(mine).into_client(hello),
            tokio::io::BufReader::new(theirs),
        )
    }

    #[test]
    fn the_hello_witness_can_only_come_from_a_hello_frame() {
        assert!(DaemonHello::from_hello_frame(&hello_frame(&["ping"])).is_some());
        assert!(
            DaemonHello::from_hello_frame(&InboundFrame::Overflow { dropped: 1 }).is_none(),
            "非 Hello 帧换出了见证 —— 「Hello 之前不许写」就破了"
        );
    }

    /// ★ **跨轨对拍**：`e2e/inbound-daemon-frames.sh` 喂给真 daemon 的那条 ping 行，
    /// 必须**逐字节**等于本模块编码器的产物。
    ///
    /// 没有这条，那套 e2e 只证明了「daemon 认得我手写的那串 JSON」，
    /// 证明不了「monitor 真发出去的那串 JSON」—— 两者一旦漂开，e2e 会**继续全绿**
    /// 而生产里一条命令都发不出去。同 `removal_cause_wire_literal_stays_in_sync` 的思路。
    #[test]
    fn the_e2e_ping_line_is_exactly_what_the_encoder_produces() {
        const SUITE: &str = include_str!("../../e2e/inbound-daemon-frames.sh");
        let key = "INBOUND_PING_LINE='";
        let at = SUITE
            .find(key)
            .expect("e2e 脚本里找不到 INBOUND_PING_LINE —— 抽取坏了，本断言在空转");
        let rest = &SUITE[at + key.len()..];
        let literal = &rest[..rest.find('\'').expect("赋值没有收尾单引号")];
        assert!(
            literal.len() > 20,
            "抽到的字面量太短（{literal:?}）—— 抽取坏了"
        );
        assert_eq!(
            format!("{literal}\n"),
            encode_request("e2e-ping-1", "ping", &Value::Null),
            "\ne2e 脚本喂给真 daemon 的行与 monitor 编码器的产物不一致。\n\
             改了编码器就把脚本里那条 `INBOUND_PING_LINE` 一起改（反之亦然）——\n\
             它们必须是同一份事实，否则 e2e 是在验证一个 monitor 永远不会发的形状。"
        );

        // ★ 光钉变量不够 —— D 审计变异 EMU2：变量一字不动，只把 `send "$INBOUND_PING_LINE"`
        //   换成一串手抄字面量 ⇒ **两轨全绿**，而 DoD 那句「喂给 daemon 的就是编码器的字节」
        //   已经不成立了（shellcheck 也拦不住：未用变量是 SC2034 warning，CI 只看 error）。
        //   所以再钉一条：那个变量必须真的被送出去，且**只此一处**发 ping。
        let sends: Vec<&str> = SUITE
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with("send ") && !l.starts_with("send() "))
            .collect();
        assert!(
            sends.len() >= 6,
            "只抽到 {} 条 send —— 抽取坏了，本断言在空转：{sends:?}",
            sends.len()
        );
        let via_var = sends
            .iter()
            .filter(|l| l.contains("$INBOUND_PING_LINE"))
            .count();
        assert_eq!(
            via_var, 1,
            "\n脚本里经 `$INBOUND_PING_LINE` 发出去的行应恰好一条（实得 {via_var}）——\n\
             那个变量是与 monitor 编码器逐字节对拍的**唯一**载体，绕过它 e2e 就变成\n\
             「daemon 认得我手抄的 JSON」，证明不了「monitor 发的那种 JSON」。"
        );
        // 反面：别的 send 里不许再出现 `"cmd":"ping"` 的手抄 ping 请求（cancel/unknown 等无妨）。
        let hand_written_ping: Vec<&&str> = sends
            .iter()
            .filter(|l| !l.contains("$INBOUND_PING_LINE") && l.contains(r#""cmd":"ping""#))
            .collect();
        // 例外：并发/坏输入之后那几条**刻意**用别的 id 手写（它们验的是路由与存活，不是编码器）。
        // 只要它们不是「第一条 ping 往返」那条即可 —— 用 id 前缀区分。
        for l in &hand_written_ping {
            assert!(
                l.contains("e2e-multi-") || l.contains("e2e-after-garbage"),
                "\n发现一条手抄的 ping 请求，它绕开了跨轨对拍：{l}\n\
                 核心那条 ping 必须走 `$INBOUND_PING_LINE`。"
            );
        }
    }

    /// ★ U8a-2c-1：同上，但钉的是**业务命令**那一行。
    ///
    /// ping 那条证明「daemon 认得 monitor 编的信封」；这条证明的是
    /// **monitor 真正会发的那条 `launch`**（`daemon_send_into` 唯一会说的 `send-into`）。
    /// 少了它，那套 e2e 只验证了「daemon 认得我手写的 launch 形状」——
    /// 而 `launch_args` 的键名/键序一改，e2e 会继续全绿而生产里一条命令都发不出去。
    #[test]
    fn the_e2e_send_into_line_is_exactly_what_the_encoder_produces() {
        const SUITE: &str = include_str!("../../e2e/inbound-daemon-frames.sh");
        let key = "INBOUND_SEND_INTO_LINE='";
        let at = SUITE
            .find(key)
            .expect("e2e 脚本里找不到 INBOUND_SEND_INTO_LINE —— 抽取坏了，本断言在空转");
        let rest = &SUITE[at + key.len()..];
        let literal = &rest[..rest.find('\'').expect("赋值没有收尾单引号")];
        assert!(
            literal.len() > 40,
            "抽到的字面量太短（{literal:?}）—— 抽取坏了"
        );
        assert_eq!(
            format!("{literal}\n"),
            encode_request(
                "e2e-si-1",
                "launch",
                &launch_args("send-into", "e2e-si-fixed", "true", None, None)
            ),
            "\ne2e 脚本喂给真 daemon 的 send-into 行与 monitor 编码器的产物不一致。\n\
             `launch_args` 的键名/键序改了就把脚本里那条 `INBOUND_SEND_INTO_LINE` 一起改 ——\n\
             它们必须是同一份事实，否则 e2e 在验证一个 monitor 永远不会发的形状。"
        );
    }

    /// ★ e2e 脚本里硬编码的那三个命令名，必须等于 daemon 的 `inbound::COMMANDS`。
    ///
    /// 那是命令面的**第五处**副本（前四处已由 daemon 侧两条护栏钉住）。没有这条的话，
    /// 加一条新命令时 e2e 不会红 —— 只是**悄悄漏测**，而 e2e 恰恰是唯一跑真进程的那一层。
    #[test]
    fn the_e2e_command_list_matches_the_daemon_command_table() {
        const SUITE: &str = include_str!("../../e2e/inbound-daemon-frames.sh");
        const DAEMON_INBOUND: &str = include_str!("../../remote-daemon-proto/src/inbound.rs");

        // daemon 侧：`pub const COMMANDS: &[&str] = &["cancel", "ping", "resolve"];`
        let i = DAEMON_INBOUND
            .find("const COMMANDS")
            .expect("daemon inbound.rs 里找不到 COMMANDS —— 抽取坏了");
        let j = DAEMON_INBOUND[i..]
            .find("];")
            .map(|k| i + k)
            .expect("COMMANDS 没有收尾");
        let mut daemon: Vec<String> = DAEMON_INBOUND[i..j]
            .split('"')
            .skip(1)
            .step_by(2)
            .filter(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
            .map(str::to_string)
            .collect();
        daemon.sort();
        daemon.dedup();
        assert!(
            daemon.len() >= 3,
            "只抽到 {} 条 daemon 命令 —— 抽取坏了，本断言在空转：{daemon:?}",
            daemon.len()
        );

        // e2e 侧：`for c in ping cancel resolve; do`
        let key = "for c in ";
        let at = SUITE
            .find(key)
            .expect("e2e 脚本里找不到命令名清单 —— 抽取坏了");
        let rest = &SUITE[at + key.len()..];
        let line = &rest[..rest.find('\n').unwrap_or(rest.len())];
        let mut suite: Vec<String> = line
            .split_whitespace()
            // 行尾是 `resolve; do` —— 剥掉分号，遇到 `do` 停。
            .map(|t| t.trim_end_matches(';'))
            .take_while(|t| {
                !t.is_empty() && *t != "do" && t.chars().all(|c| c.is_ascii_lowercase() || c == '_')
            })
            .map(str::to_string)
            .collect();
        assert!(
            suite.len() >= 3,
            "只从 e2e 脚本抽到 {} 条命令 —— 抽取坏了，本断言在空转：{suite:?}",
            suite.len()
        );
        suite.sort();
        suite.dedup();

        assert_eq!(
            suite, daemon,
            "\ne2e 脚本断言的命令集与 daemon 的 `inbound::COMMANDS` 对不上。\n\
             加/删入方向命令时这两处要一起动 —— 否则新命令在**唯一跑真进程的那一层**漏测。"
        );
    }

    /// ★ 跨轨对拍：`launch_args` 吐的键名必须**恰好**是 daemon 解析器认的那几个。
    ///
    /// 漂开的症状是「命令发出去了、daemon 回 `bad_request` 说缺字段」，而两边各自看都对。
    #[test]
    fn launch_args_field_names_match_the_daemon_parser() {
        const DAEMON_LAUNCH: &str = include_str!("../../remote-daemon-proto/src/control/launch.rs");
        let prod = guard_core::production_code(DAEMON_LAUNCH);
        // daemon 侧逐个 `get_str("<key>")` 抠出来。
        let key = "get_str(\"";
        let mut wanted: Vec<String> = Vec::new();
        let mut from = 0usize;
        while let Some(rel) = prod[from..].find(key) {
            let at = from + rel + key.len();
            let end = prod[at..].find('"').map(|k| at + k).unwrap_or(at);
            wanted.push(prod[at..end].to_string());
            from = end;
        }
        wanted.sort();
        wanted.dedup();
        assert!(
            wanted.len() >= 5,
            "只从 daemon 解析器抠到 {} 个字段 —— 抽取坏了，本断言在空转：{wanted:?}",
            wanted.len()
        );

        let full = launch_args(
            "create-or-attach",
            "cc-x",
            "true",
            Some("/tmp"),
            Some("sid-1"),
        );
        let mut got: Vec<String> = full
            .as_object()
            .expect("对象")
            .keys()
            .map(String::from)
            .collect();
        got.sort();
        assert_eq!(
            got, wanted,
            "\nmonitor 的 `launch_args` 与 daemon 的解析器字段名对不上。\n\
             两边必须同时改 —— 否则症状是「daemon 回 bad_request 说缺字段」，很难归因。"
        );

        // 可选字段真的可选：不传就不出现（daemon 侧 `cwd`/`ccm_sid` 都是 `Option`）。
        let minimal = launch_args("send-into", "cc-x", "true", None, None);
        let keys: Vec<&String> = minimal.as_object().expect("对象").keys().collect();
        assert_eq!(
            keys.len(),
            3,
            "最小形态应当只有 mode/name/payload：{keys:?}"
        );
    }

    #[test]
    fn encode_request_is_byte_stable_and_matches_the_daemon_envelope() {
        let line = encode_request("abc-0", "ping", &serde_json::json!({}));
        assert_eq!(line, "{\"id\":\"abc-0\",\"cmd\":\"ping\",\"args\":{}}\n");
        // 反向：daemon 侧就是拿它当 `Request` 反序列化的，字段名必须对得上。
        let v: Value = serde_json::from_str(line.trim_end()).expect("必须是合法 JSON");
        for k in ["id", "cmd", "args"] {
            assert!(v.get(k).is_some(), "信封缺字段 `{k}`：{line}");
        }
    }

    #[tokio::test]
    async fn a_call_writes_one_line_and_resolves_on_the_matching_reply() {
        let (client, mut peer) = client_on_duplex(&["ping"]);
        let c = client.clone();
        let caller =
            tokio::spawn(async move { c.call("ping", Value::Null, Duration::from_secs(5)).await });

        let line = next_line(&mut peer).await;
        let req: Value = serde_json::from_str(line.trim_end()).expect("请求是合法 JSON");
        let id = req["id"].as_str().expect("有 id").to_string();
        assert_eq!(req["cmd"], "ping");

        assert!(
            client.route_reply(
                &id,
                true,
                None,
                None,
                Some(serde_json::json!({ "pong": 1 }))
            ),
            "路由没找到等待者"
        );
        let got = caller.await.expect("caller task").expect("call 成功");
        assert_eq!(got, Some(serde_json::json!({ "pong": 1 })));
    }

    #[tokio::test]
    async fn an_error_reply_surfaces_code_and_message() {
        let (client, mut peer) = client_on_duplex(&["resolve"]);
        let c = client.clone();
        let caller = tokio::spawn(async move {
            c.call("resolve", serde_json::json!({}), Duration::from_secs(5))
                .await
        });
        let line = next_line(&mut peer).await;
        let id = serde_json::from_str::<Value>(line.trim_end()).expect("JSON")["id"]
            .as_str()
            .expect("id")
            .to_string();
        client.route_reply(
            &id,
            false,
            Some("bad_request".into()),
            Some("缺 sid".into()),
            None,
        );
        assert_eq!(
            caller.await.expect("task").unwrap_err(),
            CallError::Remote {
                code: "bad_request".into(),
                message: "缺 sid".into()
            }
        );
    }

    #[tokio::test]
    async fn a_cancelled_frame_ends_the_call_as_cancelled() {
        let (client, mut peer) = client_on_duplex(&["ping"]);
        let c = client.clone();
        let caller =
            tokio::spawn(async move { c.call("ping", Value::Null, Duration::from_secs(5)).await });
        let line = next_line(&mut peer).await;
        let id = serde_json::from_str::<Value>(line.trim_end()).expect("JSON")["id"]
            .as_str()
            .expect("id")
            .to_string();
        client.route_cancelled(&id);
        assert_eq!(
            caller.await.expect("task").unwrap_err(),
            CallError::Cancelled
        );
    }

    /// 命令不在 `hello.commands` 里 ⇒ 客户端侧直接拒，**一个字节都不发**。
    #[tokio::test]
    async fn an_undeclared_command_is_refused_without_writing_anything() {
        let (client, mut peer) = client_on_duplex(&["ping"]);
        let err = client
            .call("launch", Value::Null, Duration::from_secs(5))
            .await
            .unwrap_err();
        assert!(matches!(err, CallError::Unsupported { .. }), "{err:?}");
        // 对侧不该收到任何东西。
        let mut buf = String::new();
        let read = tokio::time::timeout(Duration::from_millis(80), peer.read_line(&mut buf)).await;
        assert!(read.is_err(), "被拒的命令却发出去了：{buf:?}");
    }

    /// 超时 ⇒ `Timeout`，且**自动补发一条 `cancel`**（daemon 别白跑）。
    #[tokio::test]
    async fn a_timeout_fires_a_cancel_for_the_abandoned_id() {
        let (client, mut peer) = client_on_duplex(&["ping", "cancel"]);
        let c = client.clone();
        let caller =
            tokio::spawn(
                async move { c.call("ping", Value::Null, Duration::from_millis(60)).await },
            );

        let first = next_line(&mut peer).await;
        let ping_id = serde_json::from_str::<Value>(first.trim_end()).expect("JSON")["id"]
            .as_str()
            .expect("id")
            .to_string();

        let err = caller.await.expect("task").unwrap_err();
        assert!(matches!(err, CallError::Timeout { .. }), "{err:?}");

        let mut second = String::new();
        tokio::time::timeout(Duration::from_secs(2), peer.read_line(&mut second))
            .await
            .expect("等 cancel 超时了")
            .expect("读到 cancel");
        let cancel: Value = serde_json::from_str(second.trim_end()).expect("JSON");
        assert_eq!(cancel["cmd"], "cancel");
        assert_eq!(
            cancel["args"]["target"].as_str(),
            Some(ping_id.as_str()),
            "补发的 cancel 没指向被放弃的那条命令"
        );
        assert_ne!(
            cancel["id"].as_str(),
            Some(ping_id.as_str()),
            "cancel 复用了被取消者的 id"
        );
    }

    /// daemon 没声明 `cancel` 时不许补发（否则那是一条注定 `unknown_command` 的噪声）。
    #[tokio::test]
    async fn no_cancel_is_fired_when_the_daemon_does_not_declare_it() {
        let (client, mut peer) = client_on_duplex(&["ping"]);
        let c = client.clone();
        let caller =
            tokio::spawn(
                async move { c.call("ping", Value::Null, Duration::from_millis(60)).await },
            );
        let _first = next_line(&mut peer).await;
        assert!(matches!(
            caller.await.expect("task").unwrap_err(),
            CallError::Timeout { .. }
        ));
        let mut second = String::new();
        let read =
            tokio::time::timeout(Duration::from_millis(200), peer.read_line(&mut second)).await;
        assert!(read.is_err(), "不该补发 cancel，却发了：{second:?}");
    }

    /// 超时之后**晚到的应答**不许触发「未登记的 id」——那是预期内的事，不该刷 warn。
    #[tokio::test]
    async fn a_late_reply_after_timeout_still_finds_its_registration() {
        let (client, mut peer) = client_on_duplex(&["ping"]);
        let c = client.clone();
        let caller =
            tokio::spawn(
                async move { c.call("ping", Value::Null, Duration::from_millis(60)).await },
            );
        let line = next_line(&mut peer).await;
        let id = serde_json::from_str::<Value>(line.trim_end()).expect("JSON")["id"]
            .as_str()
            .expect("id")
            .to_string();
        assert!(matches!(
            caller.await.expect("task").unwrap_err(),
            CallError::Timeout { .. }
        ));
        assert!(
            client.route_reply(&id, true, None, None, None),
            "超时后登记被摘早了 —— 晚到的应答会落进 unknown-id 的 warn"
        );
    }

    /// 登记表有上限：**调用方还在等**的那些占着位，占满就快速失败。
    #[tokio::test]
    async fn the_pending_table_is_capped_by_live_waiters() {
        let (client, _peer) = client_on_duplex(&["ping"]);
        // 把接收端**留着**（= 调用方还在等），否则会被下面那条回收逻辑扫掉。
        let mut held = Vec::new();
        for i in 0..MAX_PENDING {
            held.push(
                client
                    .register(&format!("x{i}"))
                    .unwrap_or_else(|| panic!("第 {i} 条就满了")),
            );
        }
        assert!(
            client.register("overflow").is_none(),
            "登记表没有上限 —— 死 daemon 下会无界增长"
        );
        drop(held);
    }

    /// ★ 满的时候先回收「调用方已走」的登记（D 审计发现的真泄漏路径）。
    ///
    /// 「超时不摘登记」那条设计的前提是「晚到的应答终会把它摘掉」。审计指出这个前提在
    /// **背压路径上不成立**：daemon 侧 cancel 的两条应答都是 `try_send`，应答通道满时
    /// 静默丢弃，被 abort 的命令也不补应答 ⇒ 那条 id 永远等不到任何帧。
    /// 每次超时吃 2 格，128 次封死 256 格，**而且 daemon 恢复之后也不会自愈**。
    #[tokio::test]
    async fn a_full_table_reclaims_registrations_whose_caller_has_left() {
        let (client, _peer) = client_on_duplex(&["ping"]);
        // 全部丢掉接收端 = 全是「调用方已走」的僵尸登记。
        for i in 0..MAX_PENDING {
            assert!(client.register(&format!("zombie{i}")).is_some());
        }
        assert_eq!(client.pending_len(), MAX_PENDING);
        assert!(
            client.register("fresh").is_some(),
            "表被僵尸登记撑满后再也登记不上 —— 那正是审计说的「不自愈」"
        );
        assert_eq!(
            client.pending_len(),
            1,
            "回收之后表里应当只剩刚登记的那一条"
        );
        // 反面：活着的等待者**不许**被当成僵尸扫掉。
        let (client2, _peer2) = client_on_duplex(&["ping"]);
        let mut held = Vec::new();
        for i in 0..MAX_PENDING {
            held.push(client2.register(&format!("live{i}")).expect("登记"));
        }
        assert!(
            client2.register("fresh").is_none(),
            "把还在等的调用方当成僵尸回收了 —— 那会让它们永远收不到应答"
        );
        drop(held);
    }

    #[tokio::test]
    async fn shutdown_wakes_every_waiter_with_disconnected() {
        let (client, mut peer) = client_on_duplex(&["ping"]);
        let c = client.clone();
        let caller =
            tokio::spawn(async move { c.call("ping", Value::Null, Duration::from_secs(5)).await });
        let _line = next_line(&mut peer).await;
        client.shutdown();
        assert_eq!(
            caller.await.expect("task").unwrap_err(),
            CallError::Disconnected
        );
    }

    /// 每条连接一套号段：两个客户端的 `id` 不许撞。
    #[test]
    fn ids_from_two_connections_never_collide() {
        let mk = || {
            let (mine, _theirs) = tokio::io::duplex(1024);
            let hello =
                DaemonHello::from_hello_frame(&hello_frame(&["ping"])).expect("是 Hello 帧");
            park(mine).into_client(hello)
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("rt");
        let (a, b) = rt.block_on(async { (mk(), mk()) });
        let ids_a: Vec<String> = (0..5).map(|_| a.next_id()).collect();
        let ids_b: Vec<String> = (0..5).map(|_| b.next_id()).collect();
        assert_eq!(ids_a.len(), 5);
        for x in &ids_a {
            assert!(!ids_b.contains(x), "两条连接发出了同一个 id `{x}`");
        }
    }

    /// 注册表：摘除只摘自己那条，别把重连上来的新客户端摘掉。
    #[test]
    fn unregister_never_removes_someone_elses_client() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("rt");
        let mk = || {
            let (mine, _theirs) = tokio::io::duplex(1024);
            let hello =
                DaemonHello::from_hello_frame(&hello_frame(&["ping"])).expect("是 Hello 帧");
            park(mine).into_client(hello)
        };
        let (old, new) = rt.block_on(async { (mk(), mk()) });
        let origin = "unregister-test-origin";
        register(origin, old.clone());
        register(origin, new.clone());
        unregister(origin, &old); // 旧连接迟到的收尾
        assert!(
            client_for(origin).is_some_and(|c| Arc::ptr_eq(&c, &new)),
            "旧连接的收尾把新连接摘掉了"
        );
        unregister(origin, &new);
        assert!(client_for(origin).is_none(), "自己的条目没摘掉");
    }
}
