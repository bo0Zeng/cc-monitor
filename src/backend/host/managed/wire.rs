//! 托管口（`hosts/<id>/sock`）上的小协议：瘦客户端与后端都只经它跟托管进程说话，不碰伪终端（A3-3-2）。
//!
//! 帧：`[u32 长（大端，只算种 ＋ 载荷）][u8 种][载荷]`。种 [`KIND_CTL`] 的载荷是一条 JSON（[`Ctl`]），
//! [`KIND_OUT`]（下行：伪终端原始输出）· [`KIND_IN`]（上行：要送进伪终端的原始字节）的载荷就是字节本身。
//!
//! 版本：`hello` 带 [`PROTO`]；动词只增不改（托管进程活过升级，新后端要接得住旧托管）。认不出的动词读成 [`Ctl::Unknown`]，
//! 收的那一方不理它、不断线。认不出的种、超过 [`MAX_FRAME`] 的长 ⇒ 读的那一方断这条连接。

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// 协议版本。加动词不升；改了已有动词的意思才升（那时新的一方要按对方的 `proto` 说话）。
pub(crate) const PROTO: u32 = 1;
/// 一帧最长（种 ＋ 载荷）。整屏一帧最大也就几百 KB；超过它 ⇒ 不是我们的对端，断线。
pub(crate) const MAX_FRAME: u32 = 16 * 1024 * 1024;

pub(crate) const KIND_CTL: u8 = 0;
pub(crate) const KIND_OUT: u8 = 1;
pub(crate) const KIND_IN: u8 = 2;

/// 一帧。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Frame {
    Ctl(Ctl),
    /// 伪终端原始输出（托管 → 各端）。
    Out(Vec<u8>),
    /// 要送进伪终端的原始字节（持有者 → 托管）。
    In(Vec<u8>),
}

/// 接上的是哪一种端。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum WhoKind {
    /// 一个真终端里的瘦客户端（用户的终端窗口 · ssh 进来的终端 · 手机的 SSH 终端）。
    Terminal,
    /// 后端（桌面终端页 · 手机抓屏 · 实时预览 · 打标）。
    Backend,
}

/// `hello` 里报的身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Who {
    pub(crate) kind: WhoKind,
    /// 后端那一端替哪个前端问（`desktop` · `mobile`）；瘦客户端不填。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) client: Option<String>,
}

/// 接上要什么：只看，还是要输入（最新连上、要输入的真终端持有）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Want {
    View,
    Input,
}

/// 行 × 列。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Size {
    pub(crate) rows: u16,
    pub(crate) cols: u16,
}

/// 输入此刻在谁手里（给每一端看的那一份：`mine` 按收的那一端算）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Holder {
    /// 持有那一端的连接号（托管进程里唯一，不复用）。
    pub(crate) client: u64,
    pub(crate) kind: WhoKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) front: Option<String>,
    /// 内核报的对端进程号（问不出 ⇒ 没有）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pid: Option<u32>,
    pub(crate) mine: bool,
}

/// `clients` 回的一行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ClientRow {
    pub(crate) client: u64,
    pub(crate) kind: WhoKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) front: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pid: Option<u32>,
    /// 接上的时刻（unix 毫秒）。
    pub(crate) since_ms: i64,
    /// 最近一次送来东西的时刻（unix 毫秒）。
    pub(crate) last_ms: i64,
    pub(crate) holder: bool,
    pub(crate) mine: bool,
}

/// 控制帧（种 [`KIND_CTL`] 的载荷）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "kebab-case")]
pub(crate) enum Ctl {
    // ── 上行（各端 → 托管）──
    /// 接上后第一帧。
    Hello {
        proto: u32,
        build: String,
        want: Want,
        /// 这一端自己的尺寸（真终端报；后端不报，后端从不改尺寸）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        size: Option<Size>,
        who: Who,
    },
    /// 接管输入。
    Take,
    /// 交还输入（交给最近连上的另一个真终端，没有就空着）。
    Release,
    /// 这一端的尺寸变了（只有持有者的会改伪终端）。
    Resize { rows: u16, cols: u16 },
    /// 往回抓：回滚里最近 `back` 行 ＋ 当屏。
    Capture {
        back: usize,
        #[serde(default)]
        color: bool,
    },
    /// 列连着的各端。
    Clients,
    /// 后端写来的会话身份（同 tmux 那一形写 `@ccm_sid`）。
    Tag { sid: String, agent: String },
    /// 结束这个终端（子进程组 SIGHUP）。
    End,
    /// 这一端要走了（断开，会话照跑）。
    Bye,

