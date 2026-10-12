//! 接常驻流那一段握手（`ccm -- --resident-attach`，`IPC-PROTOCOL.md` 载体表「常驻套接字」）：
//! 小中继的拒绝行 → hello → attach 行 → `{"attach":"ok"}`。桌面（monitor `remote_resident.rs`）与手机（[`crate::feed`]）同一份零件；
//! 手机那一侧整段走 [`handshake`]，桌面那一侧在它自己的链路上逐步调这里的零件（中间要问本机常驻后端换不换）。

use crate::client::{split_and_park, HelloFrame, ParkedWriter};
use crate::line::{read_capped_line, CappedLine};
use comms_inward::chan::wire::Offer;
use copy_core::copy_text;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, ReadHalf, WriteHalf};

/// 小中继（`--resident-attach`）回 `absent`：常驻后端还没绑上口（刚起 / 刚换）。桌面隔 [`RELAY_WAIT`] 再试，至多 [`RELAY_TRIES`] 次。
pub const RELAY_ABSENT: &str = "absent";
/// 等常驻后端绑上口那一段：至多试几次 · 每次隔多久（合计 6 秒）。
pub const RELAY_TRIES: u32 = 30;
pub const RELAY_WAIT: Duration = Duration::from_millis(200);

/// 握手那两行（hello · attach 回话）的上限：hello 实测一两 KB，8 KiB 给足余量，又堵死「一直发字节不发换行」。
pub const HANDSHAKE_LINE_CAP: usize = 8 * 1024;

/// 小中继断开时第一行就是它（`{"attach":"refused","reason":…}`）⇒ 那个原因；不是 ⇒ `None`。
pub fn relay_refusal(first: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(first.trim()).ok()?;
    (v["attach"] == "refused").then(|| v["reason"].as_str().unwrap_or_default().to_string())
}

/// 第一行是 hello ⇒ 它自报的 `build_id`；不是 ⇒ 那一句「不是 cc-monitor 后端」。
pub fn hello_build(line: &str) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(line.trim()).unwrap_or_default();
    if v["kind"] != "hello" {
        return Err(copy_text("rsRemoteResident.hello.notOurs", &[]));
    }
    Ok(v["build_id"].as_str().unwrap_or_default().to_string())
}

/// attach 行：「我要流」＋ 看的这一台的时区 ＋ 这条流的出口声明（后端 `listen::attach_flags` 的逆；本机 · 远端 · 手机同一处拼）。
/// `tz` ⇒ `None` 不带（后端按 UTC 写推来的钟面）；`view` ＝ 请求信封同一套词（`None` ⇒ 不带 ＝ 全量）。
pub fn attach_line(tz: Option<&str>, view: Option<&serde_json::Value>) -> String {
    let mut o = serde_json::Map::new();
    o.insert("attach".into(), true.into());
    if let Some(tz) = tz {
        o.insert("tz".into(), tz.into());
    }
    if let Some(v) = view {
        o.insert("view".into(), v.clone());
    }
    format!("{}\n", serde_json::Value::Object(o))
}

/// 远端那条流的出口声明：会话帧里不要本机才用得上的 `pid`。
pub fn remote_stream_view() -> serde_json::Value {
    serde_json::json!({ "omit": { "session_added": ["pid"] } })
}

/// attach 回话：`{"attach":"ok"}` ⇒ 成；别的 ⇒ 那一句（带那头说的原因）。
pub fn attach_reply(line: &str) -> Result<(), String> {
    let v: serde_json::Value = serde_json::from_str(line).unwrap_or_default();
    if v["attach"] == "ok" {
        return Ok(());
    }
    let why = match v["reason"].as_str() {
        Some(r) => r.to_string(),
        None => copy_text("rsRemoteResident.handshake.notAttachReply", &[]),
    };
    Err(copy_text(
        "rsRemoteResident.handshake.refused",
        &[("why", &why)],
    ))
}

/// 一行 hello（原样留着，手机交给 Kotlin）：协议版本 · 自报的身份 · 能力事实。
#[derive(Debug, Clone)]
pub struct Hello {
    pub v: u64,
    pub build_id: String,
    offer: Offer,
    pub raw: String,
}

