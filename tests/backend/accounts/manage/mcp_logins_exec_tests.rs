//! 各号之间同步 MCP 登录照那一家声明的「MCP 登录文件」那一格走：替身家声明一份行式文件 ＋「一条一份文件锁」「整份一把文件锁」两把，
//! 两个号之间同步一条，另一条与写法一字不变、两把锁照声明依次拿。走一遍真文件系统（临时家目录），夹具全是现编的假值。
#![cfg(unix)]

use crate::assets::aliases::test_support::HomeDoor;
use serde_json::json;
use std::path::PathBuf;

struct Tmp(PathBuf);
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
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
}

/// 两家替身：`fake`（a1 · a2）· `fake2`（b1）；都是替身家的账号面（MCP 登录那一格照它声明的）。
fn families() -> [crate::agents::Adapter; 2] {
    let fam = |kind, env, wrapper, default| {
        crate::agents::fake::family_row(kind, env, wrapper, default, || None)
    };
    [
        fam("fake", "CCM_FAKE_A", "fakea", true),
        fam("fake2", "CCM_FAKE_B", "fakeb", false),
    ]
}

fn library(tag: &str) -> Tmp {
    let t = Tmp(test_home::scratch(tag));
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

fn face() -> crate::agents::McpLogins {
    crate::agents::accounts_face_among(&families(), "fake")
        .and_then(|f| f.mcp_logins)
        .expect("替身家声明了 MCP 登录那一格")
}

fn logins_rel(acct: &str) -> String {
    format!(".cc-monitor/accounts/{acct}/{}", face().file)
}

/// 声明里那两把锁文件（相对号目录）：替身家声明的是「一条一份文件锁」＋「整份一把文件锁」，依次拿。
fn lock_rels(key: &str) -> [String; 2] {
    use crate::agents::McpLoginLock::{EntryFile, WholeFile};
    match face().lock {
        [EntryFile { name: entry }, WholeFile { name: whole }] => [entry(key), whole.to_string()],
        other => panic!("替身家该声明一条一份文件锁 ＋ 整份一把文件锁，声明的是 {other:?}"),
    }
}

/// 在号 `acct` 里拿住一份锁文件上的排他锁（判据扮那一家自己正在写）。
fn hold_lock(t: &Tmp, acct: &str, rel: &str) -> (PathBuf, std::fs::File) {
    let lock = t.0.join(format!(".cc-monitor/accounts/{acct}/{rel}"));
    std::fs::create_dir_all(lock.parent().unwrap()).unwrap();
    let held = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock)
        .unwrap();
    held.try_lock().expect("判据先拿到那把锁");
    (lock, held)
}

fn sync(t: &Tmp) -> crate::accounts::manage::mcp_logins_exec::Synced {
    let own = crate::platform::fs::Own::issue_data_at(&t.0.join(".cc-monitor"));
    crate::accounts::manage::mcp_logins_exec::sync_among(&HomeDoor(t.0.clone()), &own, &families())
        .expect("登录那一半一趟")
}

// 一条的 JSON 照 serde_json 写出来的键序（同一条搬到别的号原样落成这一行）；svc-2 故意不按这个序，判它一字不动。
const SVC_ONE: &str =
    r#"login svc-1 {"bearer":"fake-b1","client":"c-1","renewal":"fake-r1","until":2000000000000}"#;
const SVC_TWO: &str =
    r#"login svc-2 {"bearer":"fake-b2","renewal":"fake-r2","until":2000000000000,"client":"c-2"}"#;

/// ★ a1 有 svc-1 · svc-2，a2 只有 svc-2：同步一趟 a2 只多出 svc-1 那一行，别的行（注释 · 设置 · svc-2 · 写法）一个字节不变；
/// a1 一个字节不动；写之前照声明依次拿那一条那份锁文件 · 整份那份锁文件上的文件锁（任一把锁住时不写，都放了再补上），
/// 没建任何 `mkdir` 锁；另一家的号不收。
#[test]
fn a_login_syncs_between_two_accounts_taking_both_declared_file_locks() {
    let t = library("mcp-logins-fake");
    let a1 = format!("# a1 自己的注释\nmode = fast\n{SVC_ONE}\n{SVC_TWO}\n");
    let a2 = format!("# a2\n{SVC_TWO}\ncolor = red\n");
    let b1 = "# b1 只有自己\n".to_string();
    t.write(&logins_rel("a1"), &a1);
    t.write(&logins_rel("a2"), &a2);
    t.write(&logins_rel("b1"), &b1);
    let [entry_rel, whole_rel] = lock_rels("svc-1");

    // 那个号自己正在写（那一条那把 · 整份那把，各有人持着一回）⇒ 这一趟都不写。
    let mut locks = Vec::new();
    for rel in [&entry_rel, &whole_rel] {
        let (lock, held) = hold_lock(&t, "a2", rel);
        assert!(sync(&t).changed.is_empty(), "{rel} 有人持着还是写了");
        assert_eq!(t.read(&logins_rel("a2")), a2, "{rel} 有人持着，a2 变了");
        drop(held);
        locks.push(lock);
    }

    // 都放了 ⇒ 补上，只多那一行。
    let got = sync(&t);
    assert_eq!(got.changed, vec!["a2".to_string()], "{:?}", got.notes);
    assert_eq!(t.read(&logins_rel("a2")), format!("{a2}{SVC_ONE}\n"));
    assert_eq!(t.read(&logins_rel("a1")), a1, "登录的那个号一个字节不动");
    assert_eq!(t.read(&logins_rel("b1")), b1, "另一家的号收了这一家的");
    // 锁是照声明那一形拿的：两份锁文件在、是文件；号目录里没有别的锁目录。
    for lock in &locks {
        assert!(lock.is_file(), "没照声明拿 {} 上的文件锁", lock.display());
    }
    let dirs: Vec<String> = crate::platform::fs::Ro::read_dir(t.0.join(".cc-monitor/accounts/a2"))
        .unwrap()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|ty| ty.is_dir()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let lock_dir = entry_rel.split('/').next().unwrap().to_string();
    assert_eq!(dirs, vec![lock_dir], "多建了别的锁目录");
    // 共享集合里不落令牌；再来一趟没有要写的。
    let store = t.read(relay_route_core::ACCOUNTS_MCP_REL);
    assert!(
        !store.contains("fake-b1") && !store.contains("fake-r1"),
        "共享集合里有了令牌"
    );
    assert!(sync(&t).changed.is_empty(), "第二趟还有要写的");
}
