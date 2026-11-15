//! 读屏驱动的 `settle`：屏还不是认得的样子 ⇒ 等一次重画再读，至多那几次；认到就停，次数用完 · 不再重画 ⇒ 回最后读到的那一屏。

use super::*;

/// 假屏：按一下之后要过 `lag` 次重画才换成下一样；`still` 为真 ⇒ 再也不重画。
struct Screen {
    shown: u32,
    pending: Option<(u32, usize)>,
    lag: usize,
    still: bool,
    waits: usize,
}

impl Screen {
    fn new(lag: usize) -> Screen {
        Screen {
            shown: 0,
            pending: None,
            lag,
            still: false,
            waits: 0,
        }
    }
    fn press(&mut self) {
        self.pending = Some((self.shown + 1, self.lag));
    }
    /// 等一次重画：变了 ⇒ 读到的那一样。
    fn next(&mut self) -> Result<Option<u32>, Said> {
        self.waits += 1;
        if self.still {
            return Ok(None);
        }
        if let Some((to, left)) = self.pending {
            if left <= 1 {
                self.shown = to;
                self.pending = None;
            } else {
                self.pending = Some((to, left - 1));
            }
        }
        Ok(Some(self.shown))
    }
}

fn run(s: &mut Screen, redraws: usize) -> u32 {
    let now = s.shown;
    settle(now, redraws, |&n| n != now, || s.next()).unwrap()
}

/// ★ 按一下、过三次重画才换样子：次数够 ⇒ 认到、只等到认到为止。
#[test]
fn it_waits_until_the_screen_reads_as_wanted() {
    let mut s = Screen::new(3);
    s.press();
    assert_eq!(run(&mut s, REDRAWS_PER_PRESS), 1);
    assert_eq!(s.waits, 3, "认到了还在等");
}

/// ★ 次数用完还没换样子 ⇒ 回最后读到的那一屏（不多等一次）。
#[test]
fn it_gives_up_after_the_redraw_budget() {
    let mut s = Screen::new(REDRAWS_PER_PRESS + 1);
    s.press();
    assert_eq!(run(&mut s, REDRAWS_PER_PRESS), 0);
    assert_eq!(s.waits, REDRAWS_PER_PRESS);
}

/// ★ 不再重画 ⇒ 当场停，回此刻那一屏。
#[test]
fn it_stops_when_the_screen_no_longer_redraws() {
    let mut s = Screen::new(1);
    s.press();
    s.still = true;
    assert_eq!(run(&mut s, REDRAWS_PER_PRESS), 0);
    assert_eq!(s.waits, 1);
}

/// ★ 一开始就是认得的样子 ⇒ 一次都不等。
#[test]
fn it_does_not_wait_when_already_there() {
    let mut s = Screen::new(1);
    assert_eq!(
        settle(7_u32, REDRAWS_PER_PRESS, |&n| n == 7, || s.next()).unwrap(),
        7
    );
    assert_eq!(s.waits, 0);
}

/// 托管终端那一份驱动（判据直接给托管口，不经名单认）。
fn hosted(sock: &std::path::Path) -> Drive<'static> {
    Drive {
        sid: "",
        client: None,
        watch: Some(Watch::hosted(sock).expect("连上托管口")),
        hosted: Some(sock.to_path_buf()),
    }
}

fn shows(s: &[String], what: &str) -> bool {
    s.iter().any(|l| l.trim_end() == what)
}

/// 等到屏上出现 `what`（子进程起得慢也等得到）。
fn wait_for(d: &mut Drive<'_>, what: &str) {
    let now = d.screen().expect("取屏");
    let got = settle(
        now,
        64,
        |s| shows(s, what),
        || d.redrawn().then(|| d.screen()).transpose(),
    )
    .expect("取屏");
    assert!(shows(&got, what), "一直没等到 {what}：{got:?}");
}

