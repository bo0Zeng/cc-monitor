//! [`super`] 的判据 —— 前半是书签的**存储**（归一 · 切换 · 锁 · 原子换），
//! 后半是**窗口上那一半**（书签栏真画得出、真点得到，跑生产那个 `frame_body`）。
//!
//! | 判据 | 钉的那一形 | 两侧异源在哪 |
//! |---|---|---|
//! | [`normalize_matches_the_old_panel_case_by_case`] | 归一与老面板 `normalize` 逐格相等 | 期望是老面板那份 vitest 里的原样例（F7b 删面板那一拍的父提交里现读）＋ 手推的几格 |
//! | [`normalize_agrees_with_the_two_step_rewrite_on_a_generated_corpus`] | 同上，在 4 096 条生成路径上 | 另一侧是老面板那两步替换的逐字移植（「折叠」与「去尾」分两趟做），与生产那一趟扫描不同写法 |
//! | [`toggle_and_remove_follow_the_old_panel`] | 切换 / 删除的列表逐格相等（去重 · 保序 · 归一比较） | 期望手写 |
//! | [`the_disk_holds_exactly_what_was_written`] | 盘上那份读回来逐格相等；文件不在 ⇒ 空；读不懂 ⇒ 报错且**不覆盖** | 盘上字节读回 |
//! | [`two_writers_lose_nothing`] | 两条线程各切 N 条不同目录，完了盘上集合 == 全部 2N 条（两向） | 期望集合由循环下标另算 |
//! | [`a_writer_waits_while_another_holds_the_lock`] | 锁被拿着 ⇒ 写等着、盘上不变；锁一放 ⇒ 写进去（上一条的确定性那一半） | 锁由判据自己拿 |
//! | [`the_bar_is_really_clickable_and_the_disk_follows`] | ☆ 真点一下盘上就有 · 点书签真跳过去 · × 真点一下盘上就没了 | 按钮位置从这一帧画出来的字里现找；盘上字节读回 |
//! | [`another_windows_bookmark_shows_up_after_a_navigation`] | 别的写者加的那一条，换一次目录就看得见（之前看不见 ⇒ 不是每帧读盘） | 另一个写者走同一个写口 |
//! | [`no_data_dir_is_said_on_the_bar`] | 数据目录解不出来 ⇒ 那句话画在这一帧上；没接书签的窗口整条栏不画 | galley 读回 |
//!
//! ⚠ 全部落在 `std::env::temp_dir()` 下的独立目录里；真数据目录一个字节都不碰。

use super::*;
use std::collections::BTreeSet;

/// 判据用的书签文件落点（〔P4〕名字与「数据目录下的全路径」住 monitor 那一侧 —— 窗口只拿开窗入口算好的全路径，判据随便起个名字即可）。
fn file_in(dir: &Path) -> PathBuf {
    dir.join("书签.json")
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "ccm-fw34-{tag}-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&d).expect("建临时目录");
    d
}

/// 🔴 老面板 `src/sftp/paths.ts::normalize` 那份 vitest 的原样例（F7b 删面板那一拍的父提交里现读），
/// 外加几格手推（与老算法逐步推一遍得到的）。
#[test]
fn normalize_matches_the_old_panel_case_by_case() {
    let cases: &[(&str, &str)] = &[
        // ── 老面板 vitest 原样例 ──
        ("/a//b/", "/a/b"),
        ("/", "/"),
        ("//", "/"),
        // ── 手推 ──
        ("/home/pi", "/home/pi"),
        ("/home/pi/", "/home/pi"),
        ("///srv///data///", "/srv/data"),
        ("", ""),
        ("rel/dir/", "rel/dir"),
        ("/文档/项目/", "/文档/项目"),
    ];
    for (input, want) in cases {
        assert_eq!(normalize_dir(input), *want, "normalize_dir({input:?})");
    }
}

/// 老面板那两步替换的逐字移植：`replace(/\/+/g, "/")`，是根就回根，否则 `replace(/\/$/, "")`。
/// ⚠ 与生产那一趟扫描**刻意不同写法**（反复 `replace` 到不动为止、再去尾），两侧才异源。
fn old_two_step(path: &str) -> String {
    let mut c = path.to_string();
    while c.contains("//") {
        c = c.replace("//", "/");
    }
    if c == "/" {
        return c;
    }
    c.strip_suffix('/').map(str::to_string).unwrap_or(c)
}

