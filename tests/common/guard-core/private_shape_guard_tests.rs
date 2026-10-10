//! 仓里不留私人标识（用户原话：「发版不要把我的账号发出去 / 配置也不要发出去」）。
//! 产品仓是公开的，每个发版还附整仓源码包 ⇒ 仓里的就是发出去的。
//!
//! 本条只认**形状**，不点名：
//! ① 家目录 `/home/<名>`、`/Users/<名>`、`C:\Users\<名>` —— 名字只许是中性的（[`NEUTRAL_NAMES`] 或单个字母）；
//! ② 私网 IPv4（`10/8` · `172.16/12` · `192.168/16` · `100.64/10`）—— 例子一律用文档段（`192.0.2.x` · `198.51.100.x` · `203.0.113.x`）；
//! ③ `src/` 下的记忆链接 `[[小写-连字符-名字]]`；
//! ④ 邮箱 —— 域名只许是保留给例子的（[`EXAMPLE_DOMAINS`] 或保留顶级域）。
//!
//! 人群：`git ls-files` 的全部文本文件，跳过锁文件、第三方许可汇编、`src/vendor/`（别人的代码，原样收）、
//! `src/mobile/libs/LICENSES/`（随仓放进来的第三方二进制的许可全文，原样收，里面是原作者的署名）。
//! 点名的那份清单不在仓里：它住在门禁外面，由发版的人另跑。
//!
//! 买不到：不落在这几种形状上的私人信息（机器名、账号名本身没有形状可认）。

use regex::Regex;
use std::path::{Path, PathBuf};

/// 家目录里可以出现的名字（不分大小写）。单个字母另算，一律放过。
const NEUTRAL_NAMES: &[&str] = &[
    "user", "user2", "u2", "uu", "me", "you", "dev", "pi", "runner", "tester", "someone",
    "somebody", "other", "alice", "bob", "rig",
];

/// 邮箱域名：这几个，或以保留顶级域结尾的（见 [`RESERVED_TLDS`]）。
/// `openssh.com` 是 SSH 扩展名（`zlib@openssh.com` 这种），不是谁的邮箱。
const EXAMPLE_DOMAINS: &[&str] = &["example.com", "example.org", "example.net", "openssh.com"];
const RESERVED_TLDS: &[&str] = &["test", "invalid", "example", "local", "localhost"];
/// 形如 `128x128@2x.png` 的是图标文件名，不是邮箱。
const FILE_EXTS: &[&str] = &["png", "svg", "jpg", "jpeg", "ico", "webp", "gif"];

const LOCK_FILES: &[&str] = &["Cargo.lock", "package-lock.json"];
const SKIP_FILES: &[&str] = &["THIRD-PARTY-NOTICES.txt"];
const SKIP_DIRS: &[&str] = &["src/vendor/", "src/mobile/libs/LICENSES/"];

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("guard-core 的上三级 = 仓根")
}

fn tracked_texts(root: &Path) -> Vec<(String, String)> {
    let out = std::process::Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root)
        .output()
        .expect("跑不动 `git ls-files`");
    assert!(out.status.success(), "`git ls-files` 非零退出");
    let mut texts = Vec::new();
    for rel in String::from_utf8_lossy(&out.stdout).split('\0') {
        let name = rel.rsplit('/').next().unwrap_or(rel);
        if rel.is_empty()
            || LOCK_FILES.contains(&name)
            || SKIP_FILES.contains(&rel)
            || SKIP_DIRS.iter().any(|d| rel.starts_with(d))
        {
            continue;
        }
        let p = root.join(rel);
        let meta = match std::fs::symlink_metadata(&p) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            other => other.unwrap_or_else(|e| panic!("跟踪着的 {rel} 读不动：{e}")),
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        let bytes = std::fs::read(&p).unwrap_or_else(|e| panic!("跟踪着的 {rel} 读不动：{e}"));
        if bytes.iter().take(8000).any(|&b| b == 0) {
            continue;
        }
        texts.push((
            rel.to_string(),
            String::from_utf8_lossy(&bytes).into_owned(),
        ));
    }
    texts
}

/// 紧挨在前面就说明这是拼出来的子路径（`{d}/home/x` · `$W/home/x` · `<OUT>/home/x`），不是谁的家。
fn glued(line: &str, at: usize) -> bool {
    line[..at]
        .chars()
        .next_back()
        .is_some_and(|c| c.is_ascii_alphanumeric() || "_}$>)./-".contains(c))
}

fn home_rx() -> Regex {
    Regex::new(
        r"(/home/|/[A-Za-z]/Users/|/Users/|[A-Za-z]:(?:\\+|/)(?i:users)(?:\\+|/))([A-Za-z0-9_.-]+)",
    )
    .unwrap()
}

/// ① 家目录里不中性的名字。
fn bad_homes(line: &str, rx: &Regex) -> Vec<String> {
    let mut out = Vec::new();
    for c in rx.captures_iter(line) {
        let whole = c.get(0).unwrap();
        // `/c/Users/…` 里的 `/Users/` 那一支不算第二次。
        if glued(line, whole.start()) {
            continue;
        }
        let name = c[2].trim_end_matches(['.', '-']);
        if name.is_empty() || name.starts_with('.') {
            continue;
        }
        let neutral = (name.len() == 1 && name.chars().all(|c| c.is_ascii_alphabetic()))
            || NEUTRAL_NAMES.iter().any(|n| n.eq_ignore_ascii_case(name));
        if !neutral {
            out.push(whole.as_str().to_string());
        }
    }
    out
}

fn ip_rx() -> Regex {
    Regex::new(r"(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})").unwrap()
}

