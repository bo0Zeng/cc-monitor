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

/// 实景台架那两份 worker 与它们的父判据**共用的**那台合成远端的名字。
///
/// 🔴 抽成一个常量是承重的：父判据拿它算出期望的窗口标题、worker 拿它开窗，
/// 两处各写一份字面量时「标题里含那个名字」这一比会在两处一起改错时**恒真**。
#[cfg(not(windows))]
const XVFB_ORIGIN: &str = "xvfb-origin";

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

/// 远端源拿不到运行时就**出声**，不假装列了个空目录。
#[test]
fn a_remote_window_without_a_runtime_says_so_instead_of_showing_an_empty_dir() {
    let w = FileWindow::new(
        Source::remote(synth_cfg("synthetic-origin")),
        "/tmp".into(),
        None,
    );
    assert!(w.listing.rows.lock().unwrap().is_empty());
    let e = w.listing.error.lock().unwrap().clone();
    assert!(e.is_some(), "没有运行时却没报错 —— 那是静默的空列表");
}

/// `Source::label()` 是窗口标题的来源，别让它回空串。
///
/// ⚠ 从前这条还判一格「本机那一格逐字是『本机』两个中文字」——
/// 本机侧退役之后那一格不存在了（`source.rs` 头注那块墓碑）。
#[test]
fn every_source_has_a_non_empty_label() {
    assert_eq!(Source::remote(synth_cfg("tagged")).label(), "tagged");
    // `label` 为空时回退到 `host`（`RemoteConfig::origin_label` 的契约）。
    let mut anon = synth_cfg("");
    anon.label.clear();
    assert_eq!(Source::remote(anon).label(), "example.invalid");
}

// ════════════════════════════════════════════════════════════════════════
// 第二刀 · ② 那条链：**列它的目录 → 能往下走 → 退得回来**
// ════════════════════════════════════════════════════════════════════════

/// 🔴 **「能往下走、退得回来」这件事有判据了。**
///
/// # ⚠〔2026-09-23〕它换了构造器，**买到的东西缩了一格，如实记**
///
/// 从前这条跑在**本机侧的真目录**上（真 `read_dir`），于是它顺带买到
/// 「进去之后列的是 `sub` 的内容，不是上一层留下的」那一格相等断言。
/// 本机侧退役之后窗口只看远端，而远端列目录**跑不了真的**
/// （本仓红线不许起真连接）⇒ 那一格的**读数买不到了**。
///
/// ⇒ 换成 [`FileWindow::seeded`]（那个构造器正是为「屏幕上先有行」存在的），
/// 本条今天钉的是**换目录这件事本身**：点开一个目录 ⇒ `cwd` 换成那一行的路径；
/// 退一级 ⇒ `cwd` 换回来（`parent_dir` 那条纯函数有自己的判据，住 `source_tests`）。
/// ⚠ 「换了目录上一屏要被清掉」由 `changing_directory_clears_the_previous_rows_...`
/// 单独钉 —— 别读成本条还在判列表内容。
#[test]
fn double_clicking_a_directory_walks_into_it_and_up_walks_back() {
    let mut w = remote_window_with_rows("/srv/data", vec![dir_row("sub"), file_row("f.txt")]);
    assert_eq!(names(&w), vec!["sub", "f.txt"]);

    // `sub` 是第 0 行（目录在前）。点开它。
    assert!(w.activate(0), "点开一个目录却没换目录");
    assert_eq!(w.cwd, "/srv/data/sub");

    // 退回上一级。
    w.navigate_up();
    assert_eq!(w.cwd, "/srv/data");
}

/// 点一个**文件**：什么都不做。
///
/// ⚠ 这不是「还没做完」的占位 —— 文件那一侧（预览/编辑/下载）这一刀明确没做，
/// 而「点了文件把 cwd 换成那个文件的路径」会让下一趟 `read_dir` 报错，
/// 屏幕上出现一条莫名其妙的红字。⇒ 明确的不动。
#[test]
fn clicking_a_file_does_nothing_rather_than_cd_into_it() {
    let mut w = remote_window_with_rows("/srv/data", vec![dir_row("sub"), file_row("f.txt")]);
    let before = w.cwd.clone();
    // 第 1 行是 `f.txt`（目录在前 ⇒ 第 0 行是 `sub`）。
    assert!(!w.activate(1), "点文件竟然换了目录");
    assert_eq!(w.cwd, before);
    assert!(w.listing.error.lock().unwrap().is_none());
    // 越界也不许 panic（列表随时可能刚被刷短）。
    assert!(!w.activate(999));
}

/// 到顶了就停住 —— 不许一路 `..` 走出文件系统。
#[test]
fn walking_up_from_the_top_stays_at_the_top() {
    assert_eq!(parent_dir("/"), "/");
    assert_eq!(parent_dir("/a"), "/");

    let mut w = FileWindow::new(Source::remote(synth_cfg("r")), "/".into(), None);
    w.navigate_up();
    assert_eq!(w.cwd, "/", "从根再往上走，路径变了");
}

/// 🔴 **换目录必须把上一个目录的行清掉。**
///
/// 失效形状是真的：远端 A 慢、B 快 ⇒ 路径栏写着 B、列表里是 A 的文件，
/// 而用户会对着 B 的路径删 A 的东西。
#[test]
fn changing_directory_clears_the_previous_rows_instead_of_leaving_them_up() {
    // 起点是**先播好的一屏** ⇒ 列表非空；再换一个目录（没有运行时 ⇒ 列不出来）。
    // ⚠ 从前这里的起点是本机一棵真目录树（`FileWindow::new(Source::Local, …)`），
    //   本机侧退役之后换成 `seeded` —— 本条要的从来只是「屏幕上先有行」。
    let mut w = remote_window_with_rows("/srv/data", vec![file_row("a.bin")]);
    assert!(!w.listing.rows.lock().unwrap().is_empty());

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
}

