use super::*;

/// 合成一份远端配置。**全字段合成**，不读任何真配置 ——
/// `host` 用 `.invalid`（RFC 2606 保留），确保就算有人不小心让它真去连，
/// DNS 也解不出来。
fn synth_cfg(label: &str) -> crate::ssh_source::RemoteConfig {
    crate::ssh_source::RemoteConfig {
        host: "example.invalid".into(),
        label: label.into(),
        port: 22,
        user: "nobody".into(),
        key_path: None,
        backend_path: "/nonexistent/cc-monitor-backend".into(),
        host_key_fingerprint: None,
        addresses: Vec::new(),
        jump: None,
    }
}

/// 造一棵**结构**上像样的临时目录树（名字全合成）。回 `(根, 根下那个子目录)`。
fn synth_tree(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "ccm-filewin-shell-{}-{}-{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let sub = root.join("sub");
    std::fs::create_dir_all(sub.join("deeper")).unwrap();
    std::fs::write(root.join("f.txt"), b"xyz").unwrap();
    std::fs::write(sub.join("inner.txt"), b"0123456789").unwrap();
    (root, sub)
}

fn names(w: &FileWindow) -> Vec<String> {
    w.listing
        .rows
        .lock()
        .unwrap()
        .iter()
        .map(|r| r.name.clone())
        .collect()
}

/// 骨架真的立得起来：本机源、列一个真目录、状态进得去。
/// ⚠ **这条不开窗**（本机无图形会话）—— 它买的是「窗口状态机与数据面接得上」。
#[test]
fn a_local_window_loads_its_directory_without_a_display() {
    let (root, _) = synth_tree("load");
    let w = FileWindow::new(Source::Local, root.to_string_lossy().to_string(), None);
    assert_eq!(names(&w), vec!["sub", "f.txt"]);
    assert!(w.listing.error.lock().unwrap().is_none());
    std::fs::remove_dir_all(&root).ok();
}

/// 远端源拿不到运行时就**出声**，不假装列了个空目录。
#[test]
fn a_remote_window_without_a_runtime_says_so_instead_of_showing_an_empty_dir() {
    let w = FileWindow::new(
        Source::Remote(Box::new(synth_cfg("synthetic-origin"))),
        "/tmp".into(),
        None,
    );
    assert!(w.listing.rows.lock().unwrap().is_empty());
    let e = w.listing.error.lock().unwrap().clone();
    assert!(e.is_some(), "没有运行时却没报错 —— 那是静默的空列表");
}

/// 反空真：错误目录必须留下错误，不是一个空列表。
#[test]
fn a_bad_local_path_surfaces_an_error() {
    let w = FileWindow::new(
        Source::Local,
        "/definitely/not/a/real/path/9f3a".into(),
        None,
    );
    assert!(w.listing.rows.lock().unwrap().is_empty());
    assert!(w.listing.error.lock().unwrap().is_some());
}

/// `Source::label()` 是窗口标题的来源，别让它回空串。
#[test]
fn every_source_has_a_non_empty_label() {
    assert_eq!(Source::Local.label(), "本机");
    assert_eq!(
        Source::Remote(Box::new(synth_cfg("tagged"))).label(),
        "tagged"
    );
    // `label` 为空时回退到 `host`（`RemoteConfig::origin_label` 的契约）。
    let mut anon = synth_cfg("");
    anon.label.clear();
    assert_eq!(Source::Remote(Box::new(anon)).label(), "example.invalid");
}

// ════════════════════════════════════════════════════════════════════════
// 第二刀 · ② 那条链：**列它的目录 → 能往下走 → 退得回来**
// ════════════════════════════════════════════════════════════════════════

