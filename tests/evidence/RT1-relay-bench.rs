//! 「几十个会话同时走中转，不卡不丢」的真机台架（Win11 虚拟机上跑）。
//! 不是 cargo target，独立交叉编译：
//!
//!     rustc -O --edition 2021 --target x86_64-pc-windows-gnu -o rt1-bench.exe tests/evidence/RT1-relay-bench.rs
//!
//! 守的要求：「真机测试量一次并发再开」——「中转全量注入（`CCM_RELAY_ALL_SESSIONS`）
//! 先在虚拟机真机测试里量『几十个会话同时走中转不卡不丢』，过了就默认开」；
//! 「必须带开关，默认关；真机验过再默认开」。
//!
//! 两个角色，一个 exe：
//!   · `rt1-bench upstream <port> <gap_ms>` —— 假上游：每条请求按请求头 `x-rt1-seed` / `x-rt1-events`
//!     算出一段确定的 SSE（`gen_body`），**一个事件一个 chunk、写完就 flush、隔 gap_ms 再写下一个**；
//!     每条请求结束在 stdout 打一行 `served seed=… bytes=… ms=…`。
//!   · `rt1-bench client <base> <N> <events> <tag>` —— N 条流**同时**起跑（屏障对齐），每条
//!     `POST <base-with-/{i}>/v1/messages`，读完整个应答后与**自己按同一种子算出的期望**逐字节比；
//!     打每条流一行 ＋ 一行汇总 JSON。
//!
//! 判据（相等，不用地板）：`status==200 且 body==期望` 的条数 == N（「不丢」）；
//! 「不卡」量两样：首字节时刻（缓冲了的话首字节会等到整条流结束 ≈ events×gap）与
//! 最大读间隔（逐块透传时应 ≈ gap；被攒一下就会远大于 gap）。门槛写在汇总里、由 `RT1-vm.py` 判。
//!
//! ⚠ 期望值生成器与 `RT1-fake-claude.rs` 里那份**逐字同一段**（两份都是被测方之外的第三方：
//! 一份在假上游手里产字节，一份在消费方手里验字节 —— 中转在中间，一个字节都不许改）。
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

fn gen_body(seed: u64, events: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for e in gen_events(seed, events) {
        out.extend_from_slice(&e);
    }
    out
}

fn gen_events(seed: u64, events: usize) -> Vec<Vec<u8>> {
    let mut x = seed ^ 0x2545_f491_4f6c_dd1d;
    if x == 0 {
        x = 1;
    }
    let mut out = Vec::new();
    for i in 0..events {
        let mut payload = String::new();
        let n = 40 + (i * 37) % 200;
        for _ in 0..n {
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            let v = x.wrapping_mul(0x2545_f491_4f6c_dd1d);
            payload.push(char::from(b'a' + (v % 26) as u8));
        }
        out.push(
            format!("event: content_block_delta\ndata: {{\"i\":{i},\"t\":\"{payload}\"}}\n\n")
                .into_bytes(),
        );
    }
    out
}

fn seed_of(tag: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in tag.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn serve_one(s: TcpStream, gap: Duration) {
    let t0 = Instant::now();
    let mut r = BufReader::new(s.try_clone().expect("clone"));
    let mut line = String::new();
    if r.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    let target = line.split_whitespace().nth(1).unwrap_or("").to_string();
    let mut seed_tag = String::new();
    let mut events = 40usize;
    let mut clen = 0usize;
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).unwrap_or(0) == 0 {
            break;
        }
        let t = h.trim_end();
        if t.is_empty() {
            break;
        }
        if let Some((k, v)) = t.split_once(':') {
            let k = k.trim().to_ascii_lowercase();
            let v = v.trim();
            match k.as_str() {
                "x-rt1-seed" => seed_tag = v.to_string(),
                "x-rt1-events" => events = v.parse().unwrap_or(40),
                "content-length" => clen = v.parse().unwrap_or(0),
                _ => {}
            }
        }
    }
    let mut body = vec![0u8; clen];
    let _ = r.read_exact(&mut body);
    let mut w = s;
    let _ = w.write_all(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nTransfer-Encoding: chunked\r\n\r\n",
    );
    let _ = w.flush();
    let mut sent = 0usize;
    for ev in gen_events(seed_of(&seed_tag), events) {
        let chunk = format!("{:x}\r\n", ev.len());
        if w.write_all(chunk.as_bytes()).is_err()
            || w.write_all(&ev).is_err()
            || w.write_all(b"\r\n").is_err()
        {
            println!("served-broken seed={seed_tag} target={target} sent={sent}");
            return;
        }
        let _ = w.flush();
        sent += ev.len();
        std::thread::sleep(gap);
    }
    let _ = w.write_all(b"0\r\n\r\n");
    let _ = w.flush();
    println!(
        "served seed={seed_tag} target={target} bytes={sent} ms={}",
        t0.elapsed().as_millis()
    );
    let _ = std::io::stdout().flush();
}

