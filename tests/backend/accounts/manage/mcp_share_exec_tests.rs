//! 账号之间同步用户级 MCP 走一遍真文件系统：临时家目录里经帧面建库（z）、加号（b），再在号里加 / 被旧内容盖 /
//! 经 cc-monitor 删 / 两边各改，每一步看两个号的配置文件落成了什么；配置文件里除那一个键之外逐字节不变。
//!
//! 夹具只造结构：登录状态是写死的假邮箱，密钥是 `sk-fake-…` 假串（判据顺带核它不出现在任何一句话里）。
#![cfg(unix)]

use crate::accounts::manage::mcp_share_exec::*;
use crate::accounts::upstream_select::file_face;
use crate::assets::aliases::test_support::HomeDoor;
use crate::common::door::Door;
use crate::common::said::Said;
use crate::faces::accounts_face::{test_support::answer, KeyDoor};
use serde_json::{json, Value};
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
    Tmp(test_home::scratch(tag))
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
    fn call(&self, cmd: &str, args: Value) -> Result<Value, crate::common::said::Said> {
        let table = self.0.join(".cc-monitor/apikey-credentials.json");
        let set = |v: &crate::accounts::upstream_select::file_face::KeySetReq| {
            file_face::answer_set_at(
                &crate::platform::fs::Own::issue_data_at(table.parent().expect("表所在的那一层")),
                v,
            )
        };
        let drop = |v: &Value| {
            file_face::answer_drop_at(
                &crate::platform::fs::Own::issue_data_at(table.parent().expect("表所在的那一层")),
                v,
            )
        };
        let restore = |v: &Value| {
            file_face::answer_restore_at(
                &crate::platform::fs::Own::issue_data_at(table.parent().expect("表所在的那一层")),
                v,
            )
        };
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
        serde_json::to_value(&v.servers).unwrap(),
        json!([{ "name": "anysearch" }, { "name": "cclsp" }])
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
    assert_eq!(
        v["servers"],
        json!([{ "name": "cclsp" }]),
        "停了也照实说共享的那几条"
    );
    assert!(t
        .read(".cc-monitor/accounts-mcp.json")
        .contains("\"sync\": false"));
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
            .code
            .as_deref(),
        Some("refused"),
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
    assert_eq!(v["servers"], json!([{ "name": "cclsp" }]));
    for a in ["z", "b"] {
        assert_eq!(t.servers(a), cclsp(), "{a} 里撤掉了");
    }
    // 再同步一趟也不回来（底里已经没有它）。
    sync(&d).unwrap();
    assert_eq!(t.servers("b"), cclsp());
    assert_eq!(
        t.call("accounts-mcp-remove", json!({ "name": "anysearch" }))
            .unwrap_err()
            .code
            .as_deref(),
        Some("not_found")
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
        put(&t.door(), " ", &json!({})).unwrap_err().code.as_deref(),
        Some("bad_args")
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
    let said = serde_json::to_string(&v).unwrap()
        + t.ok("accounts-mcp-read", json!({})).to_string().as_str();
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
    assert_eq!(
        store_path_in(&t.0),
        None,
        "没有账号库 ⇒ 用户级 MCP 是 agent 自己那一份"
    );
    let t = two("ext2");
    let store = store_path_in(&t.0).expect("有账号库 ⇒ 共享集合那一份");
    let mut seen = crate::agents::Sightings::default();
    crate::agents::claudecode::assets::scan_user_mcp_at(&store, &mut seen);
    let names: Vec<&str> = seen.mcp.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, vec!["cclsp"], "资产目录那一份读法认得它");
}

#[test]
fn bad_arguments_are_refused() {
    let t = two("args");
    assert_eq!(
        t.call("accounts-mcp-read", json!({ "x": 1 }))
            .unwrap_err()
            .code
            .as_deref(),
        Some("bad_args")
    );
    assert_eq!(
        t.call("accounts-mcp-remove", json!({}))
            .unwrap_err()
            .code
            .as_deref(),
        Some("bad_args")
    );
    assert_eq!(
        t.call(
            "accounts-mcp-pick",
            json!({ "name": "cclsp", "from": "nobody" })
        )
        .unwrap_err()
        .code
        .as_deref(),
        Some("refused")
    );
}

