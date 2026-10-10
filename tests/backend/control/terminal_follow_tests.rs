//! 终端实时预览（`control/terminal_follow.rs`）的判据：真 tmux（隔离 socket，显式 `-S`），不碰任何默认 socket。
//!
//! | 性质 | 判据 |
//! |---|---|
//! | 订上 ⇒ 当场推第一帧（与抓一屏同一份成品 ＋ 票 ＋ 序号） | `first_frame_comes_at_once_with_the_preview_product` |
//! | 一帧在途：没回执之前画面再变也不推；回执之后推最新那一屏（中间的合并掉） | `one_frame_in_flight_until_acked` |
//! | 画面没变（指纹相同）不推 | 同上 |
//! | 订阅那个只读、不改尺寸的客户端不算「连着几个终端窗口」（名单 `clients` 与 ↗ 的 `list-clients` 两处）；tmux 快照读出来的不变；本仓不装客户端那一族钩子 | `the_follower_is_not_counted_as_a_terminal_window` |
//! | 退订 ⇒ 那个客户端没了、不再推；连接走了（票表丢了）⇒ 同样全收 | `unfollow_and_drop_reap_the_client` |
//! | 窗格没了 ⇒ 推一帧结束（`gone`） | `a_closed_pane_ends_the_follow` |
//! | 形状：票重复 / 不认的票 / 超过上限 / 目标不在名单 ⇒ 各自的码 | `shape_and_limits` |
//! | 退订先到（订阅那一问还没进来）⇒ 之后那一问不起客户端、不占票 | `an_unfollow_that_comes_first_is_final` |
//! | 退订落在起客户端那几下中间 ⇒ 起好的客户端当场收掉、不推、不占票 | `an_unfollow_while_starting_reaps_what_was_started` |
//! | 正在起的票也占名额、也算重复；满了回 `too_many_follows` | `a_seat_being_started_counts` |
//! | 快速订 / 退几十轮（切标签页 · 重载）之后照样订得满 | `rapid_follow_unfollow_never_fills_the_desk` |
//! | 停了的帧带后端写好的那一句；拒绝分「只能快照 / 停了」两类 | `a_closed_pane_ends_the_follow` · `refusals_say_which_way_live_went` |
//! | tmux 版本：3.2 起才有只读不改尺寸的客户端 | `tmux_version_gate` |
use super::*;
use serde_json::json;
use std::process::Command;
use std::time::{Duration, Instant};

struct Iso {
    sock: std::path::PathBuf,
}

impl Iso {
    fn new(tag: &str) -> Iso {
        let dir = std::env::temp_dir().join(format!("ccm-follow-{}-{tag}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        Iso {
            sock: dir.join("s"),
        }
    }
    fn socket(&self) -> Option<String> {
        self.sock.to_str().map(str::to_string)
    }
    fn tmux(&self, args: &[&str]) -> std::process::Output {
        Command::new("tmux")
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .arg("-S")
            .arg(&self.sock)
            .args(args)
            .output()
            .expect("tmux 不可执行 —— 本测试要求环境有 tmux")
    }
    /// 起一个会话：前台一个 `cat`（回显打进去的字）。
    fn session(&self, name: &str) {
        let o = self.tmux(&[
            "-f",
            "/dev/null",
            "new-session",
            "-d",
            "-x",
            "80",
            "-y",
            "10",
            "-s",
            name,
            "cat",
        ]);
        assert!(
            o.status.success(),
            "建会话失败：{}",
            String::from_utf8_lossy(&o.stderr)
        );
    }
    fn handle_of(&self, name: &str) -> String {
        let on = crate::control::terminals::On {
            socket: self.sock.to_str(),
        };
        let l = crate::control::terminals::list_on(on, &json!({})).expect("名单");
        l["terminals"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["tmux_name"] == name)
            .unwrap_or_else(|| panic!("名单里没有 {name}：{l}"))["terminal"]
            .as_str()
            .unwrap()
            .to_string()
    }
    fn type_in(&self, name: &str, text: &str) {
        self.tmux(&["send-keys", "-t", &format!("={name}:"), "-l", text]);
    }
    /// 此刻连在这台 tmux 上的客户端里有几个是控制模式的。
    fn control_clients(&self) -> usize {
        let o = self.tmux(&["list-clients", "-F", "#{client_control_mode}"]);
        String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter(|l| l.trim() == "1")
            .count()
    }
}

impl Drop for Iso {
    fn drop(&mut self) {
        let o = self.tmux(&["display-message", "-p", "#{pid}"]);
        let pid = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if pid.chars().all(|c| c.is_ascii_digit()) && !pid.is_empty() {
            let _ = Command::new("kill").arg(&pid).status();
        }
        let _ = std::fs::remove_dir_all(self.sock.parent().unwrap());
    }
}

/// 有界等待（夹具侧）：下一帧（至多 `within`）。
fn next_frame(rx: &mut tokio::sync::mpsc::Receiver<Frame>, within: Duration) -> Option<Frame> {
    let until = Instant::now() + within;
    loop {
        match rx.try_recv() {
            Ok(f) => return Some(f),
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => return None,
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                if Instant::now() >= until {
                    return None;
                }
                std::thread::sleep(Duration::from_millis(15));
            }
        }
    }
}

