//! `K-P1`：**常驻监听口** —— 脱离宿主之后，daemon 还能被找到、被问到、被接上。
//!
//! # 它存在的理由（不是「常驻」本身）
//!
//! 今天 monitor 与 daemon 讲协议走的就是那对 stdio 管道；宿主一退读端就断，
//! daemon 在 **153 毫秒**内自己 broken-pipe 退出（`daemon_policy.rs` 头注实测）。
//! ⇒ **真脱离的代价是「再也说不上话」** —— 那正是本模块要补的那一格。
//! `K-P1 §0b-2` 逐字：「难的**不是**怎么脱离（仓里三份现成的），
//! 难的是**脱离之后还怎么跟它对话**」。
//!
//! # 走回环 TCP，理由是判据面已经付过一遍钱
//!
//! `K-H1` 的中转（`relay/server.rs`）已经把这条路上的东西买齐了：`LOOPBACK` 字面量常量
//! + 非回环 bind 的零命中守卫 + 在途上界 + 出声的拒绝。本模块**抄它的形状**。
//! 现打（`K-P1 §0b-2㈠`，分母 = `remote-daemon-proto/src` ∪ `src-tauri/src` 下 169 个 `.rs`）：
//! `UnixListener` 0 处 · daemon 侧 `NamedPipe` 0 处 ⇒ 走 Unix socket / 命名管道都要**从零立**一套。
//!
//! ⚠ **代价如实记，这是一条真裁决不是实现细节**：回环 TCP 上**同机任何本地进程都连得上**，
//! Unix socket 有文件权限位而它没有。收窄只能靠一个 token；而 **daemon 只读铁律不许它自己写文件**
//! （`readonly_guard`）⇒ **token 只能由宿主生成、当 env 传进来**（[`ENV_TOKEN`]）。
//! 宿主那一半住 `src-tauri/src/local_daemon.rs`（`0600` 的 token 文件）。
//!
//! # 两档连接，而 hello 写在分档**之前**
//!
//! - **一条流**：认证通过、且此刻没有别的流挂着 ⇒ 这条连接接管出/入两个方向。
//! - **不限次的「只读 hello 就走」**：连上就有 hello，读完即关。
//!
//! ★ hello 必须写在分档之前，否则「这台机上有没有一个长驻 daemon」这一问
//! 只能从「`connect()` 成没成」推 —— 而 TCP 的 backlog 会让**没人 accept 的口照样连得上**，
//! 那又是一个「一直说是」的假信号（`P2d §0a` 那一形）。
//! ⇒ 有了 hello 这一档，那一问的答案是**读一行**，协议一个字节都不用加。
//!
//! ⚠ **代价如实记**：多客户端的**流**（fan-out + `Overflow.lost` 丢帧账重定义）
//! **本件明确不做**，由 `frozen_single_client_guard.rs` 那条触发器看着。
//!
//! # 诚实边界（三条，都不是措辞）
//!
//! 1. **hello 那一档不认证** —— 它泄露 `claude_dir`（用户自己的家目录路径）、`build_id`、
//!    能力集给同机任何进程。这是有意的取舍：`ccm` 那一问必须问得到，而它拿不到 token。
//!    **能改变世界的那一档（流）一律要 token。**
//! 2. **`EADDRINUSE` 只说明「有人占着这个口」，不说明占着它的是我们的 daemon** ——
//!    所以宿主那一侧连上去**先读 hello 比对**，对不上就出声并拒绝，**不许静默复用**
//!    （`P2t §1` 第 3 问问的正是这一格）。本模块这一侧的处置是：
//!    bind 不上就带 [`EXIT_ADDR_IN_USE`] 退出，**绝不自己换端口**（换端口 = 每台机 N 个 daemon
//!    互相盖 tmux hook 槽位 `[50]`，比今天更糟）。
//! 3. **本模块一个定时器都没有**：`accept` 阻塞在内核事件上，读一行阻塞在内核事件上。
//!    没有 `Duration::from_*`、没有任何会「自己醒过来」的构件
//!    （`no_timer_guard::daemon_production_code_has_no_periodic_wakeups`）。

use std::net::{IpAddr, Ipv4Addr};

/// 只听回环。**字面量常量，不是拼出来的** —— 拼出来的地址源码扫描看不见
/// （理由与 `relay/server.rs::LOOPBACK` 逐字同源）。
pub const LOOPBACK: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

