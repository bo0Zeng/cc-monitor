//! 一份策略的设置（`轮换-v5-分层.md` §2.1）：号与先后（＝ 默认优先级）· 默认上限 · 默认上限类型 · 每个号另设的上限 / 类型 / 启用延迟 / 回切阈值。
//! 上限按一天里的分段 `{from, to, v}`（分钟，`v` 0–100）；没写到的钟点 ＝ 100%。钟点按这份策略的时区。

use serde::{Deserialize, Serialize};

/// 一段上限。`from` ≤ 分钟 < `to`（`to` 至多 1440）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(export, export_to = "../../frontend/ui/generated/", rename = "CapSeg")
)]
pub struct Seg {
    pub from: u16,
    pub to: u16,
    pub v: u8,
}

/// 两窗各一组分段；某窗缺 ⇒ 跟总体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct Caps {
    #[serde(rename = "5h", default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub w5h: Option<Vec<Seg>>,
    #[serde(rename = "7d", default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub w7d: Option<Vec<Seg>>,
}

impl Caps {
    fn get(&self, w: usize) -> Option<&[Seg]> {
        if w == 0 {
            self.w5h.as_deref()
        } else {
            self.w7d.as_deref()
        }
    }
}

/// 上限类型。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum CapKind {
    /// 还有别的号能用时照这条限；所有号都到上限以后可以超过。
    #[default]
    Soft,
    /// 任何时候都不超过。
    Hard,
}

/// 号列表里的一行：`{"start": true}`（开会话用的号）或 `{"id": …}`，外加这个号另设的几样。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "SettingsRow"
    )
)]
pub struct Row {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(test, ts(optional, as = "Option<bool>"))]
    pub start: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "is_default_caps")]
    pub caps: Caps,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub cap_kind: Option<CapKind>,
    /// 启用延迟（分钟）。
    #[serde(default, skip_serializing_if = "is_zero")]
    #[cfg_attr(test, ts(optional, as = "Option<u32>"))]
    pub delay: u32,
    /// 回切阈值（%）；缺 ＝ 不回切。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub back: Option<u8>,
}

fn is_default_caps(c: &Caps) -> bool {
    c == &Caps::default()
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

/// 一份策略的设置。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct Settings {
    /// 默认上限（缺的窗 ＝ 100%）。
    #[serde(default)]
    pub caps: Caps,
    #[serde(default)]
    pub cap_kind: CapKind,
    pub accounts: Vec<Row>,
}

/// 设置 ＋ 时区（判要的那一份）：时区由策略文件头带来，缺 ⇒ UTC。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Rules {
    pub(crate) s: Settings,
    pub(crate) tz: crate::Tz,
}

impl Rules {
    pub(crate) fn utc(s: Settings) -> Self {
        Self {
            s,
            tz: crate::Tz::default(),
        }
    }

    /// 号列表（照先后），`start` 那一行换成开会话的号；重复的只留头一个。
    pub(crate) fn list(&self, start: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for r in &self.s.accounts {
            let id = if r.start {
                start.to_string()
            } else {
                r.id.clone().unwrap_or_default()
            };
            if !id.is_empty() && !out.contains(&id) {
                out.push(id);
            }
        }
        out
    }

    /// 这个号那一行（`start` 那一行认开会话的号）。
    pub(crate) fn row(&self, start: &str, a: &str) -> Option<&Row> {
        self.s.accounts.iter().find(|r| {
            if r.start {
                a == start
            } else {
                r.id.as_deref() == Some(a)
            }
        })
    }

    /// 时刻 `t` 是本地钟一天里的第几分钟。
    pub(crate) fn minute(&self, t: u64) -> u16 {
        let t = i64::try_from(t).unwrap_or(i64::MAX);
        let local = i128::from(t) + i128::from(self.tz.offset_secs(t));
        u16::try_from(local.rem_euclid(86_400) / 60).unwrap_or(0)
    }

    fn segs(&self, start: &str, a: &str, w: usize) -> &[Seg] {
        self.row(start, a)
            .and_then(|r| r.caps.get(w))
            .or_else(|| self.s.caps.get(w))
            .unwrap_or(&[])
    }

    /// ★ `C(a, w, t)`：号另设的分段 → 没另设取默认；落不进任何一段 ⇒ 100。
    pub(crate) fn cap(&self, start: &str, a: &str, w: usize, t: u64) -> f64 {
        let m = self.minute(t);
        self.segs(start, a, w)
            .iter()
            .find(|s| s.from <= m && m < s.to)
            .map_or(100.0, |s| f64::from(s.v))
    }

    /// 这个号的上限类型（号另设 → 默认）。
    pub(crate) fn kind(&self, start: &str, a: &str) -> CapKind {
        self.row(start, a)
            .and_then(|r| r.cap_kind)
            .unwrap_or(self.s.cap_kind)
    }

    /// 启用延迟（秒）。
    pub(crate) fn delay_secs(&self, start: &str, a: &str) -> u64 {
        self.row(start, a).map_or(0, |r| u64::from(r.delay) * 60)
    }

    /// 回切阈值（%）。
    pub(crate) fn back(&self, start: &str, a: &str) -> Option<f64> {
        self.row(start, a).and_then(|r| r.back).map(f64::from)
    }

    /// `t` 之后这个号（两窗）分段的下一个边界时刻（没有分段 ⇒ 空）。
    pub(crate) fn next_edge(&self, start: &str, a: &str, t: u64) -> Option<u64> {
        let m = u64::from(self.minute(t));
        let into = t % 60;
        let mut best: Option<u64> = None;
        for w in 0..2 {
            for s in self.segs(start, a, w) {
                for e in [s.from, s.to] {
                    let e = u64::from(e) % 1440;
                    // 距下一个 e 分钟还有几分钟（e == m 且已过整分 ⇒ 明天那一个）。
                    let mut d = (e + 1440 - m) % 1440;
                    if d == 0 && into > 0 {
                        d = 1440;
                    }
                    if d == 0 {
                        continue;
                    }
                    let at = t - into + d * 60;
                    best = Some(best.map_or(at, |b: u64| b.min(at)));
                }
            }
        }
        best
    }
}
