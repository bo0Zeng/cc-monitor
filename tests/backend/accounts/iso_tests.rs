//! # 要求住址：`INVARIANTS §42` → `src/doc/IPC-PROTOCOL.md` §10 入方向 `acct-iso-status` / `acct-iso-shellinit` 两小节
//!
//! 核原文：`acct-iso-status` 条逐字「先查 `$HOME/.local/bin/cc-acct-iso`（install 脚本的软链落点）、再查 `PATH`」·
//! 「**「没装」是答案不是错误**（`ok:true`、`installed:false`）」；`acct-iso-shellinit` 条逐字「退出码 0 时它的 stdout **原样**」，
//! 错误码 `not_installed` · `timed_out` · `tool_failed` · `not_run` —— 本族与这几句一一对上。〔JA1 点址 2026-09-24 · LOC1a 09-25 随帧面改址〕

use super::*;
use crate::plugin::invoke::{Done, NotRun, TIMED_OUT_CODE};

/// 固定候选恰好一条：`$HOME/.local/bin/cc-acct-iso`（install 脚本的软链落点，
/// 与远端那条把 `~/.local/bin` 前置到 `PATH` 同一个顺序）。`HOME` 拿不到 ⇒ 没有固定候选。
#[test]
fn the_fixed_candidate_is_the_install_scripts_link() {
    assert_eq!(
        fixed_candidates(Some(Path::new("/h"))),
        vec![PathBuf::from("/h/.local/bin/cc-acct-iso")]
    );
    assert!(fixed_candidates(None).is_empty());
}

/// 「没装」是一个**确定的答案**（exit 0 那一支），不是错误；它说得出查过哪儿。
/// 装了 ⇒ 带路径、不带 `looked`。两支的键集**逐字相同**（前端按同一个形状读）。
#[test]
fn the_status_line_has_one_shape_for_both_answers() {
    let yes = status_value(&Ok(PathBuf::from("/h/.local/bin/cc-acct-iso")));
    let no = status_value(&Err("找不到 `cc-acct-iso`：查过 /x".into()));
    assert_eq!(yes["installed"], true);
    assert_eq!(yes["path"], "/h/.local/bin/cc-acct-iso");
    assert!(yes["looked"].is_null());
    assert_eq!(no["installed"], false);
    assert!(no["path"].is_null());
    assert_eq!(no["looked"], "找不到 `cc-acct-iso`：查过 /x");
    let keys = |v: &serde_json::Value| v.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    assert_eq!(keys(&yes), keys(&no));
}

fn done(code: Option<i32>, out: &str, err: &str) -> Result<Done, NotRun> {
    Ok(Done {
        code,
        stdout: out.as_bytes().to_vec(),
        stderr: err.as_bytes().to_vec(),
    })
}

/// 只有退出码 0 才算产出了片段，而且 stdout **原样**交出去（围栏由 monitor 那侧判）。
/// warn 走 stderr、码仍是 0 的那一类照样交出片段 —— 不许因为 stderr 非空就判失败。
#[test]
fn only_exit_zero_hands_the_snippet_over_verbatim() {
    let snippet = "# ===== BEGIN cc-acct-iso =====\nzcc() { :; }\n# ===== END cc-acct-iso =====\n";
    assert_eq!(
        shellinit_outcome(done(Some(0), snippet, "warn: manifest 里没有默认账号")),
        Ok(snippet.to_string())
    );
    let failed = shellinit_outcome(done(Some(1), "半截", "die: 没有 manifest"));
    assert_eq!(failed.as_ref().unwrap_err().0, "tool_failed");
    assert!(
        failed.as_ref().unwrap_err().1.contains("没有 manifest"),
        "失败原因要带上它自己的那句话（stderr 优先）：{failed:?}"
    );
}

/// 四种失败各有各的码 —— 调用方要分得出「超时」「它自己失败」「根本没起来」。
#[test]
fn the_failure_codes_are_told_apart() {
    let codes: Vec<&str> = [
        shellinit_outcome(done(Some(TIMED_OUT_CODE), "", "")),
        shellinit_outcome(done(Some(2), "", "x")),
        shellinit_outcome(done(None, "", "")),
        shellinit_outcome(Err(NotRun::Failed("起不来".into()))),
        shellinit_outcome(Err(NotRun::ArgListTooLong)),
    ]
    .iter()
    .map(|r| r.as_ref().unwrap_err().0)
    .collect();
    assert_eq!(
        codes,
        vec![
            "timed_out",
            "tool_failed",
            "tool_failed",
            "not_run",
            "not_run"
        ]
    );
}

