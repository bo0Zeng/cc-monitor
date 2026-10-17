//! 接常驻流那一段握手（`ccm -- --resident-attach`，`IPC-PROTOCOL.md` 载体表「常驻套接字」）：
//! 链路一开就写 attach 行（不等 hello：hello 写在分档之前），再读小中继的拒绝行或 hello、读回话（带那台相对这一版的新旧）——
//! 从开出链路到通着一个来回。桌面与手机同一份零件；两端都整段走 [`handshake`]（经 [`crate::feed`]）。

use crate::client::{split_and_park, BackendHello, ParkedWriter};
use crate::line::{read_capped_line, CappedLine};
use cmd_wire::attach::{AttachLine, AttachReply, Relation, Yes};
use cmd_wire::watch::WatchAsk;
use copy_core::copy_text;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, ReadHalf, WriteHalf};

/// 小中继（`--resident-attach`）回 `absent`：常驻后端还没绑上口（刚起 / 刚换）。桌面隔 [`RELAY_WAIT`] 再试，至多 [`RELAY_TRIES`] 次。
pub const RELAY_ABSENT: &str = "absent";
/// 等常驻后端绑上口那一段：至多试几次 · 每次隔多久（合计 6 秒）。
pub const RELAY_TRIES: u32 = 30;
pub const RELAY_WAIT: Duration = Duration::from_millis(200);

/// 握手那两行（hello · attach 回话）的上限：hello 实测 8.6 KB（10-13：命令名 · 撤不动名单 · 每条 typed 命令的形状指纹三张表，
/// typed 命令越多越长），64 KiB 给足余量，又堵死「一直发字节不发换行」。
pub const HANDSHAKE_LINE_CAP: usize = 64 * 1024;

/// 小中继断开时第一行就是它（`{"attach":"refused","reason":…}`）⇒ 那个原因；不是 ⇒ `None`。
pub fn relay_refusal(first: &str) -> Option<String> {
    match AttachReply::of(first)? {
        AttachReply::Refused { reason } => Some(reason),
        AttachReply::Ok { .. } => None,
    }
}

/// attach 行：「我要流」＋ 看的这一台的时区 ＋ 手上这一版的身份 ＋ 在看名单（本机 · 远端 · 手机同一处拼）。
/// `tz` ⇒ `None` 不带（后端按 UTC 写推来的钟面）；`mine` ⇒ `None` 不带（那台就不回新旧）；`watch` 照名单台此刻那份（[`crate::watch::Desk::ask_for_attach`]）。
pub fn attach_line(tz: Option<&str>, mine: Option<&str>, watch: Option<WatchAsk>) -> String {
    AttachLine {
        attach: Yes,
        tz: tz.map(str::to_string),
        mine: mine.map(str::to_string),
        watch,
    }
    .line()
}

/// attach 回话：「可以」⇒ 那台相对 `mine` 的新旧（没回 ⇒ 不可比）；拒了 ⇒ [`AttachFail::Refused`]（原因原样）；别的 ⇒ 那一句。
pub fn attach_reply(line: &str) -> Result<Relation, AttachFail> {
    match AttachReply::of(line) {
        Some(AttachReply::Ok { relation }) => Ok(relation.unwrap_or(Relation::Incomparable)),
        Some(AttachReply::Refused { reason }) => Err(AttachFail::Refused(reason)),
        None => Err(AttachFail::Reply(copy_text(
            "rsRemoteResident.handshake.refused",
            &[(
                "why",
                &copy_text("rsRemoteResident.handshake.notAttachReply", &[]),
            )],
        ))),
    }
}

/// 一行 hello：自报的身份 ＋ 全形（解不开 ⇒ 只有头）。
#[derive(Debug, Clone)]
pub struct Hello {
    pub build_id: String,
    /// 那一行原样（宿主要读别的几格时照它的帧类型再解一遍）。
    pub line: String,
    /// 全形解得开 ⇒ 那一帧；解不开 ⇒ `None`（认得出是我们的后端、哪一版，别的一格都不信：一条命令不发、一帧不进）。
    pub frame: Option<cmd_wire::frame::Hello>,
}

impl Hello {
    /// 两步：先只解头（不是 hello / 头解不出 ⇒ `None`：那头不是我们的后端），再解全形。
    pub fn parse(line: &str) -> Option<Hello> {
        let line = line.trim();
        let head = cmd_wire::frame::HelloHead::of(line)?;
        let frame = match cmd_wire::frame::decode::<serde::de::IgnoredAny>(line) {
            Ok(cmd_wire::frame::Frame::Hello(h)) => Some(h),
            _ => None,
        };
        Some(Hello {
            build_id: head.build_id,
            line: line.to_string(),
            frame,
        })
    }

    /// 见证：全形解不开 ⇒ 按只有头的那一帧给（能力事实全空、帧种全对不上）。
    pub fn witness(&self) -> BackendHello {
        match &self.frame {
            Some(h) => BackendHello::of(h),
            None => BackendHello::of(&cmd_wire::frame::Hello {
                build_id: self.build_id.clone(),
                ..Default::default()
            }),
        }
    }
}