/// ② 私网 IPv4。
fn private_ips(line: &str, rx: &Regex) -> Vec<String> {
    let mut out = Vec::new();
    for c in rx.captures_iter(line) {
        let m = c.get(0).unwrap();
        let before = line[..m.start()].chars().next_back();
        let after: Vec<char> = line[m.end()..].chars().take(2).collect();
        if before.is_some_and(|b| b.is_ascii_digit() || b == '.')
            || after.first().is_some_and(|a| a.is_ascii_digit())
            || (after.first() == Some(&'.') && after.get(1).is_some_and(|a| a.is_ascii_digit()))
        {
            continue;
        }
        let o: Vec<u32> = (1..=4).map(|i| c[i].parse().unwrap()).collect();
        if o.iter().any(|&x| x > 255) {
            continue;
        }
        let private = o[0] == 10
            || (o[0] == 172 && (16..=31).contains(&o[1]))
            || (o[0] == 192 && o[1] == 168)
            || (o[0] == 100 && (64..=127).contains(&o[1]));
        if private {
            out.push(m.as_str().to_string());
        }
    }
    out
}

fn memo_rx() -> Regex {
    Regex::new(r"\[\[[a-z0-9]+(?:-[a-z0-9]+)+\]\]").unwrap()
}

fn mail_rx() -> Regex {
    Regex::new(r"[A-Za-z0-9._%+-]+@([A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.([A-Za-z]{2,}))").unwrap()
}

/// ④ 不是例子域名的邮箱。
fn real_mails(line: &str, rx: &Regex) -> Vec<String> {
    let mut out = Vec::new();
    for c in rx.captures_iter(line) {
        let domain = c[1].to_ascii_lowercase();
        let tld = c[2].to_ascii_lowercase();
        let ok = RESERVED_TLDS.contains(&tld.as_str())
            || FILE_EXTS.contains(&tld.as_str())
            || EXAMPLE_DOMAINS
                .iter()
                .any(|d| domain == *d || domain.ends_with(&format!(".{d}")));
        if !ok {
            out.push(c[0].to_string());
        }
    }
    out
}

/// 四张网，编一次用到底。
struct Nets {
    home: Regex,
    ip: Regex,
    memo: Regex,
    mail: Regex,
}

impl Nets {
    fn new() -> Self {
        Nets {
            home: home_rx(),
            ip: ip_rx(),
            memo: memo_rx(),
            mail: mail_rx(),
        }
    }

    /// 一行里四种形状的命中（`in_src` ⇒ 记忆链接那一眼也看）。
    fn hits(&self, line: &str, in_src: bool) -> Vec<String> {
        let mut out = bad_homes(line, &self.home);
        out.extend(private_ips(line, &self.ip));
        if in_src {
            out.extend(self.memo.find_iter(line).map(|x| x.as_str().to_string()));
        }
        out.extend(real_mails(line, &self.mail));
        out
    }
}

/// 正控：每一眼在合成夹具上都抓得到、该放过的放过。夹具在运行期拼，本文件自己不撞网。
#[test]
fn the_private_shape_net_catches_each_shape_and_spares_the_neutral_ones() {
    let nets = Nets::new();
    let hits = |l: &str, src: bool| nets.hits(l, src);
    let name = ["jd", "oe"].concat();
    let caught = [
        format!("cd /home/{name}/x"),
        format!(r"C:\Users\{name}\.cc-monitor"),
        format!(r#""C:\\Users\\{name}\\a""#),
        format!("/c/Users/{name}/bin"),
        format!("/Users/{name}/Library"),
        format!("ssh u@{}.{}.0.5", 10, 0),
        format!("host {}.{}.3.4", 192, 168),
        format!("{}.{}.0.1:22", 172, 20),
        format!("{}.{}.1.2", 100, 92),
        format!("见 [[{}-{}]]", "some", "memory"),
        format!("{name}@{}.{}", "uni", "edu"),
    ];
    for l in &caught {
        assert!(!hits(l, true).is_empty(), "该抓没抓：{l}");
    }
    let spared = [
        "cd /home/user/x".to_string(),
        r"C:\Users\u\.cc-monitor".to_string(),
        "/home/pi/.ssh".to_string(),
        "{d}/home/docs/sub".to_string(),
        "$W/home/projects".to_string(),
        "/home/z/p".to_string(),
        format!("{}.0.2.9 · {}.51.100.1 · {}.0.113.7", 192, 198, 203),
        "10.0.19041.1 · 1.10.0.0.5".to_string(),
        "[[package]] · [[hooks.Stop]] · [[:space:]]".to_string(),
        format!(
            "a@{} · zlib@{} · 128x128@2x.png · b@x.test",
            "example.org", "openssh.com"
        ),
    ];
    for l in &spared {
        assert!(
            hits(l, true).is_empty(),
            "该放过没放过：{l} ⇒ {:?}",
            hits(l, true)
        );
    }
    assert!(
        hits(&format!("[[{}-{}]]", "a", "b"), false).is_empty(),
        "记忆链接那一眼只看 src/"
    );
}

#[test]
fn the_repo_carries_no_private_shapes() {
    let texts = tracked_texts(&repo());
    assert!(
        texts.len() > 500,
        "人群只有 {} 份 —— `git ls-files` 口径坏了",
        texts.len()
    );
    let nets = Nets::new();
    let mut found = Vec::new();
    for (rel, text) in &texts {
        let in_src = rel.starts_with("src/");
        for (n, line) in text.lines().enumerate() {
            for h in nets.hits(line, in_src) {
                found.push(format!("{rel}:{}: {h}", n + 1));
            }
        }
    }
    assert!(
        found.is_empty(),
        "仓里有私人标识的形状（家目录名 · 私网地址 · 记忆链接 · 邮箱），{} 处：\n{}",
        found.len(),
        found.join("\n")
    );
}