/// 宿主告诉 daemon「听哪个口」的 env 名。
///
/// # 为什么端口不由 daemon 自己算
///
/// 「按家目录 hash 出一个口」这条路要求**两边各算一份**，而两份就会漂
/// （本仓已有同族先例：`shared/ccm` 与 monitor 各算一份 origin，实测分叉四处）。
/// ⇒ **只留一份实现，住宿主那一侧**（`local_daemon::listen_port_for`），daemon 只收一个数。
pub const ENV_PORT: &str = "CCM_LISTEN_PORT";

/// 宿主传进来的 attach token。**daemon 自己造不出它** —— 只读铁律不许它写文件，
/// 而 token 必须在两个宿主进程之间传得下去（上一个 monitor 退了，下一个要接上同一个 daemon）。
pub const ENV_TOKEN: &str = "CCM_LISTEN_TOKEN";

/// bind 不上（多半是 `EADDRINUSE`）的退出码。**与「起不来」区分开**：
/// 宿主看到它就知道「那个口上已经有人了」，该去连而不是再起一个。
pub const EXIT_ADDR_IN_USE: i32 = 3;

/// 监听口的配置只写了一半时的退出码。**fail closed**：宁可不起，也不要起一个不设防的口。
pub const EXIT_BAD_LISTEN_CONFIG: i32 = 4;

/// 认证通过之后回给客户端的那一行。
pub const ATTACH_OK_LINE: &str = "{\"attach\":\"ok\"}\n";

/// 一行 attach 请求的字节上限。
///
/// 请求形如 `{"attach":"<32 位十六进制>"}` —— 本机实测 **51 字节**。8 KiB 给了两个量级余量。
/// ⚠ **少了它就是一个无界堆分配**：这条连接的对端是**同机任何进程**，
/// 它完全可以一直发字节不发换行。daemon 侧为同一形栽过一次实测
/// （`inbound.rs` 头注：喂 512 MiB 无换行的流 ⇒ RSS 从 6 MiB 涨到 518 MiB）。
/// 超限语义：**拒收 + 回错**（关连接并出声，不静默截断成一行「看起来对」的 JSON）。
/// **登记住址** `src-tauri/src/byte_cap_registry.rs`（那张表默认拒绝：不登记就红）。
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

/// 有上限地读一行。
///
/// 机制是 `fill_buf`/`consume`：**超限之后只找换行、不再往 buf 里塞字节**
/// ⇒ 整行的内存占用与行长无关。这段机制在本仓已有两处同构实现
/// （`inbound.rs` 的读行循环 · `ssh_source::read_capped_line`），
/// 三处共用的是**那条教训**，不是代码 —— 它们分别跨着 sync/async 与两个 crate 的边。
///
/// ⚠ `cap` **是参数而不是直接读常量**：生产调用点传 [`ATTACH_LINE_CAP`]，
/// 而判据要能传一个小数 —— 否则「超限之后内存不涨」只能靠量 RSS 来证，而那种证法进不了单测。
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
pub const REFUSE_BUSY: &str = "stream-busy";
pub const REFUSE_AUTH: &str = "bad-token";
pub const REFUSE_MALFORMED: &str = "malformed-attach";

/// daemon 这次跑成什么形态。**由环境决定，不由 argv 决定** ——
/// argv 那张表（`main.rs::SUBCOMMANDS`）一动就要 bump `BUILD_ID` 并改
/// `IPC-PROTOCOL.md` 的对拍面，而本件没有新增任何**子命令**：
/// 它换的是**同一个流模式的载体**，不是新增一条命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// 今天那条路：stdin/stdout 一对管道，宿主一退就死。
    Stdio,
    /// 常驻：听一个回环口，认 token 之后才交出流。
    Listen { port: u16, token: String },
}

