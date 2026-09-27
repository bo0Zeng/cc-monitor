//! 〔FILES2 · 第四波 · 2026-09-27〕`control/files_upload_chunks.rs` 的行为判据。
//! 要求住址：`设计/60 §7` 第 9 条 Q5；主会话 09-27 裁「不一致 ⇒ 这台的上传改走后端链路分块写（`files-stage-chunk` 那一族）」。
//! 块形拼成暂存件之后走的是与 SFTP 那条路**同一条**提交（整份摘要核 · 改名上位）；线上那一臂（`chunks` 参数）的接线判据住
//! `files_commit_tests::the_commit_face_assembles_chunks_when_asked`。

use super::*;

fn home(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-uc-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&p).ok();
    std::fs::create_dir_all(p.join(".cc-monitor")).expect("铺家");
    std::fs::create_dir_all(p.join("dst")).expect("铺目标");
    p
}

fn sha(b: &[u8]) -> String {
    crate::files::content_sha256(b)
}

fn stage(h: &Path, key: &str, parts: &[&[u8]]) {
    for (i, b) in parts.iter().enumerate() {
        crate::control::files_commit::stage_chunk(h, key, i as u64, b).expect("送块");
    }
}

/// ★ 块拼对 ⇒ 落进目标逐字节相等、块与暂存件都不剩；少一块 / 总长不符 ⇒ 目标一个字节不动、块也收掉。
#[test]
fn chunks_assemble_into_one_commit_and_a_bad_set_lands_nothing() {
    let h = home("ok");
    let key = "a".repeat(32);
    let body: Vec<u8> = (0u8..=255).cycle().take(3000).collect();
    stage(&h, &key, &[&body[..1000], &body[1000..2500], &body[2500..]]);
    assemble_part(&h, &key, 3, 3000).expect("拼块被拒");
    crate::control::files_commit::commit_upload(
        &h,
        &key,
        &h.join("dst"),
        "f.bin",
        false,
        &sha(&body),
    )
    .expect("拼好之后的提交被拒");
    assert_eq!(std::fs::read(h.join("dst/f.bin")).expect("目标"), body);
    let staging = h.join(crate::control::files_commit::STAGING_DIR);
    for n in [
        format!("{key}.part"),
        format!("{key}.0.chunk"),
        format!("{key}.2.chunk"),
    ] {
        assert!(
            std::fs::symlink_metadata(staging.join(&n)).is_err(),
            "{n} 还在"
        );
    }
    // 反：说 3 块只送了 2 块 ⇒ 拒，目标不在，块收掉。
    let key2 = "b".repeat(32);
    stage(&h, &key2, &[b"xx", b"yy"]);
    let e = assemble_part(&h, &key2, 3, 4).expect_err("少一块却拼成了");
    assert_eq!(e.code(), "io_failed", "{e:?}");
    assert!(
        std::fs::symlink_metadata(h.join("dst/g.bin")).is_err(),
        "目标被写了"
    );
    assert!(
        std::fs::symlink_metadata(staging.join(format!("{key2}.0.chunk"))).is_err(),
        "块没收"
    );
    assert!(
        std::fs::symlink_metadata(staging.join(format!("{key2}.part"))).is_err(),
        "半份暂存件留着"
    );
    std::fs::remove_dir_all(&h).ok();
}
