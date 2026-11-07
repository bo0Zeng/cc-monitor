//! 手机送进终端的字节：键条上的几下 · 软键盘带修饰的那一下，都调 `cmd_wire::keys` 那一张表（后端同一张）。
//! Kotlin 只剩 `TermKeys.bytes` 那一个 JNI 口；一下在两侧之间是记号（`cmd_wire::keys::Press::token`）。

use cmd_wire::keys::{presses_bytes, Press};
use jni::objects::{JClass, JObjectArray};
use jni::sys::jbyteArray;
use jni::JNIEnv;

use crate::copy::strings;
use crate::guarded;

/// 几下的记号 ⇒ 字节；有一个记号读不出 ⇒ `None`。
fn bytes_of(tokens: &[String]) -> Option<Vec<u8>> {
    let presses: Vec<Press> = tokens
        .iter()
        .map(|t| Press::from_token(t))
        .collect::<Option<_>>()?;
    Some(presses_bytes(&presses))
}

/// `TermKeys.bytes(tokens)`：有一个记号读不出 ⇒ `null`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_TermKeys_bytes<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    tokens: JObjectArray<'l>,
) -> jbyteArray {
    guarded(std::ptr::null_mut(), || {
        let tokens = strings(&mut env, &tokens);
        bytes_of(&tokens)
            .and_then(|b| env.byte_array_from_slice(&b).ok())
            .map(|a| a.into_raw())
            .unwrap_or(std::ptr::null_mut())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 记号读不出一个 ⇒ 整串 `None`；读得出 ⇒ 一下接一下拼。
    #[test]
    fn tokens_turn_into_bytes() {
        assert_eq!(
            bytes_of(&["s:up".into(), "tab".into()]),
            Some(b"\x1b[Aab".to_vec())
        );
        assert_eq!(bytes_of(&["s:nope".into()]), None);
    }
}
