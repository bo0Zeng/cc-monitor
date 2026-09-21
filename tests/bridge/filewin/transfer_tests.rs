use super::*;

use std::sync::atomic::{AtomicUsize, Ordering as O};

fn p(name: &str) -> Pending {
    Pending {
        local_path: format!("/home/u/{name}"),
        remote_path: format!("/srv/{name}"),
        name: name.to_string(),
    }
}

/// 一台**时序台架**：谁在什么号上发生了什么。
///
/// 🔴 判「先问完再动手」靠的是**号的先后**，不是 `sleep` 之后看看 ——
/// 后者是时序赌博，换台机器就翻。
#[derive(Default)]
struct Tape {
    seq: AtomicUsize,
    events: std::sync::Mutex<Vec<(usize, String)>>,
}

impl Tape {
    fn mark(&self, what: &str) -> usize {
        let n = self.seq.fetch_add(1, O::SeqCst);
        self.events.lock().unwrap().push((n, what.to_string()));
        n
    }
    fn seqs_of(&self, prefix: &str) -> Vec<usize> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, w)| w.starts_with(prefix))
            .map(|(n, _)| *n)
            .collect()
    }
}

// ════════════════════════════════════════════════════════════════════════
// 正题一：**一次问完** —— 问一次、拿到全部冲突项、而且排在所有传输之前
// ════════════════════════════════════════════════════════════════════════

/// 🔴 `设计/60 §5.4d` 的前半句：**先把覆盖确认一次问完。**
///
/// 三样一起判，少一样就漏一形：
/// ① `confirm` 被调用**恰好一次**（旧面板那种 `for … confirm … await` 会调 N 次）；
/// ② 它拿到的是**全部**冲突项（相等断言，不是「至少包含」）；
/// ③ **任何一次** `launch` 的号都大于那一次 `confirm` 的号（顺序，不是巧合）。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_overwrite_question_is_asked_once_and_before_any_transfer_starts() {
    let tape = Tape::default();
    let confirm_calls = AtomicUsize::new(0);
    // a / c 在远端已经有了，b / d 没有。
    let items = vec![p("a"), p("b"), p("c"), p("d")];
    let seen: std::sync::Mutex<Vec<Pending>> = std::sync::Mutex::new(Vec::new());

    let out = run_drop(
        items.clone(),
        4,
        |it| {
            let t = &tape;
            async move {
                t.mark(&format!("probe:{}", it.name));
                it.name == "a" || it.name == "c"
            }
        },
        |clashes| {
            confirm_calls.fetch_add(1, O::SeqCst);
            tape.mark("confirm");
            *seen.lock().unwrap() = clashes.clone();
            // 人答「两件都覆盖」。
            async move { clashes }
        },
        |it| {
            let t = &tape;
            async move {
                t.mark(&format!("launch:{}", it.name));
                Ok(())
            }
        },
    )
    .await;

    // ① 恰好一次。
    assert_eq!(
        confirm_calls.load(O::SeqCst),
        1,
        "覆盖确认被问了 {} 次 —— `§5.4d` 要的是**一次问完**（旧面板那条 `for … await` 就是 N 次）",
        confirm_calls.load(O::SeqCst)
    );

    // ② 全部冲突项，逐项相等。
    assert_eq!(
        seen.lock()
            .unwrap()
            .iter()
            .map(|x| x.name.clone())
            .collect::<Vec<_>>(),
        vec!["a".to_string(), "c".to_string()],
        "那一次问的时候，摆在人面前的不是全部冲突项"
    );

    // ③ 顺序：最后一次 confirm 之后才有第一次 launch。
    let confirm_at = *tape.seqs_of("confirm").first().expect("一次都没问");
    let launches = tape.seqs_of("launch:");
    assert_eq!(
        launches.len(),
        4,
        "四件都准传了，却只起了 {} 趟",
        launches.len()
    );
    let first_launch = *launches.iter().min().unwrap();
    assert!(
        first_launch > confirm_at,
        "第一趟传输在号 {first_launch} 就起来了，而那一次确认在号 {confirm_at} \
         —— 有传输跑在「问完」之前"
    );
    // 反向：所有 probe 都在 confirm 之前（「问」这一段自己也是完整的）。
    let probes = tape.seqs_of("probe:");
    assert_eq!(probes.len(), 4);
    assert!(
        probes.iter().all(|n| *n < confirm_at),
        "有一次「远端已经有了吗」是在确认之后才问的"
    );

    assert_eq!(
        out,
        DropOutcome {
            asked: 2,
            skipped: 0,
            ok: 4,
            failed: Vec::new()
        }
    );
}