struct Got {
    status: u16,
    body: Vec<u8>,
    first_byte_ms: f64,
    max_gap_ms: f64,
    total_ms: f64,
}

fn read_response(mut s: TcpStream, t0: Instant) -> std::io::Result<Got> {
    let mut raw = Vec::new();
    let mut buf = [0u8; 16384];
    let mut first: Option<f64> = None;
    let mut last = Instant::now();
    let mut max_gap = 0f64;
    loop {
        let n = s.read(&mut buf)?;
        let now = Instant::now();
        if n == 0 {
            break;
        }
        if first.is_none() {
            first = Some(now.duration_since(t0).as_secs_f64() * 1000.0);
        } else {
            let g = now.duration_since(last).as_secs_f64() * 1000.0;
            if g > max_gap {
                max_gap = g;
            }
        }
        last = now;
        raw.extend_from_slice(&buf[..n]);
    }
    let total = t0.elapsed().as_secs_f64() * 1000.0;
    let hend = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| std::io::Error::other("no header end"))?;
    let head = String::from_utf8_lossy(&raw[..hend]).to_string();
    let status: u16 = head
        .split_whitespace()
        .nth(1)
        .and_then(|x| x.parse().ok())
        .unwrap_or(0);
    let chunked = head.lines().any(|l| {
        let l = l.to_ascii_lowercase();
        l.starts_with("transfer-encoding:") && l.contains("chunked")
    });
    let rest = &raw[hend + 4..];
    let body = if chunked {
        let mut out = Vec::new();
        let mut i = 0;
        loop {
            let Some(le) = rest[i..].windows(2).position(|w| w == b"\r\n") else {
                break;
            };
            let sz_s = String::from_utf8_lossy(&rest[i..i + le]).to_string();
            let sz = usize::from_str_radix(sz_s.split(';').next().unwrap_or("").trim(), 16)
                .unwrap_or(0);
            i += le + 2;
            if sz == 0 {
                break;
            }
            if i + sz > rest.len() {
                out.extend_from_slice(&rest[i..]);
                break;
            }
            out.extend_from_slice(&rest[i..i + sz]);
            i += sz + 2;
        }
        out
    } else {
        rest.to_vec()
    };
    Ok(Got {
        status,
        body,
        first_byte_ms: first.unwrap_or(-1.0),
        max_gap_ms: max_gap,
        total_ms: total,
    })
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("upstream") => {
            let port: u16 = a[2].parse().expect("port");
            let gap = Duration::from_millis(a[3].parse().expect("gap_ms"));
            let l = TcpListener::bind(("127.0.0.1", port)).expect("bind");
            println!("upstream listening 127.0.0.1:{}", l.local_addr().unwrap().port());
            let _ = std::io::stdout().flush();
            for c in l.incoming().flatten() {
                std::thread::spawn(move || serve_one(c, gap));
            }
        }
        Some("client") => {
            let base = a[2].trim_end_matches('/').to_string();
            let n: usize = a[3].parse().expect("N");
            let events: usize = a[4].parse().expect("events");
            let tag = a[5].clone();
            let rest = base.strip_prefix("http://").expect("http://");
            let (hp, prefix) = match rest.find('/') {
                Some(i) => (rest[..i].to_string(), rest[i..].to_string()),
                None => (rest.to_string(), String::new()),
            };
            let bar = Arc::new(Barrier::new(n));
            let mut hs = Vec::new();
            for i in 0..n {
                let bar = Arc::clone(&bar);
                let hp = hp.clone();
                let prefix = prefix.clone();
                let seed_tag = format!("{tag}-{i}");
                hs.push(std::thread::spawn(move || {
                    bar.wait();
                    let t0 = Instant::now();
                    let res = (|| -> std::io::Result<Got> {
                        let mut s = TcpStream::connect(&hp)?;
                        s.set_read_timeout(Some(Duration::from_secs(120)))?;
                        let body = format!("{{\"model\":\"rt1\",\"stream\":true,\"i\":{i}}}");
                        // 每条流一个会话 id 段（`/t/<agent>/<label>/<sid>` 的末段）：替换 base 末段里的 `{i}`。
                        let p = prefix.replace("{i}", &i.to_string());
                        let req = format!(
                            "POST {p}/v1/messages HTTP/1.1\r\nHost: {hp}\r\nContent-Type: application/json\r\nx-rt1-seed: {seed_tag}\r\nx-rt1-events: {events}\r\nContent-Length: {}\r\n\r\n{body}",
                            body.len()
                        );
                        s.write_all(req.as_bytes())?;
                        read_response(s, t0)
                    })();
                    let want = gen_body(seed_of(&seed_tag), events);
                    match res {
                        Ok(g) => (i, g.status, g.body == want, g.body.len(), want.len(), g.first_byte_ms, g.max_gap_ms, g.total_ms, String::new()),
                        Err(e) => (i, 0, false, 0, want.len(), -1.0, -1.0, t0.elapsed().as_secs_f64() * 1000.0, e.to_string()),
                    }
                }));
            }
            let mut rows = Vec::new();
            for h in hs {
                rows.push(h.join().expect("join"));
            }
            rows.sort_by_key(|r| r.0);
            let mut ok = 0;
            let mut fb: Vec<f64> = Vec::new();
            let mut gaps: Vec<f64> = Vec::new();
            let mut tot: Vec<f64> = Vec::new();
            let mut bytes = 0usize;
            for r in &rows {
                println!(
                    "stream i={} status={} same={} bytes={} want={} first_byte_ms={:.1} max_gap_ms={:.1} total_ms={:.1} err={}",
                    r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8
                );
                if r.1 == 200 && r.2 {
                    ok += 1;
                }
                bytes += r.3;
                fb.push(r.5);
                gaps.push(r.6);
                tot.push(r.7);
            }
            let pct = |v: &mut Vec<f64>, p: f64| -> f64 {
                v.sort_by(|a, b| a.partial_cmp(b).unwrap());
                if v.is_empty() {
                    return -1.0;
                }
                let k = ((v.len() as f64 - 1.0) * p).round() as usize;
                v[k]
            };
            println!(
                "SUMMARY {{\"tag\":\"{tag}\",\"n\":{n},\"events\":{events},\"ok\":{ok},\"bytes\":{bytes},\"first_byte_p50\":{:.1},\"first_byte_max\":{:.1},\"max_gap_p50\":{:.1},\"max_gap_max\":{:.1},\"total_p50\":{:.1},\"total_max\":{:.1}}}",
                pct(&mut fb.clone(), 0.5),
                pct(&mut fb, 1.0),
                pct(&mut gaps.clone(), 0.5),
                pct(&mut gaps, 1.0),
                pct(&mut tot.clone(), 0.5),
                pct(&mut tot, 1.0)
            );
        }
        _ => {
            eprintln!("usage: rt1-bench upstream <port> <gap_ms> | client <base> <N> <events> <tag>");
            std::process::exit(2);
        }
    }
}
