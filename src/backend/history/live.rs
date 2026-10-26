//! **过渡**：活会话表（[`super::live_table`]）之前的那几个名字，只剩 `history_list` · `faces/history_face` 两份还在问
//! （它们此刻在别的权力范围手里）。一律转问那张表，不另判；那两份改读表之后整份删。

/// 这台此刻活着的每个会话在干什么（[`super::live_table::LiveTable::doing`]）。
pub(crate) fn live_activity(
    agent_home: &std::path::Path,
) -> std::collections::BTreeMap<String, Option<crate::agents::SessionActivity>> {
    super::live_table::now(agent_home).doing()
}

/// 这台判的这一条活不活：活着照 `live` 那一份，否则可重连（会话账本交来的）· 已结束。
pub(crate) fn life_of(
    live: &std::collections::BTreeMap<String, Option<crate::agents::SessionActivity>>,
    sid: &str,
) -> cmd_wire::listing::RowLife {
    use cmd_wire::listing::RowLife;
    match live.get(sid) {
        Some(&activity) => RowLife::Live { activity },
        None if super::live_table::reconnectable(sid) => RowLife::Reconnectable,
        None => RowLife::Ended,
    }
}

/// 会话账本交来的那一份（`history_list` 的判据还问这个名字）。
#[cfg(test)]
pub(crate) use super::live_table::set_reconnectable;
