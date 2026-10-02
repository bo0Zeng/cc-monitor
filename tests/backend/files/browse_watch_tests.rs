//! 保鲜的另一半（挂 watch 那一档）的判据。
//!
//! # 它买到的
//!
//! - 名单差分（新增 / 卸掉 / **被上限拒掉**）三个数都是**相等**断言。
//! - 上限与本机 `inotify` 上限的**绝对量**对比 —— 现打那个上限，不抄一个数。
//! - 🔴 **真正值钱的那一格**：一个文件在**索引建好之后**才出现，
//!   而它所在的目录挂着 watch ⇒ 查询**立刻**找得到它。
//!   那就是「眼前那一块是秒级」这句话的活体。
//! - 反向那半：一个文件在索引里、但盘上已经没了 ⇒ 重列之后查询**不再**回它。
//!   这一格是 `supersedes` 那条遮盖真的在生效的唯一证据。
//! - 裸 `notify` 那一跳真的挂得上、事件真的推过来（`the_real_watcher_*`）。
//!
//! # 它**买不到**的（逐条）
//!
//! 1. **watch 绑 inode 不绑路径那一形没判**：把浏览的目录删掉再重建成同名新 inode，
//!    本模块的 watch 会留在旧 inode 上。⚠ 那**不是**本判据漏了，是**实现今天没补救**
//!    （模块头注逐字登记过）。这里不假装它被守着。
//! 2. **内核队列溢出那一形没判**（要灌几万个事件才逼得出来）。
//! 3. **跨平台没判**：`ReadDirectoryChangesW` / `FSEvents` 两侧一格都没量 ——
//!    本机没有那两个平台的真机。逐 target 的如实声明住 `files::FRESHNESS`。

use super::*;

use crate::files::index::tests::{make_tree, resident_lock};

#[test]
fn the_browse_list_is_a_real_diff() {
    let _lock = resident_lock();
    let fx = make_tree("watchdiff", 3, 1, 0);
    let a = fx.root.join("d0000");
    let b = fx.root.join("d0001");
    let c = fx.root.join("d0002");

    let first = set_browsing(&[a.clone(), b.clone()]);
    assert_eq!(
        first,
        Applied {
            added: 2,
            removed: 0,
            rejected: 0
        },
        "第一次挂两个目录，三个数应当是 (2, 0, 0)"
    );
    assert_eq!(watched_count(), 2);

    // 换一批：`a` 留着（**不该被重新挂一次**）、`b` 走、`c` 来。
    let second = set_browsing(&[a.clone(), c.clone()]);
    assert_eq!(
        second,
        Applied {
            added: 1,
            removed: 1,
            rejected: 0
        },
        "差分算错了 —— 留下来的那个被当成新增，会让每次换目录都重列一遍所有目录"
    );
    assert_eq!(watched_count(), 2);

    // 重复的目录只算一次。
    let dup = set_browsing(&[a.clone(), a.clone(), a]);
    assert_eq!(dup.added, 0, "同一个目录报了三次，不该被挂三份");
    assert_eq!(watched_count(), 1);
}

/// 🔴 超上限**拒掉并报数**，不静默截断。
#[test]
fn going_over_the_cap_is_reported_not_swallowed() {
    let _lock = resident_lock();
    let over = 3usize;
    let fx = make_tree("watchcap", MAX_BROWSE_WATCHES + over, 0, 0);
    let dirs: Vec<std::path::PathBuf> = (0..MAX_BROWSE_WATCHES + over)
        .map(|d| fx.root.join(format!("d{d:04}")))
        .collect();
    let applied = set_browsing(&dirs);
    assert_eq!(applied.added, MAX_BROWSE_WATCHES, "挂上的个数不等于上限");
    assert_eq!(
        applied.rejected, over,
        "🔴 超出的那几个被静默丢了 —— 用户会遇到「我明明在看这个目录，新文件却要等重走」，\n\
         而盘面上没有任何东西说得出为什么"
    );
    assert_eq!(watched_count(), MAX_BROWSE_WATCHES);
}

