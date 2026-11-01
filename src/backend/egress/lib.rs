//! **出网口**：这台所有出站 TCP 只在这里开（后端 `clippy.toml` 禁开 TCP 的那几个调用，只本文件那一处放行）。
//!
//! 一份出网描述（[`Egress`]）决定到一个目的地怎么走：直连，或经代理（HTTP `CONNECT` · SOCKS5，代理鉴权照描述带）；
//! 回环目的地恒直连、免代理表命中 ⇒ 直连；代理那一格读不懂 ⇒ 一条都不连、报那一句。代理永远在最外层：中转去上游 · 拨 SSH 都在它里面。
//!
//! 这台的那一份由宿主起来时装一次（[`install`]；常驻后端照它起来那一刻的环境，`Egress::from_env`），各处经 [`here`] 取；
//! 按会话的描述以后由调用方自己递一份进 [`open`]，缺省就是这台的。每台用自己的设置，不跨机器传。
//!
//! 本 crate 不读环境变量、不碰家目录、不起定时器：期限由调用方给，握手期间装在 socket 上（读写一次的上限，不是周期唤醒）。

// 测试段可以用会自己醒来的构件（夹具的期限）；生产段零期限字面量（期限都由调用方给）。
#![cfg_attr(test, allow(clippy::disallowed_methods))]

use std::io;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::OnceLock;
use std::time::Duration;

mod describe;
mod tunnel;

pub use describe::{Egress, Hop, Kind, Proxy, NO_PROXY_NAMES, PROXY_NAMES};

static HERE: OnceLock<Egress> = OnceLock::new();

/// 宿主起来时装一次这台的描述。已经装过 ⇒ 原样退回 `Err`。
pub fn install(e: Egress) -> Result<(), Egress> {
    HERE.set(e)
}

/// 这台的描述（还没装 ⇒ 直连）。
pub fn here() -> &'static Egress {
    static DIRECT: OnceLock<Egress> = OnceLock::new();
    HERE.get()
        .unwrap_or_else(|| DIRECT.get_or_init(Egress::direct))
}

/// 开出去的一条 TCP。
#[derive(Debug)]
pub struct Opened {
    /// 到目的地的字节流（经代理时是握手完了的那条隧道）。读写期限是调用方给的那一个。
    pub tcp: TcpStream,
    /// 经了代理（这条 socket 的对端是代理，不是目的地：往返时间之类按对端量的事实不是到目的地的）。
    pub via_proxy: bool,
}

/// 没开成在哪一步。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// 名字解析不出（直连时目的地的 · 经代理时代理的 · `socks5://` 时目的地的）。
    Resolve,
    /// TCP 没连上（目的地或代理）。
    Connect,
    /// 代理那一跳：握手被拒 · 读不懂 · 代理那一格设置读不懂。
    Proxy,
}

/// 没开成：哪一步 ＋ 下层的错。
#[derive(Debug)]
pub struct StageErr {
    pub stage: Stage,
    pub io: io::Error,
}

impl From<StageErr> for io::Error {
    fn from(f: StageErr) -> io::Error {
        f.io
    }
}

/// 照描述 `e` 开一条到 `host:port` 的 TCP。`deadline` 装在 socket 上（读写各一次的上限），代理握手也受它管；`None` ⇒ 不装（同 `TcpStream` 的缺省）。
pub fn open(
    e: &Egress,
    host: &str,
    port: u16,
    deadline: Option<Duration>,
) -> Result<Opened, StageErr> {
    match e.hop_for(host) {
        Hop::Direct => Ok(Opened {
            tcp: tcp_to(host, port, deadline)?,
            via_proxy: false,
        }),
        Hop::Refuse { from, why } => Err(StageErr {
            stage: Stage::Proxy,
            io: io::Error::new(io::ErrorKind::InvalidInput, format!("{from}: {why}")),
        }),
        Hop::Via(p) => {
            let mut tcp = tcp_to(&p.host, p.port, deadline)?;
            tunnel::handshake(&mut tcp, p, host, port).map_err(|io| StageErr {
                stage: if io.kind() == io::ErrorKind::NotFound {
                    Stage::Resolve
                } else {
                    Stage::Proxy
                },
                io,
            })?;
            Ok(Opened {
                tcp,
                via_proxy: true,
            })
        }
    }
}

/// 真开 TCP 的那一处：先解析、再按解析出来的地址逐个连。
fn tcp_to(host: &str, port: u16, deadline: Option<Duration>) -> Result<TcpStream, StageErr> {
    // 测试档不许出网：真开 TCP 的那个目的地（代理或目标）不是本机回环 ⇒ 当场 panic，不连、不解析名字。
    #[cfg(any(test, feature = "test-support"))]
    assert!(
        upstream_url_core::upstream_is_loopback(host),
        "测试档不许出网：{host}"
    );
    let h = host.trim_start_matches('[').trim_end_matches(']');
    let addrs: Vec<std::net::SocketAddr> = (h, port)
        .to_socket_addrs()
        .map_err(|io| StageErr {
            stage: Stage::Resolve,
            io,
        })?
        .collect();
    if addrs.is_empty() {
        return Err(StageErr {
            stage: Stage::Resolve,
            io: io::Error::new(io::ErrorKind::NotFound, format!("{h}: no address")),
        });
    }
    #[allow(
        clippy::disallowed_methods,
        reason = "出网口：这台出站 TCP 只在这一处开，中转 · 拨号 · 探活都经 `open` 到这里"
    )]
    let tcp = TcpStream::connect(&addrs[..]).map_err(|io| StageErr {
        stage: Stage::Connect,
        io,
    })?;
    let set = |tcp: &TcpStream| -> io::Result<()> {
        tcp.set_read_timeout(deadline)?;
        tcp.set_write_timeout(deadline)
    };
    set(&tcp).map_err(|io| StageErr {
        stage: Stage::Connect,
        io,
    })?;
    Ok(tcp)
}

#[cfg(test)]
#[path = "../../../tests/backend/egress/lib_tests.rs"]
mod tests;

// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本 crate 的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;
