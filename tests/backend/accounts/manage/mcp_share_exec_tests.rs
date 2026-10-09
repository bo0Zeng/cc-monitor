//! 账号之间同步用户级 MCP 走一遍真文件系统：临时家目录里经帧面建库（z）、加号（b），再在号里加 / 被旧内容盖 /
//! 经 cc-monitor 删 / 两边各改，每一步看两个号的配置文件落成了什么；配置文件里除那一个键之外逐字节不变。
//!
//! 夹具只造结构：登录状态是写死的假邮箱，密钥是 `sk-fake-…` 假串（判据顺带核它不出现在任何一句话里）。
#![cfg(unix)]

use super::*;
use crate::accounts::upstream_select::file_face;
use crate::assets::aliases::tests::HomeDoor;
use crate::faces::accounts_face::{answer, KeyDoor};
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

const SECRET: &str = "sk-fake-0123456789";

struct Tmp(PathBuf);
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn tmp(tag: &str) -> Tmp {
    let h = std::env::temp_dir().join(format!(
        "acct-mcp-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|x| x.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&h).unwrap();
    Tmp(h)
}

impl Tmp {
    fn write(&self, rel: &str, body: &str) {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }
    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.0.join(rel)).unwrap_or_default()
    }
    fn mode(&self, rel: &str) -> u32 {
        std::fs::metadata(self.0.join(rel))
            .unwrap()
            .permissions()
            .mode()
            & 0o777
    }
    fn door(&self) -> HomeDoor {
        HomeDoor(self.0.clone())
    }
    /// 一个号配置文件里的服务器表。
    fn servers(&self, acct: &str) -> Value {
        let v: Value = serde_json::from_str(&self.read(&cfg(acct))).unwrap_or(Value::Null);
        v.get("mcpServers").cloned().unwrap_or(json!({}))
    }
    fn call(&self, cmd: &str, args: Value) -> Result<Value, crate::stream::inbound::spec::Fail> {
        let table = self.0.join(".cc-monitor/apikey-credentials.json");
        let set = |v: &Value| file_face::answer_set_at(&table, v);
        let drop = |v: &Value| file_face::answer_drop_at(&table, v);
        let restore = |v: &Value| file_face::answer_restore_at(&table, v);
        let path = || Ok(table.clone());
        let keys = KeyDoor {
            set: &set,
            drop: &drop,
            restore: &restore,
            path: &path,
        };
        answer(&self.door(), cmd, &args, &keys)
    }
    fn ok(&self, cmd: &str, args: Value) -> Value {
        self.call(cmd, args.clone())
            .unwrap_or_else(|e| panic!("{cmd} {args}：{e:?}"))
    }
}

fn cfg(acct: &str) -> String {
    format!(".cc-monitor/accounts/{acct}/.claude.json")
}

/// Claude 自己写的那种排版（两格缩进），里面有登录与别的本机状态。
fn claude_json(email: &str, servers: &Value, startups: u32) -> String {
    let mut v = serde_json::to_string_pretty(&json!({
        "numStartups": startups,
        "oauthAccount": { "emailAddress": email, "accountUuid": "00000000-0000-0000-0000-000000000000" },
        "mcpServers": servers,
        "projects": { "/w/p": { "allowedTools": [], "hasTrustDialogAccepted": true } },
        "tipsHistory": { "x": 3 }
    }))
    .unwrap();
    v.push('\n');
    v
}

fn cclsp() -> Value {
    json!({ "cclsp": { "type": "stdio", "command": "cclsp", "args": [] } })
}

fn anysearch() -> Value {
    json!({ "type": "stdio", "command": "npx", "args": ["-y", "anysearch"], "env": { "ANYSEARCH_KEY": SECRET } })
}

