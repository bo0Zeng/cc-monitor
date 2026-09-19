use super::*;

/// 本模块的夹具根：每个测试一个独立目录（同进程并发跑，不许互相看见）。
fn tmp_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("ccm-kr83-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    root
}

fn project_with(root: &Path, dir_name: &str, sids: &[&str]) -> PathBuf {
    let dir = root.join("projects").join(dir_name);
    std::fs::create_dir_all(&dir).unwrap();
    for sid in sids {
        std::fs::write(dir.join(format!("{sid}.jsonl")), b"{}\n").unwrap();
    }
    dir
}

/// 一个 JSON 对象里**所有**字符串叶子（含数组里的），**不看它们挂在哪个 key 下**。
///
/// ★ 这个取法就是 `KR83D1` 第 ③ 刀（「只改字段名、内容等价 ⇒ 必须绿」）的落点：
/// 判据要判的是「**这一行带不带得出那三个数**」，不是「有没有一个叫 `sessionIds` 的 key」。
/// 按 key 取 = 判名字；按叶子取 = 判**内容**。
fn string_leaves(v: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(v: &serde_json::Value, out: &mut Vec<String>) {
        match v {
            serde_json::Value::String(s) => out.push(s.clone()),
            serde_json::Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            serde_json::Value::Object(m) => m.values().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    walk(v, &mut out);
    out
}

/// ★★ `KR83D1`：**`--list-projects` 的每一行，下游算得出 `starredCount` /
/// `hiddenCount` / `hasLive`。**
///
/// # 判的是什么
///
/// 那三个数的真相源全在 monitor 那侧、且**全部按会话 sid 索引**
///（本机 metadata 按 sid 查 star/hide；`SessionMap` 按 sid 查活）——
/// 所以「这一行够不够用」这个性质，逐字等于「**这一行说不说得出这个项目下有哪几个 sid**」。
///
/// # ⚠ 它**不判字段叫什么名字**
///
/// `local_read_surface_registry.rs` 那条退役条件逐字留了「**或等价字段**」这个口子。
/// 判据按 [`string_leaves`] 取值（不看 key），所以把 `sessionIds` 改名、
/// 或改成 `[{"sid": …}]` 这种等价形状，本条**照常绿**；
/// 只有**内容真的没了**才红。
#[test]
fn every_project_row_carries_the_session_ids_the_three_numbers_are_indexed_by() {
    let root = tmp_root("d1");
    let dir = project_with(&root, "-home-u-proj", &["sid-aaa", "sid-bbb", "sid-ccc"]);
    let row = project_row(&dir, "-home-u-proj".into()).expect("有会话的项目必须出一行");

    let leaves = string_leaves(&row);
    for sid in ["sid-aaa", "sid-bbb", "sid-ccc"] {
        assert!(
            leaves.iter().any(|s| s == sid),
            "★ 这一行里找不到 sid `{sid}` —— 下游就**算不出** starredCount / hiddenCount / hasLive，\n\
                 只能退回「每个项目再来一次 --list-sessions」（N 次进程 spawn，见 KR83D3）。\n\
                 ⚠ 本条按**值**找、不按 key 找：改字段名不会让它红，内容没了才会。\n\
                 这一行现打：{row}"
        );
    }

    // 第 ② 刀的正向那一半：清单**不许**是空的，而项目下确实有会话。
    // 「空清单」与「这个项目下没有会话」在下游是两件事 —— 后者根本不会有这一行
    //（`project_row` 返回 `None`），所以出了行还空 = 这一行坏了。
    assert_eq!(
        row["sessionCount"].as_u64(),
        Some(3),
        "sessionCount 与夹具对不上，下面那条对拍就没有意义了：{row}"
    );
    let ids_len = leaves.iter().filter(|s| s.starts_with("sid-")).count();
    assert_eq!(
        ids_len, 3,
        "★ 带出来的 sid 只有 {ids_len} 个而这个项目下有 3 个会话 —— \n\
             **空清单 / 短清单 ≠ 没有**。下游拿它当「真的是 0」就是本件要治的那个病。\n\
             这一行现打：{row}"
    );
}

/// ★ **`sessionCount` 与 sid 清单恒等长** —— 这是契约里下游用来分辨
/// 「真的没有」与「这一行坏了」的那把尺子（monitor 侧 `remote_history.rs` 逐字对拍它）。
///
/// 两侧由**同一个守卫**产出（见 `project_row` 里那段注释），所以这条钉的是
/// 「别把它们拆成两个守卫」。
///
/// ⚠ 同 [`every_project_row_carries_the_session_ids_the_three_numbers_are_indexed_by`]：
/// 数的是**带前缀的那几个值**，不按 key 取 —— 本模块**没有一条判据碰那个字段名**，
/// 这就是 `KR83D1` 第 ③ 刀（改名必须绿）在后端这一侧的落法。
#[test]
fn the_session_id_list_and_the_count_are_produced_by_the_same_guard() {
    let root = tmp_root("d1b");
    let dir = project_with(&root, "p", &["kr83-a", "kr83-b", "kr83-c", "kr83-d"]);
    // 非会话文件（sidecar / 目录）既不进计数、也不进清单。
    std::fs::write(dir.join("notes.txt"), b"x").unwrap();
    std::fs::create_dir_all(dir.join("subagents")).unwrap();

    let row = project_row(&dir, "p".into()).expect("有会话的项目必须出一行");
    let ids = string_leaves(&row)
        .into_iter()
        .filter(|s| s.starts_with("kr83-"))
        .count();
    assert_eq!(
        row["sessionCount"].as_u64(),
        Some(ids as u64),
        "★ 计数与清单长度错开了 —— 下游的「空清单是坏行还是真没有」这条判断就瞎了：{row}"
    );
    assert_eq!(ids, 4, "sidecar / 子目录不该被算进来：{row}");
}

/// 空目录（只剩 sidecar）**不出行** —— 「没有这一行」与「有这一行但清单空」
/// 必须是两件事，否则下游没法把第二种当成坏行。
#[test]
fn a_project_with_no_sessions_has_no_row_at_all() {
    let root = tmp_root("d1c");
    let dir = root.join("projects").join("empty");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("README.md"), b"x").unwrap();
    assert!(
        project_row(&dir, "empty".into()).is_none(),
        "★ 没有会话的项目出了一行 —— 那一行的空清单会与「坏行」同形"
    );
}

/// sid 的取法与 `--list-sessions` 那条**必须是同一个** —— 两条路给同一个会话
/// 两个不同的 id，下游按 sid 查 metadata 就会**查不着**，
/// 而查不着在今天的界面上长得和「没有星标」一模一样（又一次「不知道」装成 0）。
#[test]
fn the_two_query_paths_spell_a_session_id_the_same_way() {
    let root = tmp_root("d1d");
    let dir = project_with(&root, "p", &["019f75dd-875c-7c81-9eda-32f866b2c60f"]);
    let row = project_row(&dir, "p".into()).expect("row");
    let from_list_sessions =
        analyze_session(&dir.join("019f75dd-875c-7c81-9eda-32f866b2c60f.jsonl"))["sessionId"]
            .as_str()
            .unwrap()
            .to_string();
    // 按**值**找，不按 key 找（`KR83D1` 第 ③ 刀）：改字段名不许让这条红。
    assert!(
        string_leaves(&row).contains(&from_list_sessions),
        "★ `--list-sessions` 把这个会话叫 `{from_list_sessions}`，\n\
             而 `--list-projects` 那一行里找不到这个字符串 —— 下游按 sid 对不上号，\n\
             查不着在界面上长得和「没有星标」一模一样（又一次「不知道」装成 0）。\n\
             这一行现打：{row}"
    );
}
