//! 本进程里几条帧命令对后端其余部分开的门：资产域够用户文件那一扇 · 两台之间枢纽问这台自己那一跳 · 各号 MCP 同步的监听器。

use super::spec::{prepare, Prepared};
use super::REGISTRY;
use crate::stream::wire::Request;

/// 资产域（`assets/`）够用户文件的那一扇门：**本进程里那几条 `files-*` 帧命令本身**（阻塞档，原样调它们的 `run`）。
/// 住入方向是因为文件管理面的写句柄（`Face`）只有分派这一层造：这扇门就是进程内的那一次分派，
/// 照命令表那一族的档位（typed 的 `Facing` 那一档）递一份进去；资产模块只拿这个门，拿不到句柄，也不直呼 `files_write`。
#[derive(Clone, Copy)]
pub(crate) struct LocalFiles;

impl crate::assets::door::Door for LocalFiles {
    fn ask(
        &self,
        cmd: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, (String, String)> {
        let spec = REGISTRY
            .iter()
            .find(|s| s.name == cmd && s.name.starts_with("files-"))
            .ok_or_else(|| ("unknown_command".to_string(), cmd.to_string()))?;
        let req = Request {
            id: "in-process".to_string(),
            cmd: cmd.to_string(),
            args,
            within_ms: None,
            view: serde_json::Value::Null,
            tz: Default::default(),
            until: None,
        };
        // 门这一侧只交码与那一句（原话在命令自己的应答里才进复制详情）。文件管理那一族不写自家状态 ⇒ 不递 `Own`；
        //   文件管理写句柄由 `prepare` 照档位（typed 的 `Facing` 那一档）造了递进去。
        let Some(Prepared::Blocking(run)) = prepare(spec.run, None) else {
            return Err(("unknown_command".to_string(), cmd.to_string()));
        };
        run(req)
            .map(|v| v.unwrap_or(serde_json::Value::Null))
            .map_err(|f| (f.code, f.message))
    }
}

/// 两台之间那几件的枢纽问**这台自己**的那一跳：本进程那几条内层命令本身（阻塞档，原样调它们的 `run`）。
/// 限 [`HUB_INNER`] 那几条 —— 枢纽不是一扇通到任意命令的门。
pub(crate) struct LocalFrames {
    /// 引起这一趟的请求信封里的时区（内层命令照它写「几点」，问别台时转过去）。
    tz: crate::Tz,
    /// 写自家状态的句柄：内层那几条里写装记录的（`BlockingOwn`）要它。由引起这一趟的那条命令（它自己就是 `AsyncOwn`）递进来。
    own: crate::platform::Own,
}

/// 枢纽问得到的内层命令（来源那台读 · 被写那台判 / 写）。
pub(crate) const HUB_INNER: &[&str] = &[
    "cc-bus-install-state",
    "cc-bus-install",
    "mcp-sync-source",
    "mcp-sync-preview",
    "mcp-sync-apply",
    "skill-read",
    "skill-install-plan",
    "skill-install-apply",
];

impl crate::assets::hub::Here for LocalFrames {
    fn ask(
        &self,
        cmd: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, (String, String)> {
        let spec = REGISTRY
            .iter()
            .find(|s| s.name == cmd && HUB_INNER.contains(&s.name))
            .ok_or_else(|| ("unknown_command".to_string(), cmd.to_string()))?;
        let req = Request {
            id: "in-process".to_string(),
            cmd: cmd.to_string(),
            args,
            within_ms: None,
            view: serde_json::Value::Null,
            tz: self.tz.clone(),
            until: None,
        };
        // 内层那几条都在阻塞档（读 · 带码的读 · 写装记录）；枢纽在阻塞线程上问，异步档不问。
        let Some(Prepared::Blocking(run)) = prepare(spec.run, Some(&self.own)) else {
            return Err(("unknown_command".to_string(), cmd.to_string()));
        };
        run(req)
            .map(|v| v.unwrap_or(serde_json::Value::Null))
            .map_err(|f| (f.code, f.message))
    }

    fn viewer_tz(&self) -> crate::Tz {
        self.tz.clone()
    }
}

/// `tz` ＝ 引起这一趟的那条请求的信封 `tz`；`own` ＝ 分派递给那条命令的写句柄。
pub(super) fn hub_here(
    tz: &crate::Tz,
    own: &crate::platform::Own,
) -> std::sync::Arc<dyn crate::assets::hub::Here> {
    std::sync::Arc::new(LocalFrames {
        tz: tz.clone(),
        own: own.clone(),
    })
}

/// 流那一路的后端里盯这台的 `rotation.json` · 策略目录 · `quota.json` · 账号库（别的进程写了也发节点、推 `changed`）。
pub fn watch_rotation(own: &crate::platform::fs::Own) {
    crate::accounts::quota::rotation::watch_here(own);
    crate::accounts::quota::nodes::watch_logins_here();
}

/// 常驻后端里起「各号的配置文件 / 凭据文件一变就同步一趟用户级 MCP 与 MCP 登录」那个监听器
/// （配置文件与共享集合写经 [`LocalFiles`]，与帧命令同一扇门；凭据文件经 `own` 铸的窄句柄写）。
pub fn watch_account_mcp(own: &crate::platform::fs::Own) {
    crate::accounts::manage::mcp_share_watch::start(&LocalFiles, own.clone());
}

/// 这台的账号库补号色（一次，写经 [`LocalFiles`]，与帧命令同一扇门）：只由占着常驻的那一个进程起来时调
/// （`main.rs` 接中转那一刻，与 `rotation.json` 换形写回同一处、同一个写的人）。
pub fn fill_account_colors() {
    crate::accounts::manage::wire::colors_here(&LocalFiles);
}