/// 两个号：z（建库时从现有登录收成默认号）· b（加号，登录之后 Claude 写了它自己的那一份）。
fn two(tag: &str) -> Tmp {
    let t = tmp(tag);
    t.write(".claude/.credentials.json", "{\"fake\":\"cred\"}");
    t.write(".claude/settings.json", "{}");
    t.write(".claude.json", &claude_json("z@example.test", &cclsp(), 7));
    let init = t.ok("accounts-init", json!({ "name": "z" }));
    assert_eq!(init["applied"], true);
    let add = t.ok(
        "accounts-add",
        json!({ "name": "b", "kind": "subscription" }),
    );
    let notes = add["notes"].to_string();
    assert!(notes.contains('b'), "加号那一趟说了同步到 b：{notes}");
    assert_eq!(t.servers("b"), cclsp(), "加号 ⇒ 新号拿到共享集合");
    assert_eq!(t.mode(&cfg("b")), 0o600, "新建的那份只给自己读");
    t.write(&cfg("b"), &claude_json("b@example.test", &cclsp(), 1));
    t
}

/// `text` 里 `mcpServers` 那个值之外的全部字节。
fn outside(text: &str) -> String {
    let v: Value = serde_json::from_str(text).unwrap();
    let raw = serde_json::to_string_pretty(&v["mcpServers"])
        .unwrap()
        .replace('\n', "\n  ");
    let at = text.find(&raw).expect("值按两格缩进排");
    format!("{}{}", &text[..at], &text[at + raw.len()..])
}

fn add_to(servers: &Value, name: &str, def: Value) -> Value {
    let mut s = servers.clone();
    s[name] = def;
    s
}

#[test]
fn added_in_one_account_reaches_both_and_nothing_else_moves() {
    let t = two("add");
    let d = t.door();
    let b_before = t.read(&cfg("b"));
    let z_new = claude_json(
        "z@example.test",
        &add_to(&cclsp(), "anysearch", anysearch()),
        8,
    );
    t.write(&cfg("z"), &z_new);
    let v = sync(&d).unwrap();
    assert_eq!(v.changed, vec!["b".to_string()]);
    assert_eq!(
        v.servers,
        vec!["anysearch".to_string(), "cclsp".to_string()]
    );
    assert_eq!(
        t.servers("b")["anysearch"],
        anysearch(),
        "密钥照原样在同一台的号之间走"
    );
    assert_eq!(t.read(&cfg("z")), z_new, "加的那个号一个字节不动");
    let b_after = t.read(&cfg("b"));
    assert_eq!(
        outside(&b_after),
        outside(&b_before),
        "b 里除那一个键之外逐字节不变"
    );
    // 写之前那份原文进了备份（0600）；共享集合也是 0600。
    let bk = ".cc-monitor/backups/accounts-mcp/b.claude.json";
    assert_eq!(t.read(bk), b_before);
    assert_eq!(t.mode(bk), 0o600);
    assert_eq!(t.mode(".cc-monitor/accounts-mcp.json"), 0o600);
    assert_eq!(t.mode(&cfg("b")), 0o600, "被写的那份沿用原权限");
    // 再来一趟：没有要写的。
    assert!(sync(&d).unwrap().changed.is_empty());
}

