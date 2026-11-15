//! **信任在各号之间同步的那一半 IO** ＋ **cc-monitor 起会话前预标信任**。
//!
//! - 同步一趟，照家分组（[`sync_among`]：每一家只在自己的号之间，读写只经它自己的信任格）：读那一家账号 0 那一份
//!   （**只读**，一个字节不写）与清单里它每个号的那一份 → 信任过的并集 →
//!   每个号缺的那几格标上（[`super::trust_share::mark`]：只换那几格）→ 经文件管理面 `Door::put` 写，`expect` = 读到的那一份（CAS）。
//!   被抢先改了 ⇒ 这个号这一趟不写；抢先的那一下自己会再触发一次同步。同步自己写回去触发的那一趟算出来没有要写的 ⇒ 停。
//! - 预标：cc-monitor 自己起 / 重启会话之前，把工作目录要标的那几个键（适配层说，[`TrustCells::dir_keys`]）标进要用的那个号（只许清单里、又在账号库里的号目录），随后由同步传到全部号。
//!   号目录在账号库外（用户自己的配置目录）⇒ 预标与同步都不写它，[`outside_library`] 交「需手动」让人改清单。
//! - 账号 0 缺的信任（[`zero_gaps`]，只读、不持锁）：同一份家与号的取法，算出那一家其余号信任过、账号 0 未记上的目录
//!   与标上之后的整份，交「待办」那一件让人手动补（同步不写账号 0）。
//! - 整趟持账号库那把锁（与 MCP 同步 · 改账号库那几条命令同一把）。写不成只出声，不挡起会话。

use super::layout;
use super::mcp_share_exec::{lock, manifest_in, put_private, read_text, MAX_CONFIG_BYTES};
use super::model::{Account, Manifest};
use super::scan::{self, join};
use super::trust_share;
use crate::agents::TrustCells;
use crate::common::door::{self, Door, Refused};
use crate::common::said::Said;
use std::collections::BTreeSet;

/// 预标那一下的结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marked {
    /// 标上了。
    Wrote,
    /// 本来就信任过。
    Already,
    /// 没写：这台没有账号库 / 那一家没有「信任」这件事。
    Nothing,
    /// 没写：那个目录不在清单里（或写法不安全）。
    NotListed,
    /// 没写：那个号的号目录不在账号库里（用户自己的配置目录，[`scan::in_library_on_disk`]：字面在库里、落到盘上指到库外的也算）。要人去改清单。
    Outside,
    /// 没写：两次都被抢先改了（下一次文件事件由同步补上）。
    Stale,
}

/// 同步一趟（文件事件 · 加号 · 建库之后）。回改写了的号名。这台没有账号库 / 那一家没有「信任」这件事 ⇒ 什么都不做。
pub fn sync(d: &dyn Door) -> Result<Vec<String>, Said> {
    let home = super::scan::home(d).map_err(|e| Said::new("io_failed", e))?;
    let _held = lock(&home)?;
    let Some(m) = manifest_in(&home)? else {
        return Ok(Vec::new());
    };
    Ok(sync_among(d, &home, crate::agents::REGISTRY, &m))
}

/// [`sync`] 的本体：注册表里有「信任」这件事的每一家各同步一趟，只在那一家自己的号之间（清单里归它的号 ＋ 它的账号 0），
/// 读写只经它自己的信任格与配置文件；账号 0 那一份（[`layout::base_config_of`]）只读。清单里没有它的号的家不碰。
pub fn sync_among(
    d: &dyn Door,
    home: &str,
    registry: &[crate::agents::Adapter],
    m: &Manifest,
) -> Vec<String> {
    trust_families(home, registry, m)
        .into_iter()
        .flat_map(|t| sync_with(d, home, &t.cells, t.file, &t.zero, &t.family))
        .collect()
}

