//! **本机会话内容走本机后端的 Line 帧** 的判据。
//!
//! 守的要求：「本机改走后端的 Line 帧，消掉 monitor 里第二套 watcher 与第二套 offsets/seq」·
//! 「一条流，一个来源；本机与远端走同一条帧路」· 级 3 无界堆积是禁止态。
//! 设计与读数住。
//!
//! | # | 判什么 | 异源 / 两向 |
//! |---|---|---|
//! | F1 | 本机那条流上分派第一层交回的帧种类 == 会话内容 ＋ 起停 ＋ 记录文件出声 ＋ `changed` ＋ `overflow`；每种帧都喂到 | 帧取跨语言金样（后端序列化器写）；期望手写 |
//! | F3 | 两条读循环各恰好一处把交回的帧送进本机内容通道（送法各按载体）、各恰好一处送「流结束」 | 源码锚，恰好一处 |
//! | F4 | 内容出口的调用方集合（`batch_to_payloads` / `on_line_batch_awaited` / `flush_lines` / `LineIntake::open` / `Batcher::new` / `SnapshotQueue::new`） | 全仓生产段扫描，两向集合相等 |
//! | F5 | 两条载体的起参是同一份常量，且其中每一个旗标都 ∈ 后端 `STREAM_FLAGS`；`--tail-only` 在 ⟺ 消费者认定 tail-only | 后端源码 —— 住 `stream_source/stream_flag_gate_tests.rs`（同一条跨半边已登记，不另开一条） |
//! | F6 | 快照被撤时的补偿归档：本机不补、远端补 | — |
//! | F7 | monitor 生产段里那套 watcher 的名字零命中（带正控） | — |
//! | Q1 | 本机后端推来的 `changed` 走生产那一串（分派第一层 → 本机通道 → `consume_local` → 第二层）⇒ `<local>` 上各主题的 `changed/<主题>` 订阅逐格收到；交这一格的生产调用方只有分派第二层 | 期望手写；全仓生产段扫描，两向集合相等 |
//! | F8 | 真后端 × 生产 stdio 读循环 ⇒ 宣告带 `path`/`lines`、新行的 `seq` == 行号（`#[ignore]`，由 `tests/evidence/CF1-local-lines.py` 带二进制跑） | 两侧各是真实现 |

use super::*;
use crate::stream_source::{LocalItem, Ordered};
use std::collections::BTreeSet;

// ─── 小工具：函数范围（rustfmt 保证收尾 `}` 与签名同缩进）───────────────────────────

fn fn_end(lines: &[&str], start: usize) -> usize {
    let indent: String = lines[start].chars().take_while(|c| *c == ' ').collect();
    let close = format!("{indent}}}");
    (start..lines.len())
        .find(|&k| lines[k] == close)
        .unwrap_or(lines.len() - 1)
}

/// 某一行所在的函数名（往回找 `fn <名>`，且它的体要包住这一行）。
fn enclosing_fn(lines: &[&str], at: usize) -> Option<String> {
    for i in (0..=at).rev() {
        let l = lines[i];
        let rest = l
            .split(" fn ")
            .nth(1)
            .or_else(|| l.trim_start().strip_prefix("fn "));
        if let Some(rest) = rest {
            let n: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !n.is_empty() && fn_end(lines, i) >= at {
                return Some(n);
            }
        }
    }
    None
}

/// 调用形 `needle`（如 `"flush_lines("`）在 monitor 全仓生产段里的 `(文件, 外层 fn)` 集合。
/// 定义处（`fn flush_lines(`）与更长名字的尾巴（`xflush_lines(`）不算。
fn callers_of(needle: &str) -> BTreeSet<(String, String)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = BTreeSet::new();
    for (path, raw) in guard_core::scan_tree!(&root, &["rs"]) {
        let prod = guard_core::production_code(&raw);
        // 按模块住址认：人群声明带进来的兄弟包（通信层 `comms-inward` 等）认作 `<包名>/…`。
        let file = guard_core::module_address(&root, &path);
        let lines: Vec<&str> = prod.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            let hit = l.match_indices(needle).any(|(k, _)| {
                let before = &l[..k];
                let prev = before.chars().next_back();
                !before.ends_with("fn ") && !prev.is_some_and(|c| c.is_alphanumeric() || c == '_')
            });
            if hit {
                let f = enclosing_fn(&lines, i)
                    .unwrap_or_else(|| panic!("{file} 里的 `{needle}` 不在任何 fn 里 —— 抽取坏了"));
                out.insert((file.clone(), f));
            }
        }
    }
    out
}

