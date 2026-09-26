//! RT1 · 真机测试用的 **claude 替身**（Win11 虚拟机上没有真 claude，也没有账号）。
//! 不是 cargo target，独立交叉编译：
//!
//!     rustc -O --edition 2021 --target x86_64-pc-windows-gnu -o claude.exe tests/evidence/RT1-fake-claude.rs
//!
//! 守的要求：用户裁决 V115 逐字「能，用虚拟机」（虚拟机当真机测试资源）；
//! V120 逐字「真机测试量一次并发再开」（中转全量注入先在真机上量）。
//!
//! 它只模仿 monitor / 后端**看得见的那几样**（`设计/30`、后端 `observe/watcher.rs` 读的那几格），
//! 形状取自真 pidfile 的**键与类型**（值全是本程序现编的，不含任何真会话正文）：
//!   · `<配置目录>/sessions/<PID>.json`：`pid · sessionId · cwd · startedAt · kind · entrypoint · version · status · updatedAt`；
//!   · `<配置目录>/projects/<cwd 的 slug>/<sid>.jsonl`：两行合成的 user / assistant 记录；
//!   · 若进程环境里有 `ANTHROPIC_BASE_URL`（中转注入），按 `RT1_STREAMS`（缺省 3）条流
//!     往 `<base>/v1/messages` 发 POST，逐字节比对应答体与假上游按种子算出的期望（同一个
//!     生成器住 `RT1-relay-bench.rs`，两边各抄一份 —— 这里是**被测的消费方**那一份）。
//!   · 然后像一个 TUI 那样挂着读 stdin：敲 `/exit` 干净退出（删 pidfile）；窗口被关 ⇒
//!     控制台事件默认处理 ⇒ 退出码 `0xC000013A`，pidfile 留在盘上（与真 claude 被关窗时同形）。
//!
//! 每一步都写进 `RT1_LOG_DIR`（缺省 = 配置目录）下的 `fake-claude-<pid>.log`：argv · cwd ·
//! `CLAUDE_CONFIG_DIR` · `ANTHROPIC_BASE_URL` · 每条流的结局。令牌/钥匙不进日志（本程序也拿不到）。
use std::io::{BufRead, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn claude_dir() -> PathBuf {
    if let Ok(p) = std::env::var("CLAUDE_CONFIG_DIR") {
        if !p.trim().is_empty() {
            return PathBuf::from(p);
        }
    }
    // ⚠ 与真 claude 刻意不同：没给 `CLAUDE_CONFIG_DIR` 时**不**落到 `~\.claude`（那是用户真 profile，
    // V115 只许动临时目录），而落到台架的临时目录 —— 日志里 `CLAUDE_CONFIG_DIR=None` 那一行照实记下
    // 「这一层环境没传到」（Windows Terminal 新标签会从注册表重载环境，是要量的那件事）。
    let local = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(local).join("Temp").join("rt1").join("claude")
}

/// claude 的项目目录名：非字母数字一律换成 `-`。
fn slug(cwd: &str) -> String {
    cwd.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn mint_sid() -> String {
    let t = now_ms() as u64;
    let p = std::process::id() as u64;
    let a = t ^ (p << 32) ^ 0x9e37_79b9_7f4a_7c15;
    let b = a.wrapping_mul(0x5851_f42d_4c95_7f2d).rotate_left(17) ^ p;
    format!(
        "{:08x}-{:04x}-4{:03x}-8{:03x}-{:012x}",
        (a >> 32) as u32,
        (a >> 16) as u16,
        (a & 0xfff) as u16,
        (b & 0xfff) as u16,
        b & 0xffff_ffff_ffff
    )
}

struct Log(Option<std::fs::File>);
impl Log {
    fn line(&mut self, s: &str) {
        let l = format!("{} {}\n", now_ms(), s);
        if let Some(f) = self.0.as_mut() {
            let _ = f.write_all(l.as_bytes());
            let _ = f.flush();
        }
    }
}

// ── 与 RT1-relay-bench.rs 逐字同一个生成器（xorshift64*，种子 → 事件字节） ──
fn gen_body(seed: u64, events: usize) -> Vec<u8> {
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
        out.extend_from_slice(
            format!("event: content_block_delta\ndata: {{\"i\":{i},\"t\":\"{payload}\"}}\n\n")
                .as_bytes(),
        );
    }
    out
}

fn seed_of(tag: &str) -> u64 {
    // FNV-1a 64
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in tag.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// 解 `http://host:port/prefix` —— 本替身只会明文（中转在回环上）。
fn parse_base(base: &str) -> Option<(String, String)> {
    let rest = base.strip_prefix("http://")?;
    let (hp, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    Some((hp.to_string(), path.trim_end_matches('/').to_string()))
}

/// 读完一个 HTTP/1.1 应答：状态码 · 解帧后的体 · 首字节时刻 · 最大读间隔。
pub struct Got {
    pub status: u16,
    pub body: Vec<u8>,
    pub first_byte_ms: f64,
    pub max_gap_ms: f64,
    pub total_ms: f64,
}

pub fn read_response(mut s: TcpStream, t0: Instant) -> std::io::Result<Got> {
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
    let chunked = head
        .lines()
        .any(|l| l.to_ascii_lowercase().starts_with("transfer-encoding:") && l.to_ascii_lowercase().contains("chunked"));
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
        first_byte_ms: first.unwrap_or(f64::NAN),
        max_gap_ms: max_gap,
        total_ms: total,
    })
}

fn one_stream(base: &str, tag: &str, events: usize, log: &mut Log) -> bool {
    let Some((hp, prefix)) = parse_base(base) else {
        log.line(&format!("stream {tag}: ANTHROPIC_BASE_URL 不是 http:// 形，不发"));
        return false;
    };
    let t0 = Instant::now();
    let mut s = match TcpStream::connect(&hp) {
        Ok(s) => s,
        Err(e) => {
            log.line(&format!("stream {tag}: connect {hp} 失败：{e}"));
            return false;
        }
    };
    let _ = s.set_read_timeout(Some(Duration::from_secs(60)));
    let body = format!("{{\"model\":\"rt1-fake\",\"stream\":true,\"rt1_events\":{events}}}");
    let req = format!(
        "POST {prefix}/v1/messages HTTP/1.1\r\nHost: {hp}\r\nContent-Type: application/json\r\nAccept: text/event-stream\r\nx-rt1-seed: {tag}\r\nx-rt1-events: {events}\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    if let Err(e) = s.write_all(req.as_bytes()) {
        log.line(&format!("stream {tag}: 写请求失败：{e}"));
        return false;
    }
    match read_response(s, t0) {
        Ok(g) => {
            let want = gen_body(seed_of(tag), events);
            let same = g.body == want;
            log.line(&format!(
                "stream {tag}: status={} bytes={} want={} same={same} first_byte_ms={:.1} max_gap_ms={:.1} total_ms={:.1}",
                g.status,
                g.body.len(),
                want.len(),
                g.first_byte_ms,
                g.max_gap_ms,
                g.total_ms
            ));
            g.status == 200 && same
        }
        Err(e) => {
            log.line(&format!("stream {tag}: 读应答失败：{e}"));
            false
        }
    }
}

fn write_atomic(p: &Path, bytes: &[u8]) {
    let tmp = p.with_extension("json.tmp");
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(&tmp, p);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = claude_dir();
    let pid = std::process::id();
    let log_dir = std::env::var("RT1_LOG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dir.parent().map(|p| p.join("logs")).unwrap_or_else(|| dir.clone()));
    let _ = std::fs::create_dir_all(&log_dir);
    let mut log = Log(std::fs::File::create(log_dir.join(format!("fake-claude-{pid}.log"))).ok());
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    log.line(&format!("argv={:?}", &args[1..]));
    log.line(&format!("cwd={cwd}"));
    log.line(&format!(
        "CLAUDE_CONFIG_DIR={:?}",
        std::env::var("CLAUDE_CONFIG_DIR").ok()
    ));
    let base = std::env::var("ANTHROPIC_BASE_URL").ok();
    log.line(&format!("ANTHROPIC_BASE_URL={base:?}"));

    // 一次性子命令：`--version` / `-v`（monitor 探工具时会问）。
    if args.iter().any(|a| a == "--version" || a == "-v") {
        println!("2.1.999 (RT1 fake claude)");
        return;
    }
    let mut sid: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--resume" | "-r" | "--session-id" => {
                sid = args.get(i + 1).cloned();
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    let sid = sid.unwrap_or_else(mint_sid);
    log.line(&format!("sid={sid}"));

    let sessions = dir.join("sessions");
    let proj = dir.join("projects").join(slug(&cwd));
    let _ = std::fs::create_dir_all(&sessions);
    let _ = std::fs::create_dir_all(&proj);
    let started = now_ms();
    let pidfile = sessions.join(format!("{pid}.json"));
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let pj = format!(
        "{{\"pid\":{pid},\"sessionId\":\"{sid}\",\"cwd\":\"{}\",\"startedAt\":{started},\"version\":\"2.1.999\",\"kind\":\"interactive\",\"entrypoint\":\"cli\",\"status\":\"idle\",\"updatedAt\":{started}}}",
        esc(&cwd)
    );
    write_atomic(&pidfile, pj.as_bytes());
    log.line(&format!("pidfile={}", pidfile.display()));

    // 转写文件：合成的两行（结构同真记录的最小子集，正文是编的）。
    let tr = proj.join(format!("{sid}.jsonl"));
    let ts = |ms: u128| {
        let s = (ms / 1000) as i64;
        let (d, r) = (s.div_euclid(86400), s.rem_euclid(86400));
        // 1970-01-01 起的天数 → 公历（civil_from_days）
        let z = d + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let dd = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = if m <= 2 { y + 1 } else { y };
        format!(
            "{y:04}-{m:02}-{dd:02}T{:02}:{:02}:{:02}.{:03}Z",
            r / 3600,
            (r / 60) % 60,
            r % 60,
            ms % 1000
        )
    };
    let n_prior = std::fs::read_to_string(&tr)
        .map(|s| s.lines().count())
        .unwrap_or(0);
    let u = format!("{{\"type\":\"user\",\"sessionId\":\"{sid}\",\"uuid\":\"{sid}-u{n_prior}\",\"parentUuid\":null,\"cwd\":\"{}\",\"timestamp\":\"{}\",\"message\":{{\"role\":\"user\",\"content\":\"RT1 synthetic prompt #{n_prior} from pid {pid}\"}}}}\n", esc(&cwd), ts(started));
    let a = format!("{{\"type\":\"assistant\",\"sessionId\":\"{sid}\",\"uuid\":\"{sid}-a{n_prior}\",\"parentUuid\":\"{sid}-u{n_prior}\",\"cwd\":\"{}\",\"timestamp\":\"{}\",\"message\":{{\"id\":\"msg_rt1_{pid}_{n_prior}\",\"role\":\"assistant\",\"model\":\"rt1-fake\",\"content\":[{{\"type\":\"text\",\"text\":\"RT1 synthetic reply #{n_prior}\"}}]}}}}\n", esc(&cwd), ts(started + 1));
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&tr) {
        let _ = f.write_all(u.as_bytes());
        let _ = f.write_all(a.as_bytes());
    }
    log.line(&format!("transcript={}", tr.display()));

    println!("RT1 fake claude · pid {pid} · sid {sid}");
    println!("配置目录 {}", dir.display());
    match &base {
        Some(b) => {
            let n: usize = std::env::var("RT1_STREAMS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3);
            let ev: usize = std::env::var("RT1_EVENTS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(40);
            let mut ok = 0;
            for k in 0..n {
                if one_stream(b, &format!("{sid}#{k}"), ev, &mut log) {
                    ok += 1;
                }
            }
            log.line(&format!("streams ok={ok}/{n}"));
            println!("经 ANTHROPIC_BASE_URL 发了 {n} 条流，逐字节相等 {ok} 条");
        }
        None => println!("没有 ANTHROPIC_BASE_URL（直连，未注入中转）"),
    }
    // 台架用：`RT1_EXIT_AFTER=1` ⇒ 流发完就干净退出（并发几十个替身时不留一屋子挂着的进程）。
    if std::env::var("RT1_EXIT_AFTER").as_deref() == Ok("1") {
        let _ = std::fs::remove_file(&pidfile);
        log.line("RT1_EXIT_AFTER=1：流发完就退，pidfile 已删");
        return;
    }
    println!("敲 /exit 退出；直接关窗口 = 控制台事件（0xC000013A）");
    let _ = std::io::stdout().flush();
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim() == "/exit" {
            let _ = std::fs::remove_file(&pidfile);
            log.line("clean exit (/exit)，pidfile 已删");
            return;
        }
        println!("(RT1 替身不回话) {}", line.len());
    }
    // stdin 关了（不是控制台）：挂着，直到被杀。
    log.line("stdin EOF —— 挂着");
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}
