//! **补足一组会话**（帧命令 `set-plan`）：一组 ＝ 几行（目录 ＋ 别名 ＋ 个数），每行数「这台上 · 工作目录就是这一行的目录 ·
//! 用这一行的别名起的 · agent 还在」的，只起差的几个。判定只住这里（[`plan_rows`]，纯函数）；事实由入口交
//! （`faces/session_set_face.rs`：活会话表 · 启动台账里的 `aliases` · 配置文件）。
//!
//! - 「用哪条别名起的」只认启动台账记下的（`ccm @名` 那一发留的便条，`control::launch_account`）；不经别名起的不算进任何一行。
//! - 目录照这台的家目录展开 `~` 再比，末尾的 `/` 不算。
//! - 起不了的那一行（别名不存在 · 别名合不下来 · 目录不存在）照数已有的、不算进「再起」，别的行照起。
//! - 组名：行里写了 `tab-group` 就是它；否则取别名的名字，这组里同一条别名出现在不止一个目录 ⇒「别名 · 目录末一段」。

use crate::assets::aliases::profile::{self, Book, RowEdit};
use copy_core::copy_text;
use serde::Serialize;

/// 这台此刻活着的一条会话：会话 id · 工作目录 · 经哪条别名起的（台账里没有 ⇒ `None`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Live {
    pub(crate) sid: String,
    pub(crate) cwd: String,
    pub(crate) alias: Option<String>,
}

/// 一行补足的样子。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct RowPlan {
    pub(crate) cwd: String,
    pub(crate) alias: String,
    /// 要补足到几个。
    pub(crate) want: u8,
    /// 此刻已有几个。
    pub(crate) have: u32,
    /// 这一下再起几个（起不了 ⇒ 0）。
    pub(crate) more: u32,
    /// 起出来的进哪个标签页组。
    pub(crate) group: String,
    /// 这一行起不了的原因（能起 ⇒ `null`）。
    pub(crate) blocked: Option<String>,
}

/// 整组补足的样子：每行 ＋ 合计再起几个。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct SetPlan {
    pub(crate) rows: Vec<RowPlan>,
    pub(crate) more: u32,
}

/// 比目录用的那一形：展开 `~`、去掉末尾的分隔符（根目录本身留着）。
fn dir_key(d: &str, home: Option<&std::path::Path>) -> String {
    let full = crate::platform::paths::expand_tilde_from(d.trim(), home);
    let t = full.trim_end_matches(['/', '\\']);
    if t.is_empty() {
        full
    } else {
        t.to_string()
    }
}

/// 目录末一段（组名缺省用它分开同一条别名的几行）。
fn last_seg(d: &str) -> &str {
    d.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(d)
}

/// 这一行进哪个组（规则见头注）。
fn group_of(r: &RowEdit, rows: &[RowEdit]) -> String {
    if let Some(g) = &r.tab_group {
        return g.clone();
    }
    let base = r.alias.clone();
    if rows.iter().filter(|o| o.alias == r.alias).count() > 1 {
        copy_text(
            "beSessionSet.group.withDir",
            &[("group", &base), ("dir", last_seg(&r.cwd))],
        )
    } else {
        base
    }
}

/// 这一行起不了的原因：别名不存在 · 别名合不下来 · 目录不存在。
fn blocked_of(
    r: &RowEdit,
    book: &Book,
    home: Option<&std::path::Path>,
    is_dir: &dyn Fn(&str) -> bool,
) -> Option<String> {
    if book.find(&r.alias).is_none() {
        return Some(copy_text(
            "beSessionSet.alias.missing",
            &[("alias", &r.alias)],
        ));
    }
    if let Err(e) = profile::resolve(book, &r.alias, &[]) {
        return Some(e);
    }
    let d = crate::platform::paths::expand_tilde_from(r.cwd.trim(), home);
    if !is_dir(&d) {
        return Some(copy_text("beSessionSet.dir.missing", &[("cwd", &r.cwd)]));
    }
    None
}

/// 这一行已有的那几条：目录对得上、用这一行的别名起的。
pub(crate) fn members<'a>(
    r: &RowEdit,
    live: &'a [Live],
    home: Option<&std::path::Path>,
) -> Vec<&'a Live> {
    let key = dir_key(&r.cwd, home);
    live.iter()
        .filter(|l| l.alias.as_deref() == Some(r.alias.as_str()))
        .filter(|l| dir_key(&l.cwd, home) == key)
        .collect()
}

