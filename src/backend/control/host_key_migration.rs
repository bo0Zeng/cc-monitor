//! **一次换形，下一版整份删**：机器表每台那一格固化的主机指纹（`hostKeyFingerprint` · `hostKeyPinnedAt`）换进拨号核心那份库
//! （`dial/known_hosts.rs`），机器表里删掉那两格。只由常驻那一位在起来时做（[`migrate_here`]）。
//!
//! 指纹还原不出整把钥匙 ⇒ 逐台：
//! 1. 库里这台有行、某一把的指纹与它相等 ⇒ 那一格直接删；
//! 2. 库里这台有行、都对不上 ⇒ 那几行删（不认它们），指纹写成这台待核的那一行；没行 ⇒ 同样写成待核；
//! 3. 拨号时这台没记过钥匙、有待核的指纹 ⇒ 按它严格比，鉴权过了记整把、待核那一行随之去掉（`ssh_dial::known_hosts`）。
//!
//! 先写库、再写机器表，写完两份都读回比对；任一步没成 ⇒ 机器表那两格留着，下次起来再做（每一步重做都不变样）。
//! 下一版：本文件与拨号核心里待核的那一支一起删；一版里一次都没连过的机器到时变成首见。

use super::machines;
use crate::platform::Own;
use serde_json::Value;
use ssh_dial::known_hosts as kh;
use ssh_dial::Endpoint;

/// 旧那两格。
const OLD_CELLS: [&str; 2] = ["hostKeyFingerprint", "hostKeyPinnedAt"];

/// 一台旧形状的那一行 ⇒ (全部地址, 固化的指纹)。没有指纹 ⇒ `None`。
fn pinned_of(row: &Value) -> Option<(Vec<Endpoint>, String)> {
    let s = |k: &str| row.get(k).and_then(Value::as_str);
    let fp = s("hostKeyFingerprint")
        .map(str::trim)
        .filter(|f| !f.is_empty())?;
    let m = ssh_dial::machine::Machine::of(ssh_dial::machine::Fields {
        host: s("host"),
        user: s("user").or(Some("-")),
        port: row
            .get("port")
            .and_then(Value::as_u64)
            .and_then(|p| u16::try_from(p).ok()),
        addresses: row
            .get("addresses")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        ..Default::default()
    })
    .ok()?;
    let addrs = m
        .endpoints()
        .into_iter()
        .map(|(host, port)| Endpoint { host, port })
        .collect();
    Some((addrs, fp.to_string()))
}

/// 库的一份文本按机器表那几台换形（第 1、2 步）。**纯函数**。
pub(crate) fn library_after(text: &str, pinned: &[(Vec<Endpoint>, String)]) -> String {
    let mut t = text.to_string();
    for (addrs, fp) in pinned {
        let known = kh::known(&t, addrs);
        if known.iter().any(|k| kh::fingerprint(k) == *fp) {
            continue;
        }
        if let Some(next) = kh::forgotten(&t, addrs) {
            t = next;
        }
        if let Some(next) = kh::with_pending(&t, addrs, fp) {
            t = next;
        }
    }
    t
}

/// 读回比对：每台要么库里有一把指纹相等，要么待核的就是它。
pub(crate) fn library_holds(text: &str, pinned: &[(Vec<Endpoint>, String)]) -> bool {
    pinned.iter().all(|(addrs, fp)| {
        kh::known(text, addrs)
            .iter()
            .any(|k| kh::fingerprint(k) == *fp)
            || kh::pending(text, addrs).as_deref() == Some(fp.as_str())
    })
}

/// 结局。
#[derive(Debug, PartialEq)]
pub(crate) enum Moved {
    /// 机器表里没有旧那两格（换过了 / 本来就没有）。
    Nothing,
    /// 换了：这几台有指纹。
    Done(usize),
    /// 没成，下次再做（那一句）。
    Later(String),
}

/// 本体（写句柄与库的那只手是参数）。
pub(crate) fn migrate_in(own: &Own, library: &crate::dial::known_hosts::Writer) -> Moved {
    use crate::common::own_state::{read_json, Read};
    let path = match own.home_dir().and_then(|d| d.join(machines::FILE_NAME)) {
        Ok(p) => p.as_path().to_path_buf(),
        Err(e) => return Moved::Later(e.logged()),
    };
    let doc: Value = match read_json(&path, 4 << 20) {
        Read::Present(v) => v,
        Read::Absent => return Moved::Nothing,
        Read::Unreadable(why) => return Moved::Later(why.logged()),
    };
    let rows: Vec<Value> = doc
        .get("hosts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !rows
        .iter()
        .any(|r| OLD_CELLS.iter().any(|c| r.get(c).is_some()))
    {
        return Moved::Nothing;
    }
    let pinned: Vec<(Vec<Endpoint>, String)> = rows.iter().filter_map(pinned_of).collect();
    let mut after = String::new();
    if let Err(e) = library.rewrite(&mut |t| {
        let next = library_after(t, &pinned);
        after = next.clone();
        (next != t).then_some(next)
    }) {
        return Moved::Later(e.logged());
    }
    if !library_holds(&after, &pinned) {
        return Moved::Later("known_hosts read back without every pinned host".into());
    }
    let hosts: Result<Vec<machines::Host>, _> = rows
        .into_iter()
        .map(|mut r| {
            if let Some(o) = r.as_object_mut() {
                for c in OLD_CELLS {
                    o.remove(c);
                }
            }
            serde_json::from_value::<machines::Host>(r)
        })
        .collect();
    let hosts = match hosts {
        Ok(h) => h,
        Err(e) => return Moved::Later(e.to_string()),
    };
    if let Err(e) = machines::write(own, &hosts) {
        return Moved::Later(e.logged());
    }
    match machines::read_at(&path) {
        machines::Loaded::Present(back) if back == hosts => Moved::Done(pinned.len()),
        other => Moved::Later(format!("machines.json read back as {other:?}")),
    }
}

/// 常驻那一位起来时调一次。
pub fn migrate_here(own: &Own) {
    let Some(library) = crate::dial::known_hosts::installed() else {
        tracing::warn!("主机指纹换形：库的那只手没装，这一次不换");
        return;
    };
    match migrate_in(own, &library) {
        Moved::Nothing => {}
        Moved::Done(n) => tracing::info!("机器表里的主机指纹换进了 known_hosts（{n} 台有指纹）"),
        Moved::Later(why) => tracing::warn!("机器表里的主机指纹没换成，下次起来再试：{why}"),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/host_key_migration_tests.rs"]
mod tests;