/// 等那一屏落定在 `line` 上（连换行都落了：光标在下一行行首、上一行正是 `line`），再开一份新的驱动交回。
/// 新的那份头一帧（`welcome`）就带着落定的那一屏，之后推来的只是此后的变化；等的那份连接上还在路上的帧（屏已算进、推送没到）跟着它一起放掉，不算进来。
/// `nudge` ⇒ 屏上先出现 `line` 之后往里送那几个字节（子进程等它才印换行）。
fn settled_on(sock: &std::path::Path, line: &str, nudge: Option<&[u8]>) -> Drive<'static> {
    use crate::host::managed::client::{self, Captured};
    let mut w = hosted(sock);
    wait_for(&mut w, line);
    if let Some(b) = nudge {
        client::say(sock, b).expect("送进去");
    }
    let landed = |c: &Captured| {
        let [row, col] = c.cursor;
        col == 0
            && row > 0
            && c.lines
                .get(usize::from(row) - 1)
                .is_some_and(|l| l.trim_end() == line)
    };
    let shot = || client::capture(sock, 0, false).expect("取屏");
    let got = settle(shot(), 64, |c| landed(c), || Ok(w.redrawn().then(shot))).expect("取屏");
    assert!(
        landed(&got),
        "那一屏一直没落定在 {line} 上：{:?} 光标 {:?}",
        got.lines,
        got.cursor
    );
    drop(w);
    hosted(sock)
}

/// ★ 托管终端：送一行之后它先后推两帧、第二帧才是要的样子 ⇒ 每推一帧读一次，认到第二帧就回；
/// 看屏不起 tmux（托管口的推送当重画）。真起托管进程（假子进程 `sh`，沙箱数据目录）。
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hosted_terminal_settles_on_the_frame_it_pushes() {
    use crate::host::managed::run::test_support::{sandbox_own, spec, start};
    let (own, _data) = sandbox_own("drive-two");
    let (sock, _task) = start(
        &own,
        spec(
            "t-drv2",
            "printf 'ready\\r\\n'; read x; printf 'one\\r\\n'; sleep 0.3; printf 'two\\r\\n'; read y",
        ),
    )
    .await;
    tokio::task::spawn_blocking(move || {
        let mut d = settled_on(&sock, "ready", None);
        d.watch.as_mut().expect("看着").forget();
        crate::host::managed::client::say(&sock, b"x\r").expect("送进去");
        let now = d.screen().expect("取屏");
        let got = settle(
            now,
            REDRAWS_AFTER_SEND,
            |s| shows(s, "two"),
            || d.redrawn().then(|| d.screen()).transpose(),
        )
        .expect("取屏");
        assert!(
            shows(&got, "one") && shows(&got, "two"),
            "没认到第二帧：{got:?}"
        );
    })
    .await
    .expect("没崩");
}

/// 起一个托管终端（假子进程跑 `script`），等那一屏落定在 `ready` 上（[`settled_on`]，`nudge` 照它）；
/// 之后忘掉此刻之前的变化、照「按一下之后」的次数等一个不会出现的样子：断言回的是原来那一屏，交回等了几次。
#[cfg(unix)]
async fn waits_on_a_still_screen(
    tag: &str,
    id: &str,
    script: &str,
    nudge: Option<&'static [u8]>,
) -> usize {
    use crate::host::managed::run::test_support::{sandbox_own, spec, start};
    let (own, _data) = sandbox_own(tag);
    let (sock, _task) = start(&own, spec(id, script)).await;
    tokio::task::spawn_blocking(move || {
        let mut d = settled_on(&sock, "ready", nudge);
        d.watch.as_mut().expect("看着").forget();
        let now = d.screen().expect("取屏");
        let mut waits = 0;
        let got = settle(
            now.clone(),
            REDRAWS_PER_PRESS,
            |s| shows(s, "never"),
            || {
                waits += 1;
                d.redrawn().then(|| d.screen()).transpose()
            },
        )
        .expect("取屏");
        assert_eq!(got, now, "没推也换了一屏");
        waits
    })
    .await
    .expect("没崩")
}

