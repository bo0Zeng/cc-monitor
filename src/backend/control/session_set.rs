//! **补足一组会话**（帧命令 `set-plan`）：一组 ＝ 几行（目录 ＋ 别名 ＋ 个数），每行数「这台上 · 工作目录就是这一行的目录 ·
//! 用这一行的别名起的 · agent 还在」的，只起差的几个。判定只住这里（[`plan_rows`]，纯函数）；事实由入口交
//! （`faces/session_set_face.rs`：活会话表 · 启动台账里的 `aliases` · 配置文件）。
//!
//! - 「用哪条别名起的」只认启动台账记下的（`ccm @名` 那一发留的便条，`control::launch_account`）；不经别名起的不算进任何一行。
//! - 目录照这台的家目录展开 `~` 再比，末尾的 `/` 不算。
//! - 起不了的那一行（别名不存在 · 别名合不下来 · 目录不存在）照数已有的、不算进「再起」，别的行照起。
//! - 组名：行里写了 `tab-group` 就是它；否则取别名合下来的 `tab-group`（别名没写 ⇒ 别名的名字），这组里同一条别名出现在
//!   不止一个目录 ⇒「分组名 · 目录末一段」。

use crate::assets::aliases::profile::{self, Book, RowEdit};
use cmd_wire::failed::Failed;
use copy_core::copy_text;
use serde::Serialize;

/// 这台此刻活着的一条会话：会话 id · 工作目录 · 经哪条别名起的（台账里没有 ⇒ `None`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Live {
    pub sid: String,
    pub cwd: String,
    pub alias: Option<String>,
}

/// 一行补足的样子。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
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
    /// 这一行写成一段：「目录 组名 已有/要的」，起不了 ⇒「目录 原因」（组菜单那一项悬停侧卡里的一行）。
    pub(crate) said: String,
    /// 「已有 n」。
    pub(crate) have_said: String,
    /// 右侧那一格：「再起 n」· 够了「—」· 起不了 ⇒ 原因。
    pub(crate) more_said: String,
    /// 这一行的个数还能不能再加一（一行上限 · 一组合计上限都没到）。
    pub(crate) can_more: bool,
    /// 不能再加的那一句（能加 ⇒ `null`）。
    pub(crate) more_why: Option<String>,
}

/// 整组补足的样子：每行 ＋ 合计再起几个，连同界面照摆的几句。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct SetPlan {
    pub(crate) rows: Vec<RowPlan>,
    pub(crate) more: u32,
    /// 组菜单那一项的第二行：「N 个目录 · M 个会话」（有起不了的行 ⇒ 再写「K 行起不了」）。
    pub(crate) short: String,
    /// 右侧那一格：「再起 N」· 都够了「已齐」。
    pub(crate) more_said: String,
    /// 预览末行：合计再起几个。
    pub(crate) total_said: String,
    /// 主按钮的字：「启动 N」· 都够了「已齐」。
    pub(crate) start_said: String,
}