/// 🔴 **迟到的那一份不许盖掉新目录的内容。**
///
/// 判 [`store_if_current`] 本体（它正是生产那条路上**唯一**写 `rows` 的地方）：
/// 号对 ⇒ 落盘、回 `true`；号过期 ⇒ 丢掉、回 `false`，而且**一个字节都不改**。
#[test]
fn a_late_answer_from_the_directory_we_left_is_thrown_away() {
    let l = Listing::default();
    let row = |n: &str| {
        Listed::plain(Row {
            name: n.to_string(),
            path: format!("/x/{n}"),
            is_dir: false,
            size: 0,
            lossy_name: false,
        })
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

// ════════════════════════════════════════════════════════════════════════
// 第二刀 · ③ `§5.4d` 在窗口这一侧接得上吗
// ════════════════════════════════════════════════════════════════════════

/// 拖进来的本机路径 → 待传清单：**目标目录就是当前目录**，名字取 basename。
#[test]
fn dropped_paths_become_pending_uploads_into_the_current_directory() {
    let w = FileWindow::new(Source::remote(synth_cfg("r")), "/srv/data".into(), None);
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

/// 接不上就**出声**：远端源 ＋ 没有运行时 ⇒ 不许静默吞掉一摞文件。
#[test]
fn a_drop_with_no_runtime_says_so_instead_of_swallowing_the_files() {
    let mut w = FileWindow::new(Source::remote(synth_cfg("r")), "/srv/data".into(), None);
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
    // ⚠ 本条要的只是「有一个窗口」（它数的是那个消化计数器），
    //   从前拿本机一棵真目录树当窗口；本机侧退役之后换 `seeded`。
    let mut w = remote_window_with_rows("/srv/data", Vec::new());
    assert!(!w.settle_finished_drops(), "一趟都没跑却说要重列");

    w.board.finish(crate::filewin::transfer::DropOutcome {
        asked: 0,
        skipped: 0,
        ok: 1,
        failed: Vec::new(),
    });
    assert!(w.settle_finished_drops(), "跑完一趟却不重列");
    assert!(!w.settle_finished_drops(), "同一趟重列了第二次");
}

// ════════════════════════════════════════════════════════════════════════
// 第三刀 · 零流量复制在窗口这一侧接得上吗
// ════════════════════════════════════════════════════════════════════════

/// 造一个「看着某个远端目录、列表里有几行」的窗口。**不起连接**
/// （`host` 是 `.invalid`，而且这几条一次 `reload` 都不触发远端那一支）。
fn remote_window_with_rows(cwd: &str, rows: Vec<Row>) -> FileWindow {
    let mut w = FileWindow::seeded(Source::remote(synth_cfg("r")), cwd.to_string(), None, rows);
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
    // ⚠ 同 `finishing_a_drop_round_triggers_exactly_one_reload`：换了构造器，判的没变。
    let mut w = remote_window_with_rows("/srv/data", Vec::new());
    assert!(!w.settle_finished_copies(), "一趟都没跑却说要重列");

    w.copy_board
        .finish(crate::filewin::copy::CopyOutcome::Done {
            asked: false,
            verdict: None,
        });
    assert!(w.settle_finished_copies(), "跑完一趟却不重列");
    assert!(!w.settle_finished_copies(), "同一趟重列了第二次");
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
// | `opening_a_window_again_is_a_new_process_and_it_really_comes_up` | 🔴〔第十三刀 09-23〕**第二趟、第三趟开窗都成功**（一趟一个进程，pid 互不相同） | 那正是旧形态的病：第二趟被一个**进程级**标志挡回去，而且此前是静默的 |
//
// ⚠ 前两条各跑一趟自己的子进程；第三条要**三趟**，而第一条用的就是那三趟里的头一趟
// （`scenario_trips` / `scenario_a`）—— 理由是 winit 一个进程只许一个事件循环，
// 一趟子进程只量得到一趟实景开窗 ⇒ 「第二趟开窗」在实景里的量法只能是再起一个进程。
// 🔴 那一条的头注里留着旧那条性质（「同进程第二趟不许是静默成功」）为什么退役。

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
    // 🔴〔2026-09-23 本机侧退役〕**种子从「真目录树」换成「合成的几行」。**
    //    从前这里 `synth_tree()` 造一棵临时目录树、`list_local()` 列一趟当种子。
    //    窗口今天只看远端 ⇒ 那棵树没有对应的一侧了。
    //    ⚠ 这一换**不减读数**：`open_detached_seeded` 从来就不列目录（种子是给它的），
    //      这一格买的是「窗口真的起来了 ＋ 标题 ＋ 几何 ＋ 循环干净退出」。
    let cwd = "/srv/xvfb-open".to_string();
    let rows = vec![dir_row("sub"), file_row("f.txt")];
    xvfb::emit("a.seed_rows", rows.len());

    // 🔴〔第十三刀〕**印出自己的 pid**：「一趟一个进程」那条判据靠它做反空真锚
    //    （三趟的 pid 互不相同 ⇒ 那三份读数真是三个进程各自量的，
    //     不是同一趟输出被读了三遍）。
    xvfb::emit("a.pid", std::process::id());
    let req0 = open_requested();
    let opened0 = windows_opened();
    let h = open_detached_seeded(
        Source::remote(synth_cfg(XVFB_ORIGIN)),
        cwd.clone(),
        None,
        None,
        rows.iter().cloned().map(Into::into).collect(),
        None,
    );

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
        //
        // 🔴 **〔2026-09-21 登记：这一族在满盘跑里飘，现打约 1/4〕**
        //
        // **读数**：满盘 `cargo test -p monitor --lib` 现打 **5 趟红 1 趟**；另一路独立现打
        // **12 趟红 3 趟** ⇒ 汇总 **4 / 17 ≈ 24%**。而**单独**跑 `filewin::shell` 那一组
        // 26/26 绿 —— 两种配置的读数不一样，所以
        // **「单独跑绿」在这一族上不构成「验过了」**（本仓早记过这条，这里是它的又一个实例）。
        //
        // **失效链**（另一路抓的，我没独立复现全链）：硬销毁窗口 ⇒ winit 在
        // `x11/util/geometry.rs` 的 `TranslateCoordinates` 上 panic（`X11Error{ Window, .. }`）
        // ⇒ 那个 panic 毒了一把锁 ⇒ 析构里 `unwrap()` 它 ⇒ **「panic in a destructor
        // during cleanup」⇒ 非 unwind abort ⇒ 实景子进程退出码 `None`** ⇒ 父判据一次带走 2 格。
        //
        // 🔴 **而这一段的第一句话与那条链互相矛盾，我没有读数能判哪句对**：
        // 上面写着 `windowclose` 走的是**协议消息**（优雅），而那条链说它是**硬销毁**。
        // `xdotool windowclose` 的实现是「窗口在 `WM_PROTOCOLS` 里列了 `WM_DELETE_WINDOW`
        // 就发 ClientMessage，否则 `XDestroyWindow`」⇒ **winit 到底有没有列，本仓没量过。**
        // ⇒ 量法写死在这儿，谁接手谁跑：让台架起窗之后 `xprop -id <id> WM_PROTOCOLS`。
        //
        // ⚠ **锁池那一条不是这一条** —— 陈旧锁棘轮（`xvfb_rig.rs::classify_lock` 那一拍）
        // 已修且有读数（上面这 5 趟里锁数每趟都是 **0**）。**这里说的是另一个机制。**
        //
        // 🔴 **它的真危险不是「会红」，是「1/4 的红会被人学会重跑绕过」** ——
        // 一条判据不是靠变哑死的，是靠喊狼死的。⇒ 这条登记要么被修掉，要么被裁成
        // 「本族在满盘里判不了」并把那一格从满盘里摘出去**单独跑**，**不许就这么挂着**。
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

    // ══════════════════════════════════════════════════════════════════
    // 🪦〔墓碑 · 第十三刀 2026-09-23〕**「同一个进程、换一条线程再开一趟」那一段删了。**
    // ══════════════════════════════════════════════════════════════════
    //
    // 原话逐字：「台架头注第四节论证过它必然走另一条路；这里把它**量出来**
    // 而不是推出来」，它印的是 `a.second_window_count` / `a.second_run_native` /
    // `a.second_reason_len` / `a.second_reason` 四条读数，现打的结论是
    // **第二趟裁决 `err`、窗口数 0**。
    //
    // 🔴 **为什么删**：那一段量的是一个**生产里已经不存在的形状**。
    // 用户 2026-09-22 逐字裁「窗口生命周期就是销毁」，而「同进程第二趟必然失败」
    // 这条现打事实使得「关掉就销毁」与「还能再打开」**不可同时成立**
    // ⇒ 开窗改成了「一个窗口一个进程」（住 `crate::filewin::proc`）。
    // 生产那条路上**再也不会**在一个进程里开第二个窗口 ⇒ 继续量它，
    // 量的是台架自己造出来的一个形状，而不是产品的行为。
    //
    // ⚠ **那条现打读数没有被推翻，也没有过期** —— 它今天正是这条裁决的**依据**
    // （逐条写在 `filewin/proc.rs` 头注 §一）。变的不是那个事实，是它还管不管这道题。
    //
    // ⇒ 新的正题是「**第二趟、第三趟开窗都成功**」，它量法只能是**三个进程**：
    // 由 `scenario_trips` 把本工作面在同一台 Xvfb 上跑三趟，判据住
    // `opening_a_window_again_is_a_new_process_and_it_really_comes_up`。
    // 那一条与本段的关系是「**同一件事换了单位**」：从「同进程第二条线程」
    // 换成「第二个进程」。
    //
    // ⚠ 上一版那四条读数里有一条**没有**新住址：「第二趟失败时那句原因非空」。
    // 它在新形态下由 `proc_tests` 那两条买（当场死掉的进程回一句非空的原因），
    // 而**不是**由实景台架买 —— 如实登记，别以为它跟着搬过去了。
    let _ = (cwd, rows);
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
    // 种子换成合成的几行（同 `xvfb_worker_opens_a_real_window` 那条理由）。
    let h = open_detached_seeded(
        Source::remote(synth_cfg(XVFB_ORIGIN)),
        "/srv/xvfb-nodisp".to_string(),
        None,
        None,
        vec![file_row("f.txt").into()],
        None,
    );
    let (verdict, why) = join_verdict(h, 30_000);
    xvfb::emit("n.run_native", verdict);
    xvfb::emit("n.reason_len", why.chars().count());
    xvfb::emit("n.reason", why.replace('\n', " "));
}

/// 实景子进程的读数 —— 🔴〔第十三刀 2026-09-23〕**一趟变三趟。**
///
/// # 为什么是三趟，而且是三个**进程**
///
/// 上一版这里只跑一趟，另用「同一个进程里换一条线程再开一趟」去量第二趟
/// （那一段的墓碑留在工作面里）。今天生产那条路是**一个窗口一个进程**
/// ⇒ 「第二趟开窗」这件事在实景里的量法只能是**再起一个进程**。
/// 而这台架本来就是这么起进程的（`run_scenario` 每趟一个新进程）
/// ⇒ 把同一个工作面在**同一台 Xvfb** 上跑三趟，就是那道题的读数。
///
/// ⚠ **它与生产的差别，如实写清**：生产起的是 `cc-monitor-filewin`
/// （本包第二个 `[[bin]]`），台架里起的是**判据自己这个测试二进制**。
/// 两者跑的是**同一个函数**（`open_detached_seeded`，那也是
/// `filewin::proc::child_main` 唯一调的东西）⇒ 「窗口那一侧」是同一份代码；
/// 不同的是**谁在托管它**。为什么不直接起那份 `[[bin]]`：门禁那一格逐字跑
/// `cargo test --workspace --exclude code-picture-core --lib` ——**`--lib` 不构建 bin**
/// ⇒ 那份二进制在门禁里根本不存在，照它写的判据会在门禁上恒红。
/// ⇒ 「那份 bin 真的托管了窗口进程的躯体」由 `proc_tests` 的源码型判据买，
///   「一趟一个进程、pid 互不相同」由 `proc_tests` 真起进程买（拿一个替身二进制），
///   「第二、第三趟窗口真的摆上屏幕」由本台架买。**三格分开，各自说清买到什么。**
///
/// ⚠ 三趟**顺序跑、共用一台 Xvfb**：并行起三个 egui 进程会让窗口树里同时有三个
/// 命中标题的窗口，而 `wait_for_windows` 回的是**全部**命中 ⇒ 相等断言当场没法写。
#[cfg(not(windows))]
fn scenario_trips() -> &'static [crate::filewin::rows::testing::xvfb::ChildRun; 3] {
    use crate::filewin::rows::testing::xvfb;
    static RUNS: std::sync::OnceLock<[xvfb::ChildRun; 3]> = std::sync::OnceLock::new();
    RUNS.get_or_init(|| {
        // 🔴〔`P25` 2026-09-22〕拿独占闸 —— 逐条理由住 `xvfb::exclusive`。
        let _guard = xvfb::exclusive();
        xvfb::require_toolbox("「点了那颗按钮之后窗口真的起来了」");
        let screen = xvfb::Screen::start()
            .unwrap_or_else(|e| panic!("起不了 Xvfb ⇒ 这一格判不了，不是过了：{e}"));
        let mut go = || {
            xvfb::run_scenario(
                screen.display(),
                "filewin::shell::tests::xvfb_worker_opens_a_real_window",
            )
        };
        [go(), go(), go()]
    })
}

/// **第一趟**那份读数 —— 原有那两条判据（正题 ＋ 字体）按它写的，一个字没动。
#[cfg(not(windows))]
fn scenario_a() -> &'static crate::filewin::rows::testing::xvfb::ChildRun {
    &scenario_trips()[0]
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
    // 🔴 标题里那个串与 worker 那一侧**同一个来源**（同一个常量喂同一个
    //    `Source::label()`）—— 两处各写一份字面量的话，这一比会变成恒真。
    let label = Source::remote(synth_cfg(XVFB_ORIGIN)).label();
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

/// 🔴🔴 **第二趟、第三趟开窗都成功** —— 一趟一个进程。
///
/// ══════════════════════════════════════════════════════════════════════
/// # 🪦 这一格先前买的是**另一条性质**，它为什么退役（原话逐字留在这儿）
/// ══════════════════════════════════════════════════════════════════════
///
/// 上一版这条判据叫「同一个进程里第二次开窗不许是一条静默成功」，头注逐字写着：
///
/// > winit 全进程只许建一个事件循环（那个「已经建过了」的进程级标志只在
/// > web 平台会被清回去），而 eframe 把建好的那个缓存在**线程局部**里。
/// > `open_detached_seeded` 每趟 `std::thread::spawn` 一条**新线程**
/// > ⇒ **同一个进程里第二次开窗，必然失败。**
/// > 现打读数（本判据每趟自己印出来）：第二趟裁决 `err`、窗口数 `0`。
/// > ⇒ **用户把这个文件窗口关掉之后，这个 app 活着的时候再也开不起来了。**
///
/// 而它**刻意不断言「第二趟必须失败」**，原话同样逐字：
///
/// > 它**不**断言「第二趟必须失败」—— 那样就把一个缺陷钉成了期望值，
/// > 哪天有人真把它修好（复用那条线程／那个事件循环）这条判据会反过来拦住修复。
/// > 它断言的是两支**各自自洽**：回 `ok` ⇒ 屏幕上必须真有一个窗口；
/// > 回 `err`／`panic` ⇒ 必须带着一句非空的原因。
///
/// 🔴 **那句「哪天有人真把它修好」就是今天，而修法不是它设想的那一种。**
/// 用户 2026-09-22 逐字裁「**窗口生命周期就是销毁**」。而「同进程第二趟必然失败」
/// 这条**现打事实**使得「关掉就销毁」与「关掉之后还能再打开」**不可同时成立**
/// ⇒ 裁「销毁」之后只剩一条路：**每开一个窗口起一个独立进程**
/// （逐条推理链与它顺带解掉的三条缺陷住 `crate::filewin::proc` 头注）。
///
/// ⇒ 旧那条性质**在新形态下没有被推翻，是失去了指称对象**：生产那条路上
/// 再也不会有「同一个进程里的第二个窗口」。继续钉它，钉的是台架自己造的形状。
/// ⚠ 它那两支里有一支**没有**跟着搬过来：「失败时那句原因非空」。
/// 那一支今天由 `proc_tests` 买（当场死掉的**进程**回一句非空的原因）——
/// 本条不买它，别以为它跟着搬过去了。
///
/// ══════════════════════════════════════════════════════════════════════
/// # 它买到什么（逐条）
/// ══════════════════════════════════════════════════════════════════════
///
/// - **三趟各自把恰好一个窗口摆到屏幕上**（相等断言，`--onlyvisible` 那一档）。
///   🔴 **第二趟与第三趟是这一条的全部价值**：旧形态下它们恒是 0。
/// - **三趟是三个进程**（pid 互不相同）—— 反空真锚：少了它，同一份输出被读三遍
///   也会让上面三条一起绿。
/// - **三趟各自的计数器都是 +1**（`windows_opened_delta`）—— 它说明那三趟
///   **不共享进程级状态**：旧形态里第二趟的这个数也是 1，而窗口数是 0，
///   两个数分叉正是那个进程级标志的指纹；今天两个数在每一趟里都对得上。
///
/// ══════════════════════════════════════════════════════════════════════
/// # ⚠ 它买不到什么（逐条，别读宽）
/// ══════════════════════════════════════════════════════════════════════
///
/// - **台架里托管窗口的是判据自己这个二进制，不是 `cc-monitor-filewin`**
///   （理由住 `scenario_trips`：门禁那一格 `--lib`，不构建 bin）。
///   ⇒ 它买的是「**一个进程一个窗口这件事成立**」，不是「那份发版二进制跑得起来」。
/// - **真 GPU / 字体回落 / DPI / 合成器四样一个都买不到**（台架头注逐条）。
/// - 🔴 **它刻意不要求那三趟「干净退出」。** 那不是偷懒，是一条已登记的抖动：
///   关窗那一下的拆卸竞态（winit 在析构里 panic ⇒ 非 unwind 的 abort）
///   现打在满盘里约 1/4 的趟数上让实景子进程退出码变成 `None`，归因未定，
///   登记在 `P25`。它发生在**读数印出来之后**（`emit` 走 `println!`，行缓冲、
///   每行即刷）⇒ 把「退出码」并进本条只会让本条跟着抖，而本条要买的那件事
///   在读数那一刻已经成立。
///   ⚠ **这不是把抖动压下去** —— 那条竞态本身正是换进程要解的三条缺陷之一
///   （`真相源/107 §2`），它今天只崩台架自己那个进程，而那正是换进程买到的东西。
#[cfg(not(windows))]
#[test]
fn opening_a_window_again_is_a_new_process_and_it_really_comes_up() {
    let trips = scenario_trips();
    let counts: Vec<String> = trips.iter().map(|r| r.reading("a.window_count")).collect();
    let pids: Vec<String> = trips.iter().map(|r| r.reading("a.pid")).collect();
    let opened: Vec<String> = trips
        .iter()
        .map(|r| r.reading("a.windows_opened_delta"))
        .collect();
    let codes: Vec<Option<i32>> = trips.iter().map(|r| r.code).collect();
    println!(
        "  三趟现打：窗口数 {counts:?} · pid {pids:?} · \
         本进程开窗计数 {opened:?} · 退出码 {codes:?}（退出码**不判**，见头注）"
    );

    // ① 反空真锚**排在最前**：三趟真是三个进程。
    //    塌了的话（同一份输出读三遍 / `OnceLock` 只跑了一趟），下面三条会一起假绿。
    let uniq: std::collections::BTreeSet<&String> = pids.iter().collect();
    assert_eq!(
        uniq.len(),
        3,
        "三趟只来自 {} 个不同的进程（pid {pids:?}）—— 下面那几条此刻在空转",
        uniq.len()
    );

    // ② 🔴 正题：**每一趟都恰好一个窗口，第二趟第三趟也是。**
    for (i, c) in counts.iter().enumerate() {
        assert_eq!(
            c,
            "1",
            "第 {} 趟开窗，窗口树里数到 {c} 个标题含 `{WINDOW_NEEDLE}` 的窗口。\n\
             0 = 那一趟**没起来**。🔴 第二／第三趟是 0 就说明「一趟一个进程」没成立：\
             那正是旧形态的病（winit 的进程级事件循环标志把第二趟挡回去）。\n\
             三趟读数：{counts:?}",
            i + 1
        );
    }

    // ③ 每一趟自己那个计数器都是 +1 —— 三趟不共享进程级状态。
    for (i, o) in opened.iter().enumerate() {
        assert_eq!(
            o,
            "1",
            "第 {} 趟里 `windows_opened` 只涨了 {o} —— 那个计数器是**进程级**的，\
             每个窗口进程里都该恰好涨一次。三趟读数：{opened:?}",
            i + 1
        );
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

// ════════════════════════════════════════════════════════════════════════
// 🔴〔第五刀 2026-09-21〕`设计/99 §4.6.4`：那四条写操作接在这一侧
// ════════════════════════════════════════════════════════════════════════

/// 一行**目录**（第五刀起目录也能改名/删除/改权限，只是不能复制）。
///
/// ⚠〔2026-09-23〕导航那两条判据现在也吃它：本机侧退役之前它们走的是
/// 本机一棵真目录树（那棵树自带一个真目录），今天走 `seeded` ＋ 这一行。
fn dir_row(name: &str) -> Row {
    Row {
        name: name.to_string(),
        path: format!("/srv/data/{name}"),
        is_dir: true,
        size: 0,
        lossy_name: false,
    }
}

fn lossy_row() -> Row {
    Row {
        name: "\u{FFFD}odd".into(),
        path: "/srv/data/\u{FFFD}odd".into(),
        is_dir: false,
        size: 1,
        lossy_name: true,
    }
}

/// 🔴 **胶水三跳有判据了**：列表说「第 i 行的改名 / 权限 / 删除被点了」→ 窗口接上去。
///
/// 与 `a_copy_click_from_the_list_puts_up_the_rename_box_for_that_row` 逐字同一个理由：
/// 两头各自都有判据，而这三跳写在 `frame_body` 里的话**谁都没在看**。
#[test]
fn a_write_click_from_the_list_reaches_the_right_row() {
    let mut w = remote_window_with_rows("/srv/data", vec![file_row("a.bin"), dir_row("sub")]);
    assert!(w.write_prompt().is_none(), "什么都没点就摆出了框");
    assert!(!w.apply_write_clicks(None), "没人点却说接上了一跳");

    // 改名：点第 1 行（那是个**目录** —— 目录也能改名）。
    w.tally.rename_clicked = Some(1);
    assert!(
        w.apply_write_clicks(None),
        "第 1 行的改名被点了，框却没摆出来"
    );
    let p = w.write_prompt().expect("框不见了").clone();
    assert_eq!(p.src_name, "sub");
    assert_eq!(p.dir, "/srv/data");
    assert_eq!(p.text, "sub", "改名那个框没预填原名");
    assert_eq!(
        p.to_op().unwrap_err().is_empty(),
        false,
        "预填原名之后直接确定应当被拒（没有要改的东西）"
    );

    // 权限：点第 0 行。
    w.tally = crate::filewin::rows::RenderTally::default();
    w.cancel_write();
    w.tally.chmod_clicked = Some(0);
    assert!(w.apply_write_clicks(None));
    let p = w.write_prompt().expect("框不见了").clone();
    assert_eq!(p.src_name, "a.bin");
    assert_eq!(p.text, "", "权限那个框预填了东西 —— 那必然是猜的");
}

/// 🔴 **删除不经那个框** —— 它直接起一摞，确认那一步归「一次问完」。
///
/// 这个窗口没有运行时 ⇒ 它起不来，而它必须**出声**（不许静默吞掉一次删除）。
#[test]
fn a_delete_click_goes_straight_to_the_batch_and_says_so_when_it_cannot_run() {
    let mut w = remote_window_with_rows("/srv/data", vec![file_row("a.bin")]);
    w.tally.delete_clicked = Some(0);
    assert!(!w.apply_write_clicks(None), "没有运行时却说起得来");
    assert!(
        w.write_prompt().is_none(),
        "删除竟然摆出了一个「叫什么名字」的框"
    );
    let e = w
        .listing
        .error
        .lock()
        .unwrap()
        .clone()
        .expect("一次删除被吞了，屏幕上一句话都没有");
    assert!(e.contains("运行时"), "报的不是「没有运行时」：{e}");
}

/// 有损名 · 越界下标 —— **两档都不接**（第二道闸，防「按钮没了、调用还在」）。
#[test]
fn a_lossy_name_or_an_out_of_range_row_is_refused_by_the_second_gate() {
    let mut w = remote_window_with_rows("/srv/data", vec![lossy_row()]);
    for i in [0usize, 99] {
        assert!(!w.begin_rename(i), "第 {i} 行竟然摆出了改名框");
        assert!(!w.begin_chmod(i), "第 {i} 行竟然摆出了权限框");
        assert!(!w.begin_delete(i, None), "第 {i} 行竟然起了一摞删除");
    }
    assert!(w.write_prompt().is_none());
}

/// 🔴 输入不合法 ⇒ **框留着 ＋ 出声**，不静默收掉（同 `confirm_copy` 那一条）。
#[test]
fn an_impossible_input_keeps_the_write_box_up_and_says_why() {
    let mut w = remote_window_with_rows("/srv/data", vec![file_row("a.bin")]);
    assert!(w.begin_mkdir());
    for bad in ["", "   ", "sub/x", "..", "."] {
        *w.listing.error.lock().unwrap() = None;
        w.write_prompt.as_mut().unwrap().text = bad.to_string();
        assert!(!w.confirm_write(None), "「{bad}」这个名字竟然起得来");
        assert!(
            w.write_prompt().is_some(),
            "「{bad}」被拒了，框却收掉了 —— 用户会以为它做了"
        );
        assert!(
            w.listing.error.lock().unwrap().is_some(),
            "「{bad}」被拒了却一句话都没说"
        );
    }
    // 反空真：换一个能用的名字，它就不再卡在「名字不合法」这一支上
    //（这个窗口没有运行时 ⇒ 它卡在下一支，而那一支说的是另一件事）。
    *w.listing.error.lock().unwrap() = None;
    w.write_prompt.as_mut().unwrap().text = "newdir".to_string();
    assert!(!w.confirm_write(None), "没有运行时却说起得来");
    let e = w.listing.error.lock().unwrap().clone().unwrap();
    assert!(
        e.contains("运行时"),
        "合法名字被当成不合法挡了：{e} —— 那上面那几条买的就不是「输入」这一维"
    );
    w.cancel_write();
    assert!(w.write_prompt().is_none());
}

/// 一摞写操作跑完要重列目录（新目录要出现、删掉的要消失），而且**只重列一次**。
#[test]
fn finishing_a_write_round_triggers_exactly_one_reload() {
    // ⚠ 同上那两条：换了构造器，判的没变。
    let mut w = remote_window_with_rows("/srv/data", Vec::new());
    assert!(!w.settle_finished_writes(), "一摞都没跑却说要重列");
    w.write_board
        .finish(crate::filewin::writeops::WriteOutcome::default());
    assert!(w.settle_finished_writes(), "跑完一摞却不重列");
    assert!(!w.settle_finished_writes(), "同一摞重列了第二次");
}

/// 🔴 **被围栏挡住那句话真的被画在窗口上**（不是 `tracing`）。
///
/// 判据从 egui 这一帧真的交出去的 galley 里把那句话读回来 ——
/// 一条 `assert!(src.contains("colored_label"))` 在那一行被 `if false` 包住时照样绿
/// （量具与它买不到什么住 `copy::testing`）。
///
/// ⚠ 这一条是完成判据「围栏那一条要有阴性对照 …… 而且窗口上**要出声**」的那半。
#[test]
fn the_fence_line_really_gets_painted_on_the_window() {
    // ⚠ 换了构造器（本机侧退役）。本条要的只是「有一个窗口能画一帧」。
    let mut w = remote_window_with_rows("/srv/data", Vec::new());
    let ctx = egui::Context::default();
    let blocked = crate::filewin::writeops::fence_notice(
        &crate::filewin::writeops::WriteOp::Delete {
            path: "/home/u/.claude/projects/p/s.jsonl".into(),
            is_dir: false,
        },
        "/home/u/.claude/projects/p/s.jsonl",
    );
    // 先跑一帧把字体图集建起来（同 `rows_tests` 那条口径）。
    let _ = crate::filewin::find::testing::frame_text(&ctx, &mut w, Vec::new());
    w.write_board
        .finish(crate::filewin::writeops::WriteOutcome {
            blocked: vec![blocked.clone()],
            ..Default::default()
        });
    let painted = crate::filewin::find::testing::frame_text(&ctx, &mut w, Vec::new());
    assert!(
        painted.iter().any(|t| t == &blocked),
        "这一帧上没有被挡那句话。画出来的是：{painted:?}"
    );
    // 反空真：这把尺子不是「凡什么话都说画出来了」。
    assert!(
        !painted.iter().any(|t| t.contains("这句话根本没人画过它")),
        "量具在乱认"
    );
}

/// 🔴 **命中那一摞交不出任何一个下标** —— 那三条写胶水索引的是另一摞东西。
///
/// # 它接的是 `rows::show_hit_rows` 头注那条纪律
///
/// 第四刀靠的是「那个函数不画可点控件」；第五刀行上多了三颗**写**按钮
/// ⇒ 代价从「复制到错的地方」升级成「**删错东西**」，于是换成了**类型**
/// （命中那一摞的收数口是 `HitTally`，里面没有「谁被点了」这个字段）。
///
/// 本条真跑一帧**生产那个** `frame_body`（搜索框里有字 ⇒ 走命中那一支），
/// 断言两件事：① 命中真的画出来了（行数 > 0，不是空转）；
/// ② `tally` 逐字节等于默认值 ⇒ 那三条胶水这一帧接不到任何东西。
#[test]
fn the_hit_list_can_never_hand_the_window_a_row_index() {
    let ctx = egui::Context::default();
    let mut w = remote_window_with_rows("/srv/data", vec![file_row("a.bin"), dir_row("sub")]);
    // 先在搜索框里打一段字 ⇒ `showing_hits()` 为真。
    crate::filewin::find::testing::type_into_search(&ctx, &mut w, "bin");
    assert!(w.showing_hits(), "搜索框里没字，下面判的就是目录列表那一支");
    let _ = crate::filewin::find::testing::frame_text(&ctx, &mut w, Vec::new());
    // 命中那一摞这一帧画了几行（喂一份合成命中）。
    let hits: Vec<String> = (0..5).map(|i| format!("/deep/dir/h{i}.bin")).collect();
    let mut t = crate::filewin::rows::HitTally::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 800.0),
        )),
        ..Default::default()
    };
    let out = ctx.run_ui(input, |ui| {
        crate::filewin::rows::show_hit_rows(ui, &hits, &mut t)
    });
    out.drop_without_applying_deltas();
    assert_eq!(t.rows_materialized, 5, "命中一行都没画 —— 下面那一比在空转");
    // ⇒ 而这一趟**一个下标都没交出来**：`HitTally` 里压根没有那种字段。
    //   与目录列表那一支对照（那一支交得出来）—— 那正是这一条要分开的两件事。
    let mut rt = crate::filewin::rows::RenderTally::default();
    let rows = vec![Listed::plain(file_row("a.bin"))];
    let out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        },
        |ui| crate::filewin::rows::show_file_rows(ui, &rows, &mut rt, Some(0.0), None),
    );
    out.drop_without_applying_deltas();
    assert_eq!(rt.rows_materialized, 1);
    // 🔴 生产那一帧走命中那一支时，`tally` 逐字节就是默认值。
    assert_eq!(
        w.tally,
        crate::filewin::rows::RenderTally::default(),
        "走命中那一支的那一帧，`tally` 里竟然有东西 —— 那三条写胶水就会拿那个下标\
         去索引 `listing.rows`（另一摞东西）"
    );
    assert!(
        w.hits_tally.rows_materialized > 0 || w.hits_tally.total_rows == 0,
        "命中那一摞的收数口没接上"
    );
}

