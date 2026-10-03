//! 跨模块工具：日期 / 时间换算 + procStart newtype。原子 JSON 写入搬去了 `host_core::atomic_write_json`（两个前端共用的那一份）。
//!
//! `days_from_civil`：按时间戳挑子 agent 那一份进了后端之后，生产段零读者，只剩文件窗口那份日期换算的异源对拍在用。
//!
//! ## procStart newtype（P1.1）
//!
//! `FileTime(u64)` = Win32 FILETIME（自 1601-01-01 UTC，100ns 单位）；
//! 来源：Rust 端 `GetProcessTimes`、PS 端 `[Process].StartTime.ToFileTime()`。
//! Claude Code 在 `sessions/<PID>.json` 里写的 `procStart` 是另一种单位（.NET 本地 ticks，自 0001-01-01）；
//! monitor 今天不读那个字段，原先为它立的那个 newtype 与换算一个调用方都没有，删了。

/// Howard Hinnant 的 days_from_civil：把公历 (y, m, d) 转换为相对 1970-01-01 的天数。
/// 跨月 / 跨年 / 闰年都单调。
///
/// 参考：http://howardhinnant.github.io/date_algorithms.html
///
/// 生产段今天零读者（按时间戳挑子 agent 那一份进了后端）；留着给文件窗口那份日期换算当**异源**正向
/// （`filewin/source_tests.rs`，它是 `filewin/source.rs` 那份逆运算的对拍）。
#[cfg_attr(not(test), allow(dead_code))]
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 }; // Mar=0..Feb=11
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe - 719468
}

/// Win32 FILETIME (自 1601-01-01 UTC, 100ns 单位)。
/// Rust 端 GetProcessTimes / PS 端 `[Process].StartTime.ToFileTime()` 都给这个。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileTime(pub u64);

impl FileTime {
    // Win32 那一件（`from_win32`，`cfg(windows)`）住 `platform/filetime.rs`（同一个类型的第二个 impl 块）。
    /// 从字符串解析（PS 端 ToFileTime() 输出形式）。失败返 None。
    /// 保留未用：将来若给 `verify_binding` 加 ps_proc_start 校验 / 合并
    /// `HwndEntry` 跟 `SidHwndBinding` 时即用。
    #[allow(dead_code)]
    pub fn parse_str(s: &str) -> Option<Self> {
        s.parse::<u64>().ok().map(Self)
    }

    #[allow(dead_code)]
    pub fn abs_diff(self, other: Self) -> u64 {
        self.0.abs_diff(other.0)
    }
}

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

// `parse_iso8601_ms`〔散文墓碑〕删：唯一调用方（按时间戳挑子 agent 那一份）随「找 ＋ 挑」进了后端
//   （后端 `observe/search_query.rs::parse_iso8601_ms`）。

// === P3：目录扫 + JSON parse → HashMap 通用 helper ===

/// 扫 `dir` 下所有 `*.json` 文件，反序列化为 `T`，按 `key_fn` 提取 key 入 HashMap。
/// 解析失败 / 读失败的文件静默跳过。`dir` 不存在返空 map。
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
            if let Ok(s) = std::fs::read_to_string(&p) {
                if let Ok(value) = serde_json::from_str::<T>(&s) {
                    out.insert(key_fn(&value), value);
                }
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/utils_tests.rs"]
mod tests;
