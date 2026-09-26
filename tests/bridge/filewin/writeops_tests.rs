//! [`super`] 的判据 —— **那四条写操作**（`设计/99 §4.6.4` 的前半）。
//!
//! # 🔴 这一摞里哪几条是承重的
//!
//! | 判据 | 它钉的那一形 | 少了它会怎样 |
//! |---|---|---|
//! | [`a_session_file_path_is_asked_and_done_like_any_other`] | 〔FN1 · V119〕会话文件 / pidfile / subagent / 项目目录那几件**照问照做**（不许有东西在问答之前拦它们） | 用户「文件管理器全部都可以改. 不需要任何围栏」没落地：窗口上点删除什么都不发生 |
//! | [`a_refusal_from_the_backend_fence_comes_back_as_a_sentence`] | 〔F2〕后端写面拒了（〔FN1〕今天只剩路径解析那几形），那句话原样回到窗口上 | 被拒与「成功」/ 空串分不开 |
//!
//! 〔FN1 · V119〕这张表原来前三行是本地围栏那三条（踩线的不问不做 · 改名两条路径各过一遍 · 普通路径的阴性对照），
//! 靶子（本地预判）删了，三条一起退役。
//! | [`the_real_adapter_speaks_the_backend_write_face_with_root_and_rel`] | 〔F2〕四件各发哪条命令、参数切成 `(root, rel)`（读线上真到的那几行） | 发错命令 / 切错路径，后端那一侧落在别处 |
//! | [`the_question_is_asked_exactly_once_for_the_whole_batch`] | N 件只问一次，且顺序是 问 → 做 | 每件弹一次（旧面板 `uploadDropped` 那一形） |
//! | [`nothing_is_touched_when_the_answer_is_no`] | 「问过了」与「做了」分得开 | 「问了但照做」在读数上看不出来 |
//!
//! # ⚠ 这一摞买不到什么（逐条）
//!
//! - **一趟真操作的读数买不到**（本仓红线不许起真连接）⇒ `apply` 那一侧喂的是
//!   合成适配器，或者〔F2〕挂在真通道口上的一台合成后端（它只记下收到了什么、不落盘）。
//! - **生产那一侧递不进 `N > 1`**（窗口还没有多选）—— 逐字登记在 `super` 头注。
//!   这一摞按 `N > 1` 喂的是 [`super::run_writes`] 本体，也就是生产那个函数。

use std::sync::atomic::{AtomicUsize, Ordering as O};

use super::*;

/// 一条会话记录形状的路径（`projects/` 下恰两段、以 `.jsonl` 收尾）。
/// 〔FN1 · V119〕它从前叫「受保护的路径」；今天它与别的路径同一条路。
fn session_path() -> String {
    "/home/u/.claude/projects/dash-proj/abc-123.jsonl".to_string()
}

fn row(name: &str, is_dir: bool, lossy: bool) -> Listed {
    Listed::plain(crate::filewin::source::Row {
        name: name.to_string(),
        path: format!("/srv/data/{name}"),
        is_dir,
        size: 7,
        lossy_name: lossy,
    })
}

/// 一台**时序台架**（形状照 `copy_tests::Tape` 与 `transfer_tests::Tape`）。
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

/// 一个记账的 `apply`：把每一件真做过的操作按顺序记下来。
#[derive(Default)]
struct Applied(std::sync::Mutex<Vec<WriteOp>>);

impl Applied {
    fn seen(&self) -> Vec<WriteOp> {
        self.0.lock().unwrap().clone()
    }
}

// ════════════════════════════════════════════════════════════════════════
// 🔴 正题一〔FN1 · V119 翻面〕：会话文件那一行与别的行**同一条路**
// ════════════════════════════════════════════════════════════════════════
//
// 这里原来是「围栏 —— 两个方向都有判据」三条：受保护路径不问不做而且出声 · 改名两条路径各过一遍 ·
// 普通路径必须过（阴性对照）。用户 V119「文件管理器全部都可以改. 不需要任何围栏」⇒ 本地那道预判删了，
// 三条的靶子一起不在了；换成下面这一条 —— 它钉的是**反方向**：会话文件形状的路径不许再被任何东西挡在问答与 `apply` 之外。

