//! 后端这一端对托管口的同步小客户端：连上 · 说 `hello{want: view, who: backend}` · 说要说的 · 等回音 · 说再见。
//! 只放生产用得到的那一下（打标）；读写都有期限（托管进程不答 ⇒ 照实报，不挂住看守线程）。

use super::wire::{self, Ctl, Frame, Want, Who, WhoKind, PROTO};
use std::io::{Read, Write};
use std::path::Path;

/// 一次读 · 一次写最多等几秒（不是定时器：有字节就返回）。
const WITHIN_SECS: u64 = 5;

/// 写 sid 进那个托管终端的事实，等托管进程写完才回：同一条连接上 `tag` 之后紧跟一个 `clients`，
/// 托管进程按收到的先后逐帧做完（写事实在处理 `tag` 那一下当场做）⇒ `clients` 有了回音，事实已在盘上。
pub(crate) fn tag(sock: &Path, sid: &str) -> std::io::Result<()> {
    let s = own_chan::connect_blocking(sock)?;
    #[allow(
        clippy::disallowed_methods,
        reason = "后端经托管口打标那一问的读写期限：有字节就返回，不驱动循环"
    )]
    let within = std::time::Duration::from_secs(WITHIN_SECS);
    s.set_read_timeout(Some(within))?;
    s.set_write_timeout(Some(within))?;
    let mut w = &s;
    let hello = Ctl::Hello {
        proto: PROTO,
        build: crate::BUILD_ID.to_string(),
        want: Want::View,
        size: None,
        who: Who {
            kind: WhoKind::Backend,
            client: None,
        },
    };
    let tag = Ctl::Tag {
        sid: sid.to_string(),
        agent: String::new(),
    };
    for c in [hello, tag, Ctl::Clients] {
        w.write_all(&wire::encode(&Frame::Ctl(c)))?;
    }
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        while let Some((f, used)) = wire::decode(&buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?
        {
            buf.drain(..used);
            match f {
                Frame::Ctl(Ctl::ClientsAre { .. }) => {
                    // 说再见是礼貌：写不出去也不影响已写好的事实。
                    if let Err(e) = w.write_all(&wire::encode(&Frame::Ctl(Ctl::Bye))) {
                        tracing::debug!("托管口：打标之后说再见没说成：{e}");
                    }
                    return Ok(());
                }
                Frame::Ctl(Ctl::Ended { .. }) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::NotConnected,
                        "hosted terminal ended",
                    ))
                }
                _ => {}
            }
        }
        let n = (&s).read(&mut chunk)?;
        if n == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}
