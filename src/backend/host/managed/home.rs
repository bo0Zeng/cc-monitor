//! 一个托管终端的家：数据目录底下 `hosts/<id>/`（`0700`），里面 `sock`（托管口）与 `facts.json`（事实）。
//!
//! 活不活只看这个目录的独占锁（同 `own-chan`：托管进程攥着，进程没了内核放锁）——不留陈旧锁、不靠 pid。
//! 锁拿得到 ⇒ 那个托管进程已经没了（机器重启过 · 被杀），目录是陈旧的：不列；起新托管进程的那一方顺手删。
//! 写只经 `Own`：建目录 · 原子写事实 · 收尾删整个目录。

use crate::platform::fs::{Own, OwnLock, OwnPath, Ro};
use copy_core::copy_text;
use copy_core::said::Said;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 数据目录底下那一层（名字住声明表 [`relay_route_core::HOSTS_DIR_REL`]）。
pub(crate) const HOSTS: &str = relay_route_core::file_name_of(relay_route_core::HOSTS_DIR_REL);
pub(crate) const SOCK: &str = "sock";
pub const FACTS: &str = "facts.json";
/// Unix 套接字路径的上限（`sun_path` 108 字节含结尾的 0）。超过 ⇒ 起不来并说清，不偷偷换地方。
pub(crate) const SOCK_PATH_MAX: usize = 107;

/// `facts.json`：托管进程自己写，后端经托管口 `tag` 补 sid。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Facts {
    pub id: String,
    /// 名单里给人看的名（`ccm --attach <名>` 认它）。
    pub name: String,
    /// 托管进程自己的 pid 与起来的时刻（unix 毫秒）。
    pub host_pid: u32,
    pub started_ms: i64,
    /// 伪终端里那个子进程。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_pid: Option<u32>,
    pub cwd: String,
    /// 哪一家（`claude` · `codex` …；起的时候 ccm 交来，后端 `tag` 可改）。
    #[serde(default)]
    pub agent: String,
    /// 谁起的（词同 tmux 那一形的 `@ccm_client`：`ccm` ＝ 用户自己的终端 · `desktop` · `mobile`；空 ＝ 没声明），门按它判（`gate::identity_of`）。
    #[serde(default)]
    pub client: String,
    /// 后端写来的会话 id。
    #[serde(default)]
    pub sid: String,
    /// 起它时点名的号（`--account`；空 ＝ 没点名，照缺省号）。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub account: String,
    /// 此刻有没有真终端接着（瘦客户端接上 · 断开时托管进程改写）。
    #[serde(default)]
    pub attached: bool,
}

/// 句柄 · 名只许这些字（进路径、进名单、进命令行都不用转义）。
pub fn id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        && !id.starts_with('.')
}

/// `hosts/` 那一层。
pub(crate) fn hosts_dir(own: &Own) -> Result<OwnPath, Said> {
    own.path(HOSTS)
}

/// 一个托管终端的目录（不建）。
pub fn dir_of(own: &Own, id: &str) -> Result<OwnPath, Said> {
    if !id_ok(id) {
        return Err(copy_text("beHost.home.badId", &[("id", id)]).into());
    }
    hosts_dir(own)?.join(id)
}

/// 它的套接字路径；超过上限 ⇒ 拒，说清路径多长。上限只在通道是盘上一个文件的平台上有（Windows 命名管道的名字由
/// `own-chan` 自己判长短，绑那一下照实回）。
pub fn sock_of(own: &Own, id: &str) -> Result<PathBuf, Said> {
    let p = dir_of(own, id)?.as_path().join(SOCK);
    let n = p.as_os_str().len();
    if own_chan::BOUND_AS_FILE && n > SOCK_PATH_MAX {
        return Err(copy_text(
            "beHost.home.sockTooLong",
            &[
                ("path", &p.display().to_string()),
                ("len", &n.to_string()),
                ("max", &SOCK_PATH_MAX.to_string()),
            ],
        )
        .into());
    }
    Ok(p)
}