/// 🔴〔FN1 · V119〕会话文件 / pidfile / subagent 记录 / 项目目录上的删除 · 改权限 · 改名 ⇒
/// 该问的（删除 · 改权限）**照问一次**，答「做」就**原样交到 `apply`**；不问的（改名）直接交。
///
/// 反空真：`asked` 必须恰是那两件要问的（不是 0 —— 0 就是有东西在问答之前把它们拦了）。
/// 住址：用户裁决 V119（`设计/99 §1`）原话「文件管理器全部都可以改. 不需要任何围栏」。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_session_file_path_is_asked_and_done_like_any_other() {
    let asked = AtomicUsize::new(0);
    let applied = Applied::default();
    let ops = vec![
        WriteOp::Delete {
            path: session_path(),
            is_dir: false,
            raw: None,
        },
        WriteOp::Chmod {
            path: "/home/u/.claude/sessions/4321.json".to_string(),
            mode: 0o600,
            raw: None,
        },
        WriteOp::Rename {
            from: "/home/u/.claude/projects/dash-proj/abc-123/subagents/agent-1.jsonl".to_string(),
            to: "/home/u/.claude/projects/dash-proj/abc-123/subagents/agent-2.jsonl".to_string(),
            raw: None,
        },
        WriteOp::Delete {
            path: "/home/u/.claude/projects/dash-proj".to_string(),
            is_dir: true,
            raw: None,
        },
    ];
    let out = run_writes(
        ops.clone(),
        |a| {
            asked.fetch_add(a.len(), O::SeqCst);
            async move { a } // 全答「做」
        },
        |op| {
            applied.0.lock().unwrap().push(op);
            async move { Ok(()) }
        },
    )
    .await;
    assert_eq!(
        asked.load(O::SeqCst),
        3,
        "要问的那三件（两件删除 ＋ 一件改权限）没有恰好都摆到人面前 —— 有东西在问答之前把它们拦了"
    );
    assert_eq!(
        applied.seen(),
        ops,
        "🔴 V119：会话文件那几件没有原样交到 `apply` 手上"
    );
    assert_eq!((out.asked, out.ok, out.skipped), (3, 4, 0), "{out:?}");
    assert!(out.failed.is_empty(), "{out:?}");
}

/// 🔴〔F2 · 2026-09-24〕**后端那道围栏拒了，那句话原样回到窗口上** —— 不是被吞掉。
///
/// 上一版这里判的是「池子入口那道 `guard_write` 还活着」（那四条走 SFTP 的时候）。
/// 写面改走后端之后，第二道围栏在**后端**那一侧（写面那道会话数据围栏），判它的判据住
/// `tests/backend/`；窗口这一侧要保住的是：**被拒 ⇒ 一句带原话的错**，不是「成功」也不是空串。
/// 合成后端对 `root` 里带 `refuse` 的一律按围栏那一档（`refused`）拒。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refusal_from_the_backend_fence_comes_back_as_a_sentence() {
    use crate::filewin::find::testing::{wire_up, Declared, FakeBackend};
    let wired = wire_up(
        "writeops-refuse",
        FakeBackend::new(&["files-mkdir", "files-delete"], Declared::default()),
    )
    .await;
    let origin = crate::origin::Origin(wired.origin.clone());
    let e = super::apply_remote(
        &wired.line,
        &origin,
        &WriteOp::Mkdir {
            path: "/srv/refuse/newdir".into(),
        },
    )
    .await
    .expect_err("后端拒了，窗口这一侧却成了");
    assert!(
        e.contains("refused") && e.contains("refuse write"),
        "那句话里没有后端的码与原话：`{e}`"
    );
    // 阴性对照：同一条命令在普通路径上成（否则上面可以靠「什么都失败」蒙过去）。
    super::apply_remote(
        &wired.line,
        &origin,
        &WriteOp::Mkdir {
            path: "/srv/data/newdir".into(),
        },
    )
    .await
    .expect("普通路径上也失败了 —— 上面那一条在空转");
    // 后端没声明的命令 ⇒ 一个字节都没发，而且**说出来**。
    let e = super::apply_remote(
        &wired.line,
        &origin,
        &WriteOp::Chmod {
            path: "/srv/data/x".into(),
            mode: 0o644,
            raw: None,
        },
    )
    .await
    .expect_err("后端没声明 `files-chmod`，却成了");
    assert!(e.contains("files-chmod"), "没说是哪条命令不认：`{e}`");
    assert_eq!(
        wired.cmds(),
        ["files-mkdir", "files-mkdir"],
        "没声明的那条竟然上了线"
    );
}

// ════════════════════════════════════════════════════════════════════════
// 🔴 正题二：一次问完（照 `run_drop` 的形状办）
// ════════════════════════════════════════════════════════════════════════