/// 文件事件触发（不轮询）：起监听器，在 z 里加一条 ⇒ b 自己跟上。判据这一侧等它（有上限），生产侧没有任何定时。
#[test]
fn a_file_event_alone_brings_the_other_account_along() {
    let t = two("watch");
    let door: &'static HomeDoor = Box::leak(Box::new(t.door()));
    crate::accounts::manage::mcp_share_watch::start(door, crate::platform::fs::Own::for_tests());
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

// ── MCP 登录那一半（`mcp_logins_exec`）：各号凭据文件里 `mcpOAuth` 那一节 ─────────────────────────────

const MCP_KEY: &str = "fake|0123456789abcdef";

fn creds_rel(acct: &str) -> String {
    format!(".cc-monitor/accounts/{acct}/.credentials.json")
}

fn mcp_entry(at: &str, rt: &str, exp: u64) -> Value {
    json!({
        "serverName": "fake", "serverUrl": "http://127.0.0.1:1/mcp",
        "accessToken": at, "refreshToken": rt, "expiresAt": exp,
        "clientId": "client-x", "discoveryState": { "authorizationServerUrl": "http://127.0.0.1:1" }
    })
}

/// 一个号的凭据文件：订阅号登录 ＋ 一节别的 ＋（可缺）MCP 登录那一节。全是现编的假值。
fn creds_json(acct: &str, mcp: Option<Value>) -> String {
    let mut v = json!({
        "claudeAiOauth": {
            "accessToken": format!("fake-at-{acct}"), "refreshToken": format!("fake-rt-{acct}"),
            "expiresAt": 1_900_000_000_000u64, "scopes": ["user:inference"], "subscriptionType": "max"
        },
        "zzOther": { "keep": acct }
    });
    if let Some(m) = mcp {
        v["mcpOAuth"] = m;
    }
    serde_json::to_string(&v).unwrap()
}

impl Tmp {
    fn creds(&self, acct: &str) -> Value {
        serde_json::from_str(&self.read(&creds_rel(acct))).unwrap_or(Value::Null)
    }
    fn mcp(&self, acct: &str) -> Value {
        self.creds(acct)["mcpOAuth"][MCP_KEY].clone()
    }
    fn logins(&self) -> crate::accounts::manage::mcp_logins_exec::Synced {
        crate::accounts::manage::mcp_logins_exec::sync_among(
            &self.door(),
            &crate::platform::fs::Own::for_tests(),
            crate::agents::REGISTRY,
        )
        .unwrap()
    }
}

/// 凭据文件里 `mcpOAuth` 之外的全部格。
fn outside_mcp(v: &Value) -> Value {
    let mut o = v.clone();
    o.as_object_mut().unwrap().remove("mcpOAuth");
    o
}

/// z 登录了一个 MCP，b 没有。
fn two_logins(tag: &str) -> Tmp {
    let t = two(tag);
    let one = mcp_entry("at-1", "rt-1", 2_000_000_000_000);
    t.write(
        &creds_rel("z"),
        &creds_json("z", Some(json!({ MCP_KEY: one }))),
    );
    t.write(&creds_rel("b"), &creds_json("b", None));
    t
}

/// ★ 一个号登录 ⇒ 别的号也有；凭据文件里别的格原样；只给本人读；再来一趟没有要写的。
#[test]
fn a_mcp_login_in_one_account_reaches_the_other_and_nothing_else_moves() {
    let t = two_logins("login");
    let z_before = t.read(&creds_rel("z"));
    let b_before = t.creds("b");
    let v = t.logins();
    assert_eq!(v.changed, vec!["b".to_string()]);
    assert_eq!(
        t.mcp("b"),
        mcp_entry("at-1", "rt-1", 2_000_000_000_000),
        "整条照抄（客户端登记一起）"
    );
    assert_eq!(
        outside_mcp(&t.creds("b")),
        b_before,
        "b 的凭据文件里别的格变了"
    );
    assert_eq!(
        t.read(&creds_rel("z")),
        z_before,
        "登录的那个号一个字节不动"
    );
    assert_eq!(t.mode(&creds_rel("b")), 0o600);
    let store = t.read(".cc-monitor/accounts-mcp.json");
    assert!(
        !store.contains("at-1") && !store.contains("rt-1"),
        "共享集合那份文件里有了令牌"
    );
    assert!(t.logins().changed.is_empty(), "第二趟还有要写的");
}

/// 登录那一趟走有号的每一家，照 `logins_shared` 判，不看那一家声明没声明 MCP 那一格：没声明 MCP、登录可同步 ⇒ 照常同步；
/// 两样都没声明 ⇒ 不动、一句不说。
#[test]
fn logins_follow_every_family_with_accounts_not_only_those_with_mcp_cells() {
    use crate::agents::claudecode::ADAPTER as CLAUDE;
    let strip = |login: bool| {
        [crate::agents::Adapter {
            accounts: CLAUDE.accounts.map(|f| crate::agents::AccountsFace {
                mcp: None,
                mcp_logins: if login { f.mcp_logins } else { None },
                ..f
            }),
            ..CLAUDE
        }]
    };
    let own = crate::platform::fs::Own::for_tests();
    let t = two_logins("login-no-mcp");
    let quiet =
        crate::accounts::manage::mcp_logins_exec::sync_among(&t.door(), &own, &strip(false))
            .expect("一趟");
    assert_eq!(quiet, Default::default(), "两样都没声明 ⇒ 一句不说");
    assert!(t.creds("b")["mcpOAuth"].is_null(), "两样都没声明 ⇒ 不动");
    let v = crate::accounts::manage::mcp_logins_exec::sync_among(&t.door(), &own, &strip(true))
        .expect("一趟");
    assert_eq!(v.changed, vec!["b".to_string()]);
    assert_eq!(t.mcp("b"), mcp_entry("at-1", "rt-1", 2_000_000_000_000));
}

/// ★ 一个号续期 ⇒ 别的号拿到新令牌。
#[test]
fn a_renewal_in_one_account_reaches_the_other() {
    let t = two_logins("renew");
    t.logins();
    let new = mcp_entry("at-2", "rt-2", 2_000_000_600_000);
    t.write(
        &creds_rel("b"),
        &creds_json("b", Some(json!({ MCP_KEY: new.clone() }))),
    );
    assert_eq!(t.logins().changed, vec!["z".to_string()]);
    assert_eq!(t.mcp("z"), new);
    assert_eq!(
        outside_mcp(&t.creds("z")),
        outside_mcp(&serde_json::from_str(&creds_json("z", None)).unwrap())
    );
}

/// ★ 两边同时续：输的那一边被那一家清成失效的样子 ⇒ 拿到赢的那一条；没有一份副本停在作废的刷新令牌上。
#[test]
fn when_both_renew_at_once_no_copy_keeps_an_invalidated_refresh_token() {
    let t = two_logins("both");
    t.logins();
    let won = mcp_entry("at-2", "rt-2", 2_000_000_600_000);
    let dead = json!({ "serverName": "fake", "serverUrl": "http://127.0.0.1:1/mcp", "accessToken": "", "expiresAt": 0, "clientId": "client-x" });
    t.write(
        &creds_rel("z"),
        &creds_json("z", Some(json!({ MCP_KEY: won.clone() }))),
    );
    t.write(
        &creds_rel("b"),
        &creds_json("b", Some(json!({ MCP_KEY: dead }))),
    );
    t.logins();
    assert_eq!(t.mcp("z"), won, "赢的那一条被失效的盖了");
    assert_eq!(t.mcp("b"), won, "b 停在失效的那一条上");
}

/// ★ 在一个号里退出（那一条整条没了）⇒ 所有号都撤。
#[test]
fn logging_out_in_one_account_removes_it_from_the_other() {
    let t = two_logins("logout");
    t.logins();
    t.write(&creds_rel("z"), &creds_json("z", Some(json!({}))));
    assert_eq!(t.logins().changed, vec!["b".to_string()]);
    assert_eq!(t.mcp("b"), Value::Null);
    assert_eq!(t.creds("b")["zzOther"]["keep"], "b");
}

/// ★ 那个号的 claude 正在续这一条（它那把锁在）· 正在整份写这份文件（写锁在）⇒ 这一趟不写、底不动；锁放了下一趟补上。
#[test]
fn claude_holding_its_renew_or_write_lock_defers_the_write() {
    for lock in [
        "mcp-refresh-fake_0123456789abcdef.lock.lock",
        ".storage-write.lock",
    ] {
        let t = two_logins("locked");
        let held = t.0.join(format!(".cc-monitor/accounts/b/{lock}"));
        std::fs::create_dir(&held).unwrap();
        let b_before = t.read(&creds_rel("b"));
        assert!(t.logins().changed.is_empty(), "{lock} 在，还是写了");
        assert_eq!(
            t.read(&creds_rel("b")),
            b_before,
            "{lock} 在，b 的凭据文件变了"
        );
        assert!(held.is_dir(), "别人的锁不许动");
        std::fs::remove_dir(&held).unwrap();
        assert_eq!(
            t.logins().changed,
            vec!["b".to_string()],
            "{lock} 放了之后没补上"
        );
    }
}

/// 停着同步（同一个开关）⇒ 登录那一半也不动。
#[test]
fn a_paused_sync_leaves_mcp_logins_alone() {
    let t = two_logins("lpause");
    t.ok("accounts-mcp-sync", json!({ "on": false }));
    assert!(t.logins().changed.is_empty());
    assert_eq!(t.mcp("b"), Value::Null);
}

/// 凭据文件不在的号（还没登录订阅号）不参与：不替它建凭据文件。
#[test]
fn an_account_without_a_credentials_file_is_left_alone() {
    let t = two_logins("nofile");
    std::fs::remove_file(t.0.join(creds_rel("b"))).unwrap();
    assert!(t.logins().changed.is_empty());
    assert!(
        !t.0.join(creds_rel("b")).exists(),
        "替没登录的号建了凭据文件"
    );
}

// ── MCP 需登录那一判（needs_login_of）：读面夹具全是合成的 ──

use crate::agents::{McpEntry, McpRead, McpStatus};

fn need_entry(
    scope: &'static str,
    name: &str,
    status: McpStatus,
    login_in: &[Option<&str>],
) -> McpEntry {
    McpEntry {
        scope,
        name: name.to_string(),
        server: serde_json::json!({}),
        source: "/h/.claude.json".to_string(),
        status,
        login_in: login_in.iter().map(|s| s.map(str::to_string)).collect(),
        seen_ms: None,
    }
}

fn need_read(entries: Vec<McpEntry>) -> McpRead {
    McpRead {
        entries,
        dirs: vec![],
        problems: vec![],
    }
}

/// 用户级那一份 ＋ 活会话项目那一份：项目级 · 本地级要登录的带上项目；项目那一份里重复出现的用户级不再算一遍；不要登录的不算。
#[test]
fn project_servers_needing_login_count_and_carry_their_project() {
    let user = need_read(vec![
        need_entry("user", "notes", McpStatus::NeedsLogin, &[Some("work")]),
        need_entry("user", "down", McpStatus::Failed, &[]),
    ]);
    let proj = need_read(vec![
        need_entry("user", "notes", McpStatus::NeedsLogin, &[Some("work")]),
        need_entry("project", "tracker", McpStatus::NeedsLogin, &[None]),
        need_entry("local", "db", McpStatus::NeedsLogin, &[Some("side")]),
        need_entry("project", "ok", McpStatus::Disabled, &[]),
    ]);
    let got = needs_login_of(&[(None, user), (Some("/p/app".into()), proj)]);
    let need =
        |name: &str, scope: &'static str, who: &[Option<&str>], project: Option<&str>| McpNeed {
            name: name.into(),
            scope,
            login_in: who.iter().map(|s| s.map(str::to_string)).collect(),
            project: project.map(str::to_string),
        };
    assert_eq!(
        got,
        vec![
            need("notes", "user", &[Some("work")], None),
            need("tracker", "project", &[None], Some("/p/app")),
            need("db", "local", &[Some("side")], Some("/p/app")),
        ]
    );
}

