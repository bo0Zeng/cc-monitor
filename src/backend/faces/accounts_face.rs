//! 帧面宿主：改账号库的那几条 typed 命令（`accounts-*`，本体 `accounts::manage::wire`）＋ 两步装配 ——
//! ① 建 API 号 ⇒ key 写进这台的 apikey 表（`accounts::upstream_select::file_face`，与 `apikey-key-set` 同一个写口）；
//!    删号 ⇒ 表里这个号那一行跟着清掉、回滚 ⇒ 放回去（那两步由执行器排进同一份备份，写的仍是表自己那一口）；
//! ② 建号 ⇒ 这台的别名文件加上 `<名>cc` ＋ `<名>cct`；删号 ⇒ 删掉参数指向它的全部（`assets::aliases::accounts`；
//!    这台说哪几种 shell 就改哪几份，没有 tmux 的那一份只加 `<名>cc`）。只在这两个时刻动，平时不回补；
//! ③ 账号库改过之后 ⇒ 各号共用的用户级 MCP 同步一趟（`accounts::manage::mcp_share_exec`），并让监听器的名单跟上。
//! 各号共用的 MCP 那几条（`accounts-mcp-*`）也从这里进。
//! 两块互不认识，接在这里（`accounts/mod.rs` 头注：账号库与上游选择零引用）。

use crate::accounts::manage::{mcp_share_exec, mcp_share_watch, wire};
use crate::accounts::upstream_select::file_face;
use crate::assets::aliases::accounts as acct_aliases;
use crate::assets::aliases::{self, AmendErr};
use crate::common::door::Door;
use crate::platform::shell::dialect::Shell;
use crate::wire::spec::Fail;
use crate::wire::typed::{self, Command, Files, NoArgs};
use cmd_wire::accounts::{
    AccountAddArgs, AccountChange, AccountInitArgs, AccountKind, AccountLoginCmd,
    AccountMcpNameArgs, AccountMcpPickArgs, AccountMcpSyncArgs, AccountMcpView, AccountNameArgs,
    AccountRemoveArgs, AccountRepairArgs, AccountRollbackArgs, AliasChange, VerifyReport,
};
use copy_core::copy_text;
use serde_json::{json, Value};
use std::path::PathBuf;

/// 这台 key 表的几口，由门递进来（生产：`stream/inbound/` 递 `file_face` 那几个；判据给落在临时目录上的那一份）。
pub(crate) struct KeyDoor<'a> {
    /// 写一个号的 key（同 `apikey-key-set`）。
    pub(crate) set: &'a dyn Fn(&file_face::KeySetReq) -> Result<file_face::KeySet, Fail>,
    /// 摘掉一个号那一行（`{configDir}`）。
    pub(crate) drop: &'a dyn Fn(&Value) -> file_face::FileFaceAnswer,
    /// 从备份那一份放回一个号那一行（`{configDir, from}`）。
    pub(crate) restore: &'a dyn Fn(&Value) -> file_face::FileFaceAnswer,
    /// 那份表在哪。
    pub(crate) path: &'a dyn Fn() -> Result<PathBuf, String>,
}

