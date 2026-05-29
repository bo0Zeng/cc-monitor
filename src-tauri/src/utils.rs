//! 跨模块工具：日期 / 时间换算 + 原子 JSON 写入 + procStart newtype。
//!
//! `days_from_civil` 在 `subagent`（按 timestamp 关联 subagent meta）
//! 与 `history`（jsonl ISO8601 时间戳排序）两处都用，提取至此避免重复。
//!
//! ## procStart newtype（P1.1）
//!
//! 项目里 "procStart" 这一概念有**两种独立的时间表示**散落在不同来源，
//! 之前 String / u64 不分让"哪种语义"全靠注释和约定俗成。任何把
//! `SessionInfo.proc_start` (NetTicks) 当 `HwndEntry.owner_proc_start`
//! (FileTime) 比较的代码都是 silent bug。引入 `NetTicks` / `FileTime`
//! newtype 后这类混用编译期就 catch。
//!
//! - `FileTime(u64)` = Win32 FILETIME（自 1601-01-01 UTC，100ns 单位）
//!     来源：Rust 端 `GetProcessTimes`、PS 端 `[Process].StartTime.ToFileTime()`
//! - `NetTicks(u64)` = .NET DateTime.Ticks（自 0001-01-01 Local，100ns 单位）
//!     来源：Claude Code 写到 `sessions/<PID>.json` 的 `procStart` 字段
//!
//! 两者数值差 504_911_232_000_000_000（NET 从 0001-01-01 起到 1601-01-01
//! 的 ticks 数）+ 当地时区偏移。`FileTime::to_net_local_ticks()` 做这步转换。
//!
//! `atomic_write_json` 是 monitor 自写 data dir 内 JSON 文件（不是用户 profile）
//! 的统一原子写入入口。早期 `bind.rs` / `history.rs` / `auto_launch.rs` 三处各
//! 自手写 `write(tmp) + remove + rename` 三步非原子，crash 即丢；统一走本 helper
//! 后 Windows 上走 `ReplaceFileW`，非 Windows 走 `std::fs::rename`，全程原子。
//!
//! **作用范围限于** `~/.claude/claudecode-frontend/` 下 monitor 自己产物
//! （详 IPC-PROTOCOL.md § 通用约束）。用户文件（PowerShell profile）必须仍走
//! `profile_installer` 自己的 backup + 写后校验路径，详 INVARIANTS § 4。

/// Howard Hinnant 的 days_from_civil：把公历 (y, m, d) 转换为相对 1970-01-01 的天数。
/// 跨月 / 跨年 / 闰年都单调。
///
/// 参考：http://howardhinnant.github.io/date_algorithms.html
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 }; // Mar=0..Feb=11
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe - 719468
}

/// 原子 JSON 写入。Windows 走 `ReplaceFileW`（dst 不存在时 fallback 到 rename），
/// 非 Windows 走 `std::fs::rename`。失败时清理 tmp。
///
/// 设计权衡：本 helper 不做 backup / 写后校验（那是 profile_installer 的职责）；
/// 这里只解决"原子覆盖"问题，避免 `remove + rename` 中间 crash 丢文件。
pub fn atomic_write_json<T: serde::Serialize>(
    path: &std::path::Path,
    value: &T,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_string_pretty(value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let fname = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "out.json".to_string());
    let tmp = path.with_file_name(format!("{fname}.ccm-tmp-{ms}-{}", std::process::id()));
    std::fs::write(&tmp, body)?;
    let r = atomic_replace_path(&tmp, path);
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    r
}

#[cfg(windows)]
fn atomic_replace_path(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{ReplaceFileW, REPLACEFILE_WRITE_THROUGH};

    let to_wide = |p: &std::path::Path| -> Vec<u16> {
        p.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };
    let src_w = to_wide(src);
    let dst_w = to_wide(dst);

    if !dst.exists() {
        // dst 不存在 ReplaceFileW 会失败；首次写直接 rename
        return std::fs::rename(src, dst);
    }

    unsafe {
        ReplaceFileW(
            PCWSTR(dst_w.as_ptr()),
            PCWSTR(src_w.as_ptr()),
            PCWSTR::null(),
            REPLACEFILE_WRITE_THROUGH,
            None,
            None,
        )
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.message().to_string()))
    }
}

#[cfg(not(windows))]
fn atomic_replace_path(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::rename(src, dst)
}

/// Win32 FILETIME (自 1601-01-01 UTC, 100ns 单位)。
/// Rust 端 GetProcessTimes / PS 端 `[Process].StartTime.ToFileTime()` 都给这个。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileTime(pub u64);

