//! 计划「要你看」的认可与「退回」的记录：住 `~/.cc-monitor/plan-review.json`（`relay_route_core::PLAN_REVIEW_REL`），
//! 后端自己的状态，不是用户数据 —— 计划仓（`.planned-build/`）一个字节都不写，认可不回写 pb。
//!
//! - 认可：`工作区 ⇒ 片 ⇒ [键]`。键带条目的版本（[`super::needs`]），版本换了就对不上、那一条再出；
//!   每次写只留那一片此刻还在的键（过了版本的顺手清掉）。
//! - 退回：`工作区 ⇒ 片 ⇒ 格 ⇒ 那一次`（送给谁 · 送没送到 · 当时的子格与正文摘要），同一格后退回的盖掉先前的。
//!
//! 读三态（不在 ＝ 什么都没记 · 读不懂 ＝ 不覆盖、照没记算，写的那一刻回错）；读—改—写在跨进程锁里、原子写。
//! 全仓唯一的写口是本模块的 `answer_*` 三个：认可两个只从 `faces/plan_review_face.rs` 进，记退回那一个只从 `faces/plan_return_face.rs` 进。

use super::needs::{self, Returned};
use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// 那份文件最大多少字节（超了当读不懂）。
pub(crate) const MAX_BYTES: u64 = 1024 * 1024;

/// 记下的两样。
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Review {
    /// 工作区 ⇒ 片 ⇒ 认可过的键。
    #[serde(default)]
    acks: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    /// 工作区 ⇒ 片 ⇒ 格 ⇒ 最近那一次退回。
    #[serde(default)]
    returns: BTreeMap<String, BTreeMap<String, BTreeMap<String, Returned>>>,
}

impl Review {
    /// 这一条认可过没有。
    pub(crate) fn acked(&self, ws: &str, slice: &str, key: &str) -> bool {
        self.acks
            .get(ws)
            .and_then(|s| s.get(slice))
            .is_some_and(|k| k.iter().any(|x| x == key))
    }

    /// 顶块那一条认可时记着的 rev（进程刚起、还不知道顶块哪次进的那一步时先用它）。
    pub(crate) fn prior(&self, ws: &str, slice: &str) -> Option<String> {
        self.acks
            .get(ws)?
            .get(slice)?
            .iter()
            .rev()
            .find_map(|k| k.strip_prefix("top:").map(str::to_string))
    }

    /// 这一格最近那一次退回。
    pub(crate) fn returned(&self, ws: &str, slice: &str, cell: &str) -> Option<&Returned> {
        self.returns.get(ws)?.get(slice)?.get(cell)
    }

    /// 给一份成品（[`super::book::Book`] 出的）标上：每条要你看认可没有（`acked`）· 每片与整份的要你看数（`needCount`）·
    /// 退回过的格此刻的状态（`returned`：[`needs::landing`]；没退回过 ⇒ `null`；顶块那一次记在片上）。
    pub(crate) fn annotate(&self, doc: &mut Value) {
        let ws = doc
            .get("workspace")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        needs::mark_acked(doc, &|slice, key| self.acked(&ws, slice, key));
        let mut total = 0;
        if let Some(slices) = doc.get_mut("slices").and_then(Value::as_array_mut) {
            for sl in slices {
                let n = needs::count_slice(sl);
                total += n;
                sl["needCount"] = json!(n);
                let name = sl
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let cells: Vec<Value> = sl
                    .get("cells")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                // 顶块（`project`）退回过 ⇒ 那一片顶上带它此刻落没落地（与格同一判法）。
                let top = match (
                    self.returned(&ws, &name, needs::TOP_BLOCK),
                    needs::target(sl, needs::TOP_BLOCK),
                ) {
                    (Some(r), Some(t)) => needs::landing(r, &t, &cells),
                    _ => Value::Null,
                };
                sl["returned"] = top;
                if let Some(arr) = sl.get_mut("cells").and_then(Value::as_array_mut) {
                    for c in arr {
                        let id = c.get("id").and_then(Value::as_str).unwrap_or_default();
                        let st = self
                            .returned(&ws, &name, id)
                            .map_or(Value::Null, |r| needs::landing(r, c, &cells));
                        c["returned"] = st;
                    }
                }
            }
        }
        doc["needCount"] = json!(total);
    }
}

/// 这台机器上那份文件的路径；家目录解析不出来 ⇒ `None`。
pub(crate) fn review_path() -> Option<PathBuf> {
    Some(crate::platform::paths::home_dir()?.join(relay_route_core::PLAN_REVIEW_REL))
}

