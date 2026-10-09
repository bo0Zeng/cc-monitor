//! 要求：「文件管理器**做**按内容搜（那台后端执行、有字节与条数上界、可撤；标准文件管理器能力）」
//! ＋「不跟符号链接出界」。`files/grep.rs` 的判据。
//!
//! | 判据 | 钉的那一形 | 两侧异源在哪 |
//! |---|---|---|
//! | [`a_planted_tree_greps_to_hand_counted_hits`] | 铺好的树 ⇒ 命中的份 · 行号 · 那一段 · 命中几行 · 顺序逐格等于手写；忽略 ASCII 大小写那一格 | 期望手写 |
//! | [`links_are_counted_and_never_followed_out_of_the_tree`] | 树里一条链接指向树外、树外那份里有要找的 ⇒ 不命中、`links` 记一；根本身是链接 ⇒ 一份都不读 | 期望手写 |
//! | [`every_bound_stops_the_walk_and_says_which`] | 命中份数到 `limit` ⇒ `Hits`；累计字节到上界 ⇒ `Bytes`；单份过大 ⇒ 跳过记数；二进制 ⇒ 跳过记数；别的文件系统 ⇒ 不进 | 期望手写；小上界由判据注入 |
//! | [`a_raised_cancel_flag_stops_before_reading_anything`] | 取消位已置 ⇒ `Cancelled`、一份都没读 | 期望手写 |
//! | [`the_command_face_answers_exactly_the_declared_fields`] | 线上回的键 == 声明的 `fields`；`needle` 空 / 超长 ⇒ `bad_args`；读不到 ⇒ `unreadable` | 声明住 `CAPABILITIES` |

use super::*;

fn temp_root(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-grep-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&p).ok();
    std::fs::create_dir_all(&p).expect("建临时根");
    p
}

fn args(needle: &str) -> GrepArgs {
    GrepArgs {
        needle: needle.as_bytes().to_vec(),
        ignore_ascii_case: false,
        limit: DEFAULT_LIMIT,
    }
}

fn run(top: &Path, a: &GrepArgs) -> Grepped {
    search_with(top, a, &AtomicBool::new(false), |_| Some(1), CAPS).expect("搜不了")
}

/// `t/{b.txt, a.txt, sub/{c.txt, bin.dat}}`：`a.txt` 两行命中、`c.txt` 一行、`b.txt` 不中、`bin.dat` 带 NUL。
fn plant(base: &Path) -> PathBuf {
    let t = base.join("t");
    std::fs::create_dir_all(t.join("sub")).expect("铺");
    std::fs::write(
        t.join("a.txt"),
        b"first line\n  the Needle here  \nneedle again\n",
    )
    .expect("铺");
    std::fs::write(t.join("b.txt"), b"nothing to see\n").expect("铺");
    std::fs::write(t.join("sub/c.txt"), b"x\ny\nfind the needle\n").expect("铺");
    std::fs::write(t.join("sub/bin.dat"), b"needle\0binary").expect("铺");
    t
}

#[test]
fn a_planted_tree_greps_to_hand_counted_hits() {
    let base = temp_root("tree");
    let t = plant(&base);
    let g = run(&t, &args("needle"));
    let got: Vec<(String, u64, String, u64)> = g
        .hits
        .iter()
        .map(|h| {
            (
                String::from_utf8(h.path.clone()).unwrap(),
                h.line,
                String::from_utf8(h.text.clone()).unwrap(),
                h.matches,
            )
        })
        .collect();
    let at = |rel: &str| t.join(rel).to_str().unwrap().to_string();
    assert_eq!(
        got,
        vec![
            (at("a.txt"), 3, "needle again".to_string(), 1),
            (at("sub/c.txt"), 3, "find the needle".to_string(), 1),
        ],
        "大小写敏感：`Needle` 那一行不算；先这一层的文件（按名字）、再子目录"
    );
    assert_eq!(
        (g.files, g.skipped_binary, g.stopped),
        (4, 1, Stopped::Done)
    );
    // 只忽略 ASCII 大小写 ⇒ `a.txt` 第 2 行也中、共两行；那一段首尾的空白剥掉。
    let mut ci = args("NEEDLE");
    ci.ignore_ascii_case = true;
    let g = run(&t, &ci);
    assert_eq!(g.hits[0].line, 2);
    assert_eq!(g.hits[0].text, b"the Needle here".to_vec());
    assert_eq!(g.hits[0].matches, 2);
    std::fs::remove_dir_all(&base).ok();
}