/// 起一摞真的走 `writeops::run_writes`，而**不是**在窗口里另写一套确认流。
///
/// ⚠ 判源码是代理（同族先例：`transfer_tests::the_real_adapters_delegate_to_the_shared_pool`）。
/// 买的是：多选长出来那天，它自动落在「一次问完」那条路上。
#[test]
fn the_window_starts_a_batch_through_the_shared_three_step_function() {
    let prod =
        guard_core::production_code(include_str!("../../../src/bridge/src/filewin/shell.rs"));
    assert_eq!(
        prod.matches("writeops::run_writes(").count(),
        1,
        "`writeops::run_writes(` 在 `shell.rs` 生产段里不是恰好一处 —— \
         多了就是长出了第二条确认流，少了就是这一条被绕过了"
    );
    assert_eq!(
        prod.matches("writeops::apply_remote(").count(),
        1,
        "做一件的落点不是恰好一处"
    );
    // 🔴 窗口自己**不许**直接调那四条池命令 —— 它们只许经 `apply_remote` 走。
    for banned in [
        "sftp_pool::sftp_mkdir",
        "sftp_pool::sftp_delete",
        "sftp_pool::sftp_rename",
        "sftp_pool::sftp_chmod",
    ] {
        assert!(
            !prod.contains(banned),
            "`shell.rs` 直接调了 `{banned}` —— 那条路绕开了 `run_writes` 的围栏与「一次问完」"
        );
    }
}

