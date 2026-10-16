//! 帧命令 `quota-probe`：起官方客户端报一个号的用量 → 读成窗口 → 记进同一本额度账（来源 `usage`）。
//! 不起真 claude：`PATH` 上放一个假的（照夹具吐那份输出、把自己被怎么起的写下来）。

use super::*;
use crate::accounts::quota::ledger::{Ledger, Source};
use crate::accounts::quota::rotation::{self, RotationStore};
use crate::accounts::upstream_select::rotate::{Hop, LibAccount, Library};
use serde_json::{json, Value};
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
                Arc::new(std::collections::BTreeSet::new),
                None,
            ),
            rows: Box::new(|_, a| (a == "api").then_some(true)),
        }
    }

    /// 照线上那一份喂（JSON 入参经 typed 那一处收成类型，同生产），回包回成 `Value` 好按键看。
    fn probe(&self, ctx: &Ctx, args: Value) -> Result<Value, crate::wire::spec::Fail> {
        let path = format!("{}:/usr/bin:/bin", self.bin.display());
        let req: QuotaProbeReq = crate::wire::typed::take(&args)?;
        answer_probe_with(
            ctx,
            &crate::platform::fs::Own::issue_data_at(&self.root),
            &req,
            NOW,
            Some(OsStr::new(&path)),
        )
        .map(|p| serde_json::to_value(p).unwrap())
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
        .probe(&ctx, json!({"agent": "claude", "account": "b"}))
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
    assert_eq!(on_disk.accounts[0].account, "b");
    // `quota-read` 的显示态照原名出窗口（只增；`slots` 那两格照留）。
    let q = serde_json::to_value(crate::faces::rotation_face::quota_read_with(
        &ctx,
        NOW,
        &crate::Tz::default(),
    ))
    .expect("json");
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
    home.probe(&ctx, json!({"agent": "claude", "account": "0"}))
        .expect("ok");
    assert!(home.seen().contains("dir=unset"), "{}", home.seen());
}

/// ★ 读不懂 ⇒ 照实回 `unreadable` ＋ 哪一处，不写账；没登录（退出码非零）⇒ `failed`，它自己那句话进原话。
#[cfg(unix)]
#[test]
fn unreadable_output_is_said_and_not_booked_and_a_failed_run_is_failed() {
    let home = Home::new("bad");
    let ctx = home.ctx();
    let v = home
        .probe(&ctx, json!({"agent": "claude", "account": "bad"}))
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
    let crate::wire::spec::Fail {
        code,
        message: msg,
        raw,
        ..
    } = home
        .probe(&ctx, json!({"agent": "claude", "account": "c"}))
        .expect_err("没登录");
    assert_eq!(code, "failed");
    // 它自己那句话 ＋ 退出码是原话：进复制详情，不上句子。
    assert!(!msg.contains("Please run /login"), "{msg}");
    assert!(
        raw.as_deref()
            .is_some_and(|r| r.contains("Please run /login") && r.contains("exit 1")),
        "{raw:?}"
    );
    assert!(ctx.hop.quota.entry("claude-code", "c").is_none());
}

/// 入参：坏的 ⇒ `bad_args`；这一家没有这一形 ⇒ `unsupported`；按量号 ⇒ `unsupported`；不在账号库 ⇒ `not_found`。都不起进程。
#[test]
fn bad_args_unknown_accounts_and_api_keys_never_start_anything() {
    let home = Home::new("args");
    let ctx = home.ctx();
    let code = |args: Value| home.probe(&ctx, args).expect_err("应拒").code;
    assert_eq!(code(json!({"account": "b"})), "bad_args");
    assert_eq!(code(json!({"agent": "claude", "account": "_"})), "bad_args");
    assert_eq!(
        code(json!({"agent": "claude", "account": "b", "x": 1})),
        "bad_args"
    );
    assert_eq!(
        code(json!({"agent": "codex", "account": "b"})),
        "unsupported"
    );
    assert_eq!(
        code(json!({"agent": "claude", "account": "api"})),
        "unsupported"
    );
    assert_eq!(
        code(json!({"agent": "claude", "account": "nobody"})),
        "not_found"
    );
    assert_eq!(home.seen(), "", "一次都没起");
}