/// 改账号库那几条的本体（入参已由 typed 那一处收好）。key 表那几口由门递进来（[`KeyDoor`]）。
pub(crate) fn change(
    d: &dyn Door,
    req: wire::Request,
    keys: &KeyDoor,
) -> Result<AccountChange, Fail> {
    let key = match &req {
        // 预演不带 key（界面填表时每改一格问一次，明文不必跟着走）；真建号之前先判 key 与地址写不写得进去，
        // 不然号建好了、key 却落不下。
        wire::Request::Add(a) if a.kind == AccountKind::ApiKey && a.dry_run != Some(true) => {
            let k = a.key.clone().unwrap_or_default();
            file_face::check_inputs(&k, a.base_url.as_deref())?;
            Some((k, a.base_url.clone()))
        }
        _ => None,
    };
    // 删号 · 回滚才碰 key 表那一行：先读表里有哪几个号（读不了 ⇒ 删号那一趟不清它、说一句）。
    let mut unreadable = None;
    let table = match &req {
        wire::Request::Remove(_) | wire::Request::Rollback(_) => match (keys.path)() {
            Ok(p) => match file_face::account_ids_at(&p) {
                Ok(ids) => Some((p.display().to_string(), ids)),
                Err(e) => {
                    unreadable = Some(copy_text("beAcctFace.key.unreadable", &[("why", &e)]));
                    None
                }
            },
            Err(_) => None,
        },
        _ => None,
    };
    let drop = |dir: &str| {
        (keys.drop)(&json!({ "configDir": dir }))
            .map(|_| ())
            .map_err(Fail::into_note)
    };
    let restore = |dir: &str, from: &str| {
        (keys.restore)(&json!({ "configDir": dir, "from": from }))
            .map(|_| ())
            .map_err(Fail::into_note)
    };
    let kt = table.map(|(path, ids)| wire::KeyTable {
        path,
        ids,
        drop: &drop,
        restore: &restore,
    });
    let done = wire::run_change(d, &req, kt.as_ref())?;
    let mut change = done.change;
    if let (Some(note), wire::Request::Remove(_)) = (unreadable, &req) {
        change.notes.push(note);
    }
    if change.applied {
        if let (Some((k, base)), Some(acct)) = (key, change.account.clone()) {
            put_key(&mut change, keys.set, &acct.config_dir, &k, base.as_deref());
        }
    }
    if !done.alias_events.is_empty() {
        let (a, rewrote) = amend_aliases(d, &done.alias_events);
        change.aliases = vec![a];
        // 别名文件真改了 ⇒ 已开着的终端要重读它（启动文件只在启动时读一次）；链接那几条马上就能用，不用重读。
        for path in rewrote {
            change.notes.push(aliases::reload_hint(&path));
        }
    }
    if change.applied {
        sync_mcp(d, &mut change);
    }
    Ok(change)
}

crate::wire::typed::codes! {
    /// `apikey-key-set` 的码。
    pub(crate) enum KeySetErr {
        /// 现有文件解析不了：拒绝、不覆盖。
        BadFile = "bad_file",
        /// 读写盘没成 · 推不出那份文件在哪。
        IoFailed = "io_failed",
    }
}

/// `apikey-key-set`：给一个账号写 key，写完读回（本体 `accounts/upstream_select/file_face.rs`；同步文件 I/O ⇒ 阻塞档；写经分派那一层递的写句柄）。
pub(crate) struct ApikeyKeySet;

impl crate::wire::typed::Command for ApikeyKeySet {
    const NAME: &'static str = "apikey-key-set";
    type Req = file_face::KeySetReq;
    type Reply = file_face::KeySet;
    type Err = KeySetErr;
}

impl crate::wire::typed::Handle for ApikeyKeySet {
    type Needs = crate::wire::typed::needs::Own;
    fn run(
        req: file_face::KeySetReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<file_face::KeySet, KeySetErr>> + Send + 'static
    {
        async move {
            file_face::answer_set(&req, &cx.needs)
                .map_err(|f| crate::wire::typed::of_fail(f, KeySetErr::IoFailed))
        }
    }
}

/// 改账号库那几条递给本体的 key 表几口：写 key · 删号清那一行 · 回滚放回去 · 表在哪。
/// 那份表（上游选择自己的状态）写经分派那一层递进来的写句柄。
fn with_account_keys<R>(own: &crate::platform::Own, f: impl FnOnce(&KeyDoor<'_>) -> R) -> R {
    let set = |a: &file_face::KeySetReq| file_face::answer_set(a, own);
    let drop = |a: &Value| file_face::answer_drop(a, own);
    let restore = |a: &Value| file_face::answer_restore(a, own);
    f(&KeyDoor {
        set: &set,
        drop: &drop,
        restore: &restore,
        path: &file_face::machine_path,
    })
}

typed::codes! {
    /// 改账号库那几条（init · add · remove · set-default · repair · rollback）可能回的码。
    pub(crate) enum ChangeErr {
        /// 读写盘没成（原话里是系统那一句）。
        IoFailed = "io_failed",
        /// 这台还没建账号库。
        NotEnabled = "not_enabled",
        /// 这一趟不能做（重名 · 保留名 · 不认识的号 · 凭据源不对 …）。
        Refused = "refused",
        /// 这台做不了多账号。
        Unsupported = "unsupported",
    }
}

typed::codes! {
    /// `accounts-verify` 可能回的码。
    pub(crate) enum VerifyErr {
        IoFailed = "io_failed",
        Unsupported = "unsupported",
    }
}

typed::codes! {
    /// `accounts-login-cmd` 可能回的码。
    pub(crate) enum LoginCmdErr {
        IoFailed = "io_failed",
        /// 不认识这个号。
        Refused = "refused",
        Unsupported = "unsupported",
    }
}

typed::codes! {
    /// `accounts-mcp-read` · `accounts-mcp-sync` 可能回的码。
    pub(crate) enum McpErr {
        IoFailed = "io_failed",
        /// 停着同步时不许挑 · 这台没有账号库 …
        Refused = "refused",
    }
}

typed::codes! {
    /// `accounts-mcp-remove` · `accounts-mcp-pick` 可能回的码。
    pub(crate) enum McpEditErr {
        IoFailed = "io_failed",
        /// 共享集合里与哪个号里都没有这一条。
        NotFound = "not_found",
        Refused = "refused",
    }
}

/// 改账号库那几条共用的一段：交给本体，下层的失败收进 [`ChangeErr`]。
fn change_with(
    files: Files,
    own: &crate::platform::Own,
    req: wire::Request,
) -> Result<AccountChange, ChangeErr> {
    with_account_keys(own, |keys| change(files, req, keys))
        .map_err(|f| typed::of_fail(f, ChangeErr::IoFailed))
}

/// `accounts-init`：建账号库，把这台现在登录的那个身份收成默认号。
pub(crate) struct AccountsInit;

impl Command for AccountsInit {
    const NAME: &'static str = "accounts-init";
    type Req = AccountInitArgs;
    type Reply = AccountChange;
    type Err = ChangeErr;
}

impl crate::wire::typed::Handle for AccountsInit {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: AccountInitArgs,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<AccountChange, ChangeErr>> + Send + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let (files, own) = (needs.0, &needs.1);

            change_with(files, own, wire::Request::Init(req))
        }
    }
}

