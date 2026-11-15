//! **漂移账的通用形**（跟哪一家无关）：各家把「看不懂的记录」记在自己的账本里（Claude Code 那一份住
//! `claudecode/drift.rs`，Codex 那一份住 `codex/record.rs`），注册表 [`super::RecordFace::drift`] 交出来的是这一形；帧命令 `drift-report` 把各家的面并成一份。
//!
//! 计数 · 封顶 · 空键 · 快照排序只在这里（[`DriftLedger`]）；各家只交面（枚举 ＋ 线上词，[`DriftFaceWord`]），各开一本账。
//! 面的名字是那一家自己的词（`unknown_record_type` …），通用层不认，原样带着。
//! 也收得进来（`Deserialize`）：本机后端经常驻流问到别的台那一份（诊断信息 `diagnostics-report`）。

/// 一个键的记账。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct DriftEntry {
    /// 看不懂的那个值（记录 type / kind / status / token）。
    pub(crate) key: String,
    /// 见过多少次（本进程内的计数，不落盘）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub(crate) count: u64,
}

/// 一个面的快照。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct DriftFaceReport {
    /// 那一家的面的名字（那一家自己的词）。
    pub face: String,
    /// 按 `count` 降序、同数按键名升序（键数触顶后新键并进溢出键 [`OVERFLOW_KEY`]）。
    pub entries: Vec<DriftEntry>,
}

/// `drift-report` 的应答：注册表里各家的面并在一起（注册序；不记的那一家没有面）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct DriftReport {
    pub faces: Vec<DriftFaceReport>,
    /// 机器页那一格（「会话流 ×n · 已记入诊断信息 · 不影响显示」，n ＝ 各面加起来几个键；一个都没有 ⇒「无」），出口照抄。
    pub said: String,
}

impl DriftReport {
    /// 各家的面并好 ⇒ 应答（那一句只在这里写）。
    pub fn of(faces: Vec<DriftFaceReport>) -> Self {
        let n: usize = faces.iter().map(|f| f.entries.len()).sum();
        let said = if n > 0 {
            copy_core::copy_text("backend.cc.driftSome", &[("n", &n.to_string())])
        } else {
            copy_core::copy_text("hostKey.compare.none", &[])
        };
        Self { faces, said }
    }
}

/// 一家的面：说得出自己在线上叫什么（`face` 那一格）。
pub(crate) trait DriftFaceWord: Ord + Copy {
    fn word(self) -> &'static str;
}

/// 每一面最多记多少个不同的键；再多的并进 [`OVERFLOW_KEY`]。
pub(crate) const MAX_KEYS: usize = 64;
/// 键数触顶之后的新键并进的那一个。
pub(crate) const OVERFLOW_KEY: &str = "<overflow>";
/// 空键记成这一个（空串当键在诊断面上是一行空白，看不出是哪一类）。
pub(crate) const EMPTY_KEY: &str = "<empty>";

/// 一家的账本：面 → 键 → 记账。各家各开一本（一个 `static Mutex`）；本进程内计数，不落盘。
/// 断言整表形状的判据在局部账本上跑：各家的全局账本会被任何跑过解析的判据写进去；碰全局那一本的只断言「我那几条」。
pub(crate) struct DriftLedger<F> {
    faces: std::collections::BTreeMap<F, std::collections::BTreeMap<String, DriftEntry>>,
}

impl<F: DriftFaceWord> DriftLedger<F> {
    pub(crate) const fn new() -> Self {
        Self {
            faces: std::collections::BTreeMap::new(),
        }
    }

    /// 拿一家那一本（`static Mutex`）来记 · 读。毒化了也继续用：诊断面绝不能因为它 panic 而拖垮解析热路径。
    pub(crate) fn lock(
        book: &'static std::sync::Mutex<Self>,
    ) -> std::sync::MutexGuard<'static, Self> {
        book.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 记一笔。
    pub(crate) fn record(&mut self, face: F, key: &str) {
        let key = if key.is_empty() { EMPTY_KEY } else { key };
        let per = self.faces.entry(face).or_default();
        let key = if per.len() >= MAX_KEYS && !per.contains_key(key) {
            OVERFLOW_KEY
        } else {
            key
        };
        per.entry(key.to_string())
            .or_insert_with(|| DriftEntry {
                key: key.to_string(),
                count: 0,
            })
            .count += 1;
    }

    /// 只读快照：每一面按次数降序、同数按键名升序（诊断面不许每次刷新都跳）。
    /// 同数的不另比键名：条目从 `BTreeMap` 取出已按键名升序，`sort_by_key` 稳定，同数的留在键序上。
    pub(crate) fn snapshot(&self) -> Vec<DriftFaceReport> {
        self.faces
            .iter()
            .map(|(face, per)| {
                let mut entries: Vec<DriftEntry> = per.values().cloned().collect();
                entries.sort_by_key(|e| std::cmp::Reverse(e.count));
                DriftFaceReport {
                    face: face.word().to_string(),
                    entries,
                }
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/agents/drift_tests.rs"]
mod tests;
