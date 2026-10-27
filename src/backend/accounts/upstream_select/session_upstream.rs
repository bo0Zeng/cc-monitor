//! **上游描述**：一条会话一份，按来处（`ccm` 起会话时铸、拼在中转地址尾上的那一段）存。
//!
//! 这一版只有出网那两格（出网代理 · 免代理表）：`ccm` 起会话那一刻 agent 继承来的、那一家认的出网设置变量原样（名, 值）。
//! 读回来经 [`egress::Egress::from_env`] 读成出网描述 —— 怎么读代理 URL · 免代理表只在出网口一处。
//! 上游地址 · 凭据两格（用户自己的上游 · 换号）以后并进同一份。没有描述 ⇒ 调用方照这台的出网描述。
//!
//! 住上游选择：写的 `ccm`（control）与读的中转宿主（relay，读的那只手由 [`super::host_relay`] 递进去）都只够得到这一层。
//!
//! 落点：起会话便条那个目录底下的 `upstream/<来处>.json`（那个目录登记为缓存；便条那一侧只认数字名的文件）。
//! 带起 agent 那个进程的 pid：写下一份的那一下顺手清掉进程已不在的那几份（不起定时器）。

use crate::common::said::Said;
use crate::platform::fs::Own;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 一条会话的上游描述。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct UpstreamDesc {
    /// agent 继承来的出网设置变量（名, 值），原样。可能带代理鉴权：只落在本人 0700 的目录里，不进日志。
    pub(crate) egress: Vec<(String, String)>,
    /// 起 agent 的那个进程。
    pub(crate) pid: u32,
    /// 写下的时刻（unix 秒）。
    pub(crate) at: u64,
}

impl UpstreamDesc {
    /// 读成出网描述。
    pub(crate) fn egress(&self) -> egress::Egress {
        egress::Egress::from_env(&|k| {
            self.egress
                .iter()
                .find(|(n, _)| n == k)
                .map(|(_, v)| v.clone())
        })
    }
}

/// 子目录名（便条目录底下）。
const SUB: &str = "upstream";

/// 一份描述的读盘上限（几个变量）。
const DESC_MAX_BYTES: u64 = 64 << 10;

/// 一条会话一份：状态（读不懂的那份不作数）。
const DESC: crate::common::own_state::Doc<UpstreamDesc> = crate::common::own_state::Doc::state(
    ".cc-monitor/launch-pending/upstream/<origin>.json",
    DESC_MAX_BYTES,
);

/// 便条目录名（落点的上一层）。
const NOTES_DIR: &str = relay_route_core::file_name_of(relay_route_core::LAUNCH_NOTES_DIR_REL);

/// 留下来处 `origin` 那条会话的描述；顺手清掉 `alive` 答不在的进程的那几份。来处不过段闸 · 没有数据目录 ⇒ 拒。
pub(crate) fn leave(
    own: &Own,
    origin: &str,
    desc: &UpstreamDesc,
    alive: &dyn Fn(u32) -> bool,
) -> Result<(), Said> {
    let name = file_name(origin)?;
    let data = own
        .root()
        .ok_or_else(|| Said::from(copy_core::copy_text("beOwnState.path.noHome", &[])))?;
    let notes = data.join(NOTES_DIR)?;
    let dir = notes.join(SUB)?;
    for d in [&data, &notes, &dir] {
        own.ensure_dir(d).map_err(|e| failed(d.as_path(), &e))?;
    }
    let held = own.lock(&dir)?;
    // 进程已不在的那几份（读不懂的也算）清掉。
    if let Ok(rd) = crate::platform::fs::Ro::read_dir(dir.as_path()) {
        for ent in rd.flatten() {
            let path = ent.path();
            let stale = match DESC.read(&path) {
                crate::common::own_state::Read::Present(d) => !alive(d.pid),
                _ => true,
            };
            if let (true, Some(f)) = (
                stale,
                path.file_name()
                    .and_then(|n| n.to_str())
                    .and_then(|n| dir.join(n).ok()),
            ) {
                let _ = own.delete_file(&f);
            }
        }
    }
    let file = dir.join(&name)?;
    let body = crate::common::own_state::json_body(file.as_path(), desc)?;
    own.write(&file, &held, &body)
}

/// 来处 `origin` 那条会话的描述（`data` ＝ 数据目录）。没有 · 读不懂 · 来处不过段闸 ⇒ `None`。
pub(crate) fn of(data: &Path, origin: &str) -> Option<UpstreamDesc> {
    let f = data.join(NOTES_DIR).join(SUB).join(file_name(origin).ok()?);
    match DESC.read(&f) {
        crate::common::own_state::Read::Present(d) => Some(d),
        _ => None,
    }
}

/// 中转开上游那一口要的那只手：来处 ⇒ 这条会话的出网描述（数据目录取自 `own`；没有描述 ⇒ `None`，中转照这台的）。
pub(crate) fn egress_of(own: &Own) -> crate::relay::EgressOf {
    let data = own.root().map(|p| p.as_path().to_path_buf());
    std::sync::Arc::new(move |origin: &str| {
        data.as_deref()
            .and_then(|d| of(d, origin))
            .map(|d| d.egress())
    })
}

/// 来处 ⇒ 文件名；不过段闸 ⇒ 拒（不让一段来处拼出目录外的路径）。
fn file_name(origin: &str) -> Result<String, Said> {
    if relay_route_core::segment_is_safe(origin) {
        Ok(format!("{origin}.json"))
    } else {
        Err(copy_core::copy_text("beOwnState.path.notRelative", &[("rel", origin)]).into())
    }
}

fn failed(p: &Path, e: &std::io::Error) -> Said {
    Said::with_raw(
        copy_core::copy_text(
            "reason.fs.writeFailed",
            &[
                ("path", &p.display().to_string()),
                ("why", &copy_core::io_reason(e.kind())),
            ],
        ),
        e,
    )
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/upstream_select/session_upstream_tests.rs"]
mod tests;
