//! 读正文核判据里拿帧面那一层的投影（`faces::project`：出口交的 `omit` 声明）去剥一页成品的那两条：
//! 观测面是独立的包（`backend-observe`）、够不到帧面 ⇒ 挂在主 crate（经 `cross/observe.rs`）。一页成品经 `query::body::test_support` 读。

use crate::observe::query::body::test_support::{record_lines_of_page, rows_of_page};

/// 夹具里只住**正文**那几格的标记（照那份声明投影 ⇒ 一个都不许剩）。
/// 每个标记只有一个出处：思考 · 说的话 · 工具入参里主参数之外的那一格 · 工具结果正文 · 逐段改动里的一行。
const GONE: &[&str] = &[
    "ZQBODY-think",
    "ZQBODY-say",
    "ZQBODY-input",
    "ZQBODY-result",
    "ZQBODY-patch",
];

/// **折起那一行自己要用的**那几格里的标记（照那份声明投影 ⇒ 一个都不许少）。
/// 它们在原文里也住正文块，但后端判好的成品（`who.text`）里**另有一份** ⇒ 剥正文是**去重**，不是把人说的话弄丢。
/// 这一半不立，「不许有正文」那一半把投影整个弄坏也能恒绿。
/// 结果的首行预览（`results.*.preview`）是核心另出的一格：剥了结果正文它照样在（折起那一行写它）。
const KEPT: &[&str] = &["ZQKEEP-user", "ZQKEEP-queued", "ZQKEEP-preview"];

/// 折起那一行要用的格（照那份声明投影 ⇒ 逐个还在、值不变；它们正是界面画那一行读的那几格）。
const FOLDED_CELLS: &[(&str, &str)] = &[("u1", "who"), ("a1", "steps"), ("u3", "results")];

const FOLD_PAGE: &[&str] = &[
    r#"{"type":"user","uuid":"u1","timestamp":"2026-01-02T03:04:05.000Z","cwd":"/w","message":{"role":"user","content":[{"type":"text","text":"ZQKEEP-user"}]}}"#,
    r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","timestamp":"2026-01-02T03:04:06.000Z","message":{"role":"assistant","model":"m","content":[{"type":"thinking","thinking":"ZQBODY-think"},{"type":"text","text":"ZQBODY-say"},{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/w/f.txt","zq_extra":"ZQBODY-input"}}],"usage":{"input_tokens":11,"output_tokens":22}}}"#,
    r#"{"type":"user","uuid":"u2","parentUuid":"a1","timestamp":"2026-01-02T03:04:07.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"ZQKEEP-preview\nZQBODY-result"}]},"toolUseResult":{"type":"text","file":{"numLines":7}}}"#,
    r#"{"type":"user","uuid":"u3","parentUuid":"u2","timestamp":"2026-01-02T03:04:07.500Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t2","content":"ok"}]},"toolUseResult":{"filePath":"/w/f.txt","structuredPatch":[{"oldStart":1,"oldLines":1,"newStart":1,"newLines":1,"lines":["-a","+ZQBODY-patch"]}]}}"#,
    r#"{"type":"attachment","uuid":"q1","parentUuid":"u3","timestamp":"2026-01-02T03:04:08.000Z","attachment":{"type":"queued_command","prompt":"ZQKEEP-queued","commandMode":"prompt","origin":{"kind":"human"}}}"#,
];

/// 「折起那一行」那份声明（出口交的 `omit`）：正文住的那几格。
fn folded_view() -> crate::faces::project::View {
    crate::faces::project::parse_view(&serde_json::json!({"omit": {"record": [
        "blocks", "results.*.patch",
    ]}}))
    .unwrap()
    .unwrap()
}

