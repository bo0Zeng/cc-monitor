//! 账号库在**哪一个系统上都**落得成（不只 Unix）：临时家目录里造一个登录过的 `~/.claude`，经本进程的文件管理面
//! （只有 `files-home` 答临时目录）建库、加一个订阅号，看盘上落成了什么、核对报不报错。
//! 同一份在 Linux 门禁里跑，也交叉编到 Windows 上真跑（`cargo test --target x86_64-pc-windows-gnu --no-run` 之后在那台跑）。
//! 另有核对先对齐那一条（临时家目录里真建库，看新冒出来的共享项链给各号）。
//! 夹具只造结构，凭据是写死的假串，邮箱是 `*.test`。挂在资产那一层底下：判据用的那扇真盘门（`HomeDoor`）住那里。
use crate::accounts::manage::wire::{run_change, run_verify, Request};
use crate::assets::aliases::test_support::HomeDoor;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

struct Tmp(PathBuf);
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Tmp {
    fn p(&self, rel: &str) -> PathBuf {
        rel.split('/').fold(self.0.clone(), |p, s| p.join(s))
    }
    fn write(&self, rel: &str, body: &str) {
        let p = self.p(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }
    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.p(rel)).unwrap_or_default()
    }
    /// 这一项本身（不跟链接）是不是一条链接。
    fn is_link(&self, rel: &str) -> bool {
        std::fs::symlink_metadata(self.p(rel)).is_ok_and(|m| m.file_type().is_symlink())
    }
}