/// 🔴🔴 **整条链一趟走完**：真点一下行上那颗「删除」→ 围栏挡住 → 屏幕上有话。
///
/// # 它是这一摞里唯一一条**不跳任何一跳**的判据
///
/// 别的几条各钉一段：`rows_tests` 钉「点得到」、`shell_tests` 钉那三条胶水、
/// `writeops_tests` 钉三段的顺序。**而「它们真的串在一起」此前谁都没在看** ——
/// 那正是本仓那条「判据不在执行链上就等于不存在」（波 β 现打：`frame_body`
/// 被剥出来之前，这个窗口每一帧真正画的那段代码一条判据都没有）。
///
/// 本条走的是生产那一条：
/// `frame_body` → `show_file_rows`（真合成事件）→ `RenderTally::delete_clicked`
/// → `apply_write_clicks` → `begin_delete` → `start_writes` → `run_writes`
/// → 围栏 → `WriteBoard::finish`。
///
/// # 🔴 为什么它不需要一条连接（而仍然是真读数）
///
/// 喂的那一行是一条**受保护路径**（`projects/<proj>/<sid>.jsonl`）⇒ `run_writes`
/// 的第一段在本地就把它挡了，`apply` 一次都不被调 ⇒ **一个 packet 都不发**。
/// ⇒ 这一条同时是完成判据里「对一个受保护路径发删除 ⇒ 必须被挡」的**端到端**那一版。
///
/// ⚠ 买不到：真机上鼠标点得到（本机无图形会话，喂的是合成事件）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_real_click_on_delete_walks_the_whole_chain_and_the_fence_stops_it() {
    use crate::filewin::writeops::{DELETE_LABEL, FENCE_PREFIX};
    let jsonl = "/home/u/.claude/projects/dash-proj/abc-123.jsonl";
    let mut w = FileWindow::seeded(
        Source::remote(synth_cfg("e2e-fence")),
        "/home/u/.claude/projects/dash-proj".to_string(),
        tokio::runtime::Handle::try_current().ok(),
        vec![Row {
            name: "abc-123.jsonl".into(),
            path: jsonl.into(),
            is_dir: false,
            size: 12,
            lossy_name: false,
        }],
    );
    assert!(w.rt.is_some(), "这一条要一个运行时，否则它卡在另一支上");
    // 〔F2〕写面走通道 ⇒ 挂一台合成后端（它声明了 `files-delete`）。本条要证的是
    //   **本地那道预判把它挡在上线之前** ⇒ 最后断这台后端一行都没收到。
    let wired = crate::filewin::find::testing::wire_up(
        "e2e-fence",
        crate::filewin::find::testing::FakeBackend::new(
            &["files-delete"],
            crate::filewin::find::testing::Declared::default(),
        ),
    )
    .await;
    w.attach_line(wired.line.clone());
    let ctx = egui::Context::default();

    // 第一帧：建字体图集 ＋ 让上一帧的 widget 表有内容（命中测试按上一帧做）。
    let _ = crate::filewin::find::testing::frame_text(&ctx, &mut w, Vec::new());
    let painted = crate::filewin::copy::testing::painted_text(
        &ctx,
        egui::vec2(1280.0, 800.0),
        0.1,
        Vec::new(),
        |ui| w.frame_body(ui),
    );
    let buttons = crate::filewin::copy::testing::rects_of(&painted, DELETE_LABEL);
    assert_eq!(
        buttons.len(),
        1,
        "这一帧上没有那颗「{DELETE_LABEL}」—— 行上那三颗写按钮没画出来，\
         或者 `frame_body` 走的是命中那一支"
    );
    let pos = buttons[0].center();

    // 第二帧：移到按钮上；第三帧：真点下去 ⇒ 整条链在这一帧里跑完前半。
    let _ = crate::filewin::copy::testing::painted_text(
        &ctx,
        egui::vec2(1280.0, 800.0),
        0.2,
        vec![egui::Event::PointerMoved(pos)],
        |ui| w.frame_body(ui),
    );
    let _ = crate::filewin::copy::testing::painted_text(
        &ctx,
        egui::vec2(1280.0, 800.0),
        0.3,
        crate::filewin::rows::testing::click_at(pos),
        |ui| w.frame_body(ui),
    );

    // 那一摞是异步跑的 ⇒ 等它落地（**不靠睡一个猜出来的时长**：等那个可观测的数）。
    for _ in 0..200 {
        if w.write_board.rounds() > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let out = w.write_board.last().expect(
        "点了「删除」，那一摞一趟都没跑完 —— 胶水那一跳断了（`apply_write_clicks` 没接上），\
                 或者 `frame_body` 没调它",
    );
    assert_eq!(
        out.blocked.len(),
        1,
        "受保护路径上那一件没被挡，实得 {out:?} —— \
         围栏那一段要么被绕过了，要么它没看这条路径"
    );
    assert!(
        out.blocked[0].starts_with(FENCE_PREFIX) && out.blocked[0].contains(jsonl),
        "被挡那句话不对：{}",
        out.blocked[0]
    );
    // 🔴 一个字节都没动过对面的盘：`apply` 一次都没被调 ⇒ 既没成功也没失败。
    assert_eq!(out.ok, 0);
    assert!(out.failed.is_empty(), "实得 {:?}", out.failed);
    // 而且**没问过人**：那一问会教用户「这是可以删的」，答完了它照样做不了。
    assert_eq!(out.asked, 0, "受保护的那一件被摆到人面前问了");
    assert!(!w.write_board.is_asking());
    // 〔F2〕线上一行都没有 —— 挡在上线之前，不是后端替它挡的。
    assert!(
        wired.cmds().is_empty(),
        "受保护那一件上了线：{:?}",
        wired.cmds()
    );
}