/// 🔴 **N 件只问一次**，而且那一问拿到的是**全部**要问的那几件（相等断言），
/// 并且它排在任何一次动手**之前**（时序号相比）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_question_is_asked_exactly_once_for_the_whole_batch() {
    let tape = Tape::default();
    let times = AtomicUsize::new(0);
    let seen: std::sync::Mutex<Vec<WriteOp>> = std::sync::Mutex::new(Vec::new());
    // 五件：三件要问（删 ×2 · 改权限 ×1）、两件不问（新建目录 · 改名）。
    let dels: Vec<WriteOp> = (0..2)
        .map(|i| WriteOp::Delete {
            path: format!("/srv/data/d{i}"),
            is_dir: false,
            raw: None,
        })
        .collect();
    let mut ops = dels.clone();
    ops.push(WriteOp::Chmod {
        path: "/srv/data/c".into(),
        mode: 0o755,
        raw: None,
    });
    ops.push(WriteOp::Mkdir {
        path: "/srv/data/m".into(),
    });
    ops.push(WriteOp::Rename {
        from: "/srv/data/a".into(),
        to: "/srv/data/b".into(),
        raw: None,
    });

    let out = run_writes(
        ops.clone(),
        |a| {
            times.fetch_add(1, O::SeqCst);
            tape.mark("confirm");
            *seen.lock().unwrap() = a.clone();
            async move { a } // 全答「做」
        },
        |_op| {
            let t = &tape;
            async move {
                t.mark("apply");
                Ok::<(), String>(())
            }
        },
    )
    .await;

    assert_eq!(
        times.load(O::SeqCst),
        1,
        "问了 {} 次 —— 要的是**一次问完**（旧面板那一形是每件弹一次）",
        times.load(O::SeqCst)
    );
    let want: Vec<WriteOp> = ops.iter().filter(|o| o.needs_confirm()).cloned().collect();
    assert_eq!(
        *seen.lock().unwrap(),
        want,
        "那一问摆出来的不是**全部**要问的那几件"
    );
    assert_eq!(out.asked, 3, "该问的件数不对");
    let (c, a) = (
        tape.seq_of("confirm").expect("一次都没问"),
        tape.seq_of("apply").expect("一件都没做"),
    );
    assert!(
        c < a,
        "问的号是 {c}、第一次动手是 {a} —— 先问完再动手这句话破了"
    );
    assert_eq!(out.ok, 5);
    assert_eq!(out.skipped, 0);
}

/// 🔴 **「问过了」与「做了」分得开**：答「都别做」⇒ 要问的那几件一件都不做，
/// 而**不用问**的那几件照旧做（它们从来不在那一问的射程里）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nothing_is_touched_when_the_answer_is_no() {
    let applied = Applied::default();
    let mkdir = WriteOp::Mkdir {
        path: "/srv/data/m".into(),
    };
    let del = WriteOp::Delete {
        path: "/srv/data/d".into(),
        is_dir: false,
        raw: None,
    };
    let out = run_writes(
        vec![del.clone(), mkdir.clone()],
        |_| async move { Vec::new() }, // 「都别做」
        |op| {
            applied.0.lock().unwrap().push(op);
            async move { Ok(()) }
        },
    )
    .await;
    assert_eq!(
        applied.seen(),
        vec![mkdir],
        "答「都别做」之后动手的那几件不对 —— 删除必须没做，新建目录该照旧做"
    );
    assert_eq!(out.asked, 1);
    assert_eq!(out.skipped, 1);
    assert_eq!(out.ok, 1);
}

/// 一件要问的都没有 ⇒ **不问**。弹一个空框是噪音，不是慎重（同 `run_drop`）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_batch_with_nothing_dangerous_asks_nobody() {
    let asked = AtomicUsize::new(0);
    let applied = Applied::default();
    let out = run_writes(
        vec![
            WriteOp::Mkdir {
                path: "/srv/data/m".into(),
            },
            WriteOp::Rename {
                from: "/srv/data/a".into(),
                to: "/srv/data/b".into(),
                raw: None,
            },
        ],
        |_| {
            asked.fetch_add(1, O::SeqCst);
            async move { Vec::new() }
        },
        |op| {
            applied.0.lock().unwrap().push(op);
            async move { Ok(()) }
        },
    )
    .await;
    assert_eq!(asked.load(O::SeqCst), 0, "没有危险件却弹了一个框");
    assert_eq!(out.ok, 2);
    assert_eq!(applied.seen().len(), 2);
}

/// 🔴 那一问回来的东西**只起「准不准」的作用** —— 它没法凭空塞进一件新操作。
///
/// 少了这条过滤，一个坏掉（或被改坏）的看板就能把一件**调用方没交给它**的操作递进 `apply`。
/// 〔FN1〕从前的说法是「没过围栏的操作」；围栏删了，这条过滤的理由没变（人点的是哪几件，就只做哪几件）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_answer_cannot_smuggle_in_an_operation_nobody_fenced() {
    let applied = Applied::default();
    let smuggled = WriteOp::Delete {
        path: session_path(),
        is_dir: false,
        raw: None,
    };
    let out = run_writes(
        vec![WriteOp::Delete {
            path: "/srv/data/d".into(),
            is_dir: false,
            raw: None,
        }],
        move |_| async move { vec![smuggled] }, // 答复里换了一件别的
        |op| {
            applied.0.lock().unwrap().push(op);
            async move { Ok(()) }
        },
    )
    .await;
    assert_eq!(
        applied.seen(),
        Vec::new(),
        "答复里塞进来的那一件竟然被做了：{:?}",
        applied.seen()
    );
    assert_eq!(out.ok, 0);
    assert_eq!(out.skipped, 1, "原来那一件既没被准、也没被记成跳过");
}

/// 失败的那几件带着**池子给的原文**回来（不改写、不摘要）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failure_comes_back_with_the_message_the_pool_gave() {
    let out = run_writes(
        vec![WriteOp::Mkdir {
            path: "/srv/data/m".into(),
        }],
        |a| async move { a },
        |_| async move { Err("新建目录失败: permission denied".to_string()) },
    )
    .await;
    assert_eq!(out.ok, 0);
    assert_eq!(out.failed.len(), 1);
    assert_eq!(out.failed[0].1, "新建目录失败: permission denied");
    assert!(
        out.failed[0].0.contains("/srv/data/m"),
        "失败那一行没说是哪一件：{:?}",
        out.failed[0]
    );
}

