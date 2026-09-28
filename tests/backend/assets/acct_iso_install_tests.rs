//! 〔MIG-3a · 子步 3 · 主会话 09-28 裁〕cc-acct-iso 落进用户目录：链接走文件管理面的 `files-link`、配置走 `files-put`，
//! 装卸账记进 skill 装记录（`name="acct-iso"` · `at="home"`）。门 = 本进程的 `files-*`，只有 `files-home` 答临时目录。
use super::*;
use crate::assets::aliases::tests::HomeDoor;
use std::path::PathBuf;

struct TmpHome(PathBuf);
impl Drop for TmpHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn home(tag: &str) -> TmpHome {
    let h = std::env::temp_dir().join(format!(
        "mig3a-acctiso-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|x| x.as_nanos())
            .unwrap_or(0)
    ));
    let d = h.join(".cc-monitor/bin/cc-acct-iso");
    std::fs::create_dir_all(d.join("scripts")).unwrap();
    std::fs::create_dir_all(d.join("examples")).unwrap();
    std::fs::write(
        d.join("scripts/cc-acct-iso"),
        "#!/usr/bin/env bash\necho iso\n",
    )
    .unwrap();
    std::fs::write(d.join("examples/config"), "# 示例\n").unwrap();
    TmpHome(h)
}
fn dir_of(h: &TmpHome) -> String {
    h.0.join(".cc-monitor/bin/cc-acct-iso")
        .display()
        .to_string()
}

/// ★ 装一次：链接指向主脚本、配置从随包那份拷、目录 0700、记进装记录（`at:"home"`，路径相对家目录）；
/// 再装一次：两样都已在 ⇒ 一个都不动、不再记（幂等）。
#[cfg(unix)]
#[test]
fn install_links_copies_the_sample_and_records_it_once() {
    use std::os::unix::fs::PermissionsExt;
    let h = home("once");
    let seen = std::sync::Mutex::new(Vec::<Value>::new());
    let rec = |v: &Value| {
        seen.lock().unwrap().push(v.clone());
        Ok(json!({}))
    };
    let d = HomeDoor(h.0.clone());
    let got = answer_install(&d, &rec, &json!({ "dir": dir_of(&h) })).expect("装");
    assert_eq!(got["linked"], true);
    assert_eq!(got["configWritten"], true);
    let link = h.0.join(LINK_REL);
    assert_eq!(
        std::fs::read_link(&link).unwrap(),
        h.0.join(".cc-monitor/bin/cc-acct-iso/scripts/cc-acct-iso")
    );
    assert_eq!(
        std::fs::read_to_string(h.0.join(".cc-acct-iso/config")).unwrap(),
        "# 示例\n"
    );
    let mode = std::fs::metadata(h.0.join(CONFIG_DIR_REL))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o700, "配置目录不是 0700（{mode:o}）");
    let recs = seen.lock().unwrap().clone();
    assert_eq!(recs.len(), 1);
    assert_eq!(recs[0]["name"], NAME);
    assert_eq!(recs[0]["at"], "home");
    let files = recs[0]["files"].as_object().unwrap();
    assert!(
        files.contains_key(LINK_REL) && files.contains_key(".cc-acct-iso/config"),
        "{files:?}"
    );
    let again = answer_install(&d, &rec, &json!({ "dir": dir_of(&h) })).expect("再装");
    assert_eq!(
        (again["linked"].clone(), again["configWritten"].clone()),
        (json!(false), json!(false))
    );
    assert_eq!(seen.lock().unwrap().len(), 1, "什么都没建，不该再记");
}

/// 反向：装在哪不在 `~/.cc-monitor/bin/` 底下 ⇒ 拒、一个字节不动；链接那儿已经有别人的东西 ⇒ 不动它、照实说。
#[test]
fn it_refuses_a_foreign_dir_and_leaves_an_existing_link_alone() {
    let h = home("foreign");
    let d = HomeDoor(h.0.clone());
    let rec = |_: &Value| Ok(json!({}));
    let (code, _) = answer_install(
        &d,
        &rec,
        &json!({ "dir": h.0.join("elsewhere").display().to_string() }),
    )
    .expect_err("家目录别处也装了");
    assert_eq!(code, "refused");
    assert!(!h.0.join(LINK_REL).exists());
    std::fs::create_dir_all(h.0.join(".local/bin")).unwrap();
    std::fs::write(h.0.join(LINK_REL), "mine\n").unwrap();
    let got = answer_install(&d, &rec, &json!({ "dir": dir_of(&h) })).expect("装");
    assert_eq!(got["linked"], false);
    assert_eq!(
        std::fs::read_to_string(h.0.join(LINK_REL)).unwrap(),
        "mine\n",
        "用户自己的东西被动了"
    );
}