/// 握手走不通在哪一步。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachFail {
    /// 小中继拒了（`absent` 没人在听 · `unreachable` 别的原因），原因原样。
    Refused(String),
    /// 那台认不得这一形 attach 行（回 `malformed-attach`）：比这一形早 ⇒ 当旧。
    Old,
    /// 第一行不是 hello：那头不是 cc-monitor 后端（原样带回）。
    NotOurs(String),
    /// 没说完就断了 / 读不动 / 超过期限（`hello` = 断在等 hello 那一步）；带那一句与原话。
    Cut {
        hello: bool,
        said: String,
        raw: String,
    },
    /// attach 回话不是「可以」：那一句。
    Reply(String),
}

/// 握完手的那一条：读半边（带缓冲，紧跟 attach 的帧已在里面）· 停着的写半边 · hello · 那台相对这一版 · attach 行带过的名单。
pub struct Attached<S> {
    pub reader: BufReader<ReadHalf<S>>,
    pub writer: ParkedWriter<WriteHalf<S>>,
    pub hello: Hello,
    pub relation: Relation,
    pub carried: Option<WatchAsk>,
}

async fn line_within<R>(r: &mut R, hello: bool, within: Duration) -> Result<String, AttachFail>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    let mut buf = Vec::new();
    match tokio::time::timeout(within, read_capped_line(r, &mut buf, HANDSHAKE_LINE_CAP)).await {
        Err(_) => Err(cut_short(hello, format!("{} ms", within.as_millis()))),
        Ok(Ok(CappedLine::Line)) => Ok(String::from_utf8_lossy(&buf)
            .trim_end_matches(['\n', '\r'])
            .to_string()),
        Ok(Ok(CappedLine::TooLong(n))) => Err(cut_short(hello, format!("line of {n} bytes"))),
        Ok(Ok(CappedLine::Eof)) => Err(cut_short(hello, String::new())),
        Ok(Err(e)) => Err(read_failed(hello, e.to_string())),
    }
}

/// 握手那一行没说完（等 hello · 等 attach 回话）。
fn cut_short(hello: bool, raw: String) -> AttachFail {
    let said = copy_text("reason.link.lost", &[]);
    AttachFail::Cut { hello, said, raw }
}

/// 握手那一行读不动。
fn read_failed(hello: bool, raw: String) -> AttachFail {
    AttachFail::Cut {
        hello,
        said: copy_text("rsRemoteResident.handshake.readFailed", &[]),
        raw,
    }
}

/// 常驻后端认不得 attach 行时回的原因（后端 `wire/listen.rs` 的 `REFUSE_MALFORMED`）。
pub const REFUSE_MALFORMED: &str = "malformed-attach";

/// 整段握手：写 attach 行（`mine` ＝ 手上这一版；没带 ⇒ 新旧记不可比；`watch` ＝ 在看名单）→ 读第一行（拒绝 ⇒ [`AttachFail::Refused`]；不是 hello ⇒
/// [`AttachFail::NotOurs`]）→ 读回话（认不得这一形 ⇒ [`AttachFail::Old`]）。每一行各等至多 `within`。
/// 写半边在写完 attach 行那一刻就停住（[`split_and_park`]）。attach 行没写出去（链路已断）照读第一行：小中继的拒绝行先于断开。
pub async fn handshake<S>(
    mut stream: S,
    tz: Option<&str>,
    mine: Option<&str>,
    watch: Option<WatchAsk>,
    within: Duration,
) -> Result<Attached<S>, AttachFail>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let sent = async {
        stream
            .write_all(attach_line(tz, mine, watch.clone()).as_bytes())
            .await?;
        stream.flush().await
    }
    .await;
    let (rd, writer) = split_and_park(stream);
    let mut reader = BufReader::new(rd);
    let first = line_within(&mut reader, true, within).await;
    if let Ok(first) = &first {
        if let Some(reason) = relay_refusal(first) {
            return Err(AttachFail::Refused(reason));
        }
    }
    if let Err(e) = sent {
        return Err(AttachFail::Cut {
            hello: false,
            said: copy_text(
                "rsRemoteResident.handshake.attachNotSent",
                &[("why", &copy_core::io_reason(e.kind()))],
            ),
            raw: e.to_string(),
        });
    }
    let first = first?;
    let hello = Hello::parse(&first).ok_or_else(|| AttachFail::NotOurs(first.clone()))?;
    let reply = line_within(&mut reader, false, within).await?;
    let relation = match attach_reply(&reply) {
        Err(AttachFail::Refused(r)) if r == REFUSE_MALFORMED => return Err(AttachFail::Old),
        r => r?,
    };
    Ok(Attached {
        reader,
        writer,
        hello,
        relation: if mine.is_some() {
            relation
        } else {
            Relation::Incomparable
        },
        carried: watch,
    })
}

#[cfg(test)]
#[path = "../../../tests/comms/resident-client/attach_tests.rs"]
mod tests;