/// 从环境算出形态。**纯函数**（`get` 是入参），所以两条错误支都测得到 ——
/// 拿真 `std::env::var` 写的话，「token 缺席」那一支在测试里根本进不去。
///
/// 四条规则，每条都有一个具体的坏结局在后面顶着：
/// - 两个都没有 ⇒ [`Mode::Stdio`]（**今天的行为一个字节不变**）。
/// - 有口没 token ⇒ `Err`：那会起一个**同机任何进程都能对它发 `launch` 的口**。
/// - 有 token 没口 ⇒ `Err`：多半是接线漏了一半，静默退回 stdio 会让「我明明开了常驻」
///   变成一个查不出来的谜（`P2d §0a` 那一形：**假信号不报错，它只是一直说是**）。
/// - 口解析不出来 / 是 0 ⇒ `Err`：0 会让内核随机挑一个口，而宿主正等在那个算好的口上。
pub fn mode_from(get: &dyn Fn(&str) -> Option<String>) -> Result<Mode, String> {
    let raw_port = get(ENV_PORT).filter(|s| !s.trim().is_empty());
    let raw_token = get(ENV_TOKEN).filter(|s| !s.trim().is_empty());
    match (raw_port, raw_token) {
        (None, None) => Ok(Mode::Stdio),
        (None, Some(_)) => Err(format!(
            "设了 {ENV_TOKEN} 却没设 {ENV_PORT} —— 监听口的配置只写了一半。\n\
             不静默退回 stdio：那会让「我明明开了常驻」变成一个查不出来的谜。"
        )),
        (Some(_), None) => Err(format!(
            "设了 {ENV_PORT} 却没设 {ENV_TOKEN} —— 拒绝起一个不设防的口。\n\
             回环 TCP 上同机任何本地进程都连得上，而流那一档能发 `launch`/`kill`。\n\
             token 由宿主生成并用 {ENV_TOKEN} 传进来（daemon 只读，自己造不出它）。"
        )),
        (Some(p), Some(t)) => {
            let port: u16 = p
                .trim()
                .parse()
                .map_err(|e| format!("{ENV_PORT}={p:?} 不是一个端口号: {e}"))?;
            if port == 0 {
                return Err(format!(
                    "{ENV_PORT}=0 —— 0 会让内核随机挑一个口，而宿主正等在它算好的那个口上。"
                ));
            }
            Ok(Mode::Listen {
                port,
                token: t.trim().to_string(),
            })
        }
    }
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

/// 定长时间的字节比对：**跑完全部**，不提前返回。
///
/// 长度不同直接判不等（长度本来就藏不住，它在 `read_line` 的字节数里）。
fn tokens_match(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() || a.is_empty() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

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

/// 分档。`stream_taken` = 此刻是不是已经有一条流挂着。
pub fn admit(verdict: Verdict, stream_taken: bool) -> Admit {
    match verdict {
        Verdict::Malformed => Admit::Refuse(REFUSE_MALFORMED),
        Verdict::WrongToken => Admit::Refuse(REFUSE_AUTH),
        Verdict::Attach if stream_taken => Admit::Refuse(REFUSE_BUSY),
        Verdict::Attach => Admit::Stream,
    }
}

/// 往一条连接写一行并 flush。
///
/// ⚠ **本模块只有这一处 `write_all(`** —— 它是 `wire.rs` 那条
/// 「出方向帧只许有一个写者」判据的人群里新加的一员（本件同轮把 `listen.rs` 补进了那张表）。
/// 它写的**不是**出方向帧：握手行发生在 `writer_task` 起来之前 / 或者根本不给流，
/// 与 `write_and_flush_hello` 同一个性质。
pub async fn write_line<W>(w: &mut W, line: &str) -> std::io::Result<()>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::AsyncWriteExt;
    w.write_all(line.as_bytes()).await?;
    w.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k: &str| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| (*v).to_string())
        }
    }

    /// ★ 两个都没有 ⇒ 今天那条路，**一个字节不变**。
    #[test]
    fn no_env_means_todays_stdio_path() {
        assert_eq!(mode_from(&env_of(&[])).expect("空环境"), Mode::Stdio);
    }

    /// ★★ **有口没 token ⇒ 拒绝起**。这一条是本模块最要紧的一格。
    ///
    /// 放过它的后果很具体：同机任何本地进程（**含别的用户**）连上那个口就能发
    /// `launch` / `kill` —— 以本账号的身份执行。回环 TCP 没有权限位，
    /// 补回来的只有这一个 token。
    #[test]
    fn a_port_without_a_token_is_refused() {
        let e = mode_from(&env_of(&[(ENV_PORT, "51000")])).unwrap_err();
        assert!(
            e.contains(ENV_TOKEN),
            "拒绝的理由里没点名 {ENV_TOKEN} —— 那句诊断说不清该去补什么：{e}"
        );
    }

    /// 有 token 没口 ⇒ 也拒，且**不静默退回 stdio**。
    #[test]
    fn a_token_without_a_port_is_refused_loudly() {
        let e = mode_from(&env_of(&[(ENV_TOKEN, "abc")])).unwrap_err();
        assert!(e.contains(ENV_PORT), "{e}");
    }

    /// 端口 0 与不是数字的都要拒 —— 0 会让内核随机挑口，而宿主等在算好的那个口上。
    #[test]
    fn port_zero_and_garbage_are_refused() {
        assert!(mode_from(&env_of(&[(ENV_PORT, "0"), (ENV_TOKEN, "t")])).is_err());
        assert!(mode_from(&env_of(&[(ENV_PORT, "no"), (ENV_TOKEN, "t")])).is_err());
        assert!(mode_from(&env_of(&[(ENV_PORT, "70000"), (ENV_TOKEN, "t")])).is_err());
    }

    /// 空串按「没设」算 —— shell 里 `export X=` 是常态，把它读成「设了个空 token」
    /// 就等于**用空串当口令**。
    #[test]
    fn empty_strings_count_as_unset() {
        assert_eq!(mode_from(&env_of(&[(ENV_TOKEN, "  ")])).unwrap(), Mode::Stdio);
        assert!(mode_from(&env_of(&[(ENV_PORT, "51000"), (ENV_TOKEN, " ")])).is_err());
    }

    #[test]
    fn both_present_gives_listen_mode() {
        let m = mode_from(&env_of(&[(ENV_PORT, "51000"), (ENV_TOKEN, "s3cret")])).unwrap();
        assert_eq!(
            m,
            Mode::Listen {
                port: 51000,
                token: "s3cret".into()
            }
        );
    }

    /// ★ 三张脸各判一次，且**错的 token 不许被判成 `Malformed`** ——
    /// 两者的处置一样（都拒），但诊断不一样，而诊断是这条路上唯一能查的东西。
    #[test]
    fn attach_verdicts_are_three_distinct_faces() {
        assert_eq!(
            attach_verdict("{\"attach\":\"good\"}", "good"),
            Verdict::Attach
        );
        assert_eq!(
            attach_verdict("{\"attach\":\"bad\"}", "good"),
            Verdict::WrongToken
        );
        assert_eq!(attach_verdict("not json", "good"), Verdict::Malformed);
        assert_eq!(attach_verdict("{\"attach\":1}", "good"), Verdict::Malformed);
        assert_eq!(attach_verdict("{}", "good"), Verdict::Malformed);
        // 尾随换行/空白要吃掉：`read_line` 给的就是带 `\n` 的那一行。
        assert_eq!(
            attach_verdict("{\"attach\":\"good\"}\n", "good"),
            Verdict::Attach
        );
    }

    /// ★★ **空 token 永远配不上**。
    ///
    /// 少了这一格，`ENV_TOKEN` 万一被读成空串（或者哪天有人放宽了上面那条），
    /// 客户端发 `{"attach":""}` 就直接过 —— 而那看起来是「认证通过」。
    #[test]
    fn an_empty_token_never_matches() {
        assert!(!tokens_match("", ""));
        assert_eq!(attach_verdict("{\"attach\":\"\"}", ""), Verdict::WrongToken);
    }

    /// ★ 前缀 / 后缀 / 大小写都不许当成对。
    #[test]
    fn tokens_match_is_exact() {
        assert!(tokens_match("abc", "abc"));
        assert!(!tokens_match("ab", "abc"));
        assert!(!tokens_match("abcd", "abc"));
        assert!(!tokens_match("ABC", "abc"));
    }

    /// ★★ **分档表逐格钉死** —— 这是「一条流 + 不限次 hello」那一刀的全部内容。
    ///
    /// ⚠ `stream_taken` 是**入参**而不是全局，正是为了让「已经有人占着」这一支
    /// 在不起两个真客户端的前提下也走得到（`brief` 第 9 条：空真那一族）。
    #[test]
    fn the_two_tier_split_is_pinned_cell_by_cell() {
        assert_eq!(admit(Verdict::Attach, false), Admit::Stream);
        assert_eq!(
            admit(Verdict::Attach, true),
            Admit::Refuse(REFUSE_BUSY),
            "第二条要流的连接必须**出声地**拒（不是静默 FIN），否则客户端只能靠超时去猜"
        );
        assert_eq!(admit(Verdict::WrongToken, false), Admit::Refuse(REFUSE_AUTH));
        assert_eq!(
            admit(Verdict::Malformed, true),
            Admit::Refuse(REFUSE_MALFORMED)
        );
        assert_ne!(
            admit(Verdict::WrongToken, false),
            admit(Verdict::Attach, true),
            "「token 不对」与「口被占着」拒得一样 ⇒ 现场没法区分「我配置错了」与「已经有一个 monitor 连着」"
        );
    }

    /// 拒绝理由是闭集，且拼出来的每一行都是**合法 JSON**。
    ///
    /// ⚠ 这条防的是「顺手把一段外来字节拼进那行」：只要 `reason` 还是这三个常量，
    /// 那行就不可能被撕开；哪天有人改成 `refusal_line(&err.to_string())`，本条当场红。
    #[test]
    fn refusal_reasons_are_a_closed_set() {
        for r in [REFUSE_BUSY, REFUSE_AUTH, REFUSE_MALFORMED] {
            let line = refusal_line(r);
            assert!(line.ends_with('\n'), "NDJSON 每行必须以换行收尾：{line:?}");
            let v: serde_json::Value =
                serde_json::from_str(line.trim()).expect("拒绝行必须是合法 JSON");
            assert_eq!(v["attach"], "refused");
            assert_eq!(v["reason"], r);
        }
        // 反向锚点：本模块生产段里 `refusal_line(` 的实参**只许**是那三个常量。
        let prod = crate::guard_support::production_code(include_str!("listen.rs"));
        let calls: Vec<&str> = prod
            .lines()
            .map(str::trim)
            .filter(|l| l.contains("refusal_line(") && !l.starts_with("pub fn"))
            .collect();
        for c in &calls {
            assert!(
                c.contains("REFUSE_") || c.contains("reason)"),
                "`refusal_line` 被喂了一个不是闭集里的理由：{c}\n\
                 那一行会把外来字节拼进 JSON ⇒ 撕行。"
            );
        }
    }

    /// ★ `ATTACH_OK_LINE` 也得是合法的一行 NDJSON（同上，形状钉死）。
    #[test]
    fn the_ok_line_is_one_valid_ndjson_line() {
        assert!(ATTACH_OK_LINE.ends_with('\n'));
        assert_eq!(ATTACH_OK_LINE.matches('\n').count(), 1);
        let v: serde_json::Value = serde_json::from_str(ATTACH_OK_LINE.trim()).expect("合法 JSON");
        assert_eq!(v["attach"], "ok");
    }

    /// ★★ **只听回环** —— 与 `relay::server` 那条同一个理由，同一个形状。
    ///
    /// 它单独存在时是安慰剂（`bind_guard` 头注自陈过），所以行为那半由
    /// `e2e/local-backend-supervise.sh` 的真进程用例兜。
    #[test]
    fn the_listen_address_is_loopback_and_it_is_a_literal() {
        assert_eq!(LOOPBACK, IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert!(!LOOPBACK.is_unspecified(), "0.0.0.0 = 全网可达");
        let prod = crate::guard_support::production_code(include_str!("listen.rs"));
        assert!(
            prod.contains("IpAddr::V4(Ipv4Addr::LOCALHOST)"),
            "回环地址不再是一个**字面量**常量 —— 拼出来的地址源码扫描看不见"
        );
    }

    /// ★★ **超限之后内存不涨** —— 这条性质只有把 `cap` 做成参数才测得动。
    ///
    /// 拿真常量（8 KiB）来测的话，要喂进去的字节量大到只能靠量 RSS 去证，
    /// 而那种证法进不了单测（daemon 侧当年正是那么发现问题的）。
    #[tokio::test]
    async fn an_over_cap_attach_line_is_dropped_whole_and_says_so() {
        let long = format!("{}\n", "x".repeat(100));
        let mut rd = tokio::io::BufReader::new(long.as_bytes());
        assert_eq!(
            read_capped_line(&mut rd, 8).await.expect("读"),
            HandshakeLine::TooLong(100),
            "超限的行必须**整行丢弃并报出字节数**，不许截断成一行「看起来对」的 JSON"
        );
    }

    /// 三种结局各一格。⚠ `Eof` 那一格**不是错误** —— 它正是「只读 hello 就走」那一档。
    #[tokio::test]
    async fn the_three_handshake_line_outcomes_are_each_reachable() {
        let mut eof = tokio::io::BufReader::new(&b""[..]);
        assert_eq!(read_capped_line(&mut eof, 64).await.expect("读"), HandshakeLine::Eof);

        let mut one = tokio::io::BufReader::new(&b"{\"attach\":\"t\"}\n"[..]);
        assert_eq!(
            read_capped_line(&mut one, 64).await.expect("读"),
            HandshakeLine::Line("{\"attach\":\"t\"}".into())
        );

        // 没有换行就 EOF：仍当一行交出去（`read_line` 的旧行为逐字如此）。
        let mut half = tokio::io::BufReader::new(&b"abc"[..]);
        assert_eq!(
            read_capped_line(&mut half, 64).await.expect("读"),
            HandshakeLine::Line("abc".into())
        );
    }

    /// 两个退出码不许撞：宿主按它们分「口被占着」与「配置写错了」两条不同的路。
    #[test]
    fn the_two_exit_codes_are_distinct_and_nonzero() {
        assert_ne!(EXIT_ADDR_IN_USE, EXIT_BAD_LISTEN_CONFIG);
        assert!(EXIT_ADDR_IN_USE > 0 && EXIT_BAD_LISTEN_CONFIG > 0);
    }
}