// ════════════════════════════════════════════════════════════════════════
// 🔴〔第六刀 2026-09-21 · **整摞退役 2026-09-23**〕「本机」那颗按钮
// ════════════════════════════════════════════════════════════════════════
//
// 这里原来有 3 条判据，钉的是「点一下『本机』还回得来」那条往返
// （以及它当初修掉的那扇单向门）。2026-09-23 用户裁掉了整个本机侧
// ⇒ 3 条随功能一起走了。存在过什么 · 谁裁的 · 那条白名单原文，
// **完整记述只有一份**，住 `src/bridge/src/filewin/source.rs` 的头注（那块墓碑）。
//
// ⚠ **随它们一起走掉的检出力，如实点名**（三条各自独占的那一刀）：
// ⑤「没有来处也报回去成功了」· ③「按钮恒画 ⇒ 画了点了没反应」·
// ④「只摘掉界面那颗按钮、函数留着」。
// 这三刀今天**打不到任何东西** —— 靶子（那一对函数与那颗按钮）不在了，
// 不是「判据少了三条、缺陷还在」。
//
// ⚠ 而**方法**留下来了，并且本刀又用了一遍：一条判据挣不挣得到它的位子，
// 看的是「有没有只有它才红的那一刀」；而一条判据该不该删，看的是
// **它的靶子还在不在** —— 两个问题不一样，混起来就会把真判据当兼容债删掉。
//
// ════════════════════════════════════════════════════════════════════════
// 🔴〔第八刀 2026-09-22〕往外拖 —— 行上那颗「下载」到窗口那两问
// ════════════════════════════════════════════════════════════════════════

/// 🔴🔴 **整条链一趟走完**：真点一下行上那颗「下载」→ 第一问摆出来了。
///
/// # 它与 `download_tests` 那 11 条各买什么（别读重）
///
/// 那 11 条判的是 `download.rs` 里的**纯逻辑**（落点怎么算、三支裁决、那一格状态）。
/// 本条判的是**它们真的被接上了**：`frame_body` → `show_file_rows` →
/// `RenderTally::download_clicked` → `apply_pull_click` → `begin_pull`，五跳一跳不跳。
/// 本仓那条「判据不在执行链上就等于不存在」在本会话里已经抓到过两次同一形。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_real_click_on_download_opens_the_destination_question() {
    use crate::filewin::download::{Ask, DOWNLOAD_LABEL};
    let mut w = FileWindow::seeded(
        Source::remote(synth_cfg("pull-e2e")),
        "/srv/data".to_string(),
        tokio::runtime::Handle::try_current().ok(),
        vec![Row {
            name: "报表.csv".into(),
            path: "/srv/data/报表.csv".into(),
            is_dir: false,
            size: 4096,
            lossy_name: false,
        }],
    );
    assert!(w.pull_ask().is_none(), "什么都没点就摆出了框");
    let ctx = egui::Context::default();

    // 第一帧：建字体图集 ＋ 让上一帧的 widget 表有内容（命中测试按上一帧做）。
    let _ = crate::filewin::find::testing::frame_text(&ctx, &mut w, Vec::new());
    let painted = crate::filewin::copy::testing::painted_text(
        &ctx,
        egui::vec2(1280.0, 800.0),
        0.1,
        Vec::new(),
        |ui| w.frame_body(ui),
    );
    let buttons = crate::filewin::copy::testing::rects_of(&painted, DOWNLOAD_LABEL);
    assert_eq!(
        buttons.len(),
        1,
        "这一帧上没有那颗「{DOWNLOAD_LABEL}」（实得 {} 处）—— \
         行上那颗按钮没画出来，或者 `frame_body` 走的是命中那一支",
        buttons.len()
    );
    let pos = buttons[0].center();

    // 第二帧：移上去；第三帧：真按下去。
    let _ = crate::filewin::copy::testing::painted_text(
        &ctx,
        egui::vec2(1280.0, 800.0),
        0.2,
        vec![egui::Event::PointerMoved(pos)],
        |ui| w.frame_body(ui),
    );
    let _ = crate::filewin::copy::testing::painted_text(
        &ctx,
        egui::vec2(1280.0, 800.0),
        0.3,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            },
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            },
        ],
        |ui| w.frame_body(ui),
    );

    match w.pull_ask() {
        Some(Ask::Dest {
            src_path,
            src_name,
            text,
            ..
        }) => {
            assert_eq!(src_path, "/srv/data/报表.csv", "问的不是被点那一行");
            assert_eq!(src_name, "报表.csv");
            assert!(text.ends_with("/报表.csv"), "缺省落点没带上原名：{text}");
        }
        other => panic!("真点了「下载」，第一问却没摆出来：{other:?}"),
    }
    // 🔴 **一个字节都没动**：还在问，传输一趟都没起。
    assert_eq!(w.pull.rounds(), 0, "还在问，传输就起来了");
    assert!(w.pull.in_flight().is_none());
}

/// 落点已经有东西 ⇒ 窗口**换到第二问**，而不是直接起传输。
///
/// ⚠ 本条走 `confirm_pull` 那条真路（它内部调的是生产那个 `dest_exists`，
/// 真碰盘）⇒ 夹具在临时目录里摆一个**真文件**。
/// 那是刻意的：注入式的那一半已经由 `download_tests` 判过，
/// 本条要的正是「生产那条路真的会去看盘」。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn confirming_onto_an_existing_file_switches_to_the_overwrite_question() {
    use crate::filewin::download::Ask;
    let (root, _) = synth_tree("pull-ow");
    let occupied = root.join("f.txt"); // `synth_tree` 造的时候就写了内容
    assert!(
        occupied.exists(),
        "夹具没造出那个文件 —— 本条此刻在量别的东西"
    );

    let mut w = FileWindow::seeded(
        Source::remote(synth_cfg("pull-ow")),
        "/srv/data".to_string(),
        tokio::runtime::Handle::try_current().ok(),
        vec![Row {
            name: "f.txt".into(),
            path: "/srv/data/f.txt".into(),
            is_dir: false,
            size: 3,
            lossy_name: false,
        }],
    );
    assert!(w.begin_pull(0), "第一问没摆出来");
    // 把落点改成那个**真的存在**的路径。
    // 🔴 走的是生产那个访问器（`pull_dest_mut`）—— 界面上 `text_edit_singleline`
    //    拿的是同一个 `&mut`，所以本条改的那几个字正是用户敲进去的那几个字。
    *w.pull_dest_mut().expect("现在问的不是落点") = occupied.to_string_lossy().to_string();

    assert!(w.confirm_pull(None), "答完第一问却什么都没推进");
    match w.pull_ask() {
        Some(Ask::Overwrite { dest, src_name, .. }) => {
            assert_eq!(dest, &occupied.to_string_lossy().to_string());
            assert_eq!(src_name, "f.txt");
        }
        other => panic!("落点上有东西，却没换到第二问：{other:?}"),
    }
    // 🔴 **还没动手**：第二问摆着，传输一趟都没起。
    assert_eq!(w.pull.rounds(), 0, "还在问要不要盖，就已经开始拉了");
    assert!(w.pull.in_flight().is_none());

    // 取消 ⇒ 框收掉，**仍然一趟都没起**（「取消」不许等于「做」）。
    w.cancel_pull();
    assert!(w.pull_ask().is_none());
    assert_eq!(w.pull.rounds(), 0);
    let _ = std::fs::remove_dir_all(&root);
}

