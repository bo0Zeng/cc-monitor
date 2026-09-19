//! 秤 7 变体 C' 的**等价性证据**。不是 cargo target，独立编译独立跑：
//!
//!     rustc -O -o /tmp/s7eq tests/evidence/S7-blank-line-equivalence.rs && /tmp/s7eq
//!
//! 对拍两个「这一行算不算可计行」的谓词：
//!   · `orig`    = `read_session_tail` 今天那一行（整行 `from_utf8_lossy` + `trim`）
//!   · `variant` = 先看字节短路，短路不下判才退回 `orig`
//! 只要有一个「ASCII 且非空白」的字节，这行就一定不是空行 —— ASCII 字节不可能是
//! 多字节 UTF-8 序列的一部分（续/首字节都 ≥ 0x80），而 `U+FEFF` 与所有 Unicode
//! White_Space 在 ASCII 段里恰好就是 `char::from(b).is_whitespace()` 那一组。
//!
//! ⚠ 这里**必须**是 `char::from(*b).is_whitespace()` 而不是 `b.is_ascii_whitespace()`：
//! 后者**不含 `U+000B`**（垂直制表），而 `str::trim` 含它 ⇒ 一行只有 `\x0b` 时两者分歧。
//! 第一版变体就是这么写的，靠这支穷举才逮出来。

fn orig(b: &[u8]) -> bool {
    let t = String::from_utf8_lossy(b);
    t.trim_start_matches('\u{feff}').trim().is_empty()
}
fn fastpath_says_nonblank(b: &[u8]) -> bool {
    b.iter()
        .any(|c| c.is_ascii() && !char::from(*c).is_whitespace())
}
fn variant(b: &[u8]) -> bool {
    if fastpath_says_nonblank(b) {
        false
    } else {
        orig(b)
    }
}
fn main() {
    let mut bad = 0u64;
    // ① 单字节穷举
    for x in 0u16..=255 {
        let b = [x as u8];
        if orig(&b) != variant(&b) {
            bad += 1;
            println!("单字节分歧 {x:#04x}");
        }
    }
    // ② 双字节穷举
    for x in 0u16..=255 {
        for y in 0u16..=255 {
            let b = [x as u8, y as u8];
            if orig(&b) != variant(&b) {
                bad += 1;
                if bad < 10 {
                    println!("双字节分歧 {x:#04x} {y:#04x}");
                }
            }
        }
    }
    // ③ 三字节穷举（含所有 BOM / 非法 UTF-8 / CJK 前缀组合）
    for x in 0u16..=255 {
        for y in 0u16..=255 {
            for z in 0u16..=255 {
                let b = [x as u8, y as u8, z as u8];
                if orig(&b) != variant(&b) {
                    bad += 1;
                    if bad < 10 {
                        println!("三字节分歧 {x:#04x} {y:#04x} {z:#04x}");
                    }
                }
            }
        }
    }
    // ④ 随机 0..12 字节 × 300 万
    let mut s: u64 = 0x9E3779B97F4A7C15;
    let mut rng = || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    for _ in 0..3_000_000 {
        let n = (rng() % 13) as usize;
        let v: Vec<u8> = (0..n).map(|_| (rng() % 256) as u8).collect();
        if orig(&v) != variant(&v) {
            bad += 1;
            if bad < 20 {
                println!("随机分歧 {v:?}");
            }
        }
    }
    println!("分歧总数 = {bad}（0 = 两个谓词在这些人群上逐位等价）");
}