/// ★ **两头都断**：照「折起那一行」那份声明投影 ⇒ 正文那几格一个不剩、折起那一行要用的一格不少；不投影 ⇒ 正文**必须**在。
///
/// 两头各自都不够：只断「投影 ⇒ 不许有正文」的话，把投影整个弄坏（一条都不出成品）也能让它绿；
/// 只断「不投影 ⇒ 有正文」的话，声明根本没接上也能绿。⇒ 本条逐标记两向都判，再加上**条数 · 行号 · 身份一格不变**。
/// 记录成品真出的格（本页）与声明里写的格名（格目录那一套）对不上 ⇒ 投影落空 ⇒ 红。
#[test]
fn the_folded_view_drops_every_body_cell_and_keeps_every_folded_cell() {
    let page = FOLD_PAGE.join("\n") + "\n";
    let at = std::path::Path::new("/p/zq.jsonl");
    let view = folded_view();
    let shape = |fold: bool| {
        let (mut lines, next) = record_lines_of_page(at, 5, 0, page.as_bytes());
        let mut rows = rows_of_page(0, page.as_bytes());
        if fold {
            for l in lines.iter_mut().chain(rows.iter_mut()) {
                if let Some(r) = l.get_mut("record") {
                    crate::faces::project::project("record", r, &view);
                }
            }
        }
        let text = serde_json::to_string(&lines).unwrap();
        (lines, next, rows, text)
    };
    let (full_lines, full_next, full_rows, full_text) = shape(false);
    let (fold_lines, fold_next, fold_rows, fold_text) = shape(true);

    for m in GONE.iter().chain(KEPT) {
        assert!(
            full_text.contains(m),
            "不投影时 `{m}` 不在成品里 —— 夹具或投影坏了，下面那一半会恒绿"
        );
    }
    for m in GONE {
        assert!(
            !fold_text.contains(m),
            "投影过，正文标记 `{m}` 还在成品里：{fold_text}"
        );
    }
    for m in KEPT {
        assert!(
            fold_text.contains(m),
            "投影把折起那一行要显示的 `{m}` 也剥掉了"
        );
    }
    let cell = |lines: &[serde_json::Value], id: &str, key: &str| {
        lines
            .iter()
            .find(|l| l["record"]["id"] == id)
            .unwrap_or_else(|| panic!("夹具里没有 id={id} 那一条"))["record"][key]
            .clone()
    };
    for (id, key) in FOLDED_CELLS {
        let (fold, full) = (cell(&fold_lines, id, key), cell(&full_lines, id, key));
        assert!(
            !full.is_null(),
            "夹具里 `{key}`（id={id}）没有 —— 下面那一条会恒绿"
        );
        if *key == "results" {
            // 结果一句留着，只少了逐段改动。
            assert_eq!(fold["t2"]["added"], full["t2"]["added"]);
            assert!(fold["t2"].get("patch").is_none() && full["t2"].get("patch").is_some());
        } else {
            assert_eq!(fold, full, "`{key}`（id={id}）在折起那一形里变了");
        }
    }
    let a1 = fold_lines
        .iter()
        .find(|l| l["record"]["id"] == "a1")
        .unwrap();
    assert!(
        a1["record"].get("blocks").is_none(),
        "`blocks` 该是**删掉**而不是给空值（给空值等于说这一条没有正文，那是假话）：{a1}"
    );

    assert_eq!(fold_next, full_next, "剥正文不许动行号");
    assert_eq!(fold_lines.len(), full_lines.len(), "剥正文不许少出一条");
    assert_eq!(fold_lines.len(), FOLD_PAGE.len(), "夹具每一行都该出成品");
    let ident = |lines: &[serde_json::Value]| -> Vec<serde_json::Value> {
        lines
            .iter()
            .map(|l| {
                serde_json::json!([
                    l["seq"].clone(),
                    l["session_id"].clone(),
                    l["path"].clone(),
                    l["cwd"].clone(),
                    l["record"]["id"].clone(),
                    l["record"]["t"].clone(),
                    l["record"]["timeText"].clone(),
                ])
            })
            .collect()
    };
    assert_eq!(
        ident(&fold_lines),
        ident(&full_lines),
        "行标识 / 时刻字格变了"
    );
    let ends = |rows: &[serde_json::Value]| -> Vec<serde_json::Value> {
        rows.iter()
            .map(|r| serde_json::json!([r["end"].clone(), r["hash"].clone(), r["cwd"].clone()]))
            .collect()
    };
    assert_eq!(
        ends(&fold_rows),
        ends(&full_rows),
        "行摘要的 `end` / `hash` / `cwd` 变了（续传要靠它们核「还是不是那一行」）"
    );
    assert!(
        !fold_text.contains("\"blocks\""),
        "折起那一形里还剩 `blocks`：{fold_text}"
    );
    assert!(
        full_text.contains("\"blocks\""),
        "不投影时连 `blocks` 都没有 —— 上面那一条会恒绿"
    );
}

/// ★ 桌面交的那份声明（`src/shared/views/folded-record.json`，界面 `record-reads.ts` 与壳的旁路快照读同一份）：
/// 核心认得它的每一格（`parse_view` 不拒）；照它投影 ⇒ 工具入参 · 结果正文 · 逐段改动没了，思考 · 说的话 · 预览 · 主参数那一句还在。
#[test]
fn the_desktop_folded_view_in_shared_parses_and_drops_only_the_tool_bodies() {
    let decl: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../src/shared/views/folded-record.json"
    ))
    .expect("那份声明是 JSON");
    let view = crate::faces::project::parse_view(&decl)
        .unwrap_or_else(|e| panic!("核心不认那份声明：{e}"))
        .expect("声明不是空的");
    // 再加一步改文件：改前改后进了 `steps[id].body`，那一格也是正文。
    let edit = r#"{"type":"assistant","uuid":"a2","parentUuid":"u3","timestamp":"2026-01-02T03:04:07.700Z","message":{"role":"assistant","model":"m","content":[{"type":"tool_use","id":"t3","name":"Edit","input":{"file_path":"/w/f.txt","old_string":"a","new_string":"ZQBODY-change"}}],"usage":{"input_tokens":1,"output_tokens":1}}}"#;
    let page = FOLD_PAGE.join("\n") + "\n" + edit + "\n";
    let at = std::path::Path::new("/p/zq.jsonl");
    let (mut lines, _) = record_lines_of_page(at, 5, 0, page.as_bytes());
    let full = serde_json::to_string(&lines).unwrap();
    for l in &mut lines {
        if let Some(r) = l.get_mut("record") {
            crate::faces::project::project("record", r, &view);
        }
    }
    let fold = serde_json::to_string(&lines).unwrap();
    for m in [
        "ZQBODY-input",
        "ZQBODY-result",
        "ZQBODY-patch",
        "ZQBODY-change",
    ] {
        assert!(full.contains(m), "不投影时 `{m}` 不在 —— 下面那一条会恒绿");
        assert!(!fold.contains(m), "照桌面那份声明投影，`{m}` 还在：{fold}");
    }
    for m in [
        "ZQBODY-think",
        "ZQBODY-say",
        "ZQKEEP-user",
        "ZQKEEP-preview",
        "/w/f.txt",
    ] {
        assert!(fold.contains(m), "桌面那份声明把 `{m}` 也剥了");
    }
}