/// 上限与**本机现打**的 `inotify` 上限对比。
///
/// 🔴 **绝对量比，不写成百分比**（那条现打逼出来的纪律：
/// 闸不许比量具自身的分辨率还细）。
#[test]
fn the_cap_leaves_room_for_thousands_of_backends_on_one_machine() {
    // 这个住址是 Linux 专属的；读不到就说「判不了」，**不静默变绿**。
    let limit_file = std::path::Path::new("/proc")
        .join("sys")
        .join("fs")
        .join("inotify")
        .join("max_user_watches");
    match std::fs::read_to_string(&limit_file) {
        Ok(text) => {
            let limit: usize = text.trim().parse().expect("这个文件里本该是一个整数");
            // 读数（不是闸）：这台机器上「上限 ÷ 本族上限」＝ 能同时跑多少个这样的后端。
            eprintln!(
                "[24f·F2 附读数] inotify max_user_watches 现打 = {limit}；\
                 本族上限 = {MAX_BROWSE_WATCHES}；比值 = {}",
                limit / MAX_BROWSE_WATCHES
            );
            // 闸取 **128**，不取上面那个比值。
            // 🔴 理由：`max_user_watches` 是个管理员**随手改得动**的整数（老内核默认就是 8192），
            //    拿本机现打的比值当闸 = 把这台机器的配置烤进判据里（本仓「金标准把开发机
            //    烤进去只有它永远绿」那一族）。128 是「一台机器上跑上百个后端也吃不满」的量级。
            assert!(
                MAX_BROWSE_WATCHES * 128 <= limit,
                "本族上限 {MAX_BROWSE_WATCHES} × 128 = {} 超过了本机上限 {limit}。\n\
                 ⇒ 要么把上限调下来，要么把这条断言的余量系数改小**并写清为什么**。",
                MAX_BROWSE_WATCHES * 128
            );
        }
        Err(e) => {
            assert!(
                !cfg!(target_os = "linux"),
                "在 Linux 上读不到 `max_user_watches`（{e}）—— 这不是「这台机器没有上限」，\n\
                 是本条判据的量具坏了。读不到就不许拿「过了」当答案。"
            );
            eprintln!(
                "[24f] `max_user_watches` 在本 target 上读不到 ⇒ 上限余量那一格**判不了**。\n\
                 缺的证据：这个平台上「同时能挂多少个目录监听」的等价上限。"
            );
        }
    }
}

/// 🔴🔴 **本族保鲜那一半的存在理由**：索引建好**之后**新建的文件，
/// 只要它在浏览的目录里，查询立刻就找得到。
#[test]
fn a_file_created_after_the_walk_is_found_because_its_directory_is_being_watched() {
    let _lock = resident_lock();
    let fx = make_tree("fresh", 2, 2, 0);
    let dir = fx.root.join("d0000");

    // ① 先建索引 —— 此刻那个文件还不存在。
    // 🔴 这个位是**进程级**的，而 `cargo test` 默认并行 ⇒ 先串行（理由住 `index_testing.rs`）
    let _serial = crate::files::index::testing::serial();
    crate::files::index::rebuild_once(&fx.root)
        .expect("本格独占跑，抢不到那个位就是并发保护写错了 —— 不许静默当成走过了");
    let before = crate::files::index::tests::query("born-after-the-walk", 10);
    assert_eq!(
        before.total_hits, 0,
        "它还没被造出来就搜到了 —— 夹具或索引坏了，下面那一格在测别的东西"
    );

    // ② 声明「我在浏览这个目录」，然后造那个文件。
    set_browsing(&[dir.clone()]);
    std::fs::File::create(dir.join("born-after-the-walk")).expect("造那个文件");

    // ③ 事件到达等价于调一次 `on_change`（真 inotify 那一跳另有一条判据）。
    assert!(
        on_change(crate::files::raw::path_bytes(&dir)),
        "这个目录明明在名单上，`on_change` 却说它不在"
    );

    let after = crate::files::index::tests::query("born-after-the-walk", 10);
    assert_eq!(
        after.total_hits,
        1,
        "🔴 挂着 watch 的目录里新建的文件搜不到 —— 那「保鲜」这一档就只剩重走那一半，\n\
         而重走那一半的延迟是 {} 秒",
        crate::files::index::REWALK_INTERVAL_SECS
    );
    assert_eq!(
        after.index_age_secs, before.index_age_secs,
        "索引本身不该因为一次重列而变新 —— 那个年龄是「整棵树上次走完是多久以前」"
    );
}