/// 🔴 **「能往下走」这件事有判据了。**
///
/// 本机那一侧跑的是**真的**（真目录、真 `read_dir`），所以这条买到的是**行为**，
/// 不是「源码里有个叫 navigate 的函数」。远端那一侧结构上走的是同一条
/// [`FileWindow::navigate_to`]，只是它的 `reload()` 分派到 `list_remote`
/// —— 那一段的读数买不到（红线不许起真连接），委派有判据（`source_tests`）。
#[test]
fn double_clicking_a_directory_walks_into_it_and_up_walks_back() {
    let (root, sub) = synth_tree("walk");
    let mut w = FileWindow::new(Source::Local, root.to_string_lossy().to_string(), None);
    assert_eq!(names(&w), vec!["sub", "f.txt"]);

    // `sub` 是第 0 行（目录在前）。点开它。
    assert!(w.activate(0), "点开一个目录却没换目录");
    assert_eq!(w.cwd, sub.to_string_lossy().to_string());
    // 相等断言：进去之后列的是 `sub` 的内容，不是上一层留下的。
    assert_eq!(names(&w), vec!["deeper", "inner.txt"]);

    // 退回上一级。
    w.navigate_up();
    assert_eq!(w.cwd, root.to_string_lossy().to_string());
    assert_eq!(names(&w), vec!["sub", "f.txt"]);
    std::fs::remove_dir_all(&root).ok();
}

/// 点一个**文件**：什么都不做。
///
/// ⚠ 这不是「还没做完」的占位 —— 文件那一侧（预览/编辑/下载）这一刀明确没做，
/// 而「点了文件把 cwd 换成那个文件的路径」会让下一趟 `read_dir` 报错，
/// 屏幕上出现一条莫名其妙的红字。⇒ 明确的不动。
#[test]
fn clicking_a_file_does_nothing_rather_than_cd_into_it() {
    let (root, _) = synth_tree("file");
    let mut w = FileWindow::new(Source::Local, root.to_string_lossy().to_string(), None);
    let before = w.cwd.clone();
    // 第 1 行是 `f.txt`（目录在前 ⇒ 第 0 行是 `sub`）。
    assert!(!w.activate(1), "点文件竟然换了目录");
    assert_eq!(w.cwd, before);
    assert!(w.listing.error.lock().unwrap().is_none());
    // 越界也不许 panic（列表随时可能刚被刷短）。
    assert!(!w.activate(999));
    std::fs::remove_dir_all(&root).ok();
}

/// 到顶了就停住 —— 不许一路 `..` 走出文件系统。
#[test]
fn walking_up_from_the_top_stays_at_the_top() {
    let remote = Source::Remote(Box::new(synth_cfg("r")));
    assert_eq!(parent_dir(&remote, "/"), "/");
    assert_eq!(parent_dir(&remote, "/a"), "/");

    let mut w = FileWindow::new(remote, "/".into(), None);
    w.navigate_up();
    assert_eq!(w.cwd, "/", "从根再往上走，路径变了");
}

/// 🔴 **换目录必须把上一个目录的行清掉。**
///
/// 失效形状是真的：远端 A 慢、B 快 ⇒ 路径栏写着 B、列表里是 A 的文件，
/// 而用户会对着 B 的路径删 A 的东西。
#[test]
fn changing_directory_clears_the_previous_rows_instead_of_leaving_them_up() {
    let (root, _) = synth_tree("clear");
    // 起点是个真目录 ⇒ 列表非空；再换到一个**远端**目录（没有运行时 ⇒ 列不出来）。
    let mut w = FileWindow::new(Source::Local, root.to_string_lossy().to_string(), None);
    assert!(!w.listing.rows.lock().unwrap().is_empty());

    w.source = Source::Remote(Box::new(synth_cfg("r")));
    w.navigate_to("/somewhere/else".into());
    assert!(
        w.listing.rows.lock().unwrap().is_empty(),
        "换了目录还留着上一个目录的 {} 行",
        w.listing.rows.lock().unwrap().len()
    );
    assert!(
        w.listing.error.lock().unwrap().is_some(),
        "列不出来却既没有行也没有错 —— 那是静默的空目录"
    );
    std::fs::remove_dir_all(&root).ok();
}

