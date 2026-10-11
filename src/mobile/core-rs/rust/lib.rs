//! **手机嵌的 Rust 核心**（`libmobcore.so`）：只有 FFI 外壳。拨号 · 鉴权 · 复用 · 用法是后端同一份拨号核心（`ssh-dial`）。
//!
//! 边界是**手写 JNI、窄口、走消息**：帧本来就是一行一条 JSON，口只开几个，回调两个（吐事件 · Keystore 签名）。
//! 不用 UniFFI / JNA（运行时多一份 JNA、两 ABI 多 2.2 MB，量过）。
//!
//! 进程里一份 tokio 多线程运行时（两个 worker，第一次用时建）；主线程上不阻塞。每个口在边界上接住 panic
//! （`panic = "unwind"`：嵌在 App 进程里，panic 不许 abort 整个 App），接住了就回一句「核心出错」给 Kotlin。

use jni::objects::JClass;
use jni::sys::jstring;
use jni::JNIEnv;

/// 这一份核心的身份那一行（JSON）：拨号核心的 ack 协议版本 ＋ 认得的用法。Kotlin 侧冒烟与「装上的是不是这一份」用它。
pub fn version_json() -> String {
    serde_json::json!({
        "ack_v": ssh_dial::ACK_V,
        "uses": ssh_dial::USES,
    })
    .to_string()
}

/// `Core.version()`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_version<'l>(
    env: JNIEnv<'l>,
    _class: JClass<'l>,
) -> jstring {
    let text = std::panic::catch_unwind(version_json).unwrap_or_else(|_| "{}".to_string());
    env.new_string(text)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[cfg(test)]
mod tests {
    /// 身份那一行是 JSON、带拨号核心的 ack 协议版本与它认得的用法（PTY 那一种也在）。
    #[test]
    fn the_version_line_names_the_dial_core_it_carries() {
        let v: serde_json::Value = serde_json::from_str(&super::version_json()).expect("不是 JSON");
        assert_eq!(v["ack_v"], ssh_dial::ACK_V);
        assert!(v["uses"]
            .as_array()
            .is_some_and(|u| u.iter().any(|x| x == "shell")));
    }
}