/// ★ 走**帧面入口本体**：`acct-iso-status` 恒 `Ok`（「没装」也是答案，不是错误）；`acct-iso-shellinit` 要么 `Ok{snippet}`、
/// 要么是那四个码之一（不许冒出别的码）。
///
/// ⚠ 本条读的是**真进程环境**（`HOME` / `PATH`）：测试机上装没装 `cc-acct-iso` 会让内容不同 ——
/// 所以只钉与环境无关的那一半（码 ＋ 形状）。
#[test]
fn the_entry_point_keeps_its_answer_contract() {
    let st = answer_wire_status().expect("status 从不报错");
    assert!(st["installed"].is_boolean());
    let keys = st.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    assert_eq!(keys.len(), 3, "{st}");
    match answer_wire_shellinit() {
        Ok(v) => assert!(v["snippet"].is_string(), "{v}"),
        Err((code, _)) => assert!(
            ["not_installed", "timed_out", "tool_failed", "not_run"].contains(&code),
            "冒出了登记外的码 {code}"
        ),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 〔DUP2 · 主会话 09-26 裁 J4〕`acct-iso-cmd`：后端出那一行，界面零拼 shell 串。
// 要求住址：`设计/01 §1.1`「命令串……都不在前端」· `设计/90 §3` 判据 2 · `src/doc/IPC-PROTOCOL.md` §10 `acct-iso-cmd` 一节。
// 期望是**手写**的（原先界面那份 `buildAcctIsoCmd` 的逐字期望原样搬来，输出一个字节没变）。
// ═══════════════════════════════════════════════════════════════════════════

fn cmd_of(args: serde_json::Value) -> Result<String, (&'static str, String)> {
    answer_wire_cmd(&args).map(|v| v["cmd"].as_str().expect("cmd 是字符串").to_string())
}

/// 七个步骤各一行，逐字（与界面那份原先的期望同字节）。
#[test]
fn every_step_renders_the_same_line_the_front_end_used_to_build() {
    use serde_json::json;
    let cases = [
        (
            json!({"step":"init-preview","name":"z"}),
            "cc-acct-iso init 'z'",
        ),
        (
            json!({"step":"init-apply","name":"z"}),
            "cc-acct-iso init 'z' --apply",
        ),
        (json!({"step":"verify"}), "cc-acct-iso verify"),
        (json!({"step":"shellinit"}), "cc-acct-iso shellinit"),
        (json!({"step":"sync-apply"}), "cc-acct-iso sync --apply"),
        (
            json!({"step":"add-apply","name":"b"}),
            "cc-acct-iso add 'b' --apply",
        ),
        (
            json!({"step":"add-apply","name":"b","credFile":"/home/z/.claude/accounts/b.json"}),
            "cc-acct-iso add 'b' --from-credentials '/home/z/.claude/accounts/b.json' --apply",
        ),
        (json!({"step":"login","name":"z"}), "cc-acct-iso run 'z'"),
    ];
    for (args, want) in cases {
        assert_eq!(cmd_of(args.clone()).as_deref(), Ok(want), "{args}");
    }
    // 单引号进路径：唯一的 quote 断开成 `'\''`，结果不含双引号（monitor `launch.rs` 远端命令那道拒收面收得下）。
    let got = cmd_of(json!({"step":"add-apply","name":"b","credFile":"/p/it's/x.json"})).unwrap();
    assert!(got.contains("'/p/it'\\''s/x.json'"), "{got}");
    assert!(!got.contains('"'), "{got}");
}

/// 账号名过共享那一份（与建号工具逐字同）：真实名字放行；`.` · 33 位 · `-` / `_` 开头 · 元字符拒（`refused`，句子点出值）。
#[test]
fn the_account_name_is_judged_by_the_one_shared_rule() {
    use serde_json::json;
    for ok in ["z", "work", "a_b-c", "A1", &"a".repeat(32)] {
        cmd_of(json!({"step":"login","name":ok}))
            .unwrap_or_else(|e| panic!("{ok:?} 被拒了：{e:?}"));
    }
    for bad in ["", "z.edu", "-x", "_x", "a;rm", "a b", &"a".repeat(33)] {
        let e = cmd_of(json!({"step":"init-apply","name":bad})).expect_err(bad);
        assert_eq!(e.0, "refused", "{bad:?}：{e:?}");
        assert!(
            e.1.contains(&format!("{bad:?}")),
            "{bad:?}：那一句没点出是哪个值：{}",
            e.1
        );
    }
}

/// 快照路径那道校验：`"` · 控制符 · 空 各拒，句子各不相同；普通路径放行。
/// 〔FIX · `99 §2 ㊹` 甲〕`-` 开头放行（`cc-acct-iso` 把下一个词原样当值），渲出来原样单引号。
#[test]
fn the_snapshot_path_keeps_its_three_refusals_and_lets_a_leading_dash_through() {
    use serde_json::json;
    let with = |p: &str| cmd_of(json!({"step":"add-apply","name":"b","credFile":p}));
    with("/home/u/snap.json").expect("普通路径该放行");
    for p in ["--apply", "-x.json"] {
        let step = with(p).unwrap_or_else(|e| panic!("{p:?} 被拒过头：{e:?}"));
        assert!(
            step.contains(&format!("--from-credentials '{p}' --apply")),
            "{p:?}"
        );
    }
    let said: Vec<String> = ["", "a\"b", "a\u{7}b", "a\u{85}b"]
        .iter()
        .map(|p| {
            let e = with(p).expect_err(p);
            assert_eq!(e.0, "refused", "{p:?}：{e:?}");
            e.1
        })
        .collect();
    // 控制符那两形（C0 · C1）说的是同一类；空 · 双引号 · 控制符三类两两不同。
    assert!(
        said[2].contains("控制字符") && said[3].contains("控制字符"),
        "{said:?}"
    );
    let distinct: std::collections::BTreeSet<&str> = [
        said[0].as_str(),
        said[1].as_str(),
        said[2].split('：').next().unwrap_or(""),
    ]
    .into_iter()
    .collect();
    assert_eq!(distinct.len(), 3, "{said:?}");
}

/// 契约错（调用方是界面，用户手敲不出来）⇒ `bad_args`：不认识的 step · 该带的没带 · 不该带的带了 · 类型不对 · 超上界。
#[test]
fn a_malformed_request_is_a_contract_error_not_a_refusal() {
    use serde_json::json;
    for bad in [
        json!("不是对象"),
        json!({}),
        json!({"step":"deploy"}),
        json!({"step":"login"}),
        json!({"step":"verify","name":"z"}),
        json!({"step":"init-apply","name":"z","credFile":"/p"}),
        json!({"step":"add-apply","name":"b","credFile":1}),
        json!({"step":"add-apply","name":"b","extra":true}),
        json!({"step":"add-apply","name":"b","credFile":"/".repeat(CMD_MAX_PATH_BYTES + 1)}),
    ] {
        let e = cmd_of(bad.clone()).expect_err(&bad.to_string());
        assert_eq!(e.0, "bad_args", "{bad}：{e:?}");
    }
}

/// 跨语言金样：`tests/__fixtures__/acct-iso-cmd.golden.json` 的每一问喂给**生产**的 `answer_wire_cmd`，
/// 答 == 金样里手写的 `cmd`（或拒码 == `code`）。界面那一侧拿同一份当「后端会怎么答」（`tests/test-support/acct-iso-cmd-fake.ts`）。
#[test]
fn the_acct_iso_cmd_answers_match_the_cross_language_golden() {
    let g: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/acct-iso-cmd.golden.json"))
            .expect("金样读不出来");
    let cases = g["cases"].as_array().expect("金样缺 cases");
    assert!(cases.len() >= 10, "金样只有 {} 条 —— 读坏了", cases.len());
    let mut wrong = Vec::new();
    for c in cases {
        let got = answer_wire_cmd(&c["args"]);
        match (c["cmd"].as_str(), c["code"].as_str(), &got) {
            (Some(want), None, Ok(v))
                if v["cmd"] == want && v.as_object().map(|o| o.len()) == Some(1) => {}
            (None, Some(code), Err((got_code, _))) if *got_code == code => {}
            _ => wrong.push(format!("{} ⇒ {got:?}", c["args"])),
        }
    }
    assert_eq!(wrong, Vec::<String>::new(), "后端的答与金样对不上");
}

/// 〔MIG-3a · `99 §2.1 ⑬`〕围栏校验进了后端：两条都在才交出去（fail-closed）；有 BEGIN 没 END ⇒ `fence_incomplete`
/// （话里劝别贴）；连 BEGIN 都没有 ⇒ `no_fence`。围栏常量与 vendored `cc-acct-iso` 真打印的那两行逐字一致（跨语言双写点）。
#[test]
fn the_shellinit_fence_is_checked_here_and_matches_the_vendored_script() {
    let (b, e) = (SHELLINIT_FENCE_BEGIN, SHELLINIT_FENCE_END);
    let whole = format!("{b}\nzcc() {{ :; }}\n{e}\n");
    assert_eq!(fenced(whole.clone()), Ok(whole));
    let (code, said) = fenced(format!("{b}\nzcc() {{ :; }}\n")).unwrap_err();
    assert_eq!(code, "fence_incomplete");
    assert!(said.contains("别贴"), "{said}");
    assert_eq!(fenced(String::new()).unwrap_err().0, "no_fence");
    assert_eq!(fenced(e.to_string()).unwrap_err().0, "no_fence");
    let script = include_str!("../../../src/shared/cc-acct-iso/scripts/cc-acct-iso");
    assert!(script.len() > 1000, "vendored 脚本没读进来");
    for fence in [b, e] {
        assert!(
            script.contains(&format!("printf '{fence}\\n'")),
            "双写点漂移：vendored cc-acct-iso 里找不到打印 {fence:?} 的那行"
        );
    }
}