/// 逐号读出来的同一服务器（同 scope 同名同配置、出自两个号的家）在需手动清单里只出一项，要登录的号并起来；
/// 配置不一样的照实各出一项。
#[test]
fn one_server_read_from_two_homes_is_one_need() {
    let from = |home: &str, scope, name: &str, url: &str, who: &[Option<&str>]| McpEntry {
        server: serde_json::json!({"type": "http", "url": url}),
        source: format!("/h/{home}/.claude.json"),
        ..need_entry(scope, name, McpStatus::NeedsLogin, who)
    };
    let user = need_read(vec![
        from("a", "user", "notes", "http://n", &[Some("a")]),
        from("b", "user", "notes", "http://n", &[Some("b")]),
        from("a", "user", "db", "http://db-a", &[Some("a")]),
        from("b", "user", "db", "http://db-b", &[Some("b")]),
    ]);
    let proj = need_read(vec![
        from("a", "local", "wiki", "http://w", &[Some("a")]),
        from("b", "local", "wiki", "http://w", &[Some("b")]),
    ]);
    let got = needs_login_of(&[(None, user), (Some("/p/app".into()), proj)]);
    let need =
        |name: &str, scope: &'static str, who: &[Option<&str>], project: Option<&str>| McpNeed {
            name: name.into(),
            scope,
            login_in: who.iter().map(|s| s.map(str::to_string)).collect(),
            project: project.map(str::to_string),
        };
    assert_eq!(
        got,
        vec![
            need("notes", "user", &[Some("a"), Some("b")], None),
            need("db", "user", &[Some("a")], None),
            need("db", "user", &[Some("b")], None),
            need("wiki", "local", &[Some("a"), Some("b")], Some("/p/app")),
        ]
    );
}

/// 没设账号那一份要登录、与号 a 那一份同名同配置合成一条：需手动清单里那一项要登录的两位都在，没设账号那一位在前。
#[test]
fn the_bare_need_survives_merging_with_an_account() {
    let from = |source: &str, who: &[Option<&str>]| McpEntry {
        server: serde_json::json!({"type": "http", "url": "http://n"}),
        source: source.to_string(),
        ..need_entry("user", "notes", McpStatus::NeedsLogin, who)
    };
    let user = need_read(vec![
        from("/h/.claude.json", &[None]),
        from("/h/a/.claude.json", &[Some("a")]),
    ]);
    let got = needs_login_of(&[(None, user)]);
    assert_eq!(
        got,
        vec![McpNeed {
            name: "notes".into(),
            scope: "user",
            login_in: vec![None, Some("a".to_string())],
            project: None,
        }]
    );
}

// ───────── 按家走：替身家声明一种行式的非 JSON 配置（`fake-mcp.conf`），两家各有号 ─────────

/// 两家替身：`fake`（默认起的那一家，住共享集合顶层）· `fake2`；号的 MCP 那一格都是假 agent 那一份（行式）。
fn fake_families() -> [crate::agents::Adapter; 2] {
    let fam = |kind, env, wrapper, default| {
        crate::agents::fake::family_row(kind, env, wrapper, default, || None)
    };
    [
        fam("fake", "CCM_FAKE_A", "fakea", true),
        fam("fake2", "CCM_FAKE_B", "fakeb", false),
    ]
}

fn fake_conf(acct: &str) -> String {
    format!(
        ".cc-monitor/accounts/{acct}/{}",
        crate::agents::fake::MCP_FILE
    )
}

