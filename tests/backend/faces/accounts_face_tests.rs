//! 账号库那几条命令走一遍真文件系统：临时家目录里造一个 `~/.claude`，经本进程的 `files-*`（只有 `files-home` 答临时目录）
//! 建库 · 加号 · 删号 · 核对 · 修复 · 隔离 · 回滚 · 别名文件，每一条都看盘上落成了什么。
//!
//! 夹具只造结构（目录 · 链接 · 几份小 JSON），凭据是写死的假串，邮箱是 `*.test`。
#![cfg(unix)]

use super::*;
use crate::assets::aliases::tests::HomeDoor;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

struct Tmp(PathBuf);
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 这台的配置文件读出来的样子（别名清单住这里）。
fn book(t: &Tmp) -> crate::assets::aliases::profile::Book {
    crate::assets::aliases::profile::parse_book(&t.read(".cc-monitor/profiles.toml"))
}

/// 配置文件里有哪几段。
fn profile_names(t: &Tmp) -> Vec<String> {
    book(t).profiles.iter().map(|p| p.name.clone()).collect()
}

/// 那一段：基于谁 ＋ 自己写的 ccm 选项。
fn own(t: &Tmp, name: &str) -> (Option<String>, Vec<String>) {
    let b = book(t);
    let p = b.find(name).unwrap_or_else(|| panic!("没有 {name}"));
    (p.from.clone(), p.ccm_words())
}

