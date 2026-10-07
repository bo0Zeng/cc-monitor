//! 常驻监听口 —— 脱离宿主之后，backend 还能被找到、被问到、被接上（`src/doc/INVARIANTS.md` § 48.1：监听口要钥匙）。
//! stdio 载体上宿主一退读端就断、backend 随即 broken-pipe 退出；难的不是怎么脱离，是脱离之后还怎么跟它对话。
//!
//! # 走回环 TCP
//!
//! 形状照中转（`relay/`）：`LOOPBACK` 字面量常量 + 非回环 bind 的零命中守卫 + 在途上界 + 出声的拒绝。
//! 代价：回环 TCP 上同机任何本地进程都连得上（Unix socket 有文件权限位而它没有）⇒ 收窄靠钥匙：起它的那一方只交钥匙文件的路径
//! （[`ENV_TOKEN_FILE`]），常驻后端绑上口之后自己铸一把新的、原子写进那份 `0600` 的文件（`control/resident.rs::rotate_token`，
//! 后端自有状态文件，`readonly_guard` 第四层登记）。每次起都换 ⇒ 漏出去的旧钥匙随之作废；客户端每次读文件。
//! 起它的：本机 `src/frontend/shell/src/local_backend_host.rs`，远端 `--resident-ensure`（`control/resident.rs`）—— 同一种交法。
//!
//! # 两档连接，hello 写在分档之前
//!
//! - 流：认证通过 ⇒ 这条连接接管出 / 入两个方向。钥匙对上就交流，不看「有没有别人挂着」（[`admit`]）；
//!   每条连接各一份 watcher / inbound / writer（`main.rs::serve_listening`），几个前端可以同时挂着。
//! - 不限次的「只读 hello 就走」：连上就有 hello，读完即关。
//!
//! hello 写在分档之前：TCP 的 backlog 会让没人 accept 的口照样连得上，「有没有长驻后端」只能靠读一行 hello 答。
//! 多客户是「每条连接各一份」，不是把一条观测通道扇出给 N 个（`single_stream_guard.rs` 钉着「源码里恰好一份、按连接实例化」）。
//!
//! # 边界
//!
//! 1. hello 那一档不认证：它给同机任何进程看 `claude_dir`、`build_id`、能力集（`ccm` 那一问必须问得到，而它拿不到 token）。
//!    能改变世界的那一档（流）一律要 token。
//! 2. `EADDRINUSE` 只说明有人占着这个口，不说明是我们的 backend：宿主连上去先读 hello 比对，对不上就出声并拒绝；
//!    这一侧 bind 不上就带 [`EXIT_ADDR_IN_USE`] 退出，绝不自己换端口（换端口 = 每台机 N 个 backend，中转口与全部 SSH 各 N 份）。
//! 3. 一个定时器都没有：`accept` 与读一行都阻塞在内核事件上（`no_timer_guard::backend_production_code_has_no_periodic_wakeups`）。

use std::net::{IpAddr, Ipv4Addr};

/// 只听回环。**字面量常量，不是拼出来的** —— 拼出来的地址源码扫描看不见。
/// 中转的宿主（`relay/listen.rs`）绑口用的也是这一个。
pub const LOOPBACK: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

/// 宿主告诉后端「听哪个口」的 env 名。端口只算一份、住宿主那一侧（`local_backend_host::listen_port_for`）：两边各算一份就会漂。
pub const ENV_PORT: &str = crate::platform::child_env::LISTEN_PORT;

/// 钥匙文件的**路径**（不是钥匙）：起常驻后端的那一方交（本机 monitor · 远端 `--resident-ensure`，同一种），子进程自己读 ——
/// 钥匙一次都不经过 env / argv（与中转钥匙同形，`relay/door.rs` 头注）。钥匙若在环境里，常驻后端起的 tmux server 会把它
/// 拷成全局环境，之后每个窗格里的 shell · ccm · claude 都带着它。
pub const ENV_TOKEN_FILE: &str = crate::platform::child_env::LISTEN_TOKEN_FILE;