fn set(xs: &[(&str, &str)]) -> BTreeSet<(String, String)> {
    xs.iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

/// 造通道但**不起消费者**，把收端交给测试（F8 要直接看送进来的东西）。
/// 与生产的 `install` 共用同一个 `OnceLock` ⇒ 一个测试进程里只许调一次（第二次回 `None`）。
/// ⚠ 住测试文件里（本模块是 `local_lines` 的子模块，看得见那个私有 `SENDER`）：生产树里的
/// `#[cfg(test)]` 支撑项有递减棘轮（`structural_scan_tests` 的 P9 那一条）。
fn tap() -> Option<tokio::sync::mpsc::Receiver<LocalItem>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<LocalItem>(LOCAL_LINES_CAPACITY);
    SENDER.set(tx).ok()?;
    Some(rx)
}

// ─── F1 ────────────────────────────────────────────────────────────────────

const GOLDEN: &str = include_str!("../../__fixtures__/session-stream.golden.jsonl");

/// 本机那条流上，分派第一层交回（进本机内容通道）的恰好是会话内容与起停那几种；每种帧喂一行（跨语言金样的最少格那一行）。
#[test]
fn the_first_layer_hands_back_exactly_the_content_and_lifecycle_frames() {
    let mut fed = BTreeSet::new();
    let mut handed_back = BTreeSet::new();
    for line in GOLDEN.lines() {
        let f = crate::stream_source::decode(line)
            .unwrap_or_else(|e| panic!("金样解不开（{e}）：{line}"));
        let kind = serde_json::to_value(&f).unwrap()["kind"]
            .as_str()
            .unwrap()
            .to_string();
        fed.insert(kind.clone());
        if let Some(back) = crate::stream_source::route(
            crate::stream_source::Via::Local,
            &crate::origin::Origin::local(),
            None,
            f,
        ) {
            let back_kind = format!("{back:?}");
            let name: String = back_kind
                .chars()
                .take_while(|c| c.is_alphanumeric())
                .collect();
            let snake: String = name
                .chars()
                .enumerate()
                .flat_map(|(i, c)| {
                    let lower = c.to_ascii_lowercase();
                    if c.is_uppercase() && i > 0 {
                        vec!['_', lower]
                    } else {
                        vec![lower]
                    }
                })
                .collect();
            assert_eq!(snake, kind, "喂的是 `{kind}`，交回来的是别的：{back:?}");
            handed_back.insert(kind);
        }
    }
    assert_eq!(
        fed.len(),
        cmd_wire::frame_shapes::FRAMES.len() + 1,
        "金样没把每种帧都喂到：{fed:?}"
    );
    assert_eq!(
        handed_back,
        [
            "line",
            "session_added",
            // 读完了、从第几行起（本机消费者在这一帧排旁路快照）。
            "session_primed",
            "session_removed",
            // 「X 变了」（本机消费者交 `changed/<主题>` 订阅）。
            "changed",
            "session_state",
            "session_status",
            "sessions_replayed",
            // 记录文件的出声（同一条内容通道，与行同序）。
            "session_file_gone",
            "session_file_reread",
            // 运行表 · 主线外清单（会话成品，交会话账）。
            "session_runs",
            "session_branch",
            // 拥塞丢帧那一句（本机那条流也说）。
            "overflow",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect::<BTreeSet<_>>(),
        "本机分派第一层交回的帧种类不对：多交 ⇒ 别的帧混进内容通道；少交 ⇒ 本机那一种又被就地丢了"
    );
}

/// 分身会话不再在壳里整条丢（从前「显示分身会话」关掉 ⇒ 连它的批准框一起丢；隐藏只管占不占标签页，在等人的会话照样要人答）：
/// 本机与远端两条读循环里都没有那道门，也不再读那个开关。
#[test]
fn no_read_loop_drops_background_sessions() {
    for (name, src) in [
        (
            "stream_source/local.rs",
            include_str!("../../../src/frontend/shell/src/stream_source/local.rs"),
        ),
        (
            "stream_source/run.rs",
            include_str!("../../../src/frontend/shell/src/stream_source/run.rs"),
        ),
        (
            "lib.rs",
            include_str!("../../../src/frontend/shell/src/lib.rs"),
        ),
    ] {
        let code = guard_core::production_code(src);
        for gone in ["BgHide", "showBgSessions", "show_bg"] {
            assert!(
                !code.contains(gone),
                "{name} 里还有 {gone}：分身会话又在壳里被丢了"
            );
        }
    }
}

// ─── F3 ────────────────────────────────────────────────────────────────────

#[test]
fn both_read_loops_hand_content_frames_to_the_local_channel_exactly_once() {
    let stdio = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend.rs"
    ));
    let host = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend_host.rs"
    ));
    // stdio 载体是裸线程 ⇒ `_blocking` 那一形；常驻载体是 tokio 任务 ⇒ `.await` 那一形。
    for (name, src, deliver, ended, wrong) in [
        (
            "stdio 载体（local_backend·rs）",
            &stdio,
            "route.out.deliver(f);",
            "route.out.stream_ended();",
            "crate::local_lines::deliver(",
        ),
        (
            "常驻载体（local_backend_host·rs）",
            &host,
            "crate::local_lines::deliver(f).await;",
            "crate::local_lines::stream_ended().await;",
            "route.out.deliver(",
        ),
    ] {
        let d = guard_core::find_pinned(src, deliver)
            .unwrap_or_else(|e| panic!("{name}：`{deliver}` 不是恰好一处：{e}"));
        let e = guard_core::find_pinned(src, ended)
            .unwrap_or_else(|e| panic!("{name}：`{ended}` 不是恰好一处：{e}"));
        assert!(
            d < e,
            "{name}：「流结束」要送在读循环**之后**（它前面是送帧的那一处）"
        );
        // 送进通道的就是分派第一层交回来的那个 `Some(f)`：它紧跟在 `if let Some(f) = crate::stream_source::route(…) {` 后面。
        let head = &src[..d];
        let a = head
            .rfind("crate::stream_source::route(")
            .unwrap_or_else(|| panic!("{name}：送帧之前没有分派"));
        assert!(
            head[..a]
                .rfind("if let Some(f) =")
                .is_some_and(|k| a - k < 80),
            "{name}：送进通道的必须就是分派交回来的那一帧（`if let Some(f) = crate::stream_source::route(…)`）"
        );
        assert!(
            head[a..].trim_end().ends_with('{') && !head[a..].contains(';'),
            "{name}：分派与送帧之间不许夹别的语句"
        );
        assert!(
            !src.contains(wrong),
            "{name}：用了另一种载体的送法 `{wrong}`（tokio 任务里 blocking_send 会 panic；裸线程里没有 runtime 可 await）"
        );
    }
}