/// 🔴 目录与有损名那两行上**一颗「下载」都不画**。
///
/// # 它补的洞是量出来的（刀③）
///
/// 死值验：把 `rows.rs` 里那道 `is_downloadable` 闸拆掉（对每一行都画）
/// ⇒ **一条判据都不红**。而后果是一颗**死按钮**：点它 `begin_pull` 会再判一次
/// 然后什么都不做 ⇒ 屏幕上「点了没反应」，与「这个功能坏了」同形。
///
/// ⚠ `download_tests::only_a_plain_addressable_file_can_be_pulled` 判的是**那个谓词**，
/// 买不到「画不画」—— 两件事差着一跳，而那一跳正是这条要钉的。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_download_button_is_painted_on_rows_that_cannot_be_pulled() {
    use crate::filewin::download::DOWNLOAD_LABEL;
    let mut w = FileWindow::seeded(
        Source::remote(synth_cfg("pull-gate")),
        "/srv/data".to_string(),
        tokio::runtime::Handle::try_current().ok(),
        vec![
            Row {
                name: "sub".into(),
                path: "/srv/data/sub".into(),
                is_dir: true,
                size: 0,
                lossy_name: false,
            },
            Row {
                name: "bad\u{FFFD}name".into(),
                path: "/srv/data/bad\u{FFFD}name".into(),
                is_dir: false,
                size: 10,
                lossy_name: true,
            },
        ],
    );
    let ctx = egui::Context::default();
    let _ = crate::filewin::find::testing::frame_text(&ctx, &mut w, Vec::new());
    let painted = crate::filewin::copy::testing::painted_text(
        &ctx,
        egui::vec2(1280.0, 800.0),
        0.1,
        Vec::new(),
        |ui| w.frame_body(ui),
    );
    // 反空真：这一帧**真的**画了那两行（否则下面那一比是空真的）。
    assert!(
        crate::filewin::copy::testing::painted_contains(&painted, "sub"),
        "这一帧连那两行都没画出来 —— 本条此刻是空真的"
    );
    assert_eq!(
        crate::filewin::copy::testing::rects_of(&painted, DOWNLOAD_LABEL).len(),
        0,
        "目录 / 有损名那两行上画出了「{DOWNLOAD_LABEL}」—— 那是一颗死按钮：\
         点它 `begin_pull` 会再判一次然后什么都不做，屏幕上「点了没反应」"
    );

    // 🔴 阴性对照：**能拉的那一行上它必须画出来** ——
    //    少了这一半，上面那一比可以靠「哪一行都不画」全绿（那时这个功能根本不存在）。
    let mut w2 = FileWindow::seeded(
        Source::remote(synth_cfg("pull-gate-ok")),
        "/srv/data".to_string(),
        tokio::runtime::Handle::try_current().ok(),
        vec![Row {
            name: "ok.txt".into(),
            path: "/srv/data/ok.txt".into(),
            is_dir: false,
            size: 10,
            lossy_name: false,
        }],
    );
    let ctx2 = egui::Context::default();
    let _ = crate::filewin::find::testing::frame_text(&ctx2, &mut w2, Vec::new());
    let p2 = crate::filewin::copy::testing::painted_text(
        &ctx2,
        egui::vec2(1280.0, 800.0),
        0.1,
        Vec::new(),
        |ui| w2.frame_body(ui),
    );
    assert_eq!(
        crate::filewin::copy::testing::rects_of(&p2, DOWNLOAD_LABEL).len(),
        1,
        "能拉的那一行上没画「{DOWNLOAD_LABEL}」"
    );
}

// ════════════════════════════════════════════════════════════════════════
// 🔴〔第九刀 2026-09-22〕改一份远端文本 —— 行上那颗「编辑」到编辑面
// ════════════════════════════════════════════════════════════════════════

/// 🔴🔴 **整条链一趟走完**：真点一下行上那颗「编辑」→ 那趟读真的发出去了。
///
/// 五跳：`frame_body` → `show_file_rows` → `RenderTally::edit_clicked` →
/// `apply_edit_click` → `begin_edit`。⚠ 读本身连不上（`.invalid`），
/// 本条买的是「那一趟**发出去了**」（`edits.opening()` 有值），不是「读到了」。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_real_click_on_edit_fires_the_read() {
    use crate::filewin::editor::EDIT_LABEL;
    let mut w = FileWindow::seeded(
        Source::remote(synth_cfg("edit-e2e")),
        "/srv/data".to_string(),
        tokio::runtime::Handle::try_current().ok(),
        vec![Row {
            name: "app.conf".into(),
            path: "/srv/data/app.conf".into(),
            is_dir: false,
            size: 2048,
            lossy_name: false,
        }],
    );
    assert!(w.editing().is_none(), "什么都没点就有编辑面了");
    assert_eq!(w.edits.opens(), 0);
    let ctx = egui::Context::default();
    let _ = crate::filewin::find::testing::frame_text(&ctx, &mut w, Vec::new());
    let painted = crate::filewin::copy::testing::painted_text(
        &ctx,
        egui::vec2(1600.0, 800.0),
        0.1,
        Vec::new(),
        |ui| w.frame_body(ui),
    );
    let buttons = crate::filewin::copy::testing::rects_of(&painted, EDIT_LABEL);
    assert_eq!(
        buttons.len(),
        1,
        "这一帧上没有那颗「{EDIT_LABEL}」（实得 {} 处）",
        buttons.len()
    );
    let pos = buttons[0].center();
    let _ = crate::filewin::copy::testing::painted_text(
        &ctx,
        egui::vec2(1600.0, 800.0),
        0.2,
        vec![egui::Event::PointerMoved(pos)],
        |ui| w.frame_body(ui),
    );
    let _ = crate::filewin::copy::testing::painted_text(
        &ctx,
        egui::vec2(1600.0, 800.0),
        0.3,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            },
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            },
        ],
        |ui| w.frame_body(ui),
    );
    // 那一趟**发出去了** —— 要么还在飞，要么已经到货（`.invalid` 解析很快就失败）。
    for _ in 0..200 {
        if w.edits.opens() > 0 || w.edits.opening().is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(
        w.edits.opens() > 0 || w.edits.opening().is_some(),
        "真点了「{EDIT_LABEL}」，那趟读一次都没发出去 —— 胶水那一跳断了"
    );
}

/// 🔴 **太大的那一行：一颗按钮都不画，而点这一行也不会发往返 —— 但会出声。**
///
/// 这是 `设计/60 §5.4b` 那一问（「超了怎么办」）在窗口上的落点判据。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_oversized_row_never_asks_the_remote_and_still_says_why() {
    use crate::filewin::editor::EDIT_LABEL;
    let big = crate::sftp_pool::MAX_EDIT_BYTES as u64 + 1;
    let mut w = FileWindow::seeded(
        Source::remote(synth_cfg("edit-big")),
        "/srv/data".to_string(),
        tokio::runtime::Handle::try_current().ok(),
        vec![Row {
            name: "huge.log".into(),
            path: "/srv/data/huge.log".into(),
            is_dir: false,
            size: big,
            lossy_name: false,
        }],
    );
    let ctx = egui::Context::default();
    let _ = crate::filewin::find::testing::frame_text(&ctx, &mut w, Vec::new());
    let painted = crate::filewin::copy::testing::painted_text(
        &ctx,
        egui::vec2(1600.0, 800.0),
        0.1,
        Vec::new(),
        |ui| w.frame_body(ui),
    );
    // 反空真：这一帧真的画了那一行。
    assert!(
        crate::filewin::copy::testing::painted_contains(&painted, "huge.log"),
        "这一帧连那一行都没画 —— 本条此刻是空真的"
    );
    assert_eq!(
        crate::filewin::copy::testing::rects_of(&painted, EDIT_LABEL).len(),
        0,
        "超上限那一行上画出了「{EDIT_LABEL}」—— 那是一颗死按钮"
    );

    // 而**直接调那条路**（多选长出来那天会走到）也要：不发往返 ＋ 出声。
    assert!(!w.begin_edit(0, None), "超上限那一行竟然发了那趟读");
    assert_eq!(w.edits.opens(), 0, "一趟都不该发");
    assert!(w.edits.opening().is_none());
    let e = w
        .listing
        .error
        .lock()
        .unwrap()
        .clone()
        .expect("拒了却一个字都没说 —— 那与「点了没反应」同形");
    assert!(e.contains("多了 1 字节"), "那句话没说超出多少：{e}");
    assert!(e.contains("huge.log"), "没说是哪一行：{e}");
}

/// 🔴 **改了没存就关 ⇒ 先问。** 这一刀的「不静默丢弃」。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_a_dirty_pane_asks_before_throwing_the_typing_away() {
    let mut w = FileWindow::seeded(
        Source::remote(synth_cfg("edit-dirty")),
        "/srv/data".to_string(),
        tokio::runtime::Handle::try_current().ok(),
        Vec::<Row>::new(),
    );
    // 直接把编辑面立起来（读那一跳由上面那条判据钉）。
    w.edits.deliver(crate::filewin::editor::Arrived::Text {
        path: "/srv/data/app.conf".into(),
        name: "app.conf".into(),
        text: "a=1\n".into(),
    });
    assert!(w.settle_opened_edits(), "到货了却没立起编辑面");
    assert!(w.editing().is_some());
    assert!(!w.editing().unwrap().dirty());

    // 没改过 ⇒ 直接关得掉。
    assert!(w.close_edit(), "没改过却关不掉");
    assert!(w.editing().is_none());
    assert!(!w.asking_discard());

    // 再开一次、改一改 ⇒ 关不掉，那一问摆出来。
    w.edits.deliver(crate::filewin::editor::Arrived::Text {
        path: "/srv/data/app.conf".into(),
        name: "app.conf".into(),
        text: "a=1\n".into(),
    });
    assert!(w.settle_opened_edits());
    *w.editing_text_mut().expect("编辑面不见了") = "a=2\n".into();
    assert!(w.editing().unwrap().dirty());
    assert!(!w.close_edit(), "改了没存却直接关掉了 —— 用户敲的东西没了");
    assert!(w.asking_discard(), "关不掉，却也没问");
    assert!(w.editing().is_some(), "问着的时候编辑面就没了");

    // 答「先别关」⇒ 问收掉、面留着、那些字还在。
    w.keep_editing();
    assert!(!w.asking_discard());
    assert_eq!(
        w.editing().unwrap().text,
        "a=2\n",
        "答了「先别关」却把字弄丢了"
    );

    // 答「丢掉」⇒ 真关掉。
    assert!(!w.close_edit());
    w.discard_edit();
    assert!(w.editing().is_none());
    assert!(!w.asking_discard());
}

/// 存失败 ⇒ 那句原话画在编辑面上，**而用户敲的东西一个字都不少**。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_save_shows_the_reason_and_keeps_the_text() {
    let mut w = FileWindow::seeded(
        Source::remote(synth_cfg("edit-fence")),
        "/srv/refuse".to_string(),
        tokio::runtime::Handle::try_current().ok(),
        Vec::<Row>::new(),
    );
    // 〔F2〕存那一趟走后端写面（`files-write-text`）。合成后端对 `root` 里带 `refuse` 的
    //   一律按围栏那一档拒（`refused`）—— 本条要的是「拒了 ⇒ 原话画上、字不丢」。
    let wired = crate::filewin::find::testing::wire_up(
        "edit-fence",
        crate::filewin::find::testing::FakeBackend::new(
            &["files-write-text"],
            crate::filewin::find::testing::Declared::default(),
        ),
    )
    .await;
    w.attach_line(wired.line.clone());
    let jsonl = "/srv/refuse/s.jsonl";
    w.edits.deliver(crate::filewin::editor::Arrived::Text {
        path: jsonl.into(),
        name: "s.jsonl".into(),
        text: "{}\n".into(),
    });
    assert!(w.settle_opened_edits());
    *w.editing_text_mut().unwrap() = "改坏它\n".into();

    // 真发一趟存 —— 后端那一侧当场拒。
    assert!(w.save_edit(None), "那趟存一次都没发出去");
    for _ in 0..200 {
        if w.edits.saves() > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(w.settle_saved_edits(), "存的结局到货了却没落进那一份上");
    let p = w.editing().expect("存失败之后编辑面不见了");
    match p.last_save.clone() {
        Some(Err(why)) => assert!(
            why.contains("refused") && why.contains("refuse write"),
            "拒的不是后端那一句：{why}"
        ),
        other => panic!("往一条受保护路径上存，结局却是 {other:?}"),
    }
    // 🔴 一个字都不少，而且**还是 dirty**（远端那份没变）。
    assert_eq!(p.text, "改坏它\n", "存失败把用户敲的东西弄掉了");
    assert!(p.dirty(), "存失败之后却说已经存好了");
}

