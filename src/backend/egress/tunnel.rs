//! 经代理那两种握手：HTTP `CONNECT` 与 SOCKS5（RFC 1928 · 用户名口令 RFC 1929）。握手完了这条 TCP 就是到目的地的一条透明隧道。
//!
//! 握手读回包按字节读、不多读：回包后面紧跟的就是目的地的字节（TLS 的 ServerHello · SSH 的版本行），多读一个就吞掉了。

use std::io::{self, Read, Write};
use std::net::{IpAddr, TcpStream, ToSocketAddrs};

use crate::describe::{Kind, Proxy};

/// 代理那一跳的回包头最长读这么多（超了当代理不对）。
const HEAD_CAP: usize = 16 * 1024;

pub(crate) fn handshake(tcp: &mut TcpStream, p: &Proxy, host: &str, port: u16) -> io::Result<()> {
    match p.kind {
        Kind::Http => http_connect(tcp, p, host, port),
        Kind::Socks5Remote => socks5(tcp, p, Target::Name(host), port),
        Kind::Socks5Local => {
            let ip = resolve_here(host, port)?;
            socks5(tcp, p, Target::Ip(ip), port)
        }
    }
}

fn refused(what: String) -> io::Error {
    io::Error::new(io::ErrorKind::ConnectionRefused, what)
}

/// `主机:端口`，IPv6 加方括号。
fn authority(host: &str, port: u16) -> String {
    let h = host.trim_start_matches('[').trim_end_matches(']');
    if h.contains(':') {
        format!("[{h}]:{port}")
    } else {
        format!("{h}:{port}")
    }
}

fn http_connect(tcp: &mut TcpStream, p: &Proxy, host: &str, port: u16) -> io::Result<()> {
    let at = authority(host, port);
    let mut head = format!("CONNECT {at} HTTP/1.1\r\nHost: {at}\r\n");
    if let Some((u, pw)) = &p.auth {
        head.push_str(&format!(
            "Proxy-Authorization: Basic {}\r\n",
            base64(format!("{u}:{pw}").as_bytes())
        ));
    }
    head.push_str("\r\n");
    tcp.write_all(head.as_bytes())?;
    tcp.flush()?;
    let mut got = Vec::new();
    let mut b = [0u8; 1];
    while !got.ends_with(b"\r\n\r\n") {
        if got.len() >= HEAD_CAP {
            return Err(refused("proxy answer head too large".to_string()));
        }
        match tcp.read(&mut b) {
            Ok(0) => return Err(refused("proxy closed during CONNECT".to_string())),
            Ok(_) => got.push(b[0]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    let line = got.split(|c| *c == b'\r').next().unwrap_or_default();
    let line = String::from_utf8_lossy(line);
    let status = line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok());
    match status {
        Some(s) if (200..300).contains(&s) => Ok(()),
        _ => Err(refused(format!("proxy CONNECT {at}: {}", line.trim()))),
    }
}

enum Target<'a> {
    Name(&'a str),
    Ip(IpAddr),
}

fn resolve_here(host: &str, port: u16) -> io::Result<IpAddr> {
    let h = host.trim_start_matches('[').trim_end_matches(']');
    (h, port)
        .to_socket_addrs()?
        .next()
        .map(|a| a.ip())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("no address for {h}")))
}

fn read_exact(tcp: &mut TcpStream, n: usize) -> io::Result<Vec<u8>> {
    let mut v = vec![0u8; n];
    tcp.read_exact(&mut v).map_err(|e| {
        if e.kind() == io::ErrorKind::UnexpectedEof {
            refused("proxy closed during SOCKS5 handshake".to_string())
        } else {
            e
        }
    })?;
    Ok(v)
}

fn socks5(tcp: &mut TcpStream, p: &Proxy, target: Target<'_>, port: u16) -> io::Result<()> {
    // 问候：带鉴权就同时报「不要鉴权」与「用户名口令」两种，由代理挑。
    let methods: &[u8] = if p.auth.is_some() { &[0x00, 0x02] } else { &[0x00] };
    let mut hello = vec![0x05, methods.len() as u8];
    hello.extend_from_slice(methods);
    tcp.write_all(&hello)?;
    let pick = read_exact(tcp, 2)?;
    if pick[0] != 0x05 {
        return Err(refused("not a SOCKS5 proxy".to_string()));
    }
    match (pick[1], &p.auth) {
        (0x00, _) => {}
        (0x02, Some((u, pw))) => {
            if u.len() > 255 || pw.len() > 255 {
                return Err(refused("SOCKS5 credentials too long".to_string()));
            }
            let mut a = vec![0x01, u.len() as u8];
            a.extend_from_slice(u.as_bytes());
            a.push(pw.len() as u8);
            a.extend_from_slice(pw.as_bytes());
            tcp.write_all(&a)?;
            let ok = read_exact(tcp, 2)?;
            if ok[1] != 0x00 {
                return Err(refused("SOCKS5 proxy rejected the credentials".to_string()));
            }
        }
        (m, _) => return Err(refused(format!("SOCKS5 proxy offers no usable method ({m:#04x})"))),
    }
    let mut req = vec![0x05, 0x01, 0x00];
    match target {
        Target::Ip(IpAddr::V4(a)) => {
            req.push(0x01);
            req.extend_from_slice(&a.octets());
        }
        Target::Ip(IpAddr::V6(a)) => {
            req.push(0x04);
            req.extend_from_slice(&a.octets());
        }
        Target::Name(h) => {
            let h = h.trim_start_matches('[').trim_end_matches(']');
            match h.parse::<IpAddr>() {
                Ok(IpAddr::V4(a)) => {
                    req.push(0x01);
                    req.extend_from_slice(&a.octets());
                }
                Ok(IpAddr::V6(a)) => {
                    req.push(0x04);
                    req.extend_from_slice(&a.octets());
                }
                Err(_) => {
                    if h.len() > 255 {
                        return Err(refused("host name too long for SOCKS5".to_string()));
                    }
                    req.push(0x03);
                    req.push(h.len() as u8);
                    req.extend_from_slice(h.as_bytes());
                }
            }
        }
    }
    req.extend_from_slice(&port.to_be_bytes());
    tcp.write_all(&req)?;
    let rep = read_exact(tcp, 4)?;
    if rep[1] != 0x00 {
        return Err(refused(format!("SOCKS5 proxy refused the connection (reply {})", rep[1])));
    }
    // 回包里代理那一端的绑定地址：读掉，不用。
    let rest = match rep[3] {
        0x01 => 4,
        0x04 => 16,
        0x03 => usize::from(read_exact(tcp, 1)?[0]),
        _ => return Err(refused("SOCKS5 reply has an unknown address type".to_string())),
    };
    read_exact(tcp, rest + 2)?;
    Ok(())
}

/// 标准 base64（带补位），只给 `Proxy-Authorization: Basic` 用。
pub(crate) fn base64(b: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}
