//! 托管口那一张账：慢端整屏重来 · 一个写者 · 尺寸跟持有者。

use super::super::wire::{decode, Ctl, Frame, Size, Want, Who, WhoKind, PROTO};
use super::*;

fn hello(want: Want, kind: WhoKind, size: Option<Size>) -> Frame {
    Frame::Ctl(Ctl::Hello {
        proto: PROTO,
        build: "t".into(),
        want,
        size,
        who: Who { kind, client: None },
    })
}

fn sz(cols: u16, rows: u16) -> Size {
    Size { rows, cols }
}

/// 取走一端队列里的帧、解开。
fn take(out: &Outbox) -> Vec<Frame> {
    let (raw, _) = out.q.lock().unwrap().drain();
    raw.iter()
        .map(|b| decode(b).expect("解得开").expect("整帧").0)
        .collect()
}

/// 照一端收到的帧还原它那一屏（welcome / resync 整屏重来，out 接着算，size 改尺寸）。
fn replay(frames: &[Frame], mut p: vt100::Parser) -> vt100::Parser {
    for f in frames {
        match f {
            Frame::Ctl(Ctl::Welcome { size, screen, .. })
            | Frame::Ctl(Ctl::Resync { size, screen }) => {
                p = vt100::Parser::new(size.rows, size.cols, 0);
                p.process(screen.as_bytes());
            }
            Frame::Ctl(Ctl::Size { rows, cols }) => p.screen_mut().set_size(*rows, *cols),
            Frame::Out(b) => p.process(b),
            _ => {}
        }
    }
    p
}

fn grid(s: &vt100::Screen) -> String {
    s.contents_formatted().iter().map(|b| *b as char).collect()
}

#[test]
fn a_client_whose_queue_overflows_gets_a_resync_and_rebuilds_the_same_screen() {
    let mut hub = Hub::new("h", sz(80, 24));
    let (slow, slow_out) = hub.connect(Some(1), 0);
    hub.frame(slow, hello(Want::View, WhoKind::Backend, None), 0);
    // 不读：一直往里塞，超过上限。
    let line = format!("\x1b[3{}m{}\r\n", 1, "x".repeat(70));
    let mut pushed = 0usize;
    let mut i = 0u32;
    while pushed < QUEUE_CAP.as_len() + 64 * 1024 {
        let chunk = format!("{i:06} {line}");
        pushed += chunk.len();
        hub.pty_out(chunk.as_bytes());
        i += 1;
    }
    assert!(slow_out.q.lock().unwrap().overflows > 0, "塞满过");
    let frames = take(&slow_out);
    assert!(
        frames
            .iter()
            .any(|f| matches!(f, Frame::Ctl(Ctl::Resync { .. }))),
        "收到了 resync"
    );
    let p = replay(&frames, vt100::Parser::new(24, 80, 0));
    assert_eq!(
        grid(p.screen()),
        grid(hub.screen.screen()),
        "还原的屏与托管那一屏逐格相等"
    );
}

#[test]
fn a_non_holder_typing_gets_held_elsewhere_and_nothing_reaches_the_pty() {
    let mut hub = Hub::new("h", sz(80, 24));
    let (a, _a_out) = hub.connect(Some(10), 0);
    hub.frame(
        a,
        hello(Want::Input, WhoKind::Terminal, Some(sz(120, 40))),
        0,
    );
    let (b, b_out) = hub.connect(Some(11), 0);
    hub.frame(b, hello(Want::View, WhoKind::Terminal, Some(sz(80, 24))), 0);
    take(&b_out);
    let eff = hub.frame(b, Frame::In(b"rm -rf\r".to_vec()), 1);
    assert!(eff.is_empty(), "伪终端一个字节都没收到：{eff:?}");
    let got = take(&b_out);
    assert!(
        matches!(got.as_slice(), [Frame::Ctl(Ctl::HeldElsewhere { holder: Some(h) })] if h.client == a && !h.mine),
        "{got:?}"
    );
    assert_eq!(
        hub.frame(a, Frame::In(b"ok".to_vec()), 2),
        vec![Effect::Write(b"ok".to_vec())]
    );
}