/// 远端 `--resident-ensure` 起常驻后端时交的中转口 env 名（`main.rs` 在库外，够不着 `relay::` 的 crate 内口）。
pub const RELAY_PORT_ENV: &str = crate::relay::ENV_PORT;

/// bind 不上（多半是 `EADDRINUSE`）的退出码。**与「起不来」区分开**：
/// 宿主看到它就知道「那个口上已经有人了」，该去连而不是再起一个。
pub const EXIT_ADDR_IN_USE: i32 = 3;

/// 监听口的配置只写了一半时的退出码。**fail closed**：宁可不起，也不要起一个不设防的口。
pub const EXIT_BAD_LISTEN_CONFIG: i32 = 4;

/// 认证通过之后回给客户端的那一行。
pub const ATTACH_OK_LINE: &str = "{\"attach\":\"ok\"}\n";

/// 一行 attach 请求的字节上限。请求形如 `{"attach":"<32 位十六进制>"}`（约 51 字节），8 KiB 给了两个量级余量。
/// 少了它就是一个无界堆分配：对端是同机任何进程，可以一直发字节不发换行。
/// 超限：拒收 + 回错（关连接并出声，不静默截断成一行看起来对的 JSON）。登记住址 `src/frontend/shell/src/byte_cap_registry.rs`。
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

/// 拒绝的三种理由。**是闭集**：`refusal_line` 只拼这几个常量，没有任何一段外来字节
/// 会进到那行 JSON 里（由 `refusal_reasons_are_a_closed_set` 钉住）。
/// 多客户之后**不再发**（[`admit`]）；常量留着只因 monitor 本机宿主还认它（那一臂成死路）。
pub const REFUSE_BUSY: &str = "stream-busy";
pub const REFUSE_AUTH: &str = "bad-token";
pub const REFUSE_MALFORMED: &str = "malformed-attach";

/// backend 这次跑成什么形态。由环境决定，不由 argv 决定：换的是同一个流模式的载体，不是新增一条子命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// 今天那条路：stdin/stdout 一对管道，宿主一退就死。
    Stdio,
    /// 常驻：听一个回环口，认钥匙之后才交出流。钥匙文件（[`ENV_TOKEN_FILE`]）由它绑上口之后自己写一把新的。
    Listen { port: u16, token_file: String },
}

/// 从环境算出形态。纯函数（`get` 是入参），两条错误支都测得到。
/// - 两个都没有 ⇒ [`Mode::Stdio`]。
/// - 有口没钥匙文件 ⇒ `Err`：那会起一个同机任何进程都能对它发 `launch` 的口。
/// - 有钥匙文件没口 ⇒ `Err`：多半是接线漏了一半，静默退回 stdio 会让「我明明开了常驻」变成一个查不出来的谜。
/// - 口解析不出来 / 是 0 ⇒ `Err`：0 会让内核随机挑一个口，而宿主正等在那个算好的口上。
pub fn mode_from(get: &dyn Fn(&str) -> Option<String>) -> Result<Mode, String> {
    let raw_port = get(ENV_PORT).filter(|s| !s.trim().is_empty());
    let raw_file = get(ENV_TOKEN_FILE).filter(|s| !s.trim().is_empty());
    match (raw_port, raw_file) {
        (None, None) => Ok(Mode::Stdio),
        (None, Some(_)) => Err(crate::common::contract::malformed(&format!(
            "{ENV_TOKEN_FILE} is set but {ENV_PORT} is not; refusing to fall back to stdio"
        ))),
        (Some(_), None) => Err(crate::common::contract::malformed(&format!(
            "{ENV_PORT} is set but {ENV_TOKEN_FILE} is not; refusing to open an unauthenticated port"
        ))),
        (Some(p), Some(f)) => Ok(Mode::Listen {
            port: parse_port(&p)?,
            token_file: f.trim().to_string(),
        }),
    }
}

