//! **命令那一层**：入参按 `acct_core::wire` 严格收 → 读快照 → 算计划 → 预演就交出那几步、否则交执行器落盘。
//! 帧面宿主（`faces/accounts_face.rs`）在这之上接 apikey 表与别名文件那两步。

pub(crate) use super::exec::KeyTable;
use super::exec::{self, Applied};
use super::layout::{self, AddIntent, Plan, Refusal};
use super::scan::{self, is_under, join, KeyRows, Snapshot};
use crate::assets::door::{self, Door};
use acct_core::wire::{
    AccountAddArgs, AccountChange, AccountInitArgs, AccountIsolateArgs, AccountKind,
    AccountLoginCmd, AccountMcpNameArgs, AccountMcpPickArgs, AccountMcpView, AccountNameArgs,
    AccountRef, AccountRemoveArgs, AccountRepairArgs, AccountRollbackArgs, VerifyReport,
};
use copy_core::copy_text;
use serde_json::Value;

/// 一条请求（入参已按线上形状收好）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Request {
    Init(AccountInitArgs),
    Add(AccountAddArgs),
    Remove(AccountRemoveArgs),
    SetDefault(AccountNameArgs),
    Repair(AccountRepairArgs),
    Isolate(AccountIsolateArgs),
    Rollback(AccountRollbackArgs),
    Verify,
    LoginCmd(AccountNameArgs),
}

fn bad(detail: &str) -> Refusal {
    ("bad_args", crate::common::contract::malformed(detail))
}

fn take<T: serde::de::DeserializeOwned>(args: &Value) -> Result<T, Refusal> {
    let v = if args.is_null() {
        Value::Object(Default::default())
    } else {
        args.clone()
    };
    serde_json::from_value(v).map_err(|e| bad(&e.to_string()))
}

/// 入参 ⇒ [`Request`]。形状不对（多键 · 缺键 · 类型不对）⇒ `bad_args`。
pub(crate) fn parse(cmd: &str, args: &Value) -> Result<Request, Refusal> {
    Ok(match cmd {
        "accounts-init" => Request::Init(take(args)?),
        "accounts-add" => Request::Add(take(args)?),
        "accounts-remove" => Request::Remove(take(args)?),
        "accounts-set-default" => Request::SetDefault(take(args)?),
        "accounts-repair" => Request::Repair(take(args)?),
        "accounts-isolate" => Request::Isolate(take(args)?),
        "accounts-rollback" => Request::Rollback(take(args)?),
        "accounts-login-cmd" => Request::LoginCmd(take(args)?),
        "accounts-verify" => {
            if !(args.is_null() || args.as_object().is_some_and(serde_json::Map::is_empty)) {
                return Err(bad("accounts-verify takes no arguments"));
            }
            Request::Verify
        }
        other => return Err(bad(&format!("unknown command {other:?}"))),
    })
}

/// 各号共用的用户级 MCP 那几条（本体 [`super::mcp_share_exec`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum McpRequest {
    Read,
    Remove(AccountMcpNameArgs),
    Pick(AccountMcpPickArgs),
}

/// 是这几条之一 ⇒ `Some(收好的入参)`；别的命令 ⇒ `None`。
pub(crate) fn parse_mcp(cmd: &str, args: &Value) -> Option<Result<McpRequest, Refusal>> {
    Some(match cmd {
        "accounts-mcp-read" => {
            if args.is_null() || args.as_object().is_some_and(serde_json::Map::is_empty) {
                Ok(McpRequest::Read)
            } else {
                Err(bad("accounts-mcp-read takes no arguments"))
            }
        }
        "accounts-mcp-remove" => take(args).map(McpRequest::Remove),
        "accounts-mcp-pick" => take(args).map(McpRequest::Pick),
        _ => return None,
    })
}

/// 跑一条（成品是此刻的样子）。
pub(crate) fn run_mcp(d: &dyn Door, req: &McpRequest) -> Result<AccountMcpView, Refusal> {
    match req {
        McpRequest::Read => super::mcp_share_exec::read(d),
        McpRequest::Remove(a) => super::mcp_share_exec::remove(d, &a.name),
        McpRequest::Pick(a) => super::mcp_share_exec::pick(d, &a.name, a.from.as_deref()),
    }
}

/// 这台做不做得了多账号（不做 ⇒ `unsupported`）。
fn supported() -> Result<(), Refusal> {
    let os = crate::platform::acct_view::multi_account_supported()
        .err()
        .or_else(|| layout::face().is_none().then_some(std::env::consts::OS));
    match os {
        None => Ok(()),
        Some(os) => Err((
            "unsupported",
            copy_text("beAcctWire.platform.unsupported", &[("os", os)]),
        )),
    }
}

fn home_of(d: &dyn Door) -> Result<String, Refusal> {
    door::home(d).map_err(|e| ("io_failed", e))
}

