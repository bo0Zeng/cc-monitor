//! `files-delete-session` 一条命令做完：删成那份会话记录，那台的那一条历史注解也没了（界面不再另问一条）；
//! 删不成 ⇒ 注解一个字节不动。
//!
//! 沙箱：家（`projects/`）与数据目录（注解那份文件）都在这次运行独占的临时目录里。

use super::*;
use crate::platform::fs::{Face, Own};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// 这次运行的替身家：落点那一问是 `fn` 指针，指不到局部变量 ⇒ 住一份进程内只设一次的静态，各条判据共用、各用各的 sid。
static HOME: OnceLock<PathBuf> = OnceLock::new();

fn home() -> &'static Path {
    HOME.get_or_init(|| {
        let home = std::env::temp_dir()
            .join(format!("ccm-dsf-{}", std::process::id()))
            .join("home");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("projects/-p")).expect("建项目目录");
        home
    })
}

fn locate_in_sandbox(sid: &str) -> Result<PathBuf, String> {
    crate::agents::claudecode::paths::session_file_for_delete_in(home(), sid)
}

/// 生产那一个窄口，只把落点换成替身家里找。
const PORT: crate::files::write::SessionPort = crate::files::write::SessionPort {
    locate: locate_in_sandbox,
    ..SESSION_PORT
};

/// 这一条判据自己的数据目录，铺好两条注解：`sid` 那条 ＋ 一条邻居。
fn data_with(tag: &str, sid: &str, other: &str) -> PathBuf {
    let data = home().with_file_name(format!("data-{tag}"));
    let _ = std::fs::remove_dir_all(&data);
    std::fs::create_dir_all(&data).expect("建数据目录");
    std::fs::write(
        data.join(creds_core::store::HISTORY_METADATA_FILE),
        serde_json::json!({
            "version": 1,
            "entries": {
                sid: {"starred": true, "customTitle": "要删的那条", "updatedAt": 1},
                other: {"starred": true, "updatedAt": 2},
            },
        })
        .to_string(),
    )
    .expect("铺注解");
    data
}

fn annotations(data: &Path) -> String {
    std::fs::read_to_string(data.join(creds_core::store::HISTORY_METADATA_FILE))
        .expect("注解那份文件在")
}

fn entries(data: &Path) -> serde_json::Map<String, serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(&annotations(data)).expect("注解是 JSON");
    v["entries"].as_object().expect("entries 是对象").clone()
}

fn delete(own: &Own, sid: &str) -> Result<cmd_wire::files::FilesPathReply, FilesDeleteSessionErr> {
    delete_then_forget(
        &Face::issue(),
        own,
        cmd_wire::files::FilesDeleteSessionReq {
            sid: sid.to_string(),
        },
        &PORT,
    )
}

#[test]
fn deleting_an_annotated_session_also_drops_its_annotation() {
    let sid = "2f1c9a4e-0b7d-4c1e-9a55-3b2f0c8d1e77";
    let other = "0000aaaa-0000-4000-8000-0000000000ff";
    let record = home().join(format!("projects/-p/{sid}.jsonl"));
    std::fs::write(&record, "{}\n").expect("铺会话");
    let data = data_with("annot", sid, other);

    let got = delete(&Own::issue_data_at(&data), sid).expect("删一份真在的会话");

    assert!(!record.exists(), "说删了，记录还在：{got:?}");
    let left = entries(&data);
    assert!(
        !left.contains_key(sid),
        "会话删了，它那条注解还在：{left:?}"
    );
    assert!(left.contains_key(other), "邻居那条注解被连带删了：{left:?}");
}

#[test]
fn a_session_that_does_not_delete_keeps_its_annotation() {
    let sid = "7d0e1b2c-3a4f-4e5d-8c6b-9a0f1e2d3c4b";
    let other = "0000aaaa-0000-4000-8000-0000000000fe";
    let data = data_with("gone", sid, other);
    let before = annotations(&data);

    let got = delete(&Own::issue_data_at(&data), sid);

    assert!(got.is_err(), "盘上没有那份记录，删会话却说成了：{got:?}");
    assert_eq!(annotations(&data), before, "删会话没成，注解被动了");
}
