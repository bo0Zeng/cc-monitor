use super::testing::{painted_contains, painted_text};
use super::*;

use std::sync::atomic::{AtomicUsize, Ordering as O};

fn job() -> CopyJob {
    CopyJob {
        from: "/srv/data/big.bin".to_string(),
        to: "/srv/data/big.bin.copy".to_string(),
        name: "big.bin.copy".to_string(),
    }
}

fn row(name: &str, is_dir: bool, lossy: bool) -> Row {
    Row {
        name: name.to_string(),
        path: format!("/srv/data/{name}"),
        is_dir,
        size: 7,
        lossy_name: lossy,
    }
}

/// 🔴 **那句退路原文的样子** —— 逐字照 `sftp_pool::copy_remote_path` 结尾那个
/// `format!` 拼出来的形状（「为什么退」＋「⇒ 退回中转：N 字节经过了你这台机器」）。
///
/// ⚠ 这里是**合成**的一句，不是从那边 `include!` 过来的 —— 两处会不会漂，
/// 由秤 F3（`tests/bridge/sftp_copy_f3_tests.rs`，门禁 `f3-copy` 那一格）钉着；
/// 本文件钉的是**这一层不许改写它**，所以拿什么串进来不重要，
/// **出去的必须逐字含着它**。
fn slow_path_words() -> String {
    "远端的 sftp-server 握手时没报 `copy-data` 扩展（或修订号不是 1）\
     ⇒ 退回中转：8388608 字节经过了你这台机器（零流量复制没走上）"
        .to_string()
}

/// 一台**时序台架**（形状照 `transfer_tests::Tape`）：谁在什么号上发生了什么。
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
    fn seq_of(&self, what: &str) -> Option<usize> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .find(|(_, w)| w == what)
            .map(|(n, _)| *n)
    }
}

// ════════════════════════════════════════════════════════════════════════
// 正题一：**三段的顺序就是函数结构**（照 `run_drop` 的形状办）
// ════════════════════════════════════════════════════════════════════════

/// 🔴 探测 → **一次**问覆盖 → 才动手。三样一起判，少一样就漏一形：
/// ① `confirm` 恰好一次（`FnOnce` 守住了「不许第二次」，运行时这一条守「不许零次」）；
/// ② 探测的号 < 确认的号 < 动手的号（顺序，不是巧合）；
/// ③ 它拿到的就是**那一件**（相等断言，不是「非空」）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_probe_and_the_question_both_come_before_anything_is_copied() {
    let tape = Tape::default();
    let asked = AtomicUsize::new(0);
    let seen: std::sync::Mutex<Option<CopyJob>> = std::sync::Mutex::new(None);

    let out = run_copy(
        job(),
        |j| {
            let t = &tape;
            async move {
                t.mark("probe");
                assert_eq!(j, job());
                true // 目标已经在
            }
        },
        |j| {
            asked.fetch_add(1, O::SeqCst);
            tape.mark("confirm");
            *seen.lock().unwrap() = Some(j);
            async move { true } // 人答「覆盖」
        },
        |_| {
            let t = &tape;
            async move {
                t.mark("launch");
                Ok(None)
            }
        },
    )
    .await;

    assert_eq!(
        asked.load(O::SeqCst),
        1,
        "覆盖确认被问了 {} 次 —— 要的是**一次问完**",
        asked.load(O::SeqCst)
    );
    assert_eq!(
        *seen.lock().unwrap(),
        Some(job()),
        "问的时候摆出来的不是那一件"
    );

    let (p, c, l) = (
        tape.seq_of("probe").expect("一次都没探测"),
        tape.seq_of("confirm").expect("一次都没问"),
        tape.seq_of("launch").expect("一趟都没起"),
    );
    assert!(
        p < c && c < l,
        "三段的号是 probe={p} confirm={c} launch={l} —— 顺序不对（要的是探测 → 问 → 动手）"
    );
    assert_eq!(
        out,
        CopyOutcome::Done {
            asked: true,
            verdict: None
        }
    );
}