#[test]
fn the_pty_takes_the_holders_size_and_a_viewer_never_changes_it() {
    let mut hub = Hub::new("h", sz(80, 24));
    let (a, _) = hub.connect(None, 0);
    let eff = hub.frame(
        a,
        hello(Want::Input, WhoKind::Terminal, Some(sz(120, 40))),
        0,
    );
    assert_eq!(eff, vec![Effect::Resize(sz(120, 40))]);
    let (b, _) = hub.connect(None, 0);
    // B 接上只看：仍 120×40。
    assert!(hub
        .frame(b, hello(Want::View, WhoKind::Terminal, Some(sz(80, 24))), 0)
        .is_empty());
    assert_eq!(hub.size(), sz(120, 40));
    // 只看的那一端改尺寸也不动伪终端。
    assert!(hub
        .frame(b, Frame::Ctl(Ctl::Resize { rows: 30, cols: 90 }), 0)
        .is_empty());
    assert_eq!(hub.size(), sz(120, 40));
    // B 接管 ⇒ 跟 B 报的最近那个尺寸。
    let eff = hub.frame(b, Frame::Ctl(Ctl::Take), 1);
    assert_eq!(eff, vec![Effect::Resize(sz(90, 30))]);
    assert_eq!(hub.holder, Some(b));
}

#[test]
fn the_newest_terminal_wanting_input_holds_and_a_backend_never_does_by_hello() {
    let mut hub = Hub::new("h", sz(80, 24));
    let (a, _) = hub.connect(None, 0);
    hub.frame(
        a,
        hello(Want::Input, WhoKind::Terminal, Some(sz(80, 24))),
        0,
    );
    let (be, _) = hub.connect(None, 0);
    hub.frame(be, hello(Want::Input, WhoKind::Backend, None), 0);
    assert_eq!(hub.holder, Some(a), "后端 hello 不抢输入");
    let (b, _) = hub.connect(None, 0);
    hub.frame(
        b,
        hello(Want::Input, WhoKind::Terminal, Some(sz(80, 24))),
        0,
    );
    assert_eq!(hub.holder, Some(b), "最新连上的真终端持有");
}

#[test]
fn when_the_holder_leaves_input_goes_to_the_latest_other_terminal_or_nobody() {
    let mut hub = Hub::new("h", sz(80, 24));
    let (a, _) = hub.connect(None, 0);
    hub.frame(
        a,
        hello(Want::Input, WhoKind::Terminal, Some(sz(100, 30))),
        0,
    );
    let (b, _) = hub.connect(None, 0);
    hub.frame(
        b,
        hello(Want::Input, WhoKind::Terminal, Some(sz(80, 24))),
        0,
    );
    assert_eq!(hub.holder, Some(b));
    let eff = hub.gone(b);
    assert_eq!(hub.holder, Some(a));
    assert_eq!(eff, vec![Effect::Resize(sz(100, 30))], "尺寸回到 A 的");
    hub.gone(a);
    assert_eq!(hub.holder, None);
    assert_eq!(hub.size(), sz(100, 30), "没人持有 ⇒ 保持上一次的");
}

#[test]
fn a_backend_can_take_and_release_back_to_the_terminal() {
    let mut hub = Hub::new("h", sz(80, 24));
    let (a, a_out) = hub.connect(None, 0);
    hub.frame(
        a,
        hello(Want::Input, WhoKind::Terminal, Some(sz(80, 24))),
        0,
    );
    let (be, _) = hub.connect(None, 0);
    hub.frame(be, hello(Want::View, WhoKind::Backend, None), 0);
    take(&a_out);
    assert!(
        hub.frame(be, Frame::Ctl(Ctl::Take), 1).is_empty(),
        "后端接管不改尺寸"
    );
    assert_eq!(hub.holder, Some(be));
    let got = take(&a_out);
    assert!(
        got.iter().any(|f| matches!(f, Frame::Ctl(Ctl::Holder { holder: Some(h) }) if h.client == be && !h.mine)),
        "A 收到持有者换了：{got:?}"
    );
    hub.frame(be, Frame::Ctl(Ctl::Release), 2);
    assert_eq!(hub.holder, Some(a));
}

#[test]
fn welcome_carries_the_whole_screen_and_who_holds() {
    let mut hub = Hub::new("h", sz(40, 5));
    hub.pty_out(b"hello there");
    let (a, a_out) = hub.connect(Some(7), 0);
    hub.frame(a, hello(Want::Input, WhoKind::Terminal, Some(sz(40, 5))), 0);
    let got = take(&a_out);
    let p = replay(&got, vt100::Parser::new(5, 40, 0));
    assert_eq!(grid(p.screen()), grid(hub.screen.screen()));
    assert!(got.iter().any(
        |f| matches!(f, Frame::Ctl(Ctl::Holder { holder: Some(h) }) if h.mine && h.pid == Some(7))
    ));
}

