//! 帧面 `launch-local` / `launch-render-cli` 的宿主壳：起会话挑号要的两样事实（这台的账号库 · 某条会话上次用的号）
//! 住观测层与这台的家，由这层交给 `control/`（`control → observe` 是反向边）。批量起（`session_batch_face`）用同一份。

use crate::assets::door::Door;
use crate::control::launch_account::{self as la, Facts, Library};
use serde_json::Value;

/// 起会话那几条要的文件管理面（起之前预标信任经它写）：生产那一份由命令表交（`stream/inbound` 的 `LocalFiles`）。
/// 预标住这里不住 `session_batch_face`：渲那一行的两条（`launch-local` · `launch-render-cli`）不碰 tmux，引用不该连带过去。
pub(crate) type Files = &'static (dyn Door + Sync);

/// 起之前：`cwd` 标成 `dir` 那个号信任过（那一家有「信任」这件事才做；写不成只出声、照常起）。
pub(crate) fn pretrust_with(files: Option<Files>, agent: &str, dir: &str, cwd: &str) {
    use crate::accounts::manage::trust_share_exec::{pretrust, Marked};
    let Some(d) = files else { return };
    if crate::agents::accounts_face(agent)
        .and_then(|f| f.trust)
        .is_none()
    {
        return;
    }
    match pretrust(d, dir, cwd) {
        Ok(Marked::Wrote | Marked::Already | Marked::Nothing) => {}
        Ok(m) => tracing::warn!("起会话前预标信任：{dir} 没写（{m:?}），会话里会问一次"),
        Err(e) => tracing::warn!("起会话前预标信任：{dir} 没写上，会话里会问一次：{e}"),
    }
}

/// 这台的账号库（与 `accounts-list` 同一份成品：并上这台的 API key 表）。
pub(crate) fn library_of(agent: &str) -> Library {
    let rows = crate::accounts::upstream_select::file_face::machine_rows();
    Library::of_product(&crate::observe::accounts_query::list_product(
        &rows,
        agent,
        crate::accounts::upstream_select::CREDENTIALS_FILE_AGENT,
    ))
}

/// 某条会话上次用的号（这台家里那份记录）。
pub(crate) fn last_of(sid: &str) -> Option<String> {
    la::last_of(crate::platform::paths::data_home().as_deref(), sid)
}

/// 交 `f` 一份生产事实（`agent` ＝ 请求说的那一家；认不出 ⇒ 当没有账号这一维）。
pub(crate) fn with_facts<T>(agent: &str, f: impl FnOnce(&Facts) -> T) -> T {
    let library = || library_of(agent);
    f(&Facts {
        has_accounts: crate::agents::account_env_of(agent).is_some_and(|e| !e.is_empty()),
        library: &library,
        last: &last_of,
    })
}

fn agent_of(args: &Value) -> String {
    args.get("agent")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// `launch-local`。渲好了 ⇒ 交回那一行之前把工作目录标成要用的那个号信任过（界面随后在终端里跑那一行）。
pub(crate) fn answer_local(
    args: &Value,
    files: Files,
) -> Result<Value, crate::control::launch_render::Failed> {
    let agent = agent_of(args);
    let v = with_facts(&agent, |facts| {
        crate::control::launch_render::answer_local(args, facts)
    })?;
    pretrust_rendered(files, &agent, args, &v);
    Ok(v)
}

/// `launch-render-cli`。同 [`answer_local`]：渲好了 ⇒ 先预标信任。
pub(crate) fn answer_cli(
    args: &Value,
    files: Files,
) -> Result<Value, crate::control::launch_render::Failed> {
    let agent = agent_of(args);
    let v = with_facts(&agent, |facts| {
        crate::control::launch_render::answer_cli(args, facts)
    })?;
    pretrust_rendered(files, &agent, args, &v);
    Ok(v)
}

/// 渲出来的那一行用的是账号库里的号（成品 `account.configDir`）且请求给了工作目录 ⇒ 预标信任。
fn pretrust_rendered(files: Files, agent: &str, args: &Value, v: &Value) {
    if let Some((dir, cwd)) = rendered_target(args, v) {
        pretrust_with(Some(files), agent, dir, cwd);
    }
}

/// 渲好的那一行要预标哪一格：`(号目录, 工作目录)`；账号 0 / 不表态（成品 `account` 为 `null`）· 没给工作目录 ⇒ `None`。
fn rendered_target<'a>(args: &'a Value, v: &'a Value) -> Option<(&'a str, &'a str)> {
    let dir = v["account"]["configDir"].as_str()?;
    let cwd = args
        .get("cwd")
        .and_then(Value::as_str)
        .filter(|c| !c.is_empty())?;
    Some((dir, cwd))
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/launch_face_tests.rs"]
mod tests;
