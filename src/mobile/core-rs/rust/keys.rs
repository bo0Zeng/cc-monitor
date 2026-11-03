//! 手机送进终端的字节：键条上的几下 · 软键盘带修饰的那一下 · 旧转义按钮迁移，都调 `cmd_wire::keys` 那一张表（后端同一张）。
//! Kotlin 只剩 `TermKeys` 那两个 JNI 口；一下在两侧之间是记号（`cmd_wire::keys::Press::token`）。

use cmd_wire::keys::{presses_bytes, presses_of, Press};
use jni::objects::{JClass, JObject, JObjectArray, JString};
use jni::sys::{jbyteArray, jobjectArray};
use jni::JNIEnv;

use crate::copy::strings;
use crate::{guarded, string};

/// 几下的记号 ⇒ 字节；有一个记号读不出 ⇒ `None`。
fn bytes_of(tokens: &[String]) -> Option<Vec<u8>> {
    let presses: Vec<Press> = tokens.iter().map(|t| Press::from_token(t)).collect::<Option<_>>()?;
    Some(presses_bytes(&presses))
}

/// 旧按钮的转义写法 ⇒ 字节：`\e` · `\n` · `\r` · `\t` · `\\` · `\xNN`；认不得的 `\X` 反斜杠照字面。
fn legacy_bytes(seq: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(seq.len());
    let mut rest = seq;
    while let Some(at) = rest.find('\\') {
        out.extend_from_slice(rest[..at].as_bytes());
        let after = &rest[at + 1..];
        let simple = match after.chars().next() {
            Some('e') => Some(0x1b),
            Some('n') => Some(b'\n'),
            Some('r') => Some(b'\r'),
            Some('t') => Some(b'\t'),
            Some('\\') => Some(b'\\'),
            _ => None,
        };
        let hex = after
            .strip_prefix('x')
            .and_then(|h| h.get(..2))
            .filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()))
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        rest = match (simple, hex) {
            (Some(b), _) => {
                out.push(b);
                &after[1..]
            }
            (None, Some(b)) => {
                out.push(b);
                &after[3..]
            }
            (None, None) => {
                out.push(b'\\');
                after
            }
        };
    }
    out.extend_from_slice(rest.as_bytes());
    out
}

/// 旧按钮的内容 ⇒ 几下的记号；认不出 ⇒ `None`。
fn legacy_tokens(seq: &str) -> Option<Vec<String>> {
    presses_of(&legacy_bytes(seq)).map(|ps| ps.iter().map(Press::token).collect())
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

/// `TermKeys.legacyPresses(seq)`：认不出 ⇒ `null`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_TermKeys_legacyPresses<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    seq: JString<'l>,
) -> jobjectArray {
    guarded(std::ptr::null_mut(), || {
        let Some(tokens) = string(&mut env, &seq).and_then(|s| legacy_tokens(&s)) else {
            return std::ptr::null_mut();
        };
        let made = (|| -> jni::errors::Result<JObjectArray<'l>> {
            let len = i32::try_from(tokens.len()).unwrap_or(i32::MAX);
            let arr = env.new_object_array(len, "java/lang/String", JObject::null())?;
            for (i, t) in tokens.iter().enumerate() {
                let s = env.new_string(t)?;
                env.set_object_array_element(&arr, i32::try_from(i).unwrap_or(i32::MAX), s)?;
            }
            Ok(arr)
        })();
        made.map(|a| a.into_raw()).unwrap_or(std::ptr::null_mut())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 旧写法的几种转义 · 认不得的反斜杠照字面 · `\x` 后面不够两位十六进制照字面。
    #[test]
    fn legacy_escapes_turn_into_bytes() {
        assert_eq!(legacy_bytes(r"\e[A"), [0x1b, b'[', b'A']);
        assert_eq!(legacy_bytes(r"ls\n"), b"ls\n");
        assert_eq!(legacy_bytes(r"\r\t\\"), b"\r\t\\");
        assert_eq!(legacy_bytes(r"\x1b[5~"), [0x1b, b'[', b'5', b'~']);
        assert_eq!(legacy_bytes(r"\q"), br"\q");
        assert_eq!(legacy_bytes(r"\x1"), br"\x1");
        assert_eq!(legacy_bytes(r"\xg0"), br"\xg0");
        assert_eq!(legacy_bytes("中\\e"), "中\x1b".as_bytes());
        assert_eq!(legacy_bytes("\\"), b"\\");
    }

    /// 旧按钮 ⇒ 几下；认不出 ⇒ `None`。
    #[test]
    fn legacy_buttons_turn_into_presses() {
        assert_eq!(legacy_tokens(r"\e[1;5F"), Some(vec!["sctrl:end".to_string()]));
        assert_eq!(legacy_tokens(r"git status\n"), Some(vec!["tgit status".to_string(), "s:enter".to_string()]));
        assert_eq!(legacy_tokens(r"\e[?1049h"), None);
    }

    /// 记号读不出一个 ⇒ 整串 `None`；读得出 ⇒ 一下接一下拼。
    #[test]
    fn tokens_turn_into_bytes() {
        assert_eq!(bytes_of(&["s:up".into(), "tab".into()]), Some(b"\x1b[Aab".to_vec()));
        assert_eq!(bytes_of(&["s:nope".into()]), None);
    }
}
