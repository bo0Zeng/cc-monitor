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

// ════════════════════════════════════════════════════════════════════════
// 🔴〔第四刀 2026-09-20〕**实景**：那条路走完之后，窗口真的起来了
// ════════════════════════════════════════════════════════════════════════
//
// # 这一格此前记的是「判不了」，而那个判断**框大了一格**
//
// `真相源/99 §9.4` 逐字：「那个 egui 窗口真的出现在屏幕上 —— **判不动**。
// 本机没有图形会话 ⇒ `eframe::run_native` 在这台机器上必然失败。
// **缺一台有图形会话的机器。**」而同一份文件的 `§八` 那四趟读数
// （17 945 帧 / 29 秒）**本来就是在 Xvfb 上打的** —— 「有画面的机器」一直在手上。
//
// ⇒ 本段把「窗口真的被 X 服务器映射出来了」变成判据。台架与它逐格买不到什么
// 住 `xvfb_rig` 那份文件的头注（真 GPU / 字体回落 / DPI / 合成器 **四样都买不到**）。
//
// # 🔴 这一段刻意分成三条判据，各钉一件
//
// | 判据 | 它钉的那一形 | 少了它会怎样 |
// |---|---|---|
// | `the_native_window_really_comes_up_on_a_real_graphics_session` | 有画面的机器上，那条路真的把一个窗口摆到屏幕上，而且事件循环干净退出 | 「起不来」与「起来了」分不开 |
// | `a_window_that_cannot_come_up_comes_back_as_a_reason_not_a_silent_ok` | **阴性对照**：没有 X 服务器时那条路回的是一句**非空的原因** | 上一条可能恒真（一条永远回 `Ok` 的假实现照样绿） |
// | `the_second_window_in_one_process_is_never_a_silent_success` | 「说自己成了」与「屏幕上真有一个窗口」**必须一致** | 静默成功那一形没人看着（而它今天就活着，见那条判据的头注） |
//
// ⚠ 前两条各跑一趟自己的子进程，第三条与第一条**共用同一趟**（`scenario_a`）——
// 理由是 winit 一个进程只许一个事件循环，一趟子进程只量得到一趟实景开窗。

/// 这个窗口在窗口树里的认法：标题里那几个字。
///
/// ⚠ 刻意**不**跟生产那句标题逐字对：那是把一段文案抄成第二份。
/// 判「是不是我们这个窗口」靠 `Source::label()`（生产那个函数）在标题里出现。
#[cfg(not(windows))]
const WINDOW_NEEDLE: &str = "cc-monitor";

/// 等一条线程上的 `run_native` 收场，回 `(裁决, 原因)`。
///
/// 🔴 **裁决是四态不是两态**：`ok` / `err` / `panic` / `timeout`。
/// 合成两态就会把「那条线程炸了」与「它规规矩矩回了个错」揉在一起，
/// 而那两样在「上层接不接得住」这件事上不一样：`Err` 装得下一句话，
/// panic 只落在那条线程的 stderr 上。
#[cfg(not(windows))]
fn join_verdict(
    h: std::thread::JoinHandle<Result<(), String>>,
    budget_ms: u64,
) -> (&'static str, String) {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(h.join());
    });
    match rx.recv_timeout(std::time::Duration::from_millis(budget_ms)) {
        Ok(Ok(Ok(()))) => ("ok", String::new()),
        Ok(Ok(Err(e))) => ("err", e),
        Ok(Err(p)) => {
            let why = p
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| p.downcast_ref::<&str>().map(|s| (*s).to_string()))
                .unwrap_or_default();
            ("panic", why)
        }
        Err(_) => ("timeout", String::new()),
    }
}

