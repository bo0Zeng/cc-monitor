//! 常驻监听 —— 脱离宿主之后，backend 还能被找到、被问到、被接上（`src/doc/INVARIANTS.md` § 48.1：门由内核给）。
//! stdio 载体上宿主一退读端就断、backend 随即 broken-pipe 退出；难的不是怎么脱离，是脱离之后还怎么跟它对话。
//!
//! # 走 Unix 套接字，门由内核给
//!
//! 套接字住这台家里只给本人的目录（`relay_route_core::listen_socket_for`，目录 `0700`），收下的每条连接再核一次对端 uid
//! （共享 crate `own-chan`）⇒ 连得上的只有本账号自己的进程，没有钥匙这一层（从前回环 TCP 没有权限位，才要钥匙补）。
//! 远端经 ssh 跑一次 `ccm -- --resident-attach`（`control/resident.rs` 那个小中继）连它：ssh 已经证明了「是本人」。
//! 起它的：本机 `src/frontend/shell/src/local_backend_host.rs`，远端 `--resident-ensure`（`control/resident.rs`）—— 同一种交法：只交 [`ENV_RESIDENT`]。
//!
//! # 两档连接，hello 写在分档之前
//!
//! - 流：attach 行形状对 ⇒ 这条连接接管出 / 入两个方向（[`admit`]）；
//!   每条连接各一份 watcher / inbound / writer（`main.rs::serve_listening`），几个前端可以同时挂着。
//! - 不限次的「只读 hello 就走」：连上就有 hello，读完即关。
//!
//! hello 写在分档之前：「有没有长驻后端」靠读一行 hello 答，协议一个字节都不用加。
//! 多客户是「每条连接各一份」，不是把一条观测通道扇出给 N 个（`single_stream_guard.rs` 钉着「源码里恰好一份、按连接实例化」）。
//!
//! # 边界
//!
//! 1. 一台机器一个家一个常驻后端：目录独占锁抢不到就带 [`EXIT_ADDR_IN_USE`] 退出，绝不换个地方再起一个
//!    （换地方 = 每台机 N 个 backend，中转口与全部 SSH 各 N 份）。宿主连上去先读 hello 比对，对不上就出声并拒绝。
//! 2. 一个定时器都没有：`accept` 与读一行都阻塞在内核事件上（`no_timer_guard::backend_production_code_has_no_periodic_wakeups`）。
//! 3. 别的 uid 连进来（root 例外不了：内核报的就是它的 uid）⇒ 关掉、出声，hello 都不给。

/// 宿主告诉后端「你是常驻的那一个」的 env 名（值 `1`）。听哪个套接字不交：按家算，宿主与后端同一个函数。
pub const ENV_RESIDENT: &str = crate::platform::child_env::RESIDENT;

/// 远端 `--resident-ensure` 起常驻后端时交的中转口 env 名（`main.rs` 在库外，够不着 `relay::` 的 crate 内口）。
pub const RELAY_PORT_ENV: &str = crate::relay::ENV_PORT;

/// 目录独占锁抢不到（已有一个常驻后端在听）的退出码。**与「起不来」区分开**：
/// 宿主看到它就知道「已经有人在听了」，该去连而不是再起一个。
pub const EXIT_ADDR_IN_USE: i32 = 3;

/// 常驻配置不成立（开关值不认得 · 沙箱跑却要占真家 · 绑不上）的退出码。**fail closed**：宁可不起。
pub const EXIT_BAD_LISTEN_CONFIG: i32 = 4;

/// 认证通过之后回给客户端的那一行。
pub const ATTACH_OK_LINE: &str = "{\"attach\":\"ok\"}\n";

/// 一行 attach 请求的字节上限。请求形如 `{"attach":true,"flags":[…]}`（几十字节），8 KiB 给了两个量级余量。
/// 少了它就是一个无界堆分配：对端可以一直发字节不发换行。
/// 超限：拒收 + 回错（关连接并出声，不静默截断成一行看起来对的 JSON）。登记住址 `tests/frontend/shell/byte_cap_registry.rs`。
pub const ATTACH_LINE_CAP: usize = 8 * 1024;

