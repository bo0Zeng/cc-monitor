//! **布局计划** —— 纯：快照 ＋ 意图 ⇒ 一份计划（要建哪些目录、哪些链接、改哪些权限、清单改成什么）。
//! 一个字节都不写；执行在 [`super::exec`]。
//!
//! 布局：每个号一个配置目录（`0700`）。Claude 的身份文件（[`identity`]）每个号各一份（凭据 `0600`）；
//! 已知共享的那几项（[`share_items`]：适配层申报的共享表，共享库里有才链 ∪ 按需才建的 [`shared_on_demand`]，共享库里还没有
//! 也照样链、先在共享库里建出来）各是一条链回共享库的符号链接（改一处，各号同时看见）。两边都不在的名字不链，各号各用各的。

use super::model::{Account, Manifest};
use super::scan::{in_library, is_under, join, DirState, Snapshot};
use crate::agents::{AccountsFace, IdentityClass, IdentityRoot};
use crate::common::said::Said;
use crate::platform::acct_view::Item;
use acct_core::BARE_ACCOUNT;
use copy_core::copy_text;

/// 这台机器上账号库的布局（共享库那一家的：几家的号混列在一份清单里，共享库只归一家；没有 ⇒ 这台做不了多账号，`wire` 那一层先拒）。
pub(crate) fn face() -> Option<AccountsFace> {
    crate::agents::store_family_face()
}

/// 身份那几项：`(名, 原生根是家目录, 是身份本体)`（共享库那一家的，[`identity_of`]）。
pub(crate) fn identity() -> Vec<(&'static str, bool, bool)> {
    face().map(|f| identity_of(&f)).unwrap_or_default()
}

/// `f` 那一家的身份那几项：`(名, 原生根是家目录, 是身份本体)`。
pub(crate) fn identity_of(f: &AccountsFace) -> Vec<(&'static str, bool, bool)> {
    f.identity
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

/// 共享库那一家（[`face`]）账号级配置文件的文件名；它在账号 0 住哪照 [`base_config_of`]。
pub(crate) fn identity_config_file() -> &'static str {
    face().map_or("", |f| f.config_file)
}

fn is_identity(name: &str) -> bool {
    identity().iter().any(|(n, _, _)| *n == name)
}

fn secrets() -> Vec<&'static str> {
    identity()
        .into_iter()
        .filter(|(_, _, s)| *s)
        .map(|(n, _, _)| n)
        .collect()
}

/// `f` 那一家没设配置根变量时的配置根（= 默认共享库），照这一族的写法（`/` 分隔，`platform::acct_view::slash_form`）；
/// 没有那一家 ⇒ 空串。
pub(crate) fn shared_root_of(f: Option<&AccountsFace>, home: &str) -> String {
    crate::platform::acct_view::slash_form(
        &f.map(|f| (f.shared_root)(std::path::Path::new(home)))
            .unwrap_or_default()
            .to_string_lossy(),
    )
}

/// `f` 那一家账号 0 那份账号级配置文件（[`AccountsFace::config_file`]）的路径，照这一族的写法：身份表里它的原生根是家目录 ⇒
/// 住 `$HOME`；否则（原生根是配置根 · 身份表里没登它）⇒ 住账号 0 的配置目录（[`shared_root_of`]）。
pub(crate) fn base_config_of(f: &AccountsFace, home: &str) -> String {
    let at_home = f
        .identity
        .iter()
        .any(|(n, root, _)| *n == f.config_file && *root == IdentityRoot::Home);
    if at_home {
        join(home, f.config_file)
    } else {
        join(&shared_root_of(Some(f), home), f.config_file)
    }
}

/// 那一家按需才建、各号该共用的那几个目录：共享库里还没有也照样链（计划里先在共享库建出来）。
pub(crate) fn shared_on_demand() -> &'static [&'static str] {
    face().map_or(&[], |f| f.shared_on_demand)
}

