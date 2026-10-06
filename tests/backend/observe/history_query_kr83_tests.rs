use super::*;

/// 本模块的夹具根：每个测试一个独立目录（同进程并发跑，不许互相看见）。
fn tmp_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("ccm-kr83-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    root
}

/// 空目录（只剩本机后端）**不出行**：没有会话的项目不出一行 `sessionCount: 0`。
#[test]
fn a_project_with_no_sessions_has_no_row_at_all() {
    let root = tmp_root("d1c");
    let dir = root.join("projects").join("empty");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("README.md"), b"x").unwrap();
    assert!(
        project_rows(&dir, "empty").into_iter().next().is_none(),
        "没有会话的项目出了一行"
    );
}

/// 两个真实目录撞成同一个记录目录名（`/home/u/文档/x` 与 `/home/u/桌面/x` 都记在 `-home-u----x`）⇒ 两行，各数各的会话；
/// 读不出目录的会话归最近修改的那一组。此前按记录目录名并成一个项目，项目路径取最新那份的目录。
#[test]
fn two_real_directories_behind_one_record_folder_name_are_two_projects() {
    let root = tmp_root("h09");
    let dir = root.join("projects").join("-home-u----x");
    std::fs::create_dir_all(&dir).unwrap();
    let put = |sid: &str, line: &str, secs: u64| {
        let p = dir.join(format!("{sid}.jsonl"));
        std::fs::write(&p, format!("{line}\n")).unwrap();
        let f = std::fs::File::options().write(true).open(&p).unwrap();
        f.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs))
            .unwrap();
    };
    put("doc-1", r#"{"cwd":"/home/u/文档/x"}"#, 1_000);
    put("doc-2", r#"{"cwd":"/home/u/文档/x"}"#, 3_000);
    put("desk-1", r#"{"cwd":"/home/u/桌面/x"}"#, 2_000);
    put("nocwd", "{}", 500);
    let rows = project_rows(&dir, "-home-u----x");
    let got: Vec<(&str, u64, i64)> = rows
        .iter()
        .map(|r| {
            (
                r["projectPath"].as_str().unwrap(),
                r["sessionCount"].as_u64().unwrap(),
                r["lastActivityMs"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![
            ("/home/u/文档/x", 3, 3_000_000),
            ("/home/u/桌面/x", 1, 2_000_000)
        ],
        "两个目录被并成了一个项目"
    );
    assert!(rows.iter().all(|r| r["dirName"] == "-home-u----x"));
    let _ = std::fs::remove_dir_all(&root);
}

/// 分组那一个函数：读得出目录的按目录；读不出的归最近修改的那个读得出目录的会话；一个都读不出 ⇒ 空串一组。
#[test]
fn sessions_without_a_directory_join_the_most_recent_group() {
    let g = |items: &[(Option<&str>, i64)]| {
        let v: Vec<(Option<String>, i64)> = items
            .iter()
            .map(|(c, m)| (c.map(str::to_string), *m))
            .collect();
        group_by_cwd(&v)
    };
    assert_eq!(
        g(&[(Some("/a"), 1), (None, 9), (Some("/b"), 5)]),
        vec!["/a", "/b", "/b"]
    );
    assert_eq!(g(&[(None, 1), (None, 2)]), vec!["", ""]);
}

/// 工作目录是这台家里 `autostart/` 的会话不进项目清单；同一个记录目录里别的目录照常出。
#[test]
fn sessions_in_the_hidden_dir_are_left_out_of_the_project_list() {
    let root = tmp_root("autostart");
    let home = root.join("data");
    let auto = home.join("autostart");
    std::fs::create_dir_all(&auto).unwrap();
    let dir = root.join("projects").join("-x");
    std::fs::create_dir_all(&dir).unwrap();
    let put = |sid: &str, cwd: &Path| {
        std::fs::write(
            dir.join(format!("{sid}.jsonl")),
            format!("{}\n", serde_json::json!({ "cwd": cwd.to_string_lossy() })),
        )
        .unwrap();
    };
    put("mine", Path::new("/home/u/proj"));
    put("auto", &auto);
    let rows = project_rows_hiding(&dir, "-x", &|c| hidden_cwd_in(&home, c));
    let paths: Vec<&str> = rows
        .iter()
        .map(|r| r["projectPath"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["/home/u/proj"]);
    // 记录里的目录是解析过链接的那一形：家经一条链接到达时也认得出。
    #[cfg(unix)]
    {
        let link = root.join("link-data");
        std::os::unix::fs::symlink(&home, &link).unwrap();
        assert!(hidden_cwd_in(&link, &auto.to_string_lossy()));
    }
    assert!(!hidden_cwd_in(&home, "/home/u/proj"));
    let _ = std::fs::remove_dir_all(&root);
}