/// 账号库在就拿它那把锁（读 → 改 → 写这一整趟里别的后端进程进不来）；还没建 ⇒ 没东西可锁（写清单那一下的比对兜底）。
fn lock(home: &str) -> Result<Option<crate::platform::lock::DirLock>, Refusal> {
    let accts = scan::accts_root(home);
    if scan::item_at(&accts).exists() {
        crate::platform::lock::hold(std::path::Path::new(&accts))
            .map(Some)
            .map_err(|e| ("io_failed", e))
    } else {
        Ok(None)
    }
}

/// 一趟改动的结局（帧面宿主据 `accounts_after` 决定要不要并别名文件）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Done {
    pub change: AccountChange,
    /// 这一趟之后清单里的具名号；`None` = 没真改（预演 / 计划为空），别名文件不用动。
    pub accounts_after: Option<Vec<String>>,
}

/// 导入凭据的那个路径：`~/…` 按家目录展开；只收家目录底下的绝对路径（无 `..` 段 · 无控制符）。
fn cred_path(home: &str, raw: &str) -> Result<String, Refusal> {
    let t = raw.trim();
    let abs = match t.strip_prefix("~/") {
        Some(rest) => join(home, rest),
        None => t.to_string(),
    };
    let ok = abs.starts_with('/')
        && !abs.split('/').any(|s| s == "..")
        && !abs.chars().any(char::is_control)
        && is_under(&abs, home)
        && abs != home;
    if ok {
        Ok(abs.trim_end_matches('/').to_string())
    } else {
        Err((
            "refused",
            copy_text(
                "beAcctWire.cred.outsideHome",
                &[("path", raw), ("home", home)],
            ),
        ))
    }
}

/// 在终端里起 claude 登录 `name` 那一行：`<家>/.cc-monitor/bin/ccm -- --account <名>`（值一律过唯一的 quote）。
fn login_line(home: &str, name: &str) -> String {
    let ccm = join(
        &join(&join(home, crate::control::exit_policy::DIR_NAME), "bin"),
        crate::control::ccm::SUBCOMMAND_WORD,
    );
    format!(
        "{} {} {} {}",
        shell_quote_core::posix_quote(&ccm),
        crate::control::ccm::argv::flag::END,
        crate::control::ccm::argv::flag::ACCOUNT,
        shell_quote_core::posix_quote(name)
    )
}

fn names_of(m: Option<&super::model::Manifest>) -> Vec<String> {
    m.map(|m| m.managed().map(|a| a.name.clone()).collect())
        .unwrap_or_default()
}

/// 跑一条改动命令（`Verify` / `LoginCmd` 不走这里）。`keys` = 这台 key 表那两口（删号清它那一行、回滚放回去；门递进来）。
pub(crate) fn run_change(
    d: &dyn Door,
    req: &Request,
    keys: Option<&KeyTable>,
) -> Result<Done, Refusal> {
    supported()?;
    let home = home_of(d)?;
    let _held = lock(&home)?;
    let accts = scan::accts_root(&home);
    let (dirs, files, cred) = match req {
        Request::Init(a) => (vec![join(&accts, &a.name)], vec![], None),
        Request::Add(a) => {
            match a.kind {
                AccountKind::ApiKey if a.cred_file.is_some() => {
                    return Err(bad("`credFile` is only for subscription accounts"))
                }
                AccountKind::Subscription if a.key.is_some() || a.base_url.is_some() => {
                    return Err(bad("`key` / `baseUrl` are only for api-key accounts"))
                }
                _ => {}
            }
            let cred = a
                .cred_file
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .map(|raw| cred_path(&home, raw))
                .transpose()?;
            (
                vec![join(&accts, &a.name)],
                cred.iter().cloned().collect(),
                cred,
            )
        }
        _ => (vec![], vec![], None),
    };
    let mut snap = scan::scan(&home, &dirs, &files);
    snap.roots().check().map_err(|e| ("refused", e))?;
    snap.keys = keys.map(|k| KeyRows {
        path: k.path.clone(),
        ids: k.ids.clone(),
    });
    if let Request::Rollback(a) = req {
        return rollback(d, &snap, a, keys);
    }
    let (plan, dry, extra): (Plan, bool, AccountChange) = match req {
        Request::Init(a) => (
            layout::plan_init(&snap, &a.name)?,
            a.dry_run == Some(true),
            AccountChange {
                alias: super::aliases::alias_name(&a.name),
                ..AccountChange::default()
            },
        ),
        Request::Add(a) => {
            let want = AddIntent {
                name: a.name.clone(),
                api_key: a.kind == AccountKind::ApiKey,
                cred_file: cred,
                make_default: a.is_default == Some(true),
            };
            let plan = layout::plan_add(&snap, &want)?;
            let extra = AccountChange {
                account: Some(AccountRef {
                    name: a.name.clone(),
                    config_dir: join(&snap.roots().accts, &a.name),
                }),
                login_cmd: (a.kind == AccountKind::Subscription && want.cred_file.is_none())
                    .then(|| login_line(&home, &a.name)),
                alias: super::aliases::alias_name(&a.name),
                ..AccountChange::default()
            };
            (plan, a.dry_run == Some(true), extra)
        }
        Request::Remove(a) => (
            layout::plan_remove(&snap, &a.name, a.force == Some(true))?,
            a.dry_run == Some(true),
            AccountChange::default(),
        ),
        Request::SetDefault(a) => (
            layout::plan_set_default(&snap, &a.name)?,
            a.dry_run == Some(true),
            AccountChange::default(),
        ),
        Request::Repair(a) => (
            layout::plan_repair(&snap)?,
            a.dry_run == Some(true),
            AccountChange::default(),
        ),
        Request::Isolate(a) => (
            layout::plan_isolate(&snap, &a.item)?,
            a.dry_run == Some(true),
            AccountChange::default(),
        ),
        Request::Rollback(_) | Request::Verify | Request::LoginCmd(_) => {
            return Err(bad("not a change command"))
        }
    };
    let r = snap.roots().clone();
    let steps = plan
        .ops
        .iter()
        .map(|op| layout::describe(op, &r.manifest()))
        .collect();
    let mut change = AccountChange {
        notes: plan.notes.clone(),
        ..extra
    };
    if dry {
        change.steps = steps;
        return Ok(Done {
            change,
            accounts_after: None,
        });
    }
    let zero_email = snap.email_of(&home).unwrap_or_default().to_string();
    let render =
        |m: &super::model::Manifest| m.render(&r.shared, &zero_email, &exec::utc_stamp(true));
    let Applied { backup, steps } =
        exec::apply(d, &r, &plan, snap.manifest_text.as_deref(), &render, keys)?;
    change.applied = !plan.ops.is_empty();
    change.steps = steps;
    change.backup = backup;
    // 别名要并的账号表：写了清单 ⇒ 新清单；修复那一趟清单可能没变，也要补齐（旧号可能从没有过别名）。
    let after = match (&plan.manifest, req) {
        (Some(m), _) => Some(names_of(Some(m))),
        (None, Request::Repair(_)) => Some(names_of(snap.manifest())),
        _ => None,
    };
    Ok(Done {
        change,
        accounts_after: after,
    })
}

