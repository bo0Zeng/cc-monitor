//! 起会话挑号要的两样事实（这台的账号库 · 某条会话上次用的号）与起之前预标信任，住观测层与这台的家，由这层交给 `control/`
//! （`control → observe` 是反向边）。`session-open` · 换号重启 · 会话里切号都用这一份；这里不碰 tmux。

use crate::common::door::Door;
use crate::control::launch_account::Facts;
use cmd_wire::accounts::AccountsList;

/// 起会话那几条要的文件管理面（起之前预标信任经它写）：生产那一份由命令表交（`stream/inbound` 的 `LocalFiles`）。
pub(crate) type Files = &'static (dyn Door + Sync);

/// 起之前（用的是账号库里的号）：账号库先对齐（共享库里新冒出来的项各号都链上，会话起来就看得见），
/// 再把 `cwd` 标成 `dir` 那个号信任过（那一家有「信任」这件事才做）。写不成只出声、照常起。
pub(crate) fn pretrust_with(files: Option<Files>, agent: &str, dir: &str, cwd: &str) {
    use crate::accounts::manage::trust_share_exec::{pretrust, Marked};
    let Some(d) = files else { return };
    let Some(face) = crate::agents::accounts_face(agent) else {
        return;
    };
    crate::accounts::manage::wire::align_here(d);
    if face.trust.is_none() {
        return;
    }
    match pretrust(d, dir, cwd) {
        Ok(Marked::Wrote | Marked::Already | Marked::Nothing) => {}
        Ok(m) => tracing::warn!("起会话前预标信任：{dir} 没写（{m:?}），会话里会问一次"),
        Err(e) => tracing::warn!(
            "起会话前预标信任：{dir} 没写上，会话里会问一次：{}",
            e.logged()
        ),
    }
}

/// 这台的账号清单成品（就是 `accounts-list` 那一份：并上这台的 API key 表），起会话挑号 · 登录 · 账号页都读它。
/// 账号库与上游选择互不认识：key 表那两样（有哪几行 · 每行的掩码与端点）由这里读来交给账号库。
pub(crate) fn list_of(agent: &str) -> AccountsList {
    let keys = crate::accounts::upstream_select::file_face::machine_key_facts();
    let key_of = key_of(&keys);
    crate::accounts::manage::library::list_product(
        &crate::accounts::upstream_select::file_face::machine_rows(),
        &key_of,
        agent,
        crate::accounts::upstream_select::CREDENTIALS_FILE_AGENT,
        crate::relay::our_relay_listening(relay_route_core::PORT),
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
/// 交给 ccm 的计划那一边当「读这台账号清单成品」（`AccountsScan`；控制面不读观测层，由入口交）。
pub fn list_at(home: &str, agent: &str) -> AccountsList {
    let home = std::path::Path::new(home);
    let keys = crate::accounts::upstream_select::file_face::machine_key_facts();
    let key_of = key_of(&keys);
    crate::accounts::manage::library::list_product_with(
        &crate::platform::paths::home_join(home, relay_route_core::ACCOUNTS_DIR_REL),
        Some(home),
        &crate::accounts::upstream_select::file_face::machine_rows(),
        &key_of,
        agent,
        crate::accounts::upstream_select::CREDENTIALS_FILE_AGENT,
        crate::relay::our_relay_listening(relay_route_core::PORT),
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
        has_accounts: crate::agents::account_env_of(agent).is_some_and(|e| !e.is_empty()),
        library: &library,
        last: &last_of,
    })
}