#[test]
#[cfg(unix)]
fn links_are_counted_and_never_followed_out_of_the_tree() {
    let base = temp_root("links");
    let t = base.join("t");
    let outside = base.join("outside");
    std::fs::create_dir_all(&t).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("secret.txt"), b"needle outside\n").unwrap();
    std::os::unix::fs::symlink(&outside, t.join("dir-link")).unwrap();
    std::os::unix::fs::symlink(outside.join("secret.txt"), t.join("file-link")).unwrap();
    let g = run(&t, &args("needle"));
    assert!(g.hits.is_empty(), "跟着链接出了界：{:?}", g.hits);
    assert_eq!((g.links, g.files), (2, 0));
    // 根本身是一条链接 ⇒ 不进去。
    let g = run(&t.join("dir-link"), &args("needle"));
    assert!(g.hits.is_empty() && g.files == 0 && g.links == 1);
    // 正控：直接搜树外那一份 ⇒ 中（证明上面那条不是「什么都搜不到」）。
    assert_eq!(run(&outside, &args("needle")).hits.len(), 1);
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn every_bound_stops_the_walk_and_says_which() {
    let base = temp_root("bounds");
    let t = base.join("t");
    std::fs::create_dir_all(t.join("m")).unwrap();
    for n in ["a", "b", "c"] {
        std::fs::write(t.join(format!("{n}.txt")), b"needle\n").unwrap();
    }
    std::fs::write(t.join("m/z.txt"), b"needle\n").unwrap();
    // 条数：limit 2 ⇒ 两份就停。
    let mut a = args("needle");
    a.limit = 2;
    let g = run(&t, &a);
    assert_eq!((g.hits.len(), g.stopped), (2, Stopped::Hits));
    // 字节：累计上界 15（每份 7 字节）⇒ 读两份，第三份会越界 ⇒ 停。
    let small = Caps {
        file_max_bytes: 1 << 20,
        total_max_bytes: 15,
    };
    let g = search_with(
        &t,
        &args("needle"),
        &AtomicBool::new(false),
        |_| Some(1),
        small,
    )
    .unwrap();
    assert_eq!((g.files, g.bytes, g.stopped), (2, 14, Stopped::Bytes));
    // 单份过大：上界 6 ⇒ 每份都跳过、记数，走完。
    let tiny = Caps {
        file_max_bytes: 6,
        total_max_bytes: 1 << 20,
    };
    let g = search_with(
        &t,
        &args("needle"),
        &AtomicBool::new(false),
        |_| Some(1),
        tiny,
    )
    .unwrap();
    assert_eq!((g.files, g.skipped_large, g.stopped), (0, 4, Stopped::Done));
    // 别的文件系统：`m` 在另一个设备上 ⇒ 不进去（正控：上面不注入时 `m/z.txt` 是被读到的那一份之一）。
    let mdir = t.join("m");
    let g = search_with(
        &t,
        &args("needle"),
        &AtomicBool::new(false),
        |p| Some(if p == mdir { 2 } else { 1 }),
        CAPS,
    )
    .unwrap();
    assert_eq!((g.hits.len(), g.skipped_mounts), (3, 1));
    assert_eq!(run(&t, &args("needle")).hits.len(), 4);
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn a_raised_cancel_flag_stops_before_reading_anything() {
    let base = temp_root("cancel");
    let t = plant(&base);
    let g = search_with(
        &t,
        &args("needle"),
        &AtomicBool::new(true),
        |_| Some(1),
        CAPS,
    )
    .unwrap();
    assert_eq!(
        (g.stopped, g.files, g.hits.len()),
        (Stopped::Cancelled, 0, 0)
    );
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn the_command_face_answers_exactly_the_declared_fields() {
    let base = temp_root("face");
    let t = plant(&base);
    let p = t.to_str().unwrap();
    let v = crate::files::answer(
        "files.grep",
        &serde_json::json!({ "path": p, "needle": "needle" }),
    )
    .unwrap();
    let got: std::collections::BTreeSet<String> = v.as_object().unwrap().keys().cloned().collect();
    let declared: std::collections::BTreeSet<String> = crate::files::CAPABILITIES
        .iter()
        .find(|c| c.name == "files.grep")
        .unwrap()
        .fields
        .iter()
        .map(|f| f.to_string())
        .collect();
    assert_eq!(got, declared);
    let hit: std::collections::BTreeSet<&str> = v["hits"][0]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        hit,
        ["line", "lines", "matches", "path", "rel", "text"]
            .into_iter()
            .collect()
    );
    assert_eq!(
        (v["truncated"].clone(), v["stopped"].clone()),
        (serde_json::json!(false), serde_json::Value::Null)
    );
    let code = |a: serde_json::Value| crate::files::answer("files.grep", &a).unwrap_err().code;
    assert_eq!(
        code(serde_json::json!({ "path": p, "needle": "" })),
        "bad_args"
    );
    assert_eq!(
        code(serde_json::json!({ "path": p, "needle": "x".repeat(NEEDLE_MAX_BYTES + 1) })),
        "bad_args"
    );
    assert_eq!(code(serde_json::json!({ "path": p })), "bad_args");
    assert_eq!(code(serde_json::json!({ "needle": "n" })), "bad_path");
    assert_eq!(
        code(serde_json::json!({ "path": base.join("gone").to_str().unwrap(), "needle": "n" })),
        "unreadable"
    );
    std::fs::remove_dir_all(&base).ok();
}

/// 要求住址同上（跨半边那一格）：`files-grep` 的成品对跨语言金样 `tests/__fixtures__/files-grep.golden.json` 逐字相等
/// （铺的那棵树根换成 `<root>`）；窗口那一侧 `tests/frontend/filewin/grep_tests.rs` 解同一份金样。
#[test]
fn the_product_matches_the_cross_half_golden() {
    let base = temp_root("golden");
    let t = plant(&base);
    let root = t.to_str().unwrap().to_string();
    let v = crate::files::answer(
        "files.grep",
        &serde_json::json!({ "path": root, "needle": "needle", "ignore_ascii_case": true }),
    )
    .unwrap();
    let text = serde_json::to_string_pretty(&v)
        .unwrap()
        .replace(&root, "<root>");
    let got: serde_json::Value = serde_json::from_str(&text).unwrap();
    let golden: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/files-grep.golden.json")).unwrap();
    assert_eq!(got, golden, "成品与金样不等（实得：{text}）");
    std::fs::remove_dir_all(&base).ok();
}
