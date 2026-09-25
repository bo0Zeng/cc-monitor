//! 〔C4d · 第四波 4B〕`remote_ask` 的判据。
//!
//! # 守的要求（住址）
//!
//! - 主会话 09-25 裁（`调研/第四波记录/C4d.md` 「主会话裁」第 1 条，逐字）：「**口收成一份**：把 `DialRemote` ＋ 可达表从
//!   `asset_sync.rs` 提到中立住址 `src/backend/remote_ask.rs`（逻辑一字不改），`asset_sync` 改调它；判据钉
//!   「**后端生产树里开远端一次性 exec 的只有这一处**」」。
//! - `设计/01 §3.5`（逐字）：「观测方沿它本来就拥有的那条连接去拉被观测方。」
//!
//! # 判据
//!
//! 1. **一个家**（零命中 ＋ 正控，两向相等）：后端生产树里「开远端一次性 exec」的三个指纹 ——
//!    调 `uses::run(`（经池开一条链路）· 往拨号请求里钉 `abort_marker`（capture 见 hello 就收工）·
//!    `parse_request_value(`（把一份拨号请求读成可拨的形状）—— 在 `dial/` 之外**只**出现在本模块；
//!    `dial/` 自己那几处是正控（同一识别器在 `dial/link.rs` 上命中，扫描没瞎）。
//! 2. **可达表只有一个写口**：`remote-reach` 与 `assets-sync` 登记同一张表、同一个函数（行为：两条路登记后表逐格相等）。
//! 3. **问法**：查不到那台 ⇒ 明说、对面一次都没被调；查得到 ⇒ 交给对面的恰是表里那份拨号请求 ＋ 逐格引号的命令行。
//! 4. **引号**：命令行交给真 `sh` 跑，每一格原样回来（异源：真 shell 对我们的引号）。
//!
//! # 买不到
//!
//! - 🔴 真远端：`DialRemote`（capture 那一跳）没对真 sshd 跑过（同 AS2）；判据 3 用替身对面。

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex as StdMutex;

/// 生产段（剥测试模块 ＋ 整行注释 ＋ 行尾注释）—— 指纹只认代码，不认说起它的散文。
fn code_of(body: &str) -> String {
    let prod = crate::guard_support::production_code(body);
    guard_core::strip_trailing_comments(&guard_core::strip_comment_lines(&prod))
}

/// 后端生产树里，含 `needle` 的文件（相对 `src/backend/`）。
fn homes_of(needle: &str) -> (std::collections::BTreeSet<String>, usize) {
    let root = crate::guard_support::src_root();
    let mut scanned = 0usize;
    let mut homes = std::collections::BTreeSet::new();
    for (path, body) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        scanned += 1;
        if code_of(&body).contains(needle) {
            homes.insert(
                path.strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    (homes, scanned)
}

/// ★ 判据 1：开远端一次性 exec 的指纹，在 `dial/` 之外只住本模块（两向相等）；`dial/` 里的是正控。
#[test]
fn only_this_module_opens_a_one_shot_exec_on_a_remote() {
    // (指纹, dial/ 里应当命中的那几份 —— 正控)
    let prints: &[(&str, &[&str])] = &[
        ("uses::run(", &["dial/link.rs"]),
        ("abort_marker", &["dial/uses.rs"]),
        (
            "parse_request_value(",
            &["dial/link.rs", "dial/mod.rs", "dial/sftp.rs"],
        ),
    ];
    for (needle, inside_dial) in prints {
        let (homes, scanned) = homes_of(needle);
        assert!(scanned > 100, "只扫到 {scanned} 份后端源码 —— 遍历坏了");
        let outside: std::collections::BTreeSet<String> = homes
            .iter()
            .filter(|p| !p.starts_with("dial/"))
            .cloned()
            .collect();
        let want: std::collections::BTreeSet<String> = ["remote_ask.rs".to_string()].into();
        assert_eq!(
            outside, want,
            "`{needle}` 在 `dial/` 之外的家对不上 —— 多 = 又长出一个自己跑远端 exec 的地方（该改调 `remote_ask::ask`）；\
             少 = 本模块不再经这一跳（空转）"
        );
        // 正控：同一识别器在 `dial/` 里命中登记的那几份（扫描没瞎、剥法没把代码剥掉）。
        for p in *inside_dial {
            assert!(
                homes.contains(*p),
                "正控失败：`{needle}` 在 `{p}` 里没命中 —— 识别器瞎了，上面那条零命中不可信"
            );
        }
    }
}

/// 一个记账的替身对面：记下每次被交的 (拨号请求, 命令)，答一个固定串。
#[derive(Default)]
struct Recorder {
    calls: StdMutex<Vec<(Value, String)>>,
    n: AtomicUsize,
}

impl Remote for Recorder {
    fn run<'a>(
        &'a self,
        dial: &'a Value,
        command: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        self.n.fetch_add(1, Ordering::SeqCst);
        self.calls.lock().unwrap().push((dial.clone(), command));
        Box::pin(async { Ok("答".to_string()) })
    }
}

fn reach_args(origin: &str, host: &str, backend: &str) -> Value {
    json!({
        "origin": origin,
        "dial": {"host": host, "port": 22, "user": "u", "key_path": "/k"},
        "backend": backend,
    })
}

/// ★ 判据 3（反向）：查不到那台 ⇒ 明说是哪台、对面一次都没被调（不猜、不回落）。
#[tokio::test]
async fn an_unregistered_origin_is_said_and_never_dialed() {
    let table = Table::default();
    let far = Recorder::default();
    let e = ask_with("nowhere", &["--list-projects"], &table, &far)
        .await
        .expect_err("没登记的那台不许问出东西来");
    assert_eq!(e, unreachable_message("nowhere"));
    assert!(e.contains("[nowhere]"), "那句话要点名是哪台：{e}");
    assert_eq!(far.n.load(Ordering::SeqCst), 0, "没登记也去拨了");
}

/// ★ 判据 3（正向）：交给对面的恰是表里那份拨号请求 ＋ 逐格引号的命令行。
#[tokio::test]
async fn a_registered_origin_is_asked_with_exactly_its_dial_and_a_quoted_command() {
    let table = Table::default();
    answer_reach_with(&reach_args("dev", "10.0.0.2", "/opt/c c/ccm"), &table).unwrap();
    let far = Recorder::default();
    let out = ask_with("dev", &["--list-sessions", "-home-u-it's"], &table, &far)
        .await
        .unwrap();
    assert_eq!(out, "答");
    let calls = far.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].0,
        reach_args("dev", "10.0.0.2", "/opt/c c/ccm")["dial"]
    );
    // 期望值手写成字面量（不拿 `command_line` 去比它自己 —— 死值验 A3：那样两侧同源，拿掉引号也恒绿）。
    // 逐格单引号；格内的 `'` 写成 `'\''`（POSIX 单引号里没有转义：先关、给一个转义过的单引号、再开）。
    assert_eq!(
        calls[0].1,
        r#"'/opt/c c/ccm' '--list-sessions' '-home-u-it'\''s'"#
    );
}