/// 目标不存在 ⇒ **不问**。弹一个空框是噪音，不是慎重（同 `run_drop`）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_target_that_is_not_there_yet_asks_nobody() {
    let asked = AtomicUsize::new(0);
    let launched = AtomicUsize::new(0);
    let out = run_copy(
        job(),
        |_| async move { false },
        |_| {
            asked.fetch_add(1, O::SeqCst);
            async move { true }
        },
        |_| {
            launched.fetch_add(1, O::SeqCst);
            async move { Ok(None) }
        },
    )
    .await;
    assert_eq!(asked.load(O::SeqCst), 0, "目标本来就不在，却弹了覆盖确认框");
    assert_eq!(launched.load(O::SeqCst), 1);
    assert_eq!(
        out,
        CopyOutcome::Done {
            asked: false,
            verdict: None
        }
    );
}

/// 🔴 答「别覆盖」⇒ **一个字节都不动**（`launch` 一次都不许被调）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn answering_no_means_the_copy_never_starts() {
    let launched = AtomicUsize::new(0);
    let out = run_copy(
        job(),
        |_| async move { true },
        |_| async move { false },
        |_| {
            launched.fetch_add(1, O::SeqCst);
            async move { Ok(None) }
        },
    )
    .await;
    assert_eq!(
        launched.load(O::SeqCst),
        0,
        "人答了「别覆盖」，复制还是起来了 —— 那会盖掉一个他刚说不要动的文件"
    );
    assert_eq!(out, CopyOutcome::Skipped);
}

// ════════════════════════════════════════════════════════════════════════
// 正题二：**裁决不许被吞** —— 类型上、运行时、以及界面上
// ════════════════════════════════════════════════════════════════════════

/// 🔴 `sftp_pool` 那一层交上来的 [`CopyVerdict`] **原样**出现在结果里。
///
/// 两个方向各一趟（快路 `None` / 退路 `Some(原文)`）—— 只判一个方向的话，
/// 把 `verdict` 硬写成 `None` 的实现在快路那一趟上照样绿。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_verdict_from_the_pool_comes_back_untouched() {
    for want in [None, Some(slow_path_words())] {
        let w = want.clone();
        let out = run_copy(
            job(),
            |_| async move { false },
            |_| async move { true },
            move |_| async move { Ok(w) },
        )
        .await;
        assert_eq!(
            out,
            CopyOutcome::Done {
                asked: false,
                verdict: want.clone()
            },
            "裁决在这一层被改写了 —— 它是「这一趟走的是哪条路」的唯一来源"
        );
    }
}

/// 失败要带着原文出来，不许压成一句「复制失败」。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failure_comes_back_with_the_message_the_pool_gave() {
    let out = run_copy(
        job(),
        |_| async move { false },
        |_| async move { true },
        |_| async move { Err("远端 copy-data 失败（PermissionDenied）: 不许写".to_string()) },
    )
    .await;
    assert_eq!(
        out,
        CopyOutcome::Failed("远端 copy-data 失败（PermissionDenied）: 不许写".to_string())
    );
}

/// 🔴 **退路那句话要原样进到「要说给用户听的那一句」里，而且是警告档。**
///
/// 相等断言（不是「包含某个词」）：这一层的活就是把那句话套一个前缀，
/// 多改一个字都是在改写下层的读数。
#[test]
fn the_slow_path_notice_repeats_the_pools_own_words_verbatim() {
    let why = slow_path_words();
    let n = outcome_notice(&CopyOutcome::Done {
        asked: true,
        verdict: Some(why.clone()),
    });
    assert_eq!(n.text, format!("{SLOW_PATH_PREFIX}{why}"));
    assert!(n.loud, "退了路却不是警告档 —— 那一行会混在普通提示里");
    // 那句话里那个**字节数**必须一路带到界面上（`设计/60 §5` 第二段逐字要的就是它）。
    assert!(
        n.text.contains("8388608 字节经过了你这台机器"),
        "退路那句话里的过网字节数掉了：{}",
        n.text
    );
}

/// 🔴 **阴性对照**：快路**不喊**。
///
/// 没有这一条，上面那条可能只是「每一趟都摆一句警告」——
/// 那样「慢路」这个信号就等于没有（用户每次都看见同一行字）。
#[test]
fn the_fast_path_says_something_quiet_and_is_not_a_warning() {
    let n = outcome_notice(&CopyOutcome::Done {
        asked: false,
        verdict: None,
    });
    assert!(!n.loud, "零流量是它该有的样子，不是警告");
    assert!(
        !n.text.contains(SLOW_PATH_PREFIX),
        "快路那一趟也说了「走的是慢路」：{}",
        n.text
    );
}