fn machine(tag: &str) -> Tmp {
    let h = std::env::temp_dir().join(format!(
        "acct-any-os-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|x| x.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&h).unwrap();
    let t = Tmp(h);
    t.write(".claude/.credentials.json", "{\"fake\":\"cred-d\"}");
    t.write(".claude/settings.json", "{\"theme\":\"dark\"}");
    t.write(".claude/projects/p/MEMORY.md", "m");
    t.write(
        ".claude.json",
        "{\"oauthAccount\":{\"emailAddress\":\"first@example.test\"},\"projects\":{}}",
    );
    t.write("snap/cred-x.json", "{\"fake\":\"cred-x\"}");
    t
}

/// 这台此刻建不建得了符号链接：判据自己真建一条看（不问被测的那一层）。
fn can_link(dir: &Path) -> bool {
    let (target, link) = (dir.join("probe-target"), dir.join("probe-link"));
    std::fs::write(&target, "").unwrap();
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(&target, &link).is_ok();
    #[cfg(windows)]
    let made = std::os::windows::fs::symlink_file(&target, &link).is_ok();
    let _ = std::fs::remove_file(&link);
    let _ = std::fs::remove_file(&target);
    made
}

fn take<T: serde::de::DeserializeOwned>(v: Value) -> T {
    crate::wire::typed::take(&v).expect("入参收得下")
}

/// 建库（lab）＋ 加一个订阅号（x）。这台做得了多账号 ⇒ 两个号各一个配置目录：共享的那几项是链回 `~/.claude` 的链接、
/// 跟过去读到的就是共享库那一份；凭据各一份（x 的是导进来的那一份）；核对没有一条报错。
/// 做不了（如 Windows 上没开开发者模式、建不了符号链接）⇒ 答 `unsupported`，说的就是平台层那一句，盘上一样没动。
#[test]
fn the_account_library_lands_on_this_os() {
    let t = machine("lands");
    let d = HomeDoor(t.0.clone());
    let init = run_change(
        &d,
        &Request::Init(take(json!({ "name": "lab" }))),
        None,
        &Vec::new,
    );
    if !can_link(&t.0) {
        let said =
            crate::platform::acct_view::unsupported_said().expect("建不了链接却说做得了多账号");
        let e = init.expect_err("做不了多账号却建成了");
        assert_eq!(
            (e.code.as_deref(), e.said.as_str()),
            (Some("unsupported"), said.as_str())
        );
        assert!(!t.p(".cc-monitor/accounts").exists(), "拒了却动了盘");
        return;
    }
    init.unwrap_or_else(|e| panic!("建库：{e:?}"));
    run_change(
        &d,
        &Request::Add(take(json!({
            "name": "x",
            "kind": "subscription",
            "credFile": "~/snap/cred-x.json"
        }))),
        None,
        &Vec::new,
    )
    .unwrap_or_else(|e| panic!("加号：{e:?}"));
    let manifest: Value =
        serde_json::from_str(&t.read(".cc-monitor/accounts/accounts.json")).expect("清单是 JSON");
    let dirs: Vec<String> = manifest["accounts"]
        .as_array()
        .expect("清单里有号表")
        .iter()
        .filter_map(|a| a["configDir"].as_str().map(str::to_string))
        .collect();
    assert_eq!(dirs.len(), 2, "{manifest}");
    for (name, cred) in [("lab", "cred-d"), ("x", "cred-x")] {
        let acct = format!(".cc-monitor/accounts/{name}");
        assert!(
            dirs.iter().any(|d| Path::new(d) == t.p(&acct)),
            "清单里 {name} 的配置目录不是 {}：{dirs:?}",
            t.p(&acct).display()
        );
        for shared in ["settings.json", "projects"] {
            assert!(
                t.is_link(&format!("{acct}/{shared}")),
                "{name}/{shared} 不是链接"
            );
        }
        assert_eq!(
            t.read(&format!("{acct}/settings.json")),
            "{\"theme\":\"dark\"}"
        );
        assert_eq!(t.read(&format!("{acct}/projects/p/MEMORY.md")), "m");
        let c = format!("{acct}/.credentials.json");
        assert!(!t.is_link(&c), "{name} 的凭据是链接");
        assert!(t.read(&c).contains(cred), "{name} 的凭据：{}", t.read(&c));
    }
    let report =
        serde_json::to_value(run_verify(&d).unwrap_or_else(|e| panic!("核对：{e:?}"))).unwrap();
    let errors: Vec<&Value> = report["checks"]
        .as_array()
        .expect("核对有条目")
        .iter()
        .filter(|c| c["level"] == "error")
        .collect();
    assert!(errors.is_empty(), "核对报错：{errors:?}");
}

/// 临时家目录（用完删）。
#[cfg(unix)]
struct Home(PathBuf);
#[cfg(unix)]
impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[cfg(unix)]
impl Home {
    fn new(tag: &str) -> Home {
        let h = std::env::temp_dir().join(format!(
            "acct-wire-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|x| x.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&h).unwrap();
        Home(h)
    }
    fn write(&self, rel: &str, body: &str) {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }
    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.0.join(rel)).unwrap_or_default()
    }
    fn link_of(&self, rel: &str) -> Option<PathBuf> {
        std::fs::read_link(self.0.join(rel)).ok()
    }
    fn entries(&self, rel: &str) -> usize {
        crate::platform::acct_view::names(&self.0.join(rel)).map_or(0, |n| n.len())
    }
}

/// ★ 共享库里新冒出来一项已知共享的（`plugins`）：核对那一下先对齐 ⇒ 两个号都有指向它的链接、不报「缺共享链接」；
/// 认不得的新目录（`bridge-spawn`）· 锁文件不链给各号；号里自己的那份实体一个字节不动；接好之后再核不动盘（不另留备份）。
#[cfg(unix)]
#[test]
fn the_checkup_links_a_new_shared_item_instead_of_reporting_it_missing() {
    use cmd_wire::accounts::{AccountAddArgs, AccountInitArgs, AccountKind, CheckLevel};
    let h = Home::new("align");
    h.write(".claude/.credentials.json", "{\"fake\":\"cred-d\"}");
    h.write(".claude/settings.json", "{}");
    h.write(".claude/skills/a.md", "共享的（夹具句）");
    h.write(
        ".claude.json",
        "{\"oauthAccount\":{\"emailAddress\":\"first@example.test\"},\"projects\":{}}",
    );
    let d = HomeDoor(h.0.clone());
    run_change(
        &d,
        &Request::Init(AccountInitArgs {
            name: "lab".into(),
            dry_run: None,
        }),
        None,
        &Vec::new,
    )
    .unwrap();
    run_change(
        &d,
        &Request::Add(AccountAddArgs {
            name: "x".into(),
            kind: AccountKind::ApiKey,
            cred_file: None,
            base_url: None,
            key: None,
            is_default: None,
            dry_run: None,
        }),
        None,
        &Vec::new,
    )
    .unwrap();
    h.write(".claude/plugins/one", "共享库里那一份（夹具句）");
    h.write(".claude/bridge-spawn/one", "认不得的那一份（夹具句）");
    h.write(".claude/.oauth_refresh.lock.owner", "{}");
    std::fs::remove_file(h.0.join(".cc-monitor/accounts/x/skills")).unwrap();
    h.write(".cc-monitor/accounts/x/skills/own.md", "x 自己的（夹具句）");
    h.write(".cc-monitor/accounts/x/.oauth_refresh.lock.owner", "{}");

    let v = run_verify(&d).unwrap();
    let fails: Vec<String> = v
        .checks
        .iter()
        .filter(|c| c.level == CheckLevel::Fail)
        .map(|c| c.text.clone())
        .collect();
    for n in ["lab", "x"] {
        assert_eq!(
            h.link_of(&format!(".cc-monitor/accounts/{n}/plugins")),
            Some(h.0.join(".claude/plugins")),
            "{n} 没链上新冒出来的项"
        );
        assert_eq!(
            h.link_of(&format!(".cc-monitor/accounts/{n}/bridge-spawn")),
            None,
            "{n}：认不得的项被链给了号"
        );
        assert_eq!(
            h.link_of(&format!(
                ".cc-monitor/accounts/{n}/.oauth_refresh.lock.owner"
            )),
            None,
            "{n}：锁文件被链给了号"
        );
    }
    let missing = copy_core::copy_text("beAcctVerify.acct.linkMissing", &[("n", "")]);
    assert!(
        !fails
            .iter()
            .any(|f| f.contains(missing.trim_end_matches(|c| c != '×'))),
        "还在报缺：{fails:?}"
    );
    assert!(
        !v.checks
            .iter()
            .any(|c| c.level != CheckLevel::Ok && c.text.contains(".oauth_refresh.lock")),
        "号里自己的锁文件被当成要处理的：{:?}",
        v.checks
    );
    assert!(h.link_of(".cc-monitor/accounts/x/skills").is_none());
    assert_eq!(
        h.read(".cc-monitor/accounts/x/skills/own.md"),
        "x 自己的（夹具句）"
    );
    assert_eq!(h.read(".claude/plugins/one"), "共享库里那一份（夹具句）");
    let before = h.entries(".cc-monitor/accounts");
    run_verify(&d).unwrap();
    assert_eq!(
        h.entries(".cc-monitor/accounts"),
        before,
        "接好之后再核又动了盘"
    );
}