/// ★★ 反向那半：**遮盖真的在生效**。
///
/// 不钉这一格的话，把 `supersedes` 改成恒 `false` 之后上面那条**照样绿**
/// （overlay 里那一条会被当成新增命中），而「盘上已经删掉的文件不再出现在结果里」
/// 这条性质一声不吭地没了。
#[test]
fn an_entry_deleted_on_disk_stops_being_returned_once_its_directory_is_relisted() {
    let _lock = resident_lock();
    let fx = make_tree("supersede", 2, 3, 0);
    let dir = fx.root.join("d0000");
    let doomed = dir.join("about-to-be-deleted");
    std::fs::File::create(&doomed).expect("造那个待删的文件");

    // 🔴 这个位是**进程级**的，而 `cargo test` 默认并行 ⇒ 先串行（理由住 `index_testing.rs`）
    let _serial = crate::files::index::testing::serial();
    crate::files::index::rebuild_once(&fx.root)
        .expect("本格独占跑，抢不到那个位就是并发保护写错了 —— 不许静默当成走过了");
    let before = crate::files::index::tests::query("about-to-be-deleted", 10);
    assert_eq!(before.total_hits, 1, "它本该在索引里 —— 夹具坏了");

    set_browsing(&[dir.clone()]);
    std::fs::remove_file(&doomed).expect("删掉它");
    assert!(on_change(crate::files::raw::path_bytes(&dir)));

    let after = crate::files::index::tests::query("about-to-be-deleted", 10);
    assert_eq!(
        after.total_hits, 0,
        "盘上已经没有的文件还在结果里 —— overlay 没有盖住大索引里的那一条"
    );
    // 同一趟里，那个目录的**其余**条目仍然在（遮盖的粒度是「直接子项」，不是整棵子树）。
    let siblings = crate::files::index::tests::query("f0000", 10);
    assert_eq!(
        siblings.total_hits, 2,
        "遮盖把不该盖的也盖了（或者少盖了）—— 两个目录各有一个 f0000"
    );
}

#[test]
fn an_event_outside_the_browse_list_changes_nothing() {
    let _lock = resident_lock();
    let fx = make_tree("stranger", 2, 1, 0);
    set_browsing(&[fx.root.join("d0000")]);
    assert!(
        !on_change(crate::files::raw::path_bytes(&fx.root.join("d0001"))),
        "不在名单上的目录也被当成自己的事处理了 —— 那会让 overlay 无边界地长"
    );
}

#[test]
fn the_event_path_maps_back_to_the_watched_directory() {
    let _lock = resident_lock();
    let fx = make_tree("mapback", 1, 1, 0);
    let dir = fx.root.join("d0000");
    set_browsing(&[dir.clone()]);
    let dir_bytes = crate::files::raw::path_bytes(&dir).to_vec();
    // 事件给目录自己。
    assert_eq!(dir_for_event(&dir_bytes), Some(dir_bytes.clone()));
    // 事件给目录里的一个文件。
    let child = crate::files::raw::path_bytes(&dir.join("f0000")).to_vec();
    assert_eq!(dir_for_event(&child), Some(dir_bytes));
    // 事件给别处。
    assert_eq!(
        dir_for_event(crate::files::raw::path_bytes(&fx.root)),
        None,
        "名单外的路径被认成了名单上的目录"
    );
}