/// 「再起 n」· 都够了 ⇒ 整组「已齐」/ 一行「—」。
fn more_said(more: u32, whole: bool) -> String {
    match (more, whole) {
        (0, true) => copy_text("beSessionSet.more.full", &[]),
        (0, false) => copy_text("acct.val.none", &[]),
        _ => copy_text("beSessionSet.more.some", &[("n", &more.to_string())]),
    }
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

/// 别名合下来的标签页分组名（`--tab-group`）；没写 · 合不下来 ⇒ `None`。
fn alias_group(book: &Book, alias: &str) -> Option<String> {
    match profile::resolve(book, alias, &[]).ok()?.parsed {
        crate::ccm_line::argv::Parsed::Opts(o) if !o.tab_group.is_empty() => Some(o.tab_group),
        _ => None,
    }
}

/// 这一行进哪个组（规则见头注）。
fn group_of(r: &RowEdit, rows: &[RowEdit], book: &Book) -> String {
    if let Some(g) = &r.tab_group {
        return g.clone();
    }
    let base = alias_group(book, &r.alias).unwrap_or_else(|| r.alias.clone());
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
pub fn members<'a>(r: &RowEdit, live: &'a [Live], home: Option<&std::path::Path>) -> Vec<&'a Live> {
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
    let total: u32 = rows.iter().map(|r| u32::from(r.n)).sum();
    let out: Vec<RowPlan> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let have = members(r, live, home).len() as u32;
            // 同一目录同一别名的第二行：当场起不了、指着头一行（存的时候配置文件那一侧照旧再判）。
            let key = dir_key(&r.cwd, home);
            let first = rows[..i]
                .iter()
                .position(|o| o.alias == r.alias && dir_key(&o.cwd, home) == key);
            let blocked = match first {
                Some(at) => Some(copy_text(
                    "beSessionSet.row.dup",
                    &[("line", &(at + 1).to_string())],
                )),
                None => blocked_of(r, book, home, is_dir),
            };
            let more_why = if r.n >= profile::ROW_MAX {
                Some(copy_text(
                    "beSessionSet.more.rowMax",
                    &[("max", &profile::ROW_MAX.to_string())],
                ))
            } else if total >= profile::SET_MAX {
                Some(copy_text(
                    "beSessionSet.more.setMax",
                    &[("max", &profile::SET_MAX.to_string())],
                ))
            } else {
                None
            };
            let more = match blocked {
                Some(_) => 0,
                None => u32::from(r.n).saturating_sub(have),
            };
            let group = group_of(r, rows, book);
            let said = match &blocked {
                Some(why) => {
                    copy_text("beSessionSet.row.blocked", &[("cwd", &r.cwd), ("why", why)])
                }
                None => copy_text(
                    "beSessionSet.row.counted",
                    &[
                        ("cwd", &r.cwd),
                        ("group", &group),
                        ("have", &have.to_string()),
                        ("want", &r.n.to_string()),
                    ],
                ),
            };
            RowPlan {
                cwd: r.cwd.clone(),
                alias: r.alias.clone(),
                want: r.n,
                have,
                more,
                group,
                said,
                have_said: copy_text("beSessionSet.row.have", &[("n", &have.to_string())]),
                more_said: blocked.clone().unwrap_or_else(|| more_said(more, false)),
                blocked,
                can_more: more_why.is_none(),
                more_why,
            }
        })
        .collect();
    let more: u32 = out.iter().map(|r| r.more).sum();
    let dirs = {
        let mut d: Vec<String> = rows.iter().map(|r| dir_key(&r.cwd, home)).collect();
        d.sort();
        d.dedup();
        d.len()
    };
    let stuck = out.iter().filter(|r| r.blocked.is_some()).count();
    let (dirs, total_s, stuck_s) = (dirs.to_string(), total.to_string(), stuck.to_string());
    SetPlan {
        short: if stuck == 0 {
            copy_text(
                "beSessionSet.short.all",
                &[("dirs", &dirs), ("n", &total_s)],
            )
        } else {
            copy_text(
                "beSessionSet.short.blocked",
                &[("dirs", &dirs), ("n", &total_s), ("k", &stuck_s)],
            )
        },
        more_said: more_said(more, true),
        total_said: copy_text("beSessionSet.more.total", &[("n", &more.to_string())]),
        start_said: if more > 0 {
            copy_text("beSessionSet.start.some", &[("n", &more.to_string())])
        } else {
            copy_text("beSessionSet.more.full", &[])
        },
        more,
        rows: out,
    }
}

/// `set-plan` 的入参：存好的哪一组（`name`），或框里还没存的那几行（`rows`），两格给一格。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct SetPlanReq {
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub name: Option<String>,
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub rows: Option<Vec<RowEdit>>,
}

crate::wire::typed::codes! {
    /// `set-plan` 的码（入参不对是通用的 `bad_args`）：没有那一组 · 那一组写错或配置文件读不了（那一句说清哪一样）。
    pub enum SetPlanErr {
        /// 这台没有 tmux（一组只起在 tmux 里）。
        NoTmux = "no_tmux",
        NoSuchSet = "no_such_set",
        Refused = "refused",
    }
}

