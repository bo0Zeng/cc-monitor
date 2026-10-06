//! **布局计划** —— 纯：快照 ＋ 意图 ⇒ 一份计划（要建哪些目录、哪些链接、改哪些权限、清单改成什么）。
//! 一个字节都不写；执行在 [`super::exec`]。
//!
//! 布局：每个号一个配置目录（`0700`）。Claude 的身份文件（[`identity`]）每个号各一份（凭据 `0600`），
//! 共享库顶层其余每一项都是一条链回共享库的符号链接（改一处，各号同时看见）。

use super::model::{Account, Manifest, ACCOUNT_ZERO};
use super::scan::{is_under, join, DirState, Snapshot};
use crate::agents::{AccountsFace, IdentityClass, IdentityRoot};
use crate::platform::acct_view::Item;
use copy_core::copy_text;

/// 共享库顶层里**不链**给各号的那几项（`accounts` 目录 · 编辑器留的 `*.bak` / `*.bak-*`）。
const SHARE_EXCLUDE_EXACT: &[&str] = &["accounts"];

/// 这台机器上账号库的布局（注册表里那一家的；没有 ⇒ 这台做不了多账号，`wire` 那一层先拒）。
pub(crate) fn face() -> Option<AccountsFace> {
    crate::agents::account_library_face()
}

/// 身份那几项：`(名, 原生根是家目录, 是身份本体)`。
pub(crate) fn identity() -> Vec<(&'static str, bool, bool)> {
    face()
        .map(|f| f.identity)
        .unwrap_or_default()
        .iter()
        .map(|(n, root, class)| {
            (
                *n,
                *root == IdentityRoot::Home,
                *class == IdentityClass::Secret,
            )
        })
        .collect()
}

/// 那份账号级配置文件的文件名：它的原生根是家目录，账号 0 的那一份住 `$HOME`。
pub(crate) fn identity_config_file() -> &'static str {
    face().map_or("", |f| f.config_file)
}

/// 那份配置文件里装用户级 MCP 的顶层键（账号之间同步只碰它）。
pub(crate) fn user_mcp_key() -> &'static str {
    face().map_or("", |f| f.user_mcp_key)
}

pub(crate) fn is_identity(name: &str) -> bool {
    identity().iter().any(|(n, _, _)| *n == name)
}

fn secrets() -> Vec<&'static str> {
    identity()
        .into_iter()
        .filter(|(_, _, s)| *s)
        .map(|(n, _, _)| n)
        .collect()
}

/// 没设 `CLAUDE_CONFIG_DIR` 时的配置根（= 默认共享库）。
pub(crate) fn shared_root_in(home: &str) -> String {
    face()
        .map(|f| (f.shared_root)(std::path::Path::new(home)))
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// `dir` 下那份 `.claude.json` 里登录的邮箱。
pub(crate) fn email_in(dir: &str) -> Option<String> {
    face().and_then(|f| (f.email_in)(std::path::Path::new(dir)))
}

/// 共享库顶层里不链给各号的那几项。
pub(crate) fn is_excluded(name: &str) -> bool {
    SHARE_EXCLUDE_EXACT.contains(&name) || name.ends_with(".bak") || name.contains(".bak-")
}

/// 共享库里要链给每个号的那几项（排好序）：顶层全部，减去身份那几项与排除的那几项。
pub(crate) fn share_items(shared: &DirState) -> Vec<String> {
    shared
        .entries
        .keys()
        .filter(|n| !is_identity(n) && !is_excluded(n))
        .cloned()
        .collect()
}

/// 计划里的一步。路径一律绝对。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Op {
    /// 建目录（已在就只改权限）并改成 `0700`。
    MkDir(String),
    /// 新建一条链接 `at → target`。
    Link {
        target: String,
        at: String,
    },
    /// 指错了的链接改指 `target`。
    Relink {
        target: String,
        at: String,
    },
    /// 搬一份身份文件进号的目录（建账号库时）。
    Move {
        from: String,
        to: String,
    },
    /// 共享库里那一份复制成这个号自己的（`at` 原是链接或不在；共享库那份留着）。
    Isolate {
        from: String,
        at: String,
    },
    /// 导入一份凭据：复制成 `to`（`0600`）。
    Copy {
        from: String,
        to: String,
    },
    /// 删（只许在账号库里）。
    Remove(String),
    Chmod {
        mode: u32,
        at: String,
    },
    /// 写清单（写的是 [`Plan::manifest`]）。
    WriteManifest,
    /// 删号时清掉 key 表（`table`）里这个号（`config_dir`）那一行 —— 写的是 key 表自己那一口（门递进来）。
    DropKey {
        table: String,
        config_dir: String,
    },
}

