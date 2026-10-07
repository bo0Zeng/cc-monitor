//! 路径的原始字节 —— `files-read` 这一族的地基。非 UTF-8 文件名一旦在某一跳被有损解码，回程拿着替换字符去找的是一个不存在的名字
//! ⇒ 本族从遍历到回送，路径一路走字节。
//!
//! 1. 取字节走 [`std::ffi::OsStr::as_encoded_bytes`]：跨平台、无损、不需要平台条件编译（本族不住 `platform/`，写 `std::os::…` 会让 `cfgless_guard` 红）。
//! 2. 一处有损解码都不许有：把路径转成人话的那几个调用（连 `Path` 的 `Display`）在本族目录下零命中
//!    （`tests/backend/files/capability_guard.rs` 的 `no_lossy_decode_anywhere_in_the_family`）。
//! 3. 回送时 [`to_json`] 对有效 UTF-8 才交字符串，其余一律交 `{"b16": "<十六进制>"}`；两条路都双向无损（往返恒等判据，含非 UTF-8 夹具）。
//!
//! 买不到：Windows 那一侧的字节是 WTF-8，同一个文件名在两个平台上取出来的字节不保证相同 —— 只承诺同一台机器上取出去的字节原样送回来能再指到同一个名字。
//! 不做归一化（大小写 / NFC-NFD / 分隔符），匹配是字节级的。

/// 一个路径的**原始字节**。
///
/// ⚠ 这里刻意不做成 newtype 包 `Vec<u8>`：本族内部一路传 `&[u8]`，
/// 多一层包装只会多一处「什么时候拆包」的判断，而那正是有损解码溜进来的缝。
pub fn path_bytes(p: &std::path::Path) -> &[u8] {
    p.as_os_str().as_encoded_bytes()
}

// 路径字节的线上两种形（字符串 / `{"b16": …}`）住 `common/path_wire.rs`：文件管理与原生那一块（`dial/terminal.rs`）都要读这一形，
// 两块之间零互相依赖（`files/module_boundary_guard.rs`），共用的只许住 `common/`。
pub use crate::common::path_wire::{from_json, to_json, HEX_KEY};

/// 把一串路径字节还原成能交给标准库的 `PathBuf`。本族唯一一处 `unsafe`：`from_encoded_bytes_unchecked` 要求字节来自同一平台的
/// `as_encoded_bytes`；本族满足它 —— 字节要么由 [`path_bytes`] 在这台机器上取出，要么由 [`from_json`] 从我们自己发出去的那两种形里读回来。
/// 拿外来字节喂它是未定义行为 ⇒ 入方向的路径参数只许从本机发出去的那一份回流；这一条只能靠协议纪律，机器钉不住。
pub fn to_path_buf(bytes: &[u8]) -> std::path::PathBuf {
    // SAFETY: 见上面那段 —— 字节来自同一平台的 `as_encoded_bytes`。
    let os: &std::ffi::OsStr = unsafe { std::ffi::OsStr::from_encoded_bytes_unchecked(bytes) };
    std::path::PathBuf::from(os)
}

/// 一个条目是不是 `dir` 的**直接子项**。
///
/// 口径写死（别读宽）：`entry` 以 `dir` 开头、紧跟一个分隔符、之后**再没有分隔符**。
/// 分隔符两种都认（`/` 与 `\`）—— 这样同一份判词在两个平台上是同一件事。
///
/// ⚠ 它**不解 symlink、不消 `..`**：本族拿到的路径全部由自己的遍历拼出来
/// （从根一层层 `push`），里面不会有 `.` / `..` 段。外来路径不走这条判词。
pub fn is_direct_child(entry: &[u8], dir: &[u8]) -> bool {
    let sep = |c: u8| c == b'/' || c == b'\\';
    if dir.is_empty() || entry.len() <= dir.len() + 1 {
        return false;
    }
    if &entry[..dir.len()] != dir {
        return false;
    }
    if !sep(entry[dir.len()]) {
        return false;
    }
    !entry[dir.len() + 1..].iter().copied().any(sep)
}

#[cfg(test)]
#[path = "../../../tests/backend/files/raw_tests.rs"]
mod tests;
