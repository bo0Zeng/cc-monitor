//! 帧面宿主判据里引到上层（命令表 `stream::inbound::REGISTRY` · 出口投影 `views::plan_for` · 走登记表跑一条）的那几条：
//! 帧面宿主是独立的包（`backend-faces`）、够不到上层 ⇒ 经 `cross/faces.rs` 挂主 crate。

use crate::cross_wire::typed_tests::ask;
use crate::faces::read_face::test_support::{answer_at, FAMILY};
use serde_json::json;
use std::path::PathBuf;

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("c1-faces-upper-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("projects")).unwrap();
    d
}

// ── 只读查询宿主（`read_face`）────────────────────────────────────────────────────

/// ★ 本族点名的那几条都真在 `hello.commands` 里（`command_names`，从注册表派生）。
#[test]
fn every_read_face_member_is_announced_in_hello() {
    for n in FAMILY {
        assert!(
            crate::stream::inbound::command_names().contains(n),
            "`{n}` 不在 `inbound::command_names()` —— hello 不会宣告它，monitor 的 `accepts` 会拒"
        );
    }
}

/// ★ **「记录正文按需」那份声明真的作用在三条读记录命令上，而且两头都断。**
///
/// 挑格去格的本体（`faces/project.rs`）由 `project_tests` 逐格钉着；本条钉的是**这几条命令登记的成品住处**
/// （`stream/inbound/views.rs::PLACES`）对得上它们真出的应答：照那一处投影之后，工具入参与工具结果正文一个不剩、
/// 正文与推理一格不少、条数与 `next` / `eof` 一类的格不变。住处登记错了（`lines[].record` 写成 `rows[].record`），
/// 投影就落空、应答原样 —— 前一头会红。
///
/// 两头都断：不带声明 ⇒ 去掉的那几样**必须**在（夹具打到了）；带了 ⇒ 一个不剩，且只少了它们。
#[test]
fn the_three_record_reads_each_honour_the_body_on_demand_view_both_ways() {
    let home = scratch("body-on-demand");
    let dir = home.join("projects").join("-p");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("s.jsonl");
    // 结构占位：`ZQBODY-*` 只住工具入参与工具结果，`ZQKEEP-*` 住正文 · 推理 · 人说的话 —— 不采任何真会话正文。
    let body = [
        r#"{"type":"user","uuid":"u1","timestamp":"2026-01-02T03:04:05.000Z","message":{"role":"user","content":[{"type":"text","text":"ZQKEEP-asked"}]}}"#,
        r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","timestamp":"2026-01-02T03:04:06.000Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":"ZQKEEP-think"},{"type":"text","text":"ZQKEEP-said"},{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/w/f.txt","zq_extra":"ZQBODY-input"}}]}}"#,
        r#"{"type":"user","uuid":"u2","parentUuid":"a1","timestamp":"2026-01-02T03:04:07.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"ZQKEEP-preview\nZQBODY-result"}]}}"#,
    ]
    .iter()
    .map(|r| format!("{r}\n"))
    .collect::<String>();
    std::fs::write(&p, &body).unwrap();
    let path = p.to_string_lossy().to_string();
    const GONE: &[&str] = &["ZQBODY-input", "ZQBODY-result"];
    // 结果的首行预览（`results.*.preview`）是核心另出的一格：省掉结果正文之后它照样在。
    const KEPT: &[&str] = &[
        "ZQKEEP-asked",
        "ZQKEEP-think",
        "ZQKEEP-said",
        "ZQKEEP-preview",
    ];
    let view = json!({"omit": {"record": [
        "blocks[type=tool_use].input",
        "blocks[type=tool_result].content",
        "results.*.patch",
    ]}});

    // 三条各自的入参与装成品的那一格。
    let cases: [(&str, serde_json::Value, &str); 3] = [
        ("history-read", json!({"path": path}), "rows"),
        ("history-page", json!({"path": path}), "lines"),
        ("history-lines", json!({"path": path, "from": 0}), "lines"),
    ];
    for (cmd, args, key) in cases {
        let full = answer_at(&home, cmd, &args, &Default::default())
            .unwrap_or_else(|e| panic!("`{cmd}` 答错了（{:?}）：{}", e.code, e.said));
        let plan = crate::stream::inbound::views::plan_for(cmd, &view)
            .unwrap_or_else(|f| panic!("`{cmd}` 不收这份声明：{f:?}"));
        let fold = plan.apply(Some(full.clone())).unwrap();
        let text = |v: &serde_json::Value| serde_json::to_string(&v[key]).unwrap();
        let count = |v: &serde_json::Value| v[key].as_array().map_or(0, Vec::len);

        // ── 不带声明：去掉的那几样**必须**在 ──
        let t = text(&full);
        for m in GONE.iter().chain(KEPT) {
            assert!(
                t.contains(m),
                "`{cmd}` 全量里没有 `{m}` —— 夹具没打到（下面那一半会恒绿）"
            );
        }
        // ── 带了：工具那几格一个不剩，正文一格不少，条数一条不少 ──
        let t = text(&fold);
        for m in GONE {
            assert!(!t.contains(m), "`{cmd}` 照声明去过之后还在给 `{m}`：{t}");
        }
        for m in KEPT {
            assert!(t.contains(m), "`{cmd}` 把正文 `{m}` 也去掉了");
        }
        assert_eq!(count(&fold), count(&full), "`{cmd}` 去的该是格，不是行");
        assert_eq!(count(&fold), 3, "`{cmd}` 的夹具三行都该出成品");
        let strip = |v: &serde_json::Value| {
            let mut o = v.as_object().unwrap().clone();
            o.remove(key);
            o
        };
        assert_eq!(
            strip(&fold),
            strip(&full),
            "`{cmd}` 的 `{key}` 之外的格也变了"
        );
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ 事实回包照声明投影、没有例外：去掉 `touchedFiles` ⇒ 其余与不带声明那一份逐字相同；点名回包里没有的 `end` · `rev` ⇒ `bad_args`。
#[test]
fn a_projected_facts_reply_is_the_whole_one_minus_what_it_omits() {
    let home = scratch("facts-view");
    let dir = home.join("projects").join("-p");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("s.jsonl");
    std::fs::write(
        &p,
        r#"{"type":"assistant","uuid":"a1","timestamp":"2026-01-02T03:04:06.000Z","message":{"role":"assistant","model":"m","content":[{"type":"tool_use","id":"t-a1","name":"Edit","input":{"file_path":"/w/one.txt","old_string":"a","new_string":"b"}}],"usage":{"input_tokens":3,"output_tokens":4}}}"#.to_string() + "\n",
    )
    .unwrap();
    let ask = || {
        answer_at(
            &home,
            "history-facts",
            &json!({ "path": p.to_string_lossy() }),
            &Default::default(),
        )
        .unwrap()
    };
    let whole = ask();
    assert_eq!(
        whole["touchedFiles"],
        json!(["/w/one.txt"]),
        "夹具没打到（下面那一半会恒绿）"
    );
    let view = json!({"omit": {"facts": ["touchedFiles"]}});
    let plan = crate::stream::inbound::views::plan_for("history-facts", &view).unwrap();
    let mut want = whole.clone();
    want.as_object_mut().unwrap().remove("touchedFiles");
    assert_eq!(plan.apply(Some(ask())).unwrap(), want);
    for gone in ["end", "rev"] {
        let view = json!({"omit": {"facts": [gone]}});
        let e = crate::stream::inbound::views::plan_for("history-facts", &view).unwrap_err();
        assert_eq!(e.code.as_deref(), Some("bad_args"), "{gone}");
    }
    let _ = std::fs::remove_dir_all(&home);
}

// ── 轮换那一族 ──────────────────────────────────────────────────────────────────

/// ★ 登记表里有第五版的全部命令，第三版那 15 条与会话草稿那两条一条都不在（不留别名）。
#[test]
fn the_registry_has_the_v5_rotation_commands_and_none_of_the_old_ones() {
    let names: Vec<&str> = crate::stream::inbound::REGISTRY
        .iter()
        .map(|c| c.name)
        .collect();
    for want in [
        "rotation-facts",
        "rotation-try",
        "rotation-strategies",
        "rotation-view",
        "rotation-settings",
        "rotation-use",
        "rotation-reload",
        "rotation-for-new",
        "rotation-temp",
        "rotation-use-now",
        "rotation-strategy-add",
        "rotation-strategy-copy",
        "rotation-strategy-rename",
        "rotation-strategy-delete",
        "rotation-strategy-tz",
        "quota-read",
    ] {
        assert!(names.contains(&want), "缺 {want}");
    }
    for gone in [
        "rotation-save",
        "rotation-revert",
        "rotation-configs-read",
        "rotation-config-save",
        "rotation-config-rename",
        "rotation-config-delete",
        "rotation-default-set",
        "rotation-strategies-read",
        "rotation-strategy-try",
        "rotation-plan",
        "rotation-session-read",
        "rotation-session-open",
        "rotation-session-edit",
        "rotation-session-save",
        "rotation-session-revert",
        "rotation-session-set",
        "rotation-switch",
    ] {
        assert!(!names.contains(&gone), "{gone} 该删了");
    }
}

/// ★ 登记成 typed、码表照类型：`rotation-facts` 只 `bad_args`；`rotation-try` 是 `bad_args` · `bad_strategy`。
#[test]
fn rotation_facts_and_try_are_registered_typed_with_their_codes() {
    let codes = |name: &str| {
        crate::stream::inbound::REGISTRY
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("{name} 没登记"))
            .codes
    };
    assert_eq!(codes("rotation-facts"), ["bad_args"]);
    assert_eq!(codes("rotation-try"), ["bad_args", "bad_strategy"]);
}

