//! 假 agent 的上游面里报用量那一形（`UsageFace`）：与 Claude 每一格都不同形 —— Claude 是起来就吐、stdin 不喂、等它退；
//! 这一家要往 stdin 喂两行、它一行一行回显、读到第二行的回显就够了（它自己不退，等着下一行）。
//!
//! 程序是测试档里的一个回显小程序（[`ECHO_PROGRAM`]，判据把它放上 `PATH`、名字是 [`PROGRAM`]）：
//! 每读进一行，先把原样那一行记进自己旁边的 `fed.txt`，再在标准输出回一行 `echo: <那一行>`；读到 stdin 头就退。

use super::ACCOUNT_DIR_ENV;
use crate::agents::{QuotaWindow, UsageFace};

/// 报用量的那个程序名（判据照这个名字把 [`ECHO_PROGRAM`] 放上 `PATH`）。
pub const PROGRAM: &str = "fakeagent-meter";

/// 回显 stdin 的那个小程序（POSIX sh）。
pub const ECHO_PROGRAM: &str = r#"#!/bin/sh
d=$(dirname "$0")
: > "$d/fed.txt"
while IFS= read -r l; do
  printf '%s\n' "$l" >> "$d/fed.txt"
  printf 'echo: %s\n' "$l"
done
"#;

/// 第二行：要哪两窗、各用了多少、几秒后重置（`<窗>=<已用比例>@+<秒>`）。
const REPORT: &str = "meter burst=0.25@+3600 long=0.5@+86400";

/// 回显里认的那一截：`echo: meter ` 起头的那一行就是第二行的回显。
const REPORT_ECHO: &str = "echo: meter ";

/// 注册表 `DefaultUpstream.usage` 那一格（这一家不进 `REGISTRY`，判据直接拿它交通用层）。
pub const USAGE: UsageFace = UsageFace {
    program: PROGRAM,
    args: &["--meter", "lines"],
    dir_env: ACCOUNT_DIR_ENV,
    env_remove: &["FAKEAGENT_TOKEN"],
    env_set: &[("FAKEAGENT_CLOCK", "unix")],
    stdin: &["hello meter", REPORT],
    enough: Some(enough),
    read,
};

/// `UsageFace.enough` 那一格：第二行的回显到了就够了。
pub fn enough(line: &str) -> bool {
    line.starts_with(REPORT_ECHO)
}

/// `UsageFace.read` 那一格：第二行的回显 → 两窗（按出现的次序）。没有那一行 · 哪一截对不上 ⇒ 说出是哪一截。
pub fn read(out: &str, now: u64) -> Result<Vec<QuotaWindow>, String> {
    let line = out
        .lines()
        .find_map(|l| l.strip_prefix(REPORT_ECHO))
        .ok_or_else(|| format!("no `{}` line", REPORT_ECHO.trim_end()))?;
    line.split_whitespace()
        .map(|cell| {
            let bad = || format!("unreadable cell `{cell}`");
            let (name, rest) = cell.split_once('=').ok_or_else(bad)?;
            let (used, secs) = rest.split_once("@+").ok_or_else(bad)?;
            let used: f64 = used.parse().map_err(|_| bad())?;
            let secs: u64 = secs.parse().map_err(|_| bad())?;
            Ok(QuotaWindow {
                name: name.to_string(),
                used: Some(used),
                resets_at: Some(now + secs),
                warned_at: None,
            })
        })
        .collect()
}

/// 换号时写的两个头（头名与 Claude · Codex 都不同）：访问令牌带一个前缀进 `X-Fake-Pass`；凭据文件那一节的 `org` 原样进 `X-Fake-Org`。
pub const SWAP_HEADS: &[crate::agents::SwapHead] = &[
    crate::agents::SwapHead {
        name: "X-Fake-Pass",
        prefix: "Seat ",
        cell: crate::agents::CredCell::Access,
    },
    crate::agents::SwapHead {
        name: "X-Fake-Org",
        prefix: "",
        cell: crate::agents::CredCell::Field("org"),
    },
];

/// 用满回包：418 ＋ 一个自己的头 ＋ 一行纯文本（与 Claude 那份每一格都不同形）。
pub fn limit_reply(reset_at: u64, _now: u64, _key: Option<&str>) -> crate::agents::LimitReply {
    crate::agents::LimitReply {
        status: "418 Spent",
        headers: vec![("x-fake-until".to_string(), reset_at.to_string())],
        body: format!("spent until {reset_at}\n").into_bytes(),
    }
}

/// 本假 agent 的默认上游那一格（不进 `REGISTRY`：判据直接拿它交通用层）。只填判据用得到的几格，别的一律「没有」。
pub const UPSTREAM: crate::agents::DefaultUpstream = crate::agents::DefaultUpstream {
    route_id: "fake-route",
    env: "CCM_AGENT_UPSTREAM_FAKE",
    fallback: crate::agents::Fallback::One("https://fake.invalid"),
    session_header: Some("x-fake-session"),
    stream: None,
    relay_heads: &[],
    relay_env: &[],
    owns_credentials_file: false,
    settings_env: None,
    quota: None,
    window_slot: None,
    window_key: None,
    usage: Some(USAGE),
    warm: None,
    login: None,
    swap_heads: SWAP_HEADS,
    bind: crate::agents::Bind::Origin,
    limit_reply: Some(limit_reply),
    resumes_within: None,
    inject: crate::agents::Inject::Env("FAKE_BASE_URL"),
    proxy: crate::agents::ProxyFace {
        via: &[],
        skip: &[],
    },
};