/// 🔴 **这一刀最要紧的一条：那句话真的被画到窗口上了。**
///
/// 它走的是**生产那个** [`CopyBoard::ui`]，从 egui 这一帧交出去的 galley 里
/// 把文字读回来（量具与它买不到什么见 `copy_testing.rs` 头注）。
///
/// ⚠ 与上面那两条 `outcome_notice` 判据**买的不是同一样东西**
/// （`真相源/99 §9.5` 刀 2 复打记的正是这一形）：
/// 那两条买「这句话拼得对」，**看不见**「`ui()` 里那一支被 `if false` 关掉了」；
/// 这一条买「这一帧真的把它交出去排版了」，**看不见**「话拼得对不对」。两条都要。
///
/// 同一把尺子带一条**阴性对照**：快路那一趟，它必须**读不到**那个前缀。
#[test]
fn the_slow_path_notice_really_gets_painted_on_the_window() {
    let why = slow_path_words();
    let screen = egui::vec2(1280.0, 800.0);

    // ── 退路那一趟：画得出来 ──────────────────────────────────────────
    let ctx = egui::Context::default();
    let board = CopyBoard::default();
    board.finish(CopyOutcome::Done {
        asked: true,
        verdict: Some(why.clone()),
    });
    // 第一帧建字体图集，第二帧才是稳定的读数。
    let _ = painted_text(&ctx, screen, 0.0, Vec::new(), |ui| board.ui(ui));
    let slow = painted_text(&ctx, screen, 0.1, Vec::new(), |ui| board.ui(ui));
    assert!(
        !slow.is_empty(),
        "这一帧一个字都没画出来 —— 量具塌了，下面两比在空转"
    );
    assert!(
        painted_contains(&slow, SLOW_PATH_PREFIX),
        "退了路，窗口上却找不到「{SLOW_PATH_PREFIX}」这句话 —— \
         那就是 `设计/60 §5` 第二段禁止的那一形：静默花掉 2× 带宽。\n\
         这一帧画出来的是：{:?}",
        slow.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>()
    );
    assert!(
        painted_contains(&slow, "8388608 字节经过了你这台机器"),
        "「多少字节过了这台机器」没画出来 —— 只说「慢路」不说数，等于没说"
    );

    // ── 🔴 阴性对照：快路那一趟，同一把尺子读不到 ─────────────────────
    let ctx2 = egui::Context::default();
    let fast = CopyBoard::default();
    fast.finish(CopyOutcome::Done {
        asked: false,
        verdict: None,
    });
    let _ = painted_text(&ctx2, screen, 0.0, Vec::new(), |ui| fast.ui(ui));
    let quiet = painted_text(&ctx2, screen, 0.1, Vec::new(), |ui| fast.ui(ui));
    assert!(
        !quiet.is_empty(),
        "对照组这一帧一个字都没画 —— 那下面这一比恒真"
    );
    assert!(
        !painted_contains(&quiet, SLOW_PATH_PREFIX),
        "零流量那一趟也喊了「走的是慢路」—— 这把尺子恒真，上面那一比买不到东西"
    );
}

// ════════════════════════════════════════════════════════════════════════
// 看板：问一次 · 敲窗口
// ════════════════════════════════════════════════════════════════════════

/// 覆盖那一问**只答得了一次**（`oneshot` ＋ 把 `answer` 取走）。
/// 重复点第二下不许再送一次 —— 那一头已经没人听了。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_overwrite_question_is_answered_exactly_once() {
    let board = CopyBoard::default();
    let rx = board.ask(job());
    assert!(board.is_asking());
    assert!(board.settle(true), "第一次答复没送出去");
    assert!(!board.is_asking(), "答完了还摆在那儿");
    assert!(!board.settle(false), "同一问答了第二次");
    assert!(rx.await.unwrap(), "送过去的答复不是那一个");
}