#[test]
fn a_frame_before_hello_drops_the_connection() {
    let mut hub = Hub::new("h", sz(80, 24));
    let (a, _) = hub.connect(None, 0);
    assert_eq!(
        hub.frame(a, Frame::In(b"x".to_vec()), 0),
        vec![Effect::Drop(a)]
    );
}

#[test]
fn terminal_queries_are_answered_into_the_pty_not_to_clients() {
    let mut hub = Hub::new("h", sz(80, 24));
    let (a, a_out) = hub.connect(None, 0);
    hub.frame(a, hello(Want::View, WhoKind::Backend, None), 0);
    take(&a_out);
    let eff = hub.pty_out(b"\x1b[c");
    assert_eq!(
        eff,
        vec![Effect::Write(super::super::screen::DA1_REPLY.to_vec())]
    );
}

#[test]
fn child_exit_tells_every_client_with_the_code_and_closes_queues() {
    let mut hub = Hub::new("h", sz(80, 24));
    let (a, a_out) = hub.connect(None, 0);
    hub.frame(a, hello(Want::Input, WhoKind::Terminal, None), 0);
    take(&a_out);
    hub.child_exited(Some(3));
    let (raw, closed) = a_out.q.lock().unwrap().drain();
    assert!(closed);
    let got: Vec<Frame> = raw.iter().map(|b| decode(b).unwrap().unwrap().0).collect();
    assert_eq!(got, vec![Frame::Ctl(Ctl::Ended { code: Some(3) })]);
}

#[test]
fn clients_lists_everyone_with_holder_and_mine() {
    let mut hub = Hub::new("h", sz(80, 24));
    let (a, a_out) = hub.connect(Some(5), 100);
    hub.frame(a, hello(Want::Input, WhoKind::Terminal, None), 100);
    let (b, _) = hub.connect(Some(6), 200);
    hub.frame(b, hello(Want::View, WhoKind::Backend, None), 200);
    take(&a_out);
    hub.frame(a, Frame::Ctl(Ctl::Clients), 300);
    let got = take(&a_out);
    let Some(Frame::Ctl(Ctl::ClientsAre { clients })) = got.first() else {
        panic!("{got:?}")
    };
    assert_eq!(clients.len(), 2);
    assert!(clients.iter().any(|c| c.client == a
        && c.holder
        && c.mine
        && c.pid == Some(5)
        && c.last_ms == 300));
    assert!(clients
        .iter()
        .any(|c| c.client == b && !c.holder && !c.mine));
}

/// 一个真终端持有、一个后端只看着（后端接上、说过 hello，队列已取空）。
fn held_by_a_terminal_with_a_backend_viewer() -> (Hub, u64, u64, Outbox) {
    let mut hub = Hub::new("h", sz(80, 24));
    let (t, _t_out) = hub.connect(Some(10), 0);
    hub.frame(
        t,
        hello(Want::Input, WhoKind::Terminal, Some(sz(80, 24))),
        0,
    );
    let (b, b_out) = hub.connect(Some(11), 0);
    hub.frame(b, hello(Want::View, WhoKind::Backend, None), 0);
    take(&b_out);
    (hub, t, b, b_out)
}

fn say(text: &str) -> Frame {
    Frame::Ctl(Ctl::Say { text: text.into() })
}

/// ★ 投递那一句：输入在真终端手里，后端只看着说一句 ⇒ 照写进伪终端，持有者不换、谁都不收 `holder`。
#[test]
fn a_backend_saying_a_line_writes_it_into_the_pty_without_changing_the_holder() {
    let (mut hub, t, b, b_out) = held_by_a_terminal_with_a_backend_viewer();
    let eff = hub.frame(b, say("hi\r"), 1);
    assert_eq!(eff, vec![Effect::Write(b"hi\r".to_vec())]);
    assert_eq!(take(&b_out), vec![Frame::Ctl(Ctl::Said { pending: false })]);
    assert_eq!(hub.holder, Some(t), "持有者还是那个真终端");
    assert_eq!(
        hub.frame(t, Frame::In(b"x".to_vec()), 2),
        vec![Effect::Write(b"x".to_vec())],
        "真终端照旧打字"
    );
    // 真终端不说 `say`：它有自己的 `in`。
    assert_eq!(hub.frame(t, say("no\r"), 3), vec![Effect::Drop(t)]);
}

