//! 🔴 **路径的显示形（唯一一处）**：`home` 那一截写成 `~`（`/home/u/a` ⇒ `~/a`，`/home/u` ⇒ `~`；`/` 与 `\` 都认作分隔）。
//! 带路径的回包与壳的答复各配一格它写好的字，桌面与手机照抄、不自己缩。后端（`platform::paths` 转出）与壳都调这里。
//! 家说不出 · 不在它下面 ⇒ 原样。

use std::path::Path;

/// 一个路径的显示形（见模块注）。
pub fn home_short(p: &str, home: Option<&Path>) -> String {
    let short = home_short_bytes(p.as_bytes(), home);
    // 字节一路不拆多字节字：换掉的只是一段 UTF-8 前缀 ⇒ 结果仍是 UTF-8。
    String::from_utf8(short).unwrap_or_else(|_| p.to_string())
}

/// 同 [`home_short`]，按原始字节（文件那一族的路径不解码，非 UTF-8 的名字照样无损）。
pub fn home_short_bytes(p: &[u8], home: Option<&Path>) -> Vec<u8> {
    let Some(home) = home else {
        return p.to_vec();
    };
    let mut h = home.as_os_str().as_encoded_bytes();
    while let [rest @ .., b'/' | b'\\'] = h {
        h = rest;
    }
    if h.is_empty() {
        return p.to_vec();
    }
    match p.strip_prefix(h) {
        Some([]) => b"~".to_vec(),
        Some(rest @ [b'/' | b'\\', ..]) => [b"~".as_slice(), rest].concat(),
        _ => p.to_vec(),
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/copy-core/path_tests.rs"]
mod tests;
