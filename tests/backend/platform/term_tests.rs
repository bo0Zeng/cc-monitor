//! 外层终端那一层：Windows 控制台输入事件 ⇒ 键的字节 · 尺寸变了（纯解码，哪个平台都测）。

use super::console_keys::{Decoded, Decoder, Event};

fn key(unit: u16) -> Event {
    Event::Key {
        down: true,
        repeat: 1,
        unit,
    }
}

fn units(s: &str) -> Vec<Event> {
    s.encode_utf16().map(key).collect()
}

/// 虚拟终端输入模式下方向键是一串带字的按下事件：原样拼成 `ESC [ A`；抬起的那一半不算。
#[test]
fn key_downs_become_the_bytes_and_key_ups_are_dropped() {
    let mut events = units("\x1b[A");
    events.insert(
        1,
        Event::Key {
            down: false,
            repeat: 1,
            unit: u16::from(b'x'),
        },
    );
    let got = Decoder::default().feed(&events);
    assert_eq!(
        got,
        Decoded {
            bytes: b"\x1b[A".to_vec(),
            resized: false
        }
    );
}

/// 不带字的键（只按了 Shift）· 鼠标 · 焦点那一类不进字节；尺寸事件单说。
#[test]
fn keys_without_a_character_and_other_events_say_nothing_but_a_resize_is_told() {
    let got = Decoder::default().feed(&[key(0), Event::Other, Event::Resized]);
    assert_eq!(
        got,
        Decoded {
            bytes: Vec::new(),
            resized: true
        }
    );
}

/// 按住不放的那一下带重复次数：照次数出。
#[test]
fn a_repeated_key_comes_out_that_many_times() {
    let got = Decoder::default().feed(&[Event::Key {
        down: true,
        repeat: 3,
        unit: u16::from(b'a'),
    }]);
    assert_eq!(got.bytes, b"aaa");
}

/// 基本平面以外的字（表情）是一对代理码元：拼成一个字的 UTF-8；前半个落在上一批里也拼得上。
#[test]
fn a_surrogate_pair_is_one_character_even_across_two_batches() {
    let pair: Vec<u16> = "😀中".encode_utf16().collect();
    let mut d = Decoder::default();
    let first = d.feed(&[key(pair[0])]);
    assert!(first.bytes.is_empty(), "前半个先不出");
    let rest: Vec<Event> = pair[1..].iter().copied().map(key).collect();
    assert_eq!(d.feed(&rest).bytes, "😀中".as_bytes());
    // 孤零零的后半个 ⇒ 丢掉，不出乱码。
    assert!(Decoder::default().feed(&[key(pair[1])]).bytes.is_empty());
}
