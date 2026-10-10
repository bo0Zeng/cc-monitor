//! 窗口进程那一侧的躯体（`proc.rs`：读种子那一行 · 在 stdin / stdout 上起通道 · 列第一屏 · `child_main` 的行序）那几条判据 ——
//! 原住 `tests/frontend/shell/filewin/proc_tests.rs` 的后半，随躯体搬进独立包；进程形态（起进程 · 种子 · 就绪那一行）那一半留在 monitor 那一侧。

use super::*;

/// 一台合成远端的名字（带中文与连字符）。
fn synthetic_cfg() -> String {
    "台架-远端".to_string()
}

// ════════════════════════════════════════════════════════════════════════
// 🔴窗口进程在自己的 stdin / stdout 上起通道
// ════════════════════════════════════════════════════════════════════════

/// 🔴 **种子只读 stdin 的那一行，一个字节都不多拿**（之后同一根管子就是通道，多读进缓冲的就不在通道那一侧了）；
/// 一行都没有就 EOF ⇒ 空串（交给 `decode_request` 说「种子是空的」）。
/// ④ `child_main` 里「在 stdin / stdout 上起通道」排在开窗之前（源码行序，代理）。
#[test]
fn the_seed_is_exactly_the_first_line_and_the_channel_follows_on_the_same_pipes() {
    let mut r = std::io::Cursor::new(b"{\"seed\":1}\n\x00\x01frame-bytes".to_vec());
    assert_eq!(read_seed_line(&mut r).unwrap(), "{\"seed\":1}");
    assert_eq!(
        r.position() as usize,
        "{\"seed\":1}\n".len(),
        "种子之后的字节被多读走了 —— 通道那一侧会少一截"
    );
    let mut empty = std::io::Cursor::new(Vec::<u8>::new());
    assert_eq!(read_seed_line(&mut empty).unwrap(), "");
    let prod =
        guard_core::production_code(include_str!("../../../src/frontend/filewin/src/proc.rs"));
    // ④ 行序：在 stdin / stdout 上起通道排在开窗之前。
    let at_dial = guard_core::find_pinned(&prod, "line_over_stdio(req.frame)")
        .expect("child_main 里没有在 stdin / stdout 上起通道那一下");
    let at_open = guard_core::find_pinned(&prod, "super::shell::open_detached_seeded(")
        .expect("child_main 里没有开窗那一下");
    assert!(at_dial < at_open, "开窗排在起通道之前");
    // ⑤种子里每一格都真的交给了开窗那一下（漏交一格 ＝ 那一格在窗口那侧恒是默认值，
    //    种子对拍照样绿 —— 它只判「过得了进程边界」，判不了「过去之后有人接」）。
    for f in ["reveal", "bookmarks", "machines"] {
        let at = guard_core::find_pinned(&prod, &format!("        req.{f},\n"))
            .unwrap_or_else(|e| panic!("child_main 没把种子里的 `{f}` 交给开窗那一下：{e}"));
        assert!(at > at_open, "`req.{f}` 不在开窗那一下的实参里");
    }
    // 那台的名字（种子 `origin`）先造成窗口那一侧的 `Source`，再交给开窗那一下。
    let at_src = guard_core::find_pinned(&prod, "Source::remote(req.origin.clone())")
        .expect("child_main 没拿种子里那台的名字造 `Source`");
    let at_src_arg = guard_core::find_pinned(&prod[at_open..], "        source,\n")
        .expect("开窗那一下的实参里没有 `source`");
    assert!(at_src < at_open, "`Source` 造在开窗之后");
    let _ = at_src_arg;
    // ⑥〔09-28 裁 3〕第一屏：拨通之后、开窗之前列；列不出来就退（不开窗）；起点与那一屏是它列出来的那一份。
    let at_first = guard_core::find_pinned(
        &prod,
        "rt.block_on(first_screen(&line, &source, req.cwd.clone()))",
    )
    .expect("child_main 里没有列第一屏那一下");
    let at_refuse = guard_core::find_pinned(&prod, "Err(e) => return refuse(e, EXIT_NOT_LISTED),")
        .expect("第一屏列不出来那一支不是「说原话、退」");
    let at_ready = guard_core::find_pinned(&prod, "say(&Ready::Listed(rows.len()));")
        .expect("列出来之后没说就绪那一行");
    assert!(
        at_dial < at_first && at_first < at_refuse && at_refuse < at_ready && at_ready < at_open,
        "行序不是「拨 → 列 → 列不出来就退 → 说就绪 → 开窗」"
    );
    for f in ["cwd", "rows"] {
        let at = guard_core::find_pinned(&prod, &format!("        {f},\n"))
            .unwrap_or_else(|e| panic!("开窗那一下没收列出来的 `{f}`：{e}"));
        assert!(at > at_open, "列出来的 `{f}` 不在开窗那一下的实参里");
    }
}