/// ★ 线上只认 kind：后端里面记账用的路由名当 `agent` 交来 ⇒ `bad_args`，不当成那一家（轮换那两条同一个认法）。
#[test]
fn a_route_name_is_not_taken_as_an_agent() {
    let route = crate::agents::claudecode::UPSTREAM.route_id;
    assert_ne!(route, crate::agents::claudecode::AGENT_KIND);
    for (cmd, args) in [
        ("rotation-facts", json!({ "agent": route })),
        ("rotation-try", json!({ "source": "", "agent": route })),
    ] {
        let fail = ask(cmd, args).expect_err(cmd);
        assert_eq!(
            fail.code.as_deref(),
            Some("bad_args"),
            "{cmd}：{}",
            fail.said
        );
    }
}

// ── 计划读面（`plan_face`）：走登记表问的参数形状 ────────────────────────────────

#[test]
fn missing_plan_args_are_bad_args() {
    assert_eq!(
        ask("plan-read", json!({})).unwrap_err().code.as_deref(),
        Some("bad_args")
    );
    assert_eq!(
        ask("plan-cell-view", json!({"workspace": "/w"}))
            .unwrap_err()
            .code
            .as_deref(),
        Some("bad_args")
    );
    assert_eq!(
        ask("plan-list", json!({"dirs": "x"}))
            .unwrap_err()
            .code
            .as_deref(),
        Some("bad_args")
    );
}

