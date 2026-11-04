//! 账号库那几条的判据夹具：按名字跑一条（[`answer`]）。本宿主的判据与主 crate 里别族的判据（MCP 共用 · 信任共用 · 登录 MCP）
//! 借它搭夹具（经 `test-support`）。

use super::*;
use std::path::Path;

/// 这一族的应答：成品或失败（码 ＋ 那一句 ＋ 原话）。
pub type Answer = Result<Value, Said>;

/// 按名字跑一条（别族的判据也借它搭夹具）：入参经 typed 那一处收（同生产），再交给本体。门 · key 表那几口由判据给；
/// 策略目录落在门那个家目录的 `.cc-monitor/` 底下。
pub fn answer(
    d: &dyn crate::common::door::Door,
    cmd: &str,
    args: &Value,
    keys: &KeyDoor,
) -> Answer {
    use crate::wire::typed::take;
    fn out<T: serde::Serialize>(v: Result<T, Said>) -> Answer {
        v.map(|v| serde_json::to_value(v).expect("成品序列化得出"))
    }
    let own = crate::common::door::home(d)
        .ok()
        .map(|h| crate::platform::fs::Own::issue_data_at(&Path::new(&h).join(".cc-monitor")));
    let unlist = |name: &str| {
        own.as_ref()
            .map(|o| crate::accounts::quota::setup::shelf::unlist(o, name))
            .unwrap_or_default()
    };
    let change = |req| change(d, req, keys, &unlist);
    match cmd {
        "accounts-init" => out(change(wire::Request::Init(take(args)?))),
        "accounts-add" => out(change(wire::Request::Add(take(args)?))),
        "accounts-remove" => out(change(wire::Request::Remove(take(args)?))),
        "accounts-set-default" => out(change(wire::Request::SetDefault(take(args)?))),
        "accounts-repair" => out(change(wire::Request::Repair(take(args)?))),
        "accounts-rollback" => out(change(wire::Request::Rollback(take(args)?))),
        "accounts-verify" => {
            take::<NoArgs>(args)?;
            out(wire::run_verify(d))
        }
        "accounts-login-cmd" => out(wire::run_login_cmd(d, &take(args)?)),
        "accounts-mcp-read" => {
            take::<NoArgs>(args)?;
            out(mcp_share_exec::read(d))
        }
        "accounts-mcp-remove" => {
            let a: AccountMcpNameArgs = take(args)?;
            out(mcp_share_exec::remove(d, &a.name))
        }
        "accounts-mcp-pick" => {
            let a: AccountMcpPickArgs = take(args)?;
            out(mcp_share_exec::pick(d, &a.name, a.from.as_deref()))
        }
        "accounts-mcp-sync" => {
            let a: AccountMcpSyncArgs = take(args)?;
            out(mcp_share_exec::set_sync(d, a.on))
        }
        other => panic!("判据里没登这条：{other}"),
    }
}