/// 空的一摞 ⇒ 什么都不做、不问、不记数。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_empty_batch_does_nothing_at_all() {
    let asked = AtomicUsize::new(0);
    let out = run_writes(
        Vec::new(),
        |_| {
            asked.fetch_add(1, O::SeqCst);
            async move { Vec::new() }
        },
        |_| async move { Ok(()) },
    )
    .await;
    assert_eq!(out, WriteOutcome::default());
    assert_eq!(asked.load(O::SeqCst), 0);
}

// ════════════════════════════════════════════════════════════════════════
// 谁要问、谁不要 —— 那张表得有人数
// ════════════════════════════════════════════════════════════════════════

/// 🔴 **相等断言**：要问的恰好是删除与改权限那两档。
///
/// 逐条的理由住 `super` 头注 §四那张表。写成「至少删除要问」是地板，
/// 而地板在「变少」方向上是瞎的 —— 有人把改权限那一档改成不问，地板不会红。
#[test]
fn exactly_delete_and_chmod_ask_first() {
    let all = vec![
        WriteOp::Mkdir { path: "/a".into() },
        WriteOp::Rename {
            from: "/a".into(),
            to: "/b".into(),
            raw: None,
        },
        WriteOp::Delete {
            path: "/a".into(),
            is_dir: false,
            raw: None,
        },
        WriteOp::Delete {
            path: "/a".into(),
            is_dir: true,
            raw: None,
        },
        WriteOp::Chmod {
            path: "/a".into(),
            mode: 0o644,
            raw: None,
        },
    ];
    let asking: Vec<String> = all
        .iter()
        .filter(|o| o.needs_confirm())
        .map(|o| o.label())
        .collect();
    assert_eq!(
        asking,
        vec![
            "删除文件 /a".to_string(),
            "删除目录 /a（连同里面全部内容）".to_string(),
            "改权限 /a → 644".to_string()
        ],
        "要问的那几档变了 —— 连着 `super` 头注 §四那张表一起改，别只改代码"
    );
}

/// 每一件都说得出自己是什么（确认框上摆的就是这几句话）。
#[test]
fn every_op_can_say_what_it_is() {
    assert_eq!(
        WriteOp::Mkdir {
            path: "/srv/d".into()
        }
        .label(),
        "新建目录 /srv/d"
    );
    assert_eq!(
        WriteOp::Rename {
            from: "/srv/a".into(),
            to: "/srv/b".into(),
            raw: None,
        }
        .label(),
        "改名 /srv/a → /srv/b"
    );
    // 权限**按八进制**说 —— 说成十进制（`420`）用户读不出那是 `644`。
    assert_eq!(
        WriteOp::Chmod {
            path: "/srv/a".into(),
            mode: 0o644,
            raw: None,
        }
        .label(),
        "改权限 /srv/a → 644"
    );
}

// ════════════════════════════════════════════════════════════════════════
// 那个框：框里那几个字 → 一件真操作
// ════════════════════════════════════════════════════════════════════════

/// 三个框各自把字变成**同一个目录里**的一件操作。
#[test]
fn the_box_turns_what_you_typed_into_an_op_in_this_very_directory() {
    let r = row("a.bin", false, false);
    let mut p = WritePrompt::for_mkdir("/srv/data");
    p.text = "  sub  ".into(); // 两头的空白要被吃掉
    assert_eq!(
        p.to_op().unwrap(),
        WriteOp::Mkdir {
            path: "/srv/data/sub".into()
        }
    );

    let mut p = WritePrompt::for_rename("/srv/data", &r);
    assert_eq!(
        p.text, "a.bin",
        "改名那个框没预填原名（旧面板预填的是原名）"
    );
    p.text = "b.bin".into();
    assert_eq!(
        p.to_op().unwrap(),
        WriteOp::Rename {
            from: "/srv/data/a.bin".into(),
            to: "/srv/data/b.bin".into(),
            raw: None,
        }
    );

    let mut p = WritePrompt::for_chmod("/srv/data", &r);
    assert_eq!(
        p.text, "",
        "改权限那个框预填了东西 —— 列表里读不到 mode，预填的必然是猜的，\
         而用户直接点确认就是静默改坏权限"
    );
    p.text = "755".into();
    assert_eq!(
        p.to_op().unwrap(),
        WriteOp::Chmod {
            path: "/srv/data/a.bin".into(),
            mode: 0o755,
            raw: None,
        }
    );
}