/// 要链给每个号的那几项（排好序；共享库那一家的，[`share_items_of`]）。对齐 · 修复 · 摘旧链都照它；核对照每个号那一家的表（[`share_items_of`]）。
pub(crate) fn share_items(shared: &DirState) -> Vec<String> {
    face()
        .map(|f| share_items_of(&f, shared))
        .unwrap_or_default()
}

/// `f` 那一家要链给每个号的那几项：共享库顶层此刻有的 ∩ 它申报的已知共享表（身份项除外），∪ 它按需才建的那几个。
pub(crate) fn share_items_of(f: &AccountsFace, shared: &DirState) -> Vec<String> {
    let ident = identity_of(f);
    let mut items: Vec<String> = shared
        .entries
        .keys()
        .filter(|n| f.shared.contains(&n.as_str()) && !ident.iter().any(|(i, _, _)| i == n))
        .cloned()
        .collect();
    for n in f.shared_on_demand {
        if !items.iter().any(|i| i == n) {
            items.push((*n).to_string());
        }
    }
    items.sort_unstable();
    items
}

/// 按需才建的那几个共享库里还没有 ⇒ 先建（同一份计划里只建一次）。
fn seed_shared(f: &AccountsFace, plan: &mut Plan, s: &Snapshot) {
    for n in f.shared_on_demand {
        let at = join(&s.roots().shared, n);
        if !s.shared.get(n).exists() && !plan.ops.contains(&Op::MkDir(at.clone())) {
            plan.ops.push(Op::MkDir(at));
        }
    }
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
    /// 删共享库里一个空目录（对齐种下、还没人用过的按需目录），给随后挪进来的那一份让位；不空 ⇒ 门拒。
    DropEmpty(String),
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

/// 计划被拒。码只有两个：`not_enabled`（还没有账号库）· `refused`（其余，句子说清为什么）。
fn refused(said: String) -> Said {
    Said::new("refused", said)
}

fn need_manifest(s: &Snapshot) -> Result<&Manifest, Said> {
    match &s.manifest {
        None => Err(Said::new(
            "not_enabled",
            copy_text(
                "beAcctPlan.need.noManifest",
                &[("path", &s.roots().manifest())],
            ),
        )),
        Some(Err(e)) => Err(Said::new("refused", e.clone())),
        Some(Ok(m)) => Ok(m),
    }
}

fn check_name(name: &str) -> Result<(), Said> {
    if !super::model::name_ok(name) {
        return Err(refused(copy_text(
            "beAcctPlan.name.shape",
            &[
                ("name", &format!("{name:?}")),
                ("max", &shell_quote_core::ACCOUNT_NAME_MAX.to_string()),
            ],
        )));
    }
    if name == BARE_ACCOUNT {
        return Err(refused(copy_text("beAcctPlan.name.zero", &[])));
    }
    Ok(())
}

/// 新建的那个号该有的共享链接（目录还不在 ⇒ 每一项都缺），照 `f` 那一家的共享表。
fn link_all(f: &AccountsFace, plan: &mut Plan, s: &Snapshot, cfg: &str) {
    seed_shared(f, plan, s);
    link_missing(
        plan,
        s,
        cfg,
        &share_items_of(f, &s.shared),
        &[],
        |_, _, _| {},
    );
}

/// 这个号跟共享库是同一家的（共享库是缺省那一家的配置根）：只有这样的号链共享库、从它复制模板、由对齐 · 修复照看；
/// 别家的号照它那一家的表建目录与凭据，共享库那一侧不碰。
pub(crate) fn of_store_family(registry: &[crate::agents::Adapter], agent: Option<&str>) -> bool {
    let kind = crate::agents::account_kind_among(registry, agent);
    kind.is_some() && kind == crate::agents::account_kind_among(registry, None)
}

/// [`plan_add_among`] 在生产注册表上。
pub(crate) fn plan_add(s: &Snapshot, want: &AddIntent) -> Result<Plan, Said> {
    plan_add_among(crate::agents::REGISTRY, s, want)
}

/// **建账号库**：这台机器现在登录的那个身份收成名叫 `name` 的默认号 —— 身份那几份搬进 `<账号库>/<name>`，
/// 共享库里其余每一项链过去；共享库自己不在就先建它。
pub(crate) fn plan_init(s: &Snapshot, name: &str) -> Result<Plan, Said> {
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
    if let Some(f) = face() {
        link_all(&f, &mut plan, s, &cfg);
    }
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
        color: None,
        agent: None,
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
    /// 这个号是哪一家的号（线上的 kind）；缺席 ⇒ 缺省那一家。写进清单那一号的 `agent`。
    pub agent: Option<String>,
}

/// **加号**：建目录 ＋ 链共享项 ＋ 身份之外那几份状态从共享库复制一份（共享库那份是模板）＋ 可选地导入凭据 ＋ 写清单。
/// 身份本体（凭据 · 账号级配置）刻意不从共享库复制：身份只能是导入的那一份或登录产生的。
/// 布局照号那一家的账号面：共享库是缺省那一家的配置根 ⇒ 别家的号不链它、不从它复制（[`of_store_family`]），
/// 导入的凭据落成那一家凭据文件的名字；认不出的家（没有账号面）⇒ 拒。
pub(crate) fn plan_add_among(
    registry: &[crate::agents::Adapter],
    s: &Snapshot,
    want: &AddIntent,
) -> Result<Plan, Said> {
    let agent = want.agent.as_deref();
    let Some((kind, f)) = crate::agents::account_kind_among(registry, agent)
        .and_then(|k| Some((k, crate::agents::accounts_face_among(registry, k)?)))
    else {
        return Err(refused(copy_text(
            "beArgv.validate.agentNoAccounts",
            &[(
                "agent",
                &crate::agents::family_said_among(registry, agent.unwrap_or_default()),
            )],
        )));
    };
    let same_store = of_store_family(registry, agent);
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
    if same_store {
        link_all(&f, &mut plan, s, &cfg);
    }
    for (item, _, secret) in identity_of(&f) {
        if same_store && !secret && s.shared.get(item).exists() {
            plan.ops.push(Op::Isolate {
                from: join(&r.shared, item),
                at: join(&cfg, item),
            });
        }
    }
    if let Some(src) = &want.cred_file {
        // 导入写成那一家声明的凭据文件名；那一家没声明 ⇒ 说不出写成哪一名，照实拒（不落成别家的名）。
        let Some(login) = f.login else {
            return Err(refused(copy_text(
                "beAcctPlan.cred.noCredsFile",
                &[("agent", &crate::agents::family_said_among(registry, kind))],
            )));
        };
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
            to: join(&cfg, login.creds_file),
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
        color: None,
        agent: want.agent.clone(),
        extra: Default::default(),
    });
    if want.make_default {
        next = next.with_default(&want.name);
    }
    plan.write(next);
    Ok(plan)
}

