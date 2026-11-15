//! 起会话挑号要的两样事实（这台的账号库 · 某条会话上次用的号）与起之前预标信任，住观测层与这台的家，由这层交给 `control/`
//! （`control → observe` 是反向边）。`session-open` · 换号重启 · 会话里切号都用这一份；这里不碰 tmux。

use crate::common::door::Door;
use crate::control::launch_account::Facts;
use cmd_wire::accounts::AccountsList;

/// 起会话那几条要的文件管理面（起之前预标信任经它写）：生产那一份由命令表交（`stream/inbound` 的 `LocalFiles`）。
pub(crate) type Files = &'static (dyn Door + Sync);

/// 起之前（用的是账号库里的号）：账号库先对齐（共享库里新冒出来的项各号都链上，会话起来就看得见），
/// 再把 `cwd` 标成 `dir` 那个号信任过（那个号那一家有没有「信任」这件事只由 `trust_share_exec::pretrust` 判）。写不成只出声、照常起。
pub(crate) fn pretrust_with(files: Option<Files>, agent: &str, dir: &str, cwd: &str) {
    use crate::accounts::manage::trust_share_exec::{pretrust, Marked};
    let Some(d) = files else { return };
    if crate::agents::accounts_face(agent).is_none() {
        return;
    }
    crate::accounts::manage::wire::align_here(d);
    match pretrust(d, dir, cwd) {
        Ok(Marked::Wrote | Marked::Already | Marked::Nothing) => {}
        Ok(m) => tracing::warn!("起会话前预标信任：{dir} 没写（{m:?}），会话里会问一次"),
        Err(e) => tracing::warn!(
            "起会话前预标信任：{dir} 没写上，会话里会问一次：{}",
            e.logged()
        ),
    }
}

/// 这台的账号清单成品（就是 `accounts-list` 那一份：并上这台的 API key 表），起会话挑号 · 账号页都读它：照这台的家问 [`list_at`]。
/// 家说不出 ⇒ 交空的家，由 [`list_in`] 照实拒（判法只在 `platform::paths::under_home`，ccm 那一路问同一处）。
pub(crate) fn list_of(agent: &str) -> AccountsList {
    list_at_for(
        &crate::platform::paths::home_dir().unwrap_or_default(),
        Some(agent),
    )
}

/// [`list_of`]，不按起的那一家过滤：几家混列，每个号照它自己的那一家判（登录那一面读它：登的是号，不是起哪一家）。
pub(crate) fn list_mixed() -> AccountsList {
    list_at_for(
        &crate::platform::paths::home_dir().unwrap_or_default(),
        None,
    )
}

/// key 表里照账号 id 找那一行的（掩码，端点地址）（[`crate::accounts::manage::library::KeyOf`]）。
fn key_of(
    keys: &[crate::accounts::upstream_select::file_face::KeyFact],
) -> impl Fn(&str) -> Option<(Option<String>, Option<String>)> + '_ {
    |id| {
        keys.iter()
            .find(|k| k.id == id)
            .map(|k| (k.masked.clone(), k.base_url.clone()))
    }
}

/// [`list_of`]，账号库照家目录 `home` 那一份（`ccm` 的计划按它自己的家算：别名预览可以是另一个家）——
/// 交给 ccm 的计划那一边当「读这台账号清单成品」（`AccountsScan`；控制面不读观测层，由入口交）。key 表 · 中转在不在照这台的（[`list_in`]）。
pub fn list_at(home: &std::path::Path, agent: &str) -> AccountsList {
    list_at_for(home, Some(agent))
}

/// [`list_at`]，起的那一家可缺（缺 ⇒ 不按它过滤，[`list_mixed`]）。
fn list_at_for(home: &std::path::Path, launching: Option<&str>) -> AccountsList {
    list_in(
        home,
        crate::accounts::upstream_select::file_face::machine_key_path().as_deref(),
        launching,
        crate::relay::our_relay_listening(relay_route_core::PORT),
    )
}

/// 清单成品的本体，落点照交进来的：账号库照家 `home`，key 表照 `keys` 那份文件（推不出 ⇒ 零行），中转在不在照 `relay_running`。
/// 账号库与上游选择互不认识：key 表那两样（有哪几行 · 每行的掩码与端点）由这里读来交给账号库。
/// `launching` ＝ 起的那一家（新建会话的选号表只列它的号）；缺 ⇒ 不按它过滤（同 `library::list_product_among`）。
fn list_in(
    home: &std::path::Path,
    keys: Option<&std::path::Path>,
    launching: Option<&str>,
    relay_running: bool,
) -> AccountsList {
    use crate::accounts::upstream_select::file_face::{key_facts_at, rows_at};
    // 家说不出 ⇒ 照实拒、一处不读（判法只在 `under_home`）。
    let accts =
        match crate::platform::paths::under_home(Some(home), relay_route_core::ACCOUNTS_DIR_REL) {
            Ok(d) => d,
            Err(why) => return crate::accounts::manage::library::refused_list(why),
        };
    let rows = keys.map(rows_at).unwrap_or_default();
    let facts = keys.map(key_facts_at).unwrap_or_default();
    let key_of = key_of(&facts);
    crate::accounts::manage::library::list_product_among(
        crate::agents::REGISTRY,
        &accts,
        Some(home),
        &rows.ids,
        &key_of,
        launching,
        rows.agent,
        relay_running,
    )
}

/// 某条会话上次用的号（这台家里那份记录）。
pub(crate) fn last_of(sid: &str) -> Option<String> {
    crate::history::launch_book::last_of(crate::platform::paths::data_home().as_deref(), sid)
}

/// 某条会话上次起在哪种宿主里（这台家里那份记录；恢复照起时那一种读它）。
pub(crate) fn host_of(sid: &str) -> Option<crate::wire::TerminalHost> {
    crate::history::launch_book::host_of(crate::platform::paths::data_home().as_deref(), sid)
}

/// 交 `f` 一份生产事实（`agent` ＝ 请求说的那一家；认不出 ⇒ 当没有账号这一维）。
pub fn with_facts<T>(agent: &str, f: impl FnOnce(&Facts) -> T) -> T {
    let library = || list_of(agent);
    f(&Facts {
        agent: cmd_wire::AgentKind::new(agent),
        has_accounts: crate::agents::account_env_of(agent).is_some_and(|e| !e.is_empty()),
        library: &library,
        last: &last_of,
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/launch_account_face_tests.rs"]
pub(crate) mod tests;
