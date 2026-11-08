//! **机器表的读法**：这一端连各台用的那张表（`~/.cc-monitor/machines.json`，[`relay_route_core::MACHINES_REL`]）怎么读。
//! 读法只这一份：拨号层（常驻流 · 问那台 · 测试连接 · 开终端）与控制面（传输 · 部署 · 机器菜单 · `machines-read`）都经它；
//! 写的那一半（补丁 · 锁内现读现判 · 写完响一下）住 `control/machines.rs`。
//!
//! - 读不懂 · 比这一版新 ⇒ 照空表办（落点 · 上限 · 版本照声明表，[`DOC`]）；怎么报归 `machines-read`。
//! - 每次都真去读盘：没有缓存，也不用谁先装一份读法。

use crate::common::said::Said;
use copy_core::copy_text;
use ssh_dial::machine::Host;
use std::path::{Path, PathBuf};

/// 机器表：状态（读不懂 · 比这一版新 ⇒ 不覆盖；落点 · 上限 · 版本照声明表）。
pub const DOC: crate::common::own_state::Doc<OnDisk> =
    crate::common::own_state::Doc::of(&relay_route_core::MACHINES_FILE);

/// 一张表至多几台。
pub const HOSTS_MAX: usize = 256;

/// 盘上整份（顶层 `version` 归读写口）。
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OnDisk {
    pub hosts: Vec<Host>,
}

/// 现读一次的结果。
#[derive(Debug, Clone, PartialEq)]
pub enum Loaded {
    Absent,
    Present(Vec<Host>),
    /// 文件在、读不懂（那一句 ＋ 原话）。
    Unreadable(Said),
}

impl Loaded {
    /// 照用的那一张：读不懂 / 没有 ⇒ 空表。
    pub fn hosts(&self) -> Vec<Host> {
        match self {
            Loaded::Present(h) => h.clone(),
            Loaded::Absent | Loaded::Unreadable(_) => Vec::new(),
        }
    }
}

/// 某个家目录底下那份文件。
pub fn path_in(home: &Path) -> PathBuf {
    home.join(DOC.rel)
}

/// 这一端的机器表此刻那一张（现读；家目录说不出 · 没有 · 读不懂 ⇒ 空表）。
pub fn machines_now() -> Vec<Host> {
    crate::platform::paths::home_dir()
        .map(|h| read_at(&path_in(&h)).hosts())
        .unwrap_or_default()
}

fn not_ours(path: &Path, why: String) -> Said {
    Said::with_raw(
        copy_text(
            "beOwnState.read.notJson",
            &[("path", &path.display().to_string())],
        ),
        why,
    )
}

/// 读一次 `path`。每次都真去读盘。
pub fn read_at(path: &Path) -> Loaded {
    use crate::common::own_state::Read;
    match DOC.read(path) {
        Read::Absent => Loaded::Absent,
        Read::Unreadable(why) => Loaded::Unreadable(why),
        Read::Present(d) if d.hosts.len() > HOSTS_MAX => Loaded::Unreadable(not_ours(
            path,
            format!("{} hosts (at most {HOSTS_MAX})", d.hosts.len()),
        )),
        Read::Present(d) => Loaded::Present(d.hosts),
    }
}