/// 🔴 **拒得掉的那几档，一档都不许兜底编一个出来。**
///
/// 逐档的理由住 `super::clean_name` 与 `super::parse_mode`：
/// 带 `/` = 把文件放到他没在看的目录里；`..` = 让服务端对着父目录动手；
/// 权限超范围 = 把文件类型位也一起改（服务端行为未定义）。
#[test]
fn an_impossible_input_is_refused_with_a_reason_instead_of_a_guess() {
    let r = row("a.bin", false, false);
    for bad in ["", "   ", "sub/x", "/abs", ".", ".."] {
        let mut p = WritePrompt::for_mkdir("/srv/data");
        p.text = bad.into();
        let why = p.to_op().expect_err(&format!("「{bad}」这个名字竟然过了"));
        assert!(!why.is_empty(), "拒了却没说为什么");

        let mut p = WritePrompt::for_rename("/srv/data", &r);
        p.text = bad.into();
        assert!(p.to_op().is_err(), "改名接受了「{bad}」");
    }
    // 改名成原名 ⇒ 拒（没有要改的东西，而 SFTP 那侧会当成一次真 rename）。
    let mut p = WritePrompt::for_rename("/srv/data", &r);
    p.text = " a.bin ".into();
    assert!(p.to_op().is_err(), "改名成原名竟然过了");

    for bad in ["", "  ", "abc", "899", "10000", "-1", "0x644"] {
        let mut p = WritePrompt::for_chmod("/srv/data", &r);
        p.text = bad.into();
        assert!(p.to_op().is_err(), "权限位接受了「{bad}」");
    }
    // 反空真：合法的那几个真的过。
    for good in ["644", "0755", "7777", "0"] {
        let mut p = WritePrompt::for_chmod("/srv/data", &r);
        p.text = good.into();
        assert!(p.to_op().is_ok(), "权限位拒了合法的「{good}」");
    }
}

/// 每个框都说得出自己在问什么（那一行字真的摆在框上）。
#[test]
fn every_box_says_what_it_is_asking() {
    let r = row("a.bin", false, false);
    assert!(WritePrompt::for_mkdir("/srv").heading().contains("新建"));
    assert!(WritePrompt::for_rename("/srv", &r)
        .heading()
        .contains("a.bin"));
    assert!(WritePrompt::for_chmod("/srv", &r)
        .heading()
        .contains("八进制"));
}

// ════════════════════════════════════════════════════════════════════════
// 哪几行能写
// ════════════════════════════════════════════════════════════════════════

/// 🔴 **目录能写、不能复制** —— 两个判准刻意不是同一个函数。
///
/// 合成一个的后果是具体的：要么目录上长出一颗点了必失败的「复制」
/// （`copy-data` 吃文件句柄），要么目录上少了「删除」和「改名」——
/// 而那正是这一刀要补的东西。
#[test]
fn a_directory_can_be_written_and_copied_but_a_lossy_name_with_bytes_only_written() {
    let dir = row("adir", true, false);
    let file = row("a.bin", false, false);
    let lossy = row("\u{FFFD}odd", false, true);
    let lossy_raw = Listed {
        raw_name: Some(b"\xffodd".to_vec()),
        ..row("\u{FFFD}odd", false, true)
    };

    assert!(is_writable(&dir), "目录不能改名/删除/改权限？");
    assert!(is_writable(&file));
    assert!(
        !is_writable(&lossy),
        "有损名能写 —— 那个名字寻址不到真字节，删中的是另一个文件"
    );

    // 〔W5-FILES〕目录能复制了（后端 `recursive: true`，`设计/60 §6.2`）。
    assert!(crate::filewin::copy::is_copyable(&dir), "目录复制不了？");
    assert!(crate::filewin::copy::is_copyable(&file));
    // 两个判准**真的不等价**（否则上面那几条在一个函数上也全绿）：带字节的有损名能写（写面收 b16），复制照旧不收。
    assert!(is_writable(&lossy_raw));
    assert_ne!(
        is_writable(&lossy_raw),
        crate::filewin::copy::is_copyable(&lossy_raw),
        "两个判准在带字节的有损名这一档上给了同一个答案 —— 那它们就该合成一个，\
         或者其中一个错了"
    );
}

// ════════════════════════════════════════════════════════════════════════
// 看板
// ════════════════════════════════════════════════════════════════════════

/// 看板把答复真的送回去，且**只送一次**。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_board_answers_once_and_only_once() {
    let b = WriteBoard::default();
    let ops = vec![
        WriteOp::Delete {
            path: "/a".into(),
            is_dir: false,
            raw: None,
        },
        WriteOp::Chmod {
            path: "/b".into(),
            mode: 0o600,
            raw: None,
        },
    ];
    let rx = b.ask(ops.clone());
    assert!(b.is_asking());
    assert_eq!(b.asking(), ops, "摆出来的不是那几件");
    assert!(b.settle(true), "答复没送出去");
    assert!(!b.settle(true), "同一问送了第二次答复");
    assert_eq!(rx.await.unwrap(), ops, "缺省该是全勾上的");
    assert!(!b.is_asking());
}

/// 「都别做」⇒ 送回去的是**空**（不是「全做」）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn saying_no_sends_back_an_empty_list() {
    let b = WriteBoard::default();
    let rx = b.ask(vec![WriteOp::Delete {
        path: "/a".into(),
        is_dir: true,
        raw: None,
    }]);
    assert!(b.settle(false));
    assert_eq!(rx.await.unwrap(), Vec::new());
}

