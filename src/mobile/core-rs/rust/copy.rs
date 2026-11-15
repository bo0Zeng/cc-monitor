//! 手机写字的那几样（取文 ＋ 插值 · 短时长；大小由后端写好交来）直接调 `copy-core`，Kotlin 那一侧是 `Core.copyTextFlat` · `Core.shortDuration`。

use jni::objects::{JClass, JObjectArray, JString};
use jni::sys::{jlong, jstring};
use jni::JNIEnv;

use crate::{guarded, string};

/// 取文：参数按「名 · 值 · 名 · 值 …」平铺交进来。
fn text(key: &str, flat: &[String]) -> String {
    let args: Vec<(&str, &str)> = flat
        .chunks_exact(2)
        .map(|p| (p[0].as_str(), p[1].as_str()))
        .collect();
    copy_core::copy_text(key, &args)
}

/// 负的毫秒按 0。
fn ms(v: jlong) -> u64 {
    u64::try_from(v).unwrap_or(0)
}

fn out(env: &JNIEnv, s: String) -> jstring {
    env.new_string(s)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

pub(crate) fn strings(env: &mut JNIEnv, arr: &JObjectArray) -> Vec<String> {
    let n = env.get_array_length(arr).unwrap_or(0);
    (0..n)
        .map(|i| {
            env.get_object_array_element(arr, i)
                .ok()
                .and_then(|o| string(env, &JString::from(o)))
                .unwrap_or_default()
        })
        .collect()
}

/// `Core.copyTextFlat(key, args)`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_copyTextFlat<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    key: JString<'l>,
    args: JObjectArray<'l>,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        let key = string(&mut env, &key).unwrap_or_default();
        let flat = strings(&mut env, &args);
        out(&env, text(&key, &flat))
    })
}

/// `Core.shortDuration(ms)`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_shortDuration<'l>(
    env: JNIEnv<'l>,
    _class: JClass<'l>,
    v: jlong,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        out(&env, copy_core::short_duration(ms(v)))
    })
}
