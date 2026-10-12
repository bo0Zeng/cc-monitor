//! 「此刻显示这个会话的终端」那一形：那台 `session-terminals` 答出来、界面原样交本机 `terminal-processes` 去对的同一份。
//! 两头各住一层（observe 读出来 · dial 拿去对连接表），线上形只此一处。

/// `SSH_CONNECTION` 四元组（那台机器看到的：对面地址 · 对面端口 · 本机地址 · 本机端口）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SshConnection {
    /// 对面地址。
    pub(crate) client_addr: String,
    /// 对面端口。
    pub(crate) client_port: u16,
    /// 本机地址。
    pub(crate) server_addr: String,
    /// 本机端口。
    pub(crate) server_port: u16,
}

/// 一个正在显示这个会话的终端。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct Terminal {
    /// 经 ssh 连过来的那条连接；`null` = 那个终端不是经 ssh 连的（这台机器自己的屏幕上，或别的方式）。
    pub(crate) ssh: Option<SshConnection>,
    /// 最近一次动静（tmux 客户端的 `client_activity`，Unix 秒）；不在 tmux 里 ⇒ `null`。
    #[cfg_attr(test, ts(type = "number | null"))]
    pub(crate) activity: Option<u64>,
    /// 本机那个终端窗口的标签（`LC_CCM_WINDOW`，形如 `<进程号>-<起始时刻>`）；没送过来 / 形状不对 ⇒ 不带。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) window: Option<String>,
}