/// 账号库清单：a1 · a2 是 `fake` 的号，b1 是 `fake2` 的号（号目录建好，清单直接落盘）。
fn fake_library(tag: &str) -> Tmp {
    let t = tmp(tag);
    let home = t.0.display().to_string();
    let row = |name: &str, agent: &str| {
        json!({
            "name": name,
            "email": format!("{name}@example.test"),
            "configDir": format!("{home}/.cc-monitor/accounts/{name}"),
            "isDefault": name == "a1",
            "agent": agent,
        })
    };
    for n in ["a1", "a2", "b1"] {
        std::fs::create_dir_all(t.0.join(format!(".cc-monitor/accounts/{n}"))).unwrap();
    }
    t.write(
        &format!(
            "{}/{}",
            relay_route_core::ACCOUNTS_DIR_REL,
            relay_route_core::ACCOUNTS_MANIFEST_NAME
        ),
        &serde_json::to_string_pretty(&json!({
            "version": acct_core::SUPPORTED_SCHEMA,
            "accounts": [row("a1", "fake"), row("a2", "fake"), row("b1", "fake2")],
        }))
        .unwrap(),
    );
    t
}

#[test]
fn a_family_with_its_own_line_format_syncs_one_section_and_only_among_its_own_accounts() {
    let t = fake_library("acct-mcp-fake-families");
    let reg = fake_families();
    let a1 = "# a1 自己的注释\nmode = fast\nserver cclsp {\"command\":\"cclsp\"}\n";
    let a2 = "# a2\nserver old {\"command\":\"o\"}\ncolor = red\n";
    let b1 = "server b-only {\"command\":\"b\"}\n";
    t.write(&fake_conf("a1"), a1);
    t.write(&fake_conf("a2"), a2);
    t.write(&fake_conf("b1"), b1);

    let v = sync_among(&t.door(), &reg).expect("同步一趟");
    assert!(v.conflicts.is_empty(), "{:?}", v.conflicts);
    // 同一家的两个号互相补上对方那一条，只多出那一行，别的行（注释 · 别的设置 · 写法）一个字节不变。
    assert_eq!(
        t.read(&fake_conf("a1")),
        format!("{a1}server old {{\"command\":\"o\"}}\n")
    );
    assert_eq!(
        t.read(&fake_conf("a2")),
        format!("{a2}server cclsp {{\"command\":\"cclsp\"}}\n")
    );
    // 改写之前那份原文按那一家的文件名进了备份（`<号><那一家的文件名>`）。
    assert_eq!(
        t.read(&format!(
            ".cc-monitor/backups/accounts-mcp/a2{}",
            crate::agents::fake::MCP_FILE
        )),
        a2
    );
    // 另一家的号不收这一家的，这一家的号也不收另一家的。
    assert_eq!(t.read(&fake_conf("b1")), b1);
    let store: Value = serde_json::from_str(&t.read(relay_route_core::ACCOUNTS_MCP_REL)).unwrap();
    let names = |v: &Value| -> Vec<String> {
        v.as_object()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    };
    // 服务器表那一格的键照缺省那一家资产面声明的（假家是 `servers`，不是生产里 Claude 那一个）。
    assert_eq!(names(&store["servers"]), vec!["cclsp", "old"]);
    assert_eq!(names(&store["agents"]["fake2"]["servers"]), vec!["b-only"]);

    // 在 a1 里改一条 ⇒ a2 里只换那一行。
    let a1_now = t.read(&fake_conf("a1")).replace(
        "{\"command\":\"cclsp\"}",
        "{\"args\":[\"-v\"],\"command\":\"cclsp\"}",
    );
    t.write(&fake_conf("a1"), &a1_now);
    let a2_before = t.read(&fake_conf("a2"));
    sync_among(&t.door(), &reg).expect("再同步一趟");
    assert_eq!(
        t.read(&fake_conf("a2")),
        a2_before.replace(
            "{\"command\":\"cclsp\"}",
            "{\"args\":[\"-v\"],\"command\":\"cclsp\"}"
        )
    );
    assert_eq!(t.read(&fake_conf("b1")), b1);
}

#[test]
fn a_family_without_shareable_logins_is_skipped_silently_every_round() {
    let t = fake_library("acct-mcp-fake-logins");
    // 两家替身都不声明 MCP 登录那一格。
    let reg = fake_families().map(|a| crate::agents::Adapter {
        accounts: a.accounts.map(|f| crate::agents::AccountsFace {
            mcp_logins: None,
            ..f
        }),
        ..a
    });
    let own = crate::platform::fs::Own::issue_data_at(&t.0.join(".cc-monitor"));
    let before = crate::platform::fs::Ro::read_dir(t.0.join(".cc-monitor/accounts/a1"))
        .unwrap()
        .count();
    // 判定那一处：没声明的两家不同步登录；声明了的（替身原样 · 生产里 Claude 那一家）同步。
    use crate::accounts::manage::mcp_logins_exec::logins_shared;
    assert!(logins_shared(&reg, "fake").is_none() && logins_shared(&reg, "fake2").is_none());
    assert!(logins_shared(&fake_families(), "fake").is_some());
    assert!(logins_shared(
        crate::agents::REGISTRY,
        crate::agents::claudecode::AGENT_KIND
    )
    .is_some());
    // 两家都没声明 MCP 登录那一格：连跑三趟，一句都不说、什么都不改。
    for round in 1..=3 {
        let got = crate::accounts::manage::mcp_logins_exec::sync_among(&t.door(), &own, &reg)
            .expect("登录那一半一趟");
        assert_eq!(got, Default::default(), "第 {round} 趟");
    }
    // 后台监听那一下连跑三趟，日志里也没有一行。
    let heard = test_meters::heard(|| {
        for _ in 0..3 {
            crate::accounts::manage::mcp_share_watch::logins_once(&t.door(), &own, &reg);
        }
    });
    assert!(heard.is_empty(), "{heard:?}");
    assert_eq!(
        crate::platform::fs::Ro::read_dir(t.0.join(".cc-monitor/accounts/a1"))
            .unwrap()
            .count(),
        before,
        "号目录里一份都没多"
    );
}