/// 人答「全都不覆盖」⇒ 冲突那几件**一件都不传**，不冲突那几件照传。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn answering_no_skips_exactly_the_clashing_items() {
    let launched: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let out = run_drop(
        vec![p("a"), p("b"), p("c")],
        4,
        |it| async move { it.name != "b" },   // a / c 冲突
        |_clashes| async move { Vec::new() }, // 全都不覆盖
        |it| {
            let l = &launched;
            async move {
                l.lock().unwrap().push(it.name.clone());
                Ok(())
            }
        },
    )
    .await;
    assert_eq!(
        launched.lock().unwrap().clone(),
        vec!["b".to_string()],
        "答了「不覆盖」，冲突那几件还是传了"
    );
    assert_eq!(out.asked, 2);
    assert_eq!(out.skipped, 2);
    assert_eq!(out.ok, 1);
}

/// 一件都不冲突 ⇒ **不问**。弹一个空框是噪音，不是慎重。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn nothing_clashing_means_nobody_gets_asked() {
    let asked = AtomicUsize::new(0);
    let out = run_drop(
        vec![p("a"), p("b")],
        4,
        |_| async move { false },
        |c| {
            asked.fetch_add(1, O::SeqCst);
            async move { c }
        },
        |_| async move { Ok(()) },
    )
    .await;
    assert_eq!(asked.load(O::SeqCst), 0, "没有冲突却弹了确认框");
    assert_eq!(out.asked, 0);
    assert_eq!(out.ok, 2);
}

// ════════════════════════════════════════════════════════════════════════
// 正题二：**再并行起传输** —— 而且那道闸是真的
// ════════════════════════════════════════════════════════════════════════

/// 🔴 `设计/60 §5.4d` 的后半句：**并行起传输。**
///
/// 判法是**结构性**的，不是计时：四件传输各自 `Barrier(4).wait()` ——
/// 只有四件**真的同时在飞**barrier 才凑得齐。串行实现（旧面板那条 `for … await`）
/// 第一件就永远等不到第二件 ⇒ 超时，而超时在这条判据里就是红。
///
/// ⚠ 顺带用一个高水位计数器印出**真实的**同时在飞数，并按**相等**判 4
/// （不是 `>= 2`：`>= 2` 在「只并行了两件」时照样绿）。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn four_dropped_files_really_fly_at_the_same_time() {
    let gate = std::sync::Arc::new(tokio::sync::Barrier::new(4));
    let inflight = std::sync::Arc::new(AtomicUsize::new(0));
    let high = std::sync::Arc::new(AtomicUsize::new(0));

    let g = gate.clone();
    let f = inflight.clone();
    let h = high.clone();
    let fut = run_drop(
        vec![p("a"), p("b"), p("c"), p("d")],
        4,
        |_| async move { false },
        |c| async move { c },
        move |_| {
            let g = g.clone();
            let f = f.clone();
            let h = h.clone();
            async move {
                let n = f.fetch_add(1, O::SeqCst) + 1;
                h.fetch_max(n, O::SeqCst);
                // 四件不凑齐就谁也过不去。
                g.wait().await;
                f.fetch_sub(1, O::SeqCst);
                Ok(())
            }
        },
    );

    let out = tokio::time::timeout(std::time::Duration::from_secs(10), fut)
        .await
        .expect(
            "四件传输没能同时在飞（`Barrier(4)` 凑不齐 ⇒ 超时）—— \
             这正是旧面板 `uploadDropped` 那条 `for … await` 的形状，\
             而 `设计/60 §5.4d` 要的就是把它去掉",
        );
    assert_eq!(out.ok, 4);
    assert_eq!(
        high.load(O::SeqCst),
        4,
        "同时在飞的最高水位是 {}，应当是 4",
        high.load(O::SeqCst)
    );
}