/// **实景工作面**：在一台真 X 服务器上把这个窗口开起来，量那几样东西。
///
/// ⚠ 它挂着 `#[ignore]` —— 平时 `cargo test` 里它是 `ignored`（终端上看得见，
/// 不会冒充一条绿），只由它的父判据在**自己一个子进程**里点起来。
#[cfg(not(windows))]
#[test]
#[ignore = "实景工作面：由 the_native_window_really_comes_up_on_a_real_graphics_session 在它自己的进程里点起来"]
fn xvfb_worker_opens_a_real_window() {
    use crate::filewin::rows::testing::xvfb;
    let display = xvfb::child_display();
    let (root, _) = synth_tree("xvfb-open");
    let cwd = root.to_string_lossy().to_string();
    let rows = list_local(&root).expect("列那棵临时目录树 —— 这一格的前提");
    xvfb::emit("a.seed_rows", rows.len());

    let req0 = open_requested();
    let opened0 = windows_opened();
    let h = open_detached_seeded(Source::Local, cwd.clone(), None, rows.clone());

    let ids = xvfb::wait_for_windows(&display, WINDOW_NEEDLE, 20_000);
    xvfb::emit("a.window_count", ids.len());
    if let Some(id) = ids.first() {
        xvfb::emit(
            "a.window_name",
            xvfb::xdotool_on(&display, &["getwindowname", id]).unwrap_or_default(),
        );
        match xvfb::geometry(&display, id) {
            Ok(g) => {
                xvfb::emit("a.window_w", g.w);
                xvfb::emit("a.window_h", g.h);
            }
            Err(e) => xvfb::emit("a.window_w", format!("几何问不到：{e}")),
        }
        // 关窗走的是那条「请你关掉」的窗口协议消息 —— winit 收到它才会把循环收干净，
        // 而「循环干净退出」正是下面 `a.run_native=ok` 要买的东西。
        let _ = xvfb::xdotool_on(&display, &["windowclose", id]);
    }
    xvfb::emit("a.open_requested_delta", open_requested() - req0);
    xvfb::emit("a.windows_opened_delta", windows_opened() - opened0);
    let (verdict, why) = join_verdict(h, 30_000);
    xvfb::emit("a.run_native", verdict);
    xvfb::emit("a.reason_len", why.chars().count());
    xvfb::emit("a.reason", why.replace('\n', " "));

    // ── 字体：**装字体那一步真的接在开窗这条路上吗** ────────────────
    // 🔴 扫源码买不到这一条。09-20 栽过的那一形逐字：源码扫描那条与行为那条
    //    买的不是同一样东西 —— 前者看不见「按钮接没接到方法上」。
    //    这里读的是**真窗口在自己第一帧上**写下的那份裁决。
    match font_verdict() {
        None => xvfb::emit("a.font_state", "never-checked"),
        Some(FontState::NotInstalled) => xvfb::emit("a.font_state", "not-installed"),
        Some(FontState::Pending(_)) => xvfb::emit("a.font_state", "pending"),
        Some(FontState::Checked(None)) => xvfb::emit("a.font_state", "checked-ok"),
        Some(FontState::Checked(Some(note))) => {
            xvfb::emit("a.font_state", "checked-note");
            xvfb::emit("a.font_note", note.replace('\n', " "));
        }
    }

    // ── 第二趟：**同一个进程、换一条线程** ────────────────────────────
    // 台架头注第四节论证过它必然走另一条路；这里把它**量出来**而不是推出来。
    let h2 = open_detached_seeded(Source::Local, cwd, None, rows);
    let ids2 = xvfb::wait_for_windows(&display, WINDOW_NEEDLE, 6_000);
    xvfb::emit("a.second_window_count", ids2.len());
    for id in &ids2 {
        let _ = xvfb::xdotool_on(&display, &["windowclose", id]);
    }
    let (v2, why2) = join_verdict(h2, 30_000);
    xvfb::emit("a.second_run_native", v2);
    xvfb::emit("a.second_reason_len", why2.chars().count());
    xvfb::emit("a.second_reason", why2.replace('\n', " "));

    std::fs::remove_dir_all(&root).ok();
}

/// **实景工作面（阴性对照）**：`DISPLAY` 指着一台**不存在**的 X 服务器。
#[cfg(not(windows))]
#[test]
#[ignore = "实景工作面（阴性对照）：由 a_window_that_cannot_come_up_comes_back_as_a_reason_not_a_silent_ok 点起来"]
fn xvfb_worker_opens_with_no_x_server_at_all() {
    use crate::filewin::rows::testing::xvfb;
    let display = xvfb::child_display();
    assert!(
        xvfb::xdotool_on(&display, &["getdisplaygeometry"]).is_err(),
        "阴性对照的前提没建立：{display} 上**真有**一台 X 服务器 —— \
         这一格判不了（它要的正是「一台都没有」）"
    );
    let (root, _) = synth_tree("xvfb-nodisp");
    let rows = list_local(&root).expect("列那棵临时目录树 —— 这一格的前提");
    let h = open_detached_seeded(
        Source::Local,
        root.to_string_lossy().to_string(),
        None,
        rows,
    );
    let (verdict, why) = join_verdict(h, 30_000);
    xvfb::emit("n.run_native", verdict);
    xvfb::emit("n.reason_len", why.chars().count());
    xvfb::emit("n.reason", why.replace('\n', " "));
    std::fs::remove_dir_all(&root).ok();
}