    // ── 下行（托管 → 各端）──
    /// 回 `hello`：整屏字节 ＋ 此刻谁持有。
    Welcome {
        proto: u32,
        id: String,
        size: Size,
        /// 这一端的连接号。
        you: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        holder: Option<Holder>,
        screen: String,
    },
    /// 持有者变了。
    Holder {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        holder: Option<Holder>,
    },
    /// 伪终端尺寸变了（之后的 `out` 按新尺寸算）。
    Size { rows: u16, cols: u16 },
    /// 这一端落下了：照这份整屏重来（之后的 `out` 接着它）。
    Resync { size: Size, screen: String },
    /// 送进来的 `in` 没收：输入在别处。
    HeldElsewhere {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        holder: Option<Holder>,
    },
    /// 回 `capture`。
    Captured {
        lines: Vec<String>,
        capped: bool,
        size: Size,
        cursor: [u16; 2],
    },
    /// 回 `clients`。
    ClientsAre { clients: Vec<ClientRow> },
    /// 子进程退了（之后托管进程收尾退出）。
    Ended {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<i32>,
    },
    /// 认不出的动词（对端比自己新）：不理，不断线。
    #[serde(other)]
    Unknown,
}

/// 编一帧（连长度头）。
pub(crate) fn encode(f: &Frame) -> Vec<u8> {
    let json;
    let (kind, body): (u8, &[u8]) = match f {
        Frame::Ctl(c) => {
            json = serde_json::to_vec(c).unwrap_or_default();
            (KIND_CTL, &json)
        }
        Frame::Out(b) => (KIND_OUT, b),
        Frame::In(b) => (KIND_IN, b),
    };
    let len = u32::try_from(body.len() + 1).unwrap_or(u32::MAX);
    let mut v = Vec::with_capacity(body.len() + 5);
    v.extend_from_slice(&len.to_be_bytes());
    v.push(kind);
    v.extend_from_slice(body);
    v
}

/// 读帧那一下的坏结局（读的那一方据此断线）。
#[derive(Debug)]
pub(crate) enum Bad {
    Io(std::io::Error),
    TooLong(u32),
    Kind(u8),
    Json(String),
}

impl std::fmt::Display for Bad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Bad::Io(e) => write!(f, "io: {e}"),
            Bad::TooLong(n) => write!(f, "frame too long: {n}"),
            Bad::Kind(k) => write!(f, "unknown frame kind: {k}"),
            Bad::Json(e) => write!(f, "bad control frame: {e}"),
        }
    }
}

/// 从字节里解一帧：`Ok(None)` ＝ 还不够一帧。交回帧与用掉的字节数。
pub(crate) fn decode(buf: &[u8]) -> Result<Option<(Frame, usize)>, Bad> {
    if buf.len() < 4 {
        return Ok(None);
    }
    let len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if len == 0 || len > MAX_FRAME {
        return Err(Bad::TooLong(len));
    }
    let total = 4 + len as usize;
    if buf.len() < total {
        return Ok(None);
    }
    let f = frame_of(buf[4], &buf[5..total])?;
    Ok(Some((f, total)))
}

fn frame_of(kind: u8, body: &[u8]) -> Result<Frame, Bad> {
    match kind {
        KIND_CTL => serde_json::from_slice(body)
            .map(Frame::Ctl)
            .map_err(|e| Bad::Json(e.to_string())),
        KIND_OUT => Ok(Frame::Out(body.to_vec())),
        KIND_IN => Ok(Frame::In(body.to_vec())),
        k => Err(Bad::Kind(k)),
    }
}

/// 读一帧：`Ok(None)` ＝ 对端在帧边界上关了。
pub(crate) async fn read_frame<R: AsyncRead + Unpin>(r: &mut R) -> Result<Option<Frame>, Bad> {
    let mut head = [0u8; 4];
    match r.read_exact(&mut head).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(Bad::Io(e)),
    }
    let len = u32::from_be_bytes(head);
    if len == 0 || len > MAX_FRAME {
        return Err(Bad::TooLong(len));
    }
    let mut body = vec![0u8; len as usize];
    r.read_exact(&mut body).await.map_err(Bad::Io)?;
    frame_of(body[0], &body[1..]).map(Some)
}

/// 写一帧。
pub(crate) async fn write_frame<W: AsyncWrite + Unpin>(w: &mut W, f: &Frame) -> std::io::Result<()> {
    w.write_all(&encode(f)).await
}

#[cfg(test)]
#[path = "../../../../tests/backend/host/wire_tests.rs"]
mod tests;
