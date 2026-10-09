//! 帧命令 `quota-probe`：起官方客户端报一个号的用量 → 读成窗口 → 记进同一本额度账（来源 `usage`）。
//! 不起真 claude：`PATH` 上放一个假的（照夹具吐那份输出、把自己被怎么起的写下来）。

use super::*;
use crate::accounts::quota::ledger::{Ledger, Source};
use crate::accounts::quota::rotation::{self, RotationStore};
use crate::accounts::upstream_select::rotate::{Hop, LibAccount, Library};
use std::sync::Arc;

const FIXTURE: &str = include_str!("../../__fixtures__/claude-usage.fixture.txt");

/// 2026-03-05 12:00 UTC。
const NOW: u64 = 1_772_712_000;

/// 假 claude：记下参数 · `TZ` · 配置目录 · 工作目录；配置目录末段 `c` ⇒ 没登录（退出码 1）；`bad` ⇒ 吐一份对不上的；其余吐夹具。
const FAKE: &str = r#"#!/bin/sh
d=$(dirname "$0")
{ echo "args=$*"; echo "tz=${TZ-unset}"; echo "dir=${CLAUDE_CONFIG_DIR-unset}"; echo "cwd=$(pwd)"; } > "$d/seen.txt"
case "${CLAUDE_CONFIG_DIR-}" in
  */c) echo "Not logged in · Please run /login" >&2; exit 1;;
  */bad) echo "Usage looks different now"; exit 0;;
esac
cat "$d/usage.txt"
"#;

struct Home {
    root: std::path::PathBuf,
    bin: std::path::PathBuf,
}

impl Home {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("ccm-probe-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).expect("mkdir");
        let fake = bin.join("claude");
        std::fs::write(&fake, FAKE).expect("fake");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        std::fs::write(bin.join("usage.txt"), FIXTURE).expect("fixture");
        for id in ["b", "c", "bad"] {
            std::fs::create_dir_all(root.join("accts").join(id)).expect("mkdir");
        }
        Self { root, bin }
    }

    /// 账号库：0（默认配置目录）· b · c（没登录）· bad（输出对不上）· api（按量号）。
    fn ctx(&self) -> Ctx {
        let accts = self.root.join("accts");
        let library: crate::accounts::upstream_select::rotate::LibraryRead =
            Arc::new(move |_| Library {
                enabled: true,
                accounts: std::iter::once(LibAccount {
                    id: "0".into(),
                    dir: None,
                    api: false,
                })
                .chain(["b", "c", "bad", "api"].iter().map(|id| LibAccount {
                    id: id.to_string(),
                    dir: Some(accts.join(id)),
                    api: *id == "api",
                }))
                .collect(),
            });
        Ctx {
            hop: Hop::new(
                Arc::new(RotationStore::at(Some(self.root.join(rotation::FILE_NAME)))),
                Arc::new(Ledger::at(Some(self.root.join(ledger::FILE_NAME)))),
                Some(self.root.clone()),
                library,
                None,
            ),
            rows: Box::new(|_, a| (a == "api").then_some(true)),
            live: Box::new(std::collections::BTreeSet::new),
            doing: Box::new(std::collections::BTreeMap::new),
        }
    }

    fn probe(&self, ctx: &Ctx, args: Value) -> Answer {
        let path = format!("{}:/usr/bin:/bin", self.bin.display());
        answer_probe_with(ctx, &args, NOW, Some(OsStr::new(&path)))
    }

