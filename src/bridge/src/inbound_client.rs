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
/// 取值与 daemon 侧应答通道容量同量级（`src/backend/inbound.rs` 的
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
/// ⚠ **不要以为关写半边就能让 daemon 退出。** e2e 实测（`tests/e2e/inbound-daemon-frames.sh` 第 9 条）：
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

/// P2（定框 C1/C4）：**停住一个「本来就独立、没有对应『切』动作」的写端**。
///
/// # 它和 [`park`] 的 `#[cfg(test)]` 不是一回事 —— 别把这里读成「把那道门拆了」
///
/// 上面那道门防的是**从双工流里切出来、却忘了停**的裸 WriteHalf：切与停必须由
/// [`split_and_park`] 一步做完，否则会出现「有人能在 hello 之前写」的窗口。
///
/// 而本函数的入参**不是从任何流切出来的**。本机 daemon 是 `std::process::Child`，
/// 它的 `ChildStdin` / `ChildStdout` 是**两条本来就独立的管道** —— 这条路上
/// **压根没有「切」这个动作**，因此也没有「切了忘了停」这个失效模式可防。
///
/// ⇒ 本函数承认的是**结构差异**，不是给「切了不停」开后门。它仍然产出 [`ParkedWriter`]，
/// 也就是说「hello 之前不许写」那条性质**照旧由类型保证**（要拿到可写的 client，
/// 唯一的路仍是 [`ParkedWriter::into_client`]，而它要一个 [`DaemonHello`] 见证）。
///
/// # 为什么远端那条路用不了
///
/// `attach_inbound_client`（`ssh_source.rs`）本身是泛型、传输无关的，本可直接复用；
/// 卡住的是它要的 [`ParkedWriter`] 只能由 [`split_and_park`] 产出，而那个函数要一个
/// **可切的双工流**（SSH channel 读写同体）。本机没有。
///
/// # 诚实边界
///
/// ⚠ 这是**新开的一个合法口**，它本身没有判据钉「只许本机用」。
/// 有人拿它去停一个真的从双工流切出来的写端，就绕过了上面那道门（登记在 P2 的 10b）。
pub fn park_owned_writer<W>(w: W) -> ParkedWriter<W>
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
    /// 而 `src/doc/IPC-PROTOCOL.md` 把「超时归客户端」写成了契约。所以两段共用**一个 deadline**。
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
/// 对侧是 `src/backend/wire.rs::Request`（`{id, cmd, args}`，`args` 可缺省）。
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
    extras: LaunchExtras<'_>,
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
    if let Some(a) = extras.agent {
        m.insert("agent".into(), Value::String(a.to_string()));
    }
    if let (Some(w), Some(h)) = (extras.width, extras.height) {
        m.insert("width".into(), Value::String(w.to_string()));
        m.insert("height".into(), Value::String(h.to_string()));
    }
    Value::Object(m)
}

/// `K-R104`：`capture-pane` 命令的**参数构造器**（monitor 这一侧的契约面）。
///
/// 与 [`launch_args`] 同一条理由：字段名一旦与 daemon 的解析器漂开，症状是
/// 「命令发出去了、daemon 回 `invalid_args` 说缺字段」，而两边各自看都「对」。
/// 由 [`tests::the_tmux_primitive_arg_builder_matches_the_daemon_parser`] 对拍。
pub fn capture_pane_args(name: &str) -> Value {
    let mut m = serde_json::Map::new();
    m.insert("name".into(), Value::String(name.to_string()));
    Value::Object(m)
}

/// `create-or-attach` **专有**的那三个可选字段〔`K-P2` `D3` 09-03〕。
///
/// # 为什么是一个结构体，不是再挂三个位置参数
///
/// 挂上去就是**连着五个 `Option<&str>`** —— `width` 与 `height` 同型同类，
/// 调换两个实参编译器一个字都不会说，而症状是「窗口尺寸反了」这种没人会怀疑到调用点的事。
/// 具名字段让那类错**在源码上就看得见**。
///
/// ⚠ `send-into` / `send-keys-raw` 那两个 mode 用 [`LaunchExtras::default`]：
/// 这三个字段**只对新建会话有意义**（daemon 侧也只在 `CreateOrAttach` 那条臂上读它们）。
#[derive(Debug, Clone, Copy, Default)]
pub struct LaunchExtras<'a> {
    /// 哪个 AI —— 落成 tmux 的 `@ccm_agent` 标记。
    pub agent: Option<&'a str>,
    /// 新建窗口宽（十进制串）。**与 `height` 成对**：只给一半时这里直接两个都不发，
    /// 让「半个尺寸」在**发出去之前**就不存在，而不是等 daemon 回 `invalid_args`。
    pub width: Option<&'a str>,
    /// 新建窗口高（十进制串）。见 `width`。
    pub height: Option<&'a str>,
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
/// P2：**本机后端在 registry 里的 key**。
///
/// 远端用 `cfg.origin_label()`（用户配的机器名）。本机没有「机器名」这个概念 ——
/// 前端表示本机是 `origin === null`（`bridge.rs:95` 逐字记着线上约定是**省略**而不是 `null`），
/// 而 registry 的 key 是 `String` ⇒ 需要一个约定值。
///
/// ⚠ **诚实边界**：用户理论上可以把某台远端机器的 label 起成这个名字，两者就撞了。
/// 不做防御（加校验 = 在用户的命名自由上开一个没人会撞的洞），如实登记在 P2 的诚实边界里。
/// 尖括号是刻意的 —— 它不是合法的 ssh host 名，撞名要故意才做得到。
pub const LOCAL_ORIGIN: &str = "<local>";

/// **测试期 `<local>` 的独占锁**。
///
/// 登记表是**进程内全局**的，而 cargo 默认并行跑用例 ⇒ 两条都在 `<local>` 键上起真 daemon
/// 的用例会互相看见对方登记的通道。实测形态：一条用例的 `等通道出现` 立刻为真
/// （其实是另一条登记的），随后 `起来了却没有 pid`。
///
/// ⚠ 中毒也要拿到锁（`into_inner`）：一条用例 panic 不该把其余的全变成「锁中毒」，
/// 那会把**一个**真失败放大成一片假失败，反而盖住原因。
#[cfg(test)]
pub(crate) fn local_origin_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static L: std::sync::Mutex<()> = std::sync::Mutex::new(());
    L.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn client_for(origin: &str) -> Option<Arc<InboundClient>> {
    lock(registry()).get(origin).cloned()
}

#[cfg(test)]
#[path = "../../../tests/bridge/inbound_client_tests.rs"]
mod tests;