/// 把 `find::testing::FakeBackend` 挂成宿主句柄的最小包装（`wire_up` 那一份不交出句柄本身）。
struct FakeBackendHost(std::sync::Mutex<crate::find::testing::FakeBackend>);

impl FakeBackendHost {
    fn new(be: crate::find::testing::FakeBackend) -> Self {
        Self(std::sync::Mutex::new(be))
    }
}

impl comms_inward::chan::router::Backends for FakeBackendHost {
    fn call(
        &self,
        _origin: comms_inward::chan::wire::Origin,
        op: comms_inward::chan::wire::Op,
        _payload: comms_inward::chan::wire::Body,
        _view: Option<serde_json::Value>,
        _left: std::time::Duration,
        _cancel: comms_inward::chan::wire::CancelToken,
    ) -> futures::future::BoxFuture<
        'static,
        Result<comms_inward::chan::wire::Body, comms_inward::chan::wire::CallError>,
    > {
        let known = self.0.lock().unwrap().offered.iter().any(|c| c == &op.0);
        Box::pin(async move {
            if known {
                Ok(comms_inward::chan::wire::Body(
                    b"{\"kind\":\"dir\"}".to_vec(),
                ))
            } else {
                Err(comms_inward::chan::wire::CallError::Peer {
                    why: comms_inward::chan::wire::PeerFault::Unsupported,
                })
            }
        })
    }

    fn subscribe(
        &self,
        _origin: comms_inward::chan::wire::Origin,
        _kind: comms_inward::chan::wire::Kind,
        _from: Option<comms_inward::chan::wire::Cursor>,
    ) -> futures::stream::BoxStream<'static, comms_inward::chan::wire::Item> {
        Box::pin(futures::stream::empty())
    }
}

/// 〔09-28 裁 3〕答 `files-home` / `files-ls` 的替身后端：记下问了哪几条；`refuse` 里的那条回「不行 + 原话」。
struct ScreenHost {
    asked: std::sync::Mutex<Vec<String>>,
    refuse: Option<&'static str>,
}

impl comms_inward::chan::router::Backends for ScreenHost {
    fn call(
        &self,
        _origin: comms_inward::chan::wire::Origin,
        op: comms_inward::chan::wire::Op,
        payload: comms_inward::chan::wire::Body,
        _view: Option<serde_json::Value>,
        _left: std::time::Duration,
        _cancel: comms_inward::chan::wire::CancelToken,
    ) -> futures::future::BoxFuture<
        'static,
        Result<comms_inward::chan::wire::Body, comms_inward::chan::wire::CallError>,
    > {
        self.asked.lock().unwrap().push(op.0.clone());
        let refused = self.refuse == Some(op.0.as_str());
        let args: serde_json::Value = serde_json::from_slice(&payload.0).unwrap_or_default();
        Box::pin(async move {
            let json = |v: serde_json::Value| {
                Ok(comms_inward::chan::wire::Body(v.to_string().into_bytes()))
            };
            if refused {
                return Err(comms_inward::chan::wire::CallError::Peer {
                    why: comms_inward::chan::wire::PeerFault::Refused {
                        body: comms_inward::chan::wire::Body(
                            r#"{"code":"io_failed","message":"那台说：没有这个目录"}"#
                                .as_bytes()
                                .to_vec(),
                        ),
                    },
                });
            }
            match op.0.as_str() {
                "files-home" => json(serde_json::json!({ "path": "/home/台架" })),
                "files-ls" => {
                    let dir = args
                        .get("path")
                        .and_then(|p| p.as_str())
                        .unwrap_or("?")
                        .to_string();
                    json(serde_json::json!({
                        "entries": [
                            { "path": format!("{dir}/甲"), "kind": "dir" },
                            { "path": format!("{dir}/乙.txt"), "kind": "file", "size": 3 },
                        ],
                        "truncated": false,
                    }))
                }
                _ => Err(comms_inward::chan::wire::CallError::Peer {
                    why: comms_inward::chan::wire::PeerFault::Unsupported,
                }),
            }
        })
    }