/// 🔴 **阴性对照：那道闸是真的闸。**
///
/// 没有这一条，上面那条「四件同飞」可能只是因为 `lanes` 压根没被看过 ——
/// 一个不生效的闸在「四件同飞」这个方向上表现得和生效的一模一样。
/// ⇒ 把闸调到 2，同一个 `Barrier(4)` 必须**凑不齐**（超时）。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_lane_cap_actually_caps() {
    let gate = std::sync::Arc::new(tokio::sync::Barrier::new(4));
    let g = gate.clone();
    let fut = run_drop(
        vec![p("a"), p("b"), p("c"), p("d")],
        2, // ← 闸窄于 barrier 要的人数
        |_| async move { false },
        |c| async move { c },
        move |_| {
            let g = g.clone();
            async move {
                g.wait().await;
                Ok(())
            }
        },
    );
    let r = tokio::time::timeout(std::time::Duration::from_millis(600), fut).await;
    assert!(
        r.is_err(),
        "`lanes = 2` 竟然放了 4 件同时进去 —— 那个参数没人看，上面那条并行判据是空的"
    );
}

/// 失败的那几件要**逐件带着原文**出来，不许合并成一句「有几件失败」。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn failures_come_back_named_and_with_their_own_message() {
    let out = run_drop(
        vec![p("a"), p("b")],
        4,
        |_| async move { false },
        |c| async move { c },
        |it| async move {
            if it.name == "a" {
                Err("对面盘满了".into())
            } else {
                Ok(())
            }
        },
    )
    .await;
    assert_eq!(out.ok, 1);
    assert_eq!(
        out.failed,
        vec![("a".to_string(), "对面盘满了".to_string())]
    );
}

/// 空拖入 ⇒ 空结果，一个探测都不发。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_empty_drop_touches_nothing() {
    let touched = AtomicUsize::new(0);
    let out = run_drop(
        Vec::new(),
        4,
        |_| {
            touched.fetch_add(1, O::SeqCst);
            async move { false }
        },
        |c| async move { c },
        |_| {
            touched.fetch_add(1, O::SeqCst);
            async move { Ok(()) }
        },
    )
    .await;
    assert_eq!(touched.load(O::SeqCst), 0);
    assert_eq!(out, DropOutcome::default());
}

/// 🔴 **保序**：答案要配回原来那一件。
///
/// 失效形状最难看：用 `buffer_unordered` 之后 `items.zip(flags)` 把 A 的「已存在」
/// 配到 B 头上 ⇒ **覆盖了一个不该覆盖的文件**。这里让探测的耗时与顺序**反着来**
/// （第一件最慢），乱序实现会把 flag 配错。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_probe_answers_stay_glued_to_the_item_that_asked() {
    let seen: std::sync::Mutex<Vec<Pending>> = std::sync::Mutex::new(Vec::new());
    let items = vec![p("slow-and-clashing"), p("fast-and-fresh")];
    run_drop(
        items,
        4,
        |it| async move {
            if it.name.starts_with("slow") {
                tokio::time::sleep(std::time::Duration::from_millis(120)).await;
                true
            } else {
                false
            }
        },
        |clashes| {
            *seen.lock().unwrap() = clashes.clone();
            async move { clashes }
        },
        |_| async move { Ok(()) },
    )
    .await;
    assert_eq!(
        seen.lock()
            .unwrap()
            .iter()
            .map(|x| x.name.clone())
            .collect::<Vec<_>>(),
        vec!["slow-and-clashing".to_string()],
        "冲突标记配错了人 —— 那意味着会覆盖一个不该覆盖的文件"
    );
}