/// 要同步信任的每一家（[`trust_families`]）账号 0 那份配置所在的目录（去重、表序）：监看照它盯，不另判哪几家 · 账号 0 在哪。
pub(crate) fn trust_zero_dirs(
    home: &str,
    registry: &[crate::agents::Adapter],
    m: &Manifest,
) -> Vec<std::path::PathBuf> {
    let mut out: Vec<std::path::PathBuf> = Vec::new();
    for t in trust_families(home, registry, m) {
        if let Some(dir) = std::path::Path::new(&t.zero).parent() {
            if !out.iter().any(|o| o == dir) {
                out.push(dir.to_path_buf());
            }
        }
    }
    out
}

/// 一家在信任这件事上的那几样：信任格 · 配置文件名 · 账号 0 那一份的路径 · 清单里它的号。
struct TrustFamily {
    kind: &'static str,
    cells: TrustCells,
    file: &'static str,
    zero: String,
    /// 那一家的默认共享库（账号 0 的配置根；旧的就地号的号目录就是它）。
    shared: String,
    family: Manifest,
}

/// 注册表里有「信任」这件事、清单里有它的号的每一家（同步与「账号 0 缺的信任」同一份取法）。
fn trust_families(
    home: &str,
    registry: &[crate::agents::Adapter],
    m: &Manifest,
) -> Vec<TrustFamily> {
    registry
        .iter()
        .filter_map(|a| {
            let f = a.accounts?;
            let cells = f.trust?;
            let family = of_family(registry, m, a.kind);
            family.managed().next()?;
            Some(TrustFamily {
                kind: a.kind,
                cells,
                file: f.config_file,
                zero: layout::base_config_of(&f, home),
                shared: layout::shared_root_of(Some(&f), home),
                family,
            })
        })
        .collect()
}

/// 清单里一个号那一份配置读出来的样子。
struct AcctConf<'a> {
    name: &'a String,
    dir: &'a str,
    path: String,
    raw: Option<String>,
    trusted: BTreeSet<String>,
}

/// 清单 `m` 里每个号那一份 `file`：写法不安全 · 读不出来的留一行日志、不带；号目录不在账号库里的连读也不读
/// （用户自己的配置，它信任过的目录不并进库里各号，[`scan::in_library_on_disk`]）。
fn read_accounts<'a>(
    home: &str,
    cells: &TrustCells,
    file: &str,
    m: &'a Manifest,
) -> Vec<AcctConf<'a>> {
    let accts = scan::accts_root(home);
    let mut out = Vec::new();
    for a in m.managed() {
        let (name, dir) = (&a.name, &a.config_dir);
        if listed(m, dir).is_none() {
            tracing::warn!("账号之间同步信任：{name} 号的目录写法不安全，跳过它");
            continue;
        }
        if !scan::in_library_on_disk(dir, &accts) {
            tracing::warn!(
                "账号之间同步信任：{name} 号的目录 {dir} 不在账号库里，不读也不写它（需手动改清单）"
            );
            continue;
        }
        let path = join(dir, file);
        match read_conf(cells, &path) {
            Ok((raw, trusted)) => out.push(AcctConf {
                name,
                dir,
                path,
                raw,
                trusted,
            }),
            Err(e) => tracing::warn!(
                "账号之间同步信任：{name} 号的配置 {path} 读不出来，这一次跳过它：{}",
                e.logged()
            ),
        }
    }
    out
}

/// 一家账号 0 比同一家别的号缺的那几格信任：同步不写账号 0，由「待办」那一件交给人手动补。
pub struct ZeroGap {
    /// 那一家（注册表 `kind`；待办那一件的 id 带它）。
    pub kind: &'static str,
    /// 那一家对人的叫法。
    pub family: String,
    /// 账号 0 那一份配置（[`layout::base_config_of`]）。
    pub path: String,
    /// 它现在的原文（不在 ⇒ `None`）。
    pub text: Option<String>,
    /// 清单里那一家的号信任过、账号 0 未记上的目录。
    pub missing: BTreeSet<String>,
    /// 原文里标上 `missing` 之后的整份（那一家的改法标的）。
    pub whole: String,
}

