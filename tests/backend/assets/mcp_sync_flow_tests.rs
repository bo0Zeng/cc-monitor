//! 设计/96 §3.5：「写：一趟，不重算」·「`stale` ⇒ 停、说清停在哪 / 前面写了哪几个，**不重读**」—— 推拉的 I/O 进了被写那台后端自己（MIG-3a）。
use super::*;
use crate::assets::mcp_sync::There;
use crate::stream::inbound::LocalFiles;

/// 这台上的事实：`PATH` 查不动（`unknown`），路径一律「没有」—— 金样不把开发机烤进去。
struct NoFacts;
impl Facts for NoFacts {
    fn path(&self, _p: &str) -> There {
        There::Absent
    }
    fn command(&self, _name: &str) -> Option<bool> {
        None
    }
}

fn fixture(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("mig3a-mcpsync-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("src")).unwrap();
    std::fs::create_dir_all(d.join("dst")).unwrap();
    d
}

fn sub(v: &Value, src: &str, dst: &str) -> Value {
    serde_json::from_str(&v.to_string().replace("<SRC>", src).replace("<DST>", dst)).unwrap()
}

/// 跨语言金样：来源交原文 → 被写那台看差异 → 勾两条（一条要盖）写 == 金样；码集合 == 登记表。
#[test]
fn the_mcp_sync_flow_matches_the_cross_language_golden() {
    let g: Value =
        serde_json::from_str(include_str!("../../__fixtures__/mcp-sync-flow.golden.json")).unwrap();
    let root = fixture("golden");
    let (src, dst) = (
        root.join("src").display().to_string(),
        root.join("dst").display().to_string(),
    );
    std::fs::write(
        root.join("src").join(MCP_JSON),
        g["sourceText"].as_str().unwrap(),
    )
    .unwrap();
    std::fs::write(
        root.join("dst").join(MCP_JSON),
        g["targetText"].as_str().unwrap(),
    )
    .unwrap();
    let got = answer_source(&LocalFiles, &json!({ "projectDir": src })).unwrap();
    assert_eq!(got, sub(&g["sourceReply"], &src, &dst));
    let preview = answer_preview(
        &LocalFiles,
        &NoFacts,
        &json!({ "projectDir": dst, "source": got["text"], "sourcePath": got["path"], "sameMachine": true }),
    )
    .unwrap();
    assert_eq!(preview, sub(&g["previewReply"], &src, &dst));
    let applied = answer_apply(
        &LocalFiles,
        &NoFacts,
        &json!({ "projectDir": dst, "source": preview["sourceText"], "target": preview["targetText"], "take": g["take"], "overwrite": g["overwrite"] }),
    )
    .unwrap();
    assert_eq!(applied, sub(&g["applyReply"], &src, &dst));
    let after: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("dst").join(MCP_JSON)).unwrap())
            .unwrap();
    assert_eq!(
        after["mcpServers"],
        json!({ "a": { "command": "a" }, "b": { "command": "b2" }, "c": { "command": "c" } })
    );
    let _ = std::fs::remove_dir_all(&root);
    for (name, want) in g["codes"].as_object().unwrap() {
        let spec = crate::stream::inbound::REGISTRY
            .iter()
            .find(|s| s.name == name)
            .expect("登记表里没有");
        let mut codes: Vec<&str> = spec.codes.to_vec();
        codes.sort_unstable();
        let want: Vec<&str> = want
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap())
            .collect();
        assert_eq!(codes, want, "`{name}` 的拒绝码与金样不相等");
    }
}

/// 看过之后被写那份变了 ⇒ `stale`、一个字节不写（不重读重算）；同一份文件拷给自己 ⇒ 拒。
#[test]
fn a_target_that_changed_after_the_preview_is_left_alone() {
    let root = fixture("stale");
    let dst = root.join("dst").display().to_string();
    std::fs::write(root.join("dst").join(MCP_JSON), "{\"mcpServers\":{}}").unwrap();
    let seen = "{\"mcpServers\":{\"old\":{}}}";
    let (code, _) = answer_apply(
        &LocalFiles,
        &NoFacts,
        &json!({ "projectDir": dst, "source": "{\"mcpServers\":{\"a\":{}}}", "target": seen, "take": ["a"] }),
    )
    .expect_err("看过之后变了还写了");
    assert_eq!(code, "stale");
    assert_eq!(
        std::fs::read_to_string(root.join("dst").join(MCP_JSON)).unwrap(),
        "{\"mcpServers\":{}}"
    );
    let path = root.join("dst").join(MCP_JSON).display().to_string();
    let (code, _) = answer_preview(
        &LocalFiles,
        &NoFacts,
        &json!({ "projectDir": dst, "source": "{}", "sourcePath": path, "sameMachine": true }),
    )
    .expect_err("同一份文件拷给自己也收了");
    assert_eq!(code, "bad_args");
    let _ = std::fs::remove_dir_all(&root);
}
