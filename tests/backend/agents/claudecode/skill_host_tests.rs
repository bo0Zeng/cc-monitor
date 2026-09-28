//! 设计/99 §2.1 ⑬（主会话 09-27 裁：`skill_host` 连同 `SKILLS` 表与 Claude 数据的纵深围栏落 `agents/claudecode/`）· F03b（收件箱编辑的三道围栏）。
use super::*;

fn fixture(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mig3a-skillhost-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join(".claude/planned-build/w1")).unwrap();
    std::fs::write(d.join(".claude/planned-build/w1/STATUS.md"), "s").unwrap();
    std::fs::write(d.join(".claude/planned-build/INBOX.txt"), "inbox\n").unwrap();
    d
}

/// 列出来的：每个声明各一行，实例按标记文件认，可编辑路径由声明算（不看盘上有没有别的文件）；
/// 在不在场只看探测文件：缺 ⇒ 缺席原因点名那个 skill 与它期待的那条路径，在 ⇒ `null`。
#[test]
fn views_come_from_the_spec_and_the_disk() {
    let d = fixture("views");
    std::fs::create_dir_all(d.join(".claude/planned-build/no-marker")).unwrap();
    let claude = d.join("claude-home");
    std::fs::create_dir_all(claude.join("skills/cc-bus")).unwrap();
    std::fs::write(claude.join("skills/cc-bus/SKILL.md"), "x").unwrap();
    let v = views_in(&claude, &d);
    let pb = claude
        .join("skills/planned-build/bin/pb.py")
        .display()
        .to_string();
    let why = v[0]["missing_reason"]
        .as_str()
        .expect("planned-build 没装 ⇒ 有缺席原因");
    assert!(why.contains("planned-build") && why.contains(&pb), "{why}");
    assert_eq!(v[1]["missing_reason"], serde_json::Value::Null);
    let ids: Vec<&str> = v.iter().map(|x| x["id"].as_str().unwrap()).collect();
    assert_eq!(ids, SKILLS.iter().map(|s| s.id).collect::<Vec<_>>());
    assert_eq!(v[0]["instances"], serde_json::json!(["w1"]));
    assert_eq!(
        v[0]["editable"],
        serde_json::json!([d.join(".claude/planned-build/INBOX.txt").to_string_lossy()])
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// 三道围栏：声明集合之外拒 · 文件必须已存在 · 解到底落在 Claude 会话记录上拒（纵深）；合法那一份回 `(项目根, 相对段)`。
#[test]
fn the_editable_fence_refuses_what_it_should() {
    let d = fixture("fence");
    let ok = editable_target(
        "planned-build",
        &d,
        &d.join(".claude/planned-build/INBOX.txt"),
    )
    .expect("合法那一份被拒了");
    assert_eq!(ok.1.replace('\\', "/"), ".claude/planned-build/INBOX.txt");
    std::fs::write(d.join("other.txt"), "x").unwrap();
    assert!(
        editable_target("planned-build", &d, &d.join("other.txt")).is_err(),
        "声明之外的文件放行了"
    );
    assert!(
        editable_target("planned-build", &d, &d.join("nope.txt")).is_err(),
        "不存在的文件放行了"
    );
    assert!(editable_target(
        "no-such-skill",
        &d,
        &d.join(".claude/planned-build/INBOX.txt")
    )
    .is_err());
    #[cfg(unix)]
    {
        // 纵深：可编辑那一份是一条指向会话记录的链接 ⇒ 解到底之后拒。
        let rec = d.join("projects/-p/0000.jsonl");
        std::fs::create_dir_all(rec.parent().unwrap()).unwrap();
        std::fs::write(&rec, "{}").unwrap();
        let inbox = d.join(".claude/planned-build/INBOX.txt");
        std::fs::remove_file(&inbox).unwrap();
        std::os::unix::fs::symlink(&rec, &inbox).unwrap();
        assert!(
            editable_target("planned-build", &d, &inbox).is_err(),
            "链到会话记录的那一份放行了"
        );
    }
    let _ = std::fs::remove_dir_all(&d);
}

/// 纵深那一道**不覆盖**哪几类路径 —— 读数（从 monitor 那一族的判据搬来）：每一行今天真的放行，谁动了射程当场点名。
#[test]
fn every_uncovered_shape_is_still_uncovered_today() {
    for p in [
        "/home/u/.claude/projects/-x-proj",
        "/home/u/.claude/projects/-x-proj/<sid>/subagents/agent-ab12.jsonl",
        "/home/u/.claude/tasks/<sid>/17.json",
        "/home/u/.claude/settings.json",
    ] {
        assert!(
            !super::super::paths::is_session_record_file(p),
            "{p} 今天被挡住了 —— 扩射程要用户拍"
        );
    }
    assert!(
        super::super::paths::is_session_record_file("/home/u/.claude/projects/-x-proj/0000.jsonl"),
        "正控：会话记录没被认出来"
    );
}