/// `accounts-add`：新建一个号（订阅号可导入凭据；API 号的 key 写进这台的 key 表）。
pub(crate) struct AccountsAdd;

impl Command for AccountsAdd {
    const NAME: &'static str = "accounts-add";
    type Req = AccountAddArgs;
    type Reply = AccountChange;
    type Err = ChangeErr;
}

impl crate::wire::typed::Handle for AccountsAdd {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: AccountAddArgs,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<AccountChange, ChangeErr>> + Send + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let (files, own) = (needs.0, &needs.1);

            change_with(files, own, wire::Request::Add(req))
        }
    }
}

/// `accounts-remove`：删一个号。
pub(crate) struct AccountsRemove;

impl Command for AccountsRemove {
    const NAME: &'static str = "accounts-remove";
    type Req = AccountRemoveArgs;
    type Reply = AccountChange;
    type Err = ChangeErr;
}

impl crate::wire::typed::Handle for AccountsRemove {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: AccountRemoveArgs,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<AccountChange, ChangeErr>> + Send + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let (files, own) = (needs.0, &needs.1);

            change_with(files, own, wire::Request::Remove(req))
        }
    }
}

/// `accounts-set-default`：设默认号。
pub(crate) struct AccountsSetDefault;

impl Command for AccountsSetDefault {
    const NAME: &'static str = "accounts-set-default";
    type Req = AccountNameArgs;
    type Reply = AccountChange;
    type Err = ChangeErr;
}

impl crate::wire::typed::Handle for AccountsSetDefault {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: AccountNameArgs,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<AccountChange, ChangeErr>> + Send + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let (files, own) = (needs.0, &needs.1);

            change_with(files, own, wire::Request::SetDefault(req))
        }
    }
}

/// `accounts-repair`：修复账号库（幂等）。
pub(crate) struct AccountsRepair;

impl Command for AccountsRepair {
    const NAME: &'static str = "accounts-repair";
    type Req = AccountRepairArgs;
    type Reply = AccountChange;
    type Err = ChangeErr;
}

impl crate::wire::typed::Handle for AccountsRepair {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: AccountRepairArgs,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<AccountChange, ChangeErr>> + Send + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let (files, own) = (needs.0, &needs.1);

            change_with(files, own, wire::Request::Repair(req))
        }
    }
}

/// `accounts-rollback`：按一份备份还原。
pub(crate) struct AccountsRollback;

impl Command for AccountsRollback {
    const NAME: &'static str = "accounts-rollback";
    type Req = AccountRollbackArgs;
    type Reply = AccountChange;
    type Err = ChangeErr;
}

impl crate::wire::typed::Handle for AccountsRollback {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: AccountRollbackArgs,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<AccountChange, ChangeErr>> + Send + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let (files, own) = (needs.0, &needs.1);

            change_with(files, own, wire::Request::Rollback(req))
        }
    }
}

