//! 命令那一层里纯的几样（入参的形状由 typed 那一处收，判据在 `inbound_tests`）：别名事件按前后账号表之差 ·
//! 导入凭据的路径只收家目录底下 · 登录那一行的拼法；再加核对先对齐那一条（临时家目录里真建库）。
//! 其余落盘那一半在 `faces/accounts_face_tests.rs` 真跑。夹具全是合成的。
use super::*;

/// 临时家目录（用完删）。
#[cfg(unix)]
struct Home(std::path::PathBuf);
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
    fn link_of(&self, rel: &str) -> Option<std::path::PathBuf> {
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
    let d = crate::assets::aliases::tests::HomeDoor(h.0.clone());
    run_change(
        &d,
        &Request::Init(AccountInitArgs {
            name: "lab".into(),
            dry_run: None,
        }),
        None,
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
    let missing = copy_text("beAcctVerify.acct.linkMissing", &[("n", "")]);
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

/// 别名事件只跟着账号表之差：消失的按删号、新出现的按建号；没变 ⇒ 空。
#[test]
fn alias_events_follow_the_difference_of_the_two_tables() {
    let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    assert_eq!(
        alias_events(&v(&["a", "b"]), &v(&["b", "c"])),
        [
            AliasEvent::Removed("a".into()),
            AliasEvent::Added("c".into())
        ]
    );
    assert!(alias_events(&v(&["a"]), &v(&["a"])).is_empty());
    assert_eq!(
        alias_events(&[], &v(&["x"])),
        [AliasEvent::Added("x".into())]
    );
}

/// 导入凭据的路径：`~/` 按家展开、尾部 `/` 去掉；家目录外 · 家目录本身 · 相对 · `..` 段 · 控制符一律 `refused`。
#[test]
fn a_credential_path_must_live_under_home() {
    assert_eq!(cred_path("/h", "~/c/cred.json").unwrap(), "/h/c/cred.json");
    assert_eq!(cred_path("/h", " /h/c/ ").unwrap(), "/h/c");
    // `~/` · `/h/` 是家目录本身（这一条补测试时逮到：尾部 `/` 去掉之前判「不是家」，放过去了）。
    for bad in [
        "/etc/cred",
        "/h",
        "/h/",
        "~/",
        "c/cred",
        "/h/../etc",
        "/h/a\nb",
        "/hx/cred",
    ] {
        assert_eq!(
            cred_path("/h", bad).unwrap_err().code.as_deref(),
            Some("refused"),
            "{bad:?}"
        );
    }
}

/// 登录那一行：`<家>/.cc-monitor/bin/ccm -- --account <名>`，名字过 quote（带空格 · 引号也不拆开）。
#[test]
fn the_login_line_quotes_the_name() {
    let q = shell_quote_core::posix_quote;
    assert_eq!(
        login_line("/h/", "z"),
        format!("{} -- --account {}", q("/h/.cc-monitor/bin/ccm"), q("z"))
    );
    let odd = login_line("/h", "a b'c");
    assert!(
        odd.ends_with(&format!(" --account {}", q("a b'c"))),
        "{odd}"
    );
    assert_ne!(shell_quote_core::posix_quote("a b'c"), "a b'c");
}