/// 一趟跑完 ⇒ `rounds` 涨一格、结果留在板上（**画在窗口上**，不是 `println!`）。
#[test]
fn a_finished_round_is_observable_and_keeps_its_words() {
    let b = WriteBoard::default();
    assert_eq!(b.rounds(), 0);
    assert!(b.last().is_none());
    let out = WriteOutcome {
        asked: 1,
        skipped: 0,
        ok: 1,
        failed: vec![("删除文件 /a".into(), "boom".into())],
    };
    b.finish(out.clone());
    assert_eq!(b.rounds(), 1);
    assert_eq!(b.last(), Some(out));
}

/// 没有窗口也不 panic（判据里就没有窗口）。
#[test]
fn a_board_with_no_window_still_records_and_does_not_panic() {
    let b = WriteBoard::default();
    b.attach(None);
    b.poke();
    b.finish(WriteOutcome::default());
    assert_eq!(b.rounds(), 1);
}

// ════════════════════════════════════════════════════════════════════════
// 🔴 委派：这一层**一行自己的写代码都没有**
// ════════════════════════════════════════════════════════════════════════

/// 🔴〔F2 · 2026-09-24〕[`super::apply_remote`] 发的是**后端写面那四条**，参数切成 `(root, rel)`。
///
/// **行为判据**（不扫源码）：挂一台合成后端，四件各发一次，读它收到了什么。
/// 两侧不同源：期望是按「当前目录 ＋ 名字」手写的，实得是线上真到了后端的那几行。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_real_adapter_speaks_the_backend_write_face_with_root_and_rel() {
    use crate::filewin::find::testing::{wire_up, Declared, FakeBackend};
    let wired = wire_up(
        "writeops-face",
        FakeBackend::new(
            &["files-mkdir", "files-delete", "files-rename", "files-chmod"],
            Declared::default(),
        ),
    )
    .await;
    let origin = crate::origin::Origin(wired.origin.clone());
    let ops = [
        WriteOp::Mkdir {
            path: "/srv/data/新目录".into(),
        },
        WriteOp::Delete {
            path: "/srv/data/x.bin".into(),
            is_dir: false,
            raw: None,
        },
        WriteOp::Rename {
            from: "/srv/data/x.bin".into(),
            to: "/srv/data/y.bin".into(),
            raw: None,
        },
        WriteOp::Chmod {
            path: "/srv/data/y.bin".into(),
            mode: 0o640,
            raw: None,
        },
    ];
    for op in &ops {
        super::apply_remote(&wired.line, &origin, op)
            .await
            .unwrap_or_else(|e| panic!("{} 没成：{e}", op.label()));
    }
    let got: Vec<serde_json::Value> = wired.log.lock().unwrap().clone();
    let want = vec![
        serde_json::json!({ "cmd": "files-mkdir", "args": { "root": "/srv/data", "rel": "新目录" } }),
        serde_json::json!({ "cmd": "files-delete", "args": { "root": "/srv/data", "rel": "x.bin" } }),
        serde_json::json!({ "cmd": "files-rename", "args": { "root": "/srv/data", "from": "x.bin", "to": "y.bin" } }),
        serde_json::json!({ "cmd": "files-chmod", "args": { "root": "/srv/data", "rel": "y.bin", "mode": 0o640 } }),
    ];
    assert_eq!(got, want, "线上那四行与期望不等");
    // 改名跨目录 ⇒ 当场拒，一个字节不发（写面只有一个 `root`）。
    let e = super::apply_remote(
        &wired.line,
        &origin,
        &WriteOp::Rename {
            from: "/srv/data/a".into(),
            to: "/srv/other/a".into(),
            raw: None,
        },
    )
    .await
    .expect_err("跨目录改名竟然发出去了");
    assert!(!e.is_empty());
    assert_eq!(wired.log.lock().unwrap().len(), 4, "跨目录那一件上了线");
}

// ════════════════════════════════════════════════════════════════════════
// 〔FW5 · 第四波〕删目录 = 连同里面全部内容 · 乱码名走 b16 · 批量改权限
// ════════════════════════════════════════════════════════════════════════