/// 🔴 **裸 `notify` 那一跳真的挂得上、事件真的推过来。**
///
/// 上面几条用 [`on_change`] 代替「事件到了」——那是把**接线**这一跳整个跳过去了，
/// 而本仓的纪律逐字：判据不在执行链上就等于不存在。本条把那一跳补上。
///
/// ⚠ 它**等**事件（有界轮询，不是无界）。等不到就红，并把「挂不上」与「挂上了但没推」
/// 分开报 —— 那两件事的处置完全不同。
#[test]
fn the_real_watcher_arms_and_delivers() {
    let _lock = resident_lock();
    let fx = make_tree("realwatch", 1, 1, 0);
    let dir = fx.root.join("d0000");
    set_browsing(&[dir.clone()]);

    let mut w = match BrowseWatcher::start() {
        Ok(w) => w,
        Err(e) => panic!(
            "起不来一个监听器：{e}\n\
             ⇒ 这不是「本机没有 inotify」就能带过的：保鲜那一半整个压在这一跳上。"
        ),
    };
    let (armed, failures) = w.arm();
    assert!(
        failures.is_empty(),
        "watch 挂不上（这与「挂上了但事件没来」是两件事）：{failures:?}"
    );
    assert_eq!(armed, 1, "名单上一个目录，挂上的个数不是 1");

    // 🔴 这个位是**进程级**的，而 `cargo test` 默认并行 ⇒ 先串行（理由住 `index_testing.rs`）
    let _serial = crate::files::index::testing::serial();
    crate::files::index::rebuild_once(&fx.root)
        .expect("本格独占跑，抢不到那个位就是并发保护写错了 —— 不许静默当成走过了");
    std::fs::File::create(dir.join("pushed-by-the-kernel")).expect("造那个文件");

    // 有界等待：每 20 毫秒看一眼，最多 5 秒。
    let mut found = false;
    for _ in 0..250 {
        let r = crate::files::index::tests::query("pushed-by-the-kernel", 10);
        if r.total_hits == 1 {
            found = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        found,
        "watch 挂上了、文件也造了，5 秒之内那条事件没有变成一次重列。\n\
         ⇒ 要么回调没接上，要么事件被内核丢了（后者本判据分不出来，如实登记）。"
    );
}

/// 要求：「`browse_watch::BrowseWatcher` **零生产调用方** ⇒ `files-browse` 买到的是『发命令那一刻重列一遍』，
/// 不是『一有动静就跟着新』（要有人在后端进程里长期持有那个监听器）」＋仍开着第一条。
///
/// 走的是**线上那一面**（`files-browse`），不直接碰 `BrowseWatcher`：名单登记之后造一份文件，
/// 有界等待里 overlay 必须出现它 —— 没有那个长期持有的监听器，overlay 只有登记那一刻的重列，永远等不到。
/// 换名单之后 `watching` 两向跟着变（挂一个 ⇒ 1，空名单 ⇒ 0）。
#[test]
fn files_browse_keeps_a_live_watcher_that_follows_the_list() {
    let _lock = resident_lock();
    let fx = make_tree("livewatch", 2, 1, 0);
    let dir = fx.root.join("d0000");
    let p = |x: &std::path::Path| serde_json::Value::String(x.to_string_lossy().to_string());
    let v = crate::files::answer_wire("files-browse", &serde_json::json!({ "dirs": [p(&dir)] }))
        .expect("files-browse 被拒");
    assert_eq!(
        (
            v["watching"].as_u64(),
            v["watch_failed"].as_u64(),
            v["watch_error"].is_null()
        ),
        (Some(1), Some(0), true),
        "名单一个目录，真挂着的不是 1：{v}"
    );
    std::fs::File::create(dir.join("seen-live")).expect("造文件");
    let want = crate::files::raw::path_bytes(&dir.join("seen-live")).to_vec();
    let mut found = false;
    for _ in 0..250 {
        if overlay_snapshot().iter().any(|(e, _)| e == want.as_slice()) {
            found = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        found,
        "登记之后新建的文件 5 秒内没进 overlay —— 没有人在后端进程里持有那个监听器"
    );
    let v = crate::files::answer_wire("files-browse", &serde_json::json!({ "dirs": [] }))
        .expect("空名单被拒");
    assert_eq!(v["watching"].as_u64(), Some(0), "空名单之后还挂着：{v}");
}