/// 读一行 attach 请求的三种结局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandshakeLine {
    /// 对端读完 hello 就关了 —— **那正是「只读 hello 就走」那一档**，是正常结局。
    Eof,
    /// 一整行。
    Line(String),
    /// 超限。**丢弃 + 出声**，不静默。
    TooLong(usize),
}

/// 有上限地读一行：`fill_buf`/`consume`，超限之后只找换行、不再往 buf 里塞字节 ⇒ 内存占用与行长无关。
/// `cap` 是参数：判据要能传一个小数，「超限之后内存不涨」才进得了单测。
pub async fn read_capped_line<R>(rd: &mut R, cap: usize) -> std::io::Result<HandshakeLine>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    use tokio::io::AsyncBufReadExt;
    let mut buf: Vec<u8> = Vec::new();
    let mut seen: usize = 0;
    let mut overflowed = false;
    loop {
        let chunk = rd.fill_buf().await?;
        if chunk.is_empty() {
            return Ok(if seen == 0 {
                HandshakeLine::Eof
            } else if overflowed {
                HandshakeLine::TooLong(seen)
            } else {
                HandshakeLine::Line(String::from_utf8_lossy(&buf).into_owned())
            });
        }
        let (take, done) = match chunk.iter().position(|&c| c == b'\n') {
            Some(i) => (i, true),
            None => (chunk.len(), false),
        };
        seen += take;
        if seen > cap {
            overflowed = true;
            buf.clear();
            buf.shrink_to_fit();
        }
        if !overflowed {
            buf.extend_from_slice(&chunk[..take]);
        }
        let consumed = if done { take + 1 } else { take };
        rd.consume(consumed);
        if done {
            return Ok(if overflowed {
                HandshakeLine::TooLong(seen)
            } else {
                HandshakeLine::Line(String::from_utf8_lossy(&buf).into_owned())
            });
        }
    }
}

/// 拒绝的理由（闭集）：`refusal_line` 只拼这里的常量，没有任何一段外来字节
/// 会进到那行 JSON 里（由 `refusal_reasons_are_a_closed_set` 钉住）。
/// 常驻后端自己只发这一个：attach 行形状不对。
pub const REFUSE_MALFORMED: &str = "malformed-attach";
/// 小中继（`--resident-attach`）连不上：没人在听（刚起的还没绑上 / 起来就退了）。monitor 隔一会儿再接。
pub const REFUSE_ABSENT: &str = "absent";
/// 小中继连不上：别的原因（不归本人 · 路径坏了）。再接也一样。
pub const REFUSE_UNREACHABLE: &str = "unreachable";

/// backend 这次跑成什么形态。由环境决定，不由 argv 决定：换的是同一个流模式的载体，不是新增一条子命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// stdin/stdout 一对管道，宿主一退就死（被监护的本机后端 · 远端 `--stream`）。
    Stdio,
    /// 常驻：听这台家里那个套接字。
    Listen,
}

/// 从环境算出形态。纯函数（`get` 是入参）。
/// - 没有 / 空串 ⇒ [`Mode::Stdio`]（shell 里 `export X=` 是常态）。
/// - `1` ⇒ [`Mode::Listen`]。
/// - 别的值 ⇒ `Err`：多半是接线写错了，静默退回 stdio 会让「我明明开了常驻」变成一个查不出来的谜。
pub fn mode_from(get: &dyn Fn(&str) -> Option<String>) -> Result<Mode, String> {
    match get(ENV_RESIDENT).as_deref().map(str::trim) {
        None | Some("") => Ok(Mode::Stdio),
        Some("1") => Ok(Mode::Listen),
        Some(other) => Err(crate::common::contract::malformed(&format!(
            "{ENV_RESIDENT}={other:?} is neither empty nor 1; refusing to guess the carrier"
        ))),
    }
}

/// 一条 attach 请求的裁决。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// 形状对：要流。
    Attach,
    /// 根本不是一条 attach 请求（不是 JSON / 没有 `attach` 字段 / 值不是 `true`）。
    Malformed,
}