// ════════════════════════════════════════════════════════════════════════
// 看板：**有事发生就得敲窗口一下**
// ════════════════════════════════════════════════════════════════════════

/// 🔴 egui **只在有事发生时才画下一帧**。进度是从 tokio 那条线程写进来的
/// ⇒ 不敲一下，进度条要等到用户下次动鼠标才跳一格 —— 看起来就是「卡住了」，
/// 而「卡住了」与「真的没在传」在屏幕上分不开。
///
/// 三个时刻都要敲：**有问题要问** · **进度动了** · **跑完了**。
/// ⚠ 判的是 `Context::has_requested_repaint()`（egui 自己那个标志），
/// 不是「我们调了 `request_repaint`」—— 后者是判源码，前者是判行为。
#[test]
fn the_board_pokes_the_window_whenever_something_happened() {
    for (what, act) in [
        (
            "有问题要问",
            Box::new(|b: &DropBoard| {
                b.ask(vec![p("a")]);
            }) as Box<dyn Fn(&DropBoard)>,
        ),
        (
            "进度动了",
            Box::new(|b: &DropBoard| b.progress("a", 1, 2)) as Box<dyn Fn(&DropBoard)>,
        ),
        (
            "跑完了",
            Box::new(|b: &DropBoard| b.finish(DropOutcome::default())) as Box<dyn Fn(&DropBoard)>,
        ),
    ] {
        let ctx = egui::Context::default();
        // 🔴 **先把那个标志跑静**。新建的 `Context` 头几帧自己就在要求重画
        //    （建字体图集、动画那一族），不跑静的话下面那条断言恒真 ⇒ 空真。
        //    ⚠ 现打：第 1 帧之后仍然是 true，跑到第 3 帧才静下来。
        let mut settled = 0usize;
        for f in 1..=16 {
            ctx.run_ui(egui::RawInput::default(), |_ui| {})
                .drop_without_applying_deltas();
            if !ctx.has_requested_repaint() {
                settled = f;
                break;
            }
        }
        println!("  `{what}`：跑静那个标志用了 {settled} 帧");
        let settled = settled > 0;
        assert!(
            settled,
            "跑了 16 帧那个「要求重画」的标志还没静下来 —— \
             这把尺子在这台机器上量不了「是不是我们敲的」（`{what}` 这一格判不了，不是过了）"
        );

        let board = DropBoard::default();
        board.attach(Some(ctx.clone()));
        act(&board);
        assert!(
            ctx.has_requested_repaint(),
            "`{what}` 之后没敲窗口 —— 那一格在屏幕上要等用户动鼠标才更新"
        );
    }
}

/// 反空真：**没有窗口的时候它照常记数、不炸**（判据里就是这个形状）。
#[test]
fn a_board_with_no_window_still_records_and_does_not_panic() {
    let board = DropBoard::default();
    board.progress("a", 3, 9);
    board.finish(DropOutcome {
        asked: 0,
        skipped: 0,
        ok: 1,
        failed: Vec::new(),
    });
    assert_eq!(board.rounds(), 1);
    assert_eq!(board.last().map(|o| o.ok), Some(1));
}

// ════════════════════════════════════════════════════════════════════════
// 路径拼法
// ════════════════════════════════════════════════════════════════════════

