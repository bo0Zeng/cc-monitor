use super::*;

/// 金样（`tests/__fixtures__/xterm-keys.tsv`，`TERM=xterm-256color`）：一行一下，手机那一侧（`KeyTokensTest.kt`）读同一份。
/// 三列（tab 分隔）：记号（[`Press::token`]）· 字节（小写十六进制）· 方向（`=` 两向 · `>` 只编 · `<` 只解）。
/// 照单字节那几颗 · CSI 字母 · SS3 · `~` 号 · 带修饰的字 · 字的先后排；没有注释行（每行都是数据）。
const GOLDEN: &str = include_str!("../../__fixtures__/xterm-keys.tsv");

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("金样里是十六进制"))
        .collect()
}

/// `(行号, 记号, 字节, 方向)`。
fn rows() -> Vec<(usize, &'static str, Vec<u8>, &'static str)> {
    GOLDEN
        .lines()
        .enumerate()
        .map(|(n, l)| {
            let cols: Vec<&str> = l.split('\t').collect();
            assert_eq!(cols.len(), 3, "第 {} 行该是三列：{l:?}", n + 1);
            (n + 1, cols[0], unhex(cols[1]), cols[2])
        })
        .collect()
}

/// 编码：金样里「两向」与「只编」的每一行，记号 ⇒ 字节逐字节同。
#[test]
fn every_golden_press_encodes_to_its_bytes() {
    let mut seen = 0;
    for (n, token, bytes, dir) in rows() {
        if dir == "<" {
            continue;
        }
        let p = Press::from_token(token).unwrap_or_else(|| panic!("第 {n} 行记号读不出：{token:?}"));
        assert_eq!(presses_bytes(&[p]), bytes, "第 {n} 行 {token:?}");
        seen += 1;
    }
    assert!(seen > 40, "金样里编码那几行太少：{seen}");
}

/// 解码：金样里「两向」与「只解」的每一行，字节 ⇒ 恰好那一下。
#[test]
fn every_golden_byte_run_decodes_to_its_press() {
    let mut seen = 0;
    for (n, token, bytes, dir) in rows() {
        if dir == ">" {
            continue;
        }
        let got = presses_of(&bytes).unwrap_or_else(|| panic!("第 {n} 行解不出：{bytes:02x?}"));
        let got: Vec<String> = got.iter().map(Press::token).collect();
        assert_eq!(got, [token.to_string()], "第 {n} 行 {bytes:02x?}");
        seen += 1;
    }
    assert!(seen > 30, "金样里解码那几行太少：{seen}");
}

/// 表里每颗特殊键，八种修饰里凡编出 CSI / SS3 的，解回来还是那一下（编码 · 解码同一张表）。
#[test]
fn escape_sequences_round_trip_for_every_key_and_modifier() {
    let mut n = 0;
    for key in Key::ALL.into_iter().filter(|k| !matches!(form(*k), Form::Byte(_))) {
        for bits in 0..8u8 {
            let mods = Mods {
                shift: bits & 1 != 0,
                alt: bits & 2 != 0,
                ctrl: bits & 4 != 0,
            };
            let p = Press::Special(key, mods);
            let bytes = presses_bytes(std::slice::from_ref(&p));
            assert_eq!(presses_of(&bytes), Some(vec![p.clone()]), "{p:?} ⇒ {bytes:02x?}");
            n += 1;
        }
    }
    assert!(n > 100, "转义序列那几颗太少：{n}");
}

/// 记号两向：读出来再写回逐字同；修饰照 ctrl · alt · shift 的先后写。
#[test]
fn tokens_round_trip() {
    for (_, token, _, _) in rows() {
        let p = Press::from_token(token).expect("读得出");
        assert_eq!(p.token(), token);
    }
    for bad in ["", "x", "s:nope", "sbogus:up", "c:", "cctrl:ab", "sup"] {
        assert_eq!(Press::from_token(bad), None, "{bad:?} 该读不出");
    }
}

/// 一串里几下：字连着并成一段；Alt-字 · 单独的 Esc · 控制字节各算一下；认不出的序列 ⇒ 整串 `None`。
#[test]
fn a_run_of_bytes_splits_into_presses() {
    let tokens = |b: &[u8]| presses_of(b).map(|v| v.iter().map(Press::token).collect::<Vec<_>>());
    assert_eq!(
        tokens(b"ls\r\x1b[A\x1bxy\x1b"),
        Some(vec!["tls".into(), "s:enter".into(), "s:up".into(), "calt:x".into(), "ty".into(), "s:esc".into()])
    );
    assert_eq!(tokens(b"a\x1b[?1049h"), None, "认不出的 CSI");
    assert_eq!(tokens(b"\x1b[1;9A"), None, "修饰参数出界");
    assert_eq!(tokens(b"\x1b[2;5A"), None, "CSI 字母头一个参数只许 1");
    assert_eq!(tokens(b"\x1b[99999999999~"), None, "参数溢出");
    assert_eq!(tokens(b"\x1b["), None, "没收尾");
    assert_eq!(tokens(&[0xff]), None, "坏 UTF-8");
    assert_eq!(tokens(b""), Some(vec![]));
}

/// 特殊键的线上名两向，且与表的行数一致（一颗一行）。
#[test]
fn key_wire_names_round_trip() {
    for key in Key::ALL {
        assert_eq!(Key::from_wire(key.wire()), Some(key));
    }
    assert_eq!(Key::ALL.len(), 25);
    assert_eq!(Key::from_wire("Up"), None, "线上名是小写");
}
