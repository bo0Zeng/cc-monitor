//! 路径原始字节的线上两种形：有效 UTF-8 ⇒ 一个 JSON 字符串；其余 ⇒ `{"b16": "<十六进制>"}`。
//! 两个方向都双向无损（往返恒等判据住 `tests/backend/files/raw_tests.rs`，经 `files::raw` 的再导出跑）。
//! 「路径一路走字节」的那几条纪律（取字节 · 不许有损解码 · 还原 `PathBuf`）住 `files/raw.rs` 头注。

/// 十六进制小写。
fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(DIGITS[(b >> 4) as usize] as char);
        out.push(DIGITS[(b & 0x0f) as usize] as char);
    }
    out
}

fn from_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(s.len() / 2);
    let nib = |c: u8| -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            _ => None,
        }
    };
    let mut i = 0usize;
    while i < b.len() {
        out.push((nib(b[i])? << 4) | nib(b[i + 1])?);
        i += 2;
    }
    Some(out)
}

/// 线上那个键名。**只有一处住址** —— 判据按它对拍。
pub const HEX_KEY: &str = "b16";

/// 把一串路径字节交给线上，双向无损。
/// - 有效 UTF-8 ⇒ 一个 JSON 字符串（常见情形，界面直接能显示）。
/// - 其余 ⇒ `{"b16": "<十六进制>"}`。
/// 不一律走十六进制：回送的是命中，命中数可以上万，一律双倍体积是白花的钱；不一律走字符串：那就是有损解码。
pub fn to_json(bytes: &[u8]) -> serde_json::Value {
    match std::str::from_utf8(bytes) {
        Ok(s) => serde_json::Value::String(s.to_string()),
        Err(_) => {
            let mut m = serde_json::Map::new();
            m.insert(
                HEX_KEY.to_string(),
                serde_json::Value::String(to_hex(bytes)),
            );
            serde_json::Value::Object(m)
        }
    }
}

/// [`to_json`] 的**逆** —— 入方向的路径参数也走这两种形。
///
/// `None` = 这个 JSON 值不是一个路径（形状不对 / 十六进制坏了）。
/// 🔴 **不许在这里「尽力而为」**：猜错一个字节就是去动另一个文件。
pub fn from_json(v: &serde_json::Value) -> Option<Vec<u8>> {
    match v {
        serde_json::Value::String(s) => Some(s.as_bytes().to_vec()),
        serde_json::Value::Object(m) => match m.get(HEX_KEY)? {
            serde_json::Value::String(h) => from_hex(h),
            _ => None,
        },
        _ => None,
    }
}

/// 把一串路径字节还原成能交给标准库的 `PathBuf`。后端这一族唯一一处 `unsafe`：`from_encoded_bytes_unchecked` 要求字节来自同一平台的
/// `as_encoded_bytes`；本族满足它 —— 字节要么由 `files::raw::path_bytes` 在这台机器上取出，要么由 [`from_json`] 从我们自己发出去的那两种形里读回来。
/// 拿外来字节喂它是未定义行为 ⇒ 入方向的路径参数只许从本机发出去的那一份回流；这一条只能靠协议纪律，机器钉不住。
pub fn to_path_buf(bytes: &[u8]) -> std::path::PathBuf {
    // SAFETY: 见上面那段 —— 字节来自同一平台的 `as_encoded_bytes`。
    let os: &std::ffi::OsStr = unsafe { std::ffi::OsStr::from_encoded_bytes_unchecked(bytes) };
    std::path::PathBuf::from(os)
}
