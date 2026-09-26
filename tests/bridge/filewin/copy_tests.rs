use super::testing::{painted_contains, painted_text};
use super::*;

use std::sync::atomic::{AtomicUsize, Ordering as O};

fn job() -> CopyJob {
    CopyJob {
        from: "/srv/data/big.bin".to_string(),
        to: "/srv/data/big.bin.copy".to_string(),
        name: "big.bin.copy".to_string(),
        is_dir: false,
        from_raw: None,
        to_raw: None,
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
        |_, overwrite| {
            let t = &tape;
            async move {
                t.mark("launch");
                // 〔F7a〕问过且答了「覆盖」⇒ 覆盖策略就是 `true`。
                assert!(overwrite, "人答了「覆盖」，发出去的却不是覆盖策略");
                Ok(7)
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
            bytes: 7
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
        |_, overwrite| {
            launched.fetch_add(1, O::SeqCst);
            // 🔴〔F7a〕没问过 ⇒ **不覆盖**（后端 `O_EXCL`）：探完之后才冒出来的同名文件照样不会被盖掉。
            assert!(!overwrite, "没问过人，发出去的却是覆盖策略");
            async move { Ok(3) }
        },
    )
    .await;
    assert_eq!(asked.load(O::SeqCst), 0, "目标本来就不在，却弹了覆盖确认框");
    assert_eq!(launched.load(O::SeqCst), 1);
    assert_eq!(
        out,
        CopyOutcome::Done {
            asked: false,
            bytes: 3
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
        |_, _| {
            launched.fetch_add(1, O::SeqCst);
            async move { Ok(0) }
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
// 正题二：**结局不许被吞** —— 运行时，以及界面上
// ════════════════════════════════════════════════════════════════════════

/// 后端报的复制字节数**原样**出现在结果里（两个值各一趟 —— 只判一个值的话，
/// 把 `bytes` 硬写成那个值的实现照样绿）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_byte_count_from_the_backend_comes_back_untouched() {
    for want in [0u64, 8_388_608] {
        let out = run_copy(
            job(),
            |_| async move { false },
            |_| async move { true },
            move |_, _| async move { Ok(want) },
        )
        .await;
        assert_eq!(
            out,
            CopyOutcome::Done {
                asked: false,
                bytes: want
            },
            "字节数在这一层被改写了"
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
        |_, _| async move { Err("`files-copy` 被拒（refused）：refuse write: 围栏".to_string()) },
    )
    .await;
    assert_eq!(
        out,
        CopyOutcome::Failed("`files-copy` 被拒（refused）：refuse write: 围栏".to_string())
    );
}

/// 〔F7a〕成功那一句说出**复制了几个字节**与**在哪儿复制的**，而且**不是**警告档；
/// 失败那一句带原文、**是**警告档（阴性对照：两档不许画成一样）。
///
/// 🔴 第三刀那条「退路要原样出声」随 SFTP 那条路一起没了（模块头注逐条）：
/// 后端在那台机器上复制，没有会过网的第二条路，也就没有「慢路」这句话可喊。
#[test]
fn the_done_notice_says_how_many_bytes_and_where_and_is_quiet() {
    let n = outcome_notice(&CopyOutcome::Done {
        asked: true,
        bytes: 8_388_608,
    });
    assert_eq!(
        n.text,
        "复制完成：8388608 字节，在那台机器上复制的，没经过你这台机器"
    );
    assert!(!n.loud, "复制成了是它该有的样子，不是警告");
    let f = outcome_notice(&CopyOutcome::Failed("原话".into()));
    assert_eq!(f.text, "复制失败：原话");
    assert!(f.loud, "失败不是警告档 —— 那一行会混在普通提示里");
}

/// 🔴 **那句话真的被画到窗口上了**（生产那个 [`CopyBoard::ui`]，从这一帧的 galley 读回来）；
/// 「在跑」时画一行「正在那台机器上复制」，而**不画**取消那颗按钮 ——
/// 后端那一趟取消不掉，画出来就是一颗按了没用的按钮（阴性对照同一把尺子）。
#[test]
fn the_outcome_and_the_running_line_really_get_painted_and_no_cancel_button_is() {
    let screen = egui::vec2(1280.0, 800.0);
    let ctx = egui::Context::default();
    let board = CopyBoard::default();
    board.begin("big.bin.copy");
    let _ = painted_text(&ctx, screen, 0.0, Vec::new(), |ui| board.ui(ui));
    let running = painted_text(&ctx, screen, 0.1, Vec::new(), |ui| board.ui(ui));
    assert!(
        !running.is_empty(),
        "这一帧一个字都没画出来 —— 量具塌了，下面几比在空转"
    );
    assert!(
        painted_contains(&running, "正在那台机器上复制 big.bin.copy"),
        "在跑却没说在跑：{:?}",
        running.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>()
    );
    assert!(
        !painted_contains(&running, super::super::transfer::CANCEL_LABEL.as_str()),
        "画了取消那颗按钮 —— 后端那一趟取消不掉，那是一颗按了没用的按钮"
    );
    board.finish(CopyOutcome::Done {
        asked: false,
        bytes: 42,
    });
    let done = painted_text(&ctx, screen, 0.2, Vec::new(), |ui| board.ui(ui));
    assert!(
        painted_contains(&done, "复制完成：42 字节"),
        "结局没画出来：{:?}",
        done.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>()
    );
    assert!(
        !painted_contains(&done, "正在那台机器上复制"),
        "跑完了还说在跑"
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
/// **有问题要问** · **起了一趟**（〔F7a〕后端没有进度，换成「在跑」那一下）· **跑完了**。
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
            "起了一趟",
            Box::new(|b: &CopyBoard| b.begin("big.bin.copy")) as Box<dyn Fn(&CopyBoard)>,
        ),
        (
            "跑完了",
            Box::new(|b: &CopyBoard| {
                b.finish(CopyOutcome::Done {
                    asked: false,
                    bytes: 0,
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
    board.begin("x");
    assert_eq!(board.running().as_deref(), Some("x"));
    board.finish(CopyOutcome::Skipped);
    assert_eq!(board.running(), None, "跑完了还挂着「在跑」");
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
        "复制成自己过了 —— 覆盖那一形是拿一份复制品顶掉目标，这里就是顶掉源自己"
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

/// 〔W5-FILES〕能复制的是「名字寻址得到」的那一档 —— 文件与**目录**都行（目录经后端 `recursive: true` 复制整棵，
/// 要求住址 `设计/60 §6.2`「复制目录」· `§7 #6`）。**相等断言，逐档。**
#[test]
fn files_and_directories_with_addressable_names_can_be_copied() {
    assert!(is_copyable(&row("big.bin", false, false)));
    assert!(
        is_copyable(&row("adir", true, false)),
        "目录复制不了 —— 后端 `files-copy` 已经收 `recursive: true`"
    );
    assert!(
        !is_copyable(&row("\u{FFFD}odd", false, true)),
        "有损名能复制 —— 那个名字寻址不到真字节，写操作一律灰置"
    );
}

// ════════════════════════════════════════════════════════════════════════
// 委派：〔F7a · 第三波 09-24〕**经通道问后端 `files-copy`，本层一行复制逻辑都没有**
// ════════════════════════════════════════════════════════════════════════

/// 一件复制 → 线上参数：`root` = 源的上一级、`from` / `to` = 两个尾段、覆盖策略原样；
/// 目标不在源的同一个目录 ⇒ **报错不发**（防「当前目录」与「那一行的路径」写法不一致时落到别处）。
#[test]
fn copy_args_split_the_paths_like_the_other_writes_and_refuse_a_second_directory() {
    assert_eq!(
        copy_args(&job(), true).unwrap(),
        serde_json::json!({
            "root": "/srv/data",
            "from": "big.bin",
            "to": "big.bin.copy",
            "overwrite": true,
        })
    );
    assert_eq!(copy_args(&job(), false).unwrap()["overwrite"], false);
    let elsewhere = CopyJob {
        from: "/srv/data/big.bin".into(),
        to: "/srv/other/big.bin".into(),
        name: "big.bin".into(),
        is_dir: false,
        from_raw: None,
        to_raw: None,
    };
    assert!(
        copy_args(&elsewhere, false).is_err(),
        "目标在另一个目录，竟然拼出了参数 —— 那会把文件放到他没在看的地方"
    );
    // 正控：`beside` 造出来的（含当前目录带尾斜杠那一形）都拼得出来。
    let j = CopyJob::beside("/srv/data/big.bin", "/srv/data/", "x").unwrap();
    assert_eq!(copy_args(&j, false).unwrap()["root"], "/srv/data");
}

/// ★ 经真回环口、真钥匙到合成后端：线上那一行逐格相等；回的字节数原样带回；
/// 后端拒 ⇒ 原话；旧后端不认 ⇒ 一个字节都不发。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn copying_goes_through_the_channel_with_the_overwrite_policy_on_the_wire() {
    use crate::filewin::find::testing::{wire_up, Declared, FakeBackend};
    let wired = wire_up(
        "copy-wire",
        FakeBackend::new(&[CMD_COPY], Declared::default()),
    )
    .await;
    let origin = crate::filewin::source::Origin(wired.origin.clone());
    let n = copy_remote(&wired.line, &origin, &job(), true)
        .await
        .expect("一趟干净的复制被拒了");
    assert_eq!(n, 42, "回的字节数不是后端报的那个");
    assert_eq!(
        wired.log.lock().unwrap().clone(),
        vec![serde_json::json!({
            "cmd": CMD_COPY,
            "args": {"root": "/srv/data", "from": "big.bin", "to": "big.bin.copy", "overwrite": true},
        })],
        "线上那一行不是「根 ＋ 两个尾段 ＋ 覆盖策略」"
    );
    let fenced = CopyJob {
        from: "/srv/refuse/a".into(),
        to: "/srv/refuse/b".into(),
        name: "b".into(),
        is_dir: false,
        from_raw: None,
        to_raw: None,
    };
    let e = copy_remote(&wired.line, &origin, &fenced, false)
        .await
        .expect_err("后端拒了，这一层却说成了");
    assert!(e.contains("refuse write"), "拒的不是后端那一句：{e}");
    let old = wire_up("copy-wire-old", FakeBackend::new(&[], Declared::default())).await;
    let e = copy_remote(
        &old.line,
        &crate::filewin::source::Origin(old.origin.clone()),
        &job(),
        false,
    )
    .await
    .expect_err("旧后端不认这条命令，竟然复制成了");
    assert!(e.contains("版本旧"), "旧后端那一形没说清：{e}");
    assert_eq!(old.count(CMD_COPY), 0);
}

/// ⚠ **判源码是代理，不是标的**（同 `transfer_tests` / `source_tests` 的如实标注）。
///
/// 买的是：复制经 `source::ask` 说后端那条命令（窗口进程够后端的唯一一处），
/// 探测借 `transfer::probe_remote`（「那儿有没有东西」只有一个口径）；
/// SFTP 池子、进度通道、取消台一样都不碰。买不到：一台真远端上那一趟真跑过。
#[test]
fn the_real_adapter_asks_the_backend_and_touches_no_transfer_machinery() {
    let prod = guard_core::production_code(include_str!("../../../src/bridge/src/filewin/copy.rs"));
    guard_core::assert_no_test_code("filewin/copy.rs", &prod);
    assert!(
        prod.len() > 3_000,
        "生产段只剩 {} 字节 —— 剥法把它剥没了，下面几条在空转",
        prod.len()
    );
    // 〔W5-FILES · 有损名全寻址〕探目标换成 `probe_remote_at`（路径可以是字节；`probe_remote` 是它路径为串时的那一形，同一个口径）。
    for needle in ["source::ask(", "transfer::probe_remote_at("] {
        assert_eq!(
            prod.matches(needle).count(),
            1,
            "`{needle}` 在生产段里出现 {} 次（应当恰好 1 次）—— \
             多了就是长出了第二条路，少了就是这一条被换掉了",
            prod.matches(needle).count()
        );
    }
    // 针拼出来，免得命中本文件自己的说明。
    let pool = format!("sftp_{}::", "pool");
    for banned in [
        pool.as_str(),
        "tauri::ipc::Channel",
        "CancelDesk",
        "launch_unless_cancelled",
        "copy_remote_path(",
        "guard_write(",
        "is_protected_claude_data_path",
    ] {
        assert!(
            !prod.contains(banned),
            "生产段里出现了 `{banned}` —— 复制在后端，这一层不许自己碰传输／围栏／取消，\
             它该做的只有「问一次、起一趟、把结局摆出来」那三件"
        );
    }
    // 反空真：这把尺子认得出「有」。
    assert!(prod.contains("source::ask("));
}

// ════════════════════════════════════════════════════════════════════════
// 〔W5-FILES〕一摞复制（复制到另一栏的多选 / 目录）
// 要求住址：`设计/60 §6.2`「复制目录 · 批量复制」· `§6.3`「批量…走『一次问完』」「悄悄跳过其中一项正是…反面」。
// ════════════════════════════════════════════════════════════════════════

fn named(name: &str, is_dir: bool) -> CopyJob {
    CopyJob {
        from: format!("/srv/a/{name}"),
        to: format!("/srv/b/{name}"),
        name: name.to_string(),
        is_dir,
        from_raw: None,
        to_raw: None,
    }
}

/// 跑一摞：`there` 是目标已在的那几件；`answer` 是那一问的答复；`fail` 是后端会拒的那几件。
/// 回 `(结局, 问了几次, 问的是哪几件, 发出去的 (名字, overwrite))`。
async fn batch(
    jobs: Vec<CopyJob>,
    there: &[&str],
    answer: bool,
    fail: &[&str],
) -> (CopyOutcome, usize, Vec<String>, Vec<(String, bool)>) {
    let asked = std::sync::Arc::new(std::sync::Mutex::new((0usize, Vec::<String>::new())));
    let sent = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(String, bool)>::new()));
    let there: Vec<String> = there.iter().map(|s| s.to_string()).collect();
    let fail: Vec<String> = fail.iter().map(|s| s.to_string()).collect();
    let (a2, s2) = (asked.clone(), sent.clone());
    let out = run_copy_batch(
        jobs,
        move |j| {
            let hit = there.contains(&j.name);
            async move { hit }
        },
        move |js| {
            let mut g = a2.lock().unwrap();
            g.0 += 1;
            g.1 = js.iter().map(|j| j.name.clone()).collect();
            async move { answer }
        },
        move |j, ow| {
            s2.lock().unwrap().push((j.name.clone(), ow));
            let bad = fail.contains(&j.name);
            async move {
                if bad {
                    Err("后端原话".to_string())
                } else {
                    Ok(Copied {
                        name: j.name.clone(),
                        bytes: 10,
                        files: if j.is_dir { 3 } else { 1 },
                        dirs: if j.is_dir { 2 } else { 0 },
                    })
                }
            }
        },
    )
    .await;
    let g = asked.lock().unwrap().clone();
    let s = sent.lock().unwrap().clone();
    (out, g.0, g.1, s)
}

#[tokio::test]
async fn a_batch_with_a_clashing_directory_does_nothing_and_asks_nobody() {
    let (out, asks, _, sent) = batch(
        vec![named("f", false), named("d", true)],
        &["f", "d"],
        true,
        &[],
    )
    .await;
    assert!(
        matches!(&out, CopyOutcome::Refused(w) if w.contains('d')),
        "{out:?}"
    );
    assert_eq!((asks, sent.len()), (0, 0), "目录撞名还问了 / 还发了");
}

#[tokio::test]
async fn a_batch_asks_once_about_exactly_the_clashing_files() {
    let jobs = || vec![named("a", false), named("b", false), named("d", true)];
    // 答「都不覆盖」：撞名那件跳过，其余照发、不带覆盖。
    let (out, asks, which, sent) = batch(jobs(), &["b"], false, &[]).await;
    assert_eq!((asks, which), (1, vec!["b".to_string()]));
    assert_eq!(sent, vec![("a".into(), false), ("d".into(), false)]);
    let CopyOutcome::Batch(r) = out else {
        panic!("不是一摞的结局")
    };
    assert_eq!(r.skipped, vec!["b".to_string()]);
    // 答「覆盖」：撞名那件带覆盖，其余照旧不带。
    let (_, asks, _, sent) = batch(jobs(), &["b"], true, &[]).await;
    assert_eq!(asks, 1);
    assert_eq!(
        sent,
        vec![("a".into(), false), ("b".into(), true), ("d".into(), false)]
    );
    // 不撞名 ⇒ 一次都不问（弹一个空框是噪音）。
    let (_, asks, _, sent) = batch(jobs(), &[], true, &[]).await;
    assert_eq!((asks, sent.len()), (0, 3));
}

#[tokio::test]
async fn a_failure_in_a_batch_is_named_and_does_not_stop_the_rest() {
    let (out, _, _, sent) = batch(
        vec![named("a", false), named("d", true), named("z", false)],
        &[],
        false,
        &["a"],
    )
    .await;
    assert_eq!(sent.len(), 3, "第一件失败之后后面的没发");
    let n = outcome_notice(&out);
    assert!(n.loud, "有失败却不是警告档");
    assert!(
        n.text
            .contains("复制完成：2 项，4 个文件、2 个目录、20 字节")
            && n.text.contains("a 复制失败：后端原话"),
        "{}",
        n.text
    );
}

/// 目录那一件线上多 `recursive: true`；文件那一件一个键都不多。应答解析：文件那件在旧后端（没有 `files` / `dirs`）上按「一个文件」记，目录那件缺键就报错。
#[test]
fn a_directory_job_says_recursive_and_its_reply_must_count() {
    let f = CopyJob::beside("/srv/a/x", "/srv/a", "y").unwrap();
    let d = CopyJob::beside("/srv/a/x", "/srv/a", "y")
        .unwrap()
        .dir(true);
    assert_eq!(copy_args(&f, false).unwrap().get("recursive"), None);
    assert_eq!(copy_args(&d, false).unwrap()["recursive"], true);
    let old = serde_json::json!({"bytes": 5});
    assert_eq!(
        copied_from_reply(&f, &old),
        Ok(Copied {
            name: "y".into(),
            bytes: 5,
            files: 1,
            dirs: 0
        })
    );
    assert!(
        copied_from_reply(&d, &old).is_err(),
        "目录那件缺条数竟然收了"
    );
    let new = serde_json::json!({"bytes": 9, "files": 3, "dirs": 2});
    assert_eq!(
        copied_from_reply(&d, &new),
        Ok(Copied {
            name: "y".into(),
            bytes: 9,
            files: 3,
            dirs: 2
        })
    );
}
