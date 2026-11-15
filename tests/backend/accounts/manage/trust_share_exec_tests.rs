//! 信任在各号之间同步 ＋ 起会话前预标，走一遍真文件系统：临时家目录里经帧面建库（z）、加号（b · c），
//! 再在某个号 / 家目录下那一份里信任一个目录，看各号的配置落成了什么；家目录下那一份一个字节不变。
//!
//! 夹具只造结构：登录状态是写死的假邮箱，MCP 是假的。
#![cfg(unix)]

use crate::accounts::manage::trust_share_exec::*;
use crate::accounts::upstream_select::file_face;
use crate::assets::aliases::test_support::HomeDoor;
use crate::faces::accounts_face::{test_support::answer, KeyDoor};
use serde_json::{json, Value};
use sha2::Digest;
use std::path::PathBuf;

/// 一份只有这几个具名号的清单（名字 ⇒ 号目录）。
fn library(rows: &[(&str, String)]) -> crate::accounts::manage::model::Manifest {
    use crate::accounts::manage::model::{Account, Entry, Manifest};
    Manifest {
        entries: rows
            .iter()
            .map(|(name, dir)| {
                Entry::Managed(Account {
                    name: (*name).to_string(),
                    email: String::new(),
                    config_dir: dir.clone(),
                    is_default: false,
                    auth_kind: None,
                    color: None,
                    agent: None,
                    extra: Default::default(),
                })
            })
            .collect(),
        ..Manifest::default()
    }
}

struct Tmp(PathBuf);
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn tmp(tag: &str) -> Tmp {
    Tmp(test_home::scratch(tag).canonicalize().unwrap())
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
        ".cc-monitor/accounts/a/fake-config.json",
        "{\n  \"keep\": [1],\n  \"trusted\": {\n    \"/a\": true\n  }\n}\n",
    );
    t.write(".cc-monitor/accounts/b/fake-config.json", "{\"keep\": 2}\n");
    let list = library(&[
        ("a", format!("{}/.cc-monitor/accounts/a", t.home())),
        ("b", format!("{}/.cc-monitor/accounts/b", t.home())),
    ]);
    let home_before = sha(&t, "fake-config.json");
    let zero = format!("{}/fake-config.json", t.home());
    let mut changed = sync_with(
        &t.door(),
        &t.home(),
        &cells,
        "fake-config.json",
        &zero,
        &list,
    );
    changed.sort();
    assert_eq!(changed, ["a", "b"]);
    assert_eq!(
        t.value(".cc-monitor/accounts/a/fake-config.json"),
        json!({ "keep": [1], "trusted": { "/a": true, "/h": true } })
    );
    assert_eq!(
        t.value(".cc-monitor/accounts/b/fake-config.json"),
        json!({ "keep": 2, "trusted": { "/a": true, "/h": true } })
    );
    assert_eq!(sha(&t, "fake-config.json"), home_before);
    assert!(sync_with(
        &t.door(),
        &t.home(),
        &cells,
        "fake-config.json",
        &zero,
        &list
    )
    .is_empty());

    assert_eq!(
        pretrust_with(
            &t.door(),
            &t.home(),
            &cells,
            "fake-config.json",
            &list,
            &format!("{}/.cc-monitor/accounts/b", t.home()),
            "/p"
        )
        .unwrap(),
        Marked::Wrote
    );
    assert_eq!(
        t.value(".cc-monitor/accounts/b/fake-config.json")["trusted"]["/p"],
        json!(true)
    );
    assert_eq!(
        pretrust_with(
            &t.door(),
            &t.home(),
            &cells,
            "fake-config.json",
            &list,
            &format!("{}/.cc-monitor/accounts/c", t.home()),
            "/p"
        )
        .unwrap(),
        Marked::NotListed
    );
}