fn sv(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

fn tmp(tag: &str) -> Tmp {
    let h = std::env::temp_dir().join(format!(
        "acct-face-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|x| x.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&h).unwrap();
    // 别名的链接指向这台的 ccm：放一个占位，链接才不是断的。
    let bin = h.join(crate::assets::aliases::links::bin_rel());
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("ccm"), "").unwrap();
    Tmp(h)
}

impl Tmp {
    fn p(&self, rel: &str) -> PathBuf {
        self.0.join(rel)
    }
    fn s(&self, rel: &str) -> String {
        self.p(rel).display().to_string()
    }
    fn write(&self, rel: &str, body: &str) {
        let p = self.p(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }
    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.p(rel)).unwrap_or_default()
    }
    fn is_link(&self, rel: &str) -> bool {
        std::fs::read_link(self.p(rel)).is_ok()
    }
    fn link_of(&self, rel: &str) -> String {
        std::fs::read_link(self.p(rel))
            .map(|t| t.display().to_string())
            .unwrap_or_default()
    }
    fn exists(&self, rel: &str) -> bool {
        self.is_link(rel) || self.p(rel).exists()
    }
    fn mode(&self, rel: &str) -> u32 {
        std::fs::metadata(self.p(rel)).unwrap().permissions().mode() & 0o777
    }
    fn chmod(&self, rel: &str, m: u32) {
        std::fs::set_permissions(self.p(rel), std::fs::Permissions::from_mode(m)).unwrap();
    }
    fn manifest(&self) -> Value {
        serde_json::from_str(&self.read(".cc-monitor/accounts/accounts.json"))
            .unwrap_or(Value::Null)
    }
}

/// 一台登录过 Claude 的机器：共享库里有身份（凭据 · 两份本机状态 · backups/）、几项共享的、几项排除的；`$HOME/.claude.json` 带登录邮箱。
fn machine(tag: &str) -> Tmp {
    let t = tmp(tag);
    t.write(".claude/.credentials.json", "{\"fake\":\"cred-d\"}");
    t.write(".claude/stats-cache.json", "{}");
    t.write(".claude/policy-limits.json", "{}");
    t.write(".claude/backups/one.json", "{}");
    t.write(".claude/skills/foo.md", "x");
    t.write(".claude/projects/p/MEMORY.md", "m");
    t.write(".claude/settings.json", "{\"theme\":\"dark\"}");
    t.write(".claude/CLAUDE.md", "c");
    t.write(".claude/accounts/keep", "k");
    t.write(".claude/settings.json.bak", "b");
    t.write(".claude/settings.json.bak-before-x", "b");
    t.write(
        ".claude.json",
        "{\"oauthAccount\":{\"emailAddress\":\"first@example.test\"},\"projects\":{}}",
    );
    t
}

type KeyLog = Mutex<Vec<Value>>;

/// 这台 key 表落在临时家目录里的那一份。
fn key_table(t: &Tmp) -> PathBuf {
    t.p(".cc-monitor/apikey-credentials.json")
}

/// key 表里现有哪几个号。
fn key_ids(t: &Tmp) -> Vec<String> {
    file_face::account_ids_at(&key_table(t)).unwrap()
}

/// 门：写 key 那一口由调用方给，删 / 放回 / 在哪都落在临时目录那份表上。
fn call_door(
    t: &Tmp,
    set: &dyn Fn(&Value) -> file_face::FileFaceAnswer,
    cmd: &str,
    args: Value,
) -> Answer {
    let table = key_table(t);
    let drop = |v: &Value| file_face::answer_drop_at(&table, v);
    let restore = |v: &Value| file_face::answer_restore_at(&table, v);
    let path = || Ok(table.clone());
    let keys = KeyDoor {
        set,
        drop: &drop,
        restore: &restore,
        path: &path,
    };
    answer(&HomeDoor(t.0.clone()), cmd, &args, &keys)
}

/// 写 key 那一口先记下入参，再真写进临时目录那份表。
fn call_with(t: &Tmp, keys: &KeyLog, cmd: &str, args: Value) -> Answer {
    let table = key_table(t);
    let set = |v: &Value| -> file_face::FileFaceAnswer {
        keys.lock().unwrap().push(v.clone());
        file_face::answer_set_at(&table, v)
    };
    call_door(t, &set, cmd, args)
}

fn call(t: &Tmp, cmd: &str, args: Value) -> Answer {
    call_with(t, &Mutex::new(Vec::new()), cmd, args)
}

fn ok(t: &Tmp, cmd: &str, args: Value) -> Value {
    call(t, cmd, args.clone()).unwrap_or_else(|e| panic!("{cmd} {args}：{e:?}"))
}

fn code(t: &Tmp, cmd: &str, args: Value) -> String {
    match call(t, cmd, args.clone()) {
        Ok(v) => panic!("{cmd} {args} 本该被拒，却成了：{v}"),
        Err(f) => f.code,
    }
}

/// 装一套：lab（从现有登录收成默认号）＋ x（订阅号，导入一份凭据）。
fn two_accounts(tag: &str) -> Tmp {
    let t = machine(tag);
    ok(&t, "accounts-init", json!({ "name": "lab" }));
    t.write("snap/cred-x.json", "{\"fake\":\"cred-x\"}");
    ok(
        &t,
        "accounts-add",
        json!({ "name": "x", "kind": "subscription", "credFile": "~/snap/cred-x.json" }),
    );
    t
}

fn checks(v: &Value, level: &str) -> Vec<String> {
    v["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["level"] == level)
        .map(|c| c["text"].as_str().unwrap().to_string())
        .collect()
}

fn verify(t: &Tmp) -> Value {
    ok(t, "accounts-verify", json!({}))
}

/// 目录下（不跟链接）每一项的 `(相对路径, 种类, 链接目标或内容)` —— 拿来比「回滚之后与之前逐项相同」。
/// 列目录借生产那一份只读原语（`platform::acct_view`），判据里不另写一份遍历。
fn tree(root: &Path) -> Vec<(String, String, String)> {
    use crate::platform::acct_view::{item, names, Item};
    let mut out = Vec::new();
    fn walk(base: &Path, p: &Path, out: &mut Vec<(String, String, String)>) {
        for n in names(p).unwrap_or_default() {
            let path = p.join(&n);
            let rel = path.strip_prefix(base).unwrap().display().to_string();
            if rel.contains(".backup-") {
                continue;
            }
            match item(&path) {
                Item::Link { target, .. } => out.push((rel, "link".into(), target)),
                Item::Dir { .. } => {
                    out.push((rel.clone(), "dir".into(), String::new()));
                    walk(base, &path, out);
                }
                _ => {
                    let body = std::fs::read_to_string(&path).unwrap_or_default();
                    // 清单里的 updatedAt 每写一次都变，比内容时去掉它。
                    let body = body
                        .lines()
                        .filter(|l| !l.contains("\"updatedAt\""))
                        .collect::<Vec<_>>()
                        .join("\n");
                    out.push((rel, "file".into(), body));
                }
            }
        }
    }
    walk(root, root, &mut out);
    out
}

// ───────────────────────────── 建账号库 ─────────────────────────────

/// ★ 预演一个字节都不写；真做：身份那几份搬进 `<库>/lab`、共享项全是链回共享库的链接、排除的不链、权限 0700 / 0600、
/// 清单 v1 的五格 ＋ 账号 0 在末尾且没有 `configDir` 键、别名文件里有 `labcc`。
#[test]
fn init_moves_the_identity_links_the_rest_and_writes_the_manifest_and_alias() {
    let t = machine("init");
    let before = tree(&t.0);
    let dry = ok(
        &t,
        "accounts-init",
        json!({ "name": "lab", "dryRun": true }),
    );
    assert_eq!(dry["applied"], false);
    assert!(dry["steps"].as_array().unwrap().len() >= 8, "{dry}");
    assert_eq!(tree(&t.0), before, "预演动了盘");

    let got = ok(&t, "accounts-init", json!({ "name": "lab" }));
    assert_eq!(got["applied"], true);
    assert!(got["backup"].is_string());
    for moved in [
        ".credentials.json",
        "stats-cache.json",
        "policy-limits.json",
        ".claude.json",
    ] {
        assert!(
            t.p(&format!(".cc-monitor/accounts/lab/{moved}")).is_file(),
            "{moved} 没搬进来"
        );
        assert!(!t.is_link(&format!(".cc-monitor/accounts/lab/{moved}")));
    }
    assert!(
        t.p(".cc-monitor/accounts/lab/backups").is_dir()
            && !t.is_link(".cc-monitor/accounts/lab/backups")
    );
    assert!(!t.exists(".claude/.credentials.json"), "共享库里还留着凭据");
    assert!(!t.exists(".claude.json"), "$HOME/.claude.json 没迁走");
    for shared in ["skills", "projects", "settings.json", "CLAUDE.md"] {
        assert_eq!(
            t.link_of(&format!(".cc-monitor/accounts/lab/{shared}")),
            t.s(&format!(".claude/{shared}")),
            "{shared}"
        );
    }
    for excluded in [
        "accounts",
        "settings.json.bak",
        "settings.json.bak-before-x",
    ] {
        assert!(
            !t.exists(&format!(".cc-monitor/accounts/lab/{excluded}")),
            "{excluded} 被链了"
        );
    }
    assert_eq!(t.mode(".cc-monitor/accounts"), 0o700);
    assert_eq!(t.mode(".cc-monitor/accounts/lab"), 0o700);
    assert_eq!(t.mode(".cc-monitor/accounts/lab/.credentials.json"), 0o600);
    assert_eq!(t.mode(".cc-monitor/accounts/lab/.claude.json"), 0o600);
    assert_eq!(t.mode(".cc-monitor/accounts/accounts.json"), 0o600);
    let m = t.manifest();
    assert_eq!(m["version"], 1);
    assert_eq!(m["sharedStore"], t.s(".claude"));
    // 账号库在哪由家决定，清单不记它：顶层恰好这四格。
    let mut top: Vec<&str> = m.as_object().unwrap().keys().map(String::as_str).collect();
    top.sort_unstable();
    assert_eq!(top, ["accounts", "sharedStore", "updatedAt", "version"]);
    let accts = m["accounts"].as_array().unwrap();
    assert_eq!(accts.len(), 2);
    assert_eq!(
        accts[0],
        json!({ "name": "lab", "email": "first@example.test", "configDir": t.s(".cc-monitor/accounts/lab"),
                "isDefault": true, "mode": "isolated" })
    );
    assert_eq!(accts[1]["name"], "0");
    assert!(
        accts[1].get("configDir").is_none(),
        "账号 0 不许有 configDir 键"
    );
    assert_eq!(accts[1]["mode"], "bare");
    assert!(
        !t.read(".cc-monitor/accounts/accounts.json")
            .contains("cred-d"),
        "清单里漏了凭据"
    );
    // 配置文件第一次建出来：首建那两段 ＋ 这个号的两段（基于 cc / cct、只写自己的号；只在建号那一刻加）。
    assert_eq!(profile_names(&t), ["cc", "cct", "labcc", "labcct"]);
    assert_eq!(
        own(&t, "labcc"),
        (Some("cc".into()), sv(&["--account", "lab"]))
    );
    assert_eq!(
        own(&t, "labcct"),
        (Some("cct".into()), sv(&["--account", "lab"]))
    );
    assert_eq!(
        std::fs::read_link(t.p(".cc-monitor/bin/labcct"))
            .unwrap()
            .display()
            .to_string(),
        "ccm",
        "不撞名的做成指向 ccm 的链接"
    );
    assert_eq!(got["aliases"][0]["added"], json!(["labcc", "labcct"]));
    assert_eq!(got["aliasNames"], json!(["labcc", "labcct"]));
}

/// 拒绝：已建过 · 名字不合规（空格 · `../` · `/` · 保留名 `0`）· 身份文件是链接（旧的软链切号方案）。拒了一个字节都不写。
#[test]
fn init_refusals_leave_nothing_behind() {
    let t = machine("init-no");
    for bad in ["bad name", "../evil", "x/y", "0", "-x", ""] {
        assert_eq!(
            code(&t, "accounts-init", json!({ "name": bad })),
            "refused",
            "{bad:?}"
        );
    }
    assert!(!t.exists(".cc-monitor/accounts"), "被拒后落了盘");
    assert_eq!(
        code(&t, "accounts-init", json!({ "nam": "lab" })),
        "bad_args"
    );
    assert_eq!(
        code(&t, "accounts-init", json!({ "name": "lab", "extra": 1 })),
        "bad_args"
    );

    std::fs::rename(t.p(".claude/.credentials.json"), t.p("real-cred.json")).unwrap();
    std::os::unix::fs::symlink(t.p("real-cred.json"), t.p(".claude/.credentials.json")).unwrap();
    assert_eq!(
        code(&t, "accounts-init", json!({ "name": "lab" })),
        "refused"
    );
    assert!(!t.exists(".cc-monitor/accounts"));

    let u = machine("init-twice");
    ok(&u, "accounts-init", json!({ "name": "lab" }));
    assert_eq!(code(&u, "accounts-init", json!({ "name": "e" })), "refused");
}

/// 空家目录（连 `~/.claude` 都没有）：共享库先建出来，默认号只有自己，提示「没有可共享的顶层项」。
#[test]
fn init_on_an_empty_home_creates_the_shared_root_too() {
    let t = tmp("empty");
    let got = ok(&t, "accounts-init", json!({ "name": "lab" }));
    assert!(t.p(".claude").is_dir() && t.p(".cc-monitor/accounts/lab").is_dir());
    assert!(!got["notes"].as_array().unwrap().is_empty(), "{got}");
    assert_eq!(t.manifest()["accounts"][0]["email"], "");
}

// ───────────────────────────── 加号 ─────────────────────────────

/// ★ 订阅号从一份凭据导入：`0600` 的一份拷贝（源不动）· 共享项全链 · 身份之外那几份状态从共享库复制成它自己的（不是链接）·
/// `.claude.json` 不从别人那儿复制 · 清单追加 · 别名文件多一条 `xcc`、原来的 `labcc` 与用户自己的别名都在 · 导入了就不给登录那一行。
#[test]
fn add_imports_credentials_links_shared_items_and_updates_manifest_and_aliases() {
    let t = machine("add");
    ok(&t, "accounts-init", json!({ "name": "lab" }));
    // 用户自己手写进配置文件的一段：建号不碰它。
    t.write(
        ".cc-monitor/profiles.toml",
        &(t.read(".cc-monitor/profiles.toml") + "\n# 我的\n[mine]\nccm-tmux = true\n"),
    );
    t.write(".claude/stats-cache.json", "{\"tpl\":1}");
    t.write("snap/cred-x.json", "{\"fake\":\"cred-x\"}");
    t.chmod("snap/cred-x.json", 0o644);
    let got = ok(
        &t,
        "accounts-add",
        json!({ "name": "x", "kind": "subscription", "credFile": "~/snap/cred-x.json" }),
    );
    assert_eq!(got["applied"], true);
    assert_eq!(
        got["account"],
        json!({ "name": "x", "configDir": t.s(".cc-monitor/accounts/x") })
    );
    assert!(got["loginCmd"].is_null());
    assert_eq!(
        t.read(".cc-monitor/accounts/x/.credentials.json"),
        "{\"fake\":\"cred-x\"}"
    );
    assert_eq!(t.mode(".cc-monitor/accounts/x/.credentials.json"), 0o600);
    assert_eq!(
        t.read("snap/cred-x.json"),
        "{\"fake\":\"cred-x\"}",
        "源被动了"
    );
    assert_eq!(t.mode(".cc-monitor/accounts/x"), 0o700);
    assert_eq!(
        t.link_of(".cc-monitor/accounts/x/skills"),
        t.s(".claude/skills")
    );
    assert!(!t.is_link(".cc-monitor/accounts/x/stats-cache.json"));
    assert_eq!(
        t.read(".cc-monitor/accounts/x/stats-cache.json"),
        "{\"tpl\":1}"
    );
    assert!(
        !t.exists(".cc-monitor/accounts/x/.claude.json"),
        "从别处种了身份"
    );
    let m = t.manifest();
    let names: Vec<&str> = m["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["lab", "x", "0"]);
    let have = profile_names(&t);
    for n in ["labcc", "xcc", "xcct", "mine"] {
        assert!(have.iter().any(|h| h == n), "少了 {n}：{have:?}");
    }
    assert_eq!(got["aliases"][0]["added"], json!(["xcc", "xcct"]));
    // 新加的两条是链接：已开的终端马上能用，不叫人重读。
    for n in ["xcc", "xcct"] {
        assert!(t.p(&format!(".cc-monitor/bin/{n}")).is_symlink(), "{n}");
    }

    // 两个号的身份互不覆盖：各写各的。
    t.write(
        ".cc-monitor/accounts/lab/.credentials.json",
        "{\"fake\":\"d2\"}",
    );
    assert_eq!(
        t.read(".cc-monitor/accounts/x/.credentials.json"),
        "{\"fake\":\"cred-x\"}"
    );
    // 共享是活的：共享库改一处，两个号都看得见；一个号写的落进共享库。
    t.write(".claude/skills/new.md", "n");
    assert!(
        t.p(".cc-monitor/accounts/lab/skills/new.md").is_file()
            && t.p(".cc-monitor/accounts/x/skills/new.md").is_file()
    );
    t.write(".cc-monitor/accounts/x/skills/from-x.md", "y");
    assert!(t.p(".claude/skills/from-x.md").is_file() && !t.is_link(".claude/skills/from-x.md"));
}

/// 没导入凭据的订阅号：回登录那一行（claude 自己的登录界面），名字与 ccm 都过了 quote。
#[test]
fn add_without_credentials_hands_back_the_login_line() {
    let t = machine("add-login");
    ok(&t, "accounts-init", json!({ "name": "lab" }));
    let got = ok(
        &t,
        "accounts-add",
        json!({ "name": "b-2", "kind": "subscription", "isDefault": true }),
    );
    assert_eq!(
        got["loginCmd"],
        format!("'{}' -- --account 'b-2'", t.s(".cc-monitor/bin/ccm"))
    );
    assert_eq!(got["aliasNames"], json!(["b2cc", "b2cct"]));
    let m = t.manifest();
    assert_eq!(m["accounts"][0]["isDefault"], false);
    assert_eq!(m["accounts"][1]["isDefault"], true);
    assert_eq!(
        ok(&t, "accounts-login-cmd", json!({ "name": "lab" }))["cmd"],
        format!("'{}' -- --account 'lab'", t.s(".cc-monitor/bin/ccm"))
    );
    assert_eq!(
        code(&t, "accounts-login-cmd", json!({ "name": "nope" })),
        "refused"
    );
}

/// 拒绝：没建库 · 重名 · 保留名 · 凭据源不在 / 是空文件 / 是链接 / 在家目录外 · 两种号的格混用。拒了不留半成品目录。
#[test]
fn add_refusals_leave_no_half_built_account() {
    let t = machine("add-no");
    assert_eq!(
        code(
            &t,
            "accounts-add",
            json!({ "name": "x", "kind": "subscription" })
        ),
        "not_enabled"
    );
    assert!(!t.exists(".cc-monitor/accounts"));
    ok(&t, "accounts-init", json!({ "name": "lab" }));
    assert_eq!(
        code(
            &t,
            "accounts-add",
            json!({ "name": "lab", "kind": "subscription" })
        ),
        "refused"
    );
    assert_eq!(
        code(
            &t,
            "accounts-add",
            json!({ "name": "0", "kind": "subscription" })
        ),
        "refused"
    );
    t.write("snap/empty.json", "");
    std::os::unix::fs::symlink(t.p(".claude.json"), t.p("snap/link.json")).ok();
    for cred in [
        "~/snap/nope.json",
        "~/snap/empty.json",
        "~/snap/link.json",
        "/etc/hostname",
        "snap/rel.json",
        "~/../x",
    ] {
        assert_eq!(
            code(
                &t,
                "accounts-add",
                json!({ "name": "x", "kind": "subscription", "credFile": cred })
            ),
            "refused",
            "{cred}"
        );
        assert!(!t.exists(".cc-monitor/accounts/x"), "{cred}：留了半成品");
    }
    assert_eq!(
        code(
            &t,
            "accounts-add",
            json!({ "name": "x", "kind": "api-key", "credFile": "~/a" })
        ),
        "bad_args"
    );
    assert_eq!(
        code(
            &t,
            "accounts-add",
            json!({ "name": "x", "kind": "subscription", "key": "k" })
        ),
        "bad_args"
    );
    assert_eq!(
        code(&t, "accounts-add", json!({ "name": "x", "kind": "oauth" })),
        "bad_args"
    );
}

/// ★ API 号：清单写 `authKind: api-key`；key 在建好目录之后交给 apikey 表那一口（带 configDir 与 Base URL），回掩码；
/// 明文不进清单、不进应答。key 空 / 地址装不进表 ⇒ 建号之前就拒，一个字节不写。
#[test]
fn api_key_account_lands_its_key_in_the_apikey_table_not_in_the_manifest() {
    let t = machine("api");
    ok(&t, "accounts-init", json!({ "name": "lab" }));
    let keys = Mutex::new(Vec::new());
    let got = call_with(
        &t,
        &keys,
        "accounts-add",
        json!({ "name": "k", "kind": "api-key", "key": "sk-secret-abcd", "baseUrl": "https://api.example.test" }),
    )
    .unwrap();
    let masked = got["keyMasked"].as_str().unwrap();
    assert!(
        masked.ends_with("abcd") && !masked.contains("secret"),
        "{masked}"
    );
    assert!(got["keyProblem"].is_null());
    assert_eq!(key_ids(&t), vec!["k".to_string()]);
    assert!(got["loginCmd"].is_null());
    assert_eq!(
        *keys.lock().unwrap(),
        vec![
            json!({ "configDir": t.s(".cc-monitor/accounts/k"), "key": "sk-secret-abcd", "baseUrl": "https://api.example.test" })
        ]
    );
    assert_eq!(t.manifest()["accounts"][1]["authKind"], "api-key");
    assert!(!t
        .read(".cc-monitor/accounts/accounts.json")
        .contains("sk-secret"));
    assert!(!got.to_string().contains("sk-secret"));
    // 核对不因 API 号没有订阅凭据而提示「没登录」：它那几条里一条提示都没有。
    let v = verify(&t);
    let k_warns: Vec<&Value> = v["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["account"] == "k" && c["level"] == "warn")
        .collect();
    assert!(k_warns.is_empty(), "{k_warns:?}");

    for (key, base) in [("", None), ("   ", None), ("sk", Some("ftp://x.test"))] {
        let args = match base {
            Some(b) => json!({ "name": "k2", "kind": "api-key", "key": key, "baseUrl": b }),
            None => json!({ "name": "k2", "kind": "api-key", "key": key }),
        };
        assert_eq!(call(&t, "accounts-add", args).unwrap_err().code, "bad_args");
        assert!(!t.exists(".cc-monitor/accounts/k2"));
    }
}

/// key 写不进去（表那一口拒了）：号照样建好，应答里说清 key 没写进去（界面据此让人在那一行重填）。
#[test]
fn a_key_that_does_not_land_is_said_not_swallowed() {
    let t = machine("api-fail");
    ok(&t, "accounts-init", json!({ "name": "lab" }));
    let key_set = |_: &Value| -> file_face::FileFaceAnswer {
        Err(crate::stream::inbound::spec::Fail::from((
            "io_failed",
            "盘满了".to_string(),
        )))
    };
    let got = call_door(
        &t,
        &key_set,
        "accounts-add",
        json!({ "name": "k", "kind": "api-key", "key": "sk-1" }),
    )
    .unwrap();
    assert!(got["keyMasked"].is_null());
    assert!(got["keyProblem"].as_str().unwrap().contains("盘满了"));
    assert!(t.p(".cc-monitor/accounts/k").is_dir());
}

// ───────────────────────────── 删号 · 设默认 ─────────────────────────────

/// 删号只删它自己的目录，共享库一个字节不少；别名随之删掉。默认号不加 `force` 删不掉、账号 0 删不掉、不认识的号删不掉。
#[test]
fn remove_deletes_only_the_account_dir_and_its_alias() {
    let t = two_accounts("rm");
    t.write(".cc-monitor/accounts/x/skills/from-x.md", "y");
    let shared_before = tree(&t.p(".claude"));
    let got = ok(&t, "accounts-remove", json!({ "name": "x" }));
    assert_eq!(got["applied"], true);
    assert!(!t.exists(".cc-monitor/accounts/x"));
    assert_eq!(tree(&t.p(".claude")), shared_before, "共享库被动了");
    assert!(!profile_names(&t).iter().any(|n| n.starts_with("xcc")));
    assert!(profile_names(&t).iter().any(|n| n == "labcc"));
    assert!(
        !t.p(".cc-monitor/bin/xcc").is_symlink(),
        "删掉的那一段，链接也没了"
    );
    assert_eq!(got["aliases"][0]["removed"], json!(["xcc", "xcct"]));
    assert_eq!(
        code(&t, "accounts-remove", json!({ "name": "lab" })),
        "refused"
    );
    assert!(t.p(".cc-monitor/accounts/lab").is_dir());
    assert_eq!(
        code(&t, "accounts-remove", json!({ "name": "0" })),
        "refused"
    );
    assert_eq!(
        code(&t, "accounts-remove", json!({ "name": "nope" })),
        "refused"
    );
}

/// 删掉默认号（带 `force`）⇒ 剩下的第一个号接着当默认；设默认只改清单那一格。
#[test]
fn default_moves_on_remove_and_can_be_set() {
    let t = two_accounts("def");
    ok(&t, "accounts-set-default", json!({ "name": "x" }));
    let m = t.manifest();
    assert_eq!(
        (
            m["accounts"][0]["isDefault"].clone(),
            m["accounts"][1]["isDefault"].clone()
        ),
        (json!(false), json!(true))
    );
    let again = ok(&t, "accounts-set-default", json!({ "name": "x" }));
    assert_eq!(again["applied"], false, "已是默认还改了一遍");
    let got = ok(&t, "accounts-remove", json!({ "name": "x", "force": true }));
    assert!(got["notes"].to_string().contains("lab"), "{got}");
    assert_eq!(t.manifest()["accounts"][0]["isDefault"], true);
}

/// 删 API 号：key 表里它那一行跟着清掉（第一步，进同一份备份），表里别的号不动；回滚把那一行原样放回来。
#[test]
fn removing_an_api_account_drops_its_key_row_and_rollback_puts_it_back() {
    let t = machine("rmk");
    ok(&t, "accounts-init", json!({ "name": "lab" }));
    for (name, key) in [("k1", "sk-first-1111"), ("k2", "sk-second-2222")] {
        ok(
            &t,
            "accounts-add",
            json!({ "name": name, "kind": "api-key", "key": key, "baseUrl": "https://api.example.test" }),
        );
    }
    assert_eq!(key_ids(&t), vec!["k1".to_string(), "k2".to_string()]);
    let before = t.read(".cc-monitor/apikey-credentials.json");
    let preview = ok(
        &t,
        "accounts-remove",
        json!({ "name": "k1", "dryRun": true }),
    );
    assert!(
        preview["steps"][0]
            .as_str()
            .unwrap()
            .contains("apikey-credentials.json"),
        "{preview}"
    );
    assert_eq!(
        t.read(".cc-monitor/apikey-credentials.json"),
        before,
        "预演动了 key 表"
    );

    let got = ok(&t, "accounts-remove", json!({ "name": "k1" }));
    assert_eq!(key_ids(&t), vec!["k2".to_string()], "{got}");
    let table = t.read(".cc-monitor/apikey-credentials.json");
    assert!(!table.contains("sk-first-1111") && table.contains("sk-second-2222"));
    assert_eq!(t.mode(".cc-monitor/apikey-credentials.json"), 0o600);
    assert!(!t.exists(".cc-monitor/accounts/k1"));

    let back = ok(&t, "accounts-rollback", json!({}));
    assert_eq!(back["applied"], true, "{back}");
    assert_eq!(key_ids(&t), vec!["k1".to_string(), "k2".to_string()]);
    let doc: Value = serde_json::from_str(&t.read(".cc-monitor/apikey-credentials.json")).unwrap();
    assert_eq!(doc["accounts"]["k1"]["api_key"], "sk-first-1111");
    assert_eq!(doc["accounts"]["k2"]["api_key"], "sk-second-2222");
    assert!(t.p(".cc-monitor/accounts/k1").is_dir());
}

/// 删订阅号不碰 key 表：表里那几行、那份文件一个字节不变（它本来就没有那个号那一行）。
#[test]
fn removing_a_subscription_account_leaves_the_key_table_alone() {
    let t = two_accounts("rm-sub");
    ok(
        &t,
        "accounts-add",
        json!({ "name": "k", "kind": "api-key", "key": "sk-only-3333" }),
    );
    let before = t.read(".cc-monitor/apikey-credentials.json");
    assert!(before.contains("sk-only-3333"));
    let got = ok(&t, "accounts-remove", json!({ "name": "x" }));
    assert!(
        !got["steps"].to_string().contains("apikey-credentials"),
        "{got}"
    );
    assert_eq!(t.read(".cc-monitor/apikey-credentials.json"), before);
    assert_eq!(key_ids(&t), vec!["k".to_string()]);
}

/// 设默认 · 修复 · 核对的拒绝形：不认得的号 ⇒ `refused`；多给参数 ⇒ `bad_args`；还没有账号库 ⇒ `not_enabled`。被拒的那几趟盘上一个字节不动。
#[test]
fn set_default_repair_and_verify_refuse_what_they_cannot_do() {
    let t = two_accounts("refuse3");
    let before = tree(&t.0);
    assert_eq!(
        code(&t, "accounts-set-default", json!({ "name": "nope" })),
        "refused"
    );
    assert_eq!(
        code(&t, "accounts-verify", json!({ "names": ["lab"] })),
        "bad_args"
    );
    assert_eq!(
        code(&t, "accounts-repair", json!({ "force": true })),
        "bad_args"
    );
    assert_eq!(tree(&t.0), before, "被拒的那几趟动了盘");
    let empty = machine("refuse3-empty");
    let empty_before = tree(&empty.0);
    assert_eq!(code(&empty, "accounts-repair", json!({})), "not_enabled");
    assert_eq!(
        code(&empty, "accounts-set-default", json!({ "name": "lab" })),
        "not_enabled"
    );
    assert_eq!(tree(&empty.0), empty_before, "没有账号库时动了盘");
}

// ───────────────────────────── 核对 ─────────────────────────────

/// 装好的一套核下来是 PASS：账号 0 没登录 · 各号邮箱互不相同（只有一个号有邮箱时跳过）· 链接全在。
#[test]
fn a_fresh_setup_verifies_clean() {
    let t = two_accounts("v-ok");
    t.write(
        ".cc-monitor/accounts/x/.claude.json",
        "{\"oauthAccount\":{\"emailAddress\":\"second@example.test\"}}",
    );
    let v = verify(&t);
    assert_eq!(v["pass"], true, "{v}");
    assert!(checks(&v, "ok")
        .iter()
        .any(|c| c.contains("second@example.test")));
    assert_eq!(checks(&v, "fail"), Vec::<String>::new());
}

/// ★ 链指错地方 ⇒ 核对报出来（致命）；修复把它改回来。
#[test]
fn a_link_pointing_elsewhere_is_caught_by_verify() {
    let t = two_accounts("v-wrong");
    std::fs::remove_file(t.p(".cc-monitor/accounts/x/skills")).unwrap();
    std::os::unix::fs::symlink(t.p("elsewhere"), t.p(".cc-monitor/accounts/x/skills")).unwrap();
    let v = verify(&t);
    assert_eq!(v["pass"], false);
    let fails = checks(&v, "fail");
    assert!(
        fails
            .iter()
            .any(|f| f.contains("skills") && f.contains(&t.s("elsewhere"))),
        "{fails:?}"
    );
}

/// 其余几种致命：身份文件是链接（串号）· 凭据权限不对 · 号的目录权限不对 · 缺链接 · 共享项在号里是实体文件 · 两个号邮箱相同。
#[test]
fn verify_fails_on_each_broken_invariant() {
    type Case = (&'static str, fn(&Tmp), &'static str);
    let cases: &[Case] = &[
        (
            "identity-link",
            |t| {
                t.write(".claude/stats-cache.json", "{}");
                std::os::unix::fs::symlink(
                    t.p(".claude/stats-cache.json"),
                    t.p(".cc-monitor/accounts/x/stats-cache.json"),
                )
                .unwrap();
            },
            "stats-cache.json",
        ),
        (
            "cred-mode",
            |t| t.chmod(".cc-monitor/accounts/x/.credentials.json", 0o644),
            "644",
        ),
        (
            "dir-mode",
            |t| t.chmod(".cc-monitor/accounts/x", 0o755),
            "755",
        ),
        (
            "missing",
            |t| std::fs::remove_file(t.p(".cc-monitor/accounts/x/CLAUDE.md")).unwrap(),
            "1",
        ),
        (
            "entity",
            |t| {
                std::fs::remove_file(t.p(".cc-monitor/accounts/x/CLAUDE.md")).unwrap();
                t.write(".cc-monitor/accounts/x/CLAUDE.md", "own");
            },
            "CLAUDE.md",
        ),
        (
            "dup-email",
            |t| {
                t.write(
                    ".cc-monitor/accounts/x/.claude.json",
                    "{\"oauthAccount\":{\"emailAddress\":\"first@example.test\"}}",
                );
            },
            "first@example.test",
        ),
    ];
    for (tag, breakit, needle) in cases {
        let t = two_accounts(&format!("v-{tag}"));
        assert_eq!(verify(&t)["pass"], true, "{tag}：坏之前就不绿");
        breakit(&t);
        let v = verify(&t);
        let fails = checks(&v, "fail");
        assert!(fails.iter().any(|f| f.contains(needle)), "{tag}：{fails:?}");
    }
}

/// 共享库本身没有可共享的项 ⇒ 致命（根本没有共享）；源头就是断链 ⇒ 只提示（修复修不了，要到共享库里处理）。
#[test]
fn verify_on_empty_shared_and_broken_sources() {
    let t = tmp("v-empty");
    ok(&t, "accounts-init", json!({ "name": "lab" }));
    assert!(checks(&verify(&t), "fail")
        .iter()
        .any(|f| f.contains(&t.s(".claude"))));

    let u = two_accounts("v-src");
    std::os::unix::fs::symlink(u.p("gone"), u.p(".claude/local")).unwrap();
    ok(&u, "accounts-repair", json!({}));
    assert!(
        u.is_link(".cc-monitor/accounts/lab/local"),
        "断链项也该链上"
    );
    let v = verify(&u);
    assert_eq!(v["pass"], true, "{v}");
    assert!(checks(&v, "warn").iter().any(|w| w.contains("local")));
    let again = ok(&u, "accounts-repair", json!({}));
    assert_eq!(again["applied"], false, "断链让修复来回翻：{again}");
}

// ───────────────────────────── 修复 ─────────────────────────────

/// ★ 修复：补缺的链 · 改指错的链 · 清共享库里已删掉的残链 · 共享库新长的项补链 · 权限改回 0700 / 0600 · 邮箱按 `.claude.json` 刷新；
/// **别名不回补**（清单只跟着账号表里号的增减走，删掉的别名文件就是不在）；
/// 再跑一次一个字节不写（幂等）。
#[test]
fn repair_restores_every_invariant_and_is_idempotent() {
    let t = two_accounts("repair");
    std::fs::remove_file(t.p(".cc-monitor/accounts/x/CLAUDE.md")).unwrap();
    std::fs::remove_file(t.p(".cc-monitor/accounts/x/skills")).unwrap();
    std::os::unix::fs::symlink(t.p("elsewhere"), t.p(".cc-monitor/accounts/x/skills")).unwrap();
    t.write(".claude/newdir/a", "a");
    std::os::unix::fs::symlink(
        t.p(".claude/retired"),
        t.p(".cc-monitor/accounts/lab/retired"),
    )
    .unwrap();
    t.chmod(".cc-monitor/accounts/x", 0o755);
    t.chmod(".cc-monitor/accounts/x/.credentials.json", 0o644);
    t.write(
        ".cc-monitor/accounts/x/.claude.json",
        "{\"oauthAccount\":{\"emailAddress\":\"second@example.test\"}}",
    );
    std::fs::remove_file(t.p(".cc-monitor/aliases.sh")).unwrap();

    let got = ok(&t, "accounts-repair", json!({}));
    assert_eq!(got["applied"], true);
    assert_eq!(
        t.link_of(".cc-monitor/accounts/x/CLAUDE.md"),
        t.s(".claude/CLAUDE.md")
    );
    assert_eq!(
        t.link_of(".cc-monitor/accounts/x/skills"),
        t.s(".claude/skills")
    );
    for a in ["lab", "x"] {
        assert_eq!(
            t.link_of(&format!(".cc-monitor/accounts/{a}/newdir")),
            t.s(".claude/newdir"),
            "{a}"
        );
    }
    assert!(!t.exists(".cc-monitor/accounts/lab/retired"));
    assert_eq!(t.mode(".cc-monitor/accounts/x"), 0o700);
    assert_eq!(t.mode(".cc-monitor/accounts/x/.credentials.json"), 0o600);
    assert_eq!(t.manifest()["accounts"][1]["email"], "second@example.test");
    assert!(!t.exists(".cc-monitor/aliases.sh"), "修复回补了别名");
    assert_eq!(got["aliases"], json!([]));
    assert_eq!(verify(&t)["pass"], true);

    let before = tree(&t.0);
    let again = ok(&t, "accounts-repair", json!({}));
    assert_eq!(again["applied"], false, "{again}");
    assert!(again["backup"].is_null());
    assert_eq!(tree(&t.0), before);
}

/// 身份那几项在号里还是链向共享库的链接 ⇒ 修复把它**复制**成这个号自己的一份（不是删掉）；共享库那份留着。
#[test]
fn repair_privatises_a_linked_identity_item_instead_of_deleting_it() {
    let t = two_accounts("repair-iso");
    t.write(".claude/stats-cache.json", "{\"s\":1}");
    std::os::unix::fs::symlink(
        t.p(".claude/stats-cache.json"),
        t.p(".cc-monitor/accounts/x/stats-cache.json"),
    )
    .unwrap();
    ok(&t, "accounts-repair", json!({}));
    assert!(!t.is_link(".cc-monitor/accounts/x/stats-cache.json"));
    assert_eq!(
        t.read(".cc-monitor/accounts/x/stats-cache.json"),
        "{\"s\":1}"
    );
    assert_eq!(t.read(".claude/stats-cache.json"), "{\"s\":1}");
    t.write(".cc-monitor/accounts/x/stats-cache.json", "{\"mine\":1}");
    assert_eq!(
        t.read(".claude/stats-cache.json"),
        "{\"s\":1}",
        "改自己那一份写回了共享库"
    );
}

// ───────────────────────────── 隔离 ─────────────────────────────

/// 把一个共享项隔离成每个号各一份：内容取自共享库、共享库那份留着、各号互不影响；不在身份表里 ⇒ 提示下次修复不会再链它。
#[test]
fn isolate_makes_a_private_copy_per_account() {
    let t = two_accounts("iso");
    let dry = ok(
        &t,
        "accounts-isolate",
        json!({ "item": "settings.json", "dryRun": true }),
    );
    assert!(
        t.is_link(".cc-monitor/accounts/lab/settings.json"),
        "预演动了盘"
    );
    assert!(!dry["notes"].as_array().unwrap().is_empty());
    ok(&t, "accounts-isolate", json!({ "item": "settings.json" }));
    for a in ["lab", "x"] {
        let rel = format!(".cc-monitor/accounts/{a}/settings.json");
        assert!(!t.is_link(&rel), "{a}");
        assert_eq!(t.read(&rel), "{\"theme\":\"dark\"}");
    }
    assert_eq!(t.read(".claude/settings.json"), "{\"theme\":\"dark\"}");
    t.write(
        ".cc-monitor/accounts/lab/settings.json",
        "{\"theme\":\"zzz\"}",
    );
    assert_eq!(
        t.read(".cc-monitor/accounts/x/settings.json"),
        "{\"theme\":\"dark\"}"
    );
    for bad in ["", "a/b", "..", "no-such-thing"] {
        assert_eq!(
            code(&t, "accounts-isolate", json!({ "item": bad })),
            "refused",
            "{bad:?}"
        );
    }
}

// ───────────────────────────── 回滚 ─────────────────────────────

/// ★ 修复之前那一刻可以原样回去：修复前后各拍一张（不跟链接的）目录树，回滚之后与修复前逐项相同。
#[test]
fn rollback_returns_to_the_state_before_a_repair() {
    let t = two_accounts("rb-repair");
    std::fs::remove_file(t.p(".cc-monitor/accounts/x/skills")).unwrap();
    std::os::unix::fs::symlink(t.p("elsewhere"), t.p(".cc-monitor/accounts/x/skills")).unwrap();
    t.chmod(".cc-monitor/accounts/x/.credentials.json", 0o600);
    let before = tree(&t.0);
    let fixed = ok(&t, "accounts-repair", json!({}));
    let id = fixed["backup"].as_str().unwrap().to_string();
    assert_ne!(tree(&t.0), before);
    let dry = ok(&t, "accounts-rollback", json!({ "dryRun": true }));
    assert_eq!(dry["backup"], id.as_str());
    assert!(!dry["steps"].as_array().unwrap().is_empty());
    let back = ok(&t, "accounts-rollback", json!({}));
    assert_eq!(back["applied"], true);
    let after: Vec<_> = tree(&t.0)
        .into_iter()
        .filter(|(p, _, _)| !p.starts_with(".cc-monitor"))
        .collect();
    let want: Vec<_> = before
        .into_iter()
        .filter(|(p, _, _)| !p.starts_with(".cc-monitor"))
        .collect();
    assert_eq!(after, want);
    assert_eq!(t.link_of(".cc-monitor/accounts/x/skills"), t.s("elsewhere"));
    // 还原过的那一份不再是「最近一份」。
    assert_ne!(
        ok(&t, "accounts-rollback", json!({ "dryRun": true }))["backup"],
        id.as_str()
    );
}

/// 回滚建库那一趟：凭据回到共享库、`$HOME/.claude.json` 回来、号的目录与清单没了、别名随之删掉。
#[test]
fn rollback_of_init_puts_the_identity_back() {
    let t = machine("rb-init");
    let got = ok(&t, "accounts-init", json!({ "name": "lab" }));
    let id = got["backup"].as_str().unwrap().to_string();
    ok(&t, "accounts-rollback", json!({ "backup": id }));
    assert_eq!(t.read(".claude/.credentials.json"), "{\"fake\":\"cred-d\"}");
    assert!(t.p(".claude.json").is_file());
    assert!(t.p(".claude/backups/one.json").is_file());
    assert!(!t.exists(".cc-monitor/accounts/lab"));
    assert!(!t.exists(".cc-monitor/accounts/accounts.json"));
    assert!(!profile_names(&t).iter().any(|n| n == "labcc"));
}

/// 回滚一次删号 ⇒ 号回来了，它的两条也按建号加回（清单只跟着账号表里号的增减走）；用户改过名的那条不重复加。
#[test]
fn rollback_of_a_removal_brings_the_accounts_aliases_back() {
    let t = two_accounts("rb-rm");
    let got = ok(&t, "accounts-remove", json!({ "name": "x" }));
    let id = got["backup"].as_str().unwrap().to_string();
    assert!(!profile_names(&t).iter().any(|n| n == "xcc"));
    let back = ok(&t, "accounts-rollback", json!({ "backup": id }));
    assert_eq!(back["aliases"][0]["added"], json!(["xcc", "xcct"]));
    assert!(profile_names(&t).iter().any(|n| n == "xcct"));
}

/// 回滚的安全：备份名只收时间戳字符集（`..` · `/` 一律拒）；撤销清单里指到三个根之外的还原条目跳过、其余照做、整趟报「部分没做成」。
#[test]
fn rollback_refuses_traversal_and_skips_out_of_bounds_entries() {
    let t = machine("rb-safe");
    ok(&t, "accounts-init", json!({ "name": "lab" }));
    for bad in ["../evil", "a/b", "20260101-000000..x", ""] {
        assert_eq!(
            code(&t, "accounts-rollback", json!({ "backup": bad })),
            "refused",
            "{bad:?}"
        );
    }
    t.write("victim/precious.txt", "p");
    let bk = crate::platform::acct_view::names(&t.p(".cc-monitor/accounts"))
        .unwrap()
        .into_iter()
        .find(|n| n.starts_with(".backup-"))
        .map(|n| t.p(&format!(".cc-monitor/accounts/{n}")))
        .unwrap();
    let undo = std::fs::read_to_string(bk.join("undo.tsv")).unwrap();
    std::fs::write(
        bk.join("undo.tsv"),
        format!("RESTORE\t/etc/passwd\nDELETE\t{}\n{undo}", t.s("victim")),
    )
    .unwrap();
    let e = call(&t, "accounts-rollback", json!({})).unwrap_err();
    assert_eq!(e.code, "io_failed");
    assert!(t.p("victim/precious.txt").is_file(), "越界的删除照做了");
    assert_eq!(
        t.read(".claude/.credentials.json"),
        "{\"fake\":\"cred-d\"}",
        "合法条目被挡住了"
    );
}

// ───────────────────────────── 别名文件 ─────────────────────────────

/// 配置文件里有写错的地方 ⇒ 不动它（等人先改好），应答里说一句；账号那一趟照样成。
#[test]
fn a_profiles_file_with_a_mistake_is_left_alone() {
    let t = machine("alias-odd");
    t.write(".cc-monitor/profiles.toml", "[cc]\nbogus = 1\n");
    let got = ok(&t, "accounts-init", json!({ "name": "lab" }));
    assert_eq!(t.read(".cc-monitor/profiles.toml"), "[cc]\nbogus = 1\n");
    assert_eq!(got["aliases"][0]["changed"], false);
    assert!(got["aliases"][0]["note"].is_string());
    assert!(t.p(".cc-monitor/accounts/lab").is_dir());
}

/// 名字被用户别的别名占着（参数不一样）⇒ 不盖它、回执里说跳过了它；另一条照加。
#[test]
fn a_user_alias_with_the_same_name_is_not_overwritten() {
    let t = machine("alias-taken");
    t.write(
        ".cc-monitor/aliases.sh",
        "labcc() { ccm \"$@\" -- --ccm-tmux; }\n",
    );
    let got = ok(&t, "accounts-init", json!({ "name": "lab" }));
    assert_eq!(
        own(&t, "labcc"),
        (None, sv(&["--ccm-tmux"])),
        "用户那一条（迁进配置文件的）没被盖"
    );
    assert_eq!(got["aliases"][0]["skipped"], json!(["labcc"]));
    assert_eq!(got["aliases"][0]["added"], json!(["labcct"]));
}

// ───────────────────────────── 现有账号库原样接着用 ─────────────────────────────

/// 盘上那份清单里本模块不认得的格（顶层 · 每个号上）、形状不合规矩的号，改一趟之后原样都在；BOM 照读。
#[test]
fn an_existing_manifest_keeps_what_it_does_not_understand() {
    let t = machine("keep");
    ok(&t, "accounts-init", json!({ "name": "lab" }));
    let m = t.manifest();
    let mut v = m.clone();
    v["claudeVersionPinned"] = json!("2.1.200");
    v["accounts"][0]["note"] = json!("mine");
    v["accounts"]
        .as_array_mut()
        .unwrap()
        .insert(1, json!({ "name": "weird", "configDir": "relative/dir" }));
    std::fs::write(
        t.p(".cc-monitor/accounts/accounts.json"),
        format!("\u{feff}{v}"),
    )
    .unwrap();
    ok(
        &t,
        "accounts-add",
        json!({ "name": "x", "kind": "subscription" }),
    );
    let after = t.manifest();
    assert_eq!(after["claudeVersionPinned"], "2.1.200");
    assert_eq!(after["accounts"][0]["note"], "mine");
    assert_eq!(
        after["accounts"][1],
        json!({ "name": "weird", "configDir": "relative/dir" })
    );
    assert_eq!(after["accounts"][2]["name"], "x");
    assert_eq!(after["accounts"][3]["name"], "0");
    assert_eq!(
        code(
            &t,
            "accounts-add",
            json!({ "name": "weird", "kind": "subscription" })
        ),
        "refused"
    );
}

// ───────────────────────────── 端到端 ─────────────────────────────

/// ★ 端到端：空家目录 ⇒ 建库 ⇒ 加两个号（一个从假凭据导入）⇒ 别名文件自动出现且格式对 ⇒ 弄坏一个链接 ⇒ 核对报出来 ⇒ 修复 ⇒ 回滚到修复前。
#[test]
fn end_to_end_from_an_empty_home() {
    let t = tmp("e2e");
    t.write(".claude/skills/s.md", "s");
    t.write(".claude/settings.json", "{}");
    ok(&t, "accounts-init", json!({ "name": "main" }));
    t.write("snap/c.json", "{\"fake\":\"c\"}");
    ok(
        &t,
        "accounts-add",
        json!({ "name": "work", "kind": "subscription", "credFile": "~/snap/c.json" }),
    );
    ok(
        &t,
        "accounts-add",
        json!({ "name": "side-2", "kind": "subscription" }),
    );
    for (n, acct) in [
        ("maincc", "main"),
        ("workcc", "work"),
        ("side2cc", "side-2"),
    ] {
        assert_eq!(own(&t, n).1, sv(&["--account", acct]), "{n}");
    }
    assert_eq!(verify(&t)["pass"], true);
    std::fs::remove_file(t.p(".cc-monitor/accounts/work/skills")).unwrap();
    std::os::unix::fs::symlink(t.p("wrong"), t.p(".cc-monitor/accounts/work/skills")).unwrap();
    let before_fix = tree(&t.p(".cc-monitor/accounts"));
    assert_eq!(verify(&t)["pass"], false);
    ok(&t, "accounts-repair", json!({}));
    assert_eq!(verify(&t)["pass"], true);
    ok(&t, "accounts-rollback", json!({}));
    assert_eq!(tree(&t.p(".cc-monitor/accounts")), before_fix);
    assert_eq!(verify(&t)["pass"], false);
}