/// `set-plan` 的本体（配置文件 · 活着的会话 · 目录在不在由入口交）。
pub fn answer(
    req: &SetPlanReq,
    book: &Book,
    live: &[Live],
    home: Option<&std::path::Path>,
    is_dir: &dyn Fn(&str) -> bool,
) -> Result<SetPlan, crate::common::said::Said> {
    let rows = match (&req.name, &req.rows) {
        (Some(name), None) => {
            let s = book.find_set(name).ok_or_else(|| {
                crate::common::said::Said::new(
                    "no_such_set",
                    copy_text("beSessionSet.set.unknown", &[("name", name)]),
                )
            })?;
            if let Some(p) = s.problems.first() {
                return Err(crate::common::said::Said::new(
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
            return Err(crate::common::said::Said::new(
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct GroupRef {
    pub id: String,
    pub name: String,
}

/// 一行起的结果：进的组 · 起成的那几条（`session-open` 的回包，带等它报到那一问）· 没起成的那几条 · 起不了的原因。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct RowLaunched {
    pub(crate) cwd: String,
    pub(crate) alias: String,
    pub group: Option<GroupRef>,
    pub(crate) opened: Vec<cmd_wire::session::Opened>,
    /// 没起成的那几条：各是交出去那一形失败（`session-open` 回的码 · 那一句 · 复制详情）。
    pub(crate) failed: Vec<Failed>,
    pub(crate) blocked: Option<String>,
}

/// `set-launch` 的回包：每行 ＋ 起成几条 · 没成几条。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct SetLaunched {
    pub rows: Vec<RowLaunched>,
    pub(crate) started: u32,
    pub(crate) failed: u32,
    /// 起完那一句：全成「已启动 n」· 有没成的「启动 成/全」。
    pub(crate) said: String,
    /// 没成的逐条一行「别名 启动失败 · 原因」（全成 ⇒ 空串）。
    pub(crate) detail: String,
}

/// 起一组要动的那几下（入口交：找 / 建组 · 先定 sid 并写好组注解 · 走 `session-open` 起一条 · 起不成摘掉注解）。
#[expect(
    clippy::type_complexity,
    reason = "注入的那几只手照调用方的签名写在格上；另起类型名只为过这条提示"
)]
pub struct Hands<'a> {
    /// 组名 ⇒ 那一组（为这个组名建过、还在 ⇒ 它；否则新建）。
    pub group_for: &'a mut dyn FnMut(&str) -> Result<GroupRef, Failed>,
    /// 先定这一条的 sid、写好它进这一组的注解；那一家不认先定 sid ⇒ `None`（起好之后再归）。
    pub mark: &'a mut dyn FnMut(&GroupRef) -> Option<String>,
    /// 起这一行的一条（先定的 sid 交进去）。
    pub start:
        &'a mut dyn FnMut(&RowEdit, Option<&str>) -> Result<cmd_wire::session::Opened, Failed>,
    /// 起不成：摘掉先写的那一笔注解。
    pub unmark: &'a mut dyn FnMut(&str, &GroupRef),
}

/// **照补足起**：每行只起 [`RowPlan::more`] 条；起之前那一笔先写好组注解（界面不猜归哪组）；一条不行不挡别的。
/// 同一个组名的几行进同一个组，组只找 / 建一次。
pub fn launch_rows(rows: &[RowEdit], plan: &SetPlan, h: &mut Hands) -> SetLaunched {
    let mut groups: Vec<GroupRef> = Vec::new();
    let mut out = SetLaunched {
        rows: Vec::new(),
        started: 0,
        failed: 0,
        said: String::new(),
        detail: String::new(),
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
                Err(f) => {
                    done.failed.push(f);
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
    out.said = if out.failed == 0 {
        copy_text(
            "beSessionSet.launched.done",
            &[("n", &out.started.to_string())],
        )
    } else {
        copy_text(
            "beSessionSet.launched.partial",
            &[
                ("started", &out.started.to_string()),
                ("all", &(out.started + out.failed).to_string()),
            ],
        )
    };
    out.detail = out
        .rows
        .iter()
        .flat_map(|r| {
            r.failed.iter().map(|f| {
                copy_text(
                    "beSessionSet.launched.fail",
                    &[("alias", &r.alias), ("why", &f.said)],
                )
            })
        })
        .collect::<Vec<_>>()
        .join("\n");
    out
}

/// 组名 ⇒ 为它建的那一组的 id（「为这一行建过的组还在就进它」：人改了显示名、绑定照旧；组解散了，那个 id 不在了，下次新建）。
type Bindings = std::collections::BTreeMap<String, String>;

/// 绑定那一份：状态（读不懂不覆盖；落点 · 上限照声明表）。
const BINDINGS: crate::common::own_state::Doc<Bindings> =
    crate::common::own_state::Doc::of(&relay_route_core::SET_GROUPS_FILE);

/// 绑定那一份的文件名（数据目录底下）。
const BINDINGS_FILE: &str = relay_route_core::file_name_of(BINDINGS.rel);

/// 为组名 `name` 建过的那一组的 id（没有 · 读不出 ⇒ `None`）。
pub fn bound(own: &crate::platform::fs::Own, name: &str) -> Option<String> {
    let file = own.root()?.join(BINDINGS_FILE).ok()?;
    match BINDINGS.read(file.as_path()) {
        crate::common::own_state::Read::Present(b) => b.get(name).cloned(),
        _ => None,
    }
}

/// 记下：组名 `name` 进 `id` 那一组。读不懂那一份 ⇒ 不覆盖、那一句。
pub fn bind(
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
    BINDINGS.write(own, &home, &held, &b)
}

/// `set-launch` 的入参：照存好的哪一组补足、立刻起（补足是幂等的：连点两下不多起）。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct SetLaunchReq {
    pub name: String,
}

#[cfg(test)]
#[path = "../../../tests/backend/control/session_set_tests.rs"]
mod tests;