/// 这台账号库里各家账号 0 缺的信任（没有账号库 · 读不出 ⇒ 空）。家与号照 `registry`（足迹那一趟取的那一份）。只读，不持锁。
pub fn zero_gaps(d: &dyn Door, registry: &[crate::agents::Adapter]) -> Vec<ZeroGap> {
    let Ok(home) = super::scan::home(d) else {
        return Vec::new();
    };
    match manifest_in(&home) {
        Ok(Some(m)) => zero_gaps_among(&home, registry, &m),
        _ => Vec::new(),
    }
}

/// [`zero_gaps`] 的本体（注册表 · 清单是参数，判据拿替身家喂它）。家与号的取法同 [`sync_among`]
/// （[`trust_families`] · [`read_accounts`]）；账号 0 那一份读不出 · 标不上 ⇒ 说不清怎么补，那一家不出。
pub fn zero_gaps_among(
    home: &str,
    registry: &[crate::agents::Adapter],
    m: &Manifest,
) -> Vec<ZeroGap> {
    let mut out = Vec::new();
    for t in trust_families(home, registry, m) {
        let path = t.zero;
        let (text, have) = match read_conf(&t.cells, &path) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    "账号 0 缺的信任：{path} 读不出来，这一家不报：{}",
                    e.logged()
                );
                continue;
            }
        };
        let missing: BTreeSet<String> = read_accounts(home, &t.cells, t.file, &t.family)
            .into_iter()
            .flat_map(|a| a.trusted)
            .filter(|dir| !have.contains(dir))
            .collect();
        let now = text.as_deref().unwrap_or(trust_share::blank(&t.cells));
        // 缺不缺只由那一家的改法判（都已信任过 ⇒ `Ok(None)`，这一家不出）。
        let whole = match trust_share::mark(&t.cells, now, &missing) {
            Ok(Some(whole)) => whole,
            Ok(None) => continue,
            Err(e) => {
                tracing::warn!("账号 0 缺的信任：{path} 标不上，这一家不报：{e}");
                continue;
            }
        };
        out.push(ZeroGap {
            kind: t.kind,
            family: crate::agents::family_said_among(registry, t.kind),
            path,
            text,
            missing,
            whole,
        });
    }
    out
}

/// 清单里号目录不在账号库里的一个号：预标与同步都不写它，由「需手动」那一件让人去改清单。
pub struct OutsideLibrary {
    /// 号名。
    pub name: String,
    /// 清单里写的号目录。
    pub dir: String,
    /// 清单那一份的路径。
    pub manifest: String,
    /// 账号库目录。
    pub accts: String,
    /// 旧的就地号：号目录就是那一家的默认共享库（不自动迁，待办里单列那一句）。
    pub in_place: bool,
}

/// 这台清单里有「信任」这件事的那几家、号目录不在账号库里的号（没有账号库 · 读不出 ⇒ 空）。家与号照 `registry`（足迹那一趟取的那一份）。只读，不持锁。
pub fn outside_library(d: &dyn Door, registry: &[crate::agents::Adapter]) -> Vec<OutsideLibrary> {
    let Ok(home) = super::scan::home(d) else {
        return Vec::new();
    };
    match manifest_in(&home) {
        Ok(Some(m)) => outside_library_among(&home, registry, &m),
        _ => Vec::new(),
    }
}

/// [`outside_library`] 的本体（家与号的取法同 [`sync_among`]；写法不安全的号本来就不碰，不出）。
pub fn outside_library_among(
    home: &str,
    registry: &[crate::agents::Adapter],
    m: &Manifest,
) -> Vec<OutsideLibrary> {
    let accts = scan::accts_root(home);
    let manifest = join(&accts, scan::MANIFEST_FILE);
    trust_families(home, registry, m)
        .iter()
        .flat_map(|t| t.family.managed().map(move |a| (t, a)))
        .filter(|(_, a)| acct_core::config_dir_ok(&a.config_dir))
        .filter(|(_, a)| !scan::in_library_on_disk(&a.config_dir, &accts))
        .map(|(t, a)| OutsideLibrary {
            name: a.name.clone(),
            dir: a.config_dir.clone(),
            manifest: manifest.clone(),
            accts: accts.clone(),
            in_place: a.config_dir.trim_end_matches('/') == t.shared,
        })
        .collect()
}