/// 入参按类型收：多一个键 ⇒ `bad_args`（不静默不认）。
#[test]
fn an_unknown_plan_arg_is_refused() {
    let f = ask("plan-read", json!({"workspace": "/w", "fresh": true})).unwrap_err();
    assert_eq!(f.code.as_deref(), Some("bad_args"));
    let f = ask(
        "plan-ack",
        json!({"workspace": "/w", "slice": "a", "key": "k", "x": 1}),
    )
    .unwrap_err();
    assert_eq!(f.code.as_deref(), Some("bad_args"));
}

/// `slice` 只跟 `take` 走：`take` 不带 · 别的动词带了 ⇒ `bad_args`（在起 pb 之前）。
#[test]
fn slice_goes_with_take_and_only_with_it() {
    let f = ask("plan-command", json!({"workspace": "/w", "cmd": "take"})).unwrap_err();
    assert_eq!(f.code.as_deref(), Some("bad_args"));
    let f = ask(
        "plan-command",
        json!({"workspace": "/w", "cmd": "pause", "slice": "a"}),
    )
    .unwrap_err();
    assert_eq!(f.code.as_deref(), Some("bad_args"));
}

/// `path` · `domain` 只跟 `repo` 走：`repo` 不带 `path` · 别的动词带了 ⇒ `bad_args`（在起 pb 之前）；`plan-new` 的 `dir` 要绝对路径。
#[test]
fn path_goes_with_repo_and_only_with_it() {
    for bad in [
        json!({"workspace": "/w", "cmd": "repo"}),
        json!({"workspace": "/w", "cmd": "repo", "domain": "软件"}),
        json!({"workspace": "/w", "cmd": "pause", "path": "a"}),
        json!({"workspace": "/w", "cmd": "render", "domain": "软件"}),
    ] {
        assert_eq!(
            ask("plan-command", bad.clone())
                .unwrap_err()
                .code
                .as_deref(),
            Some("bad_args"),
            "{bad}"
        );
    }
    let f = ask("plan-new", json!({"dir": "rel/dir", "path": "x"})).unwrap_err();
    assert_eq!(f.code.as_deref(), Some("bad_args"));
    assert_eq!(
        ask("plan-new", json!({"dir": "/w"}))
            .unwrap_err()
            .code
            .as_deref(),
        Some("bad_args")
    );
}

