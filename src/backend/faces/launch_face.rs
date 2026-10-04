//! 帧面 `launch-local` / `launch-render-cli` 的宿主壳：起会话挑号要的两样事实（这台的账号库 · 某条会话上次用的号）
//! 住观测层与这台的家，由这层交给 `control/`（`control → observe` 是反向边）。批量起（`session_batch_face`）用同一份。

use crate::control::launch_account::{self as la, Facts, Library};
use serde_json::Value;

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

/// `launch-local`。
pub(crate) fn answer_local(args: &Value) -> Result<Value, crate::control::launch_render::Failed> {
    with_facts(&agent_of(args), |facts| {
        crate::control::launch_render::answer_local(args, facts)
    })
}

/// `launch-render-cli`。
pub(crate) fn answer_cli(args: &Value) -> Result<Value, crate::control::launch_render::Failed> {
    with_facts(&agent_of(args), |facts| {
        crate::control::launch_render::answer_cli(args, facts)
    })
}