/// 清单里 `kind` 那一家的号（号表那一格缺席 ⇒ 缺省那一家，`agents::account_kind_among`）；别的条目原样不带。
pub(crate) fn of_family(registry: &[crate::agents::Adapter], m: &Manifest, kind: &str) -> Manifest {
    let mut out = m.clone();
    out.entries.retain(|e| match e {
        super::model::Entry::Managed(a) => {
            crate::agents::account_kind_among(registry, a.agent.as_deref()) == Some(kind)
        }
        _ => true,
    });
    out
}

/// 清单里号目录是 `config_dir` 的那个号那一家的信任格与配置文件名；认不出家 · 那一家没有「信任」这件事 ⇒ `None`。
pub(crate) fn trust_of_account(
    registry: &[crate::agents::Adapter],
    m: &Manifest,
    config_dir: &str,
) -> Option<(TrustCells, &'static str)> {
    let a = listed(m, config_dir)?;
    let kind = crate::agents::account_kind_among(registry, a.agent.as_deref())?;
    let f = crate::agents::accounts_face_among(registry, kind)?;
    Some((f.trust?, f.config_file))
}

/// 一份配置：原文（不在 ⇒ `None`）＋ 信任过的目录。读不出来 / 解不开 ⇒ `Err`。
fn read_conf(
    cells: &TrustCells,
    path: &str,
) -> Result<(Option<String>, BTreeSet<String>), crate::common::said::Said> {
    let Some(raw) = read_text(path, MAX_CONFIG_BYTES)? else {
        return Ok((None, BTreeSet::new()));
    };
    let set = trust_share::trusted_in(cells, &raw).map_err(crate::common::said::Said::from)?;
    Ok((Some(raw), set))
}

/// 号目录过得了那两道关：写法安全（全仓那一条规矩）＋ 在清单里。过得了 ⇒ 清单里那一条的写法。
fn listed<'a>(m: &'a Manifest, dir: &str) -> Option<&'a Account> {
    let norm = |p: &str| p.trim_end_matches('/').to_string();
    if !acct_core::config_dir_ok(dir) {
        return None;
    }
    m.managed()
        .find(|a| acct_core::config_dir_ok(&a.config_dir) && norm(&a.config_dir) == norm(dir))
}

