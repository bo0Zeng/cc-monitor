//! 线上送回来的路径字节 → 能交给标准库的 `PathBuf`。线上那两种形（字符串 / `{"b16": …}`）本身住 `cmd_wire::files::WirePath`。
//! 「路径一路走字节」的那几条纪律（取字节 · 不许有损解码 · 还原 `PathBuf`）住 `files/raw.rs` 头注。

/// 把一串路径字节还原成能交给标准库的 `PathBuf`。后端这一族唯一一处 `unsafe`：`from_encoded_bytes_unchecked` 要求字节来自同一平台的
/// `as_encoded_bytes`；本族满足它 —— 字节要么由 `files::raw::path_bytes` 在这台机器上取出，要么由 `WirePath` 从我们自己发出去的那两种形里读回来。
/// 拿外来字节喂它是未定义行为 ⇒ 入方向的路径参数只许从本机发出去的那一份回流；这一条只能靠协议纪律，机器钉不住。
pub fn to_path_buf(bytes: &[u8]) -> std::path::PathBuf {
    // SAFETY: 见上面那段 —— 字节来自同一平台的 `as_encoded_bytes`。
    let os: &std::ffi::OsStr = unsafe { std::ffi::OsStr::from_encoded_bytes_unchecked(bytes) };
    std::path::PathBuf::from(os)
}