/// 账号页读共享集合不隐含缺省那一家：两家各在共享集合里写一个服务器 ⇒ 两条都在，第二家那条带家（缺省那一家不写，同 `mcp-read`）。
#[test]
fn reading_the_shared_set_lists_every_family_and_says_each_family_only_when_several() {
    let t = fake_library("acct-mcp-read-families");
    t.write(
        relay_route_core::ACCOUNTS_MCP_REL,
        &serde_json::to_string_pretty(&json!({
            "version": 1,
            "servers": { "a-only": { "command": "a" } },
            "base": {},
            "agents": { "fake2": { "servers": { "b-only": { "command": "b" } }, "base": {} } },
        }))
        .unwrap(),
    );
    let v = read_among(&t.door(), &fake_families()).expect("读得出");
    // 不止一家 ⇒ 每条带上那一家对人的叫法（缺家的那条是缺省那一家），界面照抄、不自己判缺的是谁。
    let said = crate::agents::fake::said_of;
    assert_eq!(
        serde_json::to_value(&v.servers).unwrap(),
        json!([
            { "name": "a-only", "agentSaid": said("fake") },
            { "name": "b-only", "agent": "fake2", "agentSaid": said("fake2") },
        ])
    );
    // 只剩一家有条目 ⇒ 不写叫法（界面不画家）。
    t.write(
        relay_route_core::ACCOUNTS_MCP_REL,
        &serde_json::to_string_pretty(&json!({
            "version": 1,
            "servers": {},
            "base": {},
            "agents": { "fake2": { "servers": { "b-only": { "command": "b" } }, "base": {} } },
        }))
        .unwrap(),
    );
    let v = read_among(&t.door(), &fake_families()).expect("读得出");
    assert_eq!(
        serde_json::to_value(&v.servers).unwrap(),
        json!([{ "name": "b-only", "agent": "fake2" }])
    );
}

/// 非缺省那一家没声明 MCP 那一格、MCP 登录可同步：它的两个号之间照常同步登录，底记在共享集合 `agents.<家>` 那一格，
/// 缺省那一家的号一个字节不动。
#[test]
fn a_non_default_family_without_mcp_cells_still_shares_its_logins_under_its_own_slot() {
    let t = tmp("acct-mcp-login-second-family");
    let home = t.0.display().to_string();
    let row = |name: &str, agent: &str| {
        json!({
            "name": name,
            "email": format!("{name}@example.test"),
            "configDir": format!("{home}/.cc-monitor/accounts/{name}"),
            "isDefault": name == "a1",
            "agent": agent,
        })
    };
    for n in ["a1", "b1", "b2"] {
        std::fs::create_dir_all(t.0.join(format!(".cc-monitor/accounts/{n}"))).unwrap();
    }
    t.write(
        &format!(
            "{}/{}",
            relay_route_core::ACCOUNTS_DIR_REL,
            relay_route_core::ACCOUNTS_MANIFEST_NAME
        ),
        &serde_json::to_string_pretty(&json!({
            "version": acct_core::SUPPORTED_SCHEMA,
            "accounts": [row("a1", "fake"), row("b1", "fake2"), row("b2", "fake2")],
        }))
        .unwrap(),
    );
    let [first, second] = fake_families();
    let reg = [
        first,
        crate::agents::Adapter {
            accounts: second
                .accounts
                .map(|f| crate::agents::AccountsFace { mcp: None, ..f }),
            ..second
        },
    ];
    let logins = |acct: &str| format!(".cc-monitor/accounts/{acct}/fake-mcp-logins.conf");
    let entry = "{\"bearer\":\"at-1\",\"renewal\":\"rt-1\",\"until\":2000000000000}";
    t.write(&logins("b1"), &format!("# b1\nlogin srv|1 {entry}\n"));
    t.write(&logins("b2"), "# b2\n");
    t.write(&logins("a1"), "# a1\n");
    let own = crate::platform::fs::Own::issue_data_at(&t.0.join(".cc-monitor"));
    let v = crate::accounts::manage::mcp_logins_exec::sync_among(&t.door(), &own, &reg)
        .expect("登录那一半一趟");
    assert_eq!(v.changed, vec!["b2".to_string()], "{:?}", v.notes);
    assert_eq!(
        t.read(&logins("b2")),
        format!("# b2\nlogin srv|1 {entry}\n")
    );
    assert_eq!(t.read(&logins("a1")), "# a1\n", "缺省那一家的号不收");
    let store: Value = serde_json::from_str(&t.read(relay_route_core::ACCOUNTS_MCP_REL)).unwrap();
    let dirs = |v: &Value| -> Vec<String> {
        v.as_object()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    };
    assert_eq!(
        dirs(&store["agents"]["fake2"]["logins"]),
        vec![
            format!("{home}/.cc-monitor/accounts/b1"),
            format!("{home}/.cc-monitor/accounts/b2")
        ],
        "底记在那一家那一格"
    );
    assert!(store["logins"].is_null(), "缺省那一格不记别家的底");
}

/// 判 MCP 状态看的家只收这一家的号：两家各有号 ⇒ 第二家只看到它自己那个号的目录（别家号里的活会话不算它的）。
#[test]
fn the_mcp_look_of_a_family_holds_only_its_own_accounts() {
    let t = fake_library("acct-mcp-look-family");
    let home = t.0.display().to_string();
    let m = manifest_in(&home).expect("读得出").expect("有账号库");
    let reg = fake_families();
    let dirs = |kind| -> Vec<(Option<String>, PathBuf)> { account_homes(&m, &reg, kind) };
    let at = |n: &str| {
        (
            Some(n.to_string()),
            PathBuf::from(format!("{home}/.cc-monitor/accounts/{n}")),
        )
    };
    assert_eq!(dirs("fake2"), vec![at("b1")]);
    assert_eq!(dirs("fake"), vec![at("a1"), at("a2")]);
}

// ───────── 账号页的删 · 挑照条目上的家：只动那一家那一格 ─────────

/// 两家各两个号（a1 · a2 是 `fake` 的，b1 · b2 是 `fake2` 的），四个号里都有同名的 `s`，同步过一趟（两格的底都记下了）。
fn two_families_with_s(tag: &str) -> Tmp {
    let t = tmp(tag);
    let home = t.0.display().to_string();
    let row = |name: &str, agent: &str| {
        json!({
            "name": name,
            "email": format!("{name}@example.test"),
            "configDir": format!("{home}/.cc-monitor/accounts/{name}"),
            "isDefault": name == "a1",
            "agent": agent,
        })
    };
    for n in ["a1", "a2", "b1", "b2"] {
        std::fs::create_dir_all(t.0.join(format!(".cc-monitor/accounts/{n}"))).unwrap();
    }
    t.write(
        &format!(
            "{}/{}",
            relay_route_core::ACCOUNTS_DIR_REL,
            relay_route_core::ACCOUNTS_MANIFEST_NAME
        ),
        &serde_json::to_string_pretty(&json!({
            "version": acct_core::SUPPORTED_SCHEMA,
            "accounts": [row("a1", "fake"), row("a2", "fake"), row("b1", "fake2"), row("b2", "fake2")],
        }))
        .unwrap(),
    );
    for n in ["a1", "a2", "b1", "b2"] {
        t.write(
            &fake_conf(n),
            &format!("# {n}\nserver s {{\"command\":\"s\"}}\nserver k {{\"command\":\"k\"}}\n"),
        );
    }
    let v = sync_among(&t.door(), &fake_families()).expect("同步一趟");
    assert!(v.conflicts.is_empty() && v.changed.is_empty(), "{v:?}");
    t
}

