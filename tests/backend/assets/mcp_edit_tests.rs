//! 设计/99 §2.1 ⑬：「命令实现经 `inbound_client` / `BackendDoor` 碰到后端的必须在「待迁」」—— D 组 MCP 写进了那台后端自己（`mcp-server-put` / `-remove`）。
use super::*;
use crate::stream::inbound::LocalFiles;

fn fixture(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("mig3a-mcp-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("建夹具目录");
    d
}

fn golden() -> Value {
    serde_json::from_str(include_str!("../../__fixtures__/mcp-edit.golden.json"))
        .expect("金样读不出来")
}

fn with_dir(v: &Value, d: &str) -> Value {
    serde_json::from_str(&v.to_string().replace("<DIR>", d)).unwrap()
}

/// 跨语言金样：经**生产**入口（文件管理面 `LocalFiles` → `files-put`）写一条、删一条不在的 == 金样；盘上是规划出的那一份。
#[test]
fn the_mcp_edit_product_matches_the_cross_language_golden() {
    let g = golden();
    let root = fixture("golden");
    let d = root.display().to_string();
    let put = answer_put(&LocalFiles, &with_dir(&g["put"], &d)).expect("写不进去");
    assert_eq!(put, with_dir(&g["putReply"], &d));
    let on_disk = std::fs::read_to_string(root.join(MCP_JSON)).unwrap();
    assert_eq!(on_disk, g["mcpJson"].as_str().unwrap());
    let gone =
        answer_remove(&LocalFiles, &with_dir(&g["removeAbsent"], &d)).expect("删不在的不该拒");
    assert_eq!(gone, with_dir(&g["removeAbsentReply"], &d));
    assert_eq!(
        std::fs::read_to_string(root.join(MCP_JSON)).unwrap(),
        on_disk,
        "删一条不在的动了盘"
    );
    let _ = std::fs::remove_dir_all(&root);
    for name in ["mcp-server-put", "mcp-server-remove"] {
        let spec = crate::stream::inbound::REGISTRY
            .iter()
            .find(|s| s.name == name)
            .expect("登记表里没有");
        let mut codes: Vec<&str> = spec.codes.to_vec();
        codes.sort_unstable();
        let mut want: Vec<&str> = g["codes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap())
            .collect();
        want.sort_unstable();
        assert_eq!(codes, want, "`{name}` 的拒绝码与金样不相等");
    }
}

/// 已存在但读不懂 ⇒ 拒绝覆盖，原文一个字节不动；相对目录 ⇒ 拒。
#[test]
fn a_broken_mcp_json_is_never_overwritten_and_relative_dirs_are_refused() {
    let root = fixture("broken");
    std::fs::write(root.join(MCP_JSON), "{ not json").unwrap();
    let args = json!({ "projectDir": root.display().to_string(), "name": "x", "server": {} });
    let (code, _) = answer_put(&LocalFiles, &args).expect_err("读不懂的也写了");
    assert_eq!(code, "refused");
    assert_eq!(
        std::fs::read_to_string(root.join(MCP_JSON)).unwrap(),
        "{ not json"
    );
    let (code, _) = answer_put(
        &LocalFiles,
        &json!({ "projectDir": "rel/dir", "name": "x", "server": {} }),
    )
    .expect_err("相对目录也收了");
    assert_eq!(code, "bad_path");
    let _ = std::fs::remove_dir_all(&root);
}
