//! 文件管理那一块的判据里引到上层的那几条：问法要走命令那一层的解码（`wire::typed::take`）与门，
//! 那一块是独立的包、够不到上层 ⇒ 挂在主 crate。

use crate::stream::inbound::files_write_face::write_tests::ask;

/// 解压的命令面：回的键 == 声明的 `fields`；路径参数收 b16。
#[test]
fn the_extract_command_face_answers_with_the_declared_fields() {
    let base = std::env::temp_dir().join(format!("ccm-fx-face-{}", std::process::id()));
    std::fs::remove_dir_all(&base).ok();
    std::fs::create_dir_all(&base).expect("建临时根");
    let mut b = tar::Builder::new(Vec::new());
    let mut h = tar::Header::new_gnu();
    h.set_entry_type(tar::EntryType::Regular);
    h.set_mode(0o644);
    h.set_size(1);
    b.append_data(&mut h, "f", &b"1"[..]).expect("写 tar 条目");
    std::fs::write(base.join("p.tar"), b.into_inner().expect("tar 收尾")).expect("铺包");
    let v = ask(
        "files-extract",
        &serde_json::json!({"root": base.to_string_lossy(), "rel": {"b16": "702e746172"}}),
    )
    .expect("解压被拒");
    // 回包就是那一份线上类型（多一格 · 少一格都解不回来）。
    serde_json::from_value::<cmd_wire::files::FilesTreeReply>(v).expect("回包照类型解得回来");
    assert_eq!(std::fs::read(base.join("p/f")).expect("f"), b"1");
    std::fs::remove_dir_all(&base).ok();
}
