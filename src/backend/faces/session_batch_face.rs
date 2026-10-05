//! 帧面 `sessions-where` / `sessions-stop` / `sessions-start` 的宿主壳：把本体（`control/session_batch.rs`）要的那几样拼成生产那一份交进去。
//!
//! 本体住 `control/`，而 tmux 名单与记录在不在住观测层（`control → observe` 是反向边）⇒ 由这层顶层壳补上，与 `fork_face` 同形。
//! tmux 名单 = 终端名单那一趟（挂着 sid 的窗格各一行）· 记录在不在 = `history-record` 那一问 · 杀 = `kill`（按 sid 认窗格）·
//! 就地键入 = `launch send-into`（带 sid ⇒ 落在挂着它的那个窗格）· 铸名 = `terminal-name-mint` · 起会话 = 这台后端自己当 ccm 跑那一行 ·
//! 谁在写 = 这台 pidfile 里持着那个 sid 的活进程（`observe::accounts_query::session_writers`）。

use crate::control::session_batch::{self as batch, Deps, TmuxEntry};
use serde_json::Value;

type Answer = Result<Value, (&'static str, String)>;

/// `client` ＝ 请求自报的前端（`args.client`，「哪个前端的会话」那一维）：杀与就地键入都带它过门。
fn with_deps(args: &Value, f: impl FnOnce(&Deps) -> Answer) -> Answer {
    let client = crate::control::gate::requester_of(args)?;
    // 批量起说是哪一家（整批一格）；停 / 问样子用不着挑号。
    let agent = args
        .get("agent")
        .and_then(Value::as_str)
        .unwrap_or_default();
    with_deps_as(client.as_deref(), agent, f)
}

/// 同 [`with_deps`]，前端与那一家已经取好（换号重启每一步在自己的阻塞线程上各拼一份）。
pub(crate) fn with_deps_as<T>(client: Option<&str>, agent: &str, f: impl FnOnce(&Deps) -> T) -> T {
    let list = tmux_rows;
    let record = |sid: &str, dir: Option<&str>| -> Result<(bool, String), String> {
        crate::observe::history_query::record_for(
            &crate::observe::history_query::agent_home(),
            dir,
            sid,
        )
        .map(|p| (p.present, p.root))
    };
    let kill = |name: &str, sid: &str| {
        crate::control::kill::run_as(name, sid, client).map(|b| b.to_json())
    };
    let send_into = |name: &str, sid: &str, line: &str| {
        let mut req = serde_json::json!({ "mode": "send-into", "name": name, "payload": line, "ccm_sid": sid });
        if let Some(c) = client {
            req["client"] = c.into();
        }
        let req = crate::control::launch::parse_request(&req)?;
        crate::control::launch::run(&req).map(|_| ())
    };
    let run_ccm = |argv: &[String]| crate::control::session_batch::run_self_as_ccm(argv);
    let mint = |base: batch::NameBase| {
        let args = match base {
            batch::NameBase::Cwd(cwd) => serde_json::json!({ "cwd": cwd }),
            batch::NameBase::ForkOf(source) => serde_json::json!({ "forkOf": source }),
        };
        crate::control::ccm::terminal_name_mint_with(
            &args,
            crate::common::session_snapshot::global(),
        )
        .map(|v| v["name"].as_str().unwrap_or_default().to_string())
    };
    let writers = |sid: &str| {
        crate::observe::accounts_query::session_writers(
            &crate::observe::history_query::agent_home(),
            sid,
        )
    };
    let caps = crate::ccm_launcher_with(crate::TMUX_PLATFORM)
        .into_iter()
        .map(str::to_string)
        .collect();
    crate::faces::launch_face::with_facts(agent, |accounts| {
        f(&Deps {
            list: &list,
            record: &record,
            kill: &kill,
            send_into: &send_into,
            run_ccm: &run_ccm,
            mint: &mint,
            caps: &caps,
            local_facts: crate::control::launch_render::local::Facts::PRODUCTION,
            accounts,
            writers: &writers,
        })
    })
}

/// 这台的 tmux 名单（挂着 sid 的窗格各一行）；`Ok(None)` ＝ 这台没装 tmux；`Err` ＝ 看不见。分叉那一格也读它。
pub(crate) fn tmux_rows() -> Result<Option<Vec<TmuxEntry>>, String> {
    let rows = crate::control::terminals::rows_here().map_err(|(_, m)| m)?;
    Ok(rows.map(|rows| {
        rows.into_iter()
            .map(|r| TmuxEntry {
                agent: crate::agents::is_agent_process(&r.program),
                terminal: r.handle(),
                sid: (!r.sid.is_empty()).then_some(r.sid),
                name: r.name,
            })
            .collect()
    }))
}

/// `sessions-where`：`{sids}` ⇒ 每个的样子（菜单就绪时问）。
pub(crate) fn where_(args: &Value) -> Answer {
    with_deps(args, |d| batch::where_(args, d))
}

/// `sessions-stop`：`{sids}` ⇒ `{results}`。
pub(crate) fn stop(args: &Value) -> Answer {
    with_deps(args, |d| batch::stop(args, d))
}

/// `sessions-start`：`{mode, local, items}` ⇒ `{results}`。
pub(crate) fn start(args: &Value) -> Answer {
    with_deps(args, |d| batch::start(args, d))
}
