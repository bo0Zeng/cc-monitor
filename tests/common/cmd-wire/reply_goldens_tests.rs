//! 手机读的那几条回包的跨语言金样（`tests/__fixtures__/wire-replies.golden.json`）。
//!
//! 每条命令一份结构夹具（`sample`：每格都填、并集取第一形，`node tests/scripts/gen-kotlin-wire.mjs --samples` 现出，不放真正文），
//! 这里按 `calls!` 那张表里的类型（写了 `reads` 的照那一形）解、照类型写成一行（`rust`）。手机那一侧（`RepliesGoldenTest`）拿生成的 Kotlin 类解 `rust`
//! 再写回，要与它逐字节同 —— 生成的类与 Rust 那一份类型对不上（少一格 · 型不对 · 顺序不同 · null 与缺席分不清）就红在那边。
//! 判据只比不写：现打的与仓里那份不一样 ⇒ 现打的落在本树 `.build/golden/` 下、红；对过之后跑 `tests/scripts/bless-golden.sh` 拷回。

use super::{typed_text, COMMANDS};

const REL: &str = "tests/__fixtures__/wire-replies.golden.json";

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

#[test]
fn every_phone_reply_has_a_golden_line_written_by_its_type() {
    let path = root().join(REL);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读不到 {REL}：{e}"));
    let golden: serde_json::Value = serde_json::from_str(&text).expect("金样不是 JSON");
    let cases = golden["cases"].as_object().expect("金样没有 cases");
    let mut fresh = serde_json::Map::new();
    for cmd in COMMANDS {
        let sample = cases
            .get(*cmd)
            .and_then(|c| c.get("sample"))
            .unwrap_or_else(|| panic!("{cmd} 没有结构夹具（`--samples` 现出那一份贴进 cases）"))
            .clone();
        let rust = typed_text(cmd, sample.clone())
            .expect("表里的命令")
            .unwrap_or_else(|e| panic!("{cmd} 的结构夹具解不开：{e}"));
        fresh.insert(
            cmd.to_string(),
            serde_json::json!({ "sample": sample, "rust": rust }),
        );
    }
    let extra: Vec<&String> = cases
        .keys()
        .filter(|k| !COMMANDS.contains(&k.as_str()))
        .collect();
    assert!(extra.is_empty(), "金样里有表外的命令：{extra:?}");
    let want = serde_json::json!({
        "说明": [
            "手机读的那几条回包的跨语言金样。",
            "sample：结构夹具（每格都填、并集取第一形；node tests/scripts/gen-kotlin-wire.mjs --samples 现出，不放真正文）。",
            "rust：cmd-wire 的 calls! 表按类型解 sample、照类型写成的那一行（tests/common/cmd-wire/reply_goldens_tests.rs）。",
            "Kotlin：core-wire 的 RepliesGoldenTest 拿生成的类解 rust、再写回，逐字节同。"
        ],
        "cases": fresh,
    });
    let out = serde_json::to_string_pretty(&want).unwrap() + "\n";
    let fresh_path = root().join(".build/golden").join(REL);
    if out != text {
        std::fs::create_dir_all(fresh_path.parent().unwrap()).unwrap();
        std::fs::write(&fresh_path, &out).unwrap();
        panic!("{REL} 与现打的不一致：现打的在 .build/golden/{REL}；对过后跑 bash tests/scripts/bless-golden.sh 拷回");
    }
    let _ = std::fs::remove_file(&fresh_path);
}