/// `accounts-verify`：链接 · 权限 · 邮箱逐项核一遍（只读）。
pub(crate) struct AccountsVerify;

impl Command for AccountsVerify {
    const NAME: &'static str = "accounts-verify";
    const TAKES_INPUT: bool = false;
    type Req = NoArgs;
    type Reply = VerifyReport;
    type Err = VerifyErr;
}

impl crate::wire::typed::Handle for AccountsVerify {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        _req: NoArgs,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<VerifyReport, VerifyErr>> + Send + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let (files, _own) = (needs.0, &needs.1);

            wire::run_verify(files).map_err(|f| typed::of_fail(f, VerifyErr::IoFailed))
        }
    }
}

/// `accounts-login-cmd`：在终端里登录一个号的那一行（「⋯」里的逃生口用）。
pub(crate) struct AccountsLoginCmd;

impl Command for AccountsLoginCmd {
    const NAME: &'static str = "accounts-login-cmd";
    type Req = AccountNameArgs;
    type Reply = AccountLoginCmd;
    type Err = LoginCmdErr;
}

impl crate::wire::typed::Handle for AccountsLoginCmd {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: AccountNameArgs,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<AccountLoginCmd, LoginCmdErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            let (files, _own) = (needs.0, &needs.1);

            wire::run_login_cmd(files, &req).map_err(|f| typed::of_fail(f, LoginCmdErr::IoFailed))
        }
    }
}

/// `accounts-mcp-read`：这台各号共用的用户级 MCP 此刻的样子（只读）。
pub(crate) struct AccountsMcpRead;

impl Command for AccountsMcpRead {
    const NAME: &'static str = "accounts-mcp-read";
    const TAKES_INPUT: bool = false;
    type Req = NoArgs;
    type Reply = AccountMcpView;
    type Err = McpErr;
}

impl crate::wire::typed::Handle for AccountsMcpRead {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        _req: NoArgs,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<AccountMcpView, McpErr>> + Send + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let (files, _own) = (needs.0, &needs.1);

            mcp_share_exec::read(files).map_err(|f| typed::of_fail(f, McpErr::IoFailed))
        }
    }
}

/// `accounts-mcp-remove`：从各号共用的用户级 MCP 里删一条（所有号一起撤）。
pub(crate) struct AccountsMcpRemove;

impl Command for AccountsMcpRemove {
    const NAME: &'static str = "accounts-mcp-remove";
    type Req = AccountMcpNameArgs;
    type Reply = AccountMcpView;
    type Err = McpEditErr;
}

impl crate::wire::typed::Handle for AccountsMcpRemove {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: AccountMcpNameArgs,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<AccountMcpView, McpEditErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            let (files, _own) = (needs.0, &needs.1);

            mcp_share_exec::remove(files, &req.name)
                .map_err(|f| typed::of_fail(f, McpEditErr::IoFailed))
        }
    }
}

/// `accounts-mcp-pick`：两边都改了的那一条用哪一版。
pub(crate) struct AccountsMcpPick;

impl Command for AccountsMcpPick {
    const NAME: &'static str = "accounts-mcp-pick";
    type Req = AccountMcpPickArgs;
    type Reply = AccountMcpView;
    type Err = McpEditErr;
}

impl crate::wire::typed::Handle for AccountsMcpPick {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: AccountMcpPickArgs,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<AccountMcpView, McpEditErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            let (files, _own) = (needs.0, &needs.1);

            mcp_share_exec::pick(files, &req.name, req.from.as_deref())
                .map_err(|f| typed::of_fail(f, McpEditErr::IoFailed))
        }
    }
}

/// `accounts-mcp-sync`：停 / 开各号之间同步用户级 MCP。
pub(crate) struct AccountsMcpSync;

impl Command for AccountsMcpSync {
    const NAME: &'static str = "accounts-mcp-sync";
    type Req = AccountMcpSyncArgs;
    type Reply = AccountMcpView;
    type Err = McpErr;
}

impl crate::wire::typed::Handle for AccountsMcpSync {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: AccountMcpSyncArgs,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<AccountMcpView, McpErr>> + Send + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let (files, _own) = (needs.0, &needs.1);

            mcp_share_exec::set_sync(files, req.on).map_err(|f| typed::of_fail(f, McpErr::IoFailed))
        }
    }
}

