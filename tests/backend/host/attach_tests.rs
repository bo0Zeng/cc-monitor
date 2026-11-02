//! 瘦客户端：假外层终端（键从一条通道进、画进一块内存），真托管进程（`sh` 假程序），沙箱数据目录。

use super::super::run::{run, Spec};
use super::super::wire::Size;
use super::*;
use crate::platform::fs::Own;
use std::sync::{Arc, Mutex};

fn sandbox_own(tag: &str) -> Own {
    let data = test_home::sandbox_root().join(format!("a-{tag}"));
    let _ = std::fs::remove_dir_all(&data);
    std::fs::create_dir_all(&data).expect("建沙箱数据目录");
    let d = data.to_string_lossy().into_owned();
    let home = test_home::sandbox_root()
        .join("home")
        .to_string_lossy()
        .into_owned();
    Own::issue_with(&move |k| match k {
        "CCM_DATA_DIR" => Some(d.clone()),
        "HOME" => Some(home.clone()),
        _ => None,
    })
}

async fn host(
    own: &Own,
    id: &str,
    script: &str,
) -> (
    std::path::PathBuf,
    tokio::task::JoinHandle<Result<(), Said>>,
) {
    let spec = Spec {
        id: id.into(),
        name: id.into(),
        size: Size { rows: 24, cols: 80 },
        cwd: String::new(),
        agent: "fake".into(),
        client: "terminal".into(),
        argv: vec!["sh".into(), "-c".into(), script.into()],
    };
    let (tx, rx) = tokio::sync::oneshot::channel();
    let own = own.clone();
    let task = tokio::spawn(async move {
        run(spec, &own, move |p| {
            let _ = tx.send(p.to_path_buf());
        })
        .await
    });
    (rx.await.expect("绑好了"), task)
}

#[derive(Clone, Default)]
struct Screen(Arc<Mutex<Vec<u8>>>);