#[test]
fn normalize_agrees_with_the_two_step_rewrite_on_a_generated_corpus() {
    // 字母表里恰好有 `/`、普通字、多字节字 ⇒ 每条路径都是这几样的任意排列。
    let alphabet = ['/', 'a', '文', '.'];
    let mut checked = 0usize;
    for n in 0..4096u32 {
        let mut s = String::new();
        let mut k = n;
        for _ in 0..6 {
            s.push(alphabet[(k % 4) as usize]);
            k /= 4;
        }
        assert_eq!(normalize_dir(&s), old_two_step(&s), "路径 {s:?}");
        checked += 1;
    }
    assert_eq!(checked, 4096);
    // 语料自检：三种要紧的形（连续 `/` · 尾 `/` · 只有 `/`）都真在人群里，否则这条对拍是空转的。
    let has = |f: &dyn Fn(&str) -> bool| {
        (0..4096u32).any(|n| {
            let mut s = String::new();
            let mut k = n;
            for _ in 0..6 {
                s.push(alphabet[(k % 4) as usize]);
                k /= 4;
            }
            f(&s)
        })
    };
    assert!(has(&|s| s.contains("//")));
    assert!(has(&|s| s.len() > 1
        && s.ends_with('/')
        && !s.ends_with("//")));
    assert!(has(&|s| s.chars().all(|c| c == '/')));
}

#[test]
fn toggle_and_remove_follow_the_old_panel() {
    // 老面板 vitest「addBookmark 去重 + 归一 + 保序」逐字那一串。
    let mut l: Vec<String> = Vec::new();
    assert!(toggle_in(&mut l, "/home/pi"));
    // ⚠ 老面板那一下是 `addBookmark`（在就不动）；窗口这边是一颗切换按钮 ⇒ 第二下是「取消」。
    //   去重那一格照旧：`/home/pi/` 与 `/home/pi` 是同一条。
    assert!(!toggle_in(&mut l, "/home/pi/"));
    assert!(toggle_in(&mut l, "/home/pi/"));
    assert!(toggle_in(&mut l, "/var/log"));
    assert_eq!(l, vec!["/home/pi".to_string(), "/var/log".to_string()]);
    // 老面板 vitest「removeBookmark 归一匹配」逐字那一串。
    let mut r = vec!["/home/pi".to_string(), "/var".to_string()];
    assert!(remove_in(&mut r, "/home/pi/"));
    assert_eq!(r, vec!["/var".to_string()]);
    assert!(!remove_in(&mut r, "/nope"), "删一条不在的 ⇒ 回 false");
    // 空路径不加。
    assert!(!toggle_in(&mut r, ""));
    assert_eq!(r, vec!["/var".to_string()]);
}

fn book(pairs: &[(&str, &[&str])]) -> Book {
    pairs
        .iter()
        .map(|(o, v)| {
            (
                (*o).to_string(),
                v.iter().map(|s| (*s).to_string()).collect(),
            )
        })
        .collect()
}

