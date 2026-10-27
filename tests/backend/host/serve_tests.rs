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
    while pushed < QUEUE_CAP + 64 * 1024 {
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