    fn seen(&self) -> String {
        std::fs::read_to_string(self.bin.join("seen.txt")).unwrap_or_default()
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// ★ 起法照适配层（`claude -p /usage` · `TZ=UTC` · 那个号的配置目录 · 家里不出会话的工作目录），读成三档窗口、记进额度账、标来源 `usage`。
#[cfg(unix)]
#[test]
fn a_probe_starts_the_official_client_reads_three_windows_and_books_them() {
    let home = Home::new("read");
    let ctx = home.ctx();
    let v = home
        .probe(&ctx, json!({"agent": "claude-code", "account": "b"}))
        .expect("ok");
    assert_eq!(v["state"], "read");
    assert_eq!(v["from"], "usage");
    let names: Vec<&str> = v["windows"]
        .as_array()
        .expect("arr")
        .iter()
        .map(|w| w["name"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(names, ["five_hour", "seven_day", "seven_day_fable"]);
    let seen = home.seen();
    assert!(seen.contains("args=-p /usage"), "{seen}");
    assert!(seen.contains("tz=UTC"), "{seen}");
    assert!(
        seen.contains(&format!(
            "dir={}",
            home.root.join("accts").join("b").display()
        )),
        "{seen}"
    );
    assert!(
        seen.contains(&format!("cwd={}", home.root.join("autostart").display())),
        "{seen}"
    );
    let o = ctx.hop.quota.entry("claude-code", "b").expect("记进账了");
    assert_eq!(o.seen_at, NOW);
    assert_eq!(o.window_seen("seven_day_fable").from, Source::Usage);
    let on_disk = ledger::answer_of(ctx.hop.quota.path(), NOW);
    assert_eq!(on_disk["accounts"][0]["account"], "b");
    // `quota-read` 的显示态照原名出窗口（只增；`slots` 那两格照留）。
    let q = crate::faces::rotation_face::quota_read_with(&ctx, NOW);
    let row = &q["accounts"][0];
    assert_eq!(row["windows"][2]["key"], "7d:fable");
    assert_eq!(row["windows"][2]["from"], "usage");
    assert!(row["windows"][2].get("resetsAt").is_none(), "没在计时");
    assert_eq!(row["slots"][0]["slot"], "5h");
}

/// 账号 0（默认配置目录）⇒ 不给配置目录那一格。
#[cfg(unix)]
#[test]
fn account_zero_runs_without_a_config_dir() {
    let home = Home::new("zero");
    let ctx = home.ctx();
    home.probe(&ctx, json!({"agent": "claude-code", "account": "0"}))
        .expect("ok");
    assert!(home.seen().contains("dir=unset"), "{}", home.seen());
}

/// ★ 读不懂 ⇒ 照实回 `unreadable` ＋ 哪一处，不写账；没登录（退出码非零）⇒ `failed` 带它自己那句话。
#[cfg(unix)]
#[test]
fn unreadable_output_is_said_and_not_booked_and_a_failed_run_is_failed() {
    let home = Home::new("bad");
    let ctx = home.ctx();
    let v = home
        .probe(&ctx, json!({"agent": "claude-code", "account": "bad"}))
        .expect("ok");
    assert_eq!(v["state"], "unreadable");
    assert!(
        v["reason"]
            .as_str()
            .is_some_and(|r| r.contains("no usage line")),
        "{v}"
    );
    assert!(
        ctx.hop.quota.entry("claude-code", "bad").is_none(),
        "不写账"
    );
    let (code, msg) = home
        .probe(&ctx, json!({"agent": "claude-code", "account": "c"}))
        .expect_err("没登录");
    assert_eq!(code, "failed");
    assert!(msg.contains("Please run /login"), "{msg}");
    assert!(ctx.hop.quota.entry("claude-code", "c").is_none());
}

/// 入参：坏的 ⇒ `bad_args`；这一家没有这一形 ⇒ `unsupported`；按量号 ⇒ `unsupported`；不在账号库 ⇒ `not_found`。都不起进程。
#[test]
fn bad_args_unknown_accounts_and_api_keys_never_start_anything() {
    let home = Home::new("args");
    let ctx = home.ctx();
    let code = |args: Value| home.probe(&ctx, args).expect_err("应拒").0;
    assert_eq!(code(json!({"account": "b"})), "bad_args");
    assert_eq!(
        code(json!({"agent": "claude-code", "account": "_"})),
        "bad_args"
    );
    assert_eq!(
        code(json!({"agent": "claude-code", "account": "b", "x": 1})),
        "bad_args"
    );
    assert_eq!(
        code(json!({"agent": "codex", "account": "b"})),
        "unsupported"
    );
    assert_eq!(
        code(json!({"agent": "claude-code", "account": "api"})),
        "unsupported"
    );
    assert_eq!(
        code(json!({"agent": "claude-code", "account": "nobody"})),
        "not_found"
    );
    assert_eq!(home.seen(), "", "一次都没起");
}
