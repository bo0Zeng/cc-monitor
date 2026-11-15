//! 瘦客户端最底一行归状态行：外层少一行报给托管 · 那一行照模板写 · 提示与队并进那一行 · 模板空照旧盖最底一行。

use super::super::status::{Snap, Values};
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
    fn parsed(&self) -> vt100::Parser {
        let mut p = vt100::Parser::new(24, 80, 0);
        p.process(&self.0.lock().unwrap());
        p
    }
    /// 外层 80×24 上第 `r` 行（从 0 数）的字。
    fn row(&self, r: u16) -> String {
        self.parsed()
            .screen()
            .contents_between(r, 0, r, 80)
            .trim_end()
            .to_string()
    }
    fn len(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

fn snap(template: &str, kv: &[(&str, &str)]) -> Snap {
    Snap {
        template: template.into(),
        values: kv
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<Values>(),
    }
}

/// 一个 80×24 的外层，交那一份状态建好。
fn client(s: Snap) -> (Client, Screen) {
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
    (Client::new(outer, s), screen)
}

fn welcome(c: &mut Client, rows: u16, mine: bool) {
    let holder = Some(Holder {
        client: if mine { 1 } else { 2 },
        kind: WhoKind::Terminal,
        front: None,
        pid: None,
        mine,
    });
    assert!(c
        .frame(Frame::Ctl(Ctl::Welcome {
            proto: PROTO,
            id: "t".into(),
            size: Size { rows, cols: 80 },
            you: 1,
            holder,
            screen: String::new(),
        }))
        .is_none());
}

fn no_list() -> Vec<(String, String)> {
    Vec::new()
}

/// ★ 模板要那一行 ⇒ 报给托管的是外层少一行；模板空 ⇒ 整个外层。
#[test]
fn the_size_told_to_the_host_leaves_the_last_row_for_the_status_line() {
    let (c, _) = client(snap("{name}", &[("name", "w1")]));
    assert_eq!(c.pty_size(), Some(Size { rows: 23, cols: 80 }));
    let (c, _) = client(snap("", &[]));
    assert_eq!(c.pty_size(), Some(Size { rows: 24, cols: 80 }));
}

/// ★ 最底一行是照模板写的那一句（左段靠左、右段靠右）；里面打满 23 行也盖不到它。
#[test]
fn the_last_row_shows_the_template_and_the_inside_never_covers_it() {
    let (mut c, s) = client(snap(
        "{name} · {cwd}|{queue}",
        &[("name", "w1"), ("cwd", "~/a"), ("queue", "排队 · 2 条")],
    ));
    welcome(&mut c, 23, true);
    let bottom = s.row(23);
    assert!(bottom.starts_with("w1 · ~/a"), "{bottom:?}");
    assert!(bottom.ends_with("排队 · 2 条"), "{bottom:?}");
    let mut fill = Vec::new();
    for i in 0..40 {
        fill.extend_from_slice(format!("line{i}\r\n").as_bytes());
    }
    assert!(c.frame(Frame::Out(fill)).is_none());
    assert!(s.row(23).starts_with("w1 · ~/a"), "{:?}", s.row(23));
    assert!(
        s.row(22).is_empty() || s.row(22).starts_with("line"),
        "{:?}",
        s.row(22)
    );
}

/// ★ 不持有敲一下键 ⇒ 那一行是「… · 回车接管」接着模板那一句，队那一格也在。
#[test]
fn a_viewer_sees_the_take_over_hint_and_the_template_in_one_line() {
    let (mut c, s) = client(snap(
        "{name}|{queue}",
        &[("name", "w1"), ("queue", "排队 · 3 条")],
    ));
    welcome(&mut c, 23, false);
    let _ = c.key(b"z", &no_list);
    let bottom = s.row(23);
    let sep = copy_text("kit.text.sep", &[]);
    assert!(
        bottom.starts_with(&format!("{}{sep}w1", held_line(c.holder.as_ref()))),
        "{bottom:?}"
    );
    assert!(bottom.ends_with("排队 · 3 条"), "{bottom:?}");
}

/// 值来了 · 持有者换了 ⇒ 那一行跟着重画；值没变 ⇒ 外层一个字节都不多写。
#[test]
fn the_line_follows_the_values_and_writes_nothing_when_nothing_changed() {
    let (mut c, s) = client(snap("{name}|{holder}", &[("name", "w1")]));
    welcome(&mut c, 23, false);
    assert!(
        s.row(23)
            .ends_with(&hold::said_of(hold::HolderWho::TerminalWindow, false)),
        "{:?}",
        s.row(23)
    );
    assert_eq!(c.snap_now(snap("{name}|{holder}", &[("name", "w2")])), None);
    assert!(s.row(23).starts_with("w2"), "{:?}", s.row(23));
    let before = s.len();
    assert_eq!(c.snap_now(snap("{name}|{holder}", &[("name", "w2")])), None);
    assert_eq!(s.len(), before, "没变不写");
    assert!(c
        .frame(Frame::Ctl(Ctl::Holder {
            holder: Some(Holder {
                client: 1,
                kind: WhoKind::Terminal,
                front: None,
                pid: None,
                mine: true,
            })
        }))
        .is_none());
    assert!(
        s.row(23)
            .ends_with(&hold::said_of(hold::HolderWho::TerminalWindow, true)),
        "{:?}",
        s.row(23)
    );
}

/// 「结束？」临时占那一行，收掉照模板重画。
#[test]
fn the_end_question_takes_the_line_and_gives_it_back() {
    let (mut c, s) = client(snap("{name}", &[("name", "w1")]));
    welcome(&mut c, 23, true);
    let _ = c.key(b"\x02x", &no_list);
    assert_eq!(s.row(23), copy_text("beHost.attach.confirmEnd", &[]));
    let _ = c.key(b"n", &no_list);
    assert_eq!(s.row(23), "w1");
}

/// ★ 模板换成空 ⇒ 当场照整个外层报尺寸、那一行不再留；换回来 ⇒ 又少一行。
#[test]
fn switching_the_template_on_and_off_resizes_at_once() {
    let (mut c, s) = client(snap("{name}", &[("name", "w1")]));
    welcome(&mut c, 23, true);
    assert_eq!(c.snap_now(snap("", &[])), Some(Size { rows: 24, cols: 80 }));
    assert!(!s.row(23).contains("w1"), "{:?}", s.row(23));
    assert_eq!(
        c.snap_now(snap("{name}", &[("name", "w1")])),
        Some(Size { rows: 23, cols: 80 })
    );
}

/// 模板空 ⇒ 不持有敲键时提示照旧盖最底一行（连队那一句）。
#[test]
fn without_a_line_the_hint_still_covers_the_last_row() {
    let (mut c, s) = client(snap("", &[("queue", "排队 · 3 条")]));
    welcome(&mut c, 24, false);
    let _ = c.key(b"z", &no_list);
    let sep = copy_text("kit.text.sep", &[]);
    assert_eq!(
        s.row(23),
        format!("{}{sep}排队 · 3 条", held_line(c.holder.as_ref()))
    );
}

/// 外层改尺寸 ⇒ 报给托管的是新外层少一行。
#[test]
fn an_outer_resize_reports_one_row_less() {
    let rows = Arc::new(Mutex::new(24u16));
    let r2 = Arc::clone(&rows);
    let (_k, keys) = tokio::sync::mpsc::unbounded_channel();
    let (_r, resized) = tokio::sync::mpsc::unbounded_channel();
    let outer = Outer {
        keys,
        resized,
        size: Box::new(move || {
            Some(Size {
                rows: *r2.lock().unwrap(),
                cols: 80,
            })
        }),
        out: Box::new(Screen::default()),
        prefix: keys::DEFAULT_PREFIX,
        vi: false,
        in_tmux: false,
    };
    let mut c = Client::new(outer, snap("{name}", &[]));
    *rows.lock().unwrap() = 30;
    assert_eq!(c.outer_resized(), Some(Size { rows: 29, cols: 80 }));
}

/// 模板里有时刻那一格才起分钟拍。
#[test]
fn only_a_template_with_the_time_ticks() {
    let (c, _) = client(snap("{name}|{time}", &[]));
    assert!(c.ticks());
    let (c, _) = client(snap("{name}", &[]));
    assert!(!c.ticks());
}

/// 「在 tmux 里」那一句临时占那一行：敲下一个键就收掉，照模板重画。
#[test]
fn the_in_tmux_notice_goes_away_at_the_next_key() {
    let (mut c, s) = client(snap("{name}", &[("name", "w1")]));
    welcome(&mut c, 23, true);
    c.notice_now(Some(copy_text("beHost.attach.inTmux", &[])));
    assert_eq!(s.row(23), copy_text("beHost.attach.inTmux", &[]));
    let _ = c.key(b"a", &no_list);
    assert_eq!(s.row(23), "w1");
}
