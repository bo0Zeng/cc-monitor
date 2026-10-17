//! `--place-self`：新传上去的那一份后端自己换进落点；`--unplace-self`：卸载那一趟传上去的那一份删掉落点那一份、再删自己。
//!
//! 部署那一趟（`deploy` crate）经 SFTP 只把字节放进暂存区（`~/.cc-monitor/staging/<名>`），再在那台上跑这份暂存件的这条子命令：
//! 它读自己的字节、自报的身份是这一版才往下，再 `rename(2)` 盖掉落点（`~/.cc-monitor/bin/ccm`；同在 `~/.cc-monitor` 下、同一块盘
//! ⇒ 原子，落点在换的全程都在）。回一行 `{"placed":true,"build_id":…}`；不对 ⇒ 失败信封（`not_self` · `outside_staging` · `place_failed`），
//! 自报不是这一版的那份暂存件当场删。
//! 卸载同一条路（SFTP 只写暂存区）：暂存件删落点那一份（不认身份：卸载就是明确授权）再删自己，回一行 `{"removed":true|false}`。

use crate::platform::fs::{Own, OwnPath, Ro};
use copy_core::copy_text;
use copy_core::said::Said;
use std::path::Path;

/// 复制详情「命令」那一项里的名字。
const CMD: &str = "place-self";

/// 复制详情「命令」那一项里卸载那一条的名字。
const UNPLACE_CMD: &str = "unplace-self";

/// 失败：码 ＋ 那一句。
type Failed = (&'static str, Said);

/// 这一份自己（`current_exe()`）；问不到 ⇒ `place_failed`。
fn this_exe() -> Result<std::path::PathBuf, Failed> {
    std::env::current_exe().map_err(|e| {
        (
            "place_failed",
            Said::with_raw(copy_text("beResident.spawn.noSelf", &[]), &e),
        )
    })
}

/// `--place-self` 的入口。
pub fn run(own: &Own) -> i32 {
    match this_exe().and_then(|exe| place(own, &exe, crate::BUILD_ID)) {
        Ok(()) => {
            println!(
                "{}",
                serde_json::json!({ "placed": true, "build_id": crate::BUILD_ID })
            );
            0
        }
        Err((code, said)) => crate::wire::detail::emit_err(CMD, code, said),
    }
}

/// `--unplace-self` 的入口。
pub fn run_unplace(own: &Own) -> i32 {
    match this_exe().and_then(|exe| unplace(own, &exe)) {
        Ok(removed) => {
            println!("{}", serde_json::json!({ "removed": removed }));
            0
        }
        Err((code, said)) => crate::wire::detail::emit_err(UNPLACE_CMD, code, said),
    }
}

/// `exe` 恰在暂存区里 ⇒ 暂存区（`Own` 发的那一条）＋ 它的名字；不在 ⇒ `outside_staging`（不是部署那一趟放上来的，一个字节不动）。
/// 两边都解开再比：暂存区本身是不是链接由远端写围栏管。
fn staged_self<'a>(own: &Own, exe: &'a Path) -> Result<(OwnPath, &'a str), Failed> {
    let staging = own
        .at_home(relay_route_core::STAGING_DIR_REL)
        .map_err(|s| ("outside_staging", s))?;
    let inside = exe.parent().and_then(|d| Ro::canonicalize(d).ok())
        == Ro::canonicalize(staging.as_path()).ok();
    match (inside, exe.file_name().and_then(|n| n.to_str())) {
        (true, Some(name)) => Ok((staging, name)),
        _ => Err((
            "outside_staging",
            copy_text(
                "bePlaceSelf.place.outsideStaging",
                &[("path", &exe.display().to_string())],
            )
            .into(),
        )),
    }
}

/// 落点那一下没成 ⇒ `place_failed`（那一句只有路径与原因词，系统原话进 `raw`）。
fn io_failed(shown: &str, e: std::io::Error) -> Failed {
    (
        "place_failed",
        Said::with_raw(
            copy_text(
                "bePlaceSelf.place.failed",
                &[("path", shown), ("why", &copy_core::io_reason(e.kind()))],
            ),
            &e,
        ),
    )
}

/// 卸载的本体：暂存区里的这一份删掉落点那一份（不在 ⇒ `false`）、再删自己。
pub(crate) fn unplace(own: &Own, exe: &Path) -> Result<bool, Failed> {
    let (staging, name) = staged_self(own, exe)?;
    let removed = own
        .clear_landing()
        .map_err(|e| io_failed(relay_route_core::BACKEND_LANDING_REL, e))?;
    if let Ok(p) = staging.join(name) {
        let _ = own.delete_file(&p);
    }
    Ok(removed)
}

/// 本体：`exe` 是这一份自己（生产 = `current_exe()`）· `mine` 是这一版自报的身份。
pub(crate) fn place(own: &Own, exe: &Path, mine: &str) -> Result<(), Failed> {
    let shown = exe.display().to_string();
    let (staging, name) = staged_self(own, exe)?;
    let io = |e: std::io::Error| io_failed(&shown, e);
    let bytes = Ro::read(exe).map_err(io)?;
    if deploy_contract::identity_of_bytes(&bytes, deploy::MARKS)
        != deploy_contract::RemoteIdentity::Stamp(mine.to_string())
    {
        // 自报的不是这一版（传坏了 · 被换过）⇒ 留着没用：删掉，下次那一趟重传。
        if let Ok(p) = staging.join(name) {
            let _ = own.delete_file(&p);
        }
        return Err((
            "not_self",
            copy_text(
                "bePlaceSelf.place.notSelf",
                &[("path", &shown), ("mine", mine)],
            )
            .into(),
        ));
    }
    own.place_staged(name).map_err(io)
}

#[cfg(test)]
#[path = "../../../tests/backend/control/place_self_tests.rs"]
mod place_self_tests;
