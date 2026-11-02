//! 跨模块工具：时间换算。原子 JSON 写入与天数 ⇒ 公历在 `host_core`（两个前端共用的那一份）。

// === P3：时间换算（归并 history / subagent / bind 三处独立实现） ===

/// SystemTime → unix ms（i64）。失败返 0。
pub fn systime_to_ms(t: std::time::SystemTime) -> i64 {
    t.duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 当前 unix ms。
pub fn now_ms() -> i64 {
    systime_to_ms(std::time::SystemTime::now())
}

// === P3：目录扫 + JSON parse → HashMap 通用 helper ===

/// 扫 `dir` 下所有 `*.json` 文件，反序列化为 `T`，按 `key_fn` 提取 key 入 HashMap。
/// 解析失败 / 读失败的文件静默跳过；超过 [`SCAN_ENTRY_CAP`] 的跳过、日志说是哪一份。`dir` 不存在返空 map。
///
/// 替代了当年 `session_map.rs` 扫 pidfile 目录那一处（那份判活已删）+ bind.rs::scan_registry_dir 两处独立实现。
pub fn scan_dir_jsons<T, K, F>(dir: &std::path::Path, key_fn: F) -> std::collections::HashMap<K, T>
where
    T: serde::de::DeserializeOwned,
    K: std::hash::Hash + Eq,
    F: Fn(&T) -> K,
{
    let mut out = std::collections::HashMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().map_or(false, |e| e == "json") {
            match host_core::read_capped(&p, SCAN_ENTRY_CAP) {
                Ok(host_core::Capped::Whole(s)) => {
                    if let Ok(value) = serde_json::from_str::<T>(&s) {
                        out.insert(key_fn(&value), value);
                    }
                }
                Ok(host_core::Capped::TooBig) => tracing::warn!(
                    "{} 超过读上限 {} 字节，跳过",
                    p.display(),
                    SCAN_ENTRY_CAP.bytes
                ),
                Ok(host_core::Capped::Absent) | Err(_) => {}
            }
        }
    }
    out
}

/// [`scan_dir_jsons`] 读一份的上限：扫的是握手表（`ps-registry/<进程号>.json`，几格整数与一个窗口记号，实测两三百字节）。
/// 超了那一份跳过、日志说是哪一份（那个 shell 下一次登记时重写）。
const SCAN_ENTRY_CAP: relay_route_core::Cap = relay_route_core::Cap {
    bytes: 16 * 1024,
    over: relay_route_core::Over::SkipAndSay,
};

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/utils_tests.rs"]
mod tests;
