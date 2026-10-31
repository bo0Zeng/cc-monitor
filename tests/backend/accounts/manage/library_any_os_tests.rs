//! 账号库在**哪一个系统上都**落得成（不只 Unix）：临时家目录里造一个登录过的 `~/.claude`，经本进程的文件管理面
//! （只有 `files-home` 答临时目录）建库、加一个订阅号，看盘上落成了什么、核对报不报错。
//! 同一份在 Linux 门禁里跑，也交叉编到 Windows 上真跑（`cargo test --target x86_64-pc-windows-gnu --no-run` 之后在那台跑）。
//! 夹具只造结构，凭据是写死的假串，邮箱是 `*.test`。
use super::wire::{run_change, run_verify, Request};
use crate::assets::aliases::tests::HomeDoor;
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
    let init = run_change(&d, &Request::Init(take(json!({ "name": "lab" }))), None);
    if !can_link(&t.0) {
        let said =
            crate::platform::acct_view::unsupported_said().expect("建不了链接却说做得了多账号");
        let e = init.err().expect("做不了多账号却建成了");
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
