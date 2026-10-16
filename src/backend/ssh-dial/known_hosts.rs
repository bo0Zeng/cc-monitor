//! **信哪把主机钥匙，规矩只此一处**（读 · 记 · 忘）：一份 OpenSSH known_hosts 的文本，后端与手机同一份规矩。
//! 文件在哪、怎么锁、怎么落盘是宿主的事（[`crate::env::HostKeys`]）；这里只有纯函数。
//!
//! 一台机器一行（记过几种类型就几行）：主机名那一段写这台的**全部地址**（逗号相连，口不是 22 的写 `[地址]:口`），
//! 后面 `算法 base64`。主机名段里含这台任一地址的行都算这台的（终端里的 `ssh` 认同一份）。

use russh::keys::{HashAlg, PublicKey};

use crate::Endpoint;

/// OpenSSH 认的主机名写法：口 22 只写主机，别的口写 `[主机]:口`（IPv6 字面量的方括号先剥掉）。
pub fn host_pattern(host: &str, port: u16) -> String {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if port == 22 {
        bare.to_string()
    } else {
        format!("[{bare}]:{port}")
    }
}

/// 这台全部地址的主机名段（去重保序，逗号相连）。
pub fn names(addrs: &[Endpoint]) -> String {
    let mut out: Vec<String> = Vec::new();
    for a in addrs {
        let p = host_pattern(&a.host, a.port);
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out.join(",")
}

/// 主机名段（逗号分开）里有这台任一地址。
fn names_hit(field: &str, addrs: &[Endpoint]) -> bool {
    field
        .split(',')
        .any(|n| addrs.iter().any(|a| host_pattern(&a.host, a.port) == n))
}

/// 这一行是这台的一把钥匙。注释 · 标记行（`@…`）· 散列过的主机名不认。
fn is_of(line: &str, addrs: &[Endpoint]) -> bool {
    match line.split_whitespace().next() {
        Some(f) if !f.starts_with(['#', '@', '|']) => names_hit(f, addrs),
        _ => false,
    }
}

/// 这台记过的钥匙（认不出的行丢掉）。
pub fn known(text: &str, addrs: &[Endpoint]) -> Vec<PublicKey> {
    text.lines()
        .filter(|l| is_of(l, addrs))
        .filter_map(|l| {
            let key: Vec<&str> = l.split_whitespace().skip(1).take(2).collect();
            PublicKey::from_openssh(&key.join(" ")).ok()
        })
        .collect()
}

/// 留下 `keep` 认的那几行（空行去掉）⇒ 新文本。
fn kept(text: &str, keep: impl Fn(&str) -> bool) -> Vec<&str> {
    text.lines()
        .filter(|l| !l.trim().is_empty() && keep(l))
        .collect()
}

fn joined(lines: &[&str]) -> String {
    if lines.is_empty() {
        String::new()
    } else {
        lines.join("\n") + "\n"
    }
}

/// 首见：这台一行都没有 ⇒ 追加一行（`key` = 算法 ＋ base64），顺手去掉这台待核的指纹；有行 ⇒ 不动（`None`）。
/// 记过的都在握手时严格比过了，走到这一步就是那一把，没有「换掉旧行」这件事。
pub fn remembered(text: &str, addrs: &[Endpoint], key: &str) -> Option<String> {
    if text.lines().any(|l| is_of(l, addrs)) {
        return None;
    }
    let line = format!("{} {key}", names(addrs));
    let mut lines = kept(text, |l| !is_pending_of(l, addrs));
    lines.push(&line);
    Some(joined(&lines))
}

/// 忘掉这台：删这台的所有行（连同待核的指纹）。没有可删的 ⇒ `None`。
pub fn forgotten(text: &str, addrs: &[Endpoint]) -> Option<String> {
    let lines = kept(text, |l| !is_of(l, addrs) && !is_pending_of(l, addrs));
    (lines.len() != kept(text, |_| true).len()).then(|| joined(&lines))
}

/// 一把钥匙的指纹（`SHA256:…`）。
pub fn fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

// ═══ 待核的指纹：桌面机器表那一格换形过来的，只活这一版（下一版连同 `connect.rs` 里用它的那一支删）═══
// 机器表里固化的只是指纹、还原不出钥匙 ⇒ 写成一行注释（ssh 不认它）；这台下回连上按它严格比，鉴权过了记整把、这一行随之去掉。

const PENDING: &str = "#cc-monitor-pending-fingerprint";

fn is_pending_of(line: &str, addrs: &[Endpoint]) -> bool {
    let mut f = line.split_whitespace();
    f.next() == Some(PENDING) && f.next().is_some_and(|n| names_hit(n, addrs))
}

/// 这台待核的指纹（没有 ⇒ `None`）。
pub fn pending(text: &str, addrs: &[Endpoint]) -> Option<String> {
    text.lines()
        .find(|l| is_pending_of(l, addrs))
        .and_then(|l| l.split_whitespace().nth(2))
        .map(str::to_string)
}

/// 给这台写一行待核的指纹（已有 ⇒ `None`）。
pub fn with_pending(text: &str, addrs: &[Endpoint], fp: &str) -> Option<String> {
    if pending(text, addrs).is_some() {
        return None;
    }
    let line = format!("{PENDING} {} {}", names(addrs), fp.trim());
    let mut lines = kept(text, |_| true);
    lines.push(&line);
    Some(joined(&lines))
}

#[cfg(test)]
#[path = "../../../tests/backend/ssh_dial_known_hosts_tests.rs"]
mod tests;