// ─── F4 ────────────────────────────────────────────────────────────────────

#[test]
fn every_line_leaves_through_the_one_intake_both_sources_share() {
    let cases: &[(&str, &[(&str, &str)], &str)] = &[
        (
            "batch_to_payloads(",
            &[("stream_source/batch.rs", "flush_lines")],
            "行 → 载荷只有一个出口；多一处 ⇒ 又长出一条不经 LineIntake 的内容路（本机 watcher 那一形）",
        ),
        (
            "on_line_batch_awaited(",
            &[("stream_source/batch.rs", "flush_lines")],
            "重放缓冲只有一个入口",
        ),
        (
            "flush_lines(",
            &[
                ("stream_source/snapshot.rs", "fetch_snapshot"),
                ("stream_source/batch.rs", "flush"),
                ("stream_source/batch.rs", "line"),
            ],
            "冲批只从 LineIntake 的两个口与旁路快照走",
        ),
        (
            "LineIntake::open(",
            &[
                ("stream_source/local.rs", "consume_local"),
                ("stream_source/run.rs", "on_attached"),
            ],
            "两个帧源（远端 on_attached · 本机 consume_local）各构造一次同一个收口",
        ),
        (
            "Batcher::new(",
            &[("stream_source/batch.rs", "open")],
            "攒批器只在收口里造 —— 远端若再自己造一个，就又是两份",
        ),
        (
            "SnapshotQueue::new(",
            &[("stream_source/batch.rs", "open")],
            "快照队列只在收口里造",
        ),
    ];
    for (needle, want, why) in cases {
        let got = callers_of(needle);
        assert!(!got.is_empty(), "`{needle}` 一个调用方都没扫到 —— 扫描坏了");
        assert_eq!(got, set(want), "`{needle}` 的生产调用方对不上：{why}");
    }
}

// ─── F7 ────────────────────────────────────────────────────────────────────

/// 那套 watcher 退场之后不许再出现的名字（生产段）。
const GONE: &[&str] = &[
    "crate::watcher",
    "mod watcher;",
    "initial_scan_done",
    "force_rescan",
];

