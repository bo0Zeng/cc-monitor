//! 〔D 组〕MCP 装到一台的 I/O 那一半：帧面 `mcp-sync-source` · `mcp-sync-preview` · `mcp-sync-apply`，只看被点的那一条。
//!
//! 每一问都只在**一台**上：来源那台交那一条（`mcp-sync-source`）；要被写的那一台自己读自己那份、判、写（`-preview` / `-apply`）。
//! 🔴 **密钥不出来源机**：`env` / `headers` 的值（与资产目录只记键名的是同一张表，`asset_catalog::MCP_KEYS_ONLY_FIELDS`）
//! 在来源那台就换成空位（`null`），交出去的只有键名与一个记号；被写那台补空位时只取用户在确认卡上填的、或它自己那一条原有的值，
//! 两样都没有 ⇒ 拒（`needs_input`），不拿骨架凑。
//! 🔴 写**不重读重算**：用户确认的是他看到的那一份；被写那台在他看过之后变了 ⇒ `stale` 就停。
//! 项目级 MCP：那份文件是哪一份 · 什么格式都照请求里的那一家（`agent`，缺 ⇒ 默认那一家；认不出 ⇒ `bad_args`；`mcp_edit::project_mcp_of`），
//! 读照那一家的读法、写只换那一个服务器那一节（`mcp_edit::upsert_mcp_server_value`）。
//! 用户级 MCP：这台有账号库 ⇒ 写进各账号共用的那一份、同步到所有号（`accounts::manage::mcp_share_exec::put`）；
//! 没有 ⇒ 它住 agent 自己的热状态文件，只当来源读，这里不往它里面写。
//! 用户级只认默认那一家：别的家的全局那一份不当来源读、也不往里写（拒），不拿默认那一家那份冒充。

use super::ext::{ExtLoc, ExtSlot};
use super::mcp_edit::{project_mcp_of, project_servers, upsert_mcp_server_value};
use super::skill_install::family_of;
use super::skill_ledger::{Record, RecordReq};
use crate::agents::{Adapter, McpCells, REGISTRY};
use crate::assets::asset_catalog::ExtKind;
use crate::assets::asset_catalog::MCP_KEYS_ONLY_FIELDS;
use crate::assets::hub_wire::{
    McpApplied, McpApplyReq, McpPreview, McpPreviewReq, McpSource, McpSourceReq, McpSourceSlot,
    McpSuspect,
};
use crate::assets::mcp_sync::{candidates, judge, servers_of, Facts, There};
use crate::common::door::{self, Door, Refused};
use crate::common::said::Said;
use cmd_wire::AgentKind;
use copy_core::copy_text;
use serde_json::{Map, Value};

/// 一条定义里装密钥的那几格换成空位：交回（换好的那一条, [(字段, 键)]）。**纯**；不是对象的原样交回、没有空位。
pub(crate) fn redact(def: &Value) -> (Value, Vec<(String, String)>) {
    let mut out = def.clone();
    let mut slots = Vec::new();
    if let Some(o) = out.as_object_mut() {
        for field in MCP_KEYS_ONLY_FIELDS {
            if let Some(Value::Object(m)) = o.get_mut(*field) {
                for (k, v) in m.iter_mut() {
                    *v = Value::Null;
                    slots.push((field.to_string(), k.clone()));
                }
            }
        }
    }
    (out, slots)
}

/// 这一级的 MCP 配置在这台上的哪一份：根 · 相对段 · 项目级那一份的格式（用户级 ⇒ `None`，那几份是 JSON 服务器表）。
struct Spot {
    root: String,
    rel: String,
    cells: Option<McpCells>,
}

impl Spot {
    /// 项目 = `<项目>/<那一家的项目级 MCP 文件>`，格式也是那一家的（`family` 是规整后的家）。
    fn project(reg: &[Adapter], dir: &str, family: Option<&AgentKind>) -> Result<Spot, Said> {
        let (rel, cells) = project_mcp_of(reg, family.map(AgentKind::as_str))?;
        Ok(Spot {
            root: dir.to_string(),
            rel: rel.to_string(),
            cells: Some(cells),
        })
    }

    /// 这一份原文 ⇒ 各服务器（`path` 是它在这台上的路径，`side` 是给人看的「哪一份」）。
    fn servers(
        &self,
        path: &str,
        text: Option<&str>,
        side: &str,
    ) -> Result<Map<String, Value>, Said> {
        match &self.cells {
            Some(c) => project_servers(c, path, text),
            None => servers_of(text, side),
        }
    }
}