fn code(
    r: Result<cmd_wire::accounts::AccountMcpView, crate::common::said::Said>,
) -> Option<String> {
    r.err().and_then(|e| e.code)
}

#[test]
fn removing_from_the_second_family_leaves_the_default_ones_slot_as_it_was() {
    let t = two_families_with_s("acct-mcp-rm-second");
    let reg = fake_families();
    let (a1, a2) = (t.read(&fake_conf("a1")), t.read(&fake_conf("a2")));
    let store_before: Value =
        serde_json::from_str(&t.read(relay_route_core::ACCOUNTS_MCP_REL)).unwrap();
    let v = remove_among(&t.door(), &reg, "s", Some("fake2")).expect("删得了");
    // 回的是完整成品：两家的条目都在、各带那一家的叫法（同 `read`），不只删的那一家。
    let said = crate::agents::fake::said_of;
    assert_eq!(
        serde_json::to_value(&v.servers).unwrap(),
        json!([
            { "name": "k", "agentSaid": said("fake") },
            { "name": "s", "agentSaid": said("fake") },
            { "name": "k", "agent": "fake2", "agentSaid": said("fake2") },
        ])
    );
    let again = read_among(&t.door(), &reg).expect("读得出");
    assert_eq!(
        (&v.servers, &v.conflicts),
        (&again.servers, &again.conflicts),
        "删之后回的与再读一遍的不同"
    );
    for b in ["b1", "b2"] {
        assert_eq!(
            t.read(&fake_conf(b)),
            format!("# {b}\nserver k {{\"command\":\"k\"}}\n")
        );
    }
    assert_eq!(
        (t.read(&fake_conf("a1")), t.read(&fake_conf("a2"))),
        (a1, a2)
    );
    let store: Value = serde_json::from_str(&t.read(relay_route_core::ACCOUNTS_MCP_REL)).unwrap();
    assert_eq!(
        store["servers"], store_before["servers"],
        "缺省那一家那一格一字不变"
    );
    assert_eq!(store["base"], store_before["base"]);
}

#[test]
fn without_an_agent_removing_still_acts_on_the_default_family() {
    let t = two_families_with_s("acct-mcp-rm-default");
    let reg = fake_families();
    let (b1, b2) = (t.read(&fake_conf("b1")), t.read(&fake_conf("b2")));
    remove_among(&t.door(), &reg, "s", None).expect("删得了");
    for a in ["a1", "a2"] {
        assert_eq!(
            t.read(&fake_conf(a)),
            format!("# {a}\nserver k {{\"command\":\"k\"}}\n")
        );
    }
    assert_eq!(
        (t.read(&fake_conf("b1")), t.read(&fake_conf("b2"))),
        (b1, b2)
    );
}

#[test]
fn picking_in_the_second_family_settles_only_its_conflict() {
    let t = two_families_with_s("acct-mcp-pick-second");
    let reg = fake_families();
    for (n, arg) in [("a1", "a1"), ("a2", "a2"), ("b1", "b1"), ("b2", "b2")] {
        let now = t.read(&fake_conf(n)).replace(
            "{\"command\":\"s\"}",
            &format!("{{\"args\":[\"{arg}\"],\"command\":\"s\"}}"),
        );
        t.write(&fake_conf(n), &now);
    }
    let v = sync_among(&t.door(), &reg).expect("同步一趟");
    assert_eq!(v.conflicts.len(), 2, "两家各一条冲突：{:?}", v.conflicts);
    let (a1, a2) = (t.read(&fake_conf("a1")), t.read(&fake_conf("a2")));
    let after = pick_among(&t.door(), &reg, "s", Some("b1"), Some("fake2")).expect("挑得了");
    // 回的是完整成品：挑完的那一家那一条没了，缺省那一家那一条还在等挑（带它的叫法）；改了的号只是这一趟落下的。
    let said = crate::agents::fake::said_of;
    assert_eq!(
        after
            .conflicts
            .iter()
            .map(|c| (c.agent.clone(), c.agent_said.clone()))
            .collect::<Vec<_>>(),
        vec![(None, Some(said("fake").to_string()))],
        "{:?}",
        after.conflicts
    );
    assert!(
        after.servers.iter().any(|s| s.agent.is_none()),
        "缺省那一家的条目不在回包里"
    );
    assert_eq!(after.changed, vec!["b2".to_string()]);
    assert!(t
        .read(&fake_conf("b2"))
        .contains("{\"args\":[\"b1\"],\"command\":\"s\"}"));
    assert_eq!(
        (t.read(&fake_conf("a1")), t.read(&fake_conf("a2"))),
        (a1, a2)
    );
    let left = read_among(&t.door(), &reg).expect("读得出");
    assert_eq!(left.conflicts.len(), 1);
    assert_eq!(left.conflicts[0].agent, None, "缺省那一家那一条还在等挑");
}

#[test]
fn an_unknown_family_is_bad_args_not_the_default_one() {
    let t = two_families_with_s("acct-mcp-unknown-family");
    let reg = fake_families();
    let before: Vec<String> = ["a1", "a2", "b1", "b2"]
        .iter()
        .map(|n| t.read(&fake_conf(n)))
        .collect();
    assert_eq!(
        code(remove_among(&t.door(), &reg, "s", Some("nobody"))).as_deref(),
        Some("bad_args")
    );
    assert_eq!(
        code(pick_among(&t.door(), &reg, "s", None, Some("nobody"))).as_deref(),
        Some("bad_args")
    );
    let after: Vec<String> = ["a1", "a2", "b1", "b2"]
        .iter()
        .map(|n| t.read(&fake_conf(n)))
        .collect();
    assert_eq!(after, before);
}

