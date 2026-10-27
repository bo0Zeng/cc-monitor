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

/// 数据目录底下那一层。
pub(crate) const HOSTS: &str = "hosts";
pub(crate) const SOCK: &str = "sock";
pub(crate) const FACTS: &str = "facts.json";
/// Unix 套接字路径的上限（`sun_path` 108 字节含结尾的 0）。超过 ⇒ 起不来并说清，不偷偷换地方。
pub(crate) const SOCK_PATH_MAX: usize = 107;
/// 事实文件读的上限（几百字节的东西，大了就不是我们写的）。
const FACTS_CAP: u64 = 64 * 1024;
/// `facts.json` 的声明：状态（托管进程活着时它是那个终端的事实；读不出来 ⇒ 这个终端不列，不覆盖）。
const FACTS_DOC: crate::common::own_state::Doc<Facts> =
    crate::common::own_state::Doc::state(".cc-monitor/hosts/<id>/facts.json", FACTS_CAP);

/// `facts.json`：托管进程自己写，后端经托管口 `tag` 补 sid。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) struct Facts {
    pub(crate) id: String,
    /// 名单里给人看的名（`ccm --attach <名>` 认它）。
    pub(crate) name: String,
    /// 托管进程自己的 pid 与起来的时刻（unix 毫秒）。
    pub(crate) host_pid: u32,
    pub(crate) started_ms: i64,
    /// 伪终端里那个子进程。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) child_pid: Option<u32>,
    pub(crate) cwd: String,
    /// 哪一家（`claude` · `codex` …；起的时候 ccm 交来，后端 `tag` 可改）。
    #[serde(default)]
    pub(crate) agent: String,
    /// 谁起的（`terminal` · `desktop` · `mobile`），门按它判。
    #[serde(default)]
    pub(crate) client: String,
    /// 后端写来的会话 id。
    #[serde(default)]
    pub(crate) sid: String,
}

/// 句柄 · 名只许这些字（进路径、进名单、进命令行都不用转义）。
pub(crate) fn id_ok(id: &str) -> bool {
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
pub(crate) fn dir_of(own: &Own, id: &str) -> Result<OwnPath, Said> {
    if !id_ok(id) {
        return Err(copy_text("beHost.home.badId", &[("id", id)]).into());
    }
    hosts_dir(own)?.join(id)
}

/// 它的套接字路径；超过上限 ⇒ 拒，说清路径多长。
pub(crate) fn sock_of(own: &Own, id: &str) -> Result<PathBuf, Said> {
    let p = dir_of(own, id)?.as_path().join(SOCK);
    let n = p.as_os_str().len();
    if n > SOCK_PATH_MAX {
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
pub(crate) struct Home {
    own: Own,
    dir: OwnPath,
    lock: OwnLock,
    sock: PathBuf,
}

impl Home {
    /// 占一个家：建 `hosts/` 与 `hosts/<id>/`（0700，本人的真目录），抢它的锁；抢不到 ⇒ 这个 id 有人在用。
    /// 陈旧的套接字文件（上一个托管进程没收尾就没了）删掉，好让托管口重绑。
    pub(crate) fn claim(own: &Own, id: &str) -> Result<Home, Said> {
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
        let stale = dir.join(SOCK)?;
        if Ro::symlink_metadata(stale.as_path()).is_ok() {
            let _ = own.delete_file(&stale);
        }
        Ok(Home {
            own: own.clone(),
            dir,
            lock,
            sock,
        })
    }

    pub(crate) fn sock(&self) -> &Path {
        &self.sock
    }

    /// 整份写事实。
    pub(crate) fn write_facts(&self, f: &Facts) -> Result<(), Said> {
        let path = self.dir.join(FACTS)?;
        let body = crate::common::own_state::json_body(path.as_path(), f)?;
        self.own.write(&path, &self.lock, &body)
    }

    /// 收尾：删整个目录（锁随之放）。
    pub(crate) fn clear(self) {
        if let Err(e) = self.own.clear_dir(&self.dir, &self.lock) {
            tracing::warn!(
                "托管终端收尾：删 {} 没删成：{e}",
                self.dir.as_path().display()
            );
        }
    }
}

/// 读一个托管终端目录里的事实（读不懂 ⇒ `None`）。
pub(crate) fn read_facts(dir: &Path) -> Option<Facts> {
    match FACTS_DOC.read(&dir.join(FACTS)) {
        crate::common::own_state::Read::Present(f) => Some(f),
        _ => None,
    }
}

/// 这台此刻活着的托管终端（锁有人攥着的那几个），按起来的先后排。陈旧的不列、也不在这里删（写只在起托管的那一处）。
pub(crate) fn alive(own: &Own) -> Vec<Facts> {
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

/// 按名或句柄认一个活着的托管终端（`ccm --attach <名|句柄>`）。
pub(crate) fn find(own: &Own, name_or_id: &str) -> Option<Facts> {
    let all = alive(own);
    all.iter()
        .find(|f| f.id == name_or_id)
        .or_else(|| all.iter().find(|f| f.name == name_or_id))
        .cloned()
}