/// **补足的唯一判法**：每行数已有、算再起几个、定组名、说起不了的原因。只读，一个字节不写。
pub(crate) fn plan_rows(
    rows: &[RowEdit],
    book: &Book,
    live: &[Live],
    home: Option<&std::path::Path>,
    is_dir: &dyn Fn(&str) -> bool,
) -> SetPlan {
    let out: Vec<RowPlan> = rows
        .iter()
        .map(|r| {
            let have = members(r, live, home).len() as u32;
            let blocked = blocked_of(r, book, home, is_dir);
            let more = match blocked {
                Some(_) => 0,
                None => u32::from(r.n).saturating_sub(have),
            };
            RowPlan {
                cwd: r.cwd.clone(),
                alias: r.alias.clone(),
                want: r.n,
                have,
                more,
                group: group_of(r, rows),
                blocked,
            }
        })
        .collect();
    SetPlan {
        more: out.iter().map(|r| r.more).sum(),
        rows: out,
    }
}

/// `set-plan` 的入参：存好的哪一组（`name`），或框里还没存的那几行（`rows`），两格给一格。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct SetPlanReq {
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) name: Option<String>,
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) rows: Option<Vec<RowEdit>>,
}

crate::wire::typed::codes! {
    /// `set-plan` 的码（入参不对是通用的 `bad_args`）：没有那一组 · 那一组写错或配置文件读不了（那一句说清哪一样）。
    pub(crate) enum SetPlanErr {
        /// 这台没有 tmux（一组只起在 tmux 里）。
        NoTmux = "no_tmux",
        NoSuchSet = "no_such_set",
        Refused = "refused",
    }
}

/// `set-plan` 的本体（配置文件 · 活着的会话 · 目录在不在由入口交）。
pub(crate) fn answer(
    req: &SetPlanReq,
    book: &Book,
    live: &[Live],
    home: Option<&std::path::Path>,
    is_dir: &dyn Fn(&str) -> bool,
) -> Result<SetPlan, (&'static str, String)> {
    let rows = match (&req.name, &req.rows) {
        (Some(name), None) => {
            let s = book.find_set(name).ok_or_else(|| {
                (
                    "no_such_set",
                    copy_text("beSessionSet.set.unknown", &[("name", name)]),
                )
            })?;
            if let Some(p) = s.problems.first() {
                return Err((
                    "refused",
                    copy_text(
                        "beSessionSet.set.broken",
                        &[("name", name), ("why", &p.message)],
                    ),
                ));
            }
            profile::edit_of_set(s).rows
        }
        (None, Some(rows)) => rows.clone(),
        _ => {
            return Err((
                "bad_args",
                crate::common::contract::malformed(
                    "`set-plan` takes exactly one of `name` and `rows`",
                ),
            ))
        }
    };
    Ok(plan_rows(&rows, book, live, home, is_dir))
}

// ───────────────────────── 一点就起：set-launch ─────────────────────────

/// 一行起出来的会话进的那一组：组 id ＋ 名字。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct GroupRef {
    pub(crate) id: String,
    pub(crate) name: String,
}

/// 一条没起成：`session-open` 回的码 ＋ 那一句。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct LaunchFail {
    pub(crate) code: String,
    pub(crate) message: String,
}

/// 一行起的结果：进的组 · 起成的那几条（`session-open` 的回包，带等它报到那一问）· 没起成的那几条 · 起不了的原因。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct RowLaunched {
    pub(crate) cwd: String,
    pub(crate) alias: String,
    pub(crate) group: Option<GroupRef>,
    pub(crate) opened: Vec<cmd_wire::session::Opened>,
    pub(crate) failed: Vec<LaunchFail>,
    pub(crate) blocked: Option<String>,
}

/// `set-launch` 的回包：每行 ＋ 起成几条 · 没成几条。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct SetLaunched {
    pub(crate) rows: Vec<RowLaunched>,
    pub(crate) started: u32,
    pub(crate) failed: u32,
}

/// 起一组要动的那几下（入口交：找 / 建组 · 先定 sid 并写好组注解 · 走 `session-open` 起一条 · 起不成摘掉注解）。
pub(crate) struct Hands<'a> {
    /// 组名 ⇒ 那一组（为这个组名建过、还在 ⇒ 它；否则新建）。
    pub(crate) group_for: &'a mut dyn FnMut(&str) -> Result<GroupRef, String>,
    /// 先定这一条的 sid、写好它进这一组的注解；那一家不认先定 sid ⇒ `None`（起好之后再归）。
    pub(crate) mark: &'a mut dyn FnMut(&GroupRef) -> Option<String>,
    /// 起这一行的一条（先定的 sid 交进去）。
    pub(crate) start:
        &'a mut dyn FnMut(&RowEdit, Option<&str>) -> Result<cmd_wire::session::Opened, LaunchFail>,
    /// 起不成：摘掉先写的那一笔注解。
    pub(crate) unmark: &'a mut dyn FnMut(&str, &GroupRef),
}