    fn subscribe(
        &self,
        _origin: comms_inward::chan::wire::Origin,
        _kind: comms_inward::chan::wire::Kind,
        _from: Option<comms_inward::chan::wire::Cursor>,
    ) -> futures::stream::BoxStream<'static, comms_inward::chan::wire::Item> {
        Box::pin(futures::stream::empty())
    }
}

/// 🔴**窗口进程自己问第一屏**（真通道口 ＋ 替身后端）：
/// ① 没给目录 ⇒ 先问 home、再列 home（恰好这两问，按这个顺序）；② 给了目录 ⇒ 只列它、**不问 home**；
/// ③ 列不出来 ⇒ 带那台的原话回错（窗口那侧据此说 `Failed` 并不开窗）；④ home 问不到 ⇒ 同样带原话、不去列。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_first_screen_asks_home_only_when_told_nothing() {
    async fn run(
        refuse: Option<&'static str>,
        cwd: Option<String>,
    ) -> (Result<(String, Vec<String>), String>, Vec<String>) {
        let be = std::sync::Arc::new(ScreenHost {
            asked: std::sync::Mutex::new(Vec::new()),
            refuse,
        });
        let line = crate::find::testing::wire(be.clone(), 1 << 20);
        let got = first_screen(&line, &Source::remote(synthetic_cfg()), cwd)
            .await
            .map(|(d, rows)| (d, rows.into_iter().map(|l| l.row.name).collect()));
        let asked = be.asked.lock().unwrap().clone();
        (got, asked)
    }
    // ①
    let (got, asked) = run(None, None).await;
    assert_eq!(
        asked,
        ["files-home", "files-ls"],
        "没给目录时问的不是「先 home、再列」"
    );
    let (dir, names) = got.expect("问得到 home、列得出来，却回了错");
    assert_eq!(dir, "/home/台架");
    assert_eq!(names.len(), 2, "那一屏不是替身答的那两行：{names:?}");
    // ②
    let (got, asked) = run(None, Some("/srv/给了".into())).await;
    assert_eq!(asked, ["files-ls"], "给了目录还去问了 home");
    assert_eq!(got.expect("列得出来").0, "/srv/给了");
    // ③
    let (got, asked) = run(Some("files-ls"), Some("/srv/不在".into())).await;
    assert_eq!(asked, ["files-ls"]);
    let e = got.expect_err("列不出来竟然回了一屏");
    assert!(e.contains("那台说：没有这个目录"), "原话没带回来：{e}");
    // ④
    let (got, asked) = run(Some("files-home"), None).await;
    assert_eq!(asked, ["files-home"], "home 问不到还去列了");
    assert!(got
        .expect_err("home 问不到竟然过了")
        .contains("那台说：没有这个目录"));
}

// ════════════════════════════════════════════════════════════════════════
// 🔴关掉窗口（或第一屏列不出来）之后，窗口进程在预算内**自己退**
// ════════════════════════════════════════════════════════════════════════
//
// 病形（Win11 真机 10-09 查出，Linux 同形）：`child_main` 收尾时放掉多线程运行时，
// 而通道的读半边是 `tokio::io::stdin()` —— 它在运行时的阻塞线程上卡在读 stdin；
// 父进程（monitor 的路由器）要等窗口进程的 stdout 收到 EOF 才放那根管子，stdin 永远不关
// ⇒ 运行时落地等那条阻塞线程，一直等 ⇒ 每关一扇窗留一个进程，直到 monitor 退出。
// 台架形状照真机那一份：起**真子进程**跑真 `child_main`、种子写进 stdin **不关**、
// stdout / stdin 交给真路由器、看它在预算内退不退。
//
// ⚠ 起的是判据自己这个测试二进制（只跑 [`worker_runs_the_real_child_main`] 那一格），不是那份 `[[bin]]`：
//   门禁跑 `--lib`，不构建 bin（同 `shell_tests::scenario_trips` 头注那一条理由）；两者跑的是同一个 `child_main`。
//   libtest 在跑那一格之前往 stdout 印一行 `running 1 test` —— 父进程逐字节读掉它，之后的字节才是通道。

/// 窗口进程收场的预算：关窗（或说完「列不出来」）之后多久之内要退。
/// 病形是**无限等**，所以预算取宽（机器满载时 egui 收场也要几秒），不拿它量快慢。
const EXIT_BUDGET: std::time::Duration = std::time::Duration::from_secs(20);