#[test]
fn the_sync_log_says_which_family_each_line_is_about() {
    let t = two_families_with_s("acct-mcp-log-family");
    let reg = fake_families();
    // a1 加一条 ⇒ a2 跟上（改写那一行）；b1 · b2 各改 s ⇒ 冲突那一行。
    let a1 = t.read(&fake_conf("a1")) + "server n {\"command\":\"n\"}\n";
    t.write(&fake_conf("a1"), &a1);
    for n in ["b1", "b2"] {
        let now = t.read(&fake_conf(n)).replace(
            "{\"command\":\"s\"}",
            &format!("{{\"args\":[\"{n}\"],\"command\":\"s\"}}"),
        );
        t.write(&fake_conf(n), &now);
    }
    let heard = test_meters::heard(|| {
        crate::accounts::manage::mcp_share_watch::share_once(&t.door(), &reg);
    });
    let changed = heard
        .iter()
        .find(|l| l.contains("a2"))
        .unwrap_or_else(|| panic!("{heard:?}"));
    assert!(
        changed.contains("fake") && !changed.contains("fake2"),
        "{changed}"
    );
    let conflict = heard
        .iter()
        .find(|l| l.contains(" s ") || l.contains("s 两边"))
        .unwrap_or_else(|| panic!("{heard:?}"));
    assert!(conflict.contains("fake2"), "{conflict}");
}

// ───────── 删 · 挑落盘之后再读别家失败：落盘那一笔照成，读不到的那一家照实说 ─────────

/// 共享集合落盘之后家目录里就有东西被改坏的门（别的进程抢着动了那一份）：写共享集合那一下照常落，落完改坏 [`Spoil`] 那一样；
/// 别的问原样交 [`HomeDoor`]。第三格记共享集合落过没有。
struct SpoilsStoreAfterPut(HomeDoor, Spoil, std::cell::Cell<bool>);

/// 落盘之后改坏的那一样：家目录里的一份文件（交家目录）· 问家目录本身就报错。
#[derive(Clone, Copy)]
enum Spoil {
    Files(fn(&std::path::Path)),
    HomeUnread,
}

fn spoiling(d: HomeDoor, s: Spoil) -> SpoilsStoreAfterPut {
    SpoilsStoreAfterPut(d, s, std::cell::Cell::new(false))
}

/// 共享集合换成解析不了的一份。
fn store_spoiled(home: &std::path::Path) {
    std::fs::write(home.join(relay_route_core::ACCOUNTS_MCP_REL), "{ not json").unwrap();
}

/// 账号清单那份文件没了（重读读成「没有账号库」）。
fn manifest_gone(home: &std::path::Path) {
    std::fs::remove_file(manifest_path(home)).unwrap();
}

/// 账号清单换成解析不了的一份（重读读成错）。
fn manifest_spoiled(home: &std::path::Path) {
    std::fs::write(manifest_path(home), "{ not json").unwrap();
}

fn manifest_path(home: &std::path::Path) -> PathBuf {
    home.join(relay_route_core::ACCOUNTS_DIR_REL)
        .join(relay_route_core::ACCOUNTS_MANIFEST_NAME)
}

impl Door for SpoilsStoreAfterPut {
    fn home(&self, r: cmd_wire::files::HomeReq) -> Result<cmd_wire::files::FilesPathReply, Said> {
        if self.2.get() && matches!(self.1, Spoil::HomeUnread) {
            return Err(Said::new(
                "io_failed",
                "fixture: home unreadable".to_string(),
            ));
        }
        self.0.home(r)
    }
    fn stat(&self, r: cmd_wire::files::StatReq) -> Result<cmd_wire::files::StatReply, Said> {
        self.0.stat(r)
    }
    fn ls(&self, r: cmd_wire::files::LsReq) -> Result<cmd_wire::files::LsReply, Said> {
        self.0.ls(r)
    }
    fn peek(
        &self,
        r: cmd_wire::files::FilesPeekReq,
    ) -> Result<cmd_wire::files::FilesPeekReply, Said> {
        self.0.peek(r)
    }
    fn put(&self, r: cmd_wire::files::FilesPutReq) -> Result<cmd_wire::files::FilesPutReply, Said> {
        let store = String::from_utf8_lossy(r.rel.bytes())
            .ends_with(relay_route_core::ACCOUNTS_MCP_REL)
            && !r.content.bytes().is_empty();
        let got = self.0.put(r)?;
        if store {
            self.2.set(true);
            if let Spoil::Files(spoil) = self.1 {
                spoil(&self.0 .0);
            }
        }
        Ok(got)
    }
    fn delete(
        &self,
        r: cmd_wire::files::FilesDeleteReq,
    ) -> Result<cmd_wire::batch::BatchReply, Said> {
        self.0.delete(r)
    }
    fn copy(&self, r: cmd_wire::files::FilesCopyReq) -> Result<cmd_wire::batch::BatchReply, Said> {
        self.0.copy(r)
    }
    fn mkdir(
        &self,
        r: cmd_wire::files::FilesMkdirReq,
    ) -> Result<cmd_wire::files::FilesPathReply, Said> {
        self.0.mkdir(r)
    }
    fn link(
        &self,
        r: cmd_wire::files::FilesLinkReq,
    ) -> Result<cmd_wire::files::FilesPathReply, Said> {
        self.0.link(r)
    }
    fn rename_to(
        &self,
        r: cmd_wire::files::FilesRenameReq,
    ) -> Result<cmd_wire::files::FilesPathReply, Said> {
        self.0.rename_to(r)
    }
    fn chmod(
        &self,
        r: cmd_wire::files::FilesChmodReq,
    ) -> Result<cmd_wire::batch::BatchReply, Said> {
        self.0.chmod(r)
    }
}

/// 落盘之后被改坏的四样：共享集合解析不了 · 账号清单没了 · 账号清单解析不了 · 重读家目录本身出错。
const SPOILS: &[(&str, Spoil)] = &[
    ("共享集合解析不了", Spoil::Files(store_spoiled)),
    ("账号清单没了", Spoil::Files(manifest_gone)),
    ("账号清单解析不了", Spoil::Files(manifest_spoiled)),
    ("家目录读不到", Spoil::HomeUnread),
];

/// 读不到的那一家照实说的那一句：经改坏之后的门只读一问，照样整笔报的那一句（只读没有落盘的那一笔可交）；
/// 只读那一问读成「没有账号库」（清单没了）⇒ 说没有账号库。
fn unread_note(d: &SpoilsStoreAfterPut, reg: &[crate::agents::Adapter]) -> String {
    let why = match read_among(d, reg) {
        Err(e) => e.said,
        Ok(v) => {
            assert_eq!(v, Default::default(), "夹具没把家目录改坏");
            copy_core::copy_text("beAcctMcpShare.sync.noLibrary", &[])
        }
    };
    let unread = copy_core::copy_text(
        "beAcctMcpShare.note.familyUnread",
        &[
            ("agent", crate::agents::fake::said_of("fake")),
            ("why", &why),
        ],
    );
    assert!(unread.contains(&why), "文案表里没有那一句：{unread}");
    unread
}

