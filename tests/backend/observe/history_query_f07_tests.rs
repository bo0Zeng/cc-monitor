/// ★ P7c1-Y2：**列候选的那一段不许自己匹配或排序 subagent**。
///
/// 〔MOD〕挑（`description` 精确匹配 ＋ 按时间戳挑最近）随「找」一起进了后端，住 `pick_subagent` 一处；
/// 列候选（`list_subagents_into`，CLI 与帧面共用）仍只列 —— 两件事各一个家，挑的规则不许在列的那一段里再长一份（`C1`）。
#[test]
fn the_backend_never_matches_or_ranks_subagents() {
    let prod = crate::guard_support::production_code(include_str!(
        "../../../src/backend/observe/history_query.rs"
    ));
    // 〔`C1` · 09-24〕本体搬进了 `list_subagents_into`（帧面与 CLI 共用，出口是参数）；
    // `list_subagents` 只剩 CLI 那层壳 ⇒ 锚跟着本体走。
    let at = prod
        .find("fn list_subagents_into(")
        .expect("找不到 list_subagents_into —— 判据在空转");
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
                 挑选逻辑只准有一份，住 `pick_subagent`（`C1`：别长第二套语义）。"
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
    let src = guard_core::production_code(include_str!(
        "../../../src/backend/observe/history_query.rs"
    ));
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

// ── 〔MOD〕挑 subagent 那一份随「找」一起进了后端（原 monitor `subagent_tests.rs` 的三条纯判据原样搬来）──

fn pick(listing: &[String], description: &str, at: &str) -> Option<PathBuf> {
    let rows: Vec<&str> = listing.iter().map(String::as_str).collect();
    super::pick_subagent(&rows, description, at)
}

use std::path::PathBuf;

/// 造一行后端 `--list-subagents` 的输出。
fn listed(path: &str, description: &str, timestamp: Option<&str>) -> String {
    let ts = match timestamp {
        Some(t) => serde_json::Value::String(t.to_string()),
        None => serde_json::Value::Null,
    };
    let v = serde_json::json!({
        "path": path,
        "description": description,
        "timestamp": ts,
    });
    v.to_string()
}

/// ★★ `KR94D1` 第 ①② 刀：**后端给的候选变了，结果就得跟着变。**
///
/// 五组读数摆在一起才说得清「跟」是什么意思：换一份候选 ⇒ 换一个答案；
/// 一条都不给 ⇒ **没得挑**（不许从别处变出一个来）；描述不匹配 ⇒ 同样没得挑。
#[test]
fn changing_what_the_backend_lists_changes_what_gets_picked() {
    let at = "2026-09-12T10:30:00.000Z";

    // ① 后端给 a / b，a 更近 ⇒ 挑 a
    let l1 = vec![
        listed("/r/agent-a.jsonl", "找它", Some("2026-09-12T10:29:00.000Z")),
        listed("/r/agent-b.jsonl", "找它", Some("2026-09-12T12:00:00.000Z")),
    ];
    assert_eq!(
        pick(&l1, "找它", at),
        Some(PathBuf::from("/r/agent-a.jsonl"))
    );

    // ② 后端换了一份候选（a/b 都不在了）⇒ 结果必须跟着换成 c
    let l2 = vec![listed(
        "/r/agent-c.jsonl",
        "找它",
        Some("2026-09-12T10:31:00.000Z"),
    )];
    assert_eq!(
        pick(&l2, "找它", at),
        Some(PathBuf::from("/r/agent-c.jsonl")),
        "后端换了候选而结果没跟 —— 那候选就不是后端决定的"
    );

    // ③ 后端一条都不给 ⇒ 没得挑
    assert_eq!(
        pick(&[], "找它", at),
        None,
        "后端零候选却挑出了东西 —— 那个东西只可能来自别处"
    );

    // ④ description 不匹配的不许混进来（筛选留在本侧，backend 不挑）
    let l3 = vec![listed(
        "/r/agent-d.jsonl",
        "别的",
        Some("2026-09-12T10:30:00.000Z"),
    )];
    assert_eq!(pick(&l3, "找它", at), None);

    // ⑤ 后端给的行不是 JSON / 少字段 ⇒ 跳过那一行，不整条崩
    let l4 = vec![
        "这不是 json".to_string(),
        r#"{"description":"找它"}"#.to_string(),
        listed("/r/agent-e.jsonl", "找它", Some(at)),
    ];
    assert_eq!(
        pick(&l4, "找它", at),
        Some(PathBuf::from("/r/agent-e.jsonl"))
    );
}

/// ★★ `KR94D3` 第 ① 刀：**候选顺序不同，仍挑出同一个**（`pick_closest` 不该看顺序）。
///
/// 「挑出同一个」是**全称**命题 ⇒ 3 个候选的 **6 种排列逐个跑**，不挑一个样本了事。
#[test]
fn the_order_the_backend_lists_them_in_does_not_change_the_pick() {
    let at = "2026-09-12T10:30:00.000Z";
    let all = [
        listed("/r/agent-a.jsonl", "找它", Some("2026-09-12T10:29:00.000Z")),
        listed("/r/agent-b.jsonl", "找它", Some("2026-09-12T10:40:00.000Z")),
        listed("/r/agent-c.jsonl", "找它", Some("2026-09-12T09:00:00.000Z")),
    ];
    let perms = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    for (i, p) in perms.iter().enumerate() {
        let listing: Vec<String> = p.iter().map(|&k| all[k].clone()).collect();
        assert_eq!(
            pick(&listing, "找它", at),
            Some(PathBuf::from("/r/agent-a.jsonl")),
            "第 {i} 种排列挑出了别的 —— `pick_closest` 看了顺序"
        );
    }
}

/// ★★ `KR94D3` 第 ② 刀：**时间戳缺失那一档，两条路的处置必须一致。**
///
/// 「两条路」在 `K-R94` 之后是同一段代码 ⇒ 真正要钉的是**缺失的两种形状**落到同一档：
/// 后端拿不到时给 `"timestamp": null`（它**不猜**），而本机那条改前是
/// 「首行读不出时间戳」—— 在后端出口上表现为**根本没有这个键**。
/// 两种形状必须同处置，而且**都不报错**（不许一条报错、一条静默取第一个）。
#[test]
fn both_shapes_of_a_missing_timestamp_land_in_the_same_tier() {
    let at = "2026-09-12T10:30:00.000Z";
    let null_shape = r#"{"path":"/r/agent-x.jsonl","description":"找它","timestamp":null}"#;
    let absent_shape = r#"{"path":"/r/agent-x.jsonl","description":"找它"}"#;

    // ① 单条：两种形状同结果，而且都是 `Some`（**不报错**、不静默丢掉）。
    let via_null = pick(&[null_shape.to_string()], "找它", at);
    let via_absent = pick(&[absent_shape.to_string()], "找它", at);
    assert_eq!(via_null, via_absent, "`null` 与「缺键」被分到了两档");
    assert_eq!(via_null, Some(PathBuf::from("/r/agent-x.jsonl")));

    // ② 多条全缺：处置是「保持后端给的次序、取第一条」——两种形状必须给同一个答案。
    let all_null = vec![
        listed("/r/agent-1.jsonl", "找它", None),
        listed("/r/agent-2.jsonl", "找它", None),
    ];
    let all_absent = vec![
        r#"{"path":"/r/agent-1.jsonl","description":"找它"}"#.to_string(),
        r#"{"path":"/r/agent-2.jsonl","description":"找它"}"#.to_string(),
    ];
    assert_eq!(pick(&all_null, "找它", at), pick(&all_absent, "找它", at));
    assert_eq!(
        pick(&all_null, "找它", at),
        Some(PathBuf::from("/r/agent-1.jsonl")),
        "全缺时间戳时的处置变了 —— 它是「取后端给的第一条」，不是报错、也不是随机"
    );

    // ③ 部分缺：**有时间戳的一定赢**，而且与缺失是哪种形状无关。
    let shapes = [
        listed("/r/agent-nots.jsonl", "找它", None),
        r#"{"path":"/r/agent-nots.jsonl","description":"找它"}"#.to_string(),
    ];
    for missing in shapes {
        let has = listed(
            "/r/agent-has.jsonl",
            "找它",
            Some("2026-09-12T10:31:00.000Z"),
        );
        let mixed = vec![missing, has];
        assert_eq!(
            pick(&mixed, "找它", at),
            Some(PathBuf::from("/r/agent-has.jsonl")),
            "缺时间戳的那条排到了有时间戳的前面"
        );
    }

    // ④ `tool_use_timestamp` 自己解析不出来：同样落这一档，**不报错**。
    let l = vec![
        listed("/r/agent-1.jsonl", "找它", Some("2026-09-12T10:00:00.000Z")),
        listed("/r/agent-2.jsonl", "找它", Some("2026-09-12T11:00:00.000Z")),
    ];
    assert_eq!(
        pick(&l, "找它", "根本不是时间戳"),
        Some(PathBuf::from("/r/agent-1.jsonl"))
    );
}
