//! `--place-self`：新传上去的那一份后端自己换进落点。
//!
//! 部署那一趟（`deploy` crate）经 SFTP 只把字节放进暂存区（`~/.cc-monitor/staging/<名>`），再在那台上跑这份暂存件的这条子命令：
//! 它读自己的字节、自报的身份是这一版才往下，再 `rename(2)` 盖掉落点（`~/.cc-monitor/bin/ccm`；同在 `~/.cc-monitor` 下、同一块盘
//! ⇒ 原子，落点在换的全程都在）。回一行 `{"placed":true,"build_id":…}`；不对 ⇒ 失败信封（`not_self` · `outside_staging` · `place_failed`），
//! 自报不是这一版的那份暂存件当场删。

use crate::platform::fs::{Own, Ro};
use copy_core::copy_text;
use copy_core::said::Said;
use std::path::Path;

/// 复制详情「命令」那一项里的名字。
const CMD: &str = "place-self";

/// `--place-self` 的入口。
pub fn run(own: &Own) -> i32 {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            return crate::wire::detail::emit_err(
                CMD,
                "place_failed",
                Said::with_raw(copy_text("beResident.spawn.noSelf", &[]), &e),
            )
        }
    };
    match place(own, &exe, crate::BUILD_ID) {
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

/// 本体：`exe` 是这一份自己（生产 = `current_exe()`）· `mine` 是这一版自报的身份。
pub(crate) fn place(own: &Own, exe: &Path, mine: &str) -> Result<(), (&'static str, Said)> {
    let shown = exe.display().to_string();
    // 恰在暂存区里（两边都解开再比：暂存区本身是不是链接由远端写围栏管）；不在 ⇒ 不是部署那一趟放上来的，一个字节不动。
    let staging = own
        .at_home(relay_route_core::STAGING_DIR_REL)
        .map_err(|s| ("outside_staging", s))?;
    let inside = exe.parent().and_then(|d| Ro::canonicalize(d).ok())
        == Ro::canonicalize(staging.as_path()).ok();
    let name = exe.file_name().and_then(|n| n.to_str());
    let (true, Some(name)) = (inside, name) else {
        return Err((
            "outside_staging",
            copy_text("bePlaceSelf.place.outsideStaging", &[("path", &shown)]).into(),
        ));
    };
    let io = |e: std::io::Error| {
        (
            "place_failed",
            Said::with_raw(
                copy_text(
                    "bePlaceSelf.place.failed",
                    &[("path", &shown), ("why", &copy_core::io_reason(e.kind()))],
                ),
                &e,
            ),
        )
    };
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