/// 🔴 **迟到的那一份不许盖掉新目录的内容。**
///
/// 判 [`store_if_current`] 本体（它正是生产那条路上**唯一**写 `rows` 的地方）：
/// 号对 ⇒ 落盘、回 `true`；号过期 ⇒ 丢掉、回 `false`，而且**一个字节都不改**。
#[test]
fn a_late_answer_from_the_directory_we_left_is_thrown_away() {
    let l = Listing::default();
    let row = |n: &str| Row {
        name: n.to_string(),
        path: format!("/x/{n}"),
        is_dir: false,
        size: 0,
        lossy_name: false,
    };

    // A 出发（拿到号 0）。
    let a = l.start();
    // 人在 A 回来之前换到了 B。
    l.invalidate();
    let b = l.start();
    assert_ne!(a, b, "换目录之后号没变 —— 那这条判据与生产都在空转");

    // B 先回来。
    assert!(l.inflight.load(std::sync::atomic::Ordering::SeqCst) > 0);
    assert!(store_if_current(&l, b, Ok(vec![row("from-B")])));
    // A 迟到。
    assert!(
        !store_if_current(&l, a, Ok(vec![row("from-A")])),
        "过期的那一份居然落盘了"
    );

    // 相等断言：盘上只有 B 的东西。
    assert_eq!(
        l.rows
            .lock()
            .unwrap()
            .iter()
            .map(|r| r.name.clone())
            .collect::<Vec<_>>(),
        vec!["from-B".to_string()]
    );
    // 在飞数收干净了 ⇒ 转圈图标不会永远转着。
    assert!(!l.is_loading(), "两趟都回来了，还说在加载");
}

/// 反空真：号对的时候它**真的**会落盘（否则上面那条「回 false」毫无意义）。
#[test]
fn the_epoch_guard_is_not_just_refusing_everything() {
    let l = Listing::default();
    let mine = l.start();
    assert!(store_if_current(&l, mine, Ok(Vec::new())));
    assert!(l.error.lock().unwrap().is_none());

    let mine = l.start();
    assert!(store_if_current(&l, mine, Err("炸了".into())));
    assert_eq!(l.error.lock().unwrap().clone(), Some("炸了".to_string()));
}

/// 「正在列」这件事要看得见 —— 否则远端慢的时候屏幕上是一个空列表，
/// 与「这个目录真的是空的」长得一模一样。
#[test]
fn a_listing_in_flight_is_visible_as_loading() {
    let l = Listing::default();
    assert!(!l.is_loading());
    let mine = l.start();
    assert!(l.is_loading(), "开了一趟却不说在加载");
    store_if_current(&l, mine, Ok(Vec::new()));
    assert!(!l.is_loading());
}

/// 「本机」那颗按钮：`list_local` 今天唯一的**用户可达**入口。
#[test]
fn the_local_button_switches_the_window_to_the_local_side() {
    let (root, _) = synth_tree("localbtn");
    let mut w = FileWindow::new(
        Source::Remote(Box::new(synth_cfg("r"))),
        "/remote/dir".into(),
        None,
    );
    assert!(w.source.is_remote());
    w.go_local(root.to_string_lossy().to_string());
    assert!(!w.source.is_remote());
    assert_eq!(names(&w), vec!["sub", "f.txt"]);
    assert!(
        w.listing.error.lock().unwrap().is_none(),
        "切到本机之后还挂着远端那条报错"
    );
    std::fs::remove_dir_all(&root).ok();
}

// ════════════════════════════════════════════════════════════════════════
// 第二刀 · ③ `§5.4d` 在窗口这一侧接得上吗
// ════════════════════════════════════════════════════════════════════════

/// 拖进来的本机路径 → 待传清单：**目标目录就是当前目录**，名字取 basename。
#[test]
fn dropped_paths_become_pending_uploads_into_the_current_directory() {
    let w = FileWindow::new(
        Source::Remote(Box::new(synth_cfg("r"))),
        "/srv/data".into(),
        None,
    );
    let got = w.pending_for(&["/home/u/a.txt".to_string(), "/home/u/dir/b.bin".to_string()]);
    assert_eq!(
        got.iter()
            .map(|p| p.remote_path.clone())
            .collect::<Vec<_>>(),
        vec!["/srv/data/a.txt", "/srv/data/b.bin"]
    );
    assert_eq!(
        got.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
        vec!["a.txt", "b.bin"]
    );
}

/// 本机那一侧**不接**拖入 —— 本机拖本机是「复制文件」，那是另一件事，本刀不做。
#[test]
fn dropping_onto_the_local_side_yields_nothing_to_upload() {
    let w = FileWindow::new(Source::Local, "/tmp".into(), None);
    assert!(w.pending_for(&["/home/u/a.txt".to_string()]).is_empty());
}