/// 🔴 **线上那几行逐字**：删**目录**带 `recursive: true`、删文件不带；有损名的相对段是 `{"b16": …}`
/// （改名的 `from` 同样），`to` 照旧是框里敲的字符串。
///
/// 期望手写（异源）：读的是合成后端真收到的那几行。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_directory_delete_recurses_and_a_lossy_name_is_addressed_by_its_bytes() {
    use crate::filewin::find::testing::{wire_up, Declared, FakeBackend};
    let wired = wire_up(
        "writeops-fw5",
        FakeBackend::new(
            &["files-delete", "files-rename", "files-chmod"],
            Declared::default(),
        ),
    )
    .await;
    let origin = crate::origin::Origin(wired.origin.clone());
    let raw = b"caf\xe9.txt".to_vec();
    let ops = [
        WriteOp::Delete {
            path: "/srv/data/build".into(),
            is_dir: true,
            raw: None,
        },
        WriteOp::Delete {
            path: "/srv/data/caf\u{FFFD}.txt".into(),
            is_dir: false,
            raw: Some(raw.clone()),
        },
        WriteOp::Rename {
            from: "/srv/data/caf\u{FFFD}.txt".into(),
            to: "/srv/data/cafe.txt".into(),
            raw: Some(raw.clone()),
        },
        WriteOp::Chmod {
            path: "/srv/data/caf\u{FFFD}.txt".into(),
            mode: 0o600,
            raw: Some(raw.clone()),
        },
    ];
    for op in &ops {
        super::apply_remote(&wired.line, &origin, op)
            .await
            .unwrap_or_else(|e| panic!("{} 没成：{e}", op.label()));
    }
    let got: Vec<serde_json::Value> = wired.log.lock().unwrap().clone();
    let b16 = serde_json::json!({ "b16": "636166e92e747874" });
    let want = vec![
        serde_json::json!({ "cmd": "files-delete", "args": { "root": "/srv/data", "rel": "build", "recursive": true } }),
        serde_json::json!({ "cmd": "files-delete", "args": { "root": "/srv/data", "rel": b16 } }),
        serde_json::json!({ "cmd": "files-rename", "args": { "root": "/srv/data", "from": b16, "to": "cafe.txt" } }),
        serde_json::json!({ "cmd": "files-chmod", "args": { "root": "/srv/data", "rel": b16, "mode": 0o600 } }),
    ];
    assert_eq!(got, want, "线上那几行与期望不等");
    // 往返上限：删目录那一件放宽，其余照旧。
    assert_eq!(super::budget_for(&ops[0]), super::TREE_BUDGET);
    assert_eq!(super::budget_for(&ops[1]), super::WRITE_BUDGET);
    assert!(super::TREE_BUDGET > super::WRITE_BUDGET);
}

/// 🔴 有损名**只有带着原始字节**才能写；两个显示串相同、字节不同的有损名是**两件**不同的操作。
#[test]
fn a_lossy_name_is_writable_only_with_its_bytes_and_two_lookalikes_stay_two_ops() {
    let bare = row("a\u{FFFD}", false, true);
    assert!(
        !is_writable(&bare),
        "有损而没有字节竟然能写 —— 那是拿显示串去寻址"
    );
    let with = |b: &[u8]| Listed {
        raw_name: Some(b.to_vec()),
        ..row("a\u{FFFD}", false, true)
    };
    let (x, y) = (with(b"a\xff"), with(b"a\xfe"));
    assert!(is_writable(&x) && is_writable(&y));
    assert_ne!(
        super::delete_op(&x),
        super::delete_op(&y),
        "🔴 两个不同的文件删起来是同一件操作 —— 「一次问完」那一步会认错"
    );
    // 改名框：有损名填的是显示串；敲回同一个显示串**不算**「就是原名」（真字节不是这几个字）。
    let mut p = WritePrompt::for_rename("/srv/data", &x);
    assert_eq!(p.text, "a\u{FFFD}");
    p.text = "a\u{FFFD}".into();
    assert!(
        p.to_op().is_ok(),
        "有损名改成它的显示串被当成了「没改」—— 那一下其实把乱码名改成了一个合法 UTF-8 名"
    );
}

/// 🔴 **批量改权限**：N 项一个框、出 N 件，每件带着自己的字节；框上按件数说话。
#[test]
fn one_chmod_box_for_many_rows_yields_one_op_per_row() {
    let a = row("a.bin", false, false);
    let d = row("sub", true, false);
    let l = Listed {
        raw_name: Some(b"\xff".to_vec()),
        ..row("\u{FFFD}", false, true)
    };
    let mut p = WritePrompt::for_chmod_many("/srv/data", &[&a, &d, &l]);
    assert_eq!(p.heading(), "把这 3 项的权限改成（八进制）：");
    assert_eq!(p.text, "", "批量那个框也不许预填（读不到现值）");
    p.text = "750".into();
    assert_eq!(
        p.to_ops().expect("合法的权限位被拒"),
        vec![
            WriteOp::Chmod {
                path: "/srv/data/a.bin".into(),
                mode: 0o750,
                raw: None,
            },
            WriteOp::Chmod {
                path: "/srv/data/sub".into(),
                mode: 0o750,
                raw: None,
            },
            WriteOp::Chmod {
                path: "/srv/data/\u{FFFD}".into(),
                mode: 0o750,
                raw: Some(b"\xff".to_vec()),
            },
        ]
    );
    assert!(p.to_op().is_err(), "三件的框被 `to_op` 当成了一件");
    // 空摞 ⇒ 拒，不出零件却说「做完了」。
    let mut empty = WritePrompt::for_chmod_many("/srv/data", &[]);
    empty.text = "644".into();
    assert!(empty.to_ops().is_err());
    // 一项的框与单改那个框是同一个（框上点名字）。
    assert_eq!(
        WritePrompt::for_chmod("/srv/data", &a),
        WritePrompt::for_chmod_many("/srv/data", &[&a])
    );
}

