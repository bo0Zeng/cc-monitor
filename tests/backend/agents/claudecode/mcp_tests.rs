//! 〔SH1 · V137〕Claude 的 MCP 布局读法：三段条目 ＋ 项目表 ＋ 坏的那份说出来。
use super::*;

fn fixture(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("sh1-mcp-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("建夹具目录");
    d
}

/// 跨语言金样：夹具经**生产**读法（`read_at`）＋ **生产**构造器（`feature_face::mcp_reply`）现算 == 手写的 reply。
/// 要求住址：`99 §1` V137「MCP 列表改由后端出成品」· `设计/05 §14.3`「成品两侧对拍」。
#[test]
fn the_mcp_product_matches_the_cross_language_golden() {
    let g: serde_json::Value =
        serde_json::from_str(include_str!("../../../__fixtures__/mcp-read.golden.json"))
            .expect("金样读不出来");
    let root = fixture("golden");
    let dir = root.join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    let cj = root.join(".claude.json");
    let d = dir.display().to_string();
    std::fs::write(&cj, g["claudeJson"].as_str().unwrap().replace("<DIR>", &d)).unwrap();
    std::fs::write(dir.join(".mcp.json"), g["mcpJson"].as_str().unwrap()).unwrap();
    let got = crate::feature_face::mcp_reply(&read_at(Some(&cj), Some(&dir)));
    let want: serde_json::Value = serde_json::from_str(
        &g["reply"]
            .to_string()
            .replace("<DIR>", &d)
            .replace("<CJ>", &cj.display().to_string()),
    )
    .unwrap();
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(got, want, "`mcp-read` 成品与金样不相等");
    let spec = crate::inbound::REGISTRY
        .iter()
        .find(|s| s.name == "mcp-read")
        .expect("登记表里没有 `mcp-read`");
    let mut codes: Vec<&str> = spec.codes.to_vec();
    codes.sort_unstable();
    let mut golden: Vec<&str> = g["codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    golden.sort_unstable();
    assert_eq!(codes, golden, "金样的拒绝码与后端登记的不相等");
}

/// 缺 ⇒ 那一段空、不出声；坏 ⇒ 那一段空并说出来。
#[test]
fn a_missing_file_is_silent_and_a_broken_one_is_said() {
    let root = fixture("broken");
    let cj = root.join(".claude.json");
    std::fs::write(&cj, "{ not json").unwrap();
    let got = read_at(Some(&cj), Some(&root.join("absent")));
    let _ = std::fs::remove_dir_all(&root);
    assert!(got.entries.is_empty() && got.dirs.is_empty());
    assert_eq!(
        got.problems.len(),
        1,
        "坏的 .claude.json 要说一句、缺的 .mcp.json 不说：{:?}",
        got.problems
    );
}