// ════════════════════════════════════════════════════════════════════════
// 🔴〔第十刀 2026-09-22〕「就是这个文件」—— 高亮 ＋ 滚进视野
// ════════════════════════════════════════════════════════════════════════

fn many_rows(n: usize) -> Vec<Row> {
    (0..n)
        .map(|i| Row {
            name: format!("f{i:06}.txt"),
            path: format!("/srv/data/f{i:06}.txt"),
            is_dir: false,
            size: 10,
            lossy_name: false,
        })
        .collect()
}

/// 🔴🔴 **偏移是算出来的，而虚拟滚动没塌。**
///
/// # 它钉的是 `设计/60 §4 戊` 立的那条纪律
///
/// 那一节逐字：「**「egui 扛得住」这句话的主语是 `show_rows`，不是 egui**」——
/// 对照组是不虚拟的 `ScrollArea::show` 在 10 万行上 **83.6 ms/帧（12 fps）**。
///
/// 「滚到第 N 行」最直观的写法是 `scroll_to_rect`，而它要**那一行这一帧真的被画出来**
/// ⇒ 虚拟滚动下只能先把全部行都画出来 —— 那正是那条纪律禁的事。
/// ⇒ 偏移 = `下标 × ROW_HEIGHT`，O(1)。
///
/// **本条同时钉两件**：① 那个偏移对；② 这一帧**物化的行数远小于总数**
/// （虚拟滚动还活着）。少了 ② 这一条会在有人改成 `scroll_to_rect` 那天照样绿。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn revealing_a_deep_row_scrolls_by_arithmetic_without_materialising_everything() {
    let n = 20_000usize;
    let target = 17_777usize;
    let rows = many_rows(n);
    let want = rows[target].name.clone();
    let mut w = FileWindow::seeded(
        Source::remote(synth_cfg("reveal-deep")),
        "/srv/data".to_string(),
        tokio::runtime::Handle::try_current().ok(),
        rows,
    );
    // 下标那一半是纯的：现扫出来就是 `target`。
    let got = crate::filewin::rows::reveal_index(&w.listing.rows.lock().unwrap(), &want)
        .expect("那一行明明在这一摞里");
    assert_eq!(got, target, "下标不对");

    let ctx = egui::Context::default();
    // 🔴 **热身帧要排在 `set_reveal` 之前**，而这一条是量具的性质、不是生产的：
    //    `find::testing::frame_text` 末尾调 `out.drop_without_applying_deltas()`
    //    ⇒ 那一帧的状态增量被丢掉，`ScrollArea` 记住的偏移**不会传到下一帧**。
    //    热身排在前面的话，那唯一一次「滚」就在热身帧里被消化掉，
    //    而下一帧的滚动位置回到 0 ⇒ 第 17 777 行没被物化、也就不会高亮
    //    （现打：实得 `revealed_row = None`）。
    //    ⚠ 生产里 egui 正常应用增量 ⇒ 偏移会留着。**这是量具的边界，别读成缺陷。**
    let _ = crate::filewin::find::testing::frame_text(&ctx, &mut w, Vec::new());
    w.set_reveal(&want);
    assert_eq!(w.reveal_name(), Some(want.as_str()));
    let _ = crate::filewin::copy::testing::painted_text(
        &ctx,
        egui::vec2(1600.0, 800.0),
        0.1,
        Vec::new(),
        |ui| w.frame_body(ui),
    );

    // 只报读数（同 `scale_f2` 那套「report-only」先例）：它是「虚拟滚动还活着」
    // 这件事的具体数，比一句散文有用。现打：total=20000 materialized=35。
    println!(
        "〔现打〕total={} materialized={} first={} last={} revealed={:?}",
        w.tally.total_rows,
        w.tally.rows_materialized,
        w.tally.first_row,
        w.tally.last_row,
        w.tally.revealed_row
    );
    // ① 高亮落在**那一行**上。
    assert_eq!(
        w.tally.revealed_row,
        Some(target),
        "高亮没落在第 {target} 行（实得 {:?}）",
        w.tally.revealed_row
    );
    // ② 🔴 虚拟滚动还活着 —— 这一帧物化的行数远小于 20 000。
    assert_eq!(w.tally.total_rows, n);
    assert!(
        w.tally.rows_materialized < 200,
        "这一帧物化了 {} 行（共 {n}）—— 虚拟滚动塌了。\n\
         ★ 那不是「慢一点」：`设计/60 §4 戊` 现打，不虚拟的 `ScrollArea::show` \
         在 10 万行上是 83.6 ms/帧（12 fps）。\n\
         ⇒ 检查有没有人把「滚到某一行」改成了 `scroll_to_rect`（它要那一行先被画出来）",
        w.tally.rows_materialized
    );
    // ③ 那一行真的在物化区间里（否则「高亮了」是在一个没画的行上）。
    assert!(
        w.tally.first_row <= target && target < w.tally.last_row,
        "第 {target} 行不在这一帧的物化区间 [{}, {}) 里 —— 滚过去了吗？",
        w.tally.first_row,
        w.tally.last_row
    );
}

/// 🔴 **阴性对照**：没给 reveal ⇒ 哪一行都不高亮。
///
/// 少了它，上一条可以靠「恒高亮第一行」之类的实现全绿。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn with_no_reveal_no_row_is_highlighted() {
    let mut w = FileWindow::seeded(
        Source::remote(synth_cfg("reveal-none")),
        "/srv/data".to_string(),
        tokio::runtime::Handle::try_current().ok(),
        many_rows(50),
    );
    assert!(w.reveal_name().is_none());
    let ctx = egui::Context::default();
    let _ = crate::filewin::find::testing::frame_text(&ctx, &mut w, Vec::new());
    let _ = crate::filewin::copy::testing::painted_text(
        &ctx,
        egui::vec2(1600.0, 800.0),
        0.1,
        Vec::new(),
        |ui| w.frame_body(ui),
    );
    assert_eq!(w.tally.revealed_row, None, "没给 reveal 却高亮了某一行");
    // 反空真：这一帧真的画了行（否则上面那一比是空真的）。
    assert!(w.tally.rows_materialized > 0);
}

/// **只滚一次。** 每帧都滚等于把用户自己的滚动按住了。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_jump_happens_once_not_every_frame() {
    let rows = many_rows(100);
    let want = rows[42].name.clone();
    let mut w = FileWindow::seeded(
        Source::remote(synth_cfg("reveal-once")),
        "/srv/data".to_string(),
        tokio::runtime::Handle::try_current().ok(),
        rows,
    );
    w.set_reveal(&want);
    // 🔴 步距**由调用方给**（它要 `ui` 才拿得到行间距，理由住 `rows::row_pitch`）。
    //    这里喂一个合成步距，判的是「乘对了」而不是「间距是多少」。
    let pitch = 21.0f32;
    match w.take_reveal_offset(pitch) {
        Some(Ok(y)) => assert_eq!(y, 42.0 * pitch, "偏移不是「下标 × 步距」"),
        other => panic!("第一次就该给出偏移，实得 {other:?}"),
    }
    // 第二次起不再给 —— 而**高亮还在**（两半刻意分开，见 `Reveal` 头注）。
    assert!(w.take_reveal_offset(pitch).is_none(), "每帧都在滚");
    assert!(w.take_reveal_offset(pitch).is_none());
    assert_eq!(w.reveal_name(), Some(want.as_str()), "滚过之后高亮也没了");
}

/// 🔴 那一行**不在这个目录里** ⇒ 出声，而且**把高亮撤掉**。
///
/// 两半都要：静默什么都不做与「跳过去了」在屏幕上同形；
/// 而留着高亮等于在屏幕上标一个不存在的东西（下一次重列时它会去命中一个同名的别人）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reveal_target_that_is_gone_says_so_and_drops_the_highlight() {
    let mut w = FileWindow::seeded(
        Source::remote(synth_cfg("reveal-gone")),
        "/srv/data".to_string(),
        tokio::runtime::Handle::try_current().ok(),
        many_rows(10),
    );
    w.set_reveal("这个文件不在这儿.txt");
    match w.take_reveal_offset(21.0) {
        Some(Err(why)) => {
            assert!(
                why.contains("这个文件不在这儿.txt"),
                "没说是哪个文件：{why}"
            );
            assert!(
                why.contains("删") || why.contains("改"),
                "没给出可能的原因：{why}"
            );
        }
        other => panic!("那一行不在这一摞里，却没出声：{other:?}"),
    }
    assert!(w.reveal_name().is_none(), "找不到那一行，高亮却还挂着");
}

/// 换目录 ⇒ 高亮清掉。
///
/// 🔴 留着的后果具体：新目录里**恰好同名**的另一个文件会被高亮，
/// 而用户会以为那就是他要找的那个。
///
/// ⚠〔2026-09-23〕从前这条还判「**换机器**那两条路同样」（`go_local` / `go_remote`
/// 各清一次）。本机侧退役之后一个窗口的机器**一辈子只有一台** ——
/// 换机器那件事不存在了，不是那两格没人看了。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn navigating_away_drops_the_highlight() {
    let mut w = FileWindow::seeded(
        Source::remote(synth_cfg("reveal-nav")),
        "/srv/data".to_string(),
        tokio::runtime::Handle::try_current().ok(),
        many_rows(10),
    );
    w.set_reveal("f000003.txt");
    assert!(w.reveal_name().is_some());
    w.navigate_to("/srv/other".into());
    assert!(w.reveal_name().is_none(), "换了目录高亮还挂着");
}

// ════════════════════════════════════════════════════════════════════════
// 🔴〔第十一刀 2026-09-22〕「当场就死了」不再被报成成功
//    〔第十三刀 2026-09-23 补记〕**被判的东西换了，这一跳没换。**
//    开窗改成起一个独立进程之后，`early_failure` 的生产调用方是
//    `filewin::proc::open_in_new_process`（判「那个**进程**是不是当场就退了」）。
//    下面这两条**照旧喂线程** —— 那是**刻意的**：它们钉的是那条轮询本身的两个方向
//    （「结束了」认得出 · 「还在跑」不误判），而线程是这两个方向最便宜的合成输入。
//    进程那一侧的行为判据住 `proc_tests`（那边不合成，真起进程）。
// ════════════════════════════════════════════════════════════════════════

