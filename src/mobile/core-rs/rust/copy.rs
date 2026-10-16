//! 手机写字的那几样（取文 ＋ 插值 · 大小 · 短时长 · 距今）直接调 `copy-core`，Kotlin 只剩 `CopyCore` 那几个 JNI 口。

use jni::objects::{JClass, JObjectArray, JString};
use jni::sys::{jlong, jstring};
use jni::JNIEnv;

use crate::{guarded, string};

/// 取文：参数按「名 · 值 · 名 · 值 …」平铺交进来。这里是转发（key 不是字面量），不是引用。
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

fn out(env: &JNIEnv, s: Option<String>) -> jstring {
    s.and_then(|s| env.new_string(s).ok())
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

fn strings(env: &mut JNIEnv, arr: &JObjectArray) -> Vec<String> {
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

/// `CopyCore.text(key, args)`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_copy_CopyCore_text<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    key: JString<'l>,
    args: JObjectArray<'l>,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        let key = string(&mut env, &key).unwrap_or_default();
        let flat = strings(&mut env, &args);
        out(&env, Some(text(&key, &flat)))
    })
}

/// `CopyCore.sizeText(bytes)`：负数按 0。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_copy_CopyCore_sizeText<'l>(
    env: JNIEnv<'l>,
    _class: JClass<'l>,
    bytes: jlong,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        out(&env, Some(copy_core::size_text(ms(bytes))))
    })
}

/// `CopyCore.shortDuration(ms)`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_copy_CopyCore_shortDuration<'l>(
    env: JNIEnv<'l>,
    _class: JClass<'l>,
    v: jlong,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        out(&env, Some(copy_core::short_duration(ms(v))))
    })
}

/// `CopyCore.relDuration(aheadMs)`：已到 ⇒ `null`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_copy_CopyCore_relDuration<'l>(
    env: JNIEnv<'l>,
    _class: JClass<'l>,
    ahead: jlong,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        out(&env, copy_core::rel_duration(ahead))
    })
}