/// 工作面：真 `child_main`，读的是**这个进程自己的** stdin / stdout。只许父判据点起来。
#[test]
#[ignore = "工作面：只由本文件的父判据在子进程里点起来"]
fn worker_runs_the_real_child_main() {
    assert_eq!(
        std::env::var("CCM_FILEWIN_PROC_CHILD").ok().as_deref(),
        Some("1"),
        "这是一个工作面，只许由它的父判据在自己的子进程里点起来"
    );
    std::process::exit(child_main());
}

/// 父进程那一侧：一个跑着真 `child_main` 的子进程，它的 stdin / stdout 接在真路由器上（stdin 不关）。
struct RealChild {
    child: std::process::Child,
    rt: tokio::runtime::Runtime,
    ready: std::sync::mpsc::Receiver<String>,
    stderr: std::sync::Arc<std::sync::Mutex<String>>,
}

impl RealChild {
    fn start(
        origin: &str,
        cwd: Option<String>,
        refuse: Option<&'static str>,
        display: Option<&str>,
    ) -> Self {
        use std::io::{BufRead, Read, Write};
        let exe = std::env::current_exe().expect("拿不到这个测试二进制自己的路径");
        let mut cmd = std::process::Command::new(&exe);
        cmd.args([
            "--exact",
            "proc::tests::worker_runs_the_real_child_main",
            "--ignored",
            "--format=terse",
            "--test-threads=1",
        ])
        .env("CCM_FILEWIN_PROC_CHILD", "1")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
        match display {
            Some(d) => {
                cmd.env("DISPLAY", d);
            }
            None => {
                cmd.env_remove("DISPLAY").env_remove("WAYLAND_DISPLAY");
            }
        }
        let mut child = cmd
            .spawn()
            .unwrap_or_else(|e| panic!("拉不起子进程 {exe:?}：{e}"));

        // 种子：stdin 第一行，**不关**（管子交给下面的路由器，它攥到窗口进程的 stdout EOF 为止 —— 同 monitor）。
        let req = filewin_contract::OpenRequest {
            origin: origin.to_string(),
            cwd,
            reveal: None,
            frame: 1 << 20,
            bookmarks: None,
            view: None,
            machines: Vec::new(),
            work_area: None,
            theme: crate::theme::testing::default_theme(),
            local_line: String::new(),
        };
        let mut stdin = child.stdin.take().expect("stdin 管子");
        stdin
            .write_all(filewin_contract::encode_request(&req).unwrap().as_bytes())
            .and_then(|()| stdin.write_all(b"\n"))
            .and_then(|()| stdin.flush())
            .expect("写不进种子");

        // libtest 先印的那一行逐字节读掉（一个字节都不多拿：之后的字节是通道帧）。
        let mut stdout = child.stdout.take().expect("stdout 管子");
        let mut head = Vec::new();
        let mut one = [0u8; 1];
        while !head.ends_with(b"running 1 test\n") {
            match stdout.read(&mut one) {
                Ok(1) => head.push(one[0]),
                _ => panic!(
                    "子进程 stdout 在 libtest 那一行之前就断了：{:?}",
                    String::from_utf8_lossy(&head)
                ),
            }
            assert!(
                head.len() < 4096,
                "子进程 stdout 开头不是 libtest 那一行：{:?}",
                String::from_utf8_lossy(&head)
            );
        }

        // stderr：就绪那一行送出来，其余攒着（红的时候印）。
        let (tx, ready) = std::sync::mpsc::channel();
        let stderr = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        {
            let err = child.stderr.take().expect("stderr 管子");
            let all = stderr.clone();
            std::thread::spawn(move || {
                for line in std::io::BufReader::new(err).lines() {
                    let Ok(line) = line else { break };
                    if filewin_contract::is_ready_line(&line) {
                        let _ = tx.send(line.clone());
                    }
                    all.lock().unwrap().push_str(&format!("{line}\n"));
                }
            });
        }

        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let be = std::sync::Arc::new(ScreenHost {
            asked: std::sync::Mutex::new(Vec::new()),
            refuse,
        });
        rt.spawn(async move {
            let wr = tokio::process::ChildStdin::from_std(stdin).unwrap();
            let rd = tokio::process::ChildStdout::from_std(stdout).unwrap();
            let _ = comms_inward::chan::router::serve(
                rd,
                wr,
                comms_inward::chan::router::Terms { frame: 1 << 20 },
                be,
            )
            .await;
        });
        Self {
            child,
            rt,
            ready,
            stderr,
        }
    }

