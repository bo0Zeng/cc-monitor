//! 插件装删：在这台照那一家声明的插件那一格（`AssetFace::plugin_cells`）跑装 · 删 · 列那一行。
//! 成败只读退出码；列那一行的输出交那一家的解法读，读不懂 ⇒ 说读不懂、不猜。装 · 删交的是那一家自己的写法（目录条目的 [`KEY`]、列出来的 `PluginListed::key`），这里不拆不拼。
//! 没声明那一格的家 ⇒ 拒（`refused`），一条都不跑。
//! 读者：扩展页卸载那一对（`ext::answer_uninstall_*`）· 枢纽装到被写那台（`hub_inner::PluginInstallCmd`）·
//! 这台扫完资产目录给插件条目标「经不经插件命令管」（[`mark_by_command`]，`asset_catalog::scan_here` 调）。
//!
//! 跑在哪个号下：命令起在那一家默认的家（后端自己的环境，不带号环境），不逐号跑 —— 插件只认各号共用的那一份
//! （Claude 的 `plugins` 在各号共享的那几样里，各号的配置根都链到同一份），一次跑的就是各号看到的那一份。

use super::ext::ExtDone;
use crate::agents::{Adapter, PluginCells, PluginListed};
use crate::common::said::Said;
use crate::platform::child::{Child, Deadline};
use copy_core::copy_text;

/// 跑完一行的结局：退出码（被信号停掉 ⇒ 缺）＋ 两条输出。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ran {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// 怎么跑一行（生产 = [`run_here`]；判据交记账的替身）。
pub type Run<'a> = &'a dyn Fn(&[String]) -> Result<Ran, Said>;

/// 装 · 删那一趟的一行插件命令最多等多久：装要从市场拉，给足。
const PLUGIN_COMMAND_DEADLINE: Deadline = Deadline::secs(300);

/// 扫资产目录那一趟只跑列表：扩展页 · 写备注 · 对端要目录都同步等它，给短的。
const PLUGIN_LIST_DEADLINE: Deadline = Deadline::secs(30);

/// 在这台起那一行、等它跑完（装 · 删那一趟，期限 [`PLUGIN_COMMAND_DEADLINE`]）。起不来 · 超时 ⇒ `io_failed` · `child_timed_out`。
pub fn run_here(argv: &[String]) -> Result<Ran, Said> {
    run_within(argv, PLUGIN_COMMAND_DEADLINE)
}

/// 同 [`run_here`]，扫资产目录那一趟用（期限 [`PLUGIN_LIST_DEADLINE`]）。
pub fn run_listing(argv: &[String]) -> Result<Ran, Said> {
    run_within(argv, PLUGIN_LIST_DEADLINE)
}