/// 一份计划。
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Plan {
    pub ops: Vec<Op>,
    /// 写回去的清单（只有 [`Op::WriteManifest`] 在计划里时才用）。
    pub manifest: Option<Manifest>,
    /// 提示（不挡这一趟）。
    pub notes: Vec<String>,
}

impl Plan {
    fn write(&mut self, m: Manifest) {
        self.manifest = Some(m);
        self.ops.push(Op::WriteManifest);
    }
}

/// 计划被拒：`(码, 那句话)`。码只有两个：`not_enabled`（还没有账号库）· `refused`（其余，句子说清为什么）。
pub(crate) type Refusal = (&'static str, String);

fn refused(said: String) -> Refusal {
    ("refused", said)
}

fn need_manifest(s: &Snapshot) -> Result<&Manifest, Refusal> {
    match &s.manifest {
        None => Err((
            "not_enabled",
            copy_text(
                "beAcctPlan.need.noManifest",
                &[("path", &s.roots().manifest())],
            ),
        )),
        Some(Err(e)) => Err(("refused", e.clone())),
        Some(Ok(m)) => Ok(m),
    }
}

fn check_name(name: &str) -> Result<(), Refusal> {
    if !super::model::name_ok(name) {
        return Err(refused(copy_text(
            "beAcctPlan.name.shape",
            &[
                ("name", &format!("{name:?}")),
                ("max", &shell_quote_core::ACCOUNT_NAME_MAX.to_string()),
            ],
        )));
    }
    if name == ACCOUNT_ZERO {
        return Err(refused(copy_text("beAcctPlan.name.zero", &[])));
    }
    Ok(())
}

/// 一个号该有的共享链接（只计划新建的那几条）。
fn link_all(plan: &mut Plan, s: &Snapshot, cfg: &str) {
    let items = share_items(&s.shared);
    if items.is_empty() {
        plan.notes.push(copy_text(
            "beAcctPlan.links.noneShared",
            &[("shared", &s.roots().shared)],
        ));
    }
    for it in items {
        plan.ops.push(Op::Link {
            target: join(&s.roots().shared, &it),
            at: join(cfg, &it),
        });
    }
}

/// **建账号库**：这台机器现在登录的那个身份收成名叫 `name` 的默认号 —— 身份那几份搬进 `<账号库>/<name>`，
/// 共享库里其余每一项链过去；共享库自己不在就先建它。
pub(crate) fn plan_init(s: &Snapshot, name: &str) -> Result<Plan, Refusal> {
    let r = s.roots();
    if s.manifest.is_some() {
        return Err(refused(copy_text(
            "beAcctPlan.init.already",
            &[("path", &r.manifest())],
        )));
    }
    check_name(name)?;
    let cfg = join(&r.accts, name);
    if s.dir(&cfg).exists() {
        return Err(refused(copy_text(
            "beAcctPlan.dir.exists",
            &[("path", &cfg)],
        )));
    }
    let mut plan = Plan::default();
    plan.ops.push(Op::MkDir(r.accts.clone()));
    if !s.shared.exists() {
        plan.ops.push(Op::MkDir(r.shared.clone()));
    }
    plan.ops.push(Op::MkDir(cfg.clone()));
    let mut moved = Vec::new();
    let mut email_dir = None;
    for (item, home_rooted, _) in identity() {
        let src = match s.shared.get(item) {
            Item::Absent => match s.home_items.get(item) {
                Some(it) if home_rooted && it.exists() => Some((join(&r.home, item), it)),
                _ => None,
            },
            it => Some((join(&r.shared, item), it)),
        };
        let Some((src, it)) = src else { continue };
        if let Item::Link { target, .. } = it {
            return Err(refused(copy_text(
                "beAcctPlan.init.linkedIdentity",
                &[("path", &src), ("target", target)],
            )));
        }
        if item == identity_config_file() {
            email_dir = src.rsplit_once('/').map(|(d, _)| d.to_string());
        }
        plan.ops.push(Op::Move {
            from: src,
            to: join(&cfg, item),
        });
        moved.push(item);
    }
    link_all(&mut plan, s, &cfg);
    for sec in secrets() {
        if moved.contains(&sec) {
            plan.ops.push(Op::Chmod {
                mode: 0o600,
                at: join(&cfg, sec),
            });
        }
    }
    let email = email_dir
        .and_then(|d| s.email_of(&d).map(str::to_string))
        .unwrap_or_default();
    let m = Manifest {
        shared_store: Some(r.shared.clone()),
        ..Manifest::default()
    }
    .with_added(Account {
        name: name.to_string(),
        email,
        config_dir: cfg,
        is_default: true,
        auth_kind: None,
        extra: Default::default(),
    });
    plan.write(m);
    Ok(plan)
}

/// 新建一个号的意图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AddIntent {
    pub name: String,
    pub api_key: bool,
    /// 已解成绝对路径的凭据文件（只订阅号）。
    pub cred_file: Option<String>,
    pub make_default: bool,
}