/// ★ 托管终端一字不推 ⇒ 等到期限照实回原来那一屏（不当成变了）。
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hosted_terminal_that_pushes_nothing_gives_back_the_same_screen() {
    let waits = waits_on_a_still_screen(
        "drive-still",
        "t-drv0",
        "printf 'ready\\r\\n'; read x",
        None,
    )
    .await;
    assert_eq!(waits, 1, "到期没变还在等");
}

/// ★ `ready` 先上屏、换行隔一拍才落（子进程等一个键才印 `\r\n`，不靠时间）⇒ 等那一屏落定再数，晚到的换行不算「推了」。
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_newline_that_lands_late_is_not_counted_as_a_push() {
    let waits = waits_on_a_still_screen(
        "drive-late",
        "t-drvl",
        "stty -echo; printf 'ready'; read k; printf '\\r\\n'; read x",
        Some(b"k\r"),
    )
    .await;
    assert_eq!(waits, 1, "到期没变还在等");
}

/// ★ 按键 / 送句之前先忘掉此刻之前的变化：送之前那一屏推来的那一下不算送之后的一次重画。
/// 托管终端先推一帧 `one`，等它落进驱动的看屏连接（读线落下的帧数涨了，不睡、不看墙钟），驱动不去读它，接着按一下
/// ⇒ 之后等重画到期还是「没变」。驱动的 sid 是空串：送那一下在认目标那一步就拒（`bad_target`，不碰 tmux），忘掉那一步在它之前。
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_press_forgets_the_change_from_before_it() {
    use crate::host::managed::run::test_support::{sandbox_own, spec, start};
    let (own, _data) = sandbox_own("drive-fgt");
    let (sock, _task) = start(
        &own,
        spec(
            "t-drvf",
            "stty -echo; printf 'ready\\r\\n'; read x; printf 'one\\r\\n'; read y",
        ),
    )
    .await;
    tokio::task::spawn_blocking(move || {
        let mut d = settled_on(&sock, "ready", None);
        let w = d.watch.as_ref().expect("看着");
        let base = w.landed();
        crate::host::managed::client::say(&sock, b"x\r").expect("送进去");
        assert!(w.wait_landed(base + 1), "送之前那一屏一直没推过来");
        let e = d.press("enter").unwrap_err();
        assert_eq!(e.code.as_deref(), Some("bad_target"));
        assert!(!d.redrawn(), "送之前那一屏推来的那一下算成了送之后的重画");
    })
    .await
    .expect("没崩");
}

/// ★ 送出去的那一下带的是构造时那条会话与那个看屏客户端：sid 空 · 长得远超句柄上限 ⇒ 认目标就拒（换成别的 sid 就过了这一步）；
/// sid 在、客户端名不合规 ⇒ 身份门拒、原话里是构造时给的那个名字。每一下都带不合规的客户端名：
/// 驱动送错了 sid 也停在身份门、碰不到 tmux。
#[test]
fn the_input_carries_the_sid_and_client_it_was_built_with() {
    let e = Drive::new("", Some("Bad_Client".into()))
        .say("hi")
        .unwrap_err();
    assert_eq!(e.code.as_deref(), Some("bad_target"), "空 sid 没拒：{e:?}");
    let long = "s".repeat(4096);
    let e = Drive::new(&long, Some("Bad_Client".into()))
        .press("enter")
        .unwrap_err();
    assert_eq!(
        e.code.as_deref(),
        Some("bad_target"),
        "送出去的不是构造时那个 sid：{e:?}"
    );
    let e = Drive::new("s-drv", Some("Bad_Client".into()))
        .press("enter")
        .unwrap_err();
    assert_eq!(e.code.as_deref(), Some("bad_args"));
    assert!(
        format!("{e:?}").contains("Bad_Client"),
        "身份门看到的不是构造时那个客户端：{e:?}"
    );
    let e = Drive::new("s-drv", Some("Bad_Client".into()))
        .say("hi")
        .unwrap_err();
    assert_eq!(e.code.as_deref(), Some("bad_args"));
}

