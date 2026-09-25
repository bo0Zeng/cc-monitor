use super::*;

#[test]
fn jsonl_stem_basics() {
    assert_eq!(
        jsonl_stem("/home/pi/.claude/projects/p/abc-123.jsonl").as_deref(),
        Some("abc-123")
    );
    assert_eq!(jsonl_stem("abc.jsonl").as_deref(), Some("abc"));
    assert_eq!(jsonl_stem("/x/y/note.txt"), None);
    assert_eq!(jsonl_stem(""), None);
}

// 〔C4d · 第四波 4B〕「首行是 hello ⇒ 老后端」那条识别（`is_old_backend_hello`〔散文墓碑〕）随逐次拨号那条路删了：
// 帧面的老后端由能力协商说「还不认」（`frame_query::call` 发之前先问 `accepts`）。

#[test]
fn shell_quote_via_ssh_source() {
    assert_eq!(
        crate::ssh_source::shell_quote("/a/b c.jsonl"),
        "'/a/b c.jsonl'"
    );
    assert_eq!(crate::ssh_source::shell_quote("a'b"), r"'a'\''b'");
}

/// 〔C4d · 第四波 4B〕🔴 **「迁移前」那一半：monitor 从前那份 join（行 ＋ 注解 ⇒ 项目 / 会话行）读同一份输入，
/// 与后端新那份出的跨语言金样逐格相等**（fork 那两格除外：monitor 那份从来不带，后端那份补上了 —— 行为变化，写在 `C4d.md`）。
///
/// 守的要求：主会话 09-25 裁（`调研/第四波记录/C4d.md`「主会话裁」第 2 条）「本机后端 … 并上注解、出成品（join 只一个家）」——
/// 换家那一刻，远端项目 / 会话行不许变样。输入与后端 `tests/backend/history_join_tests.rs` 用的**逐字相同**（两份字面量，
/// 金样由后端产出、这里读）；注解读同一份夹具。⚠ monitor 这一份随后续子步删掉之后，金样就是「迁移后」对「迁移前」的冻结读数。
#[test]
fn c4d_the_old_join_agrees_with_the_backend_golden() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/__fixtures__");
    let metadata: crate::history::HistoryMetadata = serde_json::from_str(
        &std::fs::read_to_string(dir.join("history-metadata.fixture.json")).unwrap(),
    )
    .unwrap();
    let golden: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("history-products.golden.json")).unwrap(),
    )
    .unwrap();
    const S1: &str = "0000aaaa-0000-4000-8000-000000000001";
    const S2: &str = "0000aaaa-0000-4000-8000-000000000002";
    const S3: &str = "0000aaaa-0000-4000-8000-000000000003";
    const S4: &str = "0000aaaa-0000-4000-8000-000000000004";
    let projects = [
        format!(r#"{{"dirName":"-w-alpha","projectPath":"/w/alpha","sessionCount":4,"lastActivityMs":1000,"sessionIds":["{S1}","{S2}","{S3}","{S4}"]}}"#),
        r#"{"dirName":"-w-beta","projectPath":"","sessionCount":1,"lastActivityMs":2000}"#.to_string(),
        r#"{"dirName":"-w-gamma","projectPath":"C:\\w\\gamma","sessionCount":2,"lastActivityMs":500,"sessionIds":["x"]}"#.to_string(),
    ];
    let mut old: Vec<serde_json::Value> = projects
        .iter()
        .filter_map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).unwrap();
            history_project_from_row(&v, &metadata, Some("dev"), &NoLivenessOracleYet)
                .map(|(p, _)| serde_json::to_value(p).unwrap())
        })
        .collect();
    let mut new: Vec<serde_json::Value> =
        golden["remoteProjects"]["rows"].as_array().unwrap().clone();
    let key = |v: &serde_json::Value| v["projectDir"].as_str().unwrap().to_string();
    old.sort_by_key(key);
    new.sort_by_key(key);
    assert_eq!(old, new, "项目行：monitor 从前那份 join 与后端新那份不一致");

    let sessions = [
        format!(
            r#"{{"sessionId":"{S1}","jsonlPath":"/h/.claude/projects/-w-alpha/{S1}.jsonl","startedAtMs":10,"updatedAtMs":20,"messageCountApprox":7,"firstUserExcerpt":"占位","aiTitle":"占位标题","cwd":"/w/alpha","isBg":false,"forkedFromSessionId":"{S3}","forkedFromMessageUuid":"m-1"}}"#
        ),
        format!(
            r#"{{"sessionId":"{S4}","jsonlPath":"/h/.claude/projects/-w-alpha/{S4}.jsonl","startedAtMs":30,"updatedAtMs":40,"messageCountApprox":1,"firstUserExcerpt":"","aiTitle":null,"cwd":null,"isBg":true,"forkedFromSessionId":null,"forkedFromMessageUuid":null}}"#
        ),
    ];
    let old: Vec<serde_json::Value> = sessions
        .iter()
        .filter_map(|l| {
            remote_session_entry(l, "-w-alpha", &metadata, "dev", &NoLivenessOracleYet)
                .map(|e| serde_json::to_value(e).unwrap())
        })
        .collect();
    let new: Vec<serde_json::Value> = golden["remoteSessions"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            let mut v = v.clone();
            let o = v.as_object_mut().unwrap();
            o.remove("forkedFromSessionId");
            o.remove("forkedFromMessageUuid");
            v
        })
        .collect();
    assert_eq!(
        old, new,
        "会话行：monitor 从前那份 join 与后端新那份不一致（fork 两格之外）"
    );
}