/// 托管进程攥着的那个家。
pub struct Home {
    own: Own,
    dir: OwnPath,
    lock: OwnLock,
    sock: PathBuf,
}

// SAFETY：家的锁只经 `Own::try_lock` 拿（[`Home::claim`]）：那一种锁是「有名对象在不在」（Unix 目录 `flock` · Windows 有名互斥体
// 建没建成），不认线程，在哪条线程上落地都放得对。`OwnLock` 本身在 Windows 上不是 `Send`，是因为它的另一种（`Own::lock`，等着拿的
// 互斥量）归拿它的那条线程所有 —— 家不用那一种。托管进程的家在收连接 · 读伪终端 · 收尾几条线程之间共用。
unsafe impl Send for Home {}

impl Home {
    /// 占一个家：建 `hosts/` 与 `hosts/<id>/`（0700，本人的真目录），抢它的锁；抢不到 ⇒ 这个 id 有人在用。
    /// 上一个托管进程没收尾就没了时留下的套接字与事实删掉：锁在这一个手里，那两份只可能是上一个的 ⇒ 托管口重绑、事实照写。
    pub fn claim(own: &Own, id: &str) -> Result<Home, Said> {
        let hosts = hosts_dir(own)?;
        if let Some(root) = own.root() {
            own.own_dir(&root)?;
        }
        own.own_dir(&hosts)?;
        let dir = dir_of(own, id)?;
        let sock = sock_of(own, id)?;
        own.own_dir(&dir)?;
        let lock = match own.try_lock(&dir) {
            Ok(Some(l)) => l,
            Ok(None) => return Err(copy_text("beHost.home.taken", &[("id", id)]).into()),
            Err(e) => {
                return Err(Said::with_raw(
                    copy_text("beHost.home.lockFailed", &[("id", id)]),
                    &e,
                ))
            }
        };
        for name in [SOCK, FACTS] {
            let stale = dir.join(name)?;
            if Ro::symlink_metadata(stale.as_path()).is_ok() {
                if let Err(e) = own.delete_file(&stale) {
                    // 删不掉 ⇒ 下一步绑托管口 · 写事实那一下照实报起不来。
                    tracing::warn!(
                        "托管终端：上一个留下的 {} 没删掉：{e}",
                        stale.as_path().display()
                    );
                }
            }
        }
        Ok(Home {
            own: own.clone(),
            dir,
            lock,
            sock,
        })
    }

    pub fn sock(&self) -> &Path {
        &self.sock
    }

    /// 整份写事实。
    pub fn write_facts(&self, f: &Facts) -> Result<(), Said> {
        FACTS_DOC.write_named(&self.own, &self.dir, FACTS, &self.lock, f)
    }

    /// 收尾：删整个目录（锁随之放）。
    pub fn clear(self) {
        if let Err(e) = self.own.clear_dir(&self.dir, &self.lock) {
            tracing::warn!(
                "托管终端收尾：删 {} 没删成：{e}",
                self.dir.as_path().display()
            );
        }
    }
}

/// `facts.json` 的声明：状态（托管进程活着时它是那个终端的事实；读不出来 · 超了读上限 ⇒ [`read_facts`] 说是哪一个、
/// 这个终端不列，不覆盖）。落点 · 版本 · 读上限照表；只有紧跟着的 [`read_facts`] 读它。
const FACTS_DOC: crate::common::own_state::Doc<Facts> =
    crate::common::own_state::Doc::of(&relay_route_core::HOST_FACTS_FILE);

/// 读一个托管终端目录里的事实（读不懂 ⇒ `None`，`warn!` 带目录）。
pub fn read_facts(dir: &Path) -> Option<Facts> {
    match FACTS_DOC.read(&dir.join(FACTS)) {
        crate::common::own_state::Read::Present(f) => Some(f),
        crate::common::own_state::Read::Absent => None,
        crate::common::own_state::Read::Unreadable(why) => {
            tracing::warn!(
                "托管终端 {}：事实文件读不出来，不列：{}",
                dir.display(),
                why.logged()
            );
            None
        }
    }
}