/// **加号**：建目录 ＋ 链共享项 ＋ 身份之外那几份状态从共享库复制一份（共享库那份是模板）＋ 可选地导入凭据 ＋ 写清单。
/// 身份本体（凭据 · `.claude.json`）刻意不从共享库复制：身份只能是导入的那一份或登录产生的。
pub(crate) fn plan_add(s: &Snapshot, want: &AddIntent) -> Result<Plan, Refusal> {
    let m = need_manifest(s)?;
    let r = s.roots();
    check_name(&want.name)?;
    if m.name_taken(&want.name) {
        return Err(refused(copy_text(
            "beAcctPlan.add.taken",
            &[("name", &want.name)],
        )));
    }
    let cfg = join(&r.accts, &want.name);
    if s.dir(&cfg).exists() {
        return Err(refused(copy_text(
            "beAcctPlan.dir.exists",
            &[("path", &cfg)],
        )));
    }
    let mut plan = Plan::default();
    plan.ops.push(Op::MkDir(cfg.clone()));
    link_all(&mut plan, s, &cfg);
    for (item, _, secret) in identity() {
        if !secret && s.shared.get(item).exists() {
            plan.ops.push(Op::Isolate {
                from: join(&r.shared, item),
                at: join(&cfg, item),
            });
        }
    }
    if let Some(src) = &want.cred_file {
        match s.files.get(src).unwrap_or(&Item::Absent) {
            Item::Link { .. } => {
                return Err(refused(copy_text("beAcctPlan.cred.link", &[("path", src)])));
            }
            Item::File { size: 0, .. } => {
                return Err(refused(copy_text(
                    "beAcctPlan.cred.empty",
                    &[("path", src)],
                )));
            }
            Item::File { .. } => {}
            _ => {
                return Err(refused(copy_text(
                    "beAcctPlan.cred.missing",
                    &[("path", src)],
                )))
            }
        }
        plan.ops.push(Op::Copy {
            from: src.clone(),
            to: join(&cfg, acct_core::CREDENTIALS_NAME),
        });
    }
    let mut next = m.with_added(Account {
        name: want.name.clone(),
        email: String::new(),
        config_dir: cfg,
        is_default: false,
        auth_kind: want
            .api_key
            .then(|| acct_core::AUTH_KIND_API_KEY.to_string()),
        extra: Default::default(),
    });
    if want.make_default {
        next = next.with_default(&want.name);
    }
    plan.write(next);
    Ok(plan)
}