/// 这一趟实景子进程的读数（**只跑一趟**，两条判据共用）。
#[cfg(not(windows))]
fn scenario_a() -> &'static crate::filewin::rows::testing::xvfb::ChildRun {
    use crate::filewin::rows::testing::xvfb;
    static RUN: std::sync::OnceLock<xvfb::ChildRun> = std::sync::OnceLock::new();
    RUN.get_or_init(|| {
        xvfb::require_toolbox("「点了那颗按钮之后窗口真的起来了」");
        let screen = xvfb::Screen::start()
            .unwrap_or_else(|e| panic!("起不了 Xvfb ⇒ 这一格判不了，不是过了：{e}"));
        xvfb::run_scenario(
            screen.display(),
            "filewin::shell::tests::xvfb_worker_opens_a_real_window",
        )
    })
}

/// 🔴 **「点了那颗按钮之后，窗口真的起来了」—— 这一格从此有人看着了。**
///
/// 生产那条路一个字节没动（`open_detached_seeded` 里那趟 `eframe::run_native`）；
/// 变的只是它跑在一台**真 X 服务器**上，而判据从**窗口树**里把结果读回来。
///
/// # 它买到的（逐条）
///
/// - 那条路真的让 X 服务器**映射出恰好一个窗口**（相等断言，不是「至少一个」）。
/// - 那个窗口是**我们这一个**：标题里含生产那个 `Source::label()` 的输出。
/// - 窗口有**非零面积**（一个 0×0 的窗口在窗口树里照样数得到）。
/// - 那个窗口是**真的映射到屏幕上**的那一档（`--onlyvisible`）——
///   一个「建了却没摆出来」的窗口不算。
/// - 那两个计数器各 **+1**（`open_requested` 与 `windows_opened`）。
///
/// # ⚠ 它买不到的（逐条，别读宽）
///
/// - **真 GPU / 字体回落 / DPI / 合成器四样一个都买不到**（Xvfb 软渲染、
///   无窗口管理器、恒 96 dpi）。逐格住台架头注。
/// - **它不量帧时也不量内存** —— `真相源/99` 那些数是另一个分母，
///   不许拿这一趟去替换或「订正」。
/// - **Windows 一趟没跑过**（本族整条 `cfg(not(windows))`）。
/// - **「用户在旧面板上点那颗按钮」那一跳不在这一格里** —— 那一跳由
///   jsdom 那条与包装层那两条钉着（`真相源/99 §9.5` 刀 1／刀 2）。
///   这一格接的是它下游那一段：命令进来之后窗口起没起来。
/// - 🔴 **「用户点窗口那个关闭按钮，事件循环干净退出」—— 仍然判不了。**
///   缺的证据很具体：**一个窗口管理器**。本台架手上唯一那把锤子
///   （`xdotool` 那条关窗）走的是**硬销毁**，不是「请你关掉」那条窗口协议消息；
///   而生产那个窗口自己没有关闭入口（没有一颗「关闭」按钮），
///   本刀又**只许调不许改**生产那棵树 ⇒ 这一维今天没有第二条路。
///   现打读数（本判据每趟自己印出来）：硬销毁之后 `run_native` 不是回一个 `Err`，
///   而是在下一次几何查询上 **panic**（X11 `Drawable` 那一族的错）。
///   ⚠ 那条读数**不是**这一格的裁决，它是一条**登记**：见交回件里那条
///   「release 档 `panic = "abort"`」的跟进。
/// - **窗口里面的交互这一格买不到**：生产那个窗口没有可观测出口（判据读不到它的
///   `tally`），而生产那棵树不许改。那一维由 `rows` 那一格买 ——
///   同一条 `run_native` 路、同一个生产行画函数，见
///   `a_real_pointer_click_on_the_copy_button_comes_back_as_that_row`。
#[cfg(not(windows))]
#[test]
fn the_native_window_really_comes_up_on_a_real_graphics_session() {
    let run = scenario_a();
    run.must_have_passed("「窗口真的起来了」");

    assert_eq!(
        run.reading("a.window_count"),
        "1",
        "有画面的机器上开了一趟窗，窗口树里却数到 {} 个标题含 `{WINDOW_NEEDLE}` 的窗口 —— \
         0 就是没起来（此前这一支没人看着）",
        run.reading("a.window_count")
    );
    let name = run.reading("a.window_name");
    let label = Source::Local.label();
    assert!(
        name.contains(&label),
        "窗口标题是 {name:?}，里面没有 `Source::label()` 给的 {label:?} —— \
         数到的那个窗口多半不是我们这一个"
    );
    for (k, what) in [("a.window_w", "宽"), ("a.window_h", "高")] {
        let v: u32 = run
            .reading(k)
            .parse()
            .unwrap_or_else(|_| panic!("{what}读不出整数：{}", run.reading(k)));
        assert!(v > 0, "窗口{what}是 {v} —— 一个 0 像素的窗口照样能被数到");
    }
    // 🔴 关窗那一维**仍然判不了**，理由与缺的证据逐字写在本判据头注里；
    //    这里只把读数印出来，**不判**（判它就得把今天这一形钉成期望值）。
    println!(
        "  〔登记·判不了〕硬销毁那个窗口之后 `run_native` 的裁决：{} —— {}",
        run.reading("a.run_native"),
        run.reading("a.reason")
    );
    assert_eq!(run.reading("a.open_requested_delta"), "1");
    assert_eq!(run.reading("a.windows_opened_delta"), "1");
    // 反空真：喂进去的那一屏真的有行（否则上面全是在量一个空窗）。
    let seed: usize = run.reading("a.seed_rows").parse().unwrap_or(0);
    assert!(seed > 0, "喂给窗口的那一屏是空的 —— 这一趟量的是一个空窗");

    // 🔴 **字体**：真窗口在它自己的第一帧上复核过，而且没有话要说。
    //    这一条买的是「装字体那一步**接在了 `open_detached_seeded` 上**」——
    //    `fonts_tests.rs` 那一摞全在自己造的 `Context` 上跑，**看不见接没接上**。
    assert_eq!(
        run.reading("a.font_state"),
        "checked-ok",
        "真窗口的字体裁决是 `{}`{} —— `checked-ok` 之外每一形都意味着窗口上的中文是豆腐块：\
         `never-checked` / `not-installed` = 装字体那一步没接在开窗路上；\
         `pending` = 接上了但第一帧没复核；`checked-note` = 复核过、真有缺字",
        run.reading("a.font_state"),
        {
            let n = run.reading("a.font_note");
            if n.is_empty() {
                String::new()
            } else {
                format!("（{n}）")
            }
        }
    );
}

