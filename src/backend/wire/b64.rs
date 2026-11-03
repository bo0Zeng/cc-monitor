//! CLI 面入参（`--args-b64`）的那一种编码：base64 本身住 `cmd_wire::b64`（全仓一份），这里只把解不开的那几种写成契约错那一句。

pub use cmd_wire::b64::b64_encode;
use cmd_wire::b64::B64Error;

/// [`b64_encode`] 的逆，严格（长度 · 补位 · 字母表外的字 ⇒ `Err`，坏的就是坏的，不猜）；`Err` 是契约错那一句。
pub fn b64_decode(text: &str) -> Result<Vec<u8>, String> {
    cmd_wire::b64::b64_decode(text).map_err(|e| {
        crate::common::contract::malformed(&match e {
            B64Error::Length(n) => format!("base64 length {n} is not a multiple of 4"),
            B64Error::Padding => "base64 padding is not at the end".to_string(),
            B64Error::Char(c) => format!("base64 has a character outside the alphabet: {c:?}"),
        })
    })
}