/// 来源那一份在这台上：项目 = 那一家的项目级那一份；用户级 = 默认那一家的那份（只读；别的家 ⇒ 拒，不拿默认那一家那份冒充）。
fn source_spot(
    reg: &[Adapter],
    at: &ExtLoc,
    family: Option<&AgentKind>,
    user_mcp: Option<&std::path::Path>,
) -> Result<Spot, Said> {
    if let Some(why) = super::ext::family_refused(ExtKind::Mcp, family, at) {
        return Err(Said::new("refused", why));
    }
    match at {
        ExtLoc::Project { dir } => Spot::project(reg, dir, family),
        ExtLoc::User => {
            let f = user_mcp.ok_or_else(|| {
                crate::wire::spec::fail("io_failed", copy_text("beExt.uninstall.noHome", &[]))
            })?;
            let (Some(dir), Some(name)) = (f.parent(), f.file_name()) else {
                return Err(crate::wire::spec::fail(
                    "io_failed",
                    copy_text("beExt.uninstall.noHome", &[]),
                ));
            };
            Ok(Spot {
                root: dir.display().to_string(),
                rel: name.to_string_lossy().into_owned(),
                cells: None,
            })
        }
    }
}

/// `mcp-sync-source {name, at}`（来源那台）：那一条，密钥值换成空位 `{path, def, slots, token}`。
/// `token` 按**原样那一条**（含密钥值）算：它变了（连只改了一个密钥值也算）⇒ 应用时判 `stale`。
pub(crate) fn answer_source(
    d: &dyn Door,
    user_mcp: Option<&std::path::Path>,
    req: &McpSourceReq,
) -> Result<McpSource, Said> {
    source_among(d, REGISTRY, user_mcp, req)
}

/// [`answer_source`] 照给定注册表那一半（判据拿替身家喂）。
fn source_among(
    d: &dyn Door,
    reg: &[Adapter],
    user_mcp: Option<&std::path::Path>,
    req: &McpSourceReq,
) -> Result<McpSource, Said> {
    let name = req.name.as_str();
    let at = req.at.here()?;
    let family = family_of(reg, req.agent.as_ref())?;
    let spot = source_spot(reg, &at, family.as_ref(), user_mcp)?;
    let got = door::peek(d, &spot.root, &spot.rel).map_err(|m| Said::new("refused", m))?;
    let text = got.text.ok_or_else(|| {
        crate::wire::spec::fail(
            "missing",
            copy_text("beMcpSyncFlow.source.missing", &[("path", &got.path)]),
        )
    })?;
    let servers = spot.servers(
        &got.path,
        Some(&text),
        &copy_text("beMcpSync.answerWith.sourceSide", &[]),
    )?;
    let def = servers.get(name).ok_or_else(|| {
        crate::wire::spec::fail(
            "missing",
            copy_text(
                "beExt.uninstall.noEntry",
                &[("name", name), ("path", &got.path)],
            ),
        )
    })?;
    let (open, slots) = redact(def);
    Ok(McpSource {
        path: got.path,
        def: open,
        slots: slots
            .into_iter()
            .map(|(field, key)| McpSourceSlot { field, key })
            .collect(),
        token: crate::assets::ext::token_of(def),
    })
}

/// 被写那台：`args.def` 必须是一条对象、且装密钥的那几格一个值都没有（来源交出来就该是空位）。
fn def_arg(def: &Value) -> Result<Value, Said> {
    let def = Some(def.clone()).filter(Value::is_object).ok_or_else(|| {
        crate::wire::spec::fail(
            "bad_args",
            crate::common::contract::malformed("missing `def` (an object)"),
        )
    })?;
    if redact(&def).0 != def {
        return Err(crate::wire::spec::fail(
            "bad_args",
            crate::common::contract::malformed("`def` carries values in env / headers"),
        ));
    }
    Ok(def)
}

type Target = (Spot, door::Peeked, Map<String, Value>);

/// 被写那一份在这台上（照请求里的家）：项目 = 那一家的项目级那一份；用户级 = 各账号共用的那一份（别的家 ⇒ 拒）。
fn target_of(
    d: &dyn Door,
    reg: &[Adapter],
    at: &ExtLoc,
    agent: Option<&AgentKind>,
) -> Result<Target, Said> {
    let family = family_of(reg, agent)?;
    let spot = match at {
        ExtLoc::Project { dir } => Spot::project(reg, dir, family.as_ref())?,
        ExtLoc::User => {
            let (root, rel) =
                crate::assets::ext::user_mcp_spot(d, reg, family.as_ref().map(AgentKind::as_str))?;
            Spot {
                root,
                rel,
                cells: None,
            }
        }
    };
    let tgt = door::peek(d, &spot.root, &spot.rel).map_err(|m| Said::new("refused", m))?;
    let servers = spot.servers(
        &tgt.path,
        tgt.text.as_deref(),
        &copy_text("beMcpSync.answerWith.targetSide", &[]),
    )?;
    Ok((spot, tgt, servers))
}