/// 🔴 **阴性对照**：起不来的时候，那条路回的是一句**非空的原因**。
///
/// 没有这一条，上面那条可能恒真 —— 一个「永远回 `Ok(())`、什么都不做」的假实现
/// 在上面那几条里除了窗口数之外全都会绿，而
/// `真相源/99 §8.5`／`§9.1` 记的那条教训（「编得过、跑得动、什么都不做」）
/// 正是这一族最常见的失效形状。
///
/// ⚠ 它买的是**壳那一层**：`open_detached_seeded` 的回值里装着原因。
/// **买不到**「那句原因走到了用户眼前」—— 入口那条命令把这个句柄丢掉了，
/// 见下面那条判据的头注。
#[cfg(not(windows))]
#[test]
fn a_window_that_cannot_come_up_comes_back_as_a_reason_not_a_silent_ok() {
    use crate::filewin::rows::testing::xvfb;
    xvfb::require_toolbox("「窗口起不来要出声」那条阴性对照");
    // 刻意给一个**没有任何 X 服务器**的号（台架自己只用 :90–:119）。
    let run = xvfb::run_scenario(
        ":121",
        "filewin::shell::tests::xvfb_worker_opens_with_no_x_server_at_all",
    );
    run.must_have_passed("「窗口起不来要出声」那条阴性对照");

    let verdict = run.reading("n.run_native");
    assert_ne!(
        verdict, "ok",
        "没有任何 X 服务器，那条路却回了 `Ok(())` —— 那是一条**静默成功**：\
         「窗口没起来」与「起来了」在回值上分不开"
    );
    let n: usize = run.reading("n.reason_len").parse().unwrap_or(0);
    assert!(
        n > 0,
        "裁决是 {verdict}，而原因是**空的** —— 一句空话与没有话在屏幕上分不开。原文：{}",
        run.reading("n.reason")
    );
    assert_eq!(
        verdict, "err",
        "裁决是 {verdict} 而不是 `err` —— panic／超时那两形上层**接不住**：\
         `open_detached_seeded` 回的是 `Result`，panic 只落在那条线程的 stderr 上"
    );
}