/// 这台此刻活着的托管终端（锁有人攥着的那几个），按起来的先后排。陈旧的不列、也不在这里删（写只在起托管的那一处）。
pub fn alive(own: &Own) -> Vec<Facts> {
    let Ok(hosts) = hosts_dir(own) else {
        return Vec::new();
    };
    let Ok(rd) = Ro::read_dir(hosts.as_path()) else {
        return Vec::new();
    };
    let mut v: Vec<Facts> = Vec::new();
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !id_ok(&name) || !e.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let Ok(dir) = hosts.join(&name) else { continue };
        // 拿得到锁 ⇒ 没人攥着 ⇒ 陈旧；拿到的那一把当场放掉。
        match own.try_lock(&dir) {
            Ok(None) => {}
            _ => continue,
        }
        if let Some(f) = read_facts(dir.as_path()) {
            if f.id == name {
                v.push(f);
            }
        }
    }
    v.sort_by(|a, b| a.started_ms.cmp(&b.started_ms).then(a.id.cmp(&b.id)));
    v
}

/// 删掉陈旧的家（机器重启过 · 托管进程被杀：目录与事实还在、锁没人攥）。只由起托管进程的那一方调（写只在托管那一处）。
///
/// 只删有事实的：正在占家的那一方先建目录、再抢锁、起了子进程才写事实 ⇒ 没事实的目录可能正被占着，不碰；
/// 有事实却拿得到锁的，那个托管进程已经没了。
pub(crate) fn sweep_stale(own: &Own) {
    let Ok(hosts) = hosts_dir(own) else { return };
    let Ok(rd) = Ro::read_dir(hosts.as_path()) else {
        return;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !id_ok(&name) || !e.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let Ok(dir) = hosts.join(&name) else { continue };
        if Ro::symlink_metadata(dir.as_path().join(FACTS)).is_err() {
            continue;
        }
        let Ok(Some(lock)) = own.try_lock(&dir) else {
            continue;
        };
        match own.clear_dir(&dir, &lock) {
            Ok(()) => tracing::info!("托管终端：删了陈旧的 {}", dir.as_path().display()),
            Err(e) => tracing::warn!("托管终端：陈旧的 {} 没删成：{e}", dir.as_path().display()),
        }
    }
}

/// `ccm -- --hosts`（`cch ls`）印的那几行：[`alive`] 的一个一行（名字 · 目录 · 账号 · 起的时刻 · 有没有人接着）；一个都没有 ⇒ 一句。
/// `now` 是此刻（unix 秒），时刻按 `tz` 写（`common::time::fmt_at`）。
pub fn listing(all: &[Facts], now: i64, tz: &crate::common::time::Tz) -> String {
    if all.is_empty() {
        return copy_text("beHost.hosts.none", &[]) + "\n";
    }
    let default_account = copy_text("beHost.hosts.defaultAccount", &[]);
    let (on, off) = (
        copy_text("beHost.hosts.attached", &[]),
        copy_text("beHost.hosts.detached", &[]),
    );
    all.iter()
        .map(|f| {
            let at = crate::common::time::fmt_at(f.started_ms.div_euclid(1_000), now, tz);
            let account = if f.account.is_empty() {
                &default_account
            } else {
                &f.account
            };
            copy_text(
                "beHost.hosts.row",
                &[
                    ("name", &f.name),
                    ("cwd", &f.cwd),
                    ("account", account),
                    ("at", &at),
                    ("attached", if f.attached { &on } else { &off }),
                ],
            ) + "\n"
        })
        .collect()
}

/// 按名或句柄认一个活着的托管终端（`ccm --attach <名|句柄>`）。
pub fn find(own: &Own, name_or_id: &str) -> Option<Facts> {
    let all = alive(own);
    all.iter()
        .find(|f| f.id == name_or_id)
        .or_else(|| all.iter().find(|f| f.name == name_or_id))
        .cloned()
}

#[cfg(test)]
#[path = "../../../../tests/backend/host/home_tests.rs"]
mod tests;