fn rollback(
    d: &dyn Door,
    snap: &Snapshot,
    a: &AccountRollbackArgs,
    keys: Option<&KeyTable>,
) -> Result<Done, Refusal> {
    let pick = match &a.backup {
        Some(id) => {
            if !exec::backup_id_ok(id) {
                return Err((
                    "refused",
                    copy_text("beAcctWire.rollback.badId", &[("id", id)]),
                ));
            }
            snap.backups.iter().find(|b| b.id == *id)
        }
        None => snap
            .backups
            .iter()
            .filter(|b| !b.rolled_back && b.undo.is_some())
            .max_by(|x, y| x.id.cmp(&y.id)),
    };
    let Some(b) = pick else {
        return Err((
            "refused",
            copy_text(
                "beAcctWire.rollback.none",
                &[("accts", &snap.roots().accts)],
            ),
        ));
    };
    let Some(undo) = &b.undo else {
        return Err((
            "refused",
            copy_text("beAcctWire.rollback.noUndo", &[("path", &b.path)]),
        ));
    };
    let (steps, bad_lines) = exec::undo_steps(undo);
    let mut change = AccountChange {
        backup: Some(b.id.clone()),
        notes: bad_lines
            .iter()
            .map(|l| copy_text("beAcctWire.rollback.badLine", &[("line", l)]))
            .collect(),
        ..AccountChange::default()
    };
    if a.dry_run == Some(true) {
        change.steps = steps.iter().map(exec::describe_undo).collect();
        return Ok(Done {
            change,
            accounts_after: None,
        });
    }
    let (done, failed) = exec::rollback(d, snap.roots(), &b.path, &steps, keys);
    if !failed.is_empty() {
        return Err((
            "io_failed",
            copy_text(
                "beAcctWire.rollback.partial",
                &[
                    ("done", &done.len().to_string()),
                    ("failed", &failed.join("\n")),
                    ("path", &b.path),
                ],
            ),
        ));
    }
    change.applied = true;
    change.steps = done;
    let after = scan::scan(&snap.roots().home, &[], &[]);
    Ok(Done {
        change,
        accounts_after: Some(names_of(after.manifest())),
    })
}

/// `accounts-verify`。
pub(crate) fn run_verify(d: &dyn Door) -> Result<VerifyReport, Refusal> {
    supported()?;
    let home = home_of(d)?;
    Ok(super::verify::verify(&scan::scan(&home, &[], &[]), &[]))
}

/// `accounts-login-cmd`：只算那一行（不起进程、不碰盘之外的读）。
pub(crate) fn run_login_cmd(d: &dyn Door, a: &AccountNameArgs) -> Result<AccountLoginCmd, Refusal> {
    supported()?;
    let home = home_of(d)?;
    let snap = scan::scan(&home, &[], &[]);
    let known = snap.manifest().is_some_and(|m| m.find(&a.name).is_some());
    if !known {
        return Err((
            "refused",
            copy_text("beAcctPlan.account.unknown", &[("name", &a.name)]),
        ));
    }
    Ok(AccountLoginCmd {
        cmd: login_line(&home, &a.name),
    })
}