/// 有界等待：条件成立（至多 3 秒）。
fn eventually(mut f: impl FnMut() -> bool) -> bool {
    let until = Instant::now() + Duration::from_secs(3);
    while Instant::now() < until {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    f()
}

fn screen_of(f: &Frame) -> (String, u64, Value) {
    match f {
        Frame::TerminalScreen { ticket, seq, view } => (ticket.clone(), *seq, view.clone()),
        other => panic!("等的是一帧画面，来的是 {other:?}"),
    }
}

fn reply_ok(f: &Frame) -> bool {
    matches!(f, Frame::Reply { ok: true, .. })
}

fn reply_code(f: &Frame) -> Option<String> {
    match f {
        Frame::Reply {
            ok: false, code, ..
        } => code.clone(),
        _ => None,
    }
}

fn desk(iso: &Iso) -> (Desk, tokio::sync::mpsc::Receiver<Frame>) {
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    (Desk::on_socket(tx, iso.socket()), rx)
}

#[test]
fn tmux_version_gate() {
    for (v, ok) in [
        ("3.6", true),
        ("3.2", true),
        ("3.2a", true),
        ("next-3.4", true),
        ("4.0", true),
        ("3.1c", false),
        ("2.9", false),
        ("", false),
        ("master", false),
    ] {
        assert_eq!(tmux_version_ok(v), ok, "{v:?}");
    }
}

#[test]
fn first_frame_comes_at_once_with_the_preview_product() {
    let iso = Iso::new("first");
    iso.session("f-cc");
    iso.type_in("f-cc", "zq-first");
    let h = iso.handle_of("f-cc");
    let (d, mut rx) = desk(&iso);
    assert!(reply_ok(&d.answer_wire(
        FOLLOW,
        "r1",
        &json!({ "terminal": h, "ticket": "t1" }),
        &Default::default()
    )));
    let f = next_frame(&mut rx, Duration::from_secs(5)).expect("订上之后没推第一帧");
    let (ticket, seq, view) = screen_of(&f);
    assert_eq!((ticket.as_str(), seq), ("t1", 1));
    for k in [
        "screen",
        "cols",
        "rows",
        "cursor",
        "lines",
        "captured_at",
        "captured_at_text",
    ] {
        assert!(
            view.get(k).is_some(),
            "画面那一份少了 `{k}`（要与 terminal-preview 同一份成品）：{view}"
        );
    }
    assert!(view["lines"].to_string().contains("zq-first"));
    assert!(
        view["lines"][0].get("spans").is_some(),
        "实时画面要带颜色段"
    );
    // 线上形状：kind ＋ 票 ＋ 序号 ＋ 画面。
    let wire: Value = serde_json::from_str(&crate::stream::wire::to_line(&f).unwrap()).unwrap();
    assert_eq!(wire["kind"], "terminal_screen");
    assert_eq!(wire["ticket"], "t1");
    assert_eq!(wire["seq"], 1);
}

#[test]
fn one_frame_in_flight_until_acked() {
    let iso = Iso::new("ack");
    iso.session("a-cc");
    let h = iso.handle_of("a-cc");
    let (d, mut rx) = desk(&iso);
    assert!(reply_ok(&d.answer_wire(
        FOLLOW,
        "r1",
        &json!({ "terminal": h, "ticket": "t1" }),
        &Default::default()
    )));
    let (_, s1, _) = screen_of(&next_frame(&mut rx, Duration::from_secs(5)).expect("第一帧"));
    // 没回执：画面变了也不推。
    iso.type_in("a-cc", "zq-one");
    iso.type_in("a-cc", "zq-two");
    assert!(
        next_frame(&mut rx, Duration::from_millis(600)).is_none(),
        "没回执就推了下一帧"
    );
    // 回执 ⇒ 推最新那一屏（两次变化合成一帧）。
    assert!(reply_ok(&d.answer_wire(
        FOLLOW_ACK,
        "r2",
        &json!({ "ticket": "t1", "seq": s1 }),
        &Default::default()
    )));
    let (_, s2, v2) =
        screen_of(&next_frame(&mut rx, Duration::from_secs(5)).expect("回执之后没推"));
    assert_eq!(s2, s1 + 1);
    assert!(
        v2["lines"].to_string().contains("zq-two"),
        "推的不是最新那一屏：{v2}"
    );
    // 回执之后画面没再变 ⇒ 不推。
    assert!(reply_ok(&d.answer_wire(
        FOLLOW_ACK,
        "r3",
        &json!({ "ticket": "t1", "seq": s2 }),
        &Default::default()
    )));
    assert!(
        next_frame(&mut rx, Duration::from_millis(600)).is_none(),
        "画面没变也推了"
    );
    // 再变 ⇒ 推。
    iso.type_in("a-cc", "zq-three");
    let (_, s3, v3) =
        screen_of(&next_frame(&mut rx, Duration::from_secs(5)).expect("回执之后画面再变没推"));
    assert_eq!(s3, s2 + 1);
    assert!(v3["lines"].to_string().contains("zq-three"));
}

#[test]
fn the_follower_is_not_counted_as_a_terminal_window() {
    let iso = Iso::new("count");
    iso.session("c-cc");
    iso.tmux(&["set-option", "-t", "=c-cc:", "@ccm_sid", "sid-c"]);
    let h = iso.handle_of("c-cc");
    let ls = || {
        String::from_utf8_lossy(
            &iso.tmux(&["ls", "-F", crate::observe::tmux_observe::TMUX_LS_FMT])
                .stdout,
        )
        .to_string()
    };
    let before = ls();
    let (d, mut rx) = desk(&iso);
    assert!(reply_ok(&d.answer_wire(
        FOLLOW,
        "r1",
        &json!({ "terminal": h, "ticket": "t1" }),
        &Default::default()
    )));
    next_frame(&mut rx, Duration::from_secs(5)).expect("第一帧");
    assert!(
        eventually(|| iso.control_clients() == 1),
        "订阅那个控制模式客户端没连上（正控）"
    );
    let on = crate::control::terminals::On {
        socket: iso.sock.to_str(),
    };
    let l = crate::control::terminals::list_on(on, &json!({})).unwrap();
    let row = l["terminals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["terminal"] == h.as_str())
        .unwrap()
        .clone();
    assert_eq!(
        row["clients"],
        json!([]),
        "订阅那个客户端被算成了终端窗口：{row}"
    );
    // ↗ 问「谁连着它」那一条也不认它（它没有终端窗口可切）。
    let who = crate::observe::session_terminals::list_clients(iso.sock.to_str().unwrap(), "=c-cc:")
        .expect("list-clients");
    assert_eq!(who, vec![], "↗ 那一条把订阅客户端当成了终端窗口");
    // tmux 快照（喂会话账本 · 会话快照）：「有人连着」那一列会因订阅客户端变成 1，读出来的东西必须一样（那一列今天没有读者）。
    let during = ls();
    assert_ne!(
        before, during,
        "订阅客户端连上之后 `session_attached` 那一列没变 —— 正控没成立，下面那条在空转"
    );
    assert_eq!(
        crate::observe::tmux_observe::session_rows(&before),
        crate::observe::tmux_observe::session_rows(&during),
        "订阅客户端改了会话快照读出来的行"
    );
    assert_eq!(
        crate::observe::tmux_observe::ledger_view(&before),
        crate::observe::tmux_observe::ledger_view(&during),
        "订阅客户端改了会话账本读的那一份"
    );
    // 本仓自己装的 tmux 钩子里没有客户端那一族（订阅客户端接上 / 断开会触发 client-attached / client-detached）。
    assert!(
        !crate::control::tmux_hook::HOOK_EVENTS
            .iter()
            .any(|e| e.starts_with("client-")),
        "本仓装了客户端那一族钩子：订阅客户端每次接上 / 断开都会触发它，先想清楚再装"
    );
}

#[test]
fn unfollow_and_drop_reap_the_client() {
    let iso = Iso::new("reap");
    iso.session("r-cc");
    let h = iso.handle_of("r-cc");
    let (d, mut rx) = desk(&iso);
    assert!(reply_ok(&d.answer_wire(
        FOLLOW,
        "r1",
        &json!({ "terminal": h, "ticket": "t1" }),
        &Default::default()
    )));
    next_frame(&mut rx, Duration::from_secs(5)).expect("第一帧");
    assert!(eventually(|| iso.control_clients() == 1));
    assert!(reply_ok(&d.answer_wire(
        UNFOLLOW,
        "r2",
        &json!({ "ticket": "t1" }),
        &Default::default()
    )));
    assert!(
        eventually(|| iso.control_clients() == 0),
        "退订之后那个客户端还连着"
    );
    iso.type_in("r-cc", "zq-after");
    assert!(
        next_frame(&mut rx, Duration::from_millis(500)).is_none(),
        "退订之后还在推"
    );
    assert!(
        reply_ok(&d.answer_wire(
            UNFOLLOW,
            "r3",
            &json!({ "ticket": "t1" }),
            &Default::default()
        )),
        "退订是幂等的"
    );
    // 连接走了（票表随连接丢）⇒ 全收。
    assert!(reply_ok(&d.answer_wire(
        FOLLOW,
        "r4",
        &json!({ "terminal": h, "ticket": "t2" }),
        &Default::default()
    )));
    assert!(reply_ok(&d.answer_wire(
        FOLLOW,
        "r5",
        &json!({ "terminal": h, "ticket": "t3" }),
        &Default::default()
    )));
    assert!(eventually(|| iso.control_clients() == 2));
    drop(d);
    assert!(
        eventually(|| iso.control_clients() == 0),
        "票表丢了，客户端没收"
    );
}

#[test]
fn a_closed_pane_ends_the_follow() {
    let iso = Iso::new("gone");
    iso.session("g-cc");
    iso.session("keep-cc"); // server 别跟着走
    let h = iso.handle_of("g-cc");
    let (d, mut rx) = desk(&iso);
    assert!(reply_ok(&d.answer_wire(
        FOLLOW,
        "r1",
        &json!({ "terminal": h, "ticket": "t1" }),
        &Default::default()
    )));
    let (_, s1, _) = screen_of(&next_frame(&mut rx, Duration::from_secs(5)).expect("第一帧"));
    assert!(reply_ok(&d.answer_wire(
        FOLLOW_ACK,
        "r2",
        &json!({ "ticket": "t1", "seq": s1 }),
        &Default::default()
    )));
    iso.tmux(&["kill-session", "-t", "=g-cc"]);
    let f = next_frame(&mut rx, Duration::from_secs(5)).expect("窗格没了却没说");
    match f {
        Frame::TerminalFollowEnd { ticket, why, said } => {
            assert_eq!(ticket, "t1");
            assert_eq!(why, FollowEnd::Gone);
            assert_eq!(said, copy_text("beTermFollow.end.gone", &[]));
        }
        other => panic!("等的是结束帧，来的是 {other:?}"),
    }
    assert!(
        reply_code(&d.answer_wire(
            FOLLOW_ACK,
            "r3",
            &json!({ "ticket": "t1", "seq": s1 }),
            &Default::default()
        ))
        .is_some(),
        "结束了的票还认"
    );
}

#[test]
fn shape_and_limits() {
    let iso = Iso::new("shape");
    iso.session("s-cc");
    let h = iso.handle_of("s-cc");
    let (d, mut rx) = desk(&iso);
    assert_eq!(
        reply_code(&d.answer_wire(FOLLOW, "r0", &json!({ "terminal": h }), &Default::default()))
            .as_deref(),
        Some("bad_args"),
        "没给票"
    );
    assert_eq!(
        reply_code(&d.answer_wire(FOLLOW, "r0", &json!({ "ticket": "x" }), &Default::default()))
            .as_deref(),
        Some("bad_target"),
        "没给目标"
    );
    assert_eq!(
        reply_code(&d.answer_wire(
            FOLLOW,
            "r0",
            &json!({ "terminal": "tmux-999", "ticket": "x" }),
            &Default::default()
        ))
        .as_deref(),
        Some("not_known")
    );
    assert_eq!(
        reply_code(&d.answer_wire(
            FOLLOW_ACK,
            "r0",
            &json!({ "ticket": "nope", "seq": 1 }),
            &Default::default()
        ))
        .as_deref(),
        Some("not_known")
    );
    assert!(reply_ok(&d.answer_wire(
        FOLLOW,
        "r1",
        &json!({ "terminal": h, "ticket": "t1" }),
        &Default::default()
    )));
    assert_eq!(
        reply_code(&d.answer_wire(
            FOLLOW,
            "r2",
            &json!({ "terminal": h, "ticket": "t1" }),
            &Default::default()
        ))
        .as_deref(),
        Some("bad_args"),
        "票重复"
    );
    for i in 2..=MAX_FOLLOWS_PER_CONNECTION {
        assert!(reply_ok(&d.answer_wire(
            FOLLOW,
            "r",
            &json!({ "terminal": h, "ticket": format!("t{i}") }),
            &Default::default()
        )));
    }
    assert_eq!(
        reply_code(&d.answer_wire(
            FOLLOW,
            "r9",
            &json!({ "terminal": h, "ticket": "over" }),
            &Default::default()
        ))
        .as_deref(),
        Some("too_many_follows")
    );
    while next_frame(&mut rx, Duration::from_millis(50)).is_some() {}
}

/// 订得上的那几张票（排干推来的帧）。
fn fill(d: &Desk, h: &str, rx: &mut tokio::sync::mpsc::Receiver<Frame>, prefix: &str) {
    for i in 0..MAX_FOLLOWS_PER_CONNECTION {
        let r = d.answer_wire(
            FOLLOW,
            "f",
            &json!({ "terminal": h, "ticket": format!("{prefix}{i}") }),
            &Default::default(),
        );
        assert!(reply_ok(&r), "第 {i} 张订不上：{r:?}");
    }
    while next_frame(rx, Duration::from_millis(50)).is_some() {}
}

#[test]
fn an_unfollow_that_comes_first_is_final() {
    let iso = Iso::new("first-unf");
    iso.session("u-cc");
    let h = iso.handle_of("u-cc");
    let (d, mut rx) = desk(&iso);
    // 壳替界面退订那一问可能先于订阅那一问到（两问各走各的）。
    assert!(reply_ok(&d.answer_wire(
        UNFOLLOW,
        "r1",
        &json!({ "ticket": "t1" }),
        &Default::default()
    )));
    assert!(
        reply_ok(&d.answer_wire(
            FOLLOW,
            "r2",
            &json!({ "terminal": h, "ticket": "t1" }),
            &Default::default()
        )),
        "退过的票再来订：照实回 ok（看的那一方早已不在）"
    );
    assert!(
        next_frame(&mut rx, Duration::from_millis(500)).is_none(),
        "退订先到，却还是订上了、推了帧"
    );
    assert_eq!(iso.control_clients(), 0, "退订先到，却还是起了客户端");
    // 不占票：照样订得满。
    fill(&d, &h, &mut rx, "n");
}

#[test]
fn an_unfollow_while_starting_reaps_what_was_started() {
    let iso = Iso::new("mid-unf");
    iso.session("m-cc");
    let h = iso.handle_of("m-cc");
    let (d, mut rx) = desk(&iso);
    let args = json!({ "terminal": h, "ticket": "t1" });
    let seat = d.reserve(&args).expect("占位").expect("没退过的票该占得上");
    // 起 tmux 那几下当中退订到了（就地做完、不等）。
    assert!(reply_ok(&d.answer_wire(
        UNFOLLOW,
        "r1",
        &json!({ "ticket": "t1" }),
        &Default::default()
    )));
    assert!(
        d.take_seat(&seat, &args, &Default::default()).is_ok(),
        "起好了才发现退过：回 ok"
    );
    assert!(
        next_frame(&mut rx, Duration::from_millis(500)).is_none(),
        "退过的票还推了帧"
    );
    assert!(
        eventually(|| iso.control_clients() == 0),
        "退过的票起好的客户端没收"
    );
    fill(&d, &h, &mut rx, "n");
}

#[test]
fn a_seat_being_started_counts() {
    let iso = Iso::new("seat");
    iso.session("p-cc");
    let h = iso.handle_of("p-cc");
    let (d, _rx) = desk(&iso);
    let seats: Vec<_> = (0..MAX_FOLLOWS_PER_CONNECTION)
        .map(|i| {
            d.reserve(&json!({ "terminal": h, "ticket": format!("s{i}") }))
                .expect("占位")
                .expect("没退过")
        })
        .collect();
    assert_eq!(
        reply_code(&d.answer_wire(
            FOLLOW,
            "r1",
            &json!({ "terminal": h, "ticket": "over" }),
            &Default::default()
        ))
        .as_deref(),
        Some("too_many_follows"),
        "正在起的也占名额"
    );
    assert_eq!(
        d.reserve(&json!({ "terminal": h, "ticket": "s0" }))
            .err()
            .map(|e| e.code),
        Some("bad_args".to_string()),
        "正在起的票再订一次 ⇒ 重复"
    );
    // 起不成的那一张让出名额。
    let bad = json!({ "terminal": "tmux-999", "ticket": "s0" });
    assert!(d.reserve(&bad).is_err());
    assert_eq!(
        d.follow(&bad, &Default::default()).err().map(|e| e.code),
        Some("bad_args".to_string()),
        "s0 还占着（重复）"
    );
    d.release(&seats[0]);
    assert_eq!(
        d.follow(
            &json!({ "terminal": "tmux-999", "ticket": "x" }),
            &Default::default()
        )
        .err()
        .map(|e| e.code),
        Some("not_known".to_string()),
        "让出名额之后、目标不在名单 ⇒ not_known"
    );
    assert!(
        d.reserve(&json!({ "terminal": h, "ticket": "y" }))
            .expect("占位")
            .is_some(),
        "起不成的那一张没让出名额"
    );
}

#[test]
fn rapid_follow_unfollow_never_fills_the_desk() {
    let iso = Iso::new("rapid");
    iso.session("q-cc");
    let h = iso.handle_of("q-cc");
    let (d, mut rx) = desk(&iso);
    let server = || iso.tmux(&["display-message", "-p", "#{pid}"]).stdout;
    let server_before = server();
    // 连按 Ctrl+Tab · 重载：一轮订一轮退，有时退订先到。
    let ask = |cmd: &str, args: Value| {
        let r = d.answer_wire(cmd, "q", &args, &Default::default());
        assert!(
            reply_ok(&r),
            "{cmd} 那一问没回 ok：{r:?}（那台 tmux 的 server 还是原来那个：{}）",
            server() == server_before
        );
    };
    for i in 0..(3 * MAX_FOLLOWS_PER_CONNECTION) {
        let t = format!("r{i}");
        if i % 2 == 0 {
            ask(FOLLOW, json!({ "terminal": h, "ticket": t }));
            ask(UNFOLLOW, json!({ "ticket": t }));
        } else {
            ask(UNFOLLOW, json!({ "ticket": t }));
            ask(FOLLOW, json!({ "terminal": h, "ticket": t }));
        }
    }
    assert!(
        eventually(|| iso.control_clients() == 0),
        "订 / 退几十轮之后还挂着客户端"
    );
    // 订阅那一方自己收客户端时不许把那台 tmux 弄没（直接杀控制模式客户端 ⇒ tmux 3.6a 的 server 段错误，那台上所有会话一起没）。
    assert_eq!(
        server(),
        server_before,
        "那台 tmux 的 server 中途没了（内核日志里找 `tmux: server … segfault`）"
    );
    while next_frame(&mut rx, Duration::from_millis(50)).is_some() {}
    fill(&d, &h, &mut rx, "n");
}

#[test]
fn refusals_say_which_way_live_went() {
    for (code, live) in [
        ("tmux_too_old", "snapshot_only"),
        ("no_tmux", "snapshot_only"),
        ("not_known", "stopped"),
        ("ambiguous", "stopped"),
        ("too_many_follows", "stopped"),
        ("bad_args", "stopped"),
        ("unobservable", "stopped"),
    ] {
        assert_eq!(
            serde_json::to_value(live_after_refusal(code)).unwrap(),
            serde_json::json!({ "live": live }),
            "{code}"
        );
    }
    // 三种停因各有一句，彼此不同、都不空。
    let said: Vec<String> = [FollowEnd::Gone, FollowEnd::Lost, FollowEnd::TooBig]
        .into_iter()
        .map(end_said)
        .collect();
    assert!(said.iter().all(|s| !s.trim().is_empty()));
    assert_ne!(said[0], said[1]);
    assert_ne!(said[1], said[2]);
}

impl crate::guard_support::Shaped for Refused {
    fn samples() -> Vec<Self> {
        vec![
            Refused {
                live: Live::SnapshotOnly,
            },
            Refused {
                live: Live::Stopped,
            },
        ]
    }
}
