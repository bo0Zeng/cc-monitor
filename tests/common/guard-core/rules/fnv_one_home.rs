//! FNV-1a 全仓只有一份：`cmd-wire` 的 `fnv` 模块。别处的生产代码里出现 FNV 的起点或乘数 ⇒ 又有人自己写了那个循环。
//!
//! 认法：十六进制字面量（去下划线、去类型后缀）的值落在下面那几个常数里。拆成十进制、拼出来的、调别的库的，这里看不见。

use super::{equal, none, rule};
use std::collections::BTreeSet;

const HOME: &str = "src/common/cmd-wire/src/fnv.rs";

/// 64 位起点 · 64 位乘数 · 32 位起点 · 32 位乘数。
const CONSTANTS: [u64; 4] = [
    0xcbf2_9ce4_8422_2325,
    0x0100_0000_01b3,
    0x811c_9dc5,
    0x0100_0193,
];

/// 一段代码里值落在 [`CONSTANTS`] 里的十六进制字面量（按出现先后）。
fn hits(code: &str) -> Vec<u64> {
    let b = code.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < b.len() {
        let starts = b[i] == b'0'
            && (b[i + 1] | 0x20) == b'x'
            && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_'));
        if !starts {
            i += 1;
            continue;
        }
        let mut j = i + 2;
        let mut digits = String::new();
        while j < b.len() && (b[j].is_ascii_hexdigit() || b[j] == b'_') {
            if b[j] != b'_' {
                digits.push(char::from(b[j]));
            }
            j += 1;
        }
        // `_u64` 这种后缀：`_u` 不是十六进制位，前面吃进去的 `_` 已经丢了，剩下的数位照样对。
        if let Ok(v) = u64::from_str_radix(&digits, 16) {
            if CONSTANTS.contains(&v) {
                out.push(v);
            }
        }
        i = j.max(i + 2);
    }
    out
}

#[test]
fn fnv_lives_only_in_cmd_wire() {
    rule(|repo| {
        let home: BTreeSet<u64> = hits(repo.file(HOME).prod()).into_iter().collect();
        equal("FNV 那一份住处里的常数", home, CONSTANTS);
        let found: Vec<String> = repo
            .under(&["src"], &["rs", "ts", "kt", "kts"])
            .into_iter()
            .filter(|f| f.rel != HOME && !f.rel.starts_with("src/vendor/"))
            .filter_map(|f| {
                let code = if f.rel.ends_with(".rs") {
                    f.prod()
                } else {
                    f.text()
                };
                let n = hits(code).len();
                (n > 0).then(|| format!("{}（{n} 处）", f.rel))
            })
            .collect();
        let control =
            hits("let mut h: u64 = 0xcbf2_9ce4_8422_2325_u64; h = h.wrapping_mul(0x0100_0193);")
                .len();
        none(
            "FNV 的起点 / 乘数出现在 `cmd-wire::fnv` 之外（改调那一份）",
            found,
            control,
        );
    });
}