fn parse_port(p: &str) -> Result<u16, String> {
    let port: u16 = p.trim().parse().map_err(|e| {
        crate::common::contract::malformed(&format!("{ENV_PORT}={p:?} is not a port number: {e}"))
    })?;
    if port == 0 {
        return Err(crate::common::contract::malformed(&format!("{ENV_PORT}=0 would let the kernel pick a random port, but the host waits on the one it chose")));
    }
    Ok(port)
}

/// 读钥匙文件（常驻后端绑上口之后写的那一把）：读不动 / 空 ⇒ `Err`。报错里只有路径。读者：`--resident-ensure`（口上已有人时回它）。
pub fn token_from_file(path: &str) -> Result<String, String> {
    let t = std::fs::read_to_string(path).map_err(|e| {
        crate::common::contract::malformed(&format!("{ENV_TOKEN_FILE}={path:?}: {e}"))
    })?;
    let t = t.trim();
    if t.is_empty() {
        return Err(crate::common::contract::malformed(&format!(
            "{ENV_TOKEN_FILE}={path:?} is empty; refusing to open an unauthenticated port"
        )));
    }
    Ok(t.to_string())
}

/// 一条 attach 请求的裁决。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// token 对上了。**这只说明「可以要流」**，要不要给还看那一档占没占（见 [`Admit`]）。
    Attach,
    /// token 不对。
    WrongToken,
    /// 根本不是一条 attach 请求（不是 JSON / 没有 `attach` 字段 / 值不是串）。
    Malformed,
}

/// 判一行 attach 请求。**纯函数**。
///
/// 形状：`{"attach":"<token>"}`。刻意**不复用 `wire::Frame`** —— 那是冻结兼容面
/// （仓外 aterm 在读），往它加变体是一次跨仓契约变更；而握手这件事只发生在
/// hello 之后、流之前，它不该出现在任何一条被消费的帧流里。
///
/// ⚠ 比对走 [`tokens_match`]（逐字节全跑完），不是 `==`：
/// `==` 在第一个不同的字节就返回，长度/前缀信息会从耗时里漏出去。
/// 这是**同机**攻击面，计时侧信道在这里不是理论上的东西。
pub fn attach_verdict(line: &str, expected: &str) -> Verdict {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
        return Verdict::Malformed;
    };
    let Some(got) = v.get("attach").and_then(|x| x.as_str()) else {
        return Verdict::Malformed;
    };
    if tokens_match(got, expected) {
        Verdict::Attach
    } else {
        Verdict::WrongToken
    }
}

/// attach 行里这条连接要的流模式旗标（`{"attach":…,"flags":["--tail-only",…]}`）。
/// 缺 ⇒ `Ok(None)`（用进程起参那一份）；有但不是串数组、或含 `lib::STREAM_FLAGS` 以外的 ⇒ `Err`（当 malformed 拒）。
/// 回这条连接索要了什么：每个客户各按自己的能力协商（monitor `decide_stream_flags`）。
pub fn attach_flags(line: &str) -> Result<Option<crate::StreamWants>, ()> {
    let v: serde_json::Value = serde_json::from_str(line.trim()).map_err(|_| ())?;
    let Some(raw) = v.get("flags") else {
        return Ok(None);
    };
    let list = raw.as_array().ok_or(())?;
    let mut words = Vec::with_capacity(list.len());
    for f in list {
        let w = f.as_str().ok_or(())?;
        if !crate::STREAM_FLAGS.contains(&w) {
            return Err(());
        }
        words.push(w.to_string());
    }
    Ok(Some(crate::split_stream_flags(words).1))
}

/// 定长比对的唯一住址在中转那一份（中转口的门也用它）。
pub(crate) use comms_outward::tokens_match;

/// 拒绝那一行。`reason` 只许是本模块那三个常量之一。
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

/// 分档。多客户：钥匙对上就交流，不再看「有没有人占着」（[`REFUSE_BUSY`] 不再发）。
pub fn admit(verdict: Verdict) -> Admit {
    match verdict {
        Verdict::Malformed => Admit::Refuse(REFUSE_MALFORMED),
        Verdict::WrongToken => Admit::Refuse(REFUSE_AUTH),
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
