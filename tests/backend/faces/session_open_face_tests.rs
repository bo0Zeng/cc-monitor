//! 恢复读记录那一处：终端没了时换的模型 / 权限档（注解 `resumeWith`）在记录没长时盖过记录里的，长了就作废。

use super::*;
use crate::history::history_annotations::kept_for_resume;

fn of_record() -> ResumeOf {
    ResumeOf {
        agent: "claude".into(),
        cwd: "/w".into(),
        model: Some("claude-opus-4-5-20251101".into()),
        permission_mode: Some("acceptEdits".into()),
    }
}

fn pick(model: Option<&str>, mode: Option<&str>, end: u64) -> ResumeWith {
    ResumeWith {
        model: model.map(str::to_string),
        permission_mode: mode.map(str::to_string),
        end,
    }
}

/// ★ 记下 `opus` 且记录没长 ⇒ 恢复带 `opus`；记录追加一行 ⇒ 那一份作废、照记录；记下「默认」⇒ 不带模型；
/// 只记了权限档 ⇒ 模型照记录。
#[test]
fn a_kept_pick_wins_until_the_record_grows() {
    let d = std::env::temp_dir().join(format!("ccm-open-kept-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let rec = d.join("s.jsonl");
    std::fs::write(&rec, "{\"i\":0}\n").unwrap();
    let len = std::fs::metadata(&rec).unwrap().len();

    let kept = kept_for_resume(Some(pick(Some("opus"), Some("plan"), len)), &rec);
    let r = with_kept(of_record(), kept);
    assert_eq!(r.model.as_deref(), Some("opus"));
    assert_eq!(r.permission_mode.as_deref(), Some("plan"));

    let r = with_kept(
        of_record(),
        kept_for_resume(Some(pick(Some("default"), None, len)), &rec),
    );
    assert_eq!(r.model, None, "默认那一项不带模型");
    assert_eq!(r.permission_mode.as_deref(), Some("acceptEdits"));

    let r = with_kept(
        of_record(),
        kept_for_resume(Some(pick(None, Some("plan"), len)), &rec),
    );
    assert_eq!(r.model.as_deref(), Some("claude-opus-4-5-20251101"));

    std::fs::write(&rec, "{\"i\":0}\n{\"i\":1}\n").unwrap();
    let grown = kept_for_resume(Some(pick(Some("opus"), Some("plan"), len)), &rec);
    assert_eq!(grown, None);
    let r = with_kept(of_record(), grown);
    assert_eq!(r.model.as_deref(), Some("claude-opus-4-5-20251101"));
    assert_eq!(r.permission_mode.as_deref(), Some("acceptEdits"));
    let _ = std::fs::remove_dir_all(&d);
}