/// **删号**：只删它自己的目录（共享库不动）；删的是默认号 ⇒ 要 `force`，并由第一个号接着当默认。
pub(crate) fn plan_remove(s: &Snapshot, name: &str, force: bool) -> Result<Plan, Refusal> {
    let m = need_manifest(s)?;
    let r = s.roots();
    if name == ACCOUNT_ZERO {
        return Err(refused(copy_text("beAcctPlan.remove.zero", &[])));
    }
    let a = m
        .find(name)
        .ok_or_else(|| refused(copy_text("beAcctPlan.account.unknown", &[("name", name)])))?;
    if a.is_default && !force {
        return Err(refused(copy_text(
            "beAcctPlan.remove.default",
            &[("name", name)],
        )));
    }
    if !is_under(&a.config_dir, &r.accts) || a.config_dir == r.accts {
        return Err(refused(copy_text(
            "beAcctPlan.remove.outside",
            &[("path", &a.config_dir), ("accts", &r.accts)],
        )));
    }
    let mut plan = Plan::default();
    // key 表里有这个号那一行 ⇒ 第一步先清它（清不掉 ⇒ 后面一步都不做）；那份表不在家目录底下 ⇒ 备份不了，不动它、说一句。
    let id = acct_core::apikey_account_id_of_dir(&a.config_dir);
    if let (Some(k), Some(id)) = (&s.keys, id) {
        if k.ids.contains(&id) {
            if is_under(&k.path, &r.home) {
                plan.ops.push(Op::DropKey {
                    table: k.path.clone(),
                    config_dir: a.config_dir.clone(),
                });
            } else {
                plan.notes.push(copy_text(
                    "beAcctPlan.remove.keyOutside",
                    &[("path", &k.path), ("name", name)],
                ));
            }
        }
    }
    if s.dir(&a.config_dir).exists() {
        plan.ops.push(Op::Remove(a.config_dir.clone()));
    }
    let (next, new_default) = m.without(name);
    if let Some(d) = new_default {
        plan.notes
            .push(copy_text("beAcctPlan.remove.newDefault", &[("name", &d)]));
    }
    plan.notes
        .push(copy_text("beAcctPlan.remove.backupKeeps", &[]));
    plan.write(next);
    Ok(plan)
}

/// **设默认号**：只改清单。
pub(crate) fn plan_set_default(s: &Snapshot, name: &str) -> Result<Plan, Refusal> {
    let m = need_manifest(s)?;
    let a = m
        .find(name)
        .ok_or_else(|| refused(copy_text("beAcctPlan.account.unknown", &[("name", name)])))?;
    let mut plan = Plan::default();
    if !a.is_default {
        plan.write(m.with_default(name));
    }
    Ok(plan)
}

/// **修复**（幂等）：目录改回 `0700` · 身份本体改回 `0600` · 缺的共享链接补上 · 指错的改指 · 不再属于共享集的残链清掉
/// （身份那几项若还是链向共享库的链接 ⇒ 复制成这个号自己的一份，不是删掉）· 清单里的邮箱按各号 `.claude.json` 刷新。
/// 共享项在号的目录里是一份实体文件 ⇒ 不动它，只提示（那是这个号自己的改动，合并与否由人定）。
pub(crate) fn plan_repair(s: &Snapshot) -> Result<Plan, Refusal> {
    let m = need_manifest(s)?;
    let r = s.roots();
    let mut plan = Plan::default();
    let mut next = m.clone();
    let items = share_items(&s.shared);
    for a in m.managed() {
        let c = &a.config_dir;
        if !r.controls(c) {
            plan.notes.push(copy_text(
                "beAcctPlan.repair.skipOutside",
                &[("name", &a.name), ("path", c)],
            ));
            continue;
        }
        let d = s.dir(c);
        if !d.is_dir() {
            plan.notes.push(copy_text(
                "beAcctPlan.repair.skipMissing",
                &[("name", &a.name), ("path", c)],
            ));
            continue;
        }
        if *c != r.shared {
            if d.mode().is_some_and(|mo| mo != 0o700) {
                plan.ops.push(Op::Chmod {
                    mode: 0o700,
                    at: c.clone(),
                });
            }
            for sec in secrets() {
                if let Item::File { mode: Some(mo), .. } = d.get(sec) {
                    if *mo != 0o600 {
                        plan.ops.push(Op::Chmod {
                            mode: 0o600,
                            at: join(c, sec),
                        });
                    }
                }
            }
            for it in &items {
                let want = join(&r.shared, it);
                match d.get(it) {
                    Item::Link { target, .. } if *target == want => {}
                    Item::Link { .. } => plan.ops.push(Op::Relink {
                        target: want,
                        at: join(c, it),
                    }),
                    Item::Absent => plan.ops.push(Op::Link {
                        target: want,
                        at: join(c, it),
                    }),
                    _ => plan.notes.push(copy_text(
                        "beAcctPlan.repair.entity",
                        &[("name", &a.name), ("item", it)],
                    )),
                }
            }
            for (base, it) in &d.entries {
                let Item::Link { target, .. } = it else {
                    continue;
                };
                if !is_under(target, &r.shared) || *target == r.shared {
                    continue;
                }
                let in_shared = s.shared.get(base).exists();
                if is_identity(base) {
                    plan.ops.push(if in_shared {
                        Op::Isolate {
                            from: join(&r.shared, base),
                            at: join(c, base),
                        }
                    } else {
                        Op::Remove(join(c, base))
                    });
                } else if is_excluded(base) || !in_shared {
                    plan.ops.push(Op::Remove(join(c, base)));
                }
            }
        }
        if let Some(live) = s.email_of(c) {
            if live != a.email {
                next = next.with_email(&a.name, live);
            }
        }
    }
    if next != *m {
        plan.write(next);
    }
    Ok(plan)
}

