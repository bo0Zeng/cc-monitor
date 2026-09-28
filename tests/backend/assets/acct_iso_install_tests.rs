//! 〔MIG-3a · 子步 3 · 主会话 09-28 裁 2 · 09-28 预裁〕cc-acct-iso 装到这台：随二进制带着的字节经 `files-put` 落 `~/.cc-monitor/bin/cc-acct-iso/`、
//! 链接走 `files-link`、配置走 `files-put`，装卸账记进 skill 装记录（`name="acct-iso"` · `at="home"`）。
//! 门 = 本进程的 `files-*`，只有 `files-home` 答临时目录。
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
    std::fs::create_dir_all(&h).unwrap();
    TmpHome(h)
}

/// ★ 装一次：六份字节 ＋ 身份标记落在部署那一根、脚本可执行；链接指向主脚本；配置从随包那份拷、目录 0700；
/// 这一趟写了的全记进装记录（`at:"home"`，路径相对家目录）。再装一次：全都一致 ⇒ 一个字节不写、不再记（幂等）。
#[cfg(unix)]
#[test]
fn install_lands_the_embedded_bytes_links_copies_the_sample_and_records_it_once() {
    use std::os::unix::fs::PermissionsExt;
    let h = home("once");
    let seen = std::sync::Mutex::new(Vec::<Value>::new());
    let rec = |v: &Value| {
        seen.lock().unwrap().push(v.clone());
        Ok(json!({}))
    };
    let d = HomeDoor(h.0.clone());
    let got = answer_install(&d, &rec).expect("装");
    assert_eq!(got["written"], FILES.len());
    assert_eq!(got["version"], vendor_id());
    assert_eq!(
        (got["linked"].clone(), got["configWritten"].clone()),
        (json!(true), json!(true))
    );
    let dest = h.0.join(DEST_REL);
    for (rel, bytes, exec) in FILES {
        assert_eq!(
            std::fs::read(dest.join(rel)).unwrap(),
            *bytes,
            "{rel} 落盘的不是内嵌那一份"
        );
        let mode = std::fs::metadata(dest.join(rel))
            .unwrap()
            .permissions()
            .mode()
            & 0o111;
        assert_eq!(mode != 0, *exec, "{rel} 的可执行位不对（{mode:o}）");
    }
    assert_eq!(
        std::fs::read_to_string(dest.join(".vendor_id"))
            .unwrap()
            .trim(),
        vendor_id()
    );
    assert_eq!(
        std::fs::read_link(h.0.join(LINK_REL)).unwrap(),
        dest.join("scripts/cc-acct-iso")
    );
    let sample = FILES.iter().find(|f| f.0 == "examples/config").unwrap().1;
    assert_eq!(
        std::fs::read(h.0.join(".cc-acct-iso/config")).unwrap(),
        sample
    );
    let mode = std::fs::metadata(h.0.join(CONFIG_DIR_REL))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o700, "配置目录不是 0700（{mode:o}）");
    let recs = seen.lock().unwrap().clone();
    assert_eq!(recs.len(), 1);
    assert_eq!(
        (recs[0]["name"].clone(), recs[0]["at"].clone()),
        (json!(NAME), json!("home"))
    );
    let files = recs[0]["files"].as_object().unwrap();
    for k in [
        LINK_REL,
        ".cc-acct-iso/config",
        ".cc-monitor/bin/cc-acct-iso/scripts/cc-acct-iso",
        ".cc-monitor/bin/cc-acct-iso/.vendor_id",
    ] {
        assert!(files.contains_key(k), "装记录里没有 {k}：{files:?}");
    }
    let again = answer_install(&d, &rec).expect("再装");
    assert_eq!(
        (
            again["written"].clone(),
            again["linked"].clone(),
            again["configWritten"].clone()
        ),
        (json!(0), json!(false), json!(false))
    );
    assert_eq!(seen.lock().unwrap().len(), 1, "什么都没写，不该再记");
}

/// 反向：部署那一根里有一份旧的 ⇒ 按读到的那一份换成内嵌那一份（记成「不是这一趟建的」）；
/// 链接那儿已经有别人的东西 ⇒ 不动它、照实说；配置已在 ⇒ 不覆盖。
#[test]
fn it_replaces_a_stale_payload_and_leaves_the_users_things_alone() {
    let h = home("stale");
    let dest = h.0.join(DEST_REL);
    std::fs::create_dir_all(dest.join("scripts")).unwrap();
    std::fs::write(dest.join("scripts/lib.sh"), "# 旧的\n").unwrap();
    std::fs::create_dir_all(h.0.join(".local/bin")).unwrap();
    std::fs::write(h.0.join(LINK_REL), "mine\n").unwrap();
    std::fs::create_dir_all(h.0.join(".cc-acct-iso")).unwrap();
    std::fs::write(h.0.join(".cc-acct-iso/config"), "# 我的\n").unwrap();
    let seen = std::sync::Mutex::new(Vec::<Value>::new());
    let rec = |v: &Value| {
        seen.lock().unwrap().push(v.clone());
        Ok(json!({}))
    };
    let got = answer_install(&HomeDoor(h.0.clone()), &rec).expect("装");
    let lib = FILES.iter().find(|f| f.0 == "scripts/lib.sh").unwrap().1;
    assert_eq!(
        std::fs::read(dest.join("scripts/lib.sh")).unwrap(),
        lib,
        "旧的那份没换掉"
    );
    assert_eq!(
        (got["linked"].clone(), got["configWritten"].clone()),
        (json!(false), json!(false))
    );
    assert_eq!(
        std::fs::read_to_string(h.0.join(LINK_REL)).unwrap(),
        "mine\n",
        "用户自己的东西被动了"
    );
    assert_eq!(
        std::fs::read_to_string(h.0.join(".cc-acct-iso/config")).unwrap(),
        "# 我的\n"
    );
    let recs = seen.lock().unwrap().clone();
    let files = recs[0]["files"].as_object().unwrap();
    assert_eq!(
        files[".cc-monitor/bin/cc-acct-iso/scripts/lib.sh"]["created"],
        false
    );
    assert!(!files.contains_key(LINK_REL) && !files.contains_key(".cc-acct-iso/config"));
}

/// 内嵌那几份**恰是指纹覆盖的那几份、按菜谱那个顺序**：`.vendor_id` = 那几份依次拼起来的 sha256 前 16 位（`VENDOR.md` 菜谱）
/// ⇒ 少内嵌一份 / 多一份 / 换了顺序 / 内容漂了，这一比都对不上（装出去缺文件、而标记说齐了）；每一份内嵌的也就是盘上那一份。
#[test]
fn the_embedded_files_match_the_vendored_tree() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../shared/cc-acct-iso");
    let mut all = Vec::new();
    for (rel, bytes, _) in FILES {
        assert_eq!(
            std::fs::read(root.join(rel)).unwrap(),
            *bytes,
            "{rel} 内嵌的不是盘上那一份"
        );
        all.extend_from_slice(bytes);
    }
    let sha = crate::files::content_sha256(&all);
    assert_eq!(
        &sha[..16],
        vendor_id(),
        "内嵌那几份（按这个顺序）算出的指纹不是 `.vendor_id`"
    );
    assert_eq!(
        std::fs::read_to_string(root.join(".vendor_id"))
            .unwrap()
            .trim(),
        vendor_id()
    );
}