#[test]
fn the_disk_holds_exactly_what_was_written() {
    let dir = scratch("disk");
    let file = file_in(&dir);
    assert_eq!(
        read_book(&file).unwrap(),
        Book::new(),
        "文件不在 ⇒ 空的一份"
    );

    mutate(&file, |b| {
        b.insert("devbox".into(), vec!["/srv".into(), "/home/user".into()]);
    })
    .unwrap();
    assert_eq!(
        read_book(&file).unwrap(),
        book(&[("devbox", &["/srv", "/home/user"])])
    );

    // 切掉最后一条 ⇒ 那台机器那一格整个不在（不留空数组）。
    let (_, after) = mutate(&file, |b| {
        let l = b.get_mut("devbox").unwrap();
        l.clear();
    })
    .unwrap();
    assert!(after.is_empty());
    assert_eq!(read_book(&file).unwrap(), Book::new());

    // 读不懂 ⇒ 报错，而且**不覆盖**（覆盖掉就是把别的机器那几格一起清了）。
    std::fs::write(&file, b"{ not json").unwrap();
    let e = mutate(&file, |b| b.insert("devbox".into(), vec!["/x".into()])).unwrap_err();
    assert!(e.contains("读不懂"), "那句话要说清是读不懂：{e}");
    assert_eq!(
        std::fs::read(&file).unwrap(),
        b"{ not json",
        "坏文件一个字节都没动"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// 🔴 两个写者（窗口 · monitor，或两个窗口）**不丢更新**。
///
/// 两条线程各开各的锁句柄（`flock` 按打开的文件描述算，同进程两次打开照样互斥），
/// 各切 `N` 条互不相同的目录。完了盘上那一格的集合必须 == 全部 `2N` 条。
#[test]
fn two_writers_lose_nothing() {
    const N: usize = 40;
    let dir = scratch("race");
    let file = file_in(&dir);
    let spawn = |tag: &'static str| {
        let file = file.clone();
        std::thread::spawn(move || {
            for i in 0..N {
                mutate(&file, |b| {
                    toggle_in(b.entry("devbox".into()).or_default(), &format!("/{tag}/{i}"))
                })
                .unwrap();
            }
        })
    };
    let (a, b) = (spawn("a"), spawn("b"));
    a.join().unwrap();
    b.join().unwrap();
    let got: BTreeSet<String> = read_book(&file).unwrap()["devbox"].iter().cloned().collect();
    let want: BTreeSet<String> = (0..N)
        .flat_map(|i| [format!("/a/{i}"), format!("/b/{i}")])
        .collect();
    assert_eq!(
        got,
        want,
        "少了：{:?}\n多了：{:?}",
        want.difference(&got).collect::<Vec<_>>(),
        got.difference(&want).collect::<Vec<_>>()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// 🔴 **写口真的上锁**（确定性的那一半 —— 上一条是概率性的：两条线程不一定真撞上）。
///
/// 判据自己先拿住锁 ⇒ 另一条线程的写**必须等**（300 ms 内不许写进去）；放了锁 ⇒ 它才写进去。
/// 把 [`mutate`] 里那一下上锁摘掉，这条当场红（死值验那一刀就下在那儿）。
#[test]
fn a_writer_waits_while_another_holds_the_lock() {
    let dir = scratch("lock");
    let file = file_in(&dir);
    let held = lock_store(&file).expect("判据自己拿锁");
    let (tx, rx) = std::sync::mpsc::channel();
    let f2 = file.clone();
    let t = std::thread::spawn(move || {
        mutate(&f2, |b| toggle_in(b.entry("devbox".into()).or_default(), "/x")).unwrap();
        tx.send(()).unwrap();
    });
    assert!(
        rx.recv_timeout(std::time::Duration::from_millis(300))
            .is_err(),
        "锁被别人拿着时，写照样进去了 —— 写口没上锁"
    );
    assert_eq!(
        read_book(&file).unwrap(),
        Book::new(),
        "锁拿着的时候盘上就变了"
    );
    drop(held);
    rx.recv_timeout(std::time::Duration::from_secs(10))
        .expect("锁放了还写不进去");
    t.join().unwrap();
    assert_eq!(read_book(&file).unwrap(), book(&[("devbox", &["/x"])]));
    let _ = std::fs::remove_dir_all(&dir);
}

// ═══════════════════════════════════════════════════════════════════
// 窗口上那一半：书签栏真画得出、真点得到（跑生产那个 `frame_body`）
// ═══════════════════════════════════════════════════════════════════

use crate::filewin::copy::testing::{rects_of, text_in_frame, PaintedText};
use crate::filewin::find::testing::{window_on, wire_up, Declared, FakeBackend};
use crate::filewin::shell::FileWindow;

/// 一帧生产那个 `frame_body`，交回这一帧画出来的字 ＋ 位置。
fn frame(ctx: &egui::Context, w: &mut FileWindow, events: Vec<egui::Event>) -> Vec<PaintedText> {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1600.0, 900.0),
        )),
        events,
        ..Default::default()
    };
    let out = ctx.run_ui(input, |ui| w.frame_body(ui));
    let painted = text_in_frame(&out);
    out.drop_without_applying_deltas();
    painted
}

/// 点这一帧上内容**正好等于** `label` 的那一段（恰好一处，否则量具先红）。
fn click_label(ctx: &egui::Context, w: &mut FileWindow, label: &str) -> Vec<PaintedText> {
    let painted = frame(ctx, w, Vec::new());
    let at = rects_of(&painted, label);
    assert_eq!(
        at.len(),
        1,
        "这一帧上「{label}」该恰好一处，现打 {}：{painted:?}",
        at.len()
    );
    frame(
        ctx,
        w,
        crate::filewin::rows::testing::click_at(at[0].center()),
    )
}

