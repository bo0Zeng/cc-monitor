//! 瘦客户端最底一行：不持有输入时，持有者那一句与队那一句并成一句（后端写好），谁也不盖掉谁。

use super::super::wire::Size;
use super::*;
use std::sync::{Arc, Mutex};

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
    /// 外层 80×24 上最底那一行的字。
    fn bottom(&self) -> String {
        let mut p = vt100::Parser::new(24, 80, 0);
        p.process(&self.0.lock().unwrap());
        p.screen().contents_between(23, 0, 23, 80).trim_end().to_string()
    }
}

/// 一个 80×24 的外层，接上时谁都不持有输入。
fn viewer() -> (Client, Screen) {
    let (_k, keys) = tokio::sync::mpsc::unbounded_channel();
    let (_r, resized) = tokio::sync::mpsc::unbounded_channel();
    let screen = Screen::default();
    let outer = Outer {
        keys,
        resized,
        size: Box::new(|| Some(Size { rows: 24, cols: 80 })),
        out: Box::new(screen.clone()),
        prefix: keys::DEFAULT_PREFIX,
        vi: false,
        in_tmux: false,
    };
    let mut c = Client::new(outer);
    assert!(c
        .frame(Frame::Ctl(Ctl::Welcome {
            proto: PROTO,
            id: "t".into(),
            size: Size { rows: 24, cols: 80 },
            you: 1,
            holder: None,
            screen: String::new(),
        }))
        .is_none());
    (c, screen)
}

fn waiting(n: usize) -> String {
    copy_text("beQueue.said.waiting", &[("n", &n.to_string())])
}

/// ★ 不持有、队里三条、敲一下键出提示 ⇒ 最底一行逐字是「无人持有输入 · 回车接管 · 排队 · 3 条」。
#[test]
fn a_viewer_sees_the_holder_and_the_queue_in_one_bottom_line() {
    let (mut c, s) = viewer();
    c.queued_now(Some(waiting(3)));
    let _ = c.key(b"z", &Vec::new);
    assert_eq!(s.bottom(), "无人持有输入 · 回车接管 · 排队 · 3 条");
}

/// 提示盖着时队变了 ⇒ 照新的整句重盖（持有者那一句还在）；队清了 ⇒ 只剩持有者那一句。
#[test]
fn the_hint_follows_the_queue_while_it_is_shown() {
    let (mut c, s) = viewer();
    let _ = c.key(b"z", &Vec::new);
    let held = held_line(None);
    assert_eq!(s.bottom(), held);
    c.queued_now(Some(waiting(2)));
    assert_eq!(
        s.bottom(),
        format!("{held}{}{}", copy_text("kit.text.sep", &[]), waiting(2))
    );
    c.queued_now(None);
    assert_eq!(s.bottom(), held);
}
