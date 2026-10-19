//! 「待办」里记下的选择：点过「不用了」的那几件 · 选了「我自己贴」的那份启动文件 · 首次运行「开始用」那一块点过「跳过」。
//! 住 `~/.cc-monitor/chores.json`（`relay_route_core::CHORES_REL`），后端自己的状态，不是用户数据；全仓唯一的写者是 [`answer_mark`]。
//! 读三态（不在 ＝ 什么都没记 · 读不懂 ＝ 不覆盖、照没记算判，写的那一刻回错）。

use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 那份文件最大多少字节（超了当读不懂）。
pub(crate) const MAX_BYTES: u64 = 64 * 1024;

/// 那一份：状态（读不懂不覆盖）。
const DOC: crate::common::own_state::Doc<Marks> =
    crate::common::own_state::Doc::state(relay_route_core::CHORES_REL, MAX_BYTES);

/// 记下的那几样（盘上那一份，也是 `chores-mark` 的回包：改完记着的整份）。
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "ChoresMarks"
    )
)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Marks {
    /// 点过「不用了」的那几件（件的 `id`）。
    #[serde(default)]
    pub declined: Vec<String>,
    /// 选了「我自己贴」的那份启动文件（绝对路径）；没选 ⇒ `None`。
    #[serde(default)]
    pub self_paste: Option<String>,
    /// 首次运行「开始用」那一块点过「跳过」（`footprint/readiness.rs` 读它）。
    #[serde(default)]
    pub start_skipped: bool,
}

/// 这台机器上那份文件的路径；家目录解析不出来 ⇒ `None`。
pub(crate) fn marks_path() -> Option<PathBuf> {
    Some(crate::platform::paths::home_join(
        &crate::platform::paths::home_dir()?,
        relay_route_core::CHORES_REL,
    ))
}

/// `chores-mark` 的入参：按 `op` 分（多一个键 ⇒ `bad_args`；不带参数的那几种也写成空结构体：内标签枚举的单元变体不拒多出来的键）。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum ChoresMarkReq {
    /// 不用了（`id` 同 `data-report` 的 `todo[].id`）。
    Decline { id: String },
    /// 还是要做。
    Undecline { id: String },
    /// 我自己贴（那一份启动文件，绝对路径）。
    SelfPaste { rc: String },
    /// 改回让 cc-monitor 接上。
    UnselfPaste {},
    /// 首次运行「开始用」那一块跳过。
    SkipStart {},
    /// 撤回跳过。
    UnskipStart {},
}

/// 读一次（三态）。
pub(crate) fn read_at(path: &Path) -> crate::common::own_state::Read<Marks> {
    DOC.read(path)
}

/// 判「待办」用：读不懂的那份照什么都没记算（写的那一刻才回错）。
pub(crate) fn current(path: Option<&Path>) -> Marks {
    match path.map(read_at) {
        Some(crate::common::own_state::Read::Present(m)) => m,
        _ => Marks::default(),
    }
}

/// 改一样（[`ChoresMarkReq`]）⇒ 改完的那一份。
pub(crate) fn mark_at(
    own: &crate::platform::fs::Own,
    req: &ChoresMarkReq,
) -> Result<Marks, crate::wire::spec::Fail> {
    let bad =
        |d: &str| crate::wire::spec::Fail::new("bad_args", crate::common::contract::malformed(d));
    let nonempty = |s: &str, k: &str| {
        if s.is_empty() {
            Err(bad(&format!("`{k}` missing")))
        } else {
            Ok(())
        }
    };
    if let ChoresMarkReq::Decline { id } | ChoresMarkReq::Undecline { id } = req {
        nonempty(id, "id")?;
    }
    if let ChoresMarkReq::SelfPaste { rc } = req {
        nonempty(rc, "rc")?;
    }
    let io = |e: crate::common::said::Said| crate::wire::spec::Fail::from(("io_failed", e));
    let dir = own.home_dir().map_err(|_| {
        crate::wire::spec::Fail::from(("io_failed", copy_text("beChore.marks.noHome", &[])))
    })?;
    let file = own.at_home(DOC.rel).map_err(io)?;
    let path = file.as_path();
    // 只建那一层、建的那一下就是 0700。
    own.ensure_dir(&dir).map_err(|e| {
        (
            "io_failed",
            crate::common::said::Said::with_raw(
                copy_text(
                    "beChore.marks.mkdirFailed",
                    &[
                        ("dir", &dir.as_path().display().to_string()),
                        ("why", &copy_core::io_reason(e.kind())),
                    ],
                ),
                &e,
            ),
        )
    })?;
    let held = own
        .lock(&dir)
        .map_err(|e| io(crate::common::said::Said::from(e)))?;
    let mut m = match read_at(path) {
        crate::common::own_state::Read::Absent => Marks::default(),
        crate::common::own_state::Read::Present(m) => m,
        crate::common::own_state::Read::Unreadable(why) => {
            return Err(crate::wire::spec::Fail::from(("marks_unreadable", why)))
        }
    };
    match req {
        ChoresMarkReq::Decline { id } => {
            if !m.declined.contains(id) {
                m.declined.push(id.clone());
            }
        }
        ChoresMarkReq::Undecline { id } => m.declined.retain(|d| d != id),
        ChoresMarkReq::SelfPaste { rc } => m.self_paste = Some(rc.clone()),
        ChoresMarkReq::UnselfPaste {} => m.self_paste = None,
        ChoresMarkReq::SkipStart {} => m.start_skipped = true,
        ChoresMarkReq::UnskipStart {} => m.start_skipped = false,
    }
    let body = crate::common::own_state::json_body(path, &m).map_err(io)?;
    own.write(&file, &held, &body).map_err(io)?;
    Ok(m)
}

/// `chores-mark`：帧面入口（**写口**，只从 `stream/inbound/` 进）。
pub(crate) fn answer_mark(
    req: &ChoresMarkReq,
    own: &crate::platform::fs::Own,
) -> Result<Marks, crate::wire::spec::Fail> {
    mark_at(own, req)
}

#[cfg(test)]
#[path = "../../../../tests/backend/footprint/chores_marks_tests.rs"]
mod tests;