/// **照补足起**：每行只起 [`RowPlan::more`] 条；起之前那一笔先写好组注解（界面不猜归哪组）；一条不行不挡别的。
/// 同一个组名的几行进同一个组，组只找 / 建一次。
pub(crate) fn launch_rows(rows: &[RowEdit], plan: &SetPlan, h: &mut Hands) -> SetLaunched {
    let mut groups: Vec<GroupRef> = Vec::new();
    let mut out = SetLaunched {
        rows: Vec::new(),
        started: 0,
        failed: 0,
    };
    for (r, p) in rows.iter().zip(&plan.rows) {
        let mut done = RowLaunched {
            cwd: r.cwd.clone(),
            alias: r.alias.clone(),
            group: None,
            opened: Vec::new(),
            failed: Vec::new(),
            blocked: p.blocked.clone(),
        };
        if p.more > 0 {
            let group = match groups.iter().find(|g| g.name == p.group) {
                Some(g) => Ok(g.clone()),
                None => (h.group_for)(&p.group).inspect(|g| groups.push(g.clone())),
            };
            match group {
                Err(why) => {
                    done.failed.push(LaunchFail {
                        code: "refused".into(),
                        message: why,
                    });
                    out.failed += p.more;
                }
                Ok(g) => {
                    for _ in 0..p.more {
                        let sid = (h.mark)(&g);
                        match (h.start)(r, sid.as_deref()) {
                            Ok(o) => {
                                done.opened.push(o);
                                out.started += 1;
                            }
                            Err(f) => {
                                if let Some(s) = &sid {
                                    (h.unmark)(s, &g);
                                }
                                done.failed.push(f);
                                out.failed += 1;
                            }
                        }
                    }
                    done.group = Some(g);
                }
            }
        }
        out.rows.push(done);
    }
    out
}

/// 组名 ⇒ 为它建的那一组的 id（「为这一行建过的组还在就进它」：人改了显示名、绑定照旧；组解散了，那个 id 不在了，下次新建）。
type Bindings = std::collections::BTreeMap<String, String>;

/// 绑定那一份：状态（读不懂不覆盖）。
const BINDINGS: crate::common::own_state::Doc<Bindings> =
    crate::common::own_state::Doc::state(".cc-monitor/set-groups.json", 1 << 20);

/// 绑定那一份的文件名（数据目录底下）。
const BINDINGS_FILE: &str = "set-groups.json";

/// 为组名 `name` 建过的那一组的 id（没有 · 读不出 ⇒ `None`）。
pub(crate) fn bound(own: &crate::platform::fs::Own, name: &str) -> Option<String> {
    let file = own.root()?.join(BINDINGS_FILE).ok()?;
    match BINDINGS.read(file.as_path()) {
        crate::common::own_state::Read::Present(b) => b.get(name).cloned(),
        _ => None,
    }
}

/// 记下：组名 `name` 进 `id` 那一组。读不懂那一份 ⇒ 不覆盖、那一句。
pub(crate) fn bind(
    own: &crate::platform::fs::Own,
    name: &str,
    id: &str,
) -> Result<(), crate::common::said::Said> {
    let home = own
        .root()
        .ok_or_else(|| crate::common::said::Said::from(copy_text("beOwnState.path.noHome", &[])))?;
    let file = home.join(BINDINGS_FILE)?;
    let held = own.lock(&home)?;
    let mut b = match BINDINGS.read(file.as_path()) {
        crate::common::own_state::Read::Absent => Bindings::new(),
        crate::common::own_state::Read::Present(b) => b,
        crate::common::own_state::Read::Unreadable(why) => return Err(why),
    };
    b.insert(name.to_string(), id.to_string());
    let body = crate::common::own_state::json_body(file.as_path(), &b)?;
    own.write(&file, &held, &body)
}

/// `set-launch` 的入参：照存好的哪一组补足、立刻起（补足是幂等的：连点两下不多起）。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct SetLaunchReq {
    pub(crate) name: String,
}

#[cfg(test)]
#[path = "../../../tests/backend/control/session_set_tests.rs"]
mod tests;
