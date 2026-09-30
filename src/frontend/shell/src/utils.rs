//! 跨模块工具：日期 / 时间换算 + procStart newtype。〔P4〕原子 JSON 写入搬去了 `host_core::atomic_write_json`（两个前端共用的那一份）。
//!
//! `days_from_civil`：〔MOD〕按时间戳挑子 agent 那一份进了后端之后，生产段零读者，只剩文件窗口那份日期换算的异源对拍在用。
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

/// Howard Hinnant 的 days_from_civil：把公历 (y, m, d) 转换为相对 1970-01-01 的天数。
/// 跨月 / 跨年 / 闰年都单调。
///
/// 参考：http://howardhinnant.github.io/date_algorithms.html
///
/// 〔MOD〕生产段今天零读者（按时间戳挑子 agent 那一份进了后端）；留着给文件窗口那份日期换算当**异源**正向
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

/// .NET DateTime.Ticks (自 0001-01-01 Local, 100ns 单位)。
/// Claude Code 写到 sessions/<PID>.json 的 procStart 字段是这个形式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct NetTicks(pub u64);

impl FileTime {
    // 〔P4 · 阶段 H〕Win32 那两件（`from_win32` · `to_net_local_ticks`，都是 `cfg(windows)`）搬进 `platform/filetime.rs`（同一个类型的第二个 impl 块）。
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

// 〔MOD〕`parse_iso8601_ms`〔散文墓碑〕删：唯一调用方（按时间戳挑子 agent 那一份）随「找 ＋ 挑」进了后端
//   （后端 `observe/search_query.rs::parse_iso8601_ms`）。

// === P3：目录扫 + JSON parse → HashMap 通用 helper ===

/// 扫 `dir` 下所有 `*.json` 文件，反序列化为 `T`，按 `key_fn` 提取 key 入 HashMap。
/// 解析失败 / 读失败的文件静默跳过。`dir` 不存在返空 map。
///
/// 替代了当年 `session_map.rs` 扫 pidfile 目录那一处（〔LOC1b〕那份判活已删）+ bind.rs::scan_registry_dir 两处独立实现。
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

/// 把命令字符串编码成 PowerShell `-EncodedCommand` 接受的格式：
/// **UTF-16LE 字节序列的标准 base64**（PowerShell 文档里所谓的 "Unicode" 编码）。
///
/// 用途：安全地把含空格 / 括号 / 引号 / `;` 的 PowerShell 命令透过 `wt.exe` →
/// `powershell.exe` 多层 shell 传递。base64 token 只含 `[A-Za-z0-9+/=]`，**不含**
/// 任何一层 shell 的引号 / 分隔符（wt 用 `;` 分隔多 tab、cmd 用引号配对），因此
/// 不会被任何一层误解析——彻底绕开"多层引号转义地狱"。
pub fn powershell_encoded_command(cmd: &str) -> String {
    let utf16le: Vec<u8> = cmd.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    base64_encode(&utf16le)
}

/// 标准 RFC 4648 base64（含 `=` 填充）。仅 `powershell_encoded_command` 使用，
/// 故不引第三方 crate（实现 ~15 行，且 RFC 测试向量守护）。
fn base64_encode(bytes: &[u8]) -> String {
    const TBL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TBL[((n >> 18) & 63) as usize] as char);
        out.push(TBL[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TBL[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TBL[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/utils_tests.rs"]
mod tests;
