//! 钥匙屏一行的第二行（keyLine）：照那把公钥（OpenSSH 一行）写「算法 · SHA256:前4…后2」。
//! 整串指纹是拨号核心那一份（`ssh_dial::known_hosts::fingerprint`，导入 · 生成时存的也是它），这里只切、配上算法名。
//! Kotlin 只剩 `Core.keyLine` 那一个 JNI 口，照摆。

use jni::objects::{JClass, JString};
use jni::sys::jstring;
use jni::JNIEnv;
use russh::keys::ssh_key::{Algorithm, EcdsaCurve};

use crate::{guarded, new_jstring, string};

/// 算法给人看的名字；不在这几样里的照 ssh 名。
fn algorithm_name(a: &Algorithm) -> String {
    match a {
        Algorithm::Ed25519 => "Ed25519".into(),
        Algorithm::Ecdsa { curve } => match curve {
            EcdsaCurve::NistP256 => "ECDSA P-256".into(),
            EcdsaCurve::NistP384 => "ECDSA P-384".into(),
            EcdsaCurve::NistP521 => "ECDSA P-521".into(),
        },
        Algorithm::Rsa { .. } => "RSA".into(),
        other => other.as_str().to_string(),
    }
}

/// 整串 `SHA256:<b64>` ⇒ `SHA256:<前 4>…<后 2>`；太短的不切。
fn short_fingerprint(full: &str) -> String {
    let (head, body) = full.split_once(':').unwrap_or(("", full));
    let chars: Vec<char> = body.chars().collect();
    if chars.len() <= 6 {
        return full.to_string();
    }
    let front: String = chars[..4].iter().collect();
    let back: String = chars[chars.len() - 2..].iter().collect();
    format!("{head}:{front}…{back}")
}

/// 一行 OpenSSH 公钥 ⇒ keyLine；读不出 ⇒ 「公钥无法读取」那一句。
pub(crate) fn key_line(openssh: &str) -> String {
    match russh::keys::PublicKey::from_openssh(openssh.trim()) {
        Ok(key) => format!(
            "{}{}{}",
            algorithm_name(&key.algorithm()),
            copy_core::copy_text("kit.text.sep", &[]),
            short_fingerprint(&ssh_dial::known_hosts::fingerprint(&key)),
        ),
        Err(_) => copy_core::copy_text("mobIdentity.row.unreadable", &[]),
    }
}

/// `Core.keyLine(publicKeyOpenSsh)`：恒回一句（读不出也是一句）。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_keyLine<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    line: JString<'l>,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        let line = string(&mut env, &line).unwrap_or_default();
        new_jstring(&env, key_line(&line))
    })
}

#[cfg(test)]
#[path = "../../../../tests/mobile/core-rs/key_line_tests.rs"]
mod tests;