#[test]
fn a_pending_upload_targets_the_current_remote_directory_with_forward_slashes() {
    let got = Pending::into_remote_dir("/home/u/x.bin", "/srv/data").unwrap();
    assert_eq!(got.remote_path, "/srv/data/x.bin");
    assert_eq!(got.name, "x.bin");
    // 结尾已经有斜杠也不许拼出 `//`。
    assert_eq!(
        Pending::into_remote_dir("/home/u/x.bin", "/srv/data/")
            .unwrap()
            .remote_path,
        "/srv/data/x.bin"
    );
    // 根目录。
    assert_eq!(
        Pending::into_remote_dir("/home/u/x.bin", "/")
            .unwrap()
            .remote_path,
        "/x.bin"
    );
    // 拿不到名字 ⇒ `None`，**不编一个出来**。
    assert!(Pending::into_remote_dir("/", "/srv").is_none());
    assert!(Pending::into_remote_dir("", "/srv").is_none());
}

// ════════════════════════════════════════════════════════════════════════
// 委派：**不许自己写一条传输**
// ════════════════════════════════════════════════════════════════════════

/// ⚠ **判源码是代理，不是标的**（同 `source_tests` 那条的如实标注）。
///
/// 买的是：上传与探测走的是 `sftp_pool` 那两条既有命令 ——
/// 于是 4 条传输车道、取消登记、断点续传全都照旧生效，本模块一行传输代码都没有。
/// 买不到：那两条命令今天真连得上。
#[test]
fn the_real_adapters_delegate_to_the_shared_pool() {
    let prod =
        guard_core::production_code(include_str!("../../../src/bridge/src/filewin/transfer.rs"));
    assert!(
        prod.len() > 3_000,
        "生产段只剩 {} 字节 —— 剥法把它剥没了，下面几条在空转",
        prod.len()
    );
    for needle in [
        "sftp_pool::sftp_stat(",
        "sftp_pool::sftp_upload(",
        "sftp_pool::TRANSFER_LANE_CAP",
        "pub fn lanes()",
    ] {
        assert_eq!(
            prod.matches(needle).count(),
            1,
            "`{needle}` 在生产段里出现 {} 次（应当恰好 1 次）—— \
             多了就是长出了第二条路，少了就是这一条被换掉了",
            prod.matches(needle).count()
        );
    }
    // 自己开连接 / 自己分块写的痕迹，一个都不许有。
    for banned in [
        "connect_sftp",
        "russh_sftp",
        "SftpSession",
        "with_sftp(",
        "lease_transfer",
    ] {
        assert!(
            !prod.contains(banned),
            "生产段里出现了 `{banned}` —— 这一层不许自己碰连接／通道，\
             它该做的只有「先问完再并行」那件事"
        );
    }
}

/// 🔴 **闸的数值不许在这里另写一份。**
///
/// 真正的车道闸在池里（`lease_transfer`，`设计/60 §5.4a` 的 `6 − 4 = 2`）。
/// 本层那个并发上限**取的就是池里那个常量** —— 相等断言，而不是「注释里说是 4」。
#[test]
fn our_concurrency_cap_is_the_pools_own_lane_count() {
    assert_eq!(
        crate::sftp_pool::TRANSFER_LANE_CAP,
        4,
        "池里的车道数变了 —— 本层跟着它走，但这个数变了要回 `设计/60 §5.4a` 重读一遍理由"
    );
    // 本层那个落点回的就是池里那个数（相等，不是「注释里说是 4」）。
    assert_eq!(lanes(), crate::sftp_pool::TRANSFER_LANE_CAP);
    // 窗口那一侧**只经这一个落点**拿那个数，不自己再写一份。
    let shell =
        guard_core::production_code(include_str!("../../../src/bridge/src/filewin/shell.rs"));
    assert_eq!(
        shell.matches("transfer::lanes()").count(),
        1,
        "窗口那一侧起传输时用的不是 `transfer::lanes()`（或者用了两处）"
    );
    assert!(
        !shell.contains("TRANSFER_LANE_CAP"),
        "`shell.rs` 直接引了池里那个常量 —— 那就有两个地方在答「起几件」，\
         而两个地方一定会漂"
    );
}
