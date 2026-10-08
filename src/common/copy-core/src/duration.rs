//! 时长格式化（〔`rules.json` C-W6〕表里只放 `{dur}`，值由这里出）。与取文口分住：单位格是表里的几条键，
//! 取它们的是这里的调用点（取文口那份文件是定义，不算引用）。

use crate::copy_text;

/// 时长 → 给人看的一格（〔`rules.json` C-W6〕表里只放 `{dur}`，值由它出；单位格住表里 `durationFormat.unit.*`）：不满 1 秒写毫秒 · 不满 1 分钟按十分之一秒四舍五入、
/// 整数不带「.0」· 不满 1 小时写「N 分钟」或「N 分 M 秒」· 往上写「N 小时」或「N 小时 M 分」（秒数舍去）。
/// 与前端 `copy-table.ts::formatDuration` 同形（两侧各对金样 `tests/__fixtures__/duration-format.golden.json`）。
pub fn format_duration(ms: u64) -> String {
    if ms < 1000 {
        return copy_text("durationFormat.unit.ms", &[("n", &ms.to_string())]);
    }
    let tenths = (ms + 50) / 100;
    if tenths < 600 {
        let n = if tenths % 10 == 0 {
            (tenths / 10).to_string()
        } else {
            format!("{}.{}", tenths / 10, tenths % 10)
        };
        return copy_text("durationFormat.unit.sec", &[("n", &n)]);
    }
    let s = (ms + 500) / 1000;
    if s < 3600 {
        return if s % 60 == 0 {
            copy_text("durationFormat.unit.min", &[("n", &(s / 60).to_string())])
        } else {
            copy_text(
                "durationFormat.unit.minSec",
                &[("m", &(s / 60).to_string()), ("s", &(s % 60).to_string())],
            )
        };
    }
    let h = (s / 3600).to_string();
    let m = (s % 3600) / 60;
    if m == 0 {
        copy_text("durationFormat.unit.hour", &[("n", &h)])
    } else {
        copy_text(
            "durationFormat.unit.hourMin",
            &[("h", &h), ("m", &m.to_string())],
        )
    }
}

/// [`format_duration`] 的 `Duration` 入口（毫秒截到 `u64`）。
pub fn format_elapsed(d: std::time::Duration) -> String {
    format_duration(u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}
