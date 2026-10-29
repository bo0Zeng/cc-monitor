//! 托管进程从哪个程序跑（A3-3-23 照推荐）。
//!
//! 跑着的程序文件在 Windows 上不能被覆盖；托管进程一直攥着它，装新版时安装器就换不掉 ccm.exe。⇒ 在那样的平台上，托管进程从按版本的
//! 副本跑：`<数据目录>/hosts/.bin/<版本>/ccm.exe`，起托管的那一方没有就把自己复制一份过去；别的版本目录由它顺手删，删不掉（有托管进程
//! 正从那里跑，系统不许删）就留着，等下一次。别的平台上就是自己。
//!
//! 目录名以 `.` 打头：托管 id 不许以 `.` 打头（[`super::home::id_ok`]），名单与陈旧清扫都不会把它当成一个托管终端。
//! 起托管那一下（`control::ccm` 的 `execute_hosted`）调 [`host_program`]。

use crate::platform::fs::{Own, OwnPath, Ro};
use copy_core::said::Said;
use std::path::{Path, PathBuf};

/// `hosts/` 底下放副本的那一层。
const BIN: &str = ".bin";

/// 副本那一份的声明：缓存（没有 · 坏了 ⇒ 起托管时照自己再复制一份；后端不读它，只交给系统去跑）。
const PROGRAM_DOC: crate::common::own_state::Doc<Vec<u8>> =
    crate::common::own_state::Doc::of(&relay_route_core::HOST_PROGRAM_FILE);

/// 副本的文件名（Windows `ccm.exe`）。
fn program_name() -> String {
    format!("ccm{}", std::env::consts::EXE_SUFFIX)
}

/// 托管进程（与它里面那一跳）要跑的那个程序：跑着的程序文件不能被覆盖的平台上是按版本的副本（[`versioned_copy`]，从 `me`
/// —— 本程序自己 —— 复制），别处就是这一趟被叫成的那个词（`called`）。
pub(crate) fn host_program(own: &Own, called: &str, me: &Path) -> Result<String, Said> {
    if crate::platform::child::RUNNING_PROGRAM_IS_LOCKED {
        versioned_copy(own, env!("CARGO_PKG_VERSION"), me).map(|p| p.to_string_lossy().into_owned())
    } else {
        Ok(called.to_string())
    }
}

/// `hosts/.bin/<version>/ccm(.exe)`：没有就把 `me` 复制过去（锁着那个版本目录、先写旁名再换名，几方同时起也不坏）；
/// 已有就用它（不再复制：正跑着的那一份盖不掉，同一个版本也不该变）。然后把别的版本目录删掉，删不掉的留着。
pub(crate) fn versioned_copy(own: &Own, version: &str, me: &Path) -> Result<PathBuf, Said> {
    let hosts = super::home::hosts_dir(own)?;
    let root = hosts.join(BIN)?;
    let dir = root.join(version)?;
    if let Some(top) = own.root() {
        own.own_dir(&top)?;
    }
    own.own_dir(&hosts)?;
    own.own_dir(&root)?;
    own.own_dir(&dir)?;
    let program = dir.join(&program_name())?;
    if Ro::symlink_metadata(program.as_path()).is_err() {
        let held = own.lock(&dir)?;
        if Ro::symlink_metadata(program.as_path()).is_err() {
            own.write_streamed(&program, &held, |out| {
                let mut src = Ro::open(me)?;
                std::io::copy(&mut src, out).map(|_| ())
            })?;
            tracing::info!(
                "托管进程的副本复制好了：{}（{}）",
                program.as_path().display(),
                PROGRAM_DOC.rel
            );
        }
    }
    sweep_other_versions(own, &root, version);
    Ok(program.as_path().to_path_buf())
}

/// 删掉 `root` 底下除 `keep` 以外的版本目录；删不掉（里面的程序正跑着）⇒ 留着，记一行。
fn sweep_other_versions(own: &Own, root: &OwnPath, keep: &str) {
    let Ok(rd) = Ro::read_dir(root.as_path()) else {
        return;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name == keep || !e.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let Ok(old) = root.join(&name) else { continue };
        let cleared = own
            .lock(&old)
            .map_err(|(said, _)| std::io::Error::other(said))
            .and_then(|held| own.clear_dir(&old, &held));
        if let Err(e) = cleared {
            tracing::info!(
                "托管进程的旧副本 {} 这次没删（多半还有托管进程从那里跑）：{e}",
                old.as_path().display()
            );
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/host/bin_tests.rs"]
mod tests;
