/// 读一个子运行的记录之前，父记录照样过读会话那道围栏：`projects/` 之外的路径一律拒。
#[test]
fn reading_a_run_refuses_parents_outside_projects() {
    let tmp = std::env::temp_dir().join(format!("runs-fence-{}", std::process::id()));
    let projects = tmp.join("projects");
    std::fs::create_dir_all(&projects).expect("建目录");
    let outside = tmp.join("outside.jsonl");
    std::fs::write(&outside, "{}\n").expect("写文件");
    let got = super::run_source(&tmp, &outside.to_string_lossy(), Some("r1"), None);
    assert_eq!(
        got.map_err(|(c, _)| c),
        Err("path_refused"),
        "projects/ 之外的父记录必须被围栏拒绝"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

/// ★ 按工具调用 id 读一个前台派出、还在跑的子运行（父侧还没有带运行号的结果）：子运行记录旁边派出那一刻落了盘的
/// 那一份对上它；那一份不在 ⇒ 照旧「还对不上」。夹具只造结构。
#[test]
fn reading_a_run_by_its_call_finds_a_foreground_run_still_running() {
    let tmp = std::env::temp_dir().join(format!("runs-fg-{}", std::process::id()));
    let proj = tmp.join("projects").join("p");
    let parent = proj.join("s1.jsonl");
    let kids = proj.join("s1").join("subagents");
    std::fs::create_dir_all(&kids).expect("建目录");
    std::fs::write(
        &parent,
        r#"{"type":"assistant","uuid":"a1","message":{"id":"m1","role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Agent","input":{"description":"d","prompt":"p"}}]}}"#.to_string() + "\n",
    )
    .expect("写父记录");
    let child = kids.join("agent-f1.jsonl");
    std::fs::write(
        &child,
        r#"{"type":"user","uuid":"c1","isSidechain":true,"agentId":"f1","message":{"role":"user","content":"p"}}"#.to_string() + "\n",
    )
    .expect("写子记录");
    let p = parent.to_string_lossy().into_owned();
    let got = super::run_source(&tmp, &p, None, Some("t1"));
    assert_eq!(
        got.map_err(|(c, _)| c),
        Err("not_found"),
        "那一份没落盘 ⇒ 还对不上"
    );
    std::fs::write(
        kids.join("agent-f1.meta.json"),
        r#"{"agentType":"k","toolUseId":"t1"}"#,
    )
    .expect("写那一份");
    let got = super::run_source(&tmp, &p, None, Some("t1"));
    assert_eq!(
        got.map_err(|(c, _)| c),
        Ok((child, "f1".to_string())),
        "派出那一刻落盘的那一份对上了"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}