// ═══════════════════════════════════════════════════════════════════════
// 〔GP1 · 第四波〕改权限那个框显示现值
//
// 要求住址：`设计/60 §7`（改权限时显示现值）· `调研/第四波记录/FW5.md §四`（缺后端读口，已报备）·
// `调研/第四波记录/GP1.md §5`（P2）。后端那一半（`files-stat` 送 `mode`）的判据住后端
// `files::tests::gp1_stat_reports_the_declared_fields_and_the_real_mode_bits`。
// ═══════════════════════════════════════════════════════════════════════

/// P2：现值那一句 ＋ 预填 == 手写表（一项 · 多项同 · 多项不同 · 有一项读不到 · 空摞）。
#[test]
fn gp1_the_chmod_box_says_the_current_mode_and_prefills_only_what_it_really_read() {
    let r = |line: &str, prefill: Option<&str>| ModeReadout {
        line: line.to_string(),
        prefill: prefill.map(str::to_string),
    };
    let cells: [(&[Option<u32>], ModeReadout); 6] = [
        (&[Some(0o644)], r("现在是 644", Some("644"))),
        (&[Some(0o4755)], r("现在是 4755", Some("4755"))),
        (&[Some(0o750), Some(0o750)], r("现在是 750", Some("750"))),
        (
            &[Some(0o750), Some(0o644)],
            r("这几项现在的权限不一样", None),
        ),
        (&[Some(0o644), None], r("读不到现在的权限", None)),
        (&[], r("读不到现在的权限", None)),
    ];
    for (modes, want) in cells {
        assert_eq!(mode_readout(modes), want, "mode_readout({modes:?})");
    }
}

/// P2′：应答里的 `mode` 怎么读 —— 缺席 / 类型不对 / 超出低 12 位 ⇒ 当读不到（不猜一个值）。
#[test]
fn gp1_the_mode_is_read_from_the_stat_reply_or_not_at_all() {
    assert_eq!(mode_of(&serde_json::json!({"mode": 420})), Some(0o644));
    assert_eq!(
        mode_of(&serde_json::json!({"size": 1})),
        None,
        "非 unix 那台不送 ⇒ 读不到"
    );
    assert_eq!(mode_of(&serde_json::json!({"mode": "644"})), None);
    assert_eq!(mode_of(&serde_json::json!({"mode": -1})), None);
    assert_eq!(mode_of(&serde_json::json!({"mode": 0o10000})), None);
}

/// P2″：预填至多一次；用户已经动过那一栏 ⇒ 不盖；清空之后不再被塞回去。
#[test]
fn gp1_the_current_mode_is_prefilled_at_most_once_and_never_over_what_you_typed() {
    let a = row("a.bin", false, false);
    let got = mode_readout(&[Some(0o640)]);
    let mut p = WritePrompt::for_chmod("/srv/data", &a);
    assert_eq!(p.text, "", "框摆出来时空着（现值还没答回来）");
    apply_prefill(&mut p, &got);
    assert_eq!(p.text, "640");
    p.text.clear();
    apply_prefill(&mut p, &got);
    assert_eq!(p.text, "", "用户清空之后又被塞回去了");
    // 用户先动了手 ⇒ 不盖。
    let mut q = WritePrompt::for_chmod("/srv/data", &a);
    q.text = "7".into();
    apply_prefill(&mut q, &got);
    assert_eq!(q.text, "7");
    // 各项不同 ⇒ 不预填（替用户挑一个就是猜）。
    let mut m = WritePrompt::for_chmod_many("/srv/data", &[&a, &a]);
    apply_prefill(&mut m, &mode_readout(&[Some(0o600), Some(0o644)]));
    assert_eq!(m.text, "");
    // 读回来的值原样确认 ⇒ 出的那一件就是原值（点确认 = 不变，不会静默改坏）。
    p.text = "640".into();
    assert_eq!(
        p.to_op().expect("合法"),
        WriteOp::Chmod {
            path: "/srv/data/a.bin".into(),
            mode: 0o640,
            raw: None,
        }
    );
}

/// P2‴：现值那一趟的代数 —— 框换了之后，上一个框晚到的答案不许落到新框上。
#[test]
fn gp1_a_late_answer_for_an_old_box_never_lands_on_the_new_one() {
    let probe = ModeProbe::default();
    let old = probe.start();
    let new = probe.start();
    probe.land(old, vec![Some(0o777)]);
    assert_eq!(probe.readout(), None, "旧框那一趟的答案落到了新框上");
    probe.land(new, vec![Some(0o600)]);
    assert_eq!(
        probe.readout().map(|r| r.line),
        Some("现在是 600".to_string())
    );
}

// P2⁗（现值那一趟真上线、按盘上真文件答）住 `shell_tests.rs::gp1_the_mode_probe_asks_files_stat_per_target_over_the_wire`：
// 它要在临时目录里铺真文件、设真权限位，本文件留在单元层（`test_tiers` 分区）。
