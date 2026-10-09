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

/// ★ **历史查询那几条读路不许整读 jsonl**〔audit-0805 F07 / 报告 I-10 与 B-6 第 5 环〕。
///
/// `analyze_session` 此前是 `read_to_string`：`--list-sessions` 对该项目**每个** jsonl 调它一次；
///   43 个项目 / 2.4 GB 的机器上，一次列表就是把 2.4 GB 读进内存再逐行解析。
///
/// 扫描本身是必须的（要数行、要判 bg），**但不必先整份进内存**。
/// ⚠ 「慢/费内存」**不会让任何测试变红** ⇒ 只能靠源码形态钉（同 F04 那条）。
#[test]
fn the_history_readers_stream_instead_of_slurping() {
    let src = guard_core::production_code(include_str!(
        "../../../src/backend/observe/history_query.rs"
    ));
    // 项目目录那一格只读记录开头（适配层，`agents::first_in_head`），上界由那边的行为判据钉。
    // 〔perfC〕逐行扫那一段住清单缓存那一格（`ListingEntry::scan`，变长只扫尾巴也在那里）；`analyze_session` 只转调它。
    let begin = src
        .find("fn analyze_session(")
        .expect("找不到 analyze_session —— 抽取器坏了");
    let end = src[begin..]
        .find("\n}\n")
        .expect("找不到 analyze_session 的结尾");
    assert!(
        src[begin..begin + end].contains("ListingEntry::scan("),
        "analyze_session 不再转调 `ListingEntry::scan` —— 下面那条钉的就不是它读文件的那一段了"
    );
    for (name, sig) in [("ListingEntry", "impl ListingEntry {")] {
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