/// ★ 先忘再送：送那一下带来的变化留给之后等（忘掉的只是送之前的）。送法换成替身（不碰 tmux、不经托管名单）：
/// 它往托管终端里送一个键、等那一帧落进驱动的看屏连接才回，记下送时读线落下了几帧。
/// 托管终端送之前先推一帧 `one`（落定了才送），替身送出去之后再推一帧 `two`
/// ⇒ 送时只落下送之前那一帧 · 送完之后等得到那一下重画（忘在送之后 ⇒ 送那一下推来的也被忘掉、等到期还是「没变」）。
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_change_a_send_brings_is_kept_for_the_wait_after_it() {
    use crate::host::managed::run::test_support::{sandbox_own, spec, start};
    let (own, _data) = sandbox_own("drive-ord");
    let (sock, _task) = start(
        &own,
        spec(
            "t-drvo",
            "stty -echo; printf 'ready\\r\\n'; read x; printf 'one\\r\\n'; read y; printf 'two\\r\\n'; read z",
        ),
    )
    .await;
    tokio::task::spawn_blocking(move || {
        let mut d = settled_on(&sock, "ready", None);
        let w = d.watch.as_ref().expect("看着");
        let base = w.landed();
        crate::host::managed::client::say(&sock, b"x\r").expect("送进去");
        assert!(w.wait_landed(base + 1), "送之前那一屏一直没推过来");
        let mut seen = Vec::new();
        let req = TerminalInputReq::default();
        d.forget_then_send(&req, |_, w| {
            let w = w.expect("看着");
            seen.push(w.landed() - base);
            crate::host::managed::client::say(&sock, b"y\r").expect("送进去");
            assert!(w.wait_landed(base + 2), "送那一下推来的那一屏一直没落下");
        });
        assert_eq!(seen, [1], "送的那一下没走替身，或者送了不止一次");
        assert!(d.redrawn(), "送那一下推来的那一屏被当成送之前的变化忘掉了");
    })
    .await
    .expect("没崩");
}

// ── 认准框才按、按完复核（`answer_box`）：替身屏幕照剧本一屏一屏给，记下按了哪几下 ──────────────────
// 一屏一屏是真屏夹具（夹具 claude-ask-box-*，替身在私有 tmux 里画、截下；怎么重截见本文件末尾那一条），
// 认框用 Claude Code 那一家的认法（生产 `press_box` 交的正是它）。

/// 一份夹具那一屏（照文件名去掉 `.screen.txt` 那一截点；每一份照字面 `include_str!`，夹具守卫认得出谁在用）。
macro_rules! screen {
    ("wide-160") => {
        shot(include_str!(
            "../../__fixtures__/claude-ask-box-wide-160.screen.txt"
        ))
    };
    ("main-160") => {
        shot(include_str!(
            "../../__fixtures__/claude-ask-box-main-160.screen.txt"
        ))
    };
    ("other-tool-160") => {
        shot(include_str!(
            "../../__fixtures__/claude-ask-box-other-tool-160.screen.txt"
        ))
    };
    ("queue-first") => {
        shot(include_str!(
            "../../__fixtures__/claude-ask-box-queue-first.screen.txt"
        ))
    };
    ("queue-next") => {
        shot(include_str!(
            "../../__fixtures__/claude-ask-box-queue-next.screen.txt"
        ))
    };
    ("queue-done") => {
        shot(include_str!(
            "../../__fixtures__/claude-ask-box-queue-done.screen.txt"
        ))
    };
    ("wrap-60") => {
        shot(include_str!(
            "../../__fixtures__/claude-ask-box-wrap-60.screen.txt"
        ))
    };
    ("split-40") => {
        shot(include_str!(
            "../../__fixtures__/claude-ask-box-split-40.screen.txt"
        ))
    };
    ("short-80x11") => {
        shot(include_str!(
            "../../__fixtures__/claude-ask-box-short-80x11.screen.txt"
        ))
    };
}