    /// 等就绪那一行（解成 [`Ready`]）。
    fn ready(&self, budget: std::time::Duration) -> Ready {
        let line = self.ready.recv_timeout(budget).unwrap_or_else(|_| {
            panic!(
                "子进程 {budget:?} 内没说就绪那一行。stderr：\n{}",
                self.stderr.lock().unwrap()
            )
        });
        filewin_contract::decode_ready(&line).expect("就绪那一行解不出来")
    }

    /// 在 `budget` 内退了 ⇒ 退出码；没退 ⇒ 杀掉、回 `Err(已等多久)`。
    fn exit_within(
        &mut self,
        budget: std::time::Duration,
    ) -> Result<Option<i32>, std::time::Duration> {
        let t0 = std::time::Instant::now();
        while t0.elapsed() < budget {
            if let Ok(Some(st)) = self.child.try_wait() {
                return Ok(st.code());
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        Err(t0.elapsed())
    }
}

impl Drop for RealChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = &self.rt;
    }
}

/// 🔴**第一屏列不出来 ⇒ 说原话、退 —— 父进程攥着 stdin 不关，照样在预算内退**（退出码 [`EXIT_NOT_LISTED`]）。
/// 不要图形会话（窗口一个都不开），哪台机器都量得到；关窗那一形住下面 Xvfb 那一格。
#[test]
fn a_window_process_that_cannot_list_exits_even_though_stdin_stays_open() {
    let mut c = RealChild::start("台架-退", Some("/srv/不在".into()), Some("files-ls"), None);
    match c.ready(EXIT_BUDGET) {
        Ready::Failed(why) => assert!(why.contains("那台说：没有这个目录"), "原话没带上：{why}"),
        r => panic!("列不出来却说了 {r:?}"),
    }
    match c.exit_within(EXIT_BUDGET) {
        Ok(code) => assert_eq!(code, Some(EXIT_NOT_LISTED), "退了，但退出码不对"),
        Err(waited) => panic!(
            "说完「列不出来」之后 {waited:?} 还没退（父进程没关 stdin —— 生产上 monitor 就是这样）：\
             运行时落地在等卡着读 stdin 的那条阻塞线程。stderr：\n{}",
            c.stderr.lock().unwrap()
        ),
    }
}

/// 🔴**关掉窗口 ⇒ 窗口进程在预算内退（退出码 0），父进程攥着 stdin 不关。**
/// 真 X 服务器（Xvfb）上开真窗口，像窗口管理器那样请它关（`WM_DELETE_WINDOW`）。
/// ⚠ 买不到真桌面那一层（窗口管理器 · Windows）——Windows 那一形由真机台架量（交回里写读数）。
#[cfg(not(windows))]
#[test]
fn closing_the_window_ends_the_process_even_though_stdin_stays_open() {
    use crate::rows::testing::xvfb;
    const ORIGIN: &str = "proc-exit-origin";
    let _guard = xvfb::exclusive();
    xvfb::require_toolbox("「关掉窗口进程就退」");
    let screen = xvfb::Screen::start()
        .unwrap_or_else(|e| panic!("起不了 Xvfb ⇒ 这一格判不了，不是过了：{e}"));
    let mut c = RealChild::start(ORIGIN, Some("/srv/在".into()), None, Some(screen.display()));
    assert_eq!(
        c.ready(EXIT_BUDGET),
        Ready::Listed(2),
        "第一屏不是替身答的那两行"
    );
    let ids = xvfb::wait_for_windows(screen.display(), ORIGIN, 20_000);
    assert_eq!(
        ids.len(),
        1,
        "窗口没起来（数到 {} 个）。stderr：\n{}",
        ids.len(),
        c.stderr.lock().unwrap()
    );
    xvfb::close_like_a_wm(screen.display(), &ids[0]).expect("关窗那条消息没送到");
    match c.exit_within(EXIT_BUDGET) {
        Ok(code) => assert_eq!(code, Some(0), "关窗后退了，但退出码不是「开过又关了」"),
        Err(waited) => panic!(
            "关窗之后 {waited:?} 还没退（父进程没关 stdin —— 生产上 monitor 就是这样）：\
             运行时落地在等卡着读 stdin 的那条阻塞线程。stderr：\n{}",
            c.stderr.lock().unwrap()
        ),
    }
}