#[expect(
    clippy::disallowed_methods,
    reason = "起那一家声明的插件装 · 删 · 列那一行（`AssetFace::plugin_cells` 给的 argv，不过 shell、stdin 空、有期限）：装 · 删由那一家自己的命令写它自己的目录，后端不写；不许加第二种用途"
)]
fn run_within(argv: &[String], within: Deadline) -> Result<Ran, Said> {
    let Some((program, args)) = argv.split_first() else {
        return Err(crate::wire::spec::fail(
            "io_failed",
            crate::common::contract::malformed("empty plugin command"),
        ));
    };
    let out = Child::new(program).args(args).run(within).map_err(|e| {
        let line = argv.join(" ");
        let (code, said) = e.into_cmd_said("io_failed", |why| {
            copy_text("beExt.plugin.spawnFailed", &[("cmd", &line), ("why", why)])
        });
        said.coded(code)
    })?;
    Ok(Ran {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// 没声明插件那一格 ⇒ 拒（说为什么）。
fn refused(registry: &[Adapter], agent: Option<&cmd_wire::AgentKind>) -> Said {
    crate::wire::spec::fail(
        "refused",
        super::ext::plugin_read_only_said(registry, agent),
    )
}

/// 那一家（`agent` 缺 ⇒ 默认那一家）的插件那一格：没声明 · 说不出是哪一家 ⇒ `refused`，认不出那一家 ⇒ `bad_args`。
/// 「声明了没有」全文件只此一处判，往下照拿到的那一格跑。
fn cells_of(
    registry: &[Adapter],
    agent: Option<&cmd_wire::AgentKind>,
) -> Result<PluginCells, Said> {
    super::skill_install::family_among(registry, agent.map(|a| a.as_str()))?
        .and_then(|kind| crate::agents::plugin_cells_among(registry, kind))
        .ok_or_else(|| refused(registry, agent))
}

/// 声明了插件那一格（`agent` 缺 ⇒ 默认那一家）。
pub fn declared(registry: &[Adapter], agent: Option<&cmd_wire::AgentKind>) -> bool {
    cells_of(registry, agent).is_ok()
}

/// 装那一行给人看的样子（卡上写它；`key` = 那一家的写法）。
pub fn install_line(
    registry: &[Adapter],
    agent: Option<&cmd_wire::AgentKind>,
    key: &str,
) -> Result<String, Said> {
    Ok((cells_of(registry, agent)?.install)(key).join(" "))
}

/// 删那一行给人看的样子（卡上写它；`key` 同上）。
pub fn remove_line(
    registry: &[Adapter],
    agent: Option<&cmd_wire::AgentKind>,
    key: &str,
) -> Result<String, Said> {
    Ok((cells_of(registry, agent)?.remove)(key).join(" "))
}

/// 跑一行、读退出码：非 0 · 被停下 ⇒ `io_failed`；退出码与那条命令自己的话进原话（复制详情），不进句子。
fn run_ok(argv: &[String], run: Run) -> Result<Ran, Said> {
    let r = run(argv)?;
    if r.code == Some(0) {
        return Ok(r);
    }
    let line = argv.join(" ");
    let said = match r.code {
        Some(_) => copy_text("beExt.plugin.failed", &[("cmd", &line)]),
        None => copy_text("beExt.plugin.stopped", &[("cmd", &line)]),
    };
    let told = [r.stderr.trim(), r.stdout.trim()]
        .into_iter()
        .find(|s| !s.is_empty());
    let raw = match (r.code, told) {
        (Some(code), Some(t)) => format!("exit {code}\n{t}"),
        (Some(code), None) => format!("exit {code}"),
        (None, t) => t.unwrap_or_default().to_string(),
    };
    Err(Said::new("io_failed", said).and_raw(Some(raw.as_str()).filter(|s| !s.is_empty())))
}

/// 列出这一家这台装着的插件：列那一行没成 ⇒ `io_failed`；输出读不懂 ⇒ `bad_file`。
pub fn listed(
    registry: &[Adapter],
    agent: Option<&cmd_wire::AgentKind>,
    run: Run,
) -> Result<Vec<PluginListed>, Said> {
    let cells = cells_of(registry, agent)?;
    let argv = (cells.list)();
    let r = run_ok(&argv, run)?;
    (cells.read_list)(&r.stdout).ok_or_else(|| {
        Said::new(
            "bad_file",
            copy_text("beExt.plugin.listUnreadable", &[("cmd", &argv.join(" "))]),
        )
        .and_raw(Some(r.stdout.trim()).filter(|s| !s.is_empty()))
    })
}

/// 装一个插件（`key` = 那一家的写法，原样交）：跑装那一行，成了再列一次；列里不见那个写法 · 列没成 ⇒ 做成了、另说一句。
pub fn install(
    registry: &[Adapter],
    agent: Option<&cmd_wire::AgentKind>,
    key: &str,
    run: Run,
) -> Result<ExtDone, Said> {
    let argv = (cells_of(registry, agent)?.install)(key);
    run_ok(&argv, run)?;
    let note = match listed(registry, agent, run) {
        Ok(l) if l.iter().any(|p| p.key == key) => None,
        Ok(_) => Some(copy_text("beExt.plugin.notListed", &[("name", key)])),
        Err(e) => Some(e.said),
    };
    Ok(ExtDone {
        path: argv.join(" "),
        changed: vec![key.to_string()],
        note,
    })
}

/// 删一个插件（`key` 同上）：跑删那一行，成了再列一次；列里还有那个写法 · 列没成 ⇒ 做成了、另说一句。
pub fn remove(
    registry: &[Adapter],
    agent: Option<&cmd_wire::AgentKind>,
    key: &str,
    run: Run,
) -> Result<ExtDone, Said> {
    let argv = (cells_of(registry, agent)?.remove)(key);
    run_ok(&argv, run)?;
    let note = match listed(registry, agent, run) {
        Ok(l) if l.iter().any(|p| p.key == key) => {
            Some(copy_text("beExt.plugin.stillListed", &[("name", key)]))
        }
        Ok(_) => None,
        Err(e) => Some(e.said),
    };
    Ok(ExtDone {
        path: argv.join(" "),
        changed: vec![key.to_string()],
        note,
    })
}

/// 目录条目 summary 里「经那一家的插件命令管」那一格的键（真 ⇒ 表上给「卸载」「装到…」；假 · 缺 ⇒ 不给）。
pub const BY_COMMAND: &str = "byCommand";

/// 目录条目 summary 里「那一家自己的写法」那一格的键（[`PluginSeen::key`](crate::agents::PluginSeen::key) 原样）：
/// 装 · 删交它、列出来照它比；缺（旧对端）⇒ 不给动作。
pub const KEY: &str = "key";

/// 目录条目上那一家的写法（[`KEY`]）；没有（旧对端）⇒ `None`。读这一格全仓只经它。
pub fn key_of(summary: &serde_json::Value) -> Option<&str> {
    summary.get(KEY).and_then(serde_json::Value::as_str)
}

/// 这台扫出的插件条目照各家自己列出来的结果标上 [`BY_COMMAND`]：条目上那一家的写法（[`KEY`]）在它的列表里（用户级）⇒ 真，
/// 否则假；条目上没有写法 ⇒ 留缺（通用层不拿光名字凑）。
/// 只问声明了插件那一格、且这一趟扫出了插件的那几家，每家跑一次列表；列不出（起不来 · 退出码非 0 · 读不懂）⇒
/// 那一家的那一格留缺（表上照实说列不出），回那一句（进目录的 `problems`）。
pub fn mark_by_command(
    registry: &[Adapter],
    assets: &mut [super::asset_catalog::Asset],
    run: Run,
) -> Vec<String> {
    use super::asset_catalog::ExtKind;
    let mut families: Vec<Option<cmd_wire::AgentKind>> = Vec::new();
    for a in assets.iter().filter(|a| a.kind == ExtKind::Plugin) {
        if !families.contains(&a.agent) {
            families.push(a.agent.clone());
        }
    }
    let mut problems = Vec::new();
    for family in families {
        if !declared(registry, family.as_ref()) {
            continue;
        }
        // 列不出 ⇒ 那一格留缺（表上照实说列不出），不记成假（假是「列出来了、里面没有它」）。
        let keys: Option<Vec<String>> = match listed(registry, family.as_ref(), run) {
            Ok(l) => Some(l.into_iter().map(|p| p.key).collect()),
            Err(e) => {
                problems.push(e.said);
                None
            }
        };
        for a in assets
            .iter_mut()
            .filter(|a| a.kind == ExtKind::Plugin && a.agent == family)
        {
            let key = key_of(&a.summary).map(str::to_string);
            let Some(m) = a.summary.as_object_mut() else {
                continue;
            };
            match (&keys, key) {
                (Some(keys), Some(key)) => {
                    let on = a.project.is_none() && keys.contains(&key);
                    m.insert(BY_COMMAND.to_string(), serde_json::Value::Bool(on));
                }
                _ => {
                    m.remove(BY_COMMAND);
                }
            }
        }
    }
    problems
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/plugin_exec_tests.rs"]
mod tests;