fn has_value(existing: Option<&Value>, field: &str, key: &str) -> Option<String> {
    existing?.get(field)?.get(key)?.as_str().map(str::to_string)
}

/// `mcp-sync-preview {name, at, def}`（要被写的那一台）：读自己那份、判「新 / 一样 / 不同」、可疑项、每个空位这台有没有原值。
pub(crate) fn answer_preview(
    d: &dyn Door,
    facts: &dyn Facts,
    req: &McpPreviewReq,
) -> Result<McpPreview, Said> {
    preview_among(d, REGISTRY, facts, req)
}

/// [`answer_preview`] 照给定注册表那一半（判据拿替身家喂）。
fn preview_among(
    d: &dyn Door,
    reg: &[Adapter],
    facts: &dyn Facts,
    req: &McpPreviewReq,
) -> Result<McpPreview, Said> {
    let name = req.name.as_str();
    let at = req.at.here()?;
    let def = def_arg(&req.def)?;
    let (_, tgt, servers) = target_of(d, reg, &at, req.agent.as_ref())?;
    let existing = servers.get(name);
    let state = match existing.map(|e| redact(e).0) {
        None => "new",
        Some(e) if e == def => "same",
        Some(_) => "differs",
    };
    let slots = redact(&def)
        .1
        .into_iter()
        .map(|(field, key)| ExtSlot {
            kept: has_value(existing, &field, &key).is_some(),
            field,
            key,
        })
        .collect();
    let suspects = judge(&candidates(&def), facts)
        .into_iter()
        .map(|s| McpSuspect {
            kind: s.kind.to_string(),
            field: s.field,
            value: s.value,
            there: s.there.map(|t| There::wire(t).to_string()),
        })
        .collect();
    Ok(McpPreview {
        path: tgt.path,
        state: state.to_string(),
        def,
        slots,
        suspects,
        target: tgt.text.as_deref().map(crate::common::digest::fnv_digest),
    })
}

/// `mcp-sync-apply {name, at, def, fill, target}`：`target` = 看卡时这台那份的记号（不存在 = `null`）；`fill` = `{env: {键: 值}, headers: {…}}`
/// （用户填的；没填的键沿用这台原有的值）。写完记进装记录（写口由 `stream/inbound/` 递进来）。回 `{path, written, recordFailed}`。
pub(crate) fn answer_apply(
    d: &dyn Door,
    record: Record,
    req: &McpApplyReq,
) -> Result<McpApplied, Said> {
    apply_among(d, REGISTRY, record, req)
}

/// [`answer_apply`] 照给定注册表那一半（判据拿替身家喂）。
fn apply_among(
    d: &dyn Door,
    reg: &[Adapter],
    record: Record,
    req: &McpApplyReq,
) -> Result<McpApplied, Said> {
    let name = req.name.as_str();
    let at = req.at.here()?;
    let def = def_arg(&req.def)?;
    let (spot, tgt, servers) = target_of(d, reg, &at, req.agent.as_ref())?;
    let seen = req.target.as_deref();
    let now = tgt.text.as_deref().map(crate::common::digest::fnv_digest);
    if now.as_deref() != seen {
        return Err(crate::wire::spec::fail(
            "stale",
            copy_text("beMcpSyncFlow.apply.stale", &[]),
        ));
    }
    let existing = servers.get(name);
    let mut full = def.clone();
    for (field, key) in redact(&def).1 {
        let typed = req
            .fill
            .as_ref()
            .and_then(|f| f.get(&field))
            .and_then(|m| m.get(&key))
            .filter(|v| !v.is_empty())
            .cloned();
        let value = typed
            .or_else(|| has_value(existing, &field, &key))
            .ok_or_else(|| {
                crate::wire::spec::fail(
                    "needs_input",
                    copy_text(
                        "beExt.apply.needsValue",
                        &[("field", &field), ("key", &key)],
                    ),
                )
            })?;
        full[field.as_str()][key.as_str()] = Value::String(value);
    }
    let at_path = match at {
        ExtLoc::User => tgt.path.clone(),
        ExtLoc::Project { .. } => door::join_under(&spot.root, &spot.rel),
    };
    if existing == Some(&full) {
        return Ok(McpApplied {
            path: at_path,
            written: false,
            record_failed: None,
        });
    }
    if at == ExtLoc::User {
        // 全局 = 这台各账号共用的那一份：写进它、同步到所有号（不记装记录：从全局删只有一条路，各账号一起撤）。
        let view = crate::accounts::manage::mcp_share_exec::put(d, name, &full)?;
        let note = (!view.notes.is_empty()).then(|| view.notes.join(" "));
        return Ok(McpApplied {
            path: at_path,
            written: true,
            record_failed: note,
        });
    }
    // 项目级那一份恒带格式（[`Spot::project`] 照那一家取过一次，这里不再取）。
    let Some(cells) = spot.cells else {
        return Err(crate::wire::spec::fail(
            "bad_args",
            crate::common::contract::malformed("project MCP without a config format"),
        ));
    };
    let next = upsert_mcp_server_value(&cells, &at_path, tgt.text.as_deref(), name, &full)
        .map_err(|m| Said::new("refused", m))?;
    // 先记账、再写文件：改之前的原文只有装记录这一份留底 ⇒ 记不上（读不懂 · 写不进）就一个字节不写。
    let after = crate::common::digest::fnv_digest(&next);
    let noted = record(&RecordReq::McpAdd {
        file: at_path.clone(),
        name: name.to_string(),
        digest: crate::assets::skill_ledger::mcp_digest(&full),
        before: tgt.text.clone(),
        after: after.clone(),
    })?;
    let landed = door::put(
        d,
        &spot.root,
        &spot.rel,
        &next,
        tgt.text.as_deref(),
        false,
        false,
    )
    .map_err(|e| {
        undo_noted(record, &at_path, Some(&after), noted.prior);
        match e {
            Refused::Stale(_) => {
                crate::wire::spec::fail("stale", copy_text("beMcpSyncFlow.apply.stale", &[]))
            }
            e => crate::wire::spec::fail("refused", e.said()),
        }
    })?;
    Ok(McpApplied {
        path: landed.path,
        written: landed.changed,
        record_failed: None,
    })
}