/// 读一次（三态）。
pub(crate) fn read_at(path: &Path) -> crate::common::own_state::Read<Review> {
    crate::common::own_state::read_json(path, MAX_BYTES)
}

/// 标成品用：读不懂的那份照什么都没记算（写的那一刻才回错）。
pub(crate) fn current(path: Option<&Path>) -> Review {
    match path.map(read_at) {
        Some(crate::common::own_state::Read::Present(r)) => r,
        _ => Review::default(),
    }
}

type Err = crate::stream::inbound::spec::Fail;

/// 读—改—写一次（跨进程锁里；读不懂的不覆盖）。
fn edit_at(path: &Path, f: impl FnOnce(&mut Review)) -> Result<(), Err> {
    let dir = path.parent().ok_or_else(|| {
        Err::new(
            "bad_args",
            crate::common::contract::malformed("review path has no parent"),
        )
    })?;
    crate::common::own_dir::ensure_private_dir(dir).map_err(|e| {
        Err::from((
            "io_failed",
            crate::common::said::Said::with_raw(
                copy_text(
                    "bePlan.review.mkdirFailed",
                    &[("dir", &dir.display().to_string())],
                ),
                e,
            ),
        ))
    })?;
    let _g = crate::platform::lock::hold(dir)
        .map_err(|e| Err::from(("io_failed", crate::common::said::Said::from(e))))?;
    let mut r = match read_at(path) {
        crate::common::own_state::Read::Absent => Review::default(),
        crate::common::own_state::Read::Present(r) => r,
        crate::common::own_state::Read::Unreadable(why) => {
            return Err(Err::from(("review_unreadable", why)))
        }
    };
    f(&mut r);
    crate::common::own_state::write_json(path, &r).map_err(|e| Err::from(("io_failed", e)))
}

/// 认可一条：那一片只留此刻还在的键（`live`）＋ 这一条。
pub(crate) fn ack_at(
    path: &Path,
    ws: &str,
    slice: &str,
    key: &str,
    live: &[String],
) -> Result<(), Err> {
    edit_at(path, |r| {
        let keys = r
            .acks
            .entry(ws.to_string())
            .or_default()
            .entry(slice.to_string())
            .or_default();
        keys.retain(|k| live.contains(k) && k != key);
        keys.push(key.to_string());
    })
}

/// 撤掉一条认可（没有也不算错）。
pub(crate) fn unack_at(path: &Path, ws: &str, slice: &str, key: &str) -> Result<(), Err> {
    edit_at(path, |r| {
        if let Some(s) = r.acks.get_mut(ws) {
            if let Some(k) = s.get_mut(slice) {
                k.retain(|x| x != key);
                if k.is_empty() {
                    s.remove(slice);
                }
            }
            if s.is_empty() {
                r.acks.remove(ws);
            }
        }
    })
}

/// 记一次退回（盖掉那一格先前那一次）。
pub(crate) fn returned_at(
    path: &Path,
    ws: &str,
    slice: &str,
    cell: &str,
    rec: Returned,
) -> Result<(), Err> {
    edit_at(path, |r| {
        r.returns
            .entry(ws.to_string())
            .or_default()
            .entry(slice.to_string())
            .or_default()
            .insert(cell.to_string(), rec);
    })
}

fn path_or_err() -> Result<PathBuf, Err> {
    review_path().ok_or_else(|| Err::new("io_failed", copy_text("bePlan.review.noHome", &[])))
}

/// `plan-ack` 的写口（**只从 `faces/plan_review_face.rs` 进**）。
pub(crate) fn answer_ack(ws: &str, slice: &str, key: &str, live: &[String]) -> Result<(), Err> {
    ack_at(&path_or_err()?, ws, slice, key, live)
}

/// `plan-unack` 的写口（同上）。
pub(crate) fn answer_unack(ws: &str, slice: &str, key: &str) -> Result<(), Err> {
    unack_at(&path_or_err()?, ws, slice, key)
}

/// `plan-return` 送出去之后记那一次的写口（**只从 `faces/plan_return_face.rs` 进**）。
pub(crate) fn answer_returned(ws: &str, slice: &str, cell: &str, rec: Returned) -> Result<(), Err> {
    returned_at(&path_or_err()?, ws, slice, cell, rec)
}

#[cfg(test)]
#[path = "../../../tests/backend/plan/review_tests.rs"]
mod tests;