/// 这一条的框：后台子 agent（`general-purpose`）要批 `touch ./plum.txt`。
fn mine_box() -> Vec<String> {
    screen!("wide-160")
}
/// 排着的另一条（`touch ./pear.txt`，1 of 2）。
fn other_box() -> Vec<String> {
    screen!("queue-first")
}
/// 答完、框没了（输入框回来了）。
fn no_box() -> Vec<String> {
    screen!("queue-done")
}
/// 重画到一半的那一帧：这一条的框只画到问句、选项与底行还没出来。
fn half_box() -> Vec<String> {
    let mut s = mine_box();
    let ask = s
        .iter()
        .position(|l| l.trim() == "Do you want to proceed?")
        .expect("夹具里有问句");
    s.truncate(ask + 1);
    s
}

/// 替身：`screens` 是此刻那一屏起往后每次重画换成的样子；按一下之后换成 `after` 那几屏（`None` ＝ 按了也不变）。
struct Script {
    refused: Option<Refused>,
    now: Vec<String>,
    ahead: std::collections::VecDeque<Vec<String>>,
    after: Option<Vec<Vec<String>>>,
    taps: Vec<crate::agents::Keystroke>,
    sent: crate::control::terminals_wire::Sent,
    /// 按下那一下报错（不知道送没送到）。
    tap_fails: bool,
    /// 按下之后读屏报错。
    read_fails_after_tap: bool,
    /// 接下来几次读屏插进来的那几屏（抓到重画到一半的那一帧）；读完了照 `now`。
    torn: std::collections::VecDeque<Vec<String>>,
}

fn shot(s: &str) -> Vec<String> {
    s.lines().map(str::to_string).collect()
}

fn delivered() -> crate::control::terminals_wire::Sent {
    crate::control::terminals::input_reply(cmd_wire::session::SentResult::Delivered, None, None)
}

impl Script {
    fn new(screens: Vec<Vec<String>>, after: Option<Vec<Vec<String>>>) -> Script {
        let mut ahead: std::collections::VecDeque<Vec<String>> = screens.into();
        let now = ahead.pop_front().unwrap_or_default();
        Script {
            refused: None,
            now,
            ahead,
            after,
            taps: Vec::new(),
            sent: delivered(),
            tap_fails: false,
            read_fails_after_tap: false,
            torn: std::collections::VecDeque::new(),
        }
    }
}

impl BoxIo for Script {
    fn open(&mut self) -> Result<Option<Refused>, Said> {
        Ok(self.refused)
    }
    fn screen(&mut self) -> Result<Vec<String>, Said> {
        if self.read_fails_after_tap && !self.taps.is_empty() {
            return Err(Said::new("no_such_session", "（夹具）".to_string()));
        }
        Ok(self.torn.pop_front().unwrap_or_else(|| self.now.clone()))
    }
    fn redrawn(&mut self) -> bool {
        match self.ahead.pop_front() {
            Some(s) => {
                self.now = s;
                true
            }
            None => false,
        }
    }
    fn tap(
        &mut self,
        k: crate::agents::Keystroke,
    ) -> Result<crate::control::terminals_wire::Sent, Said> {
        self.taps.push(k);
        if self.tap_fails {
            return Err(Said::new("no_such_session", "（夹具）".to_string()));
        }
        if let Some(after) = self.after.take() {
            self.ahead = after.into();
        }
        Ok(self.sent.clone())
    }
}

/// ★ 从按下那一下起出的错（按键报错 · 按后读屏报错）：键可能已经进了终端 ⇒ 不确定，不往上抛成别的码（出口会当没这一条、再按一次）。
#[test]
fn an_error_from_the_press_on_is_unsure() {
    let mut tap = Script::new(vec![mine_box()], Some(vec![no_box()]));
    tap.tap_fails = true;
    assert!(matches!(run_box(&mut tap), OnBox::Stays));
    let mut read = Script::new(vec![mine_box()], Some(vec![no_box()]));
    read.read_fails_after_tap = true;
    assert!(matches!(run_box(&mut read), OnBox::Stays));
}