/// 换成 typed 之前钉的线上字节（回包过一遍 `Value` ＝ 帧里那一份写法，夹具根换成 `<root>`）：读得懂的一份 · 读不懂的一份，
/// 外加同一本账上 `quota-read` 照生产那条路（`quota_read_with`）写出来的那一份。
#[cfg(unix)]
#[test]
fn the_probe_and_quota_read_replies_keep_their_wire_bytes() {
    let home = Home::new("bytes");
    let ctx = home.ctx();
    let bytes = |v: &dyn erased_serde_free::Ser| v.bytes(&home.root);
    let absent = crate::faces::rotation_face::quota_read_with(&ctx, NOW, &crate::Tz::default());
    let read = home
        .probe(&ctx, json!({"agent": "claude", "account": "b"}))
        .expect("ok");
    let bad = home
        .probe(&ctx, json!({"agent": "claude", "account": "bad"}))
        .expect("ok");
    let q = crate::faces::rotation_face::quota_read_with(&ctx, NOW, &crate::Tz::default());
    std::fs::write(ctx.hop.quota.path().unwrap(), "{not json").unwrap();
    // 复制详情里带此刻的钟与这台的版本号 ⇒ 那一格只比它在不在。
    let mut unreadable = serde_json::to_value(crate::faces::rotation_face::quota_read_with(
        &ctx,
        NOW,
        &crate::Tz::default(),
    ))
    .unwrap();
    assert!(unreadable["detail"]
        .as_str()
        .is_some_and(|d| d.contains("码：unreadable")));
    unreadable["detail"] = json!("<detail>");
    let got = [
        bytes(&read),
        bytes(&bad),
        bytes(&q),
        bytes(&absent),
        bytes(&unreadable),
    ];
    let want: [&str; 5] = [
        r##"{"account":"b","agent":"claude","from":"usage","now":1772712000,"path":"<root>/quota.json","reason":null,"state":"read","windows":[{"name":"five_hour","resetsAt":1772899200,"used":0.03},{"name":"seven_day","resetsAt":1773041400,"used":0.21},{"name":"seven_day_fable","used":0.0}]}"##,
        r##"{"account":"bad","agent":"claude","from":"usage","now":1772712000,"path":"<root>/quota.json","reason":"no usage line","state":"unreadable","windows":[]}"##,
        r##"{"accounts":[{"account":"b","agent":"claude","fiveHour":"5h 3%","kind":"sub","limiting":"7d","login":"needsLogin","reading":{"refused":false,"windows":[{"name":"five_hour","resetsAt":1772899200,"resetsAtRelText":"+2d","resetsAtText":"03-07 16:00","used":0.03},{"name":"seven_day","resetsAt":1773041400,"resetsAtRelText":"+3d","resetsAtText":"03-09 07:30","used":0.21},{"name":"seven_day_fable","used":0.0}]},"rows":[[{"text":"b","tone":"plain"},{"text":"订阅","tone":"plain"},{"text":"需登录","tone":"warn"}],[{"text":"5h","tone":"plain"},{"text":"3%","tone":"plain"},{"text":"↻03-07 16:00","tone":"plain"},{"text":"+2d","tone":"plain"}],[{"text":"7d","tone":"plain"},{"text":"21%","tone":"plain"},{"text":"↻03-09 07:30","tone":"plain"},{"text":"+3d","tone":"plain"}],[{"text":"7d fable","tone":"plain"},{"text":"0%","tone":"plain"}],[{"text":"超额","tone":"plain"},{"text":"—","tone":"plain"}],[{"text":"采样","tone":"plain"},{"text":"12:00","tone":"plain"}]],"seenAt":1772712000,"seenAtText":"12:00","slots":[{"pct":3,"resetText":"↻03-07 16:00","resetsAt":1772899200,"resetsAtRelText":"+2d","resetsAtText":"03-07 16:00","slot":"5h","text":"3%","tone":"plain"},{"pct":21,"resetText":"↻03-09 07:30","resetsAt":1773041400,"resetsAtRelText":"+3d","resetsAtText":"03-09 07:30","slot":"7d","text":"21%","tone":"plain"}],"stale":false,"state":"ok","usage":{"slot":"7d","text":"7d 21%","tone":"plain","value":"21%","window":"7d"},"warm":{"act":"wait","at":1772715600,"atRelText":"+1h","atText":"13:00","text":"需登录"},"windows":[{"from":"usage","key":"5h","name":"five_hour","pct":3,"resetsAt":1772899200,"resetsAtRelText":"+2d","resetsAtText":"03-07 16:00","seenAt":1772712000,"seenAtText":"12:00","tone":"plain"},{"from":"usage","key":"7d","name":"seven_day","pct":21,"resetsAt":1773041400,"resetsAtRelText":"+3d","resetsAtText":"03-09 07:30","seenAt":1772712000,"seenAtText":"12:00","tone":"plain"},{"from":"usage","key":"7d:fable","name":"seven_day_fable","pct":0,"seenAt":1772712000,"seenAtText":"12:00","tone":"plain"}],"windowsSeen":{"five_hour":{"at":1772712000,"atText":"12:00","from":"usage"},"seven_day":{"at":1772712000,"atText":"12:00","from":"usage"},"seven_day_fable":{"at":1772712000,"atText":"12:00","from":"usage"}}}],"candidates":[{"account":"b","agent":"claude","kind":"sub"},{"account":"0","agent":"claude","kind":"sub"},{"account":"c","agent":"claude","kind":"sub"},{"account":"bad","agent":"claude","kind":"sub"},{"account":"api","agent":"claude","kind":"api"}],"detail":null,"earliestReturn":null,"fiveHour":null,"names":{"accounts":{"0":"~/.claude","_":"未指定账号"},"slots":{"5h":"5h","7d":"7d"}},"now":1772712000,"path":"<root>/quota.json","reason":null,"state":"present","text":null,"unseen":[{"account":"0","agent":"claude","fiveHour":null,"kind":"sub","login":"needsLogin","rows":[[{"text":"~/.claude","tone":"plain"},{"text":"订阅","tone":"plain"},{"text":"需登录","tone":"warn"}],[{"text":"5h","tone":"plain"},{"text":"—","tone":"plain"},{"text":"无采样","tone":"plain"}],[{"text":"7d","tone":"plain"},{"text":"—","tone":"plain"}]],"usage":{"slot":"5h","text":"5h —","tone":"plain","value":"—","window":"5h"},"warm":{"act":"wait","at":1772715600,"atRelText":"+1h","atText":"13:00","text":"需登录"}},{"account":"c","agent":"claude","fiveHour":null,"kind":"sub","login":"needsLogin","rows":[[{"text":"c","tone":"plain"},{"text":"订阅","tone":"plain"},{"text":"需登录","tone":"warn"}],[{"text":"5h","tone":"plain"},{"text":"—","tone":"plain"},{"text":"无采样","tone":"plain"}],[{"text":"7d","tone":"plain"},{"text":"—","tone":"plain"}]],"usage":{"slot":"5h","text":"5h —","tone":"plain","value":"—","window":"5h"},"warm":{"act":"wait","at":1772715600,"atRelText":"+1h","atText":"13:00","text":"需登录"}},{"account":"bad","agent":"claude","fiveHour":null,"kind":"sub","login":"needsLogin","rows":[[{"text":"bad","tone":"plain"},{"text":"订阅","tone":"plain"},{"text":"需登录","tone":"warn"}],[{"text":"5h","tone":"plain"},{"text":"—","tone":"plain"},{"text":"无采样","tone":"plain"}],[{"text":"7d","tone":"plain"},{"text":"—","tone":"plain"}]],"usage":{"slot":"5h","text":"5h —","tone":"plain","value":"—","window":"5h"},"warm":{"act":"wait","at":1772715600,"atRelText":"+1h","atText":"13:00","text":"需登录"}},{"account":"api","agent":"claude","fiveHour":null,"kind":"api","login":"ok","rows":[[{"text":"api","tone":"plain"},{"text":"按量","tone":"plain"}],[{"text":"5h/7d","tone":"plain"},{"text":"—","tone":"plain"},{"text":"无采样","tone":"plain"}]],"usage":{"text":"按量","tone":"plain","value":"按量"},"warm":{"act":"send","text":"无采样 · 发一句才知道"}}],"usableNow":["api"],"verdict":{"sub":"api","text":"可发 · 1 个号","tone":"plain"}}"##,
        r##"{"accounts":[],"candidates":[{"account":"0","agent":"claude","kind":"sub"},{"account":"b","agent":"claude","kind":"sub"},{"account":"c","agent":"claude","kind":"sub"},{"account":"bad","agent":"claude","kind":"sub"},{"account":"api","agent":"claude","kind":"api"}],"detail":null,"earliestReturn":null,"fiveHour":null,"names":{"accounts":{"0":"~/.claude","_":"未指定账号"},"slots":{"5h":"5h","7d":"7d"}},"now":1772712000,"path":"<root>/quota.json","reason":null,"state":"absent","text":null,"unseen":[{"account":"0","agent":"claude","fiveHour":null,"kind":"sub","login":"needsLogin","rows":[[{"text":"~/.claude","tone":"plain"},{"text":"订阅","tone":"plain"},{"text":"需登录","tone":"warn"}],[{"text":"5h","tone":"plain"},{"text":"—","tone":"plain"},{"text":"无采样","tone":"plain"}],[{"text":"7d","tone":"plain"},{"text":"—","tone":"plain"}]],"usage":{"slot":"5h","text":"5h —","tone":"plain","value":"—","window":"5h"},"warm":{"act":"wait","at":1772715600,"atRelText":"+1h","atText":"13:00","text":"需登录"}},{"account":"b","agent":"claude","fiveHour":null,"kind":"sub","login":"needsLogin","rows":[[{"text":"b","tone":"plain"},{"text":"订阅","tone":"plain"},{"text":"需登录","tone":"warn"}],[{"text":"5h","tone":"plain"},{"text":"—","tone":"plain"},{"text":"无采样","tone":"plain"}],[{"text":"7d","tone":"plain"},{"text":"—","tone":"plain"}]],"usage":{"slot":"5h","text":"5h —","tone":"plain","value":"—","window":"5h"},"warm":{"act":"wait","at":1772715600,"atRelText":"+1h","atText":"13:00","text":"需登录"}},{"account":"c","agent":"claude","fiveHour":null,"kind":"sub","login":"needsLogin","rows":[[{"text":"c","tone":"plain"},{"text":"订阅","tone":"plain"},{"text":"需登录","tone":"warn"}],[{"text":"5h","tone":"plain"},{"text":"—","tone":"plain"},{"text":"无采样","tone":"plain"}],[{"text":"7d","tone":"plain"},{"text":"—","tone":"plain"}]],"usage":{"slot":"5h","text":"5h —","tone":"plain","value":"—","window":"5h"},"warm":{"act":"wait","at":1772715600,"atRelText":"+1h","atText":"13:00","text":"需登录"}},{"account":"bad","agent":"claude","fiveHour":null,"kind":"sub","login":"needsLogin","rows":[[{"text":"bad","tone":"plain"},{"text":"订阅","tone":"plain"},{"text":"需登录","tone":"warn"}],[{"text":"5h","tone":"plain"},{"text":"—","tone":"plain"},{"text":"无采样","tone":"plain"}],[{"text":"7d","tone":"plain"},{"text":"—","tone":"plain"}]],"usage":{"slot":"5h","text":"5h —","tone":"plain","value":"—","window":"5h"},"warm":{"act":"wait","at":1772715600,"atRelText":"+1h","atText":"13:00","text":"需登录"}},{"account":"api","agent":"claude","fiveHour":null,"kind":"api","login":"ok","rows":[[{"text":"api","tone":"plain"},{"text":"按量","tone":"plain"}],[{"text":"5h/7d","tone":"plain"},{"text":"—","tone":"plain"},{"text":"无采样","tone":"plain"}]],"usage":{"text":"按量","tone":"plain","value":"按量"},"warm":{"act":"send","text":"无采样 · 发一句才知道"}}],"usableNow":["api"],"verdict":null}"##,
        r##"{"accounts":[],"candidates":[{"account":"0","agent":"claude","kind":"sub"},{"account":"b","agent":"claude","kind":"sub"},{"account":"c","agent":"claude","kind":"sub"},{"account":"bad","agent":"claude","kind":"sub"},{"account":"api","agent":"claude","kind":"api"}],"detail":"<detail>","earliestReturn":null,"fiveHour":"5h 无法读取","names":{"accounts":{"0":"~/.claude","_":"未指定账号"},"slots":{"5h":"5h","7d":"7d"}},"now":1772712000,"path":"<root>/quota.json","reason":"读取 <root>/quota.json 失败 · 内容无法解析","state":"unreadable","text":"读取 <root>/quota.json 失败 · 内容无法解析","unseen":[{"account":"0","agent":"claude","fiveHour":null,"kind":"sub","login":"needsLogin","rows":[[{"text":"~/.claude","tone":"plain"},{"text":"订阅","tone":"plain"},{"text":"需登录","tone":"warn"}],[{"text":"5h","tone":"plain"},{"text":"—","tone":"plain"},{"text":"无采样","tone":"plain"}],[{"text":"7d","tone":"plain"},{"text":"—","tone":"plain"}]],"usage":{"slot":"5h","text":"5h —","tone":"plain","value":"—","window":"5h"},"warm":{"act":"wait","at":1772712900,"atRelText":"+15m","atText":"12:15","text":"读取 <root>/quota.json 失败 · 内容无法解析 · 不发"}},{"account":"b","agent":"claude","fiveHour":null,"kind":"sub","login":"needsLogin","rows":[[{"text":"b","tone":"plain"},{"text":"订阅","tone":"plain"},{"text":"需登录","tone":"warn"}],[{"text":"5h","tone":"plain"},{"text":"—","tone":"plain"},{"text":"无采样","tone":"plain"}],[{"text":"7d","tone":"plain"},{"text":"—","tone":"plain"}]],"usage":{"slot":"5h","text":"5h —","tone":"plain","value":"—","window":"5h"},"warm":{"act":"wait","at":1772712900,"atRelText":"+15m","atText":"12:15","text":"读取 <root>/quota.json 失败 · 内容无法解析 · 不发"}},{"account":"c","agent":"claude","fiveHour":null,"kind":"sub","login":"needsLogin","rows":[[{"text":"c","tone":"plain"},{"text":"订阅","tone":"plain"},{"text":"需登录","tone":"warn"}],[{"text":"5h","tone":"plain"},{"text":"—","tone":"plain"},{"text":"无采样","tone":"plain"}],[{"text":"7d","tone":"plain"},{"text":"—","tone":"plain"}]],"usage":{"slot":"5h","text":"5h —","tone":"plain","value":"—","window":"5h"},"warm":{"act":"wait","at":1772712900,"atRelText":"+15m","atText":"12:15","text":"读取 <root>/quota.json 失败 · 内容无法解析 · 不发"}},{"account":"bad","agent":"claude","fiveHour":null,"kind":"sub","login":"needsLogin","rows":[[{"text":"bad","tone":"plain"},{"text":"订阅","tone":"plain"},{"text":"需登录","tone":"warn"}],[{"text":"5h","tone":"plain"},{"text":"—","tone":"plain"},{"text":"无采样","tone":"plain"}],[{"text":"7d","tone":"plain"},{"text":"—","tone":"plain"}]],"usage":{"slot":"5h","text":"5h —","tone":"plain","value":"—","window":"5h"},"warm":{"act":"wait","at":1772712900,"atRelText":"+15m","atText":"12:15","text":"读取 <root>/quota.json 失败 · 内容无法解析 · 不发"}},{"account":"api","agent":"claude","fiveHour":null,"kind":"api","login":"ok","rows":[[{"text":"api","tone":"plain"},{"text":"按量","tone":"plain"}],[{"text":"5h/7d","tone":"plain"},{"text":"—","tone":"plain"},{"text":"无采样","tone":"plain"}]],"usage":{"text":"按量","tone":"plain","value":"按量"},"warm":{"act":"wait","at":1772712900,"atRelText":"+15m","atText":"12:15","text":"读取 <root>/quota.json 失败 · 内容无法解析 · 不发"}}],"usableNow":["api"],"verdict":null}"##,
    ];
    for (g, w) in got.iter().zip(want) {
        assert_eq!(g, w, "线上字节变了。现打：\n{}", got.join("\n----\n"));
    }
}

/// 回包 → 帧里那一份字节（先成 `Value`：键按名排，同分派那一层 `spec::wire`）。
mod erased_serde_free {
    pub(super) trait Ser {
        fn bytes(&self, root: &std::path::Path) -> String;
    }
    impl<T: serde::Serialize> Ser for T {
        fn bytes(&self, root: &std::path::Path) -> String {
            serde_json::to_string(&serde_json::to_value(self).unwrap())
                .unwrap()
                .replace(&root.display().to_string(), "<root>")
        }
    }
}