/// 等列目录落地（带上限，绝不挂死）。
async fn settle_listing(w: &FileWindow, who: &str) {
    for _ in 0..600 {
        if !w.listing.is_loading() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("{who}：等了 3 秒目录还没列完");
}

/// 🔴 **真点一下**：☆ 加进去（盘上真有）→ 换目录 → 点那条书签跳回来 → × 删掉（盘上真没了）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_bar_is_really_clickable_and_the_disk_follows() {
    let root = scratch("ui");
    let (a, b) = (root.join("a"), root.join("b"));
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    let (a, b) = (
        a.to_string_lossy().to_string(),
        b.to_string_lossy().to_string(),
    );
    let file = file_in(&root);
    let wired = wire_up(
        "fw34-bm-ui",
        FakeBackend::new(&["files-ls"], Declared::default()),
    )
    .await;
    let mut w = window_on(&wired, &a);
    w.shelf = Some(Shelf::open(
        Some(file.clone()),
        &Origin("fw34-bm-ui".into()),
    ));
    let ctx = egui::Context::default();

    // ① ☆ 加书签 ⇒ 盘上那一格恰好是当前目录；按钮换成 ★。
    let after = click_label(&ctx, &mut w, ADD_LABEL.as_str());
    let _ = after;
    assert_eq!(
        read_book(&file).unwrap(),
        book(&[("fw34-bm-ui", &[a.as_str()])])
    );
    let painted = frame(&ctx, &mut w, Vec::new());
    assert_eq!(
        rects_of(&painted, DROP_LABEL.as_str()).len(),
        1,
        "加完之后按钮该写「{DROP_LABEL}」",
        DROP_LABEL = DROP_LABEL.as_str()
    );
    assert_eq!(rects_of(&painted, ADD_LABEL.as_str()).len(), 0);

    // ② 换到 b，点那条书签 ⇒ 真的跳回 a（跳转收在帧尾：同一帧里就换了）。
    w.navigate_to(b.clone());
    settle_listing(&w, "换到 b").await;
    let _ = click_label(&ctx, &mut w, &a);
    assert_eq!(w.cwd, a, "点了书签却没跳过去");

    // ③ × 删掉 ⇒ 盘上那台机器那一格整个没了。
    let _ = click_label(&ctx, &mut w, REMOVE_LABEL.as_str());
    assert_eq!(read_book(&file).unwrap(), Book::new(), "点了 × 盘上还在");
    let _ = std::fs::remove_dir_all(&root);
}

/// 别的窗口（另一个进程）刚加的书签，本窗口**换一次目录**就看得见。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn another_windows_bookmark_shows_up_after_a_navigation() {
    let root = scratch("other");
    let a = root.join("a");
    std::fs::create_dir_all(&a).unwrap();
    let a = a.to_string_lossy().to_string();
    let file = file_in(&root);
    let wired = wire_up(
        "fw34-bm-other",
        FakeBackend::new(&["files-ls"], Declared::default()),
    )
    .await;
    let mut w = window_on(&wired, "/");
    w.shelf = Some(Shelf::open(
        Some(file.clone()),
        &Origin("fw34-bm-other".into()),
    ));
    // 另一个写者（同一个写口）。
    mutate(&file, |b| {
        toggle_in(b.entry("fw34-bm-other".into()).or_default(), "/elsewhere")
    })
    .unwrap();
    assert!(
        w.shelf.as_ref().unwrap().list().is_empty(),
        "还没换目录就看见了 —— 那说明每帧在读盘"
    );
    w.navigate_to(a);
    assert_eq!(
        w.shelf.as_ref().unwrap().list(),
        vec!["/elsewhere".to_string()]
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// 数据目录解不出来 ⇒ 书签栏上**出声**（从这一帧的 galley 里读回来），按钮点了也不写任何地方。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_data_dir_is_said_on_the_bar() {
    let wired = wire_up("fw34-bm-nodir", FakeBackend::new(&[], Declared::default())).await;
    let mut w = window_on(&wired, "/srv");
    w.shelf = Some(Shelf::open(None, &Origin("fw34-bm-nodir".into())));
    let ctx = egui::Context::default();
    let painted = frame(&ctx, &mut w, Vec::new());
    assert!(
        painted.iter().any(|(t, _)| t == NO_DATA_DIR.as_str()),
        "那句话没画出来：{painted:?}"
    );
    let _ = click_label(&ctx, &mut w, ADD_LABEL.as_str());
    assert!(w.shelf.as_ref().unwrap().list().is_empty());
    // 阴性对照：没接书签（判据直接建的窗口）⇒ 整条书签栏不画。
    let mut bare = window_on(&wired, "/srv");
    let painted = frame(&ctx, &mut bare, Vec::new());
    assert!(
        rects_of(&painted, ADD_LABEL.as_str()).is_empty()
            && !painted.iter().any(|(t, _)| t == NO_DATA_DIR.as_str())
    );
}