/// ★ 按完那一屏认不出、紧接着再读一屏框又在（抓到重画到一半的那一帧）⇒ 不算框没了。
#[test]
fn one_torn_frame_after_the_press_is_not_gone() {
    let mut s = Script::new(vec![mine_box()], Some(vec![half_box()]));
    s.torn = std::collections::VecDeque::from([mine_box(), mine_box(), half_box(), mine_box()]);
    assert!(matches!(run_box(&mut s), OnBox::Stays));
}

const ALLOW: crate::agents::Keystroke = crate::agents::Keystroke::Char("1");

/// 认框：Claude Code 那一家登在注册表里的认法（同出口经 `ask_face` 拿到的那一格），认的是 `touch ./plum.txt` 那一条（后台子 agent `general-purpose` 发的）。
fn mine(l: &[String]) -> bool {
    let face = crate::agents::ask_face(crate::agents::claudecode::AGENT_KIND).expect("登着答题口");
    let screen = face.on_screen.expect("登着认框");
    let plum = (face.read)(&serde_json::json!({
        "session_id": "s-1",
        "hook_event_name": "PermissionRequest",
        "tool_name": "Bash",
        "tool_input": {"command": "touch ./plum.txt", "description": "make plum"},
        "agent_id": "a-9",
        "agent_type": "general-purpose"
    }))
    .expect("读得出");
    (screen.seen)(l, &plum)
}

fn run_box(s: &mut Script) -> OnBox {
    answer_box(s, &mine, &|_| false, ALLOW).unwrap()
}

/// ★ 夹具认得准：这一条的框认出，排着的另一条 · 没有框 · 画到一半的都不认（下面几条照它们排剧本）。
#[test]
fn the_fixture_screens_read_as_meant() {
    assert!(mine(&mine_box()));
    assert!(!mine(&other_box()));
    assert!(!mine(&no_box()));
    assert!(!mine(&half_box()));
}

/// ★ 认准才按：屏上正是这一条的框 ⇒ 按一下；按完那一屏框没了 ⇒ 已答。
#[test]
fn a_seen_box_is_pressed_once_and_checked_gone() {
    let mut s = Script::new(vec![mine_box()], Some(vec![no_box()]));
    assert!(matches!(run_box(&mut s), OnBox::Gone));
    assert_eq!(s.taps, [ALLOW]);
}

/// ★ 框还没画出来、过一次重画才是它 ⇒ 等到再按。
#[test]
fn it_waits_a_redraw_for_the_box_before_pressing() {
    let mut s = Script::new(vec![half_box(), mine_box()], Some(vec![no_box()]));
    assert!(matches!(run_box(&mut s), OnBox::Gone));
    assert_eq!(s.taps, [ALLOW]);
}

/// ★ 认不出不按：几次重画都不是这一条的框（别的框 · 没有框）⇒ 没认出，一个键都不按。
#[test]
fn an_unseen_box_presses_nothing() {
    let mut s = Script::new(
        vec![other_box(), other_box(), no_box(), no_box(), no_box()],
        None,
    );
    assert!(matches!(run_box(&mut s), OnBox::NotSeen));
    assert!(s.taps.is_empty(), "认不出也按了");
}

/// ★ 按完复核：按了、框还在（几次重画都还是它）⇒ 不确定；换成了排着的下一条 ⇒ 已答（不把下一条当这一条）。
#[test]
fn after_pressing_the_screen_is_read_again() {
    let mut stays = Script::new(vec![mine_box()], Some(vec![mine_box(), mine_box()]));
    assert!(matches!(run_box(&mut stays), OnBox::Stays));
    assert_eq!(stays.taps.len(), 1, "框还在也只按一下");
    let mut still = Script::new(vec![mine_box()], Some(vec![]));
    assert!(
        matches!(run_box(&mut still), OnBox::Stays),
        "按了屏不再变 ⇒ 判不了 ⇒ 不确定"
    );
    let mut next = Script::new(vec![mine_box()], Some(vec![other_box()]));
    assert!(matches!(run_box(&mut next), OnBox::Gone));
}

