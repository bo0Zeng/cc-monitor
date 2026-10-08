//! 信任在各号之间同步 ＋ 起会话前预标，走一遍真文件系统：临时家目录里经帧面建库（z）、加号（b · c），
//! 再在某个号 / 家目录下那一份里信任一个目录，看各号的配置落成了什么；家目录下那一份一个字节不变。
//!
//! 夹具只造结构：登录状态是写死的假邮箱，MCP 是假的。
#![cfg(unix)]

use super::*;
use crate::accounts::upstream_select::file_face;
use crate::assets::aliases::tests::HomeDoor;
use crate::faces::accounts_face::{answer, KeyDoor};
use serde_json::{json, Value};
use sha2::Digest;
use std::path::PathBuf;

struct Tmp(PathBuf);
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn tmp(tag: &str) -> Tmp {
    let h = std::env::temp_dir().join(format!(
        "acct-trust-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|x| x.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&h).unwrap();
    Tmp(h.canonicalize().unwrap())
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
    fn value(&self, rel: &str) -> Value {
        serde_json::from_str(&self.read(rel)).unwrap()
    }
    fn door(&self) -> HomeDoor {
        HomeDoor(self.0.clone())
    }
    fn home(&self) -> String {
        self.0.display().to_string()
    }
    fn dir(&self, acct: &str) -> String {
        format!("{}/.cc-monitor/accounts/{acct}", self.home())
    }
    fn ok(&self, cmd: &str, args: Value) -> Value {
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
        answer(&self.door(), cmd, &args, &keys).unwrap_or_else(|e| panic!("{cmd} {args}：{e:?}"))
    }
    fn trusted_in(&self, rel: &str, dir: &str) -> bool {
        self.value(rel)["projects"][dir]["hasTrustDialogAccepted"] == json!(true)
    }
}

fn cfg(acct: &str) -> String {
    format!(".cc-monitor/accounts/{acct}/.claude.json")
}

/// Claude 自己写的那种排版：登录 · MCP · 别的项目若干（键不按字母序）。
fn claude_json(email: &str, projects: Value) -> String {
    let mut v = serde_json::to_string_pretty(&json!({
        "numStartups": 3,
        "oauthAccount": { "emailAddress": email, "accountUuid": "00000000-0000-0000-0000-000000000000" },
        "mcpServers": { "cclsp": { "type": "stdio", "command": "cclsp", "args": [] } },
        "projects": projects,
        "tipsHistory": { "x": 3 }
    }))
    .unwrap();
    v.push('\n');
    v
}

fn sha(t: &Tmp, rel: &str) -> Vec<u8> {
    sha2::Sha256::digest(std::fs::read(t.0.join(rel)).unwrap()).to_vec()
}

/// 三个号：z（建库时从现有登录收成默认号）· b · c；各自信任过的项目不同。家目录下那一份信任 `/w/home`。
fn three(tag: &str) -> Tmp {
    let t = tmp(tag);
    t.write(".claude/.credentials.json", "{\"fake\":\"cred\"}");
    t.write(".claude/settings.json", "{}");
    t.write(".claude.json", &claude_json("z@example.test", json!({})));
    assert_eq!(
        t.ok("accounts-init", json!({ "name": "z" }))["applied"],
        true
    );
    for n in ["b", "c"] {
        t.ok("accounts-add", json!({ "name": n, "kind": "subscription" }));
    }
    // 建库把家目录下那一份收进了 z；之后用户裸跑 Claude 又写出一份新的（账号 0）。
    t.write(
        ".claude.json",
        &claude_json(
            "zero@example.test",
            json!({ "/w/home": { "hasTrustDialogAccepted": true, "allowedTools": [] } }),
        ),
    );
    t.write(
        &cfg("z"),
        &claude_json(
            "z@example.test",
            json!({ "/w/z": { "allowedTools": ["Bash"], "hasTrustDialogAccepted": true } }),
        ),
    );
    t.write(
        &cfg("b"),
        &claude_json(
            "b@example.test",
            json!({ "/w/b": { "zeta": 1, "hasTrustDialogAccepted": false } }),
        ),
    );
    t.write(
        &cfg("c"),
        &claude_json(
            "c@example.test",
            json!({ "/w/c": { "hasTrustDialogAccepted": false } }),
        ),
    );
    t
}

/// 号 b 信任了 X ⇒ 同步之后 z · c 里 X 那一格为真，别的键逐个相等；家目录下那一份并进来，但它自己一个字节不变。
#[test]
fn a_dir_trusted_in_one_account_reaches_every_other_and_the_home_file_stays() {
    let t = three("reach");
    let mut b = t.value(&cfg("b"));
    b["projects"]["/w/x"] = json!({ "hasTrustDialogAccepted": true, "allowedTools": [] });
    t.write(
        &cfg("b"),
        &format!("{}\n", serde_json::to_string_pretty(&b).unwrap()),
    );
    let home_before = sha(&t, ".claude.json");
    let before: Vec<Value> = ["z", "b", "c"].iter().map(|a| t.value(&cfg(a))).collect();

    let mut changed = sync(&t.door()).unwrap();
    changed.sort();
    assert_eq!(changed, ["b", "c", "z"], "三个号都缺别处的几个");

    let all = ["/w/home", "/w/z", "/w/x"];
    for (i, a) in ["z", "b", "c"].iter().enumerate() {
        let mut want = before[i].clone();
        for d in all {
            match want["projects"].get_mut(d) {
                Some(e) => e["hasTrustDialogAccepted"] = json!(true),
                None => want["projects"][d] = json!({ "hasTrustDialogAccepted": true }),
            }
        }
        assert_eq!(t.value(&cfg(a)), want, "{a} 号只该多出信任那几格");
    }
    assert!(
        !t.trusted_in(&cfg("z"), "/w/b") && !t.trusted_in(&cfg("z"), "/w/c"),
        "没信任过的不传"
    );
    assert_eq!(sha(&t, ".claude.json"), home_before, "家目录下那一份被写了");
}

/// 同步自己写回去之后再来一趟：没有要写的（不回环），各号一个字节不变。
#[test]
fn the_pass_after_its_own_writes_writes_nothing() {
    let t = three("loop");
    assert!(!sync(&t.door()).unwrap().is_empty());
    let after: Vec<String> = ["z", "b", "c"].iter().map(|a| t.read(&cfg(a))).collect();
    assert!(sync(&t.door()).unwrap().is_empty(), "第二趟还在写");
    let again: Vec<String> = ["z", "b", "c"].iter().map(|a| t.read(&cfg(a))).collect();
    assert_eq!(after, again);
}

/// 预标：只写清单里那个号；清单外的目录 · 写法不安全的路径不写；已经信任过 ⇒ 不写。
#[test]
fn pretrust_marks_only_a_listed_account() {
    let t = three("pre");
    let work = t.0.join("work/proj");
    std::fs::create_dir_all(&work).unwrap();
    let cwd = work.display().to_string();
    let others: Vec<String> = ["z", "c"].iter().map(|a| t.read(&cfg(a))).collect();
    assert_eq!(
        pretrust(&t.door(), &t.dir("b"), &cwd).unwrap(),
        Marked::Wrote
    );
    assert!(t.trusted_in(&cfg("b"), &cwd));
    let again: Vec<String> = ["z", "c"].iter().map(|a| t.read(&cfg(a))).collect();
    assert_eq!(others, again, "预标只写要用的那个号（别的号由同步补）");
    assert_eq!(
        pretrust(&t.door(), &t.dir("b"), &cwd).unwrap(),
        Marked::Already
    );

    t.write("stray/.claude.json", "{}\n");
    let stray = format!("{}/stray", t.home());
    assert_eq!(
        pretrust(&t.door(), &stray, &cwd).unwrap(),
        Marked::NotListed
    );
    assert_eq!(t.read("stray/.claude.json"), "{}\n");
    assert_eq!(
        pretrust(&t.door(), &format!("{}/$(x)", t.home()), &cwd).unwrap(),
        Marked::NotListed
    );
    let home_before = sha(&t, ".claude.json");
    assert_eq!(
        pretrust(&t.door(), &t.home(), &cwd).unwrap(),
        Marked::NotListed
    );
    assert_eq!(sha(&t, ".claude.json"), home_before);
}

/// 工作目录经符号链接给 ⇒ 标的是那一家进程认的那一形（适配层的 `dir_keys`）。
#[test]
fn pretrust_uses_the_key_the_agent_itself_uses() {
    let t = three("key");
    std::fs::create_dir_all(t.0.join("real")).unwrap();
    std::os::unix::fs::symlink(t.0.join("real"), t.0.join("via")).unwrap();
    let via = format!("{}/via", t.home());
    assert_eq!(
        pretrust(&t.door(), &t.dir("c"), &via).unwrap(),
        Marked::Wrote
    );
    assert!(t.trusted_in(&cfg("c"), &format!("{}/real", t.home())));
}

/// 假适配层的格式（`fake-config.json` → `trusted[目录]`）喂同一个本体：并集照样传开、家目录下那一份照样不写。
#[test]
fn the_same_pass_runs_on_the_fake_adapters_format() {
    let t = tmp("fake");
    let cells = crate::agents::fake::TRUST_CELLS;
    t.write("fake-config.json", "{\"trusted\": {\"/h\": true}}\n");
    t.write(
        "a/fake-config.json",
        "{\n  \"keep\": [1],\n  \"trusted\": {\n    \"/a\": true\n  }\n}\n",
    );
    t.write("b/fake-config.json", "{\"keep\": 2}\n");
    let list = vec![
        ("a".to_string(), format!("{}/a", t.home())),
        ("b".to_string(), format!("{}/b", t.home())),
    ];
    let home_before = sha(&t, "fake-config.json");
    let mut changed = sync_with(&t.door(), &t.home(), &cells, "fake-config.json", &list);
    changed.sort();
    assert_eq!(changed, ["a", "b"]);
    assert_eq!(
        t.value("a/fake-config.json"),
        json!({ "keep": [1], "trusted": { "/a": true, "/h": true } })
    );
    assert_eq!(
        t.value("b/fake-config.json"),
        json!({ "keep": 2, "trusted": { "/a": true, "/h": true } })
    );
    assert_eq!(sha(&t, "fake-config.json"), home_before);
    assert!(sync_with(&t.door(), &t.home(), &cells, "fake-config.json", &list).is_empty());

    assert_eq!(
        pretrust_with(
            &t.door(),
            &t.home(),
            &cells,
            "fake-config.json",
            &list,
            &format!("{}/b", t.home()),
            "/p"
        )
        .unwrap(),
        Marked::Wrote
    );
    assert_eq!(t.value("b/fake-config.json")["trusted"]["/p"], json!(true));
    assert_eq!(
        pretrust_with(
            &t.door(),
            &t.home(),
            &cells,
            "fake-config.json",
            &list,
            &format!("{}/c", t.home()),
            "/p"
        )
        .unwrap(),
        Marked::NotListed
    );
}

/// 会话起在 git 仓的子目录里：Claude 按仓的根记信任 ⇒ 预标那个号里仓的根与工作目录两格都标上，别的键逐字相等；
/// `.git` 是文件（worktree）也认；不在仓里只标工作目录那一格；别的号 · 家目录下那一份一个字节不变。
#[test]
fn pretrust_also_marks_the_git_root_the_agent_keys_by() {
    let t = three("git");
    t.write("work/repo/.git/HEAD", "ref: refs/heads/main\n");
    t.write("work/wt/.git", "gitdir: /elsewhere/.git/worktrees/wt\n");
    for d in ["work/repo/sub/deep", "work/wt/sub", "work/plain/sub"] {
        std::fs::create_dir_all(t.0.join(d)).unwrap();
    }
    let at = |rel: &str| format!("{}/{rel}", t.home());
    let home_before = sha(&t, ".claude.json");
    let others: Vec<String> = ["z", "c"].iter().map(|a| t.read(&cfg(a))).collect();
    let cases: [(&str, &[&str]); 3] = [
        ("work/repo/sub/deep", &["work/repo/sub/deep", "work/repo"]),
        ("work/wt/sub", &["work/wt/sub", "work/wt"]),
        ("work/plain/sub", &["work/plain/sub"]),
    ];
    for (cwd, keys) in cases {
        let mut want = t.value(&cfg("b"));
        for k in keys {
            want["projects"][at(k)] = json!({ "hasTrustDialogAccepted": true });
        }
        assert_eq!(
            pretrust(&t.door(), &t.dir("b"), &at(cwd)).unwrap(),
            Marked::Wrote
        );
        assert_eq!(t.value(&cfg("b")), want, "从 {cwd} 起：该标的是 {keys:?}");
    }
    let again: Vec<String> = ["z", "c"].iter().map(|a| t.read(&cfg(a))).collect();
    assert_eq!(others, again);
    assert_eq!(sha(&t, ".claude.json"), home_before, "家目录下那一份被写了");
}

/// 通用层不认 git：假适配层只按目录本身记 ⇒ 仓的子目录里起，也只标那一格。
#[test]
fn the_generic_layer_does_not_know_about_git() {
    let t = tmp("fakegit");
    let cells = crate::agents::fake::TRUST_CELLS;
    t.write("repo/.git/HEAD", "x\n");
    std::fs::create_dir_all(t.0.join("repo/sub")).unwrap();
    t.write("a/fake-config.json", "{}\n");
    let list = vec![("a".to_string(), format!("{}/a", t.home()))];
    let sub = format!("{}/repo/sub", t.home());
    assert_eq!(
        pretrust_with(
            &t.door(),
            &t.home(),
            &cells,
            "fake-config.json",
            &list,
            &format!("{}/a", t.home()),
            &sub
        )
        .unwrap(),
        Marked::Wrote
    );
    assert_eq!(
        t.value("a/fake-config.json"),
        json!({ "trusted": { sub: true } })
    );
}