/// 先记了账、文件却没写上：装记录那两格放回成记之前的样子（放不回只记一行日志：那份文件没动，撤回时会报「之后又被改过」）。
pub(crate) fn undo_noted(
    record: Record,
    file: &str,
    after: Option<&str>,
    prior: Option<crate::assets::skill_ledger::McpPrior>,
) {
    let Some(prior) = prior else { return };
    if let Err(e) = record(&RecordReq::McpUndo {
        file: file.to_string(),
        after: after.map(str::to_string),
        prior,
    }) {
        tracing::warn!("装记录没能放回（{file}）：{}", e.said);
    }
}

/// 撤回 cc-monitor 对一份项目级 MCP 配置的改动：整份回到头一回改之前的原文（原来没有 ⇒ 删掉那份），装记录里那一份整格摘掉。
/// 只认装记录里记过的那几份（`ledger` 是装记录的路径；没记过 ⇒ `not_found`）；
/// 现在那一份的整份摘要不等于 cc-monitor 最后写进去的那一份（之后被改过 · 已不在）⇒ `stale`，一个字节不动。写经 `common/door`，带 CAS。
pub(crate) fn revert_project(
    d: &dyn Door,
    ledger: &std::path::Path,
    record: Record,
    file: &str,
) -> Result<super::ext::McpReverted, Said> {
    let book = crate::assets::skill_ledger::load_at(ledger)?;
    let touched = book.touched.get(file).ok_or_else(|| {
        crate::wire::spec::fail(
            "not_found",
            copy_text("beMcpSyncFlow.revert.notRecorded", &[]),
        )
    })?;
    let p = std::path::Path::new(file);
    let (Some(root), Some(rel)) = (p.parent(), p.file_name()) else {
        return Err(crate::wire::spec::fail(
            "bad_args",
            crate::common::contract::malformed("`file` is not a file path"),
        ));
    };
    let (root, rel) = (
        root.display().to_string(),
        rel.to_string_lossy().into_owned(),
    );
    let changed_since =
        || crate::wire::spec::fail("stale", copy_text("beMcpSyncFlow.revert.changedSince", &[]));
    let got = door::peek(d, &root, &rel).map_err(|m| Said::new("refused", m))?;
    let now = got.text.ok_or_else(changed_since)?;
    if crate::common::digest::fnv_digest(&now) != touched.after {
        return Err(changed_since());
    }
    // 先在装记录的锁里核「最后写进去的摘要」还是读到的那一份、摘掉（只摘这一回改动带进来的），再写文件；
    // 文件写不上 ⇒ 记录放回（摘掉之前的那两格在回包里）。
    let noted = record(&RecordReq::McpRevert {
        file: file.to_string(),
        after: touched.after.clone(),
    })?;
    let wrote = match &touched.before {
        Some(orig) => door::put(d, &root, &rel, orig, Some(&now), false, false).map(|_| ()),
        None => door::delete(d, &root, &rel, &now),
    };
    if let Err(e) = wrote {
        undo_noted(record, file, None, noted.prior);
        return Err(match e {
            Refused::Stale(_) => changed_since(),
            e => crate::wire::spec::fail("refused", e.said()),
        });
    }
    Ok(super::ext::McpReverted {
        path: file.to_string(),
        removed: touched.before.is_none(),
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/mcp_sync_flow_tests.rs"]
mod tests;