/// ★ 停止同步：停着时一个号里加的不同步过去、已同步的一条不删；cc-monitor 里的删 / 挑被拒；开回来那一刻照常同步一趟。
#[test]
fn stopping_the_sync_leaves_each_account_alone_until_it_is_turned_back_on() {
    let t = two("pause");
    let d = t.door();
    let v = t.ok("accounts-mcp-sync", json!({ "on": false }));
    assert_eq!(v["sync"], false, "{v}");
    assert_eq!(v["servers"], json!(["cclsp"]), "停了也照实说共享的那几条");
    assert_eq!(
        t.read(".cc-monitor/accounts-mcp.json")
            .contains("\"sync\": false"),
        true
    );
    let z_new = claude_json(
        "z@example.test",
        &add_to(&cclsp(), "anysearch", anysearch()),
        8,
    );
    t.write(&cfg("z"), &z_new);
    let b_before = t.read(&cfg("b"));
    let quiet = sync(&d).unwrap();
    assert!(
        quiet.changed.is_empty() && !quiet.sync,
        "停着 ⇒ 文件事件那一趟什么都不写"
    );
    assert_eq!(t.read(&cfg("b")), b_before, "停着 ⇒ b 一个字节不动");
    assert_eq!(
        t.servers("z")["cclsp"],
        cclsp()["cclsp"],
        "已同步过去的不删"
    );
    assert_eq!(
        t.call("accounts-mcp-remove", json!({ "name": "cclsp" }))
            .unwrap_err()
            .code,
        "refused",
        "停着 ⇒ 从所有号删一条被拒（不然就成了同步）"
    );
    assert_eq!(t.ok("accounts-mcp-read", json!({}))["sync"], false);
    let on = t.ok("accounts-mcp-sync", json!({ "on": true }));
    assert_eq!(on["sync"], true);
    assert_eq!(
        on["changed"],
        json!(["b"]),
        "开回来 ⇒ 立刻同步一趟：z 停着时加的那条到了 b"
    );
    assert_eq!(t.servers("b")["anysearch"], anysearch());
    assert!(
        !t.read(".cc-monitor/accounts-mcp.json").contains("\"sync\""),
        "开着时那一格不写"
    );
    let again = t.ok("accounts-mcp-sync", json!({ "on": true }));
    assert_eq!(again["changed"], json!([]), "已经开着 ⇒ 不写，照答");
}

#[test]
fn an_old_full_rewrite_gets_the_entry_put_back() {
    let t = two("back");
    let d = t.door();
    t.write(
        &cfg("z"),
        &claude_json(
            "z@example.test",
            &add_to(&cclsp(), "anysearch", anysearch()),
            8,
        ),
    );
    sync(&d).unwrap();
    // b 里的 Claude 拿内存里的旧内容整份重写（还没有 anysearch，别的格也是它自己的新值）。
    let old = claude_json("b@example.test", &cclsp(), 2);
    t.write(&cfg("b"), &old);
    let v = sync(&d).unwrap();
    assert_eq!(v.changed, vec!["b".to_string()]);
    assert_eq!(
        t.servers("b")["anysearch"],
        anysearch(),
        "被盖掉的那一条补回来了"
    );
    assert_eq!(
        outside(&t.read(&cfg("b"))),
        outside(&old),
        "补回只动那一个键"
    );
    assert_eq!(t.servers("z")["anysearch"], anysearch());
}

#[test]
fn removed_in_cc_monitor_leaves_both_accounts() {
    let t = two("rm");
    let d = t.door();
    t.write(
        &cfg("z"),
        &claude_json(
            "z@example.test",
            &add_to(&cclsp(), "anysearch", anysearch()),
            8,
        ),
    );
    sync(&d).unwrap();
    let v = t.ok("accounts-mcp-remove", json!({ "name": "anysearch" }));
    assert_eq!(v["servers"], json!(["cclsp"]));
    for a in ["z", "b"] {
        assert_eq!(t.servers(a), cclsp(), "{a} 里撤掉了");
    }
    // 再同步一趟也不回来（底里已经没有它）。
    sync(&d).unwrap();
    assert_eq!(t.servers("b"), cclsp());
    assert_eq!(
        t.call("accounts-mcp-remove", json!({ "name": "anysearch" }))
            .unwrap_err()
            .code,
        "not_found"
    );
}

