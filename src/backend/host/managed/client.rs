//! 后端这一端对托管口的同步小客户端：连上 · 说 `hello{want: view, who: backend}` · 说要说的 · 等回音 · 说再见。
//! 读写都有期限（托管进程不答 ⇒ 照实报，不挂住调用方的线程）。持有输入那一条长连接在 [`super::hold`]。

use super::wire::{self, ClientRow, Ctl, Frame, Size, Want, Who, WhoKind, PROTO};
use std::io::{Read, Write};
use std::path::Path;

/// 一次读 · 一次写最多等几秒（不是定时器：有字节就返回）。
const WITHIN_SECS: u64 = 5;

fn bad(why: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, why.to_string())
}

/// 连上、设好期限、说 `hello`（只看：后端从不改尺寸；`front` ＝ 替哪个前端问）。
pub(crate) fn open(sock: &Path, front: Option<&str>) -> std::io::Result<own_chan::BlockingStream> {
    let s = own_chan::connect_blocking(sock)?;
    #[allow(
        clippy::disallowed_methods,
        reason = "后端经托管口问一下的读写期限：有字节就返回，不驱动循环"
    )]
    let within = std::time::Duration::from_secs(WITHIN_SECS);
    s.set_read_timeout(Some(within))?;
    s.set_write_timeout(Some(within))?;
    let hello = Ctl::Hello {
        proto: PROTO,
        build: crate::BUILD_ID.to_string(),
        want: Want::View,
        size: None,
        who: Who {
            kind: WhoKind::Backend,
            client: front.map(str::to_string),
        },
    };
    send(&s, &Frame::Ctl(hello))?;
    Ok(s)
}

/// 写一帧。
pub(crate) fn send(s: &own_chan::BlockingStream, f: &Frame) -> std::io::Result<()> {
    (&*s).write_all(&wire::encode(f))
}

/// 从这条连接上读一帧（`buf` 存着上一回剩下的半帧）。断了 ⇒ `UnexpectedEof`。
pub(crate) fn next_frame(
    s: &own_chan::BlockingStream,
    buf: &mut Vec<u8>,
) -> std::io::Result<Frame> {
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        if let Some((f, used)) = wire::decode(buf).map_err(|e| bad(&e.to_string()))? {
            buf.drain(..used);
            return Ok(f);
        }
        let n = (&*s).read(&mut chunk)?;
        if n == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

/// 说再见是礼貌：写不出去也不影响已经问到的。
pub(crate) fn bye(s: &own_chan::BlockingStream) {
    if let Err(e) = send(s, &Frame::Ctl(Ctl::Bye)) {
        tracing::debug!("托管口：说再见没说成：{e}");
    }
}

fn ended() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::NotConnected, "hosted terminal ended")
}

/// 说完 `ctls`，读到 `pick` 认的那一帧为止（终端结束了 ⇒ `NotConnected`），说再见。
fn ask<T>(sock: &Path, ctls: Vec<Ctl>, pick: impl Fn(Ctl) -> Option<T>) -> std::io::Result<T> {
    let s = open(sock, None)?;
    for c in ctls {
        send(&s, &Frame::Ctl(c))?;
    }
    let mut buf = Vec::new();
    loop {
        match next_frame(&s, &mut buf)? {
            Frame::Ctl(Ctl::Ended { .. }) => return Err(ended()),
            Frame::Ctl(c) => {
                if let Some(t) = pick(c) {
                    bye(&s);
                    return Ok(t);
                }
            }
            _ => {}
        }
    }
}

/// 写 sid 进那个托管终端的事实，等托管进程写完才回：同一条连接上 `tag` 之后紧跟一个 `clients`，
/// 托管进程按收到的先后逐帧做完（写事实在处理 `tag` 那一下当场做）⇒ `clients` 有了回音，事实已在盘上。
pub(crate) fn tag(sock: &Path, sid: &str) -> std::io::Result<()> {
    let tag = Ctl::Tag {
        sid: sid.to_string(),
        agent: String::new(),
    };
    ask(sock, vec![tag, Ctl::Clients], |c| {
        matches!(c, Ctl::ClientsAre { .. }).then_some(())
    })
}

/// 连着的各端（问的这一条自己不算：去掉 `mine` 那一行）。
pub(crate) fn clients(sock: &Path) -> std::io::Result<Vec<ClientRow>> {
    ask(sock, vec![Ctl::Clients], |c| match c {
        Ctl::ClientsAre { clients } => Some(clients.into_iter().filter(|r| !r.mine).collect()),
        _ => None,
    })
}

/// 抓回来的一屏：回滚里最近 `back` 行 ＋ 当屏（`color` ⇒ 行里带 SGR）、尺寸、光标（行 · 列）。
pub(crate) struct Captured {
    pub(crate) lines: Vec<String>,
    pub(crate) capped: bool,
    pub(crate) size: Size,
    pub(crate) cursor: [u16; 2],
}

pub(crate) fn capture(sock: &Path, back: usize, color: bool) -> std::io::Result<Captured> {
    ask(sock, vec![Ctl::Capture { back, color }], |c| match c {
        Ctl::Captured {
            lines,
            capped,
            size,
            cursor,
        } => Some(Captured {
            lines,
            capped,
            size,
            cursor,
        }),
        _ => None,
    })
}

/// 结束这个终端（子进程组挂断）：等到 `ended`（或托管进程收尾断开）才回。
pub(crate) fn end(sock: &Path) -> std::io::Result<()> {
    let s = open(sock, None)?;
    send(&s, &Frame::Ctl(Ctl::End))?;
    let mut buf = Vec::new();
    loop {
        match next_frame(&s, &mut buf) {
            Ok(Frame::Ctl(Ctl::Ended { .. })) => return Ok(()),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => return Err(e),
        }
    }
}