/// 🔴 **会被盖掉的是目标，不是源。**
///
/// 探成 `from` 的话源一定存在 ⇒ 每一趟都弹一次覆盖确认，而真正会被盖掉的
/// 那个目标**一次都没被问过**。那一形在本机看不见（`probe_target` 要真连接），
/// 所以把它压到 [`CopyJob::overwrite_target`] 这一条相等断言上。
#[test]
fn the_path_we_probe_is_the_one_that_would_get_overwritten() {
    let j = job();
    assert_eq!(j.overwrite_target(), j.to);
    assert_ne!(
        j.overwrite_target(),
        j.from,
        "探的是源 —— 源当然在，于是每趟都白问一次"
    );
}

/// 🔴 egui **只在有事发生时才画下一帧** ⇒ 三个时刻都要敲窗口：
/// **有问题要问** · **进度动了** · **跑完了**。
///
/// ⚠ 判的是 `Context::has_requested_repaint()`（egui 自己那个标志，＝ 行为），
/// 不是「源码里有 `request_repaint` 这行字」。
/// ⚠ 反空真：新建的 `Context` 头几帧自己就在要求重画 ⇒ **先把那个标志跑静**；
/// 跑不静就报「这把尺子在这台机器上判不了」，**不报「过了」**（同 `transfer_tests`）。
#[test]
fn the_copy_board_pokes_the_window_whenever_something_happened() {
    for (what, act) in [
        (
            "有问题要问",
            Box::new(|b: &CopyBoard| {
                b.ask(job());
            }) as Box<dyn Fn(&CopyBoard)>,
        ),
        (
            "进度动了",
            Box::new(|b: &CopyBoard| b.progress("big.bin.copy", 1, 2)) as Box<dyn Fn(&CopyBoard)>,
        ),
        (
            "跑完了",
            Box::new(|b: &CopyBoard| {
                b.finish(CopyOutcome::Done {
                    asked: false,
                    verdict: None,
                })
            }) as Box<dyn Fn(&CopyBoard)>,
        ),
    ] {
        let ctx = egui::Context::default();
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
        assert!(
            settled > 0,
            "跑了 16 帧那个「要求重画」的标志还没静下来 —— \
             这把尺子在这台机器上量不了（`{what}` 这一格**判不了**，不是过了）"
        );

        let board = CopyBoard::default();
        board.attach(Some(ctx.clone()));
        act(&board);
        assert!(
            ctx.has_requested_repaint(),
            "`{what}` 之后没敲窗口 —— 那一格在屏幕上要等用户动鼠标才更新"
        );
    }
}

/// 反空真：没有窗口的时候它照常记数、不炸。
#[test]
fn a_copy_board_with_no_window_still_records_and_does_not_panic() {
    let board = CopyBoard::default();
    board.progress("x", 3, 9);
    board.finish(CopyOutcome::Skipped);
    assert_eq!(board.rounds(), 1);
    assert_eq!(board.last(), Some(CopyOutcome::Skipped));
}

// ════════════════════════════════════════════════════════════════════════
// 名字：**只在同一个目录里改名**
// ════════════════════════════════════════════════════════════════════════

#[test]
fn a_copy_job_is_a_rename_inside_the_same_directory() {
    let got = CopyJob::beside("/srv/data/big.bin", "/srv/data", "big.bin.copy").unwrap();
    assert_eq!(got.to, "/srv/data/big.bin.copy");
    assert_eq!(got.name, "big.bin.copy");
    assert_eq!(got.from, "/srv/data/big.bin");
    // 目录结尾已经有斜杠也不许拼出 `//`。
    assert_eq!(
        CopyJob::beside("/srv/data/big.bin", "/srv/data/", "x")
            .unwrap()
            .to,
        "/srv/data/x"
    );
    // 根目录。
    assert_eq!(CopyJob::beside("/big.bin", "/", "x").unwrap().to, "/x");
    // 前后空白剪掉（框里很容易多敲一个空格）。
    assert_eq!(
        CopyJob::beside("/srv/a", "/srv", "  b  ").unwrap().name,
        "b"
    );

    // 三档 `None`，**一个都不许兜底**：
    assert!(
        CopyJob::beside("/srv/a", "/srv", "   ").is_none(),
        "空名字过了"
    );
    assert!(
        CopyJob::beside("/srv/a", "/srv", "../../etc/x").is_none(),
        "名字里带 `/` 过了 —— 那会把文件放到他没在看的目录里"
    );
    assert!(
        CopyJob::beside("/srv/a", "/srv", "a").is_none(),
        "复制成自己过了 —— `copy_remote_path` 会先删 `to` 再换名，那等于把源删了"
    );
}

