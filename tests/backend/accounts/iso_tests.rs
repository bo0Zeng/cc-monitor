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
    let yes: serde_json::Value = serde_json::from_str(&status_line(&Ok(PathBuf::from(
        "/h/.local/bin/cc-acct-iso",
    ))))
    .unwrap();
    let no: serde_json::Value =
        serde_json::from_str(&status_line(&Err("找不到 `cc-acct-iso`：查过 /x".into()))).unwrap();
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

/// ★ 走**入口本体**：`--acct-iso-status` 恒 exit 0（「没装」也是答案，不是错误）；
/// 认不得的动词 / 没有动词 exit 2，且 stderr 是一行 `{code,message}`、stdout 为空。
///
/// ⚠ 本条读的是**真进程环境**（`HOME` / `PATH`）：测试机上装没装 `cc-acct-iso` 会让
/// status 那一行的内容不同 —— 所以只钉与环境无关的那一半（码 ＋ 形状）。
#[test]
fn the_entry_point_keeps_its_exit_code_contract() {
    let st = answer(&["--acct-iso-status".to_string()]);
    assert_eq!(st.code, 0);
    assert!(st.stderr.is_none());
    let v: serde_json::Value = serde_json::from_str(st.stdout.trim()).expect("status 是一行 JSON");
    assert!(v["installed"].is_boolean());
    for bad in [vec!["--acct-iso-nope".to_string()], vec![]] {
        let a = answer(&bad);
        assert_eq!(a.code, 2);
        assert!(a.stdout.is_empty());
        let e: serde_json::Value = serde_json::from_str(a.stderr.as_deref().unwrap()).unwrap();
        assert_eq!(e["code"], "bad_args");
    }
}