/// **隔离**一项：共享库里的 `item` 复制成每个号自己的一份（号那一格是链接或不在的才动；共享库那份留着当模板）。
pub(crate) fn plan_isolate(s: &Snapshot, item: &str) -> Result<Plan, Refusal> {
    if item.is_empty() || item.contains('/') || item == "." || item == ".." {
        return Err(refused(copy_text(
            "beAcctPlan.isolate.badItem",
            &[("item", item)],
        )));
    }
    if face().is_some_and(|f| f.watched.contains(&item)) {
        return Err(refused(copy_text(
            "beAcctPlan.isolate.watched",
            &[("item", item)],
        )));
    }
    let m = need_manifest(s)?;
    let r = s.roots();
    if !s.shared.get(item).exists() {
        return Err(refused(copy_text(
            "beAcctPlan.isolate.notShared",
            &[("item", item), ("shared", &r.shared)],
        )));
    }
    let mut plan = Plan::default();
    if !is_identity(item) {
        plan.notes.push(copy_text(
            "beAcctPlan.isolate.notDeclared",
            &[("item", item)],
        ));
    }
    for a in m.managed() {
        let c = &a.config_dir;
        if !r.controls(c) || *c == r.shared || !s.dir(c).is_dir() {
            continue;
        }
        match s.dir(c).get(item) {
            Item::Link { .. } | Item::Absent => plan.ops.push(Op::Isolate {
                from: join(&r.shared, item),
                at: join(c, item),
            }),
            _ => plan.notes.push(copy_text(
                "beAcctPlan.isolate.already",
                &[("name", &a.name), ("item", item)],
            )),
        }
    }
    Ok(plan)
}

/// 一步给人看的那一句。
pub(crate) fn describe(op: &Op, manifest_path: &str) -> String {
    match op {
        Op::MkDir(p) => copy_text("beAcctPlan.step.mkdir", &[("path", p)]),
        Op::Link { target, at } => {
            copy_text("beAcctPlan.step.link", &[("at", at), ("target", target)])
        }
        Op::Relink { target, at } => {
            copy_text("beAcctPlan.step.relink", &[("at", at), ("target", target)])
        }
        Op::Move { from, to } => copy_text("beAcctPlan.step.move", &[("from", from), ("to", to)]),
        Op::Isolate { from, at } => {
            copy_text("beAcctPlan.step.isolate", &[("at", at), ("from", from)])
        }
        Op::Copy { from, to } => copy_text("beAcctPlan.step.copy", &[("from", from), ("to", to)]),
        Op::Remove(p) => copy_text("beAcctPlan.step.remove", &[("path", p)]),
        Op::Chmod { mode, at } => copy_text(
            "beAcctPlan.step.chmod",
            &[("at", at), ("mode", &format!("{mode:o}"))],
        ),
        Op::WriteManifest => copy_text("beAcctPlan.step.manifest", &[("path", manifest_path)]),
        Op::DropKey { table, config_dir } => copy_text(
            "beAcctPlan.step.dropKey",
            &[("dir", config_dir), ("path", table)],
        ),
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/manage/layout_tests.rs"]
mod tests;
