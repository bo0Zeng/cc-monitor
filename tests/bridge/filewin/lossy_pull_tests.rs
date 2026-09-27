//! 〔FILES2 · 第四波 · 2026-09-27〕`filewin/lossy_pull.rs` 的判据 —— **有损名（非 UTF-8）的下载**。
//!
//! 要求住址：`设计/60 §7` 第 9 条 Q4；主会话 09-27 按通行做法裁：「落到 Linux 本机 ⇒ 字节原样当文件名；落到 Windows 本机 ⇒
//! 名字按有损形替换并在结局里说一句『名字里有认不出的字节，已改成 X』」。远端寻址（就地拷进暂存区再走传输）是本路补的手段（`FILES2.md` 第二节）。
//!
//! | 判据 | 钉的那一形 | 两侧异源在哪 |
//! |---|---|---|
//! | [`a_kept_lossy_name_lands_as_its_raw_bytes_on_unix_and_a_renamed_one_as_typed`] | 尾段没改 ⇒ 父目录 ＋ 原始字节（b16）；改了 ⇒ 原样串 | 期望手写 |
//! | [`a_lossy_pull_stages_by_bytes_downloads_the_staged_copy_and_removes_it`] | 线上顺序与载荷：home → 建暂存区 → 收 0700 → 按字节拷 → 下那份暂存件（落点 b16）→ 删暂存件 | 实得是合成对端真收到的 |
//! | [`the_staging_dir_is_the_backend_one`] | 窗口那份暂存区常量 == 后端 `files_commit::STAGING_DIR` | 读两侧源码 |

use super::*;
use crate::filewin::transfer::tests::{xfer_rig, Ends};

#[test]
#[cfg(unix)]
fn a_kept_lossy_name_lands_as_its_raw_bytes_on_unix_and_a_renamed_one_as_typed() {
    let (v, note) = local_dest("/home/u/dl/f\u{FFFD}.bin", "f\u{FFFD}.bin", b"f\xfe.bin");
    assert_eq!(
        v,
        crate::filewin::source::wire_bytes(b"/home/u/dl/f\xfe.bin"),
        "尾段没改却没换成原始字节"
    );
    assert_eq!(note, None, "Linux 上字节原样，不该出声");
    let (v, note) = local_dest("/home/u/dl/renamed.bin", "f\u{FFFD}.bin", b"f\xfe.bin");
    assert_eq!(v, serde_json::json!("/home/u/dl/renamed.bin"));
    assert_eq!(note, None);
    assert_eq!(
        local_path_of(&crate::filewin::source::wire_bytes(b"/a/f\xfe")),
        Some(std::path::PathBuf::from(
            <std::ffi::OsStr as std::os::unix::ffi::OsStrExt>::from_bytes(b"/a/f\xfe")
        ))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lossy_pull_stages_by_bytes_downloads_the_staged_copy_and_removes_it() {
    let (line, origin, log) = xfer_rig(Ends::Done).await;
    let board = crate::filewin::download::DownloadBoard::default();
    let local = crate::filewin::source::wire_bytes(b"/tmp/f\xfe");
    pull_by_bytes(&line, &origin, b"/srv/d\xff/f\xfe", local.clone(), &board)
        .await
        .expect("该走通");
    let got: Vec<(String, serde_json::Value)> = log
        .lock()
        .unwrap()
        .iter()
        .filter(|(s, _)| s != "dropped")
        .cloned()
        .collect();
    let names: Vec<&str> = got.iter().map(|(s, _)| s.as_str()).collect();
    assert_eq!(
        names,
        [
            "files-home",
            "files-mkdir",
            "files-chmod",
            "files-copy",
            "transfer-download",
            "subscribe",
            "files-delete"
        ],
        "顺序不对"
    );
    // 合成对端把 home 答成 `/srv/a.bin`（它对别的命令一律这么答）。
    let home = "/srv/a.bin";
    assert_eq!(
        got[1].1,
        serde_json::json!({ "root": format!("{home}/.cc-monitor"), "rel": "staging" })
    );
    assert_eq!(got[2].1["mode"], serde_json::json!(0o700));
    let to = got[3].1["to"].as_str().expect("to").to_string();
    let key = to
        .strip_prefix("srv/a.bin/.cc-monitor/staging/")
        .and_then(|t| t.strip_suffix(".part"))
        .unwrap_or_else(|| panic!("暂存件不在暂存区：{to}"));
    assert!(
        key.len() == 32
            && key
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "键不是 32 位小写十六进制：{key}"
    );
    assert_eq!(got[3].1["root"], serde_json::json!("/"));
    assert_eq!(
        got[3].1["from"],
        crate::filewin::source::wire_bytes(b"srv/d\xff/f\xfe"),
        "源没按字节寻址"
    );
    assert_eq!(
        got[4].1,
        serde_json::json!({ "remote_path": format!("/{to}"), "local_path": local }),
        "下的不是那份暂存件 / 落点没按字节"
    );
    assert_eq!(
        got[6].1,
        serde_json::json!({ "root": format!("{home}/.cc-monitor/staging"), "rel": format!("{key}.part") }),
        "删的不是那份暂存件"
    );
}

#[test]
fn the_staging_dir_is_the_backend_one() {
    let backend =
        guard_core::production_code(include_str!("../../../src/backend/control/files_commit.rs"));
    let needle = format!("pub const STAGING_DIR: &str = \"{STAGING_DIR}\";");
    assert_eq!(
        backend.matches(needle.as_str()).count(),
        1,
        "窗口那份暂存区（{STAGING_DIR}）与后端 `files_commit::STAGING_DIR` 不是同一个值"
    );
}