/// 缺省名与旧面板同一套（`panel.ts::copyFile` 的 `${e.name}.copy`）。
#[test]
fn the_suggested_name_matches_the_old_panel() {
    assert_eq!(CopyPrompt::suggest("big.bin"), "big.bin.copy");
    let p = CopyPrompt::for_row("/srv/data", &row("big.bin", false, false));
    assert_eq!(p.from, "/srv/data/big.bin");
    assert_eq!(p.dir, "/srv/data");
    assert_eq!(p.new_name, "big.bin.copy");
    assert_eq!(p.to_job().unwrap().to, "/srv/data/big.bin.copy");
}

/// 能复制的只有「普通文件 ＋ 名字寻址得到」那一档。**相等断言，逐档。**
#[test]
fn only_plain_files_with_addressable_names_can_be_copied() {
    assert!(is_copyable(&row("big.bin", false, false)));
    assert!(
        !is_copyable(&row("adir", true, false)),
        "目录能复制 —— `copy-data` 吃的是文件句柄，目录递归不在这一层"
    );
    assert!(
        !is_copyable(&row("\u{FFFD}odd", false, true)),
        "有损名能复制 —— 那个名字寻址不到真字节，写操作一律灰置"
    );
}

// ════════════════════════════════════════════════════════════════════════
// 委派：**不许自己拼传输，也不许绕过那条池的入口**
// ════════════════════════════════════════════════════════════════════════

/// ⚠ **判源码是代理，不是标的**（同 `transfer_tests` / `source_tests` 的如实标注）。
///
/// 买的是：复制走的是 `sftp_pool::sftp_copy` 那条既有命令 ——
/// 于是 `guard_write` 两道围栏、`register_cancel`、`lease_raw`（车道 ＋ 通道预算）
/// 全都照旧生效，本模块一行传输代码都没有。
/// 买不到：那条命令今天真连得上。
#[test]
fn the_real_adapter_delegates_to_the_pools_own_copy_command() {
    let prod = guard_core::production_code(include_str!("../../../src/bridge/src/filewin/copy.rs"));
    guard_core::assert_no_test_code("filewin/copy.rs", &prod);
    assert!(
        prod.len() > 3_000,
        "生产段只剩 {} 字节 —— 剥法把它剥没了，下面几条在空转",
        prod.len()
    );
    for needle in ["sftp_pool::sftp_copy(", "transfer::probe_remote("] {
        assert_eq!(
            prod.matches(needle).count(),
            1,
            "`{needle}` 在生产段里出现 {} 次（应当恰好 1 次）—— \
             多了就是长出了第二条路，少了就是这一条被换掉了",
            prod.matches(needle).count()
        );
    }
    // 🔴 自己开连接 / 自己借裸通道 / 绕过那条入口直呼核心，一个都不许有。
    //    `copy_remote_path(` 尤其要挡：绕过 `sftp_copy` 就等于同时丢掉
    //    两道 `guard_write`（Claude 数据源围栏）与 `register_cancel`（取消登记）。
    for banned in [
        "connect_sftp",
        "russh_sftp",
        "SftpSession",
        "with_sftp(",
        "lease_raw",
        "lease_transfer",
        "copy_remote_path(",
        "guard_write(",
        // 探测「目标在不在」只许借 `transfer::probe_remote`，不许在这一层
        // 再拨一次 `sftp_stat`（「stat 失败算不算存在」那一档会漂）。
        "sftp_pool::sftp_stat(",
    ] {
        assert!(
            !prod.contains(banned),
            "生产段里出现了 `{banned}` —— 这一层不许自己碰连接／通道／围栏，\
             它该做的只有「问一次、起一趟、把裁决摆出来」那三件"
        );
    }
    // 反空真：这把尺子认得出「有」。
    assert!(prod.contains("sftp_pool::sftp_copy("));
}