/// 当场就失败的那条线程，`early_failure` 认得出来。
#[test]
fn a_thread_that_dies_at_once_is_recognised_as_a_failure() {
    let h: std::thread::JoinHandle<Result<(), String>> =
        std::thread::spawn(|| Err("开窗失败: 事件循环不能重建".into()));
    assert!(
        crate::filewin::shell::early_failure(
            || h.is_finished(),
            std::time::Duration::from_millis(500)
        ),
        "一条立刻就回 Err 的线程没被认出来 —— 那一形会被报成「窗口起来了」"
    );
    // 原因拿得回来（上层要把它交给用户）。
    match h.join() {
        Ok(Err(e)) => assert!(e.contains("事件循环"), "{e}"),
        other => panic!("{other:?}"),
    }
}

/// 🔴 **阴性对照**：还在跑的那条线程**不许**被当成失败。
///
/// 少了它，一个「恒回 true」的实现照样绿 —— 而那时**每一次**开窗都会被报成失败，
/// 连真起来的那次也是。
#[test]
fn a_thread_still_running_is_not_mistaken_for_a_failure() {
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let h: std::thread::JoinHandle<Result<(), String>> = std::thread::spawn(move || {
        // 一直占着这条线程，直到判据放它走 —— 这就是「窗口起来了」那一形的形状
        //（`run_native` 占着线程直到窗口关闭）。
        let _ = rx.recv();
        Ok(())
    });
    assert!(
        !crate::filewin::shell::early_failure(
            || h.is_finished(),
            std::time::Duration::from_millis(120)
        ),
        "还占着线程的那一条被当成了失败 —— 那会让每一次真开窗都报错"
    );
    let _ = tx.send(());
    let _ = h.join();
}

/// 开窗那条路**真的**经这一跳走，而且排在起进程之后。
///
/// ⚠ 判源码是代理（同族先例住 `entry_tests` 那条「空路径那一支」）。
/// 买的是：起进程的结果不再被 `let _ = …` 丢掉，而且那一跳有东西可看时才跑。
///
/// 🔴〔第十三刀 2026-09-23〕**射程从 `entry.rs` 换到了 `proc.rs`。**
/// 上一版这一条扫的是 `entry.rs`，因为那时「起线程 ＋ 看它死没死」两步都写在入口里。
/// 今天入口只剩一句 `open_in_new_process(…)?`，那两步整块搬进了 `filewin::proc`
/// ⇒ 继续扫 `entry.rs` 的话，这一条会在一个**恒为零**的人群上报绿。
/// ⚠ 入口那一侧**没有失去判据**：`proc_tests` 里那条零命中型盯着
/// 「入口那条路上不许再有『同进程开一个』的写法」（`D11`）。
#[test]
fn the_spawn_result_is_never_thrown_away() {
    let prod = guard_core::production_code(include_str!("../../../src/bridge/src/filewin/proc.rs"));
    // ★ 反向自检：剥完不是空的，否则下面几比全在空人群上。
    assert!(
        prod.contains("pub fn open_in_new_process"),
        "剥生产段把那条路一起剥掉了 —— 下面几比此刻不可信"
    );
    assert_eq!(
        prod.matches("early_failure(").count(),
        1,
        "`proc.rs` 生产段里 `early_failure(` 不是恰好一处 —— \
         这一族轮询全仓只许一处（`rust_timer_registry` 登记的就是它）"
    );
    let spawn_needle = format!("{}_window(", "spawn");
    assert!(
        !prod.contains(&format!("let _ = {spawn_needle}")),
        "起进程的结果又被 `let _ = …` 丢掉了 —— 那就回到了「静默成功」那一形"
    );
    // 🔴 **先把那个函数项切出来，再比先后** —— 而这一刀是死值验逼出来的，不是洁癖。
    //
    // 上一版在**整份生产段**上比 `find(spawn_window()` 与 `find(early_failure()`。
    // 死值验现打：把那一跳原地挪到起进程**之前**，这一条**照旧报绿**。
    // 病根是 `spawn_window` 的**定义**（`pub fn spawn_window(`）就在文件里更靠前的位置
    // ⇒ `at_spawn` 拿到的是定义的偏移，恒小于任何一处调用 ⇒ **那一比恒真**。
    // ⇒ 人群必须收到「`open_in_new_process` 这一个函数项」里面。
    let at_fn = prod
        .find("pub fn open_in_new_process")
        .expect("`open_in_new_process` 不在生产段里 —— 抽取器坏了");
    let rest = &prod[at_fn..];
    let body_end = rest
        .find("\n}\n")
        .expect("`open_in_new_process` 的花括号没收口 —— 抽取器看不懂它了");
    let body = &rest[..body_end];
    // ★ 反向自检：切出来的那一段里**两者都在**。任一缺席 ⇒ 下面那一比是空转的。
    let at_spawn = body
        .find(spawn_needle.as_str())
        .expect("切出来的那个函数项里没有起进程那一句 —— 切法坏了，下面那一比此刻恒真");
    let at_check = body
        .find("early_failure(")
        .expect("切出来的那个函数项里没有那一跳 —— 切法坏了");
    assert!(
        at_spawn < at_check,
        "那一跳排在起进程**之前** —— 那时还没有进程可看"
    );
}

// ════════════════════════════════════════════════════════════════════════
// 🔴〔补齐五项 2026-09-23〕排序下拉 · 面包屑 · 在此打开终端
// ════════════════════════════════════════════════════════════════════════

fn sized_row(name: &str, size: u64) -> Row {
    Row {
        name: name.to_string(),
        path: format!("/srv/data/{name}"),
        is_dir: false,
        size,
        lossy_name: false,
    }
}

/// 🔴 **换档当场重排手上那一摞**（不是「记下来等下次列目录」）；同一档再点一次什么都不做。
#[test]
fn picking_a_sort_reorders_the_rows_already_on_screen() {
    let mut w = remote_window_with_rows(
        "/srv/data",
        vec![sized_row("a", 1), sized_row("b", 300), sized_row("c", 20)],
    );
    assert_eq!(names(&w), ["a", "b", "c"]);
    assert!(w.set_sort(SortBy::Size), "换到另一档该回 true");
    assert_eq!(names(&w), ["b", "c", "a"], "换档之后屏幕上那一摞没重排");
    assert!(!w.set_sort(SortBy::Size), "同一档再点一次不该算「换了」");
    assert!(w.set_sort(SortBy::Name));
    assert_eq!(names(&w), ["a", "b", "c"]);
}

/// 工具栏上真画出了那三样：排序下拉（带当前那一档）· 「在此打开终端」· 面包屑每一段。
///
/// ⚠ 判的是**这一帧画出来的文字**（生产那个 `frame_body`），不是源码里有没有那几个字面量。
#[test]
fn the_toolbar_really_paints_sort_breadcrumbs_and_the_terminal_button() {
    let mut w = remote_window_with_rows("/srv/data/子目录", vec![file_row("x")]);
    let ctx = egui::Context::default();
    let _ = crate::filewin::find::testing::frame_text(&ctx, &mut w, Vec::new());
    let painted = crate::filewin::find::testing::frame_text(&ctx, &mut w, Vec::new());
    for want in ["在此打开终端", "排序：名称", "/", "srv", "data", "子目录"] {
        assert!(
            painted.iter().any(|t| t == want),
            "这一帧上没有「{want}」。画出来的是：{painted:?}"
        );
    }
    // 换档之后下拉那一格跟着变（它读的是同一个状态，不是一个写死的串）。
    w.set_sort(SortBy::Type);
    let painted = crate::filewin::find::testing::frame_text(&ctx, &mut w, Vec::new());
    assert!(painted.iter().any(|t| t == "排序：类型"));
    assert!(!painted.iter().any(|t| t == "排序：名称"), "旧那一档还画着");
}

/// 🔴 **没有运行时的窗口点「在此打开终端」：出声，而且那句话画在窗口上**（不是静默什么都不发生）。
#[test]
fn opening_a_terminal_with_no_runtime_says_so_on_the_window() {
    let mut w = remote_window_with_rows("/srv/data", Vec::new());
    assert_eq!(w.term_notice(), None);
    assert!(!w.open_terminal_here(None), "没有运行时却说发出去了");
    let said = w.term_notice().expect("没有运行时，却一句话都没留下");
    let ctx = egui::Context::default();
    let _ = crate::filewin::find::testing::frame_text(&ctx, &mut w, Vec::new());
    let painted = crate::filewin::find::testing::frame_text(&ctx, &mut w, Vec::new());
    assert!(
        painted.iter().any(|t| t == &said),
        "那句话没画出来：{painted:?}"
    );
}

/// 🔴 **「在此打开终端」拼出来的那一串与旧面板逐字节相同 —— 跨语言对拍。**
///
/// 两侧不同源：期望串**现读** `tests/remote-launch.test.ts` 里旧面板那条判据的三行
/// （`buildOpenTerminalCmd` 的黄金样例），本侧喂同样的三个入参。
/// TS 那一份哪天改了行为（它的判据也就跟着改了），这里当场红 —— 不再是「有账、没自动对拍」。
#[test]
fn the_open_terminal_command_equals_the_old_panels_byte_for_byte() {
    const TS: &str = include_str!("../../../tests/remote-launch.test.ts");
    const SHELL_LINE: &str = "const shell = \"exec ${SHELL:-bash} -l\";";
    const GOLDEN: &[(&str, &str, &str)] = &[
        (
            "eq(buildOpenTerminalCmd(\"/home/pi/p\"), `cd '/home/pi/p' && ${shell}`);",
            "/home/pi/p",
            "cd '/home/pi/p' && exec ${SHELL:-bash} -l",
        ),
        (
            "eq(buildOpenTerminalCmd(\"  \"), shell);",
            "  ",
            "exec ${SHELL:-bash} -l",
        ),
        (
            "eq(buildOpenTerminalCmd(\"/a b/c\"), `cd '/a b/c' && ${shell}`);",
            "/a b/c",
            "cd '/a b/c' && exec ${SHELL:-bash} -l",
        ),
    ];
    guard_core::pin_line(TS, SHELL_LINE).expect("TS 那条黄金样例里 `shell` 那一行变了");
    for (ts_line, input, want) in GOLDEN {
        guard_core::pin_line(TS, ts_line)
            .unwrap_or_else(|e| panic!("TS 那一侧的黄金样例变了 —— 两份漂开了：{e}"));
        assert_eq!(build_open_terminal_cmd(input), *want, "入参 {input:?}");
    }
    // ⚠ 双引号那一条：模板自己**一个都不带**（`launch.rs` 拒掉含双引号的 `remote_cmd`）；
    //   路径里自带的双引号会原样进单引号里 ⇒ 那一形由 `launch.rs` 拒、窗口出声，不在这里兜。
    assert!(!build_open_terminal_cmd("").contains('"'));
}