/// 接不上就**出声**：远端源 ＋ 没有运行时 ⇒ 不许静默吞掉一摞文件。
#[test]
fn a_drop_with_no_runtime_says_so_instead_of_swallowing_the_files() {
    let mut w = FileWindow::new(
        Source::Remote(Box::new(synth_cfg("r"))),
        "/srv/data".into(),
        None,
    );
    let items = w.pending_for(&["/home/u/a.txt".to_string()]);
    assert_eq!(items.len(), 1);
    *w.listing.error.lock().unwrap() = None;
    assert!(!w.start_drop(items, None), "没有运行时却说起得来");
    assert!(
        w.listing.error.lock().unwrap().is_some(),
        "一摞文件被吞了，屏幕上一句话都没有"
    );
}

/// 传完一趟要重列目录（新文件得出现），而且**只重列一次**。
#[test]
fn finishing_a_drop_round_triggers_exactly_one_reload() {
    let (root, _) = synth_tree("round");
    let mut w = FileWindow::new(Source::Local, root.to_string_lossy().to_string(), None);
    assert!(!w.settle_finished_drops(), "一趟都没跑却说要重列");

    w.board.finish(crate::filewin::transfer::DropOutcome {
        asked: 0,
        skipped: 0,
        ok: 1,
        failed: Vec::new(),
    });
    assert!(w.settle_finished_drops(), "跑完一趟却不重列");
    assert!(!w.settle_finished_drops(), "同一趟重列了第二次");
    std::fs::remove_dir_all(&root).ok();
}

// ════════════════════════════════════════════════════════════════════════
// 第三刀 · 零流量复制在窗口这一侧接得上吗
// ════════════════════════════════════════════════════════════════════════

/// 造一个「看着某个远端目录、列表里有几行」的窗口。**不起连接**
/// （`host` 是 `.invalid`，而且这几条一次 `reload` 都不触发远端那一支）。
fn remote_window_with_rows(cwd: &str, rows: Vec<Row>) -> FileWindow {
    let mut w = FileWindow::seeded(
        Source::Remote(Box::new(synth_cfg("r"))),
        cwd.to_string(),
        None,
        rows,
    );
    *w.listing.error.lock().unwrap() = None;
    w.tally = crate::filewin::rows::RenderTally::default();
    w
}

fn file_row(name: &str) -> Row {
    Row {
        name: name.to_string(),
        path: format!("/srv/data/{name}"),
        is_dir: false,
        size: 9,
        lossy_name: false,
    }
}

/// 🔴 **胶水那一跳有判据了**：列表说「第 i 行的复制被点了」→ 窗口摆出「复制为」框。
///
/// ⚠ 它判的正是 `show_file_rows` 与 `begin_copy` **中间**那一跳 ——
/// 两头各自都有判据，而这一跳写在 `ui()` 里的话谁都没在看
/// （第一刀栽过的那一形：判据钉的是副本，生产那一份没人管）。
#[test]
fn a_copy_click_from_the_list_puts_up_the_rename_box_for_that_row() {
    let mut w = remote_window_with_rows("/srv/data", vec![file_row("a.bin"), file_row("b.bin")]);
    assert!(w.copy_prompt().is_none(), "什么都没点就摆出了框");
    assert!(!w.apply_copy_click(), "没人点却说摆出来了");

    w.tally.copy_clicked = Some(1);
    assert!(w.apply_copy_click(), "第 1 行的复制被点了，框却没摆出来");
    let p = w.copy_prompt().expect("框不见了");
    // 相等断言，逐项：摆的是**那一行**、目标落在**当前目录**、缺省名同旧面板。
    assert_eq!(p.from, "/srv/data/b.bin");
    assert_eq!(p.src_name, "b.bin");
    assert_eq!(p.dir, "/srv/data");
    assert_eq!(p.new_name, "b.bin.copy");
}

/// 目录 · 有损名 · 越界下标 —— 三档都**不摆框**（第二道闸，防「按钮没了、调用还在」）。
#[test]
fn directories_lossy_names_and_out_of_range_rows_put_up_nothing() {
    let dir = Row {
        name: "adir".into(),
        path: "/srv/data/adir".into(),
        is_dir: true,
        size: 0,
        lossy_name: false,
    };
    let lossy = Row {
        name: "\u{FFFD}odd".into(),
        path: "/srv/data/\u{FFFD}odd".into(),
        is_dir: false,
        size: 1,
        lossy_name: true,
    };
    let mut w = remote_window_with_rows("/srv/data", vec![dir, lossy]);
    assert!(!w.begin_copy(0), "目录也摆出了「复制为」框");
    assert!(!w.begin_copy(1), "有损名也摆出了「复制为」框");
    assert!(!w.begin_copy(99), "越界下标也摆出了框（或者 panic 了）");
    assert!(w.copy_prompt().is_none());
}