fn hits_in(prod: &str) -> Vec<&'static str> {
    GONE.iter().copied().filter(|n| prod.contains(n)).collect()
}

#[test]
fn the_second_watcher_leaves_no_trace_in_monitor_production() {
    // 正控：同一个判定函数喂一段合成代码，四针全中。
    let synthetic = format!(
        "mod {w};\nuse crate::{w}::JsonlLine;\nlet {i} = h.{i};\nlet _ = {f}_tx.send(x);\n",
        w = "watcher",
        i = "initial_scan_done",
        f = "force_rescan"
    );
    assert_eq!(hits_in(&synthetic).len(), GONE.len(), "判定函数本身是瞎的");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut bad = Vec::new();
    let mut n = 0usize;
    for (path, raw) in guard_core::scan_tree!(&root, &["rs"]) {
        n += 1;
        let prod = guard_core::production_code(&raw);
        for h in hits_in(&prod) {
            bad.push(format!("{} · `{h}`", path.display()));
        }
    }
    assert!(n >= 90, "只扫到 {n} 份 .rs —— 遍历坏了");
    assert!(
        !root.join("watcher.rs").exists(),
        "`src/frontend/shell/src/watcher.rs` 又出现了 —— 那是第二套 watcher"
    );
    assert!(
        bad.is_empty(),
        "monitor 生产段里那套 watcher 的名字又出现了：\n{}",
        bad.join("\n")
    );
}

// ─── F8（真后端，`#[ignore]`）────────────────────────────────────────────────