/// 一家同步一趟：信任那几格 · 配置文件名 · 账号 0 那一份的路径 · 清单（那一家的具名号）是参数（判据拿假适配层的格式喂它）。
/// 账号 0 那一份（`zero`）只并它信任过的、一个字节不写。读不出来的号这一趟不参与（不读它、也不写它）。
pub fn sync_with(
    d: &dyn Door,
    home: &str,
    cells: &TrustCells,
    file: &str,
    zero: &str,
    m: &Manifest,
) -> Vec<String> {
    let mut union = BTreeSet::new();
    match read_conf(cells, zero) {
        Ok((_, set)) => union.extend(set),
        Err(e) => tracing::warn!(
            "账号之间同步信任：{zero} 读不出来，这一次不并它：{}",
            e.logged()
        ),
    }
    let seen = read_accounts(home, cells, file, m);
    for s in &seen {
        union.extend(s.trusted.iter().cloned());
    }
    let mut changed = Vec::new();
    for AcctConf {
        name,
        dir,
        path,
        raw,
        ..
    } in seen
    {
        match write_marks(d, home, cells, dir, &path, raw.as_deref(), &union) {
            Ok(Marked::Wrote) => changed.push(name.clone()),
            Ok(Marked::Outside) => tracing::warn!(
                "账号之间同步信任：{name} 号的目录 {dir} 不在账号库里，不写它（需手动改清单）"
            ),
            Ok(Marked::Stale) => {
                tracing::info!(
                    "账号之间同步信任：{name} 号的配置刚被改过，这一次没写（它一改完会再同步一次）"
                )
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("账号之间同步信任：{name} 号这一次没写上：{}", e.logged()),
        }
    }
    changed
}

/// 号目录 `dir` 里那一份配置（`path`）标上 `dirs`：缺的那几格换上，CAS 写回（`expect` = 读到的那一份）。
/// 预标与同步的写都只经这里：号目录不在账号库里（[`scan::in_library_on_disk`]）⇒ [`Marked::Outside`]，一个字节不写。
fn write_marks(
    d: &dyn Door,
    home: &str,
    cells: &TrustCells,
    dir: &str,
    path: &str,
    raw: Option<&str>,
    dirs: &BTreeSet<String>,
) -> Result<Marked, crate::common::said::Said> {
    if !scan::in_library_on_disk(dir, &scan::accts_root(home)) {
        return Ok(Marked::Outside);
    }
    let Some(text) = trust_share::mark(cells, raw.unwrap_or(trust_share::blank(cells)), dirs)?
    else {
        return Ok(Marked::Already);
    };
    let rel = door::rel_under(home, path)?;
    match put_private(d, home, &rel, &text, raw) {
        Ok(()) => Ok(Marked::Wrote),
        Err(Refused::Stale(_)) => Ok(Marked::Stale),
        Err(e) => Err(crate::common::said::Said::from(e.said())),
    }
}

/// 起会话之前：`cwd` 标进 `config_dir` 那个号（必须在清单里），照那个号那一家的信任格与配置文件（[`trust_of_account`]）。
/// 这台没有账号库 / 那一家没有「信任」这件事 ⇒ [`Marked::Nothing`]；不在清单里 ⇒ [`Marked::NotListed`]；
/// 号目录不在账号库里 ⇒ [`Marked::Outside`]（不写；需手动那一件由 [`outside_library`] 出）。
pub fn pretrust(
    d: &dyn Door,
    config_dir: &str,
    cwd: &str,
) -> Result<Marked, crate::common::said::Said> {
    let home = super::scan::home(d)?;
    let _held = lock(&home)?;
    let Some(m) = manifest_in(&home)? else {
        return Ok(Marked::Nothing);
    };
    if listed(&m, config_dir).is_none() {
        return Ok(Marked::NotListed);
    }
    let Some((cells, file)) = trust_of_account(crate::agents::REGISTRY, &m, config_dir) else {
        return Ok(Marked::Nothing);
    };
    pretrust_with(d, &home, &cells, file, &m, config_dir, cwd)
}

/// [`pretrust`] 的本体（同 [`sync_with`]，几格是参数）。被抢先改了 ⇒ 重读再试一次；还不成 ⇒ [`Marked::Stale`]。
pub fn pretrust_with(
    d: &dyn Door,
    home: &str,
    cells: &TrustCells,
    file: &str,
    m: &Manifest,
    config_dir: &str,
    cwd: &str,
) -> Result<Marked, crate::common::said::Said> {
    let Some(Account {
        config_dir: dir, ..
    }) = listed(m, config_dir)
    else {
        return Ok(Marked::NotListed);
    };
    // 库外的那一份连读都不读：读不出来也照样交「不在账号库里」（写那一下 `write_marks` 问的是同一处）。
    if !scan::in_library_on_disk(dir, &scan::accts_root(home)) {
        return Ok(Marked::Outside);
    }
    let dirs: BTreeSet<String> = (cells.dir_keys)(cwd).into_iter().collect();
    let path = join(dir, file);
    for _ in 0..2 {
        let raw = read_text(&path, MAX_CONFIG_BYTES)?;
        match write_marks(d, home, cells, dir, &path, raw.as_deref(), &dirs)? {
            Marked::Stale => continue,
            done => return Ok(done),
        }
    }
    Ok(Marked::Stale)
}