/// 判一行 attach 请求。**纯函数**。形状：`{"attach":true}`（可带 `tz` · `view`，见 [`attach_flags`]）。
///
/// 刻意**不复用 `wire::Frame`** —— 那是冻结兼容面（仓外 aterm 在读），往它加变体是一次跨仓契约变更；
/// 而握手这件事只发生在 hello 之后、流之前，它不该出现在任何一条被消费的帧流里。
pub fn attach_verdict(line: &str) -> Verdict {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
        return Verdict::Malformed;
    };
    if v.get("attach") == Some(&serde_json::Value::Bool(true)) {
        Verdict::Attach
    } else {
        Verdict::Malformed
    }
}

/// attach 行里这条连接要什么（`{"attach":true,"tz":"Asia/Shanghai","view":{…}}`）：`tz` 同 [`crate::TZ_FLAG`]（认不得 ⇒ UTC）·
/// `view` 同请求信封的声明（[`crate::StreamView`]；缺 ＝ 全量）。别的键（`attach` 以外）· 声明认不出 ⇒ `Err`（当 malformed 拒）。
/// 每条连接各按自己的这一行定，不看进程起参。
pub fn attach_flags(line: &str) -> Result<Option<crate::StreamWants>, ()> {
    let v: serde_json::Value = serde_json::from_str(line.trim()).map_err(|_| ())?;
    let o = v.as_object().ok_or(())?;
    if o.keys()
        .any(|k| !matches!(k.as_str(), "attach" | "tz" | "view"))
    {
        return Err(());
    }
    let view = match o.get("view") {
        Some(decl) => crate::StreamView::parse(decl).map_err(|_| ())?,
        None => None,
    };
    Ok(Some(crate::StreamWants {
        tz: o.get("tz").map(crate::Tz::of).unwrap_or_default(),
        view,
    }))
}

/// 拒绝那一行。`reason` 只许是本模块的常量。
pub fn refusal_line(reason: &str) -> String {
    format!("{{\"attach\":\"refused\",\"reason\":\"{reason}\"}}\n")
}

/// 这条连接给什么档。**把「谁占着流」这件事做成入参**，而不是去读一个全局 ——
/// 入参才测得到「已经有人占着」那一支（否则那一支要真起两个客户端才走得到，
/// 而那正是 `brief` 第 9 条说的**空真**）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admit {
    /// 交出流。
    Stream,
    /// 拒绝，并把理由写回去（**出声**，不是静默 FIN —— 照 `relay::serve` 那条 503 的形状）。
    Refuse(&'static str),
}

/// 分档。多客户：形状对就交流，不看「有没有人占着」。
pub fn admit(verdict: Verdict) -> Admit {
    match verdict {
        Verdict::Malformed => Admit::Refuse(REFUSE_MALFORMED),
        Verdict::Attach => Admit::Stream,
    }
}

/// 此刻连着的流（多客户）。连接号从 1 起（0 留给空转那一份 watcher 的槽位）；
/// 「最后一个客户走了」= [`Clients::leave`] 回 0 —— 不是「起我的那个 monitor 退了」。
#[derive(Debug, Default)]
pub struct Clients {
    live: std::collections::BTreeSet<u64>,
    next: u64,
}

impl Clients {
    /// 接上一条：回它的连接号。
    pub fn join(&mut self) -> u64 {
        self.next += 1;
        self.live.insert(self.next);
        self.next
    }

    /// 走了一条：回还剩几条（同一个号走两次不重复扣）。
    pub fn leave(&mut self, id: u64) -> usize {
        self.live.remove(&id);
        self.live.len()
    }

    /// 此刻几条。
    pub fn count(&self) -> usize {
        self.live.len()
    }
}

/// 往一条连接写一行并 flush。本模块只有这一处 `write_all(`（在 `wire.rs`「出方向帧只许有一个写者」那条判据的人群里）；
/// 它写的不是出方向帧：握手行发生在 `writer_task` 起来之前，或者根本不给流。
pub async fn write_line<W>(w: &mut W, line: &str) -> std::io::Result<()>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::AsyncWriteExt;
    w.write_all(line.as_bytes()).await?;
    w.flush().await
}

#[cfg(test)]
#[path = "../../../tests/backend/stream/listen_tests.rs"]
mod tests;