impl Write for Screen {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Screen {
    /// 外层终端看到的样子：画出去的字节喂进一个 80×24 的终端。
    fn seen(&self) -> vt100::Parser {
        let mut p = vt100::Parser::new(24, 80, 0);
        p.process(&self.0.lock().unwrap());
        p
    }
    fn text(&self) -> String {
        self.seen().screen().contents()
    }
    fn raw(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

struct Fake {
    keys: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    _resized: tokio::sync::mpsc::UnboundedSender<()>,
    queued: tokio::sync::mpsc::UnboundedSender<Option<String>>,
    screen: Screen,
    task: tokio::task::JoinHandle<Result<Left, Said>>,
}

/// 假外层接的那个托管终端在断开 · 结束那一行里叫什么。
const NAME: &str = "t-name";

/// 离开备用屏（`?1049l`）之后往外层原屏写的那几个字节。
fn after_leave(s: &Screen) -> String {
    let raw = s.raw();
    let at = raw.rfind("\x1b[?1049l").expect("离开过备用屏") + "\x1b[?1049l".len();
    raw[at..].to_string()
}

fn attach_fake(sock: &std::path::Path) -> Fake {
    let (ktx, krx) = tokio::sync::mpsc::unbounded_channel();
    let (rtx, rrx) = tokio::sync::mpsc::unbounded_channel();
    let (qtx, qrx) = tokio::sync::mpsc::unbounded_channel();
    let screen = Screen::default();
    let outer = Outer {
        keys: krx,
        resized: rrx,
        size: Box::new(|| Some(Size { rows: 24, cols: 80 })),
        out: Box::new(screen.clone()),
        prefix: keys::DEFAULT_PREFIX,
        vi: false,
        in_tmux: false,
    };
    let sock = sock.to_path_buf();
    let task = tokio::spawn(async move { session(&sock, NAME, outer, &Vec::new, qrx).await });
    Fake {
        keys: ktx,
        _resized: rtx,
        queued: qtx,
        screen,
        task,
    }
}

/// 等外层终端上出现那段字（拍子跟着托管口走，判据这边只能轮询；每轮让出一下）。
async fn until_seen(s: &Screen, needle: &str) {
    // 按墙钟兜底、不按轮数：机器忙时让出一轮的耗时差几十倍，数轮数会在负载高时假红。
    let deadline = std::time::Instant::now() + guard_core::wall::CEILING;
    while std::time::Instant::now() < deadline {
        if s.text().contains(needle) || s.raw().contains(needle) {
            return;
        }
        tokio::task::yield_now().await;
        std::thread::yield_now();
    }
    panic!("没等到 {needle:?}：{}", s.text());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn detaching_leaves_the_terminal_running_and_reattaching_shows_the_same_screen() {
    let own = sandbox_own("detach");
    let (sock, task) = host(
        &own,
        "a-detach",
        r#"printf 'ready\r\n'; while read x; do printf 'echo:%s\r\n' "$x"; done"#,
    )
    .await;
    let a = attach_fake(&sock);
    until_seen(&a.screen, "ready").await;
    a.keys.send(b"hello\r".to_vec()).unwrap();
    until_seen(&a.screen, "echo:hello").await;
    let before = a.screen.seen();
    a.keys.send(b"\x02d".to_vec()).unwrap();
    assert_eq!(a.task.await.unwrap().unwrap(), Left::Detached);
    // 还原外层之后，原屏上印一行：断开了 · 照跑 · 怎么接回（A3-3-32-11）。
    assert_eq!(
        after_leave(&a.screen),
        format!(
            "{}\r\n",
            copy_text("beHost.attach.detached", &[("name", NAME)])
        )
    );
    // 外层的鼠标上报：接上时开了，断开后最后一次开关是关（A3-3-33）。
    mouse_left_off(&a.screen);
    // 托管与子进程照跑。
    assert_eq!(super::super::home::alive(&own).len(), 1);
    assert!(!a.screen.raw().ends_with("\x02"), "前缀没送进去");

    let b = attach_fake(&sock);
    until_seen(&b.screen, "echo:hello").await;
    let after = b.screen.seen();
    assert_eq!(
        before.screen().contents(),
        after.screen().contents(),
        "再接上屏幕逐格相同"
    );
    assert_eq!(
        before.screen().cursor_position(),
        after.screen().cursor_position()
    );
    b.keys.send(b"\x02x".to_vec()).unwrap();
    until_seen(&b.screen, &copy_text("beHost.attach.confirmEnd", &[])).await;
    b.keys.send(b"y".to_vec()).unwrap();
    assert!(matches!(b.task.await.unwrap().unwrap(), Left::Ended(_)));
    // 结束之后原屏上那一行带名字。
    assert_eq!(
        after_leave(&b.screen),
        format!(
            "{}\r\n",
            copy_text("beHost.attach.ended", &[("name", NAME)])
        )
    );
    task.await.unwrap().unwrap();
}

/// `前缀 D` 单子：种类那一列是那几个短词，末行一次图例。
#[test]
fn the_clients_sheet_uses_the_short_words_and_ends_with_the_legend() {
    let row =
        |client: u64, kind: WhoKind, front: Option<&str>, holder: bool, mine: bool| ClientRow {
            client,
            kind,
            front: front.map(str::to_string),
            pid: Some(7),
            since_ms: 0,
            last_ms: 0,
            holder,
            mine,
        };
    let lines = client_lines(&[
        row(1, WhoKind::Terminal, None, true, false),
        row(2, WhoKind::Backend, Some("desktop"), false, false),
        row(3, WhoKind::Terminal, None, false, true),
    ]);
    assert_eq!(lines.len(), 4);
    assert!(lines[0].contains(&copy_text("beTerminal.who.terminalWindow", &[])));
    assert!(lines[1].contains(&copy_text("beTerminal.who.desktop", &[])));
    assert_eq!(
        lines.last().map(String::as_str),
        Some(copy_text("beHost.attach.clientsLegend", &[]).as_str())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_viewer_types_nothing_until_enter_takes_over() {
    let own = sandbox_own("take");
    let (sock, task) = host(
        &own,
        "a-take",
        r#"printf 'ready\r\n'; while read x; do printf 'echo:%s\r\n' "$x"; done"#,
    )
    .await;
    let a = attach_fake(&sock);
    until_seen(&a.screen, "ready").await;
    // B 接上 ⇒ B 持有，A 只看：标题「[只看 宽×高]」，尺寸是伪终端的（跟持有者 B 的 80×24）。
    let b = attach_fake(&sock);
    until_seen(&b.screen, "ready").await;
    until_seen(
        &a.screen,
        &copy_text("beHost.attach.viewOnly", &[("size", "80×24")]),
    )
    .await;
    a.keys.send(b"zz".to_vec()).unwrap();
    until_seen(
        &a.screen,
        &copy_text(
            "beHost.attach.heldBy",
            &[("said", &copy_text("beTerminal.holder.terminalWindow", &[]))],
        ),
    )
    .await;
    // 回车接管（这个回车不送进去），之后送得进。
    a.keys.send(b"\r".to_vec()).unwrap();
    for _ in 0..2000 {
        tokio::task::yield_now().await;
    }
    a.keys.send(b"mine\r".to_vec()).unwrap();
    until_seen(&b.screen, "echo:mine").await;
    assert!(!b.screen.text().contains("echo:zz"), "只看时敲的键没送进去");
    assert!(
        !b.screen.text().contains("echo:\r"),
        "接管的那个回车没送进去"
    );
    b.keys.send(b"\x02x".to_vec()).unwrap();
    for _ in 0..2000 {
        tokio::task::yield_now().await;
    }
    b.keys.send(b"y".to_vec()).unwrap();
    task.await.unwrap().unwrap();
    let _ = a.task.await;
    let _ = b.task.await;
}

/// ★ 底行照说来的那一句摆：来「排队 2 条」⇒ 最底一行是它；换成「排队 1 条」⇒ 跟着换；里面出字之后照盖；清了 ⇒ 那一行照里面重画。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_bottom_line_shows_the_owner_written_queue_line() {
    let own = sandbox_own("queued");
    let (sock, task) = host(
        &own,
        "a-queued",
        r#"printf 'ready\r\n'; while read x; do printf 'echo:%s\r\n' "$x"; done"#,
    )
    .await;
    let a = attach_fake(&sock);
    until_seen(&a.screen, "ready").await;
    let line = |n: usize| copy_text("beQueue.said.waiting", &[("n", &n.to_string())]);
    let bottom = |s: &Screen| s.seen().screen().contents_between(23, 0, 23, 80);
    a.queued.send(Some(line(2))).unwrap();
    until_seen(&a.screen, &line(2)).await;
    assert!(
        bottom(&a.screen).contains(&line(2)),
        "盖在最底一行：{:?}",
        bottom(&a.screen)
    );
    a.queued.send(Some(line(1))).unwrap();
    until_seen(&a.screen, &line(1)).await;
    assert!(!a.screen.text().contains(&line(2)), "换了那一句");
    a.keys.send(b"hi\r".to_vec()).unwrap();
    until_seen(&a.screen, "echo:hi").await;
    assert!(
        bottom(&a.screen).contains(&line(1)),
        "出字之后照盖：{:?}",
        bottom(&a.screen)
    );
    a.queued.send(None).unwrap();
    for _ in 0..20_000 {
        if !a.screen.text().contains(&line(1)) {
            break;
        }
        tokio::task::yield_now().await;
        std::thread::yield_now();
    }
    assert!(
        !a.screen.text().contains(&line(1)),
        "清了 ⇒ 不再盖：{}",
        a.screen.text()
    );
    assert!(a.screen.text().contains("echo:hi"), "重画照里面");
    a.keys.send(b"\x02x".to_vec()).unwrap();
    until_seen(&a.screen, &copy_text("beHost.attach.confirmEnd", &[])).await;
    a.keys.send(b"y".to_vec()).unwrap();
    assert!(matches!(a.task.await.unwrap().unwrap(), Left::Ended(_)));
    task.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_session_ending_returns_the_childs_code() {
    let own = sandbox_own("code");
    let (sock, task) = host(&own, "a-code", r#"printf 'ready\r\n'; read x; exit 5"#).await;
    let a = attach_fake(&sock);
    until_seen(&a.screen, "ready").await;
    a.keys.send(b"\r".to_vec()).unwrap();
    assert_eq!(a.task.await.unwrap().unwrap(), Left::Ended(Some(5)));
    assert_eq!(
        after_leave(&a.screen),
        format!(
            "{}\r\n",
            copy_text("beHost.attach.ended", &[("name", NAME)])
        ),
        "出来时还原外层终端，原屏上印结束那一行"
    );
    task.await.unwrap().unwrap();
}

/// ★ 一接上就敲回车：托管那一屏已经有字时，welcome 一到外层就看得见；这时敲的回车照送进去，
/// 不能因为 welcome 说的还是换持有者之前的那一位而被当成「回车接管」吞掉。
/// 托管那一张账与瘦客户端直接接起来，逐帧喂：喂完 welcome 就敲，不等后面那几帧。
#[test]
fn enter_right_after_the_welcome_shows_the_screen_reaches_the_pty() {
    let mut hub = super::super::serve::Hub::new("h", Size { rows: 24, cols: 80 });
    assert!(hub.pty_out(b"ready\r\n").is_empty());
    let (id, outbox) = hub.connect(None, 0);
    let hello = Frame::Ctl(Ctl::Hello {
        proto: PROTO,
        build: "t".into(),
        want: Want::Input,
        size: Some(Size { rows: 24, cols: 80 }),
        who: Who {
            kind: WhoKind::Terminal,
            client: None,
        },
    });
    hub.frame(id, hello, 0);
    let (raw, _) = outbox.q.lock().unwrap().drain();
    let frames: Vec<Frame> = raw
        .iter()
        .map(|b| wire::decode(b).expect("解得开").expect("整帧").0)
        .collect();
    assert!(
        matches!(frames.first(), Some(Frame::Ctl(Ctl::Welcome { .. }))),
        "头一帧是 welcome：{frames:?}"
    );
    let (mut c, s) = bare(24, 80);
    assert!(c.frame(frames[0].clone()).is_none());
    assert!(s.text().contains("ready"), "welcome 一到就画出那一屏");
    let (sends, left) = c.key(b"\r", &no_list);
    assert!(left.is_none());
    assert_eq!(sends, vec![Frame::In(b"\r".to_vec())], "回车送进去了");
    for f in &frames[1..] {
        assert!(c.frame(f.clone()).is_none());
    }
    assert_eq!(
        hub.frame(id, Frame::In(b"\r".to_vec()), 1),
        vec![super::super::serve::Effect::Write(b"\r".to_vec())],
        "托管照写进伪终端"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn copy_mode_shows_history_from_the_host() {
    let own = sandbox_own("copy");
    let (sock, task) = host(
        &own,
        "a-copy",
        r#"i=1; while [ $i -le 60 ]; do printf 'row %s\r\n' $i; i=$((i+1)); done; read x"#,
    )
    .await;
    let a = attach_fake(&sock);
    until_seen(&a.screen, "row 60").await;
    a.keys.send(b"\x02[".to_vec()).unwrap();
    until_seen(&a.screen, "[0/").await;
    a.keys.send(b"\x1b[5~\x1b[5~".to_vec()).unwrap();
    until_seen(&a.screen, "row 5\n").await;
    a.keys.send(b"q".to_vec()).unwrap();
    until_seen(&a.screen, "row 60").await;
    a.keys.send(b"\r".to_vec()).unwrap();
    let _ = a.task.await;
    task.await.unwrap().unwrap();
}

// ── 不起托管进程、直接喂帧的那几条（外层是假的，尺寸由判据定）──

fn bare(outer_rows: u16, outer_cols: u16) -> (Client, Screen) {
    let (_k, keys) = tokio::sync::mpsc::unbounded_channel();
    let (_r, resized) = tokio::sync::mpsc::unbounded_channel();
    let screen = Screen::default();
    let outer = Outer {
        keys,
        resized,
        size: Box::new(move || {
            Some(Size {
                rows: outer_rows,
                cols: outer_cols,
            })
        }),
        out: Box::new(screen.clone()),
        prefix: keys::DEFAULT_PREFIX,
        vi: false,
        in_tmux: false,
    };
    (Client::new(outer), screen)
}

fn holder_of(mine: bool) -> Option<Holder> {
    Some(Holder {
        client: if mine { 1 } else { 2 },
        kind: WhoKind::Terminal,
        front: None,
        pid: None,
        mine,
    })
}

fn welcome(c: &mut Client, rows: u16, cols: u16, mine: bool) {
    assert!(c
        .frame(Frame::Ctl(Ctl::Welcome {
            proto: PROTO,
            id: "t".into(),
            size: Size { rows, cols },
            you: 1,
            holder: holder_of(mine),
            screen: String::new(),
        }))
        .is_none());
}

fn out(c: &mut Client, b: &[u8]) {
    assert!(c.frame(Frame::Out(b.to_vec())).is_none());
}

fn no_list() -> Vec<(String, String)> {
    Vec::new()
}

/// 里面开括号粘贴 · 焦点 ⇒ 持有者在外层也开；只看的不开，接管那一下补上。
#[test]
fn the_holder_mirrors_paste_and_focus_and_a_viewer_does_not() {
    let (mut c, s) = bare(24, 80);
    welcome(&mut c, 24, 80, true);
    out(&mut c, b"\x1b[?2004h\x1b[?1004h");
    assert!(s.raw().contains("\x1b[?2004h"), "持有者外层开括号粘贴");
    assert!(s.raw().contains("\x1b[?1004h"), "持有者外层开焦点");

    let (mut v, vs) = bare(24, 80);
    welcome(&mut v, 24, 80, false);
    out(&mut v, b"\x1b[?2004h\x1b[?1004h\x1b[?1000h\x1b[?1006h");
    for seq in ["\x1b[?2004h", "\x1b[?1004h"] {
        assert!(!vs.raw().contains(seq), "只看的不开 {seq:?}");
    }
    assert!(v
        .frame(Frame::Ctl(Ctl::Holder {
            holder: holder_of(true)
        }))
        .is_none());
    for seq in ["\x1b[?2004h", "\x1b[?1004h", "\x1b[?1000h", "\x1b[?1006h"] {
        assert!(vs.raw().contains(seq), "接管之后补开 {seq:?}");
    }
}

#[test]
fn the_cursor_shape_reaches_the_outer_terminal() {
    let (mut c, s) = bare(24, 80);
    welcome(&mut c, 24, 80, true);
    out(&mut c, b"\x1b[5 q");
    assert!(s.raw().contains("\x1b[5 q"));
}

/// 同步输出开着时屏照算、外层一个字都没收到；关上那一下整段画出。
#[test]
fn synchronized_output_is_drawn_only_when_it_closes() {
    let (mut c, s) = bare(24, 80);
    welcome(&mut c, 24, 80, true);
    out(&mut c, b"\x1b[?2026hhalf");
    out(&mut c, b" done");
    assert!(!s.raw().contains("half"), "开着时不画：{:?}", s.raw());
    out(&mut c, b"\x1b[?2026l");
    assert!(s.text().contains("half done"));
}

/// 伪终端 120×40、外层 80×24（只看的一端）⇒ 每行只画前 80 列、不折行；标题带伪终端的尺寸。
#[test]
fn a_smaller_outer_terminal_shows_the_top_left_crop_without_wrapping() {
    let (mut c, s) = bare(24, 80);
    welcome(&mut c, 40, 120, false);
    let mut b = Vec::new();
    for i in 0..40 {
        b.extend_from_slice(format!("L{i:02}{}R", "-".repeat(116)).as_bytes());
        if i < 39 {
            b.extend_from_slice(b"\r\n");
        }
    }
    out(&mut c, &b);
    let seen = s.seen();
    let rows: Vec<String> = seen.screen().rows(0, 80).collect();
    for (i, r) in rows.iter().enumerate() {
        assert_eq!(r, &format!("L{i:02}{}", "-".repeat(77)), "第 {i} 行");
    }
    assert!(s.raw().contains("120×40"), "标题带伪终端的尺寸");
    // 再来一行（滚一下）：照旧裁着画。
    out(&mut c, b"\r\nNEW");
    let rows: Vec<String> = s.seen().screen().rows(0, 80).collect();
    assert_eq!(rows[0], format!("L01{}", "-".repeat(77)));
    assert_eq!(rows[23], format!("L24{}", "-".repeat(77)));
}

/// 翻看着的时候子进程接着打印：本地屏照算，退出翻看外层就是最新一屏。
#[test]
fn output_during_copy_mode_is_kept_and_quitting_shows_the_latest_screen() {
    let (mut c, s) = bare(24, 80);
    welcome(&mut c, 24, 80, true);
    out(&mut c, b"first\r\n");
    let (sends, left) = c.key(b"\x02[", &no_list);
    assert!(left.is_none());
    assert!(sends
        .iter()
        .any(|f| matches!(f, Frame::Ctl(Ctl::Capture { .. }))));
    assert!(c
        .frame(Frame::Ctl(Ctl::Captured {
            lines: vec!["first".into()],
            capped: false,
            size: Size { rows: 24, cols: 80 },
            cursor: [1, 0],
        }))
        .is_none());
    out(&mut c, b"late line\r\n");
    assert!(!s.text().contains("late line"), "翻看着不画到外层");
    let _ = c.key(b"q", &no_list);
    assert!(s.text().contains("late line"), "退出翻看是最新一屏");
    assert!(s.text().contains("first"));
}

// ── 鼠标滚轮（A3-3-33）──

/// 外层收到过鼠标上报的开（SGR 1006 · 按键起落），而且每一样最后一次是关。
fn mouse_left_off(s: &Screen) {
    let raw = s.raw();
    for n in ["1000", "1006"] {
        let on = raw
            .rfind(&format!("\x1b[?{n}h"))
            .unwrap_or_else(|| panic!("外层开过 ?{n}"));
        assert!(
            raw[on..].contains(&format!("\x1b[?{n}l")),
            "?{n} 最后一次是关：{:?}",
            &raw[on..]
        );
    }
    for n in ["1002", "1003"] {
        if let Some(on) = raw.rfind(&format!("\x1b[?{n}h")) {
            assert!(
                raw[on..].contains(&format!("\x1b[?{n}l")),
                "?{n} 最后一次是关"
            );
        }
    }
}

const WHEEL_UP: &[u8] = b"\x1b[<64;10;5M";
const WHEEL_DOWN: &[u8] = b"\x1b[<65;10;5M";

fn rows_1_to_60(c: &mut Client) -> Vec<String> {
    let lines: Vec<String> = (1..=60).map(|i| format!("row {i}")).collect();
    out(c, lines.join("\r\n").as_bytes());
    lines
}

fn captured(c: &mut Client, lines: Vec<String>) {
    assert!(c
        .frame(Frame::Ctl(Ctl::Captured {
            lines,
            capped: false,
            size: Size { rows: 24, cols: 80 },
            cursor: [23, 6],
        }))
        .is_none());
}

fn first_row(s: &Screen) -> String {
    s.seen().screen().rows(0, 80).next().unwrap_or_default()
}

/// ★ 里面没开鼠标：滚轮往上 ⇒ 进翻看、外层第一行往上挪 5 行；再上 5 行；往下滚到底 ⇒ 退回实时。外层开着鼠标上报。
#[test]
fn the_wheel_scrolls_into_history_and_back_out_to_live() {
    let (mut c, s) = bare(24, 80);
    welcome(&mut c, 24, 80, true);
    let lines = rows_1_to_60(&mut c);
    assert!(s.raw().contains("\x1b[?1006h"), "外层开 SGR 鼠标上报");
    assert!(s.raw().contains("\x1b[?1000h"), "外层开按键起落上报");
    assert!(first_row(&s).starts_with("row 37"), "{:?}", first_row(&s));
    let (sends, left) = c.key(WHEEL_UP, &no_list);
    assert!(left.is_none());
    assert!(
        sends
            .iter()
            .any(|f| matches!(f, Frame::Ctl(Ctl::Capture { .. }))),
        "{sends:?}"
    );
    assert!(
        !sends.iter().any(|f| matches!(f, Frame::In(_))),
        "滚轮没送进去"
    );
    captured(&mut c, lines);
    assert!(first_row(&s).starts_with("row 32"), "{:?}", first_row(&s));
    let (sends, _) = c.key(WHEEL_UP, &no_list);
    assert!(sends.is_empty());
    assert!(first_row(&s).starts_with("row 27"), "{:?}", first_row(&s));
    let _ = c.key(WHEEL_DOWN, &no_list);
    assert!(first_row(&s).starts_with("row 32"), "{:?}", first_row(&s));
    let _ = c.key(WHEEL_DOWN, &no_list);
    assert_eq!(c.over, Over::None, "滚到底退回实时");
    assert!(first_row(&s).starts_with("row 37"), "{:?}", first_row(&s));
    assert!(s.text().contains("row 60"));
}

/// 翻看还没回来就滚了几格：照记着，回来时落在那儿。
#[test]
fn wheel_ticks_before_the_history_arrives_are_kept() {
    let (mut c, s) = bare(24, 80);
    welcome(&mut c, 24, 80, true);
    let lines = rows_1_to_60(&mut c);
    let _ = c.key(WHEEL_UP, &no_list);
    let (sends, _) = c.key(WHEEL_UP, &no_list);
    assert!(sends.is_empty(), "只要一次历史");
    captured(&mut c, lines);
    assert!(first_row(&s).starts_with("row 27"), "{:?}", first_row(&s));
}

/// 里面自己开了鼠标上报 ⇒ 持有者照里面的编码原样交进去，不进翻看。
#[test]
fn the_wheel_goes_to_the_inside_when_it_asked_for_the_mouse() {
    let (mut c, _s) = bare(24, 80);
    welcome(&mut c, 24, 80, true);
    out(&mut c, b"\x1b[?1000h\x1b[?1006h");
    let (sends, _) = c.key(WHEEL_UP, &no_list);
    assert_eq!(sends, vec![Frame::In(WHEEL_UP.to_vec())]);
    assert_eq!(c.over, Over::None);
    out(&mut c, b"\x1b[?1006l");
    let (sends, _) = c.key(b"\x1b[<0;3;4M", &no_list);
    assert_eq!(
        sends,
        vec![Frame::In(vec![0x1b, b'[', b'M', 32, 3 + 32, 4 + 32])],
        "里面没开 SGR ⇒ 老编码"
    );
}

/// 里面要拖动 ⇒ 持有者的外层也开拖动上报；关了退回只报按键起落。
#[test]
fn the_outer_reports_drags_only_while_the_inside_wants_them() {
    let (mut c, s) = bare(24, 80);
    welcome(&mut c, 24, 80, true);
    out(&mut c, b"\x1b[?1002h");
    assert!(s.raw().contains("\x1b[?1002h"));
    out(&mut c, b"\x1b[?1002l");
    let raw = s.raw();
    let on = raw.rfind("\x1b[?1002h").unwrap();
    assert!(raw[on..].contains("\x1b[?1002l"));
    assert!(raw[on..].contains("\x1b[?1000h"));
}

/// 里面没开鼠标时点一下 ⇒ 什么都不送（不把上报的字节当键送进去）。
#[test]
fn a_click_is_dropped_when_the_inside_did_not_ask_for_the_mouse() {
    let (mut c, _s) = bare(24, 80);
    welcome(&mut c, 24, 80, true);
    let (sends, _) = c.key(b"a\x1b[<0;3;4Mb\x1b[<0;3;4m", &no_list);
    assert_eq!(
        sends,
        vec![Frame::In(b"a".to_vec()), Frame::In(b"b".to_vec())]
    );
}

/// 只看的一端：滚轮照样进翻看；里面要鼠标时不交、也不弹「回车接管」那一行。
#[test]
fn a_viewer_scrolls_history_but_never_sends_the_mouse() {
    let (mut c, s) = bare(24, 80);
    welcome(&mut c, 24, 80, false);
    out(&mut c, b"x");
    assert!(s.raw().contains("\x1b[?1006h"), "只看的外层也开鼠标上报");
    let (sends, _) = c.key(WHEEL_UP, &no_list);
    assert!(sends
        .iter()
        .any(|f| matches!(f, Frame::Ctl(Ctl::Capture { .. }))));
    let (mut v, _vs) = bare(24, 80);
    welcome(&mut v, 24, 80, false);
    out(&mut v, b"\x1b[?1000h\x1b[?1006h");
    let (sends, _) = v.key(WHEEL_UP, &no_list);
    assert!(sends.is_empty());
    assert_eq!(v.over, Over::None);
}

/// 瘦客户端半路没了（出错 · panic · 任务被丢）⇒ 落地那一下照样离开备用屏、关鼠标上报。
#[test]
fn dropping_the_client_turns_the_mouse_off_and_leaves_the_alternate_screen() {
    let (mut c, s) = bare(24, 80);
    welcome(&mut c, 24, 80, true);
    out(&mut c, b"x");
    drop(c);
    mouse_left_off(&s);
    assert!(s.raw().ends_with("\x1b[?1049l"), "{:?}", s.raw());
}
