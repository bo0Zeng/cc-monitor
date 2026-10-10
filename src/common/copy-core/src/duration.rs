//! 时长格式化（〔`rules.json` C-W6〕表里只放 `{dur}`，值由这里出）。与取文口分住：单位格是表里的几条键，
//! 取它们的是这里的调用点（取文口那份文件是定义，不算引用）。

use crate::copy_text;

/// 时长 → 给人看的一格（〔`rules.json` C-W6〕表里只放 `{dur}`，值由它出；单位格住表里 `durationFormat.unit.*`）：不满 1 秒写毫秒 · 不满 1 分钟按十分之一秒四舍五入、
/// 整数不带「.0」· 不满 1 小时写「N 分钟」或「N 分 M 秒」· 往上写「N 小时」或「N 小时 M 分」（秒数舍去）。
/// 前端没有它的孪生（已等多久那一格改用短时长 [`short_duration`]，界面那一份随之删）；金样 `tests/__fixtures__/duration-format.golden.json` 只钉这一侧。
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

/// 短时长（文案规范 N5 那一种写法）：`<1s` · `45s` · `6m` · `1h50m` · `2h` · `3d`（不满 1 秒写 `<1s`，满 24 小时只写天）；秒以下舍去、分钟以下舍去。单位格住表里 `durationFormat.short.*`。
/// **会走的钟与距今都只经它写**：会话状态一句里那一截（「后台任务运行中 · make test-all · 12m」）· 已等多久 `waitedText` · 距今 `…RelText`（[`rel_duration`]）；
/// 出口那一侧的读口（桌面 `duration-format.ts::fmtDur` · 手机端 `DurationFormat.short`）与它各对金样 `tests/__fixtures__/short-duration.golden.json`。
pub fn short_duration(ms: u64) -> String {
    let d = ms / 1000;
    if d == 0 {
        return copy_text("durationFormat.short.under", &[]);
    }
    if d < 60 {
        return copy_text("durationFormat.short.sec", &[("n", &d.to_string())]);
    }
    let days = d / (24 * 3600);
    if days > 0 {
        return copy_text("durationFormat.short.day", &[("n", &days.to_string())]);
    }
    let mins = d / 60;
    if mins < 60 {
        return copy_text("durationFormat.short.min", &[("n", &mins.to_string())]);
    }
    let (h, m) = (mins / 60, mins % 60);
    if m == 0 {
        copy_text("durationFormat.short.hour", &[("n", &h.to_string())])
    } else {
        copy_text(
            "durationFormat.short.hourMin",
            &[("h", &h.to_string()), ("m", &m.to_string())],
        )
    }
}

/// 距今（只写将来）：`+12m` · `+1h50m` · `+3d` —— 短时长（[`short_duration`]）向上取整到分钟，前面加 `+`（`durationFormat.rel.ahead`）。
/// 已到 / 已过 ⇒ `None`。
pub fn rel_duration(ms_ahead: i64) -> Option<String> {
    let ms = u64::try_from(ms_ahead).ok().filter(|&m| m > 0)?;
    let up = ms.div_ceil(60_000) * 60_000;
    Some(copy_text(
        "durationFormat.rel.ahead",
        &[("dur", &short_duration(up))],
    ))
}