/// .NET DateTime.Ticks (自 0001-01-01 Local, 100ns 单位)。
/// Claude Code 写到 sessions/<PID>.json 的 procStart 字段是这个形式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct NetTicks(pub u64);

impl FileTime {
    /// 从 Win32 FILETIME struct 提取 u64。
    #[cfg(windows)]
    pub fn from_win32(ft: &windows::Win32::Foundation::FILETIME) -> Self {
        Self(((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64))
    }

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

    /// FILETIME UTC → .NET Local Ticks。
    /// Claude Code 写的 procStart 是 .NET 形式；要跟它比较时必须先转。
    /// Windows-only —— 用 `FileTimeToLocalFileTime` 修当地时区偏移。
    #[cfg(windows)]
    pub fn to_net_local_ticks(self) -> NetTicks {
        use windows::Win32::Foundation::FILETIME;
        /// 从 .NET 0001-01-01 起到 Win32 1601-01-01 之间的 100ns 数。
        const NET_EPOCH_TO_WIN32_FILETIME_TICKS: u64 = 504_911_232_000_000_000;

        #[link(name = "kernel32")]
        extern "system" {
            fn FileTimeToLocalFileTime(
                lpFileTime: *const FILETIME,
                lpLocalFileTime: *mut FILETIME,
            ) -> i32;
        }

        let utc = FILETIME {
            dwLowDateTime: (self.0 & 0xFFFF_FFFF) as u32,
            dwHighDateTime: (self.0 >> 32) as u32,
        };
        let mut local = FILETIME::default();
        let ok = unsafe { FileTimeToLocalFileTime(&utc, &mut local) };
        if ok == 0 {
            // 极罕见 — 退化为 UTC + 偏移（仍然单调但跟 Claude 比有时区差）
            return NetTicks(self.0 + NET_EPOCH_TO_WIN32_FILETIME_TICKS);
        }
        let local_u64 = ((local.dwHighDateTime as u64) << 32) | (local.dwLowDateTime as u64);
        NetTicks(local_u64 + NET_EPOCH_TO_WIN32_FILETIME_TICKS)
    }
}

impl NetTicks {
    /// 从字符串解析（Claude Code procStart 字段形式）。失败返 None。
    pub fn parse_str(s: &str) -> Option<Self> {
        s.parse::<u64>().ok().map(Self)
    }

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

/// 解析 ISO 8601 字符串到 unix ms。失败返 None。仅用于排序 / 显示。
///
/// 形如 `2026-05-20T15:11:42[.fff]Z` 或带时区 `+HH:MM`（时区被忽略——按 UTC 算）。
/// frac 自动归一到 ms：3 位 → 原值，>3 位 → 除以 10^(n-3)，<3 位 → 乘 10^(3-n)。
/// 用 `days_from_civil` 保证跨月跨年单调（原 `(y*12+m)*31+d` 月末有 bug）。
pub fn parse_iso8601_ms(s: &str) -> Option<i64> {
    let bytes = s.as_bytes();
    if bytes.len() < 19 {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    let month: u32 = s.get(5..7)?.parse().ok()?;
    let day: u32 = s.get(8..10)?.parse().ok()?;
    let hour: u32 = s.get(11..13)?.parse().ok()?;
    let min: u32 = s.get(14..16)?.parse().ok()?;
    let sec: u32 = s.get(17..19)?.parse().ok()?;
    let mut ms: i64 = 0;
    if s.len() > 19 && bytes[19] == b'.' {
        let mut end = 20;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        let frac = s.get(20..end)?;
        let frac_num: i64 = frac.parse().ok()?;
        ms = match frac.len() {
            0 => 0,
            1 => frac_num * 100,
            2 => frac_num * 10,
            3 => frac_num,
            n if n > 3 => frac_num / 10_i64.pow((n - 3) as u32),
            _ => 0,
        };
    }
    let days = days_from_civil(year, month as i64, day as i64);
    let total = days * 86_400 + hour as i64 * 3600 + min as i64 * 60 + sec as i64;
    Some(total * 1000 + ms)
}

// === P3：目录扫 + JSON parse → HashMap 通用 helper ===

/// 扫 `dir` 下所有 `*.json` 文件，反序列化为 `T`，按 `key_fn` 提取 key 入 HashMap。
/// 解析失败 / 读失败的文件静默跳过。`dir` 不存在返空 map。
///
/// 替代了 session_map.rs::scan_dir + bind.rs::scan_registry_dir 两处独立实现。
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
mod tests {
    use super::*;

    #[test]
    fn epoch_is_zero() {
        // 1970-01-01 = 0 days since itself
        assert_eq!(days_from_civil(1970, 1, 1), 0);
    }

    #[test]
    fn monotonic_across_month() {
        let jan_31 = days_from_civil(2026, 1, 31);
        let feb_1 = days_from_civil(2026, 2, 1);
        assert_eq!(feb_1 - jan_31, 1);
    }

    #[test]
    fn monotonic_across_year() {
        let dec_31 = days_from_civil(2025, 12, 31);
        let jan_1 = days_from_civil(2026, 1, 1);
        assert_eq!(jan_1 - dec_31, 1);
    }

    #[test]
    fn leap_year_feb_29() {
        // 2024 是闰年，Feb 有 29 天
        let feb_28 = days_from_civil(2024, 2, 28);
        let feb_29 = days_from_civil(2024, 2, 29);
        let mar_1 = days_from_civil(2024, 3, 1);
        assert_eq!(feb_29 - feb_28, 1);
        assert_eq!(mar_1 - feb_29, 1);
    }

    #[test]
    fn atomic_write_json_first_write_creates_file() {
        let tmp = std::env::temp_dir().join(format!(
            "ccm-utils-test-first-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_file(&tmp);
        let v = serde_json::json!({ "a": 1, "b": "hi" });
        atomic_write_json(&tmp, &v).unwrap();
        let on_disk: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&tmp).unwrap()).unwrap();
        assert_eq!(on_disk["a"], 1);
        assert_eq!(on_disk["b"], "hi");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn atomic_write_json_replace_keeps_content() {
        let tmp = std::env::temp_dir().join(format!(
            "ccm-utils-test-replace-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        atomic_write_json(&tmp, &serde_json::json!({ "v": 1 })).unwrap();
        atomic_write_json(&tmp, &serde_json::json!({ "v": 2, "extra": "y" })).unwrap();
        let on_disk: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&tmp).unwrap()).unwrap();
        assert_eq!(on_disk["v"], 2);
        assert_eq!(on_disk["extra"], "y");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn iso_parse_simple() {
        let ms = parse_iso8601_ms("2026-05-20T15:11:42.345Z").unwrap();
        assert!(ms > 1_700_000_000_000);
        assert!(ms < 2_000_000_000_000);
    }

    #[test]
    fn iso_parse_no_fraction() {
        let ms = parse_iso8601_ms("2026-05-20T15:11:42Z").unwrap();
        assert!(ms > 1_700_000_000_000);
    }

    #[test]
    fn iso_parse_bad_returns_none() {
        assert!(parse_iso8601_ms("not-a-date").is_none());
    }

    #[test]
    fn iso_parse_short_frac_normalizes() {
        // 1 位 frac → 乘 100 to ms
        let a = parse_iso8601_ms("2026-05-20T00:00:00.5Z").unwrap();
        let b = parse_iso8601_ms("2026-05-20T00:00:00.500Z").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn iso_parse_long_frac_truncates() {
        // 6 位 frac → 除 1000 to ms（剥 us）
        let a = parse_iso8601_ms("2026-05-20T00:00:00.123456Z").unwrap();
        let b = parse_iso8601_ms("2026-05-20T00:00:00.123Z").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn now_ms_increases_monotonically() {
        let a = now_ms();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = now_ms();
        assert!(b >= a);
    }

    #[test]
    fn scan_dir_jsons_empty_when_missing() {
        let dir = std::env::temp_dir().join(format!("ccm-scan-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let m: std::collections::HashMap<String, serde_json::Value> =
            scan_dir_jsons(&dir, |v: &serde_json::Value| {
                v.get("k")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string()
            });
        assert!(m.is_empty());
    }

    #[test]
    fn scan_dir_jsons_parses_jsons_only() {
        let dir = std::env::temp_dir().join(format!(
            "ccm-scan-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.json"), r#"{"k":"one"}"#).unwrap();
        std::fs::write(dir.join("b.json"), r#"{"k":"two"}"#).unwrap();
        std::fs::write(dir.join("ignored.txt"), "not json").unwrap();
        std::fs::write(dir.join("broken.json"), "not valid json").unwrap();
        let m: std::collections::HashMap<String, serde_json::Value> =
            scan_dir_jsons(&dir, |v: &serde_json::Value| {
                v.get("k")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string()
            });
        assert_eq!(m.len(), 2);
        assert!(m.contains_key("one"));
        assert!(m.contains_key("two"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_write_json_no_stray_tmp() {
        // 写完 dst 父目录里不应该有任何 ccm-tmp-* 残留
        let dir =
            std::env::temp_dir().join(format!("ccm-utils-test-tmpcheck-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dst = dir.join("a.json");
        let _ = std::fs::remove_file(&dst);
        atomic_write_json(&dst, &serde_json::json!({ "k": 1 })).unwrap();
        let stray = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().contains(".ccm-tmp-"));
        assert!(!stray, "ccm-tmp- 残留未清理");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