/// ★ 终端不收（没了 · 只剩 shell）⇒ 照那个原因回，一个键不按；按的那一下没送到 ⇒ 交回那一下的结局。
#[test]
fn a_closed_terminal_or_an_unsent_press_says_so() {
    let mut gone = Script::new(vec![mine_box()], Some(vec![no_box()]));
    gone.refused = Some(Refused::Ended);
    assert!(matches!(run_box(&mut gone), OnBox::Closed(_)));
    assert!(gone.taps.is_empty());
    let mut unsent = Script::new(vec![mine_box()], Some(vec![no_box()]));
    unsent.sent = crate::control::terminals::input_reply(
        cmd_wire::session::SentResult::Refused,
        Some(Refused::Ended),
        None,
    );
    assert!(
        matches!(run_box(&mut unsent), OnBox::Closed(_)),
        "被拒 ⇒ 一个键没进终端"
    );
}

/// ★ 认出了这一条的框，屏上却说分不出是哪一条（`doubt`：排着的条数比认得的多）⇒ 一个键都不按。
#[test]
fn a_box_in_doubt_is_not_pressed() {
    let mut s = Script::new(vec![mine_box()], Some(vec![no_box()]));
    assert!(matches!(
        answer_box(&mut s, &mine, &|_| true, ALLOW).unwrap(),
        OnBox::Doubt
    ));
    assert!(s.taps.is_empty());
}

/// ★ 按之前紧接着再认一次：认得的那一屏之后，按之前读到的那一屏认不出了（框刚被别处答掉）⇒ 不按。
#[test]
fn the_box_is_seen_again_right_before_the_press() {
    let mut s = Script::new(vec![mine_box()], Some(vec![no_box()]));
    s.torn = std::collections::VecDeque::from([mine_box(), no_box()]);
    assert!(matches!(run_box(&mut s), OnBox::NotSeen));
    assert!(s.taps.is_empty(), "按之前那一屏已经没框也按了");
}

// ── 真屏夹具怎么来：替身在私有 tmux 里画框，照生产抓屏那一条 argv 截下 ──────────────────
// 读的有两处：上面那几条 · Claude Code 那一家的认框判据（`agents::claudecode::asks`）。

/// 替身怎么起、截哪一屏：窗口宽高 · 排着的请求（`种类|命令|说明[|标题]`，种类空 ⇒ 主会话）·
/// 起来之后等到屏上出现的那一句 · 之后依次按的键（每按一下等到的那一句）。
struct Shot {
    /// 夹具（仓相对）。
    path: &'static str,
    size: (u16, u16),
    asks: &'static [&'static str],
    until: &'static str,
    keys: &'static [(&'static str, &'static str)],
}

const PLUM: &str = "general-purpose|touch ./plum.txt|make plum";
const PEAR: &str = "general-purpose|touch ./pear.txt|make pear";
const LONG: &str = "general-purpose|touch ./a_very_long_file_name_number_one.txt ./a_very_long_file_name_number_two.txt ./a_very_long_file_name_number_three.txt|Create three files";
const FOOT: &str = "Esc to cancel";