#[test]
fn changed_in_both_accounts_is_a_conflict_until_the_user_picks() {
    let t = two("both");
    let d = t.door();
    let vz = json!({ "type": "stdio", "command": "cclsp", "args": ["--z"] });
    let vb = json!({ "type": "stdio", "command": "cclsp", "args": ["--b"] });
    let z_text = claude_json("z@example.test", &json!({ "cclsp": vz }), 9);
    let b_text = claude_json("b@example.test", &json!({ "cclsp": vb }), 9);
    t.write(&cfg("z"), &z_text);
    t.write(&cfg("b"), &b_text);
    let v = sync(&d).unwrap();
    assert!(v.changed.is_empty(), "不自动选：一个号都不写");
    assert_eq!((t.read(&cfg("z")), t.read(&cfg("b"))), (z_text, b_text));
    assert_eq!(v.conflicts.len(), 1);
    let seen = t.ok("accounts-mcp-read", json!({}));
    let froms: Vec<&Value> = seen["conflicts"][0]["choices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| &c["from"])
        .collect();
    assert_eq!(froms, vec![&Value::Null, &json!("z"), &json!("b")]);
    let after = t.ok("accounts-mcp-pick", json!({ "name": "cclsp", "from": "z" }));
    assert_eq!(after["conflicts"], json!([]));
    assert_eq!(after["changed"], json!(["b"]));
    assert_eq!(t.servers("b")["cclsp"], vz);
}

#[test]
fn put_from_cc_monitor_reaches_every_account() {
    let t = two("put");
    let v = put(&t.door(), "anysearch", &anysearch()).unwrap();
    assert_eq!(v.changed, vec!["z".to_string(), "b".to_string()]);
    for a in ["z", "b"] {
        assert_eq!(t.servers(a)["anysearch"], anysearch());
    }
    assert_eq!(
        put(&t.door(), " ", &json!({})).unwrap_err().code,
        "bad_args"
    );
}

#[test]
fn no_value_ever_leaves_in_what_is_said() {
    let t = two("quiet");
    t.write(
        &cfg("z"),
        &claude_json(
            "z@example.test",
            &add_to(&cclsp(), "anysearch", anysearch()),
            8,
        ),
    );
    t.write(&cfg("b"), "{ not json");
    let v = sync(&t.door()).unwrap();
    assert_eq!(v.notes.len(), 1, "读不出来的那个号说一句：{:?}", v.notes);
    assert!(v.notes[0].contains('b'));
    let said =
        serde_json::to_string(&v).unwrap() + &t.ok("accounts-mcp-read", json!({})).to_string();
    assert!(
        !said.contains(SECRET) && !said.contains("npx"),
        "成品里只有名字与号名：{said}"
    );
    assert_eq!(
        t.read(&cfg("b")),
        "{ not json",
        "读不出来的那份一个字节不碰"
    );
}

#[test]
fn the_shared_set_is_what_the_extension_page_reads_as_user_level() {
    let t = tmp("ext");
    let home = t.0.display().to_string();
    assert_eq!(
        store_file_in(&home),
        None,
        "没有账号库 ⇒ 用户级 MCP 是 agent 自己那一份"
    );
    let t = two("ext2");
    let home = t.0.display().to_string();
    let store = store_file_in(&home).expect("有账号库 ⇒ 共享集合那一份");
    let mut seen = crate::agents::Sightings::default();
    crate::agents::claudecode::assets::scan_user_mcp_at(std::path::Path::new(&store), &mut seen);
    let names: Vec<&str> = seen.mcp.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, vec!["cclsp"], "资产目录那一份读法认得它");
}

#[test]
fn bad_arguments_are_refused() {
    let t = two("args");
    assert_eq!(
        t.call("accounts-mcp-read", json!({ "x": 1 }))
            .unwrap_err()
            .code,
        "bad_args"
    );
    assert_eq!(
        t.call("accounts-mcp-remove", json!({})).unwrap_err().code,
        "bad_args"
    );
    assert_eq!(
        t.call(
            "accounts-mcp-pick",
            json!({ "name": "cclsp", "from": "nobody" })
        )
        .unwrap_err()
        .code,
        "refused"
    );
}

/// 文件事件触发（不轮询）：起监听器，在 z 里加一条 ⇒ b 自己跟上。判据这一侧等它（有上限），生产侧没有任何定时。
#[test]
fn a_file_event_alone_brings_the_other_account_along() {
    let t = two("watch");
    let door: &'static HomeDoor = Box::leak(Box::new(t.door()));
    super::super::mcp_share_watch::start(door);
    t.write(
        &cfg("z"),
        &claude_json(
            "z@example.test",
            &add_to(&cclsp(), "anysearch", anysearch()),
            8,
        ),
    );
    let mut waited = 0;
    while t.servers("b").get("anysearch").is_none() && waited < 200 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        waited += 1;
    }
    assert_eq!(t.servers("b")["anysearch"], anysearch(), "10 秒内跟上");
}