/// 两个替身家各两个号：`fa` 记 JSON（`fake-config.json`，账号 0 那一份照身份表缺席住它的配置目录 `shared/`）·
/// `fb` 记行式（`fake.conf`，身份表说它的原生根是家目录 ⇒ 账号 0 那一份住 `$HOME`）。同步一趟：各家只在自家号之间并、
/// 只补自家号缺的（别家信任过的目录不进、别家的文件不在自家号目录里冒出来），两家的账号 0 那一份一字不变。
#[test]
fn each_family_syncs_only_among_its_own_accounts_and_never_writes_account_zero() {
    use crate::accounts::manage::model::{Account, Entry, Manifest};
    let t = tmp("families");
    let fa = crate::agents::fake::family_row("fa", "CCM_FA_DIR", "fa", true, || None);
    let fb_face = crate::agents::AccountsFace {
        identity: &[(
            "fake.conf",
            crate::agents::IdentityRoot::Home,
            crate::agents::IdentityClass::State,
        )],
        config_file: "fake.conf",
        trust: Some(crate::agents::fake::LINE_TRUST_CELLS),
        ..fa.accounts.expect("替身家有账号面")
    };
    let fb = crate::agents::Adapter {
        accounts: Some(fb_face),
        ..crate::agents::fake::family_row("fb", "CCM_FB_DIR", "fb", false, || None)
    };
    let registry = [fa, fb];
    t.write(
        "shared/fake-config.json",
        "{\"trusted\": {\"/a0\": true}}\n",
    );
    t.write("fake.conf", "trust /b0\n");
    t.write(
        ".cc-monitor/accounts/a1/fake-config.json",
        "{\"trusted\": {\"/a1\": true}}\n",
    );
    t.write(".cc-monitor/accounts/a2/fake-config.json", "{}\n");
    t.write(".cc-monitor/accounts/b1/fake.conf", "trust /b1\n");
    t.write(".cc-monitor/accounts/b2/fake.conf", "color = red\n");
    let row = |name: &str, kind: &str| {
        Entry::Managed(Account {
            name: name.to_string(),
            email: String::new(),
            config_dir: format!("{}/.cc-monitor/accounts/{name}", t.home()),
            is_default: false,
            auth_kind: None,
            color: None,
            agent: Some(kind.to_string()),
            extra: Default::default(),
        })
    };
    let m = Manifest {
        entries: vec![
            row("a1", "fa"),
            row("b1", "fb"),
            row("a2", "fa"),
            row("b2", "fb"),
        ],
        ..Manifest::default()
    };
    let zero_a = sha(&t, "shared/fake-config.json");
    let zero_b = sha(&t, "fake.conf");

    let mut changed = sync_among(&t.door(), &t.home(), &registry, &m);
    changed.sort();
    assert_eq!(changed, ["a1", "a2", "b1", "b2"]);
    assert_eq!(
        t.value(".cc-monitor/accounts/a1/fake-config.json"),
        json!({ "trusted": { "/a0": true, "/a1": true } })
    );
    assert_eq!(
        t.value(".cc-monitor/accounts/a2/fake-config.json"),
        json!({ "trusted": { "/a0": true, "/a1": true } })
    );
    assert_eq!(
        t.read(".cc-monitor/accounts/b1/fake.conf"),
        "trust /b1\ntrust /b0\n"
    );
    assert_eq!(
        t.read(".cc-monitor/accounts/b2/fake.conf"),
        "color = red\ntrust /b0\ntrust /b1\n"
    );
    for (a, other) in [
        ("a1", "fake.conf"),
        ("a2", "fake.conf"),
        ("b1", "fake-config.json"),
        ("b2", "fake-config.json"),
    ] {
        assert!(
            !t.0.join(".cc-monitor/accounts")
                .join(a)
                .join(other)
                .exists(),
            "{a} 号目录里冒出了别家的 {other}"
        );
    }
    assert_eq!(
        sha(&t, "shared/fake-config.json"),
        zero_a,
        "fa 的账号 0 被写了"
    );
    assert_eq!(sha(&t, "fake.conf"), zero_b, "fb 的账号 0 被写了");
    assert!(
        sync_among(&t.door(), &t.home(), &registry, &m).is_empty(),
        "第二趟还在写"
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
    t.write(".cc-monitor/accounts/a/fake-config.json", "{}\n");
    let list = library(&[("a", format!("{}/.cc-monitor/accounts/a", t.home()))]);
    let sub = format!("{}/repo/sub", t.home());
    assert_eq!(
        pretrust_with(
            &t.door(),
            &t.home(),
            &cells,
            "fake-config.json",
            &list,
            &format!("{}/.cc-monitor/accounts/a", t.home()),
            &sub
        )
        .unwrap(),
        Marked::Wrote
    );
    assert_eq!(
        t.value(".cc-monitor/accounts/a/fake-config.json"),
        json!({ "trusted": { sub: true } })
    );
}

/// 一家自己声明的改法（假 agent 的行式配置，不是 JSON）：同步只补每个号缺的那几行、本来就有的不重写；
/// 预标只多那一行，别的字节逐字不变；家目录下那一份一个字节不写。
#[test]
fn a_familys_own_trust_edit_is_used_instead_of_json() {
    let t = tmp("lines");
    let cells = crate::agents::fake::LINE_TRUST_CELLS;
    let file = "fake.conf";
    t.write(file, "# 家目录那一份\ntrust /h\n");
    let a_body = "# 号 a\ncolor = blue\ntrust /a\n  spaced = 1\n";
    t.write(&format!(".cc-monitor/accounts/a/{file}"), a_body);
    t.write(&format!(".cc-monitor/accounts/b/{file}"), "color = red");
    let list = library(&[
        ("a", format!("{}/.cc-monitor/accounts/a", t.home())),
        ("b", format!("{}/.cc-monitor/accounts/b", t.home())),
    ]);
    let home_before = sha(&t, file);
    let zero = format!("{}/{file}", t.home());
    let mut changed = sync_with(&t.door(), &t.home(), &cells, file, &zero, &list);
    changed.sort();
    assert_eq!(changed, ["a", "b"]);
    assert_eq!(
        t.read(&format!(".cc-monitor/accounts/a/{file}")),
        format!("{a_body}trust /h\n")
    );
    assert_eq!(
        t.read(&format!(".cc-monitor/accounts/b/{file}")),
        "color = red\ntrust /a\ntrust /h\n"
    );
    assert_eq!(sha(&t, file), home_before, "家目录下那一份一个字节不写");
    assert!(
        sync_with(&t.door(), &t.home(), &cells, file, &zero, &list).is_empty(),
        "都已标上 ⇒ 不重写"
    );
    let b_before = t.read(&format!(".cc-monitor/accounts/b/{file}"));
    assert_eq!(
        pretrust_with(
            &t.door(),
            &t.home(),
            &cells,
            file,
            &list,
            &format!("{}/.cc-monitor/accounts/b", t.home()),
            "/p"
        )
        .unwrap(),
        Marked::Wrote
    );
    assert_eq!(
        t.read(&format!(".cc-monitor/accounts/b/{file}")),
        format!("{b_before}trust /p\n")
    );
    // 读不懂的那一份不写。
    let bad = "!! 不是这一家的写法\n";
    t.write(&format!(".cc-monitor/accounts/b/{file}"), bad);
    assert!(pretrust_with(
        &t.door(),
        &t.home(),
        &cells,
        file,
        &list,
        &format!("{}/.cc-monitor/accounts/b", t.home()),
        "/q"
    )
    .is_err());
    assert_eq!(t.read(&format!(".cc-monitor/accounts/b/{file}")), bad);
}

/// 清单里某个号的号目录指到账号库外（用户自己的配置目录，手改清单就能写成这样）：预标不写它、照实交「不在账号库里」；
/// 同步一趟也不写它（库里的号照常补），那一份一个字节不变。
#[test]
fn a_config_dir_outside_the_library_is_never_written() {
    let t = tmp("outside");
    let cells = crate::agents::fake::TRUST_CELLS;
    let file = "fake-config.json";
    let inside = format!("{}/.cc-monitor/accounts/in", t.home());
    let own = format!("{}/.own-agent", t.home());
    t.write(
        ".cc-monitor/accounts/in/fake-config.json",
        "{\"trusted\": {\"/i\": true}}\n",
    );
    t.write(
        ".own-agent/fake-config.json",
        "{\"trusted\": {\"/o\": true}}\n",
    );
    let list = library(&[("in", inside.clone()), ("out", own.clone())]);
    let own_before = sha(&t, ".own-agent/fake-config.json");

    assert_eq!(
        pretrust_with(&t.door(), &t.home(), &cells, file, &list, &own, "/p").unwrap(),
        Marked::Outside
    );
    assert_eq!(
        sha(&t, ".own-agent/fake-config.json"),
        own_before,
        "预标写了库外那一份"
    );
    // 库外那一份读不出来（那个位置是个目录）也照样交「不在账号库里」（判在读之前，不读它）。
    let unread = format!("{}/.own-unreadable", t.home());
    std::fs::create_dir_all(t.0.join(".own-unreadable/fake-config.json")).unwrap();
    let odd = library(&[("odd", unread.clone())]);
    assert_eq!(
        pretrust_with(&t.door(), &t.home(), &cells, file, &odd, &unread, "/p").unwrap(),
        Marked::Outside
    );

    // 账号 0 信任过 /z ⇒ 库里的号照常补上；库外那一份连读也不读：它信任过的 /o 不并进库里。
    t.write(file, "{\"trusted\": {\"/z\": true}}\n");
    let zero = format!("{}/{file}", t.home());
    let changed = sync_with(&t.door(), &t.home(), &cells, file, &zero, &list);
    assert_eq!(changed, ["in"], "库外那个号不在改写之列");
    assert_eq!(
        sha(&t, ".own-agent/fake-config.json"),
        own_before,
        "同步写了库外那一份"
    );
    let got = t.read(".cc-monitor/accounts/in/fake-config.json");
    assert!(got.contains("/z") && !got.contains("/o"), "{got}");
    assert_eq!(
        pretrust_with(&t.door(), &t.home(), &cells, file, &list, &inside, "/p").unwrap(),
        Marked::Wrote,
        "库里的号照常预标"
    );

    // 需手动那一件只出库外那个号；库根本身 · 只是名字前缀相同的目录都算库外。
    let fa = [crate::agents::fake::family_row(
        "fa",
        "CCM_FA_DIR",
        "fa",
        true,
        || None,
    )];
    let accts = format!("{}/.cc-monitor/accounts", t.home());
    let names = |m| -> Vec<String> {
        outside_library_among(&t.home(), &fa, m)
            .into_iter()
            .map(|o| o.name)
            .collect()
    };
    assert_eq!(names(&list), ["out"]);
    let edge = library(&[
        ("root", format!("{accts}/")),
        ("twin", format!("{accts}-x/a")),
        ("in", inside.clone()),
    ]);
    assert_eq!(names(&edge), ["root", "twin"]);
    let o = &outside_library_among(&t.home(), &fa, &list)[0];
    assert_eq!(
        (o.dir.as_str(), o.manifest.clone(), o.accts.as_str()),
        (
            own.as_str(),
            format!("{accts}/accounts.json"),
            accts.as_str()
        )
    );
}

/// 库里那个号目录本身是个符号链接、指到库外（用户自己的配置目录）：字面上在库里，落盘却是用户那一份 ⇒ 照库外办：
/// 预标交「不在账号库里」、同步不写它，那一份一个字节不变；需手动那一件也出它。
#[test]
fn a_library_dir_that_links_outside_counts_as_outside() {
    let t = tmp("outside-link");
    let cells = crate::agents::fake::TRUST_CELLS;
    let file = "fake-config.json";
    let inside = format!("{}/.cc-monitor/accounts/in", t.home());
    let linked = format!("{}/.cc-monitor/accounts/lnk", t.home());
    t.write(
        ".cc-monitor/accounts/in/fake-config.json",
        "{\"trusted\": {\"/i\": true}}\n",
    );
    t.write(
        ".own-agent/fake-config.json",
        "{\"trusted\": {\"/o\": true}}\n",
    );
    std::os::unix::fs::symlink(t.0.join(".own-agent"), &linked).unwrap();
    let list = library(&[("in", inside.clone()), ("lnk", linked.clone())]);
    let own_before = sha(&t, ".own-agent/fake-config.json");

    assert_eq!(
        pretrust_with(&t.door(), &t.home(), &cells, file, &list, &linked, "/p").unwrap(),
        Marked::Outside
    );
    t.write(file, "{\"trusted\": {\"/z\": true}}\n");
    let zero = format!("{}/{file}", t.home());
    let changed = sync_with(&t.door(), &t.home(), &cells, file, &zero, &list);
    assert_eq!(changed, ["in"], "链到库外的那个号不在改写之列");
    assert_eq!(
        sha(&t, ".own-agent/fake-config.json"),
        own_before,
        "写了链到库外的那一份"
    );
    let fa = [crate::agents::fake::family_row(
        "fa",
        "CCM_FA_DIR",
        "fa",
        true,
        || None,
    )];
    let names: Vec<String> = outside_library_among(&t.home(), &fa, &list)
        .into_iter()
        .map(|o| o.name)
        .collect();
    assert_eq!(names, ["lnk"]);
}

/// 「哪几家要同步信任 · 账号 0 那份在哪」只判一处（[`trust_families`]）：后端生产源码里取账号 0 那份配置路径
/// （`base_config_of(`，定义不算）只在它那一处；监看要盯的目录也照它（`trust_zero_dirs`），不另判一遍。
#[test]
fn which_families_sync_trust_and_where_account_zero_lives_is_judged_once() {
    let root = guard_core::addr::backend_src_root();
    let mut hits: Vec<(String, usize)> = Vec::new();
    for (p, src) in guard_core::scan_tree!(&root, &["rs"]) {
        let prod = guard_core::production_code(&src);
        let n =
            prod.matches("base_config_of(").count() - prod.matches("fn base_config_of(").count();
        if n > 0 {
            let rel = p
                .strip_prefix(&root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            hits.push((rel, n));
        }
    }
    assert_eq!(
        hits,
        vec![("accounts/manage/trust_share_exec.rs".to_string(), 1)],
        "取账号 0 那份配置路径的不止 `trust_families` 一处 —— 改调它"
    );
}