/// **真后端（stdio 载体、隔离 HOME）× 生产读循环（`local_stdio_consumer`）× 本机内容通道。**
///
/// 由 `tests/evidence/CF1-local-lines.py` 带 `CF1_BACKEND`（后端二进制）来跑；没有那个变量就不该跑（`#[ignore]`）。
/// 夹具：一个活的 `sleep` 进程冒充会话（pidfile 的 `procStart` 取它自己的 `/proc` 启动时刻 —— 后端的冒名检查认这个），
/// 会话文件先写 3 行（其中一行是空白，不计行号）。起参就是生产那一份 `LOCAL_STREAM_ARGS`。
/// 期望：宣告帧带 `path` 与 `lines == 2`（prime 到的完整行数）；再追加一行 ⇒ `line` 帧 `seq == 2`、原文不变，
/// 且历史那两行**不**作为 `line` 帧重放（tail-only）；后端被杀 ⇒ 读循环收尾送「流结束」。
#[test]
#[ignore = "要真后端二进制：由 tests/evidence/CF1-local-lines.py 带 CF1_BACKEND 来跑"]
fn a_real_backend_feeds_local_lines_through_the_production_read_loop() {
    use std::io::Write;
    let _local = crate::inbound_client::local_origin_test_lock();
    let bin = std::env::var("CF1_BACKEND").expect("没有 CF1_BACKEND —— 这条只该由读数脚本来跑");
    let mut rx = tap().expect("本机内容通道已经被装过了 —— 这条要单独跑");
    let home = std::env::temp_dir().join(format!("cf1-local-lines-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let claude = home.join(".claude");
    let proj = claude.join("projects").join("-tmp-cf1");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::create_dir_all(claude.join("sessions")).unwrap();
    // 冒充会话的活进程。
    // ⚠ C7i：它不许带着「我在哪个 tmux 窗格里」—— 后端宣告时会去给那个窗格打 `@ccm_sid`。
    //   后端自己的 `TMUX` 也摘了、`TMUX_TMPDIR` 指进临时家目录（与 SR1a 那条同一套隔离）。
    let mut sleeper = std::process::Command::new("sleep")
        .arg("600")
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .spawn()
        .unwrap();
    let pid = sleeper.id();
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    let after_comm = &stat[stat.rfind(')').unwrap() + 2..];
    // `/proc/<pid>/stat` 第 22 列 starttime；`comm` 之后从第 3 列起数 ⇒ 下标 19。
    let ticks: u64 = after_comm.split(' ').nth(19).unwrap().parse().unwrap();
    let sid = "cf100000-0000-4000-8000-000000000001";
    let jsonl = proj.join(format!("{sid}.jsonl"));
    std::fs::write(
        &jsonl,
        "{\"type\":\"user\",\"n\":0}\n   \n{\"type\":\"assistant\",\"n\":1}\n",
    )
    .unwrap();
    std::fs::write(
        claude.join("sessions").join(format!("{pid}.json")),
        format!(
            "{{\"pid\":{pid},\"sessionId\":\"{sid}\",\"cwd\":\"/tmp/cf1\",\"kind\":\"interactive\",\"procStart\":\"{ticks}\"}}"
        ),
    )
    .unwrap();
    // 断言失败时也要把两个子进程收掉（否则后端握着继承来的 stderr，外层 cargo 要等到 `sleep 600` 自己退才收工）。
    struct Reap(Vec<u32>);
    impl Drop for Reap {
        fn drop(&mut self) {
            for pid in &self.0 {
                let _ = std::process::Command::new("kill")
                    .arg(pid.to_string())
                    .status();
            }
        }
    }
    let mut reap = Reap(vec![pid]);
    let mut child = std::process::Command::new(&bin)
        .args(crate::local_backend::LOCAL_STREAM_ARGS)
        .env("HOME", &home)
        .env("CLAUDE_CONFIG_DIR", &claude)
        .env("TMUX_TMPDIR", &home)
        .env_remove("TMUX")
        .env_remove("CCM_LISTEN_PORT")
        .env_remove("CCM_LISTEN_TOKEN_FILE")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("起不了后端");
    reap.0.push(child.id());
    let (stdin, stdout) = (child.stdin.take().unwrap(), child.stdout.take().unwrap());
    let reader = std::thread::spawn(move || {
        crate::local_backend::local_stdio_consumer(
            &crate::local_backend::StdioRoute::local(),
            stdin,
            stdout,
        )
    });
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let next = |rx: &mut tokio::sync::mpsc::Receiver<LocalItem>| {
        rt.block_on(async {
            tokio::time::timeout(guard_core::wall::CEILING, rx.recv())
                .await
                .expect("20 秒没等到下一件")
                .expect("通道断了")
        })
    };
    // ① 宣告（不等记录）之后读完那一帧：带 path 与预读到的完整行数；它之前不许有这个会话的行帧。
    let (path, lines) = loop {
        match next(&mut rx) {
            LocalItem::Frame(Ordered::SessionPrimed(p)) if p.sid == sid => break (p.path, p.lines),
            LocalItem::Frame(Ordered::Line(l)) if l.session_id == sid => {
                panic!("宣告之前就来了这个会话的行帧")
            }
            _ => continue,
        }
    };
    assert_eq!(path.as_deref(), Some(jsonl.to_string_lossy().as_ref()));
    assert_eq!(lines, Some(2), "prime 到的完整行数（空白行不计）");
    // ② 追加一行 ⇒ 下一条这个会话的 line 帧就是它：seq == 行号 2（历史两行不重放）。
    // 后端出成品：这一行要进得了界面（带链身份的 user 记录），帧上才有 `message` 可比。
    let appended = "{\"type\":\"user\",\"uuid\":\"u2\",\"timestamp\":\"t\",\"message\":{\"role\":\"user\",\"content\":\"x\"}}";
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&jsonl)
        .unwrap();
    writeln!(f, "{appended}").unwrap();
    drop(f);
    // ⚠ 后端先宣告（Phase 1 同步扫、prime 游标）、**之后**才挂上 `projects/` 的 watch ⇒ 恰好落在这两步之间的
    //   一次写不会有事件，要等下一次写才被读出来（游标已 prime，不丢，只是晚到 —— 首跑实打到过一次 20 s 空等）。
    //   真会话会一直写，这里照做：每 2 s 没等到就再补一行**空白行**（不计行号、不改编号）催一次事件。
    let wait_line = |rx: &mut tokio::sync::mpsc::Receiver<LocalItem>| {
        rt.block_on(async {
            tokio::time::timeout(guard_core::wall::CEILING, rx.recv())
                .await
                .ok()
                .map(|x| x.expect("通道断了"))
        })
    };
    let mut nudges = 0;
    let (seq, message) = loop {
        match wait_line(&mut rx) {
            Some(LocalItem::Frame(Ordered::Line(l))) if l.session_id == sid => {
                break (l.seq, l.record)
            }
            Some(_) => continue,
            None => {
                nudges += 1;
                assert!(nudges <= 10, "补了 10 次事件还没等到新行");
                let mut f = std::fs::OpenOptions::new()
                    .append(true)
                    .open(&jsonl)
                    .unwrap();
                writeln!(f, "  ").unwrap();
            }
        }
    };
    assert_eq!(seq, 2);
    let m = message.expect("后端没给那一行的成品");
    assert!(
        m.0.get().contains("\"id\":\"u2\""),
        "成品不是那一行：{}",
        m.0.get()
    );
    println!("CF1-LOCAL-LINES nudges={nudges}");
    // ③ 后端没了 ⇒ 读循环收尾送「流结束」。
    let _ = child.kill();
    let _ = child.wait();
    loop {
        if let LocalItem::StreamEnded = next(&mut rx) {
            break;
        }
    }
    let _ = reader.join();
    let _ = sleeper.kill();
    let _ = sleeper.wait();
    let _ = std::fs::remove_dir_all(&home);
    println!("CF1-LOCAL-LINES ok");
}

// ─── Q1 本机那条路上的 `changed` ⇒ 界面那条订阅收得到 ───────────────────────
//
// 要求住址：`INVARIANTS §40` 逐字「我的目的就是把本地当成不走 ssh 的远端」—— 远端那条收事那一头
// 收到 `changed` 就交 `changed/<主题>` 订阅；本机这条从前在吸收点丢过额度 · 账号清单 · 配置文件，界面只能等别的事件顺带重读。

/// 手写线上帧走生产那一串：`decode` → 分派第一层 → 本机内容通道 → `consume_local` → 第二层 → 重放缓冲；
/// 订了 `<local>` 上 `changed/quota` · `changed/rotation` · `changed/accounts` 的订阅各逐格收到自己那一格（期望手写）。
#[tokio::test]
async fn local_changed_pushes_reach_the_ui_subscription() {
    use crate::chan::wire::Item as WItem;
    use crate::event_replay::{EventReplay, ItemSink};
    #[derive(Default)]
    struct Rec(std::sync::Mutex<Vec<(u64, serde_json::Value)>>);
    impl ItemSink for Rec {
        fn deliver(&self, _: &str, sub: u64, items: Vec<WItem>) {
            for i in items {
                if let WItem::Frame { body, .. } = i {
                    self.0
                        .lock()
                        .unwrap()
                        .push((sub, serde_json::from_slice(&body.0).unwrap()));
                }
            }
        }
    }
    let replay = std::sync::Arc::new(EventReplay::new());
    let rec = std::sync::Arc::new(Rec::default());
    replay.attach_sink(rec.clone());
    for (id, topic) in [(1, "quota"), (2, "rotation"), (3, "accounts")] {
        replay.subscribe(
            "w",
            id,
            &crate::origin::Origin::local(),
            &format!("{}/{topic}", crate::event_replay::CHANGED_KIND),
            None,
            16,
        );
    }
    let (tx, rx) = tokio::sync::mpsc::channel::<LocalItem>(16);
    let health: crate::stream_source::HealthOut = std::sync::Arc::new(|_| Ok(()));
    let consumer = tokio::spawn(crate::stream_source::consume_local(
        crate::origin::LOCAL.to_string(),
        rx,
        replay.clone(),
        health,
    ));
    for line in [
        r#"{"kind":"changed","topic":"quota"}"#,
        r#"{"kind":"changed","topic":"rotation","key":"s1"}"#,
        r#"{"kind":"changed","topic":"accounts"}"#,
    ] {
        let f = crate::stream_source::decode(line).expect("手写帧解不出来");
        if let Some(f) = crate::stream_source::route(
            crate::stream_source::Via::Local,
            &crate::origin::Origin::local(),
            None,
            f,
        ) {
            tx.send(LocalItem::Frame(f)).await.unwrap();
        }
    }
    drop(tx);
    tokio::time::timeout(guard_core::wall::CEILING, consumer)
        .await
        .expect("本机消费者 5 秒没收摊")
        .unwrap();
    assert_eq!(
        *rec.0.lock().unwrap(),
        vec![
            (1, serde_json::json!({})),
            (2, serde_json::json!({"key": "s1"})),
            (3, serde_json::json!({})),
        ],
        "本机后端推来的 `changed`，界面那几条 `changed/<主题>` 订阅没收到（远端那条路收得到）"
    );
}

/// 两条路交界面的是同一个口：后端推来的 `changed` 那一格在 monitor 生产段里恰好由分派第二层调一处（本机远端同一处）。
/// 另一处是壳自己说的那一声：照机器表对齐完远端流（`machine_table::tell_followed`；本机后端那一帧 `changed {machines}` 分派那里不转）。
#[test]
fn both_paths_hand_changed_pushes_to_the_same_replay_entry() {
    let want = set(&[
        ("stream_source/dispatch.rs", "apply"),
        ("machine_table.rs", "tell_followed"),
    ]);
    for needle in ["replay.changed("] {
        assert_eq!(callers_of(needle), want, "`{needle}` 的生产调用方对不上");
    }
}
