/// ★ P7c1-Y2：**daemon 不许自己匹配或排序 subagent**。
///
/// 最容易的写法是把 monitor 的 `load_subagent` 整套搬过来。那会长出第二套语义
/// —— 定框 `C1` 逐字排除，而本轮已经在那个 `or` 上数出**四份**实现。
/// ⇒ 这里只列候选；`description` 精确匹配与按时间戳挑最近**留在 monitor**。
#[test]
fn the_daemon_never_matches_or_ranks_subagents() {
    let prod = crate::guard_support::production_code(include_str!("../../../src/backend/observe/history_query.rs"));
    let at = prod
        .find("pub fn list_subagents(")
        .expect("找不到 list_subagents —— 判据在空转");
    let body: String = prod[at..].chars().take(2600).collect();
    for banned in [
        "description ==",
        "sort_by",
        "sort_by_key",
        "parse_iso8601",
        ".abs()",
    ] {
        assert!(
            !body.contains(banned),
            "`list_subagents` 里出现了 {banned:?} —— 它开始自己**挑**了。\n\
                 挑选逻辑只准有一份，住 monitor 的 `pick_closest`（`C1`：别长第二套语义）。"
        );
    }
    // 反面：它必须真的**用了那条既有围栏**，而不是自己写一套路径检查。
    assert!(
        body.contains("fence_under_projects("),
        "`list_subagents` 没走 `fence_under_projects` —— \n\
             那是全文件**唯一**的 canonicalize + 前缀校验（audit-0805 定框 E3），别再造一份。"
    );
}

/// ★ P7c1-Y1：越界路径必须被拒。
#[test]
fn listing_subagents_refuses_paths_outside_projects() {
    let tmp = std::env::temp_dir().join(format!("p7c1-{}", std::process::id()));
    let projects = tmp.join("projects");
    std::fs::create_dir_all(&projects).expect("建目录");
    let outside = tmp.join("outside.jsonl");
    std::fs::write(&outside, "{}\n").expect("写文件");
    let code = super::list_subagents(
        &tmp,
        &[
            "--list-subagents".to_string(),
            outside.to_string_lossy().into_owned(),
        ],
    );
    assert_eq!(code, 2, "projects/ 之外的路径必须被围栏拒绝");
    let _ = std::fs::remove_dir_all(&tmp);
}

/// ★ **历史查询那几条读路不许整读 jsonl**〔audit-0805 F07 / 报告 I-10 与 B-6 第 5 环〕。
///
/// 两处此前都是 `read_to_string`：
/// - `extract_cwd_from_head` —— 下一行就 `.take(40)`，却先把 257 MB 整份读进来；
///   `--list-projects` 对**每个项目**调它一次。
/// - `analyze_session` —— `--list-sessions` 对该项目**每个** jsonl 调它一次；
///   43 个项目 / 2.4 GB 的机器上，一次列表就是把 2.4 GB 读进内存再逐行解析。
///
/// 扫描本身是必须的（要数行、要判 bg），**但不必先整份进内存**。
/// ⚠ 「慢/费内存」**不会让任何测试变红** ⇒ 只能靠源码形态钉（同 F04 那条）。
#[test]
fn the_history_readers_stream_instead_of_slurping() {
    let src = guard_core::production_code(include_str!("../../../src/backend/observe/history_query.rs"));
    for (name, sig) in [
        ("extract_cwd_from_head", "fn extract_cwd_from_head("),
        ("analyze_session", "fn analyze_session("),
    ] {
        let begin = src
            .find(sig)
            .unwrap_or_else(|| panic!("找不到 {name} —— 抽取器坏了，本条会零命中地绿"));
        let end = src[begin..]
            .find("\n}\n")
            .unwrap_or_else(|| panic!("找不到 {name} 的结尾 —— 抽取器坏了"));
        let body = &src[begin..begin + end];
        assert!(
            body.contains("BufReader"),
            "{name} 里没有 `BufReader` —— 要么切错范围（本条会零命中地绿），要么它被改回整读了"
        );
        assert!(
            !body.contains("read_to_string("),
            "{name} 又在整读 jsonl 了。\n\
                 `--list-projects` / `--list-sessions` 会对**每个**项目/会话文件调它一次，\n\
                 而 43 个项目 / 2.4 GB 的机器上那就是一次列表读 2.4 GB。\n\
                 慢不会让任何测试变红 —— 所以这条只能靠源码形态钉。"
        );
    }
}