/// 删第二家那一条、落盘之后重读别家读不出（[`SPOILS`] 每一样）⇒ 不报整笔失败，也不丢掉刚删的那一家：回删的那一家这一趟的条目，
/// 号里真删了；读不到的缺省那一家照实一句备注（它的叫法 ＋ 读不出的那一句），不当成删失败。
#[test]
fn after_removing_an_unreadable_other_family_is_said_not_failed() {
    for (what, spoil) in SPOILS {
        let t = two_families_with_s("acct-mcp-rm-other-unread");
        let reg = fake_families();
        let d = spoiling(t.door(), *spoil);
        let v = remove_among(&d, &reg, "s", Some("fake2"))
            .unwrap_or_else(|e| panic!("{what}：落盘那一笔照成，却整笔报了 {e:?}"));
        assert_eq!(
            serde_json::to_value(&v.servers).unwrap(),
            json!([{ "name": "k", "agent": "fake2" }]),
            "{what}：删的那一家这一趟的条目"
        );
        for b in ["b1", "b2"] {
            assert_eq!(
                t.read(&fake_conf(b)),
                format!("# {b}\nserver k {{\"command\":\"k\"}}\n"),
                "{what}：号里没删"
            );
        }
        assert_eq!(v.notes, vec![unread_note(&d, &reg)], "{what}");
    }
}

/// 两家的号各把 `s` 改成自己那一版：每一家各一条冲突。
fn two_families_in_conflict(tag: &str) -> Tmp {
    let t = two_families_with_s(tag);
    for n in ["a1", "a2", "b1", "b2"] {
        let now = t.read(&fake_conf(n)).replace(
            "{\"command\":\"s\"}",
            &format!("{{\"args\":[\"{n}\"],\"command\":\"s\"}}"),
        );
        t.write(&fake_conf(n), &now);
    }
    let v = sync_among(&t.door(), &fake_families()).expect("同步一趟");
    assert_eq!(v.conflicts.len(), 2, "两家各一条冲突：{:?}", v.conflicts);
    t
}

/// 挑一版同删一条：第二家挑 b1 那一版、落盘之后重读别家读不出（[`SPOILS`] 每一样）⇒ 回挑的那一家这一趟的成品
/// （它那一条冲突了结、b2 改成 b1 那一版、改了的号只有 b2），读不到的缺省那一家照实一句备注。
#[test]
fn after_picking_an_unreadable_other_family_is_said_not_failed() {
    for (what, spoil) in SPOILS {
        let t = two_families_in_conflict("acct-mcp-pick-other-unread");
        let reg = fake_families();
        let (a1, a2) = (t.read(&fake_conf("a1")), t.read(&fake_conf("a2")));
        let d = spoiling(t.door(), *spoil);
        let v = pick_among(&d, &reg, "s", Some("b1"), Some("fake2"))
            .unwrap_or_else(|e| panic!("{what}：落盘那一笔照成，却整笔报了 {e:?}"));
        assert!(v.conflicts.is_empty(), "{what}：{:?}", v.conflicts);
        assert_eq!(v.changed, vec!["b2".to_string()], "{what}");
        assert!(
            v.servers
                .iter()
                .all(|s| s.agent.as_deref() == Some("fake2")),
            "{what}：读不到的那一家不列条目：{:?}",
            v.servers
        );
        assert!(
            t.read(&fake_conf("b2"))
                .contains("{\"args\":[\"b1\"],\"command\":\"s\"}"),
            "{what}：b2 没改成 b1 那一版"
        );
        assert_eq!(
            (t.read(&fake_conf("a1")), t.read(&fake_conf("a2"))),
            (a1, a2),
            "{what}：缺省那一家的号不动"
        );
        assert_eq!(v.notes, vec![unread_note(&d, &reg)], "{what}");
    }
}

/// 家名里有非 UTF-8 的字节（替身家）⇒ 账号库清单照那个家原样读（不经字符串转一道、读成另一个目录读丢）；空家照实拒、不去根上读。
#[cfg(unix)]
#[test]
fn the_manifest_is_read_from_a_non_utf8_home_as_it_is() {
    use std::os::unix::ffi::OsStrExt as _;
    let t =
        Tmp(test_home::scratch("acct-mcp-bytes").join(std::ffi::OsStr::from_bytes(b"h\xffome")));
    t.write(
        &format!(
            "{}/{}",
            relay_route_core::ACCOUNTS_DIR_REL,
            relay_route_core::ACCOUNTS_MANIFEST_NAME
        ),
        &serde_json::to_string_pretty(&json!({
            "version": acct_core::SUPPORTED_SCHEMA,
            "accounts": [{"name": "a1", "email": "", "configDir": "/x/a1", "isDefault": true}],
        }))
        .unwrap(),
    );
    let m = manifest_in(&t.0).expect("读得出").expect("有账号库");
    assert_eq!(
        m.managed().map(|a| a.name.as_str()).collect::<Vec<_>>(),
        ["a1"]
    );
    assert!(
        manifest_in(std::path::Path::new("")).is_err(),
        "空家拿根目录兜底去读了"
    );
}

/// 清单里号 b 的号目录指到账号库外（手改清单 · 旧的就地号）：同步 MCP 不写那一份（用户的文件用户自己改），一个字节不变；库里的号照常同步。
#[test]
fn an_account_dir_outside_the_library_is_never_written() {
    let t = two("mcp-outside");
    let own = format!("{}/.own-b", t.0.display());
    t.write(".own-b/.claude.json", &t.read(&cfg("b")));
    let list = ".cc-monitor/accounts/accounts.json";
    let lib = t.read(list);
    let inb = format!("{}/.cc-monitor/accounts/b", t.0.display());
    assert!(lib.contains(&inb), "清单里 b 的号目录：{lib}");
    t.write(list, &lib.replace(&inb, &own));
    let own_before = t.read(".own-b/.claude.json");
    t.write(
        &cfg("z"),
        &claude_json(
            "z@example.test",
            &add_to(&cclsp(), "anysearch", anysearch()),
            8,
        ),
    );
    let v = sync(&t.door()).unwrap();
    assert!(!v.changed.contains(&"b".to_string()), "{:?}", v.changed);
    assert_eq!(
        t.read(".own-b/.claude.json"),
        own_before,
        "同步写了库外那一份"
    );
}