/// 🔴 **「说自己成了」与「屏幕上真有一个窗口」必须一致。**
///
/// # 这一条是 Xvfb 逮出来的一条**真东西**，如实写在这儿
///
/// winit 全进程只许建一个事件循环（那个「已经建过了」的进程级标志只在
/// web 平台会被清回去），而 eframe 把建好的那个缓存在**线程局部**里。
/// `open_detached_seeded` 每趟 `std::thread::spawn` 一条**新线程**
/// ⇒ **同一个进程里第二次开窗，必然失败。**
/// 现打读数（本判据每趟自己印出来）：第二趟裁决 `err`、窗口数 `0`。
/// ⇒ **用户把这个文件窗口关掉之后，这个 app 活着的时候再也开不起来了。**
///
/// ⚠ 而它今天是**静默**的：入口那条命令把 `open_detached_seeded` 的句柄丢掉
/// （`let _ = …`），命令照旧回 `Ok(行数)` ⇒ webview 那侧看到的是「成功」。
/// 这正是 `真相源/99 §9.4` 逐字留下的那一句：
/// 「**列得出来、窗口却起不来**那一支仍然没人看着」。
/// 🔴 **修它要动生产那棵树（本刀只许调不许改）⇒ 已交回报备，本刀不改。**
///
/// # 这条判据刻意写成「两支都绿、静默那一支红」
///
/// 它**不**断言「第二趟必须失败」—— 那样就把一个缺陷钉成了期望值，
/// 哪天有人真把它修好（复用那条线程／那个事件循环）这条判据会反过来拦住修复。
/// 它断言的是两支**各自自洽**：
///
/// - 回 `ok` ⇒ 屏幕上**必须**真有一个窗口（否则就是静默成功）；
/// - 回 `err`／`panic` ⇒ **必须**带着一句非空的原因。
///
/// ⇒ 今天走第二支（绿）；修好之后走第一支（还是绿）；
/// 变成「回 `ok` 而窗口没起来」⇒ **红**。
#[cfg(not(windows))]
#[test]
fn the_second_window_in_one_process_is_never_a_silent_success() {
    let run = scenario_a();
    run.must_have_passed("「第二趟开窗不许是静默成功」");
    let verdict = run.reading("a.second_run_native");
    let count = run.reading("a.second_window_count");
    let reason_len: usize = run.reading("a.second_reason_len").parse().unwrap_or(0);
    println!(
        "  第二趟开窗（同一个进程、换一条线程）：裁决 {verdict} · 窗口数 {count} · \
         原因 {reason_len} 字 · 原文 {}",
        run.reading("a.second_reason")
    );
    match verdict.as_str() {
        "ok" => assert_eq!(
            count, "1",
            "第二趟回了 `Ok(())`，而窗口树里有 {count} 个窗口 —— **静默成功**：\
             上层（入口那条命令）会把它当成功报给 webview，而屏幕上什么都没有"
        ),
        "err" | "panic" => assert!(
            reason_len > 0,
            "第二趟裁决是 {verdict} 而原因是空的 —— 上层连「为什么没起来」都拿不到"
        ),
        other => panic!(
            "第二趟裁决是 {other:?} —— 超时那一形意味着那条线程既没成也没回错，\
             它在上层眼里与成功一模一样"
        ),
    }
}
/// 🔴 **进程 DPI 归属：`any_thread_hook` 的 Windows 分支必须把 winit 关掉。**
///
/// 论证与四格现打读数住 `shell.rs` 头注（2026-09-20，本机那台 Win11 虚拟机的真桌面）。
/// 那四格里承重的是两对：
/// - 「没有 `tao`、`dpi_aware=true`」⇒ `UNAWARE` → `PER_MONITOR_AWARE_V2`
///   ⇒ **winit 自己确实会设进程级那块状态**（「两个主人」不是推测）；
/// - 「没有 `tao`、`dpi_aware=false`」⇒ 全程 `UNAWARE`
///   ⇒ **这个开关是活的**，关掉之后 winit 真的不去碰它。
///
/// ⚠ **这条只扫得动源码，扫不动行为** —— Windows 那个分支在本机（Linux）
/// 被 `cfg` 掉了，`cargo test` 永远执行不到它。能进执行链的只有「那一句在不在」。
/// ⇒ 所以它钉**两侧**：`false` 要在，且不许有人把它改回 `true`
/// （只钉「含 `with_dpi_aware`」的话，改成 `true` 不会红 —— 那就是一把恒绿的尺）。
#[test]
fn the_windows_branch_hands_process_dpi_to_tauri() {
    let shell =
        guard_core::production_code(include_str!("../../../src/bridge/src/filewin/shell.rs"));
    assert_eq!(
        shell.matches("with_dpi_aware(builder, false)").count(),
        1,
        "`any_thread_hook` 的 Windows 分支没有把 winit 的 DPI 设置关掉 —— \
         那个进程里就又有两个人在设 `SetProcessDpiAwarenessContext` 了"
    );
    assert!(
        !shell.contains("with_dpi_aware(builder, true)"),
        "有人把它改回 `true` 了 —— 现打读数说这一句是活的开关，\
         改回 `true` 就是把「两个主人」那条风险重新请回来"
    );
}