/// 本机那一侧**出声**：`copy-data` 是 SFTP 协议的扩展，本机复制压根不经 SFTP。
#[test]
fn the_local_side_refuses_to_copy_and_says_why() {
    let (root, _) = synth_tree("copylocal");
    let mut w = FileWindow::new(Source::Local, root.to_string_lossy().to_string(), None);
    *w.listing.error.lock().unwrap() = None;
    assert!(!w.begin_copy(1), "本机那一侧竟然摆出了「复制为」框");
    assert!(
        w.listing.error.lock().unwrap().is_some(),
        "本机那一侧点了复制，屏幕上一句话都没有 —— 那与「点了没反应」分不开"
    );
    std::fs::remove_dir_all(&root).ok();
}

/// 接不上就**出声**：远端源 ＋ 没有运行时 ⇒ 不许静默吞掉一趟复制。
#[test]
fn a_copy_with_no_runtime_says_so_instead_of_doing_nothing() {
    let mut w = remote_window_with_rows("/srv/data", vec![file_row("a.bin")]);
    let job = crate::filewin::copy::CopyJob::beside("/srv/data/a.bin", "/srv/data", "a.bin.copy")
        .expect("这个名字应当是合法的");
    assert!(!w.start_copy(job, None), "没有运行时却说起得来");
    assert!(
        w.listing.error.lock().unwrap().is_some(),
        "一趟复制被吞了，屏幕上一句话都没有"
    );
}

/// 🔴 名字不合法 ⇒ **框留着 ＋ 出声**，不静默收掉。
///
/// 收掉的话用户点了「复制」什么都没发生，与「已经开始复制了」长得一模一样。
#[test]
fn an_impossible_new_name_keeps_the_box_up_and_says_why() {
    let mut w = remote_window_with_rows("/srv/data", vec![file_row("a.bin")]);
    w.tally.copy_clicked = Some(0);
    assert!(w.apply_copy_click());

    for bad in ["", "   ", "sub/a.bin", "a.bin"] {
        *w.listing.error.lock().unwrap() = None;
        w.copy_prompt.as_mut().unwrap().new_name = bad.to_string();
        assert!(!w.confirm_copy(None), "「{bad}」这个名字竟然起得来");
        assert!(
            w.copy_prompt().is_some(),
            "「{bad}」被拒了，框却收掉了 —— 用户会以为复制开始了"
        );
        assert!(
            w.listing.error.lock().unwrap().is_some(),
            "「{bad}」被拒了却一句话都没说"
        );
    }
    // 反空真：换一个能用的名字，它就不再卡在「名字不合法」这一支上
    //（这个窗口没有运行时 ⇒ 它会卡在下一支，而那一支说的是另一件事）。
    *w.listing.error.lock().unwrap() = None;
    w.copy_prompt.as_mut().unwrap().new_name = "a.bin.copy".to_string();
    assert!(!w.confirm_copy(None), "没有运行时却说起得来");
    let e = w.listing.error.lock().unwrap().clone().unwrap();
    assert!(
        e.contains("运行时"),
        "合法名字被当成不合法挡了：{e} —— 那上面那几条买的就不是「名字」这一维"
    );
    // 取消把框收掉。
    w.cancel_copy();
    assert!(w.copy_prompt().is_none());
}

/// 复制跑完一趟要重列目录（复制出来的那份得出现），而且**只重列一次**。
#[test]
fn finishing_a_copy_round_triggers_exactly_one_reload() {
    let (root, _) = synth_tree("copyround");
    let mut w = FileWindow::new(Source::Local, root.to_string_lossy().to_string(), None);
    assert!(!w.settle_finished_copies(), "一趟都没跑却说要重列");

    w.copy_board
        .finish(crate::filewin::copy::CopyOutcome::Done {
            asked: false,
            verdict: None,
        });
    assert!(w.settle_finished_copies(), "跑完一趟却不重列");
    assert!(!w.settle_finished_copies(), "同一趟重列了第二次");
    std::fs::remove_dir_all(&root).ok();
}
