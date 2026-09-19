use super::*;
use std::path::PathBuf;

struct TmpDir(PathBuf);
impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn tmpdir(tag: &str) -> TmpDir {
    let p = std::env::temp_dir().join(format!(
        "p8a-{tag}-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    TmpDir(p)
}

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

/// `P8a-Y1`：**没有**这条出口 —— 文件不存在是诚实的空，不是错。
#[test]
fn an_absent_file_is_an_honest_empty_not_an_error() {
    let t = tmpdir("absent");
    let s = survey_marketplaces_in(&t.0).expect("文件不存在不该是错");
    assert!(s.file_absent, "没有那份文件时必须把 file_absent 立起来");
    assert!(s.entries.is_empty());
}

/// `P8a-Y1`：**读不到**这条出口 —— 坏 json 必须回 `Err`，**不是空表**。
///
/// 空表与上一条测的「一个都没有」在界面上长得一模一样，
/// 那正是 `U3` 记的「那个 0 是瞎的」同一种病。
#[test]
fn a_broken_file_is_an_error_not_an_empty_list() {
    let t = tmpdir("broken");
    write(
        &t.0.join("plugins").join("known_marketplaces.json"),
        "{ 这不是 json",
    );
    let err = survey_marketplaces_in(&t.0).expect_err("解析失败必须回 Err");
    assert!(err.contains("解析失败"), "错误里要说清是解析失败：{err}");
}

/// `P8a-Y1`：顶层不是对象 —— 同样是 `Err`，不是空表。
#[test]
fn a_wrongly_shaped_file_is_an_error_too() {
    let t = tmpdir("shape");
    write(&t.0.join("plugins").join("known_marketplaces.json"), "[]");
    let err = survey_marketplaces_in(&t.0).expect_err("形状不对必须回 Err");
    assert!(err.contains("顶层不是一个对象"), "{err}");
}

/// `P8a-Y2`：数得出来时给的是**声明数**（manifest 的 `plugins[]` 长度），
/// 而**不是**快照目录里的目录数 —— 本机那两个数是 276 与 39，差得很远。
#[test]
fn the_count_is_what_the_manifest_declares_not_what_the_snapshot_contains() {
    let t = tmpdir("declared");
    let loc = t.0.join("mk");
    // manifest 声明 3 个……
    write(
        &loc.join(".claude-plugin").join("marketplace.json"),
        r#"{"plugins":[{"name":"a"},{"name":"b"},{"name":"c"}]}"#,
    );
    // ……而快照目录里只躺着 1 个。数错了这条会当场分出来。
    std::fs::create_dir_all(loc.join("plugins").join("only-one")).unwrap();
    write(
        &t.0.join("plugins").join("known_marketplaces.json"),
        &format!(
            r#"{{"mk":{{"source":{{"source":"github","repo":"a/b"}},"installLocation":{:?},"lastUpdated":"2026-08-02T00:00:00Z"}}}}"#,
            loc.to_string_lossy()
        ),
    );
    let s = survey_marketplaces_in(&t.0).unwrap();
    assert!(!s.file_absent);
    assert_eq!(s.entries.len(), 1);
    let e = &s.entries[0];
    assert_eq!(e.declared_plugins, Some(3), "要数 manifest 声明的那 3 个");
    assert_eq!(e.declared_error, None);
    assert_eq!(e.source.as_deref(), Some("github:a/b"));
    assert_eq!(e.last_updated.as_deref(), Some("2026-08-02T00:00:00Z"));
}

/// `P8a-Y2`：数不出来时是 `None` **且带理由**，**不是 `Some(0)`**。
#[test]
fn an_unreadable_count_is_null_with_a_reason_never_zero() {
    let t = tmpdir("nocount");
    write(
        &t.0.join("plugins").join("known_marketplaces.json"),
        r#"{"mk":{"installLocation":"/nonexistent/p8a"}}"#,
    );
    let s = survey_marketplaces_in(&t.0).unwrap();
    let e = &s.entries[0];
    assert_eq!(
        e.declared_plugins, None,
        "读不到必须是 null —— `Some(0)` 会被渲染成「声明 0 个」，那是假话"
    );
    let why = e.declared_error.as_deref().unwrap_or("");
    assert!(
        why.contains("没有"),
        "null 必须带着理由一起出去，否则等于没说：{why:?}"
    );
}

/// `P8a-Y2`：一条坏的**不许**毁掉整张表（降级只降那一行）。
#[test]
fn one_bad_row_does_not_kill_the_whole_table() {
    let t = tmpdir("mixed");
    let good = t.0.join("good");
    write(
        &good.join(".claude-plugin").join("marketplace.json"),
        r#"{"plugins":[{"name":"a"}]}"#,
    );
    write(
        &t.0.join("plugins").join("known_marketplaces.json"),
        &format!(
            r#"{{"a-good":{{"installLocation":{:?}}},"b-bad":{{"installLocation":"/nonexistent/p8a"}}}}"#,
            good.to_string_lossy()
        ),
    );
    let s = survey_marketplaces_in(&t.0).unwrap();
    assert_eq!(s.entries.len(), 2, "坏的那条也要在表里，不能被吞掉");
    assert_eq!(s.entries[0].declared_plugins, Some(1));
    assert_eq!(s.entries[1].declared_plugins, None);
    assert!(s.entries[1].declared_error.is_some());
}

/// `P8a-Y2`：manifest 超上限 ⇒ 那一行降级**并说清是超限**，不是静默、也不是 0。
#[test]
fn an_oversized_manifest_says_so_instead_of_counting() {
    let t = tmpdir("cap");
    let loc = t.0.join("mk");
    let manifest = loc.join(".claude-plugin").join("marketplace.json");
    write(&manifest, "x");
    // 直接把上限拿来验（不造一个 32 MB 的文件）：读上限函数是同一个。
    let err = read_capped(&manifest, 0).expect_err("1 字节 > 0 字节上限");
    assert!(err.contains("超过上限"), "{err}");
}

/// `P8a-Y2`：`installLocation` 缺失时也要说清「无从去数」，不是默默 0。
#[test]
fn a_missing_install_location_is_explained() {
    let t = tmpdir("noloc");
    write(
        &t.0.join("plugins").join("known_marketplaces.json"),
        r#"{"mk":{"source":{"source":"github","repo":"a/b"}}}"#,
    );
    let s = survey_marketplaces_in(&t.0).unwrap();
    assert_eq!(s.entries[0].declared_plugins, None);
    assert!(s.entries[0]
        .declared_error
        .as_deref()
        .unwrap_or("")
        .contains("installLocation"));
}

/// ★ 本模块**不许**去数快照目录 —— 那 39 个不是用户安装的。
/// 判据钉的是「生产段没有人 `read_dir` 那个 `plugins/` 目录」。
#[test]
fn the_snapshot_directory_is_never_counted() {
    let src = guard_core::production_source(include_str!("../../src/bridge/src/plugins.rs"));
    assert!(
        !src.contains("read_dir"),
        "生产段出现了 `read_dir` —— 本模块只数 manifest 的声明，\
             数快照目录里的文件夹就是把「下载快照的内容」说成「用户装的」"
    );
}