impl Hello {
    /// 是 hello 那一形 ⇒ `Some`；别的 ⇒ `None`。能力事实缺格 ⇒ 当空（旧后端）。
    pub fn parse(line: &str) -> Option<Hello> {
        let v: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
        let obj = v.as_object()?;
        if obj.get("kind")?.as_str()? != "hello" {
            return None;
        }
        let strs = |k: &str| -> Vec<String> {
            obj.get(k)
                .and_then(serde_json::Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };
        // 能力事实的另三格：读法住 `Offer::facts_of` · `Offer::shapes_of`（monitor 解 hello 同调它们）。
        let (unavailable, uncancellable) = Offer::facts_of(obj);
        let offer = Offer::new(strs("commands"), unavailable, uncancellable)
            .with_shapes(Offer::shapes_of(obj));
        Some(Hello {
            v: obj
                .get("v")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
            build_id: obj
                .get("build_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            offer,
            raw: line.trim().to_string(),
        })
    }
}

impl HelloFrame for Hello {
    fn hello_offer(&self) -> Option<Offer> {
        Some(self.offer.clone())
    }
}

/// 握手走不通在哪一步。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachFail {
    /// 小中继拒了（`absent` 没人在听 · `unreachable` 别的原因 · 常驻后端嫌 attach 行不对），原因原样。
    Refused(String),
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

/// 握完手的那一条：读半边（带缓冲，紧跟 attach 的帧已在里面）· 停着的写半边 · hello。
pub struct Attached<S> {
    pub reader: BufReader<ReadHalf<S>>,
    pub writer: ParkedWriter<WriteHalf<S>>,
    pub hello: Hello,
}

async fn line_within<R>(r: &mut R, hello: bool, within: Duration) -> Result<String, AttachFail>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    let cut = |said: &str, raw: String| AttachFail::Cut {
        hello,
        said: copy_text(said, &[]),
        raw,
    };
    let key = if hello {
        "rsRemoteResident.handshake.helloCut"
    } else {
        "rsRemoteResident.handshake.replyCut"
    };
    let mut buf = Vec::new();
    match tokio::time::timeout(within, read_capped_line(r, &mut buf, HANDSHAKE_LINE_CAP)).await {
        Err(_) => Err(cut(key, format!("{} ms", within.as_millis()))),
        Ok(Ok(CappedLine::Line)) => Ok(String::from_utf8_lossy(&buf)
            .trim_end_matches(['\n', '\r'])
            .to_string()),
        Ok(Ok(CappedLine::TooLong(n))) => Err(cut(key, format!("line of {n} bytes"))),
        Ok(Ok(CappedLine::Eof)) => Err(cut(key, String::new())),
        Ok(Err(e)) => Err(cut("rsRemoteResident.handshake.readFailed", e.to_string())),
    }
}

/// 整段握手（手机那一侧）：读第一行（拒绝 ⇒ [`AttachFail::Refused`]；不是 hello ⇒ [`AttachFail::NotOurs`]）→ 写 attach 行 → 等「可以」。
/// 每一行各等至多 `within`。写半边在切开的那一刻就停住（[`split_and_park`]），握手那一行经切开之前的流写。
pub async fn handshake<S>(
    mut stream: S,
    tz: Option<&str>,
    view: Option<&serde_json::Value>,
    within: Duration,
) -> Result<Attached<S>, AttachFail>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    // 握手那两行读在切开之前（attach 行要经同一条流写回去）：一字节一字节读，不预读过 hello 那一行。
    let first = read_line_unbuffered(&mut stream, true, within).await?;
    if let Some(reason) = relay_refusal(&first) {
        return Err(AttachFail::Refused(reason));
    }
    let hello = Hello::parse(&first).ok_or_else(|| AttachFail::NotOurs(first.clone()))?;
    let line = attach_line(tz, view);
    let sent = async {
        stream.write_all(line.as_bytes()).await?;
        stream.flush().await
    }
    .await;
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
    let (rd, writer) = split_and_park(stream);
    let mut reader = BufReader::new(rd);
    let reply = line_within(&mut reader, false, within).await?;
    if let Some(reason) = relay_refusal(&reply) {
        return Err(AttachFail::Refused(reason));
    }
    attach_reply(&reply).map_err(AttachFail::Reply)?;
    Ok(Attached {
        reader,
        writer,
        hello,
    })
}

/// 切开之前读一行（不带缓冲：多读一个字节都会被切开之后的读半边丢掉）。
async fn read_line_unbuffered<S>(
    s: &mut S,
    hello: bool,
    within: Duration,
) -> Result<String, AttachFail>
where
    S: AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;
    let key = if hello {
        "rsRemoteResident.handshake.helloCut"
    } else {
        "rsRemoteResident.handshake.replyCut"
    };
    let cut = |said: &str, raw: String| AttachFail::Cut {
        hello,
        said: copy_text(said, &[]),
        raw,
    };
    let read = async {
        let mut out = Vec::new();
        let mut one = [0u8; 1];
        loop {
            match s.read(&mut one).await {
                Ok(0) => return Err(cut(key, String::new())),
                Ok(_) if one[0] == b'\n' => return Ok(out),
                Ok(_) => {
                    out.push(one[0]);
                    if out.len() > HANDSHAKE_LINE_CAP {
                        return Err(cut(key, format!("line over {HANDSHAKE_LINE_CAP} bytes")));
                    }
                }
                Err(e) => return Err(cut("rsRemoteResident.handshake.readFailed", e.to_string())),
            }
        }
    };
    match tokio::time::timeout(within, read).await {
        Err(_) => Err(cut(key, format!("{} ms", within.as_millis()))),
        Ok(r) => r.map(|b| {
            String::from_utf8_lossy(&b)
                .trim_end_matches('\r')
                .to_string()
        }),
    }
}

#[cfg(test)]
#[path = "../../../tests/comms/resident-client/attach_tests.rs"]
mod tests;
