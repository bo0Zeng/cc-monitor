//! 〔FILES2 · 第四波 · 2026-09-27〕`filewin/chunk_upload.rs` 的判据 —— **上传的块形**（SFTP 起始目录不是后端 home 那台机器）。
//!
//! 要求住址：`设计/60 §7` 第 3 / 9 条 Q5；主会话 09-27 按通行做法裁：「连上时比 SFTP `realpath(".")` 与那台后端的 `$HOME`；
//! 不一致 ⇒ 这台的上传改走后端链路分块写（`files-stage-chunk` 那一族），不走 SFTP 暂存；出声一次说原因」。
//!
//! | 判据 | 钉的那一形 | 两侧异源在哪 |
//! |---|---|---|
//! | [`a_mismatch_switches_this_window_to_chunks_once_and_the_chunks_rebuild_the_file`] | 传输台答 `sftp_home_mismatch` ⇒ 这一件改走块形、`via_backend` 记下原因；块拼回来 == 本机那份、提交带 `chunks` / `bytes` / 整份摘要；下一件直接走块形、不再开 SFTP 的单 | 期望是本机文件自己的字节与 `sha2` 现算的摘要 |
//! | [`hex_is_lowercase_and_byte_exact`] | 十六进制与 `format!("{:02x}")` 逐字相同 | 期望由标准库现算 |

use super::*;
use crate::filewin::transfer::tests::{steps, xfer_rig, Ends};
use sha2::Digest as _;

#[test]
fn hex_is_lowercase_and_byte_exact() {
    let b: Vec<u8> = (0u8..=255).collect();
    let want: String = b.iter().map(|x| format!("{x:02x}")).collect();
    assert_eq!(hex(&b), want);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_mismatch_switches_this_window_to_chunks_once_and_the_chunks_rebuild_the_file() {
    let dir = std::env::temp_dir().join(format!("ccm-cu-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("临时目录");
    let body: Vec<u8> = (0..(CHUNK_RAW * 2 + 1000))
        .map(|i| (i * 7 % 251) as u8)
        .collect();
    let local = dir.join("big.bin");
    std::fs::write(&local, &body).expect("铺本机文件");
    let (line, origin, log) = xfer_rig(Ends::Mismatch).await;
    let board = crate::filewin::transfer::DropBoard::default();
    let item = crate::filewin::transfer::Pending::into_remote_dir(&local.to_string_lossy(), "/srv")
        .expect("待传");
    crate::filewin::transfer::upload_remote(&line, &origin, &item, &board)
        .await
        .expect("块形该走通");
    assert!(
        board.via_backend().is_some_and(|w| w.contains("home")),
        "改走后端链路却没记下原因"
    );
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
            "transfer-upload",
            "subscribe",
            "files-stage-chunk",
            "files-stage-chunk",
            "files-stage-chunk",
            "files-commit-upload"
        ],
        "顺序不对"
    );
    assert_eq!(
        got[1].1["home"],
        serde_json::json!("/srv/a.bin"),
        "开单没带后端的 home"
    );
    let mut rebuilt = Vec::new();
    let key = got[3].1["key"].clone();
    for (i, (_, a)) in got[3..6].iter().enumerate() {
        assert_eq!(a["seq"], serde_json::json!(i), "块号不对");
        assert_eq!(a["key"], key, "几块不是同一个键");
        let hexed = a["content"]["b16"].as_str().expect("b16");
        rebuilt.extend(
            (0..hexed.len() / 2).map(|k| u8::from_str_radix(&hexed[2 * k..2 * k + 2], 16).unwrap()),
        );
    }
    assert_eq!(rebuilt, body, "块拼回来不是本机那份");
    let want_sha: String = sha2::Sha256::digest(&body)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect();
    assert_eq!(
        got[6].1,
        serde_json::json!({
            "key": key, "root": "/srv", "rel": "big.bin", "overwrite": false,
            "expect": { "sha256": want_sha }, "chunks": 3, "bytes": body.len(),
        })
    );
    // 下一件：这一窗已经改走后端链路 ⇒ 不再开 SFTP 的单。
    let before = steps(&log)
        .iter()
        .filter(|s| *s == "transfer-upload")
        .count();
    crate::filewin::transfer::upload_remote(&line, &origin, &item, &board)
        .await
        .expect("第二件块形该走通");
    assert_eq!(
        steps(&log)
            .iter()
            .filter(|s| *s == "transfer-upload")
            .count(),
        before,
        "改走后端链路之后又开了 SFTP 的单"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// 窗口认的那个收场码 == 后端传输台发的那个（两个 crate 互相引不到，读后端源码钉相等）。
#[test]
fn the_mismatch_code_is_the_backend_one() {
    let backend =
        guard_core::production_code(include_str!("../../../src/backend/control/transfer.rs"));
    let needle = format!(
        "pub const SFTP_HOME_MISMATCH: &str = \"{}\";",
        crate::filewin::transfer::SFTP_HOME_MISMATCH
    );
    assert_eq!(
        backend.matches(needle.as_str()).count(),
        1,
        "两侧的收场码不是同一个"
    );
}
