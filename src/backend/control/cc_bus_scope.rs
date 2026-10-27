//! cc-bus 的范围：组（A13-4）。`bus-envelope` 核完各格之后在这里判这一封送不送得到，规矩只在后端这一处，脚本不判。
//!
//! - 发件人在几组 ⇒ 收件人得是这几组并起来的组员之一；不是 ⇒ `not_same_group`（原话说发件人在哪几组）。
//! - 发件人不在任何组 ⇒ 照今天，发给谁都行。
//! - 组照 `groups-read` 那一份（并几台 · 判定都在 `history/groups.rs`）：先问这台的常驻（各台），常驻不在只读这台。
//!   这台那一片没读成（两边都没答 · 这台那一格读不到）⇒ `no_roster`，不退回成全放行。
//! - 整组投递不做（A76）。

use crate::history::groups::GroupsRead;
use crate::history::history_browse::Read;

use super::cc_bus_envelope::Refusal;

/// 这台那一片算不算读到了：答了 · 上次所知都算（上次所知也是一份名单）；读不到 · 没同步过不算。
fn here_read(r: &GroupsRead) -> Result<(), String> {
    match r
        .machines
        .iter()
        .find(|m| m.origin.is_local())
        .map(|m| &m.read)
    {
        Some(Read::Ok { .. } | Read::Stale { .. }) => Ok(()),
        Some(Read::Failed { why }) => Err(why.clone()),
        Some(Read::Unread) | None => Err("this machine's slice was not read".into()),
    }
}

/// 判一封：`roster` 是读到的那一份，或没读成的那一句。纯函数。
pub(crate) fn scope(
    from: &str,
    to: &str,
    roster: Result<&GroupsRead, &str>,
) -> Result<(), Refusal> {
    let r = roster
        .map_err(str::to_string)
        .and_then(|r| here_read(r).map(|()| r))
        .map_err(|why| ("no_roster", format!("group roster not read: {why}")))?;
    let mine: Vec<_> = r
        .groups
        .iter()
        .filter(|g| g.members.iter().any(|m| m.sid == from))
        .collect();
    if mine.is_empty() || mine.iter().any(|g| g.members.iter().any(|m| m.sid == to)) {
        return Ok(());
    }
    let names: Vec<String> = mine
        .iter()
        .map(|g| g.name.clone().unwrap_or_else(|| g.id.clone()))
        .collect();
    Err((
        "not_same_group",
        format!("`to` {to:?} is in none of the sender's groups: {names:?}"),
    ))
}

/// 读名单：先问这台的常驻（`groups-read` 问到各台），常驻不在只读这台；都没成 ⇒ 那一句。
pub(crate) async fn groups_now(tz: crate::common::time::Tz) -> Result<GroupsRead, String> {
    match groups_via_resident(&tz).await {
        Ok(r) => Ok(r),
        Err(why) => {
            tracing::info!("cc-bus 范围：常驻没答（{why}），只读这台的组");
            crate::history::groups::read_here(tz)
                .await
                .map_err(|here| format!("resident: {why}; this machine: {here}"))
        }
    }
}

/// 转给这台的常驻：`groups-read {这台 ＋ 机器表里的每一台}`（等 20 秒：它问到各台 ＋ 回程）。
/// `session-self` 那一问也走这一处（命令表 `registry/history.rs` 里那一份同样的换成调这里）。
pub(crate) async fn groups_via_resident(
    tz: &crate::common::time::Tz,
) -> Result<GroupsRead, String> {
    let home = crate::platform::paths::data_home().ok_or_else(|| "no data home".to_string())?;
    let machines: Vec<cmd_wire::Machine> =
        std::iter::once(cmd_wire::Machine(cmd_wire::LOCAL.into()))
            .chain(
                super::machines::here()
                    .iter()
                    .map(|h| cmd_wire::Machine(h.name().to_string())),
            )
            .collect();
    let asked = crate::wire::listen::ResidentCall {
        cmd: <crate::history::groups::GroupsReadCmd as crate::wire::typed::Command>::NAME
            .to_string(),
        args: serde_json::json!({ "machines": machines }),
    };
    let ms = 20_000;
    let v = super::resident::call_at(
        &home,
        &asked,
        Some(ms),
        tz,
        crate::platform::child::Deadline::millis(ms),
    )
    .await
    .map_err(|f| f.message)?;
    serde_json::from_value(v).map_err(|e| e.to_string())
}

#[cfg(test)]
#[path = "../../../tests/backend/control/cc_bus_scope_tests.rs"]
mod tests;