/// ★ 判据 2：`remote-reach` 与 `assets-sync` 登记的是同一张表、同一个写口 —— 两条路登记同一台之后表逐格相等；
/// 再登记一次换掉拨号请求与路径、对面的 id 留着。
#[test]
fn both_doors_register_through_the_one_writer() {
    let a = Table::default();
    let b = Table::default();
    answer_reach_with(&reach_args("dev", "h1", "/b1"), &a).unwrap();
    register(&b, &reach_args("dev", "h1", "/b1")).unwrap();
    assert_eq!(*lock(&a), *lock(&b), "两扇门登记出来的表不一样");
    // 对面的 id（资产目录那一路拉回来之后写的）在再登记时留着。
    lock(&a).get_mut("dev").unwrap().peer = Some("p".into());
    answer_reach_with(&reach_args("dev", "h2", "/b2"), &a).unwrap();
    let row = lock(&a).get("dev").cloned().unwrap();
    assert_eq!(row.peer.as_deref(), Some("p"));
    assert_eq!(row.backend, "/b2");
    assert_eq!(row.dial["host"], "h2");
    // 半给的入参拒，表不动。
    for bad in [
        json!({}),
        json!({"origin": ""}),
        json!({"origin": "x", "dial": {}}),
        json!({"origin": "x", "backend": "/b"}),
        json!({"origin": "x", "dial": "nope", "backend": "/b"}),
    ] {
        let before = lock(&a).clone();
        let e = answer_reach_with(&bad, &a).expect_err("半给的入参该拒");
        assert_eq!(e.0, "bad_args");
        assert_eq!(*lock(&a), before, "拒了还动了表：{bad}");
    }
}

/// 可达表有界：满了拒新的一台，已在表里的照样能再登记。
#[test]
fn the_table_is_bounded() {
    let t = Table::default();
    for i in 0..MAX_REACH {
        register(&t, &reach_args(&format!("m{i}"), "h", "/b")).unwrap();
    }
    assert_eq!(
        register(&t, &reach_args("one-more", "h", "/b"))
            .unwrap_err()
            .0,
        "bad_args"
    );
    register(&t, &reach_args("m0", "h9", "/b")).expect("已在表里的那台照样能再登记");
    assert_eq!(lock(&t).len(), MAX_REACH);
}

/// ★ 判据 4：命令行交给真 `sh`，每一格原样回来（带单引号 / 双引号 / `$HOME` / 反引号 / 空格 / 中文）。
#[cfg(unix)]
#[test]
fn a_real_posix_shell_reads_every_argument_back_verbatim() {
    let tricky = ["it's", "say \"hi\"", "$HOME", "`id`", "a b", "中文-项目"];
    let mut argv = vec!["%s\\n"];
    argv.extend(tricky.iter());
    let line = command_line("printf", &argv);
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(&line)
        .output()
        .expect("起 sh");
    assert!(out.status.success(), "sh 没跑通：{line}");
    let got: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(
        got,
        tricky.iter().map(|s| s.to_string()).collect::<Vec<_>>()
    );
}
