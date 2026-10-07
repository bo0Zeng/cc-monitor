//! 一格文本 ⇄ 一串词（`form.rs`）：切回来逐字相等。

use super::*;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn pick<'a, T>(&mut self, v: &'a [T]) -> &'a T {
        &v[(self.next() % v.len() as u64) as usize]
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// 一格文本的写法：切回来逐字相等（任意词，含空白 · 两种引号 · 反斜杠 · 空串）；几条手写的样子；引号没配对 ⇒ 拒。
#[test]
fn box_text_splits_back_to_the_same_words() {
    let chars = [
        'a', ' ', '\t', '\n', '\'', '"', '\\', '-', '文', '\u{3000}', '$',
    ];
    let mut r = Rng(0xdead_beef_cafe_f00d);
    for _ in 0..5000 {
        let ws: Vec<String> = (0..r.below(4))
            .map(|_| (0..r.below(6)).map(|_| *r.pick(&chars)).collect())
            .collect();
        assert_eq!(
            split_box(&join_box(&ws)),
            Ok(ws.clone()),
            "{:?}",
            join_box(&ws)
        );
    }
    assert_eq!(
        join_box(&s(&["C:\\work", "a b", "it's", ""])),
        "C:\\work 'a b' \"it's\" ''"
    );
    assert_eq!(
        split_box("a\\ b 'c d'\"e\" \"x\\\"y\""),
        Ok(s(&["a b", "c de", "x\"y"]))
    );
    assert_eq!(
        split_box("C:\\work\\x"),
        Ok(s(&["C:\\work\\x"])),
        "引号外的反斜杠只转义空白、引号与它自己"
    );
    for bad in ["'a", "\"a", "a 'b c", "\"a\\\""] {
        assert!(split_box(bad).is_err(), "{bad:?} 放行了");
    }
}