/// `record` 要 `slice` · `summary` · `quotes` 都在；别的动词带了 `summary` / `quotes` ⇒ `bad_args`（在起 pb 之前）。
#[test]
fn record_takes_slice_summary_and_quotes_and_nothing_else_does() {
    for bad in [
        json!({"workspace": "/w", "cmd": "record", "summary": "x", "quotes": ["y"]}),
        json!({"workspace": "/w", "cmd": "record", "slice": "a"}),
        json!({"workspace": "/w", "cmd": "pause", "summary": "x"}),
        json!({"workspace": "/w", "cmd": "take", "slice": "a", "quotes": ["y"]}),
    ] {
        assert_eq!(
            ask("plan-command", bad.clone())
                .unwrap_err()
                .code
                .as_deref(),
            Some("bad_args"),
            "{bad}"
        );
    }
}

/// `plan-refs {sid, texts, docs}`：入参按类型收（缺 · 多 ⇒ `bad_args`）；认不出片 ⇒ 两样 `null`、一段对一段 · 一棵对一棵地空，不起 pb。
#[test]
fn plan_refs_takes_sid_texts_and_trees_and_answers_one_each() {
    for bad in [
        json!({"texts": ["A1"], "docs": []}),
        json!({"sid": "s", "texts": []}),
        json!({"sid": "s", "texts": [], "docs": [], "x": 1}),
        json!({"sid": "s", "texts": [], "docs": ["A1"]}),
    ] {
        assert_eq!(
            ask("plan-refs", bad.clone()).unwrap_err().code.as_deref(),
            Some("bad_args"),
            "{bad}"
        );
    }
    let tree = serde_json::to_value(md_core::parse("看 A1")).unwrap();
    let r = ask(
        "plan-refs",
        json!({"sid": "00000000-no-such-session", "texts": ["看 A1", "", "B2"], "docs": [tree]}),
    )
    .unwrap();
    assert_eq!(
        r,
        Some(json!({"workspace": null, "slice": null, "refs": [[], [], []], "docs": [null]}))
    );
}