/// 账号库改过之后（建库 · 加号 · 删号 …）各号共用的用户级 MCP 跟着同步一趟，监听器的名单也跟上（新建的目录此前没被盯着）。
/// 同步没做成不挡这一趟改动，说一句。
fn sync_mcp(d: &dyn Door, change: &mut AccountChange) {
    match mcp_share_exec::sync(d) {
        Ok(v) => {
            if !v.changed.is_empty() {
                change.notes.push(copy_text(
                    "beAcctFace.mcp.synced",
                    &[(
                        "accounts",
                        &v.changed.join(&copy_text("beAcctFace.mcp.listSep", &[])),
                    )],
                ));
            }
            change.notes.extend(v.notes);
            if !v.conflicts.is_empty() {
                let names: Vec<&str> = v.conflicts.iter().map(|c| c.name.as_str()).collect();
                change.notes.push(copy_text(
                    "beAcctFace.mcp.conflicts",
                    &[(
                        "names",
                        &names.join(&copy_text("beAcctFace.mcp.listSep", &[])),
                    )],
                ));
            }
        }
        Err(f) => change.notes.push(copy_text(
            "beAcctFace.mcp.failed",
            &[("why", &f.into_note())],
        )),
    }
    mcp_share_watch::kick();
}

fn put_key(
    change: &mut AccountChange,
    key_set: &dyn Fn(&file_face::KeySetReq) -> Result<file_face::KeySet, Fail>,
    config_dir: &str,
    key: &str,
    base_url: Option<&str>,
) {
    match key_set(&file_face::KeySetReq {
        config_dir: config_dir.to_string(),
        key: key.to_string(),
        base_url: base_url.map(str::to_string),
    }) {
        Ok(v) => change.key_masked = Some(v.masked),
        Err(f) => {
            change.key_problem = Some(copy_text(
                "beAcctFace.key.notSaved",
                &[("why", &f.into_note())],
            ))
        }
    }
}

/// 建号 / 删号那一刻改配置文件（再照它生成别名）。文件里有写错的地方 ⇒ 不动它，说一句。
/// 回：这一趟的结局 ＋ 动了的别名文件。
fn amend_aliases(d: &dyn Door, events: &[wire::AliasEvent]) -> (AliasChange, Vec<String>) {
    let tmux = aliases::Caps::of(Shell::Posix).tmux;
    let dia = Shell::Posix.dialect();
    let same = |a: &str, b: &str| dia.same_name(a, b);
    let (mut added, mut removed, mut skipped) = (Vec::new(), Vec::new(), Vec::new());
    let mut f = |book: &aliases::profile::Book| -> Vec<aliases::profile::Change> {
        (added, removed, skipped) = (Vec::new(), Vec::new(), Vec::new());
        let mut changes = Vec::new();
        for ev in events {
            let p = match ev {
                wire::AliasEvent::Added(acc) => acct_aliases::plan_add(book, acc, tmux, &same),
                wire::AliasEvent::Removed(acc) => acct_aliases::plan_remove(book, acc),
            };
            added.extend(p.added);
            skipped.extend(p.skipped);
            removed.extend(p.removed);
            changes.extend(p.changes);
        }
        changes
    };
    let mut rewrote = Vec::new();
    let change = match aliases::amend_via(d, &mut f) {
        Ok(r) => {
            rewrote = r.rewrote;
            AliasChange {
                path: r.path,
                changed: r.wrote,
                added,
                removed,
                skipped,
                note: None,
            }
        }
        Err(AmendErr::Unparsed { path, n }) => AliasChange {
            note: Some(copy_text(
                "beAcctFace.aliases.unparsed",
                &[("path", &path), ("n", &n.to_string())],
            )),
            path,
            changed: false,
            added: Vec::new(),
            removed: Vec::new(),
            skipped: Vec::new(),
        },
        Err(AmendErr::Failed(e)) => AliasChange {
            path: aliases_path_hint(d),
            changed: false,
            added: Vec::new(),
            removed: Vec::new(),
            skipped: Vec::new(),
            note: Some(copy_text("beAcctFace.aliases.writeFailed", &[("why", &e)])),
        },
    };
    (change, rewrote)
}

/// 写没成时说是哪一份（家目录都问不到 ⇒ 空串）。
fn aliases_path_hint(d: &dyn Door) -> String {
    crate::common::door::home(d)
        .map(|h| aliases::profile::path_in(&h))
        .unwrap_or_default()
}
#[cfg(test)]
#[path = "../../../tests/backend/faces/accounts_face_tests.rs"]
pub(crate) mod tests;