/// ★ 持有者敲了半句没回车 ⇒ 这一句拒（`pending`），伪终端一个字节没多；回车之后 · 删回空行之后照写。
#[test]
fn a_say_while_the_holder_has_typed_half_a_line_is_refused_and_writes_nothing() {
    let (mut hub, t, b, b_out) = held_by_a_terminal_with_a_backend_viewer();
    hub.frame(t, Frame::In("半句".as_bytes().to_vec()), 1);
    assert!(hub.frame(b, say("hi\r"), 2).is_empty(), "一个字节没多");
    assert_eq!(take(&b_out), vec![Frame::Ctl(Ctl::Said { pending: true })]);
    // 删两个字 ⇒ 行空了。
    hub.frame(t, Frame::In(b"\x7f\x7f".to_vec()), 3);
    assert_eq!(
        hub.frame(b, say("a\r"), 4),
        vec![Effect::Write(b"a\r".to_vec())]
    );
    // 再敲 ⇒ 拒；回车交了 ⇒ 照写。
    hub.frame(t, Frame::In(b"ls".to_vec()), 5);
    assert!(hub.frame(b, say("b\r"), 6).is_empty());
    hub.frame(t, Frame::In(b"\r".to_vec()), 7);
    assert_eq!(
        hub.frame(b, say("b\r"), 8),
        vec![Effect::Write(b"b\r".to_vec())]
    );
    // 方向键 · 焦点这类转义序列不算敲字。
    hub.frame(t, Frame::In(b"\x1b[A\x1b[I\x1bOB".to_vec()), 9);
    assert_eq!(
        hub.frame(b, say("c\r"), 10),
        vec![Effect::Write(b"c\r".to_vec())]
    );
}

/// ★ 半句跟伪终端走、不跟持有者走（A3-3-32-13）：持有者 A 敲了 `abc` 没回车 → 后端接管 → 后端说一句 ⇒ 仍 `pending`、伪终端一个字节没多；
/// 新持有者 Ctrl-U 清掉那一行之后照写。再敲半句、另一个真终端接上、原持有者走了 ⇒ 半句照旧挡着。
#[test]
fn changing_the_holder_keeps_the_half_typed_line_blocking_a_say() {
    let (mut hub, t, b, b_out) = held_by_a_terminal_with_a_backend_viewer();
    hub.frame(t, Frame::In(b"abc".to_vec()), 1);
    let (d, _d_out) = hub.connect(Some(12), 2);
    hub.frame(d, hello(Want::View, WhoKind::Backend, None), 2);
    hub.frame(d, Frame::Ctl(Ctl::Take), 3);
    assert_eq!(hub.holder, Some(d));
    take(&b_out);
    assert!(hub.frame(b, say("hi\r"), 4).is_empty(), "一个字节没多");
    assert_eq!(take(&b_out), vec![Frame::Ctl(Ctl::Said { pending: true })]);
    // 新持有者清掉那一行 ⇒ 照写。
    hub.frame(d, Frame::In(b"\x15".to_vec()), 5);
    assert_eq!(
        hub.frame(b, say("a\r"), 6),
        vec![Effect::Write(b"a\r".to_vec())]
    );
    // 再敲半句，换个真终端接上、原持有者走了 ⇒ 半句还在、照旧挡着。
    hub.frame(d, Frame::In(b"xy".to_vec()), 7);
    let (t2, _o) = hub.connect(Some(13), 8);
    hub.frame(
        t2,
        hello(Want::Input, WhoKind::Terminal, Some(sz(80, 24))),
        8,
    );
    hub.gone(d);
    assert!(hub.frame(b, say("b\r"), 9).is_empty());
}

/// ★ 中断（A3-3-32-10）：后端说 `interrupt` ⇒ 伪终端收一个 ESC、回 `interrupted`；不看持有者、不看半句、不换持有者。
/// 真终端说 `interrupt` ⇒ 不是它该说的（它有自己的 `in`），断它。
#[test]
fn an_interrupt_from_the_backend_writes_one_esc_whoever_holds_and_whatever_is_half_typed() {
    let (mut hub, t, b, b_out) = held_by_a_terminal_with_a_backend_viewer();
    hub.frame(t, Frame::In(b"half".to_vec()), 1);
    let eff = hub.frame(b, Frame::Ctl(Ctl::Interrupt), 2);
    assert_eq!(eff, vec![Effect::Write(b"\x1b".to_vec())]);
    assert_eq!(take(&b_out), vec![Frame::Ctl(Ctl::Interrupted)]);
    assert_eq!(hub.holder, Some(t), "持有者不换");
    assert!(hub.frame(b, say("x\r"), 3).is_empty(), "半句照旧在");
    assert_eq!(
        hub.frame(t, Frame::Ctl(Ctl::Interrupt), 4),
        vec![Effect::Drop(t)]
    );
}