const SHOTS: &[Shot] = &[
    Shot {
        path: "tests/__fixtures__/claude-ask-box-wide-160.screen.txt",
        size: (160, 24),
        asks: &[PLUM],
        until: FOOT,
        keys: &[],
    },
    Shot {
        path: "tests/__fixtures__/claude-ask-box-main-160.screen.txt",
        size: (160, 24),
        asks: &["|touch ./plum.txt|make plum"],
        until: FOOT,
        keys: &[],
    },
    Shot {
        path: "tests/__fixtures__/claude-ask-box-other-tool-160.screen.txt",
        size: (160, 24),
        asks: &["general-purpose|touch ./plum.txt|make plum|Edit file"],
        until: FOOT,
        keys: &[],
    },
    Shot {
        path: "tests/__fixtures__/claude-ask-box-queue-first.screen.txt",
        size: (160, 24),
        asks: &[PEAR, PLUM],
        until: "1 of 2",
        keys: &[],
    },
    Shot {
        path: "tests/__fixtures__/claude-ask-box-queue-next.screen.txt",
        size: (160, 24),
        asks: &[PEAR, PLUM],
        until: "1 of 2",
        keys: &[("1", "2 of 2")],
    },
    Shot {
        path: "tests/__fixtures__/claude-ask-box-queue-done.screen.txt",
        size: (160, 24),
        asks: &[PEAR, PLUM],
        until: "1 of 2",
        keys: &[("1", "2 of 2"), ("Escape", "? for shortcuts")],
    },
    Shot {
        path: "tests/__fixtures__/claude-ask-box-wrap-60.screen.txt",
        size: (60, 30),
        asks: &[LONG, PEAR],
        until: "1 of 2",
        keys: &[],
    },
    Shot {
        path: "tests/__fixtures__/claude-ask-box-split-40.screen.txt",
        size: (40, 40),
        asks: &[LONG],
        until: FOOT,
        keys: &[],
    },
    Shot {
        path: "tests/__fixtures__/claude-ask-box-short-80x11.screen.txt",
        size: (80, 11),
        asks: &[LONG],
        until: FOOT,
        keys: &[],
    },
];

/// ★ 夹具就是替身此刻画出来的那一屏：每一形在判据用的私有 tmux（[`crate::tmux_sandbox::SandboxTmux`]：私有 socket、
/// `-f /dev/null`、摘掉 `TMUX` · `TMUX_PANE`、`HOME` 指沙箱目录）里起替身、照剧本按键，照生产抓屏那一条 argv
/// （[`crate::control::capture_pane::capture_argv_with`]）截下，与仓里那份逐字比。
/// 只比不写：不等 ⇒ 现打的落进 `.build/golden/`、红；对过后 `bash tests/scripts/bless-golden.sh` 拷回。
#[cfg(unix)]
#[test]
fn the_ask_box_fixtures_match_what_the_stand_in_draws() {
    let script = guard_core::addr::repo_root().join("tests/__fixtures__/fake-claude-ask-box.sh");
    let sb = crate::tmux_sandbox::SandboxTmux::new("askbox");
    let grab = |t: &str| {
        let argv = crate::control::capture_pane::capture_argv_with(t, false, 0);
        let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
        let o = sb.run(&argv);
        assert!(
            o.status.success(),
            "抓屏：{}",
            String::from_utf8_lossy(&o.stderr)
        );
        String::from_utf8_lossy(&o.stdout).into_owned()
    };
    // 等到屏上出现 `until`、且紧接着再抓一屏一字不差（画完了），回那一屏。
    let settled = |t: &str, until: &str| {
        let last = std::cell::RefCell::new(String::new());
        let done = sb.wait_for(5000, |_| {
            let now = grab(t);
            let ok = now.contains(until) && *last.borrow() == now;
            *last.borrow_mut() = now;
            ok
        });
        assert!(done, "{t} 一直没画出 {until}");
        grab(t)
    };
    let mut red = Vec::new();
    for (i, s) in SHOTS.iter().enumerate() {
        let t = format!("box{i}");
        let mut cmd = format!("exec bash '{}'", script.display());
        for a in s.asks {
            cmd.push_str(&format!(" '{a}'"));
        }
        let (w, h) = (s.size.0.to_string(), s.size.1.to_string());
        sb.ok(&["new-session", "-d", "-s", &t, "-x", &w, "-y", &h, &cmd]);
        let mut got = settled(&t, s.until);
        for (k, until) in s.keys {
            sb.ok(&["send-keys", "-t", &t, k]);
            got = settled(&t, until);
        }
        if let Some(e) = crate::golden::mismatch(s.path, &got) {
            red.push(e);
        }
    }
    assert!(
        red.is_empty(),
        "真屏夹具与替身此刻画的不一致：\n{}",
        red.join("\n")
    );
}