/// **删号**：只删它自己的目录（共享库不动）；删的是默认号 ⇒ 要 `force`，并由第一个号接着当默认。
pub(crate) fn plan_remove(s: &Snapshot, name: &str, force: bool) -> Result<Plan, Said> {
    let m = need_manifest(s)?;
    let r = s.roots();
    if name == BARE_ACCOUNT {
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
    if !in_library(&a.config_dir, &r.accts) {
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
pub(crate) fn plan_set_default(s: &Snapshot, name: &str) -> Result<Plan, Said> {
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

/// **补号色**（幂等）：旧清单里还没分到号色的号照库序分一格（[`Manifest::with_colors`]）、写回清单；都有了 ⇒ 空计划（不动盘）。
pub(crate) fn plan_colors(s: &Snapshot) -> Result<Plan, Said> {
    let m = need_manifest(s)?;
    let mut plan = Plan::default();
    if m.needs_colors() {
        plan.write(m.with_colors());
    }
    Ok(plan)
}

/// 按需才建的那几个共享库里还没有（或只有对齐种下的一份空目录，[`Snapshot::empty_shared`]）⇒ 恰好一个号里有一份实体目录：
/// 挪进共享库当共用的那一份（空目录先摘掉）；否则共享库里没有就建一份空的（两个号以上各有一份 ⇒ 不替人合并，核对照「各有一份」提示）。
/// 那个号此刻有活会话（[`Snapshot::busy`]；有认不出号的活会话 ⇒ 当所有号都忙）⇒ 这一轮不挪它那一份、出一条提示，
/// 照「各有一份」那一支走（挪走到链回之间会话会重建一份，计划半路撞「已在」停下）。
/// 回挪过来的那几个 `(号的目录, 项)`（那个号随后照样链回去）。只有「修复」调它。
fn lift_on_demand<'a>(plan: &mut Plan, s: &Snapshot, m: &Manifest) -> Vec<(String, &'a str)> {
    let r = s.roots();
    let mut lifted = Vec::new();
    let busy = |a: &Account| s.busy_unknown || s.busy.contains(&a.config_dir);
    for n in shared_on_demand() {
        let seeded = s.empty_shared.contains(*n);
        if s.shared.get(n).exists() && !seeded {
            continue;
        }
        let holders: Vec<&Account> = own_dirs(s, m)
            .filter(|a| matches!(s.dir(&a.config_dir).get(n), Item::Dir { .. }))
            .collect();
        match holders.as_slice() {
            [one] if busy(one) => {
                plan.notes.push(copy_text(
                    "beAcctPlan.repair.liftBusy",
                    &[("name", &one.name), ("item", n)],
                ));
                if !seeded {
                    plan.ops.push(Op::MkDir(join(&r.shared, n)));
                }
            }
            [one] => {
                if seeded {
                    plan.ops.push(Op::DropEmpty(join(&r.shared, n)));
                }
                plan.ops.push(Op::Move {
                    from: join(&one.config_dir, n),
                    to: join(&r.shared, n),
                });
                lifted.push((one.config_dir.clone(), *n));
            }
            _ if seeded => {}
            _ => plan.ops.push(Op::MkDir(join(&r.shared, n))),
        }
    }
    lifted
}

/// 清单里账号库管得着、不是共享库本身、目录在的那几个号。
fn own_dirs<'a>(s: &'a Snapshot, m: &'a Manifest) -> impl Iterator<Item = &'a Account> + 'a {
    let r = s.roots();
    m.managed()
        .filter(|a| of_store_family(crate::agents::REGISTRY, a.agent.as_deref()))
        .filter(move |a| a.config_dir != r.shared && r.controls(&a.config_dir))
        .filter(move |a| s.dir(&a.config_dir).is_dir())
}

/// 号的目录 `c` 照 `items` 逐项：号里没有 · 它那一份刚挪进共享库（`lifted`）⇒ 链回共享库；号里已有的（链接或实体）交 `had` 判。
/// 对齐与修复补链都走这一处。
fn link_missing(
    plan: &mut Plan,
    s: &Snapshot,
    c: &str,
    items: &[String],
    lifted: &[(String, &str)],
    mut had: impl FnMut(&mut Plan, &str, &Item),
) {
    let shared = &s.roots().shared;
    let d = s.dir(c);
    for it in items {
        let moved = lifted
            .iter()
            .any(|(from, n)| from == c && *n == it.as_str());
        match d.get(it) {
            Item::Absent => {}
            _ if moved => {}
            other => {
                had(plan, it, other);
                continue;
            }
        }
        plan.ops.push(Op::Link {
            target: join(shared, it),
            at: join(c, it),
        });
    }
}

/// **账号库对齐**（常驻那一个起来时 · 核对之前 · 起会话之前做）：按需才建的那几个共享库里没有 ⇒ 建一份空的；
/// 每个号照 [`share_items`] 缺的链补上（已知共享表里的一项共享库里新冒出来也在内；认不得的名字不链）。
/// 只建目录 · 建链接 · 摘自己建的旧链（[`drop_own_stale`]）：号里已有的别的（指错的链接 · 自己的实体，按需目录的实体也算）
/// 不动，挪实体是「修复」的事。接好之后再算 ⇒ 空计划（不写、不留备份）。
pub(crate) fn plan_align(s: &Snapshot) -> Result<Plan, Said> {
    let m = need_manifest(s)?;
    let items = share_items(&s.shared);
    let mut plan = Plan::default();
    if let Some(f) = face() {
        seed_shared(&f, &mut plan, s);
    }
    for a in own_dirs(s, m) {
        link_missing(&mut plan, s, &a.config_dir, &items, &[], |_, _, _| {});
        drop_own_stale(&mut plan, s, &a.config_dir, &items);
    }
    Ok(plan)
}

/// 号的目录 `c` 里 cc-monitor 自己建的那一形链接（`<号>/<名> → 共享库/<名>`）不该再在的 ⇒ 摘掉这条链（链的目标一个字节不动）：
/// 名字不在这一次的共享集 `items` 里（认不得的 · 锁 · 原子写的旁名 · 共享库那头已不在的）。身份那几项不在此列（修复复制成号自己的一份）。
fn drop_own_stale(plan: &mut Plan, s: &Snapshot, c: &str, items: &[String]) {
    let shared = &s.roots().shared;
    for (base, it) in &s.dir(c).entries {
        let Item::Link { target, .. } = it else {
            continue;
        };
        if *target == join(shared, base) && !is_identity(base) && !items.contains(base) {
            plan.ops.push(Op::Remove(join(c, base)));
        }
    }
}

/// **修复**（幂等）：目录改回 `0700` · 身份本体改回 `0600` · 缺的共享链接补上 · 指错的改指 · 不再属于共享集的残链清掉
/// （身份那几项若还是链向共享库的链接 ⇒ 复制成这个号自己的一份，不是删掉）· 清单里的邮箱按快照里各号读出的邮箱刷新（各照各家的读法，`scan`）。
/// 共享项在号的目录里是一份实体文件 ⇒ 不动它，只提示（那是这个号自己的改动，合并与否由人定）。
pub(crate) fn plan_repair(s: &Snapshot) -> Result<Plan, Said> {
    let m = need_manifest(s)?;
    let r = s.roots();
    let mut plan = Plan::default();
    let mut next = m.clone();
    let items = share_items(&s.shared);
    let lifted = lift_on_demand(&mut plan, s, m);
    for a in m
        .managed()
        .filter(|a| of_store_family(crate::agents::REGISTRY, a.agent.as_deref()))
    {
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
            link_missing(&mut plan, s, c, &items, &lifted, |plan, it, had| {
                let want = join(&r.shared, it);
                match had {
                    Item::Link { target, .. } if *target == want => {}
                    Item::Link { .. } => plan.ops.push(Op::Relink {
                        target: want,
                        at: join(c, it),
                    }),
                    _ => plan.notes.push(copy_text(
                        "beAcctPlan.repair.entity",
                        &[("name", &a.name), ("item", it)],
                    )),
                }
            });
            for (base, it) in &d.entries {
                let Item::Link { target, .. } = it else {
                    continue;
                };
                if !is_under(target, &r.shared) || *target == r.shared {
                    continue;
                }
                if is_identity(base) {
                    plan.ops.push(if s.shared.get(base).exists() {
                        Op::Isolate {
                            from: join(&r.shared, base),
                            at: join(c, base),
                        }
                    } else {
                        Op::Remove(join(c, base))
                    });
                } else if !items.contains(base) {
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
        Op::Remove(p) | Op::DropEmpty(p) => copy_text("beAcctPlan.step.remove", &[("path", p)]),
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

// 平台层造的旁名（`fs::side_name`）进不了账号库的共享集。
#[cfg(test)]
#[path = "../../../../tests/backend/accounts/manage/side_name_exclusion_tests.rs"]
mod side_name_exclusion_tests;
