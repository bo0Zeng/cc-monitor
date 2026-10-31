//! 「此刻显示这个会话的终端」那一形：那台 `session-terminals` 答出来、本机后端（`session-window`）问回来拿去对连接表的同一份。
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
    /// 键恒在：产这一格的 `session-terminals` 不是经 ssh 也写 `null`；交来的缺这一键 ⇒ 认不得的形（`bad_args`），不当「不是经 ssh」。
    #[serde(deserialize_with = "Option::deserialize")]
    pub(crate) ssh: Option<SshConnection>,
    /// 最近一次动静（tmux 客户端的 `client_activity`，Unix 秒）；不在 tmux 里 ⇒ `null`。
    #[cfg_attr(test, ts(type = "number | null"))]
    pub(crate) activity: Option<u64>,
    /// 本机那个终端窗口的标签（`LC_CCM_WINDOW`，[`WindowLabel`]）；没送过来 / 形状不对 ⇒ 不带。
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "WindowLabel::lenient"
    )]
    #[cfg_attr(test, ts(optional, type = "string"))]
    pub(crate) window: Option<WindowLabel>,
}

/// 窗口标签：`LC_CCM_WINDOW` 那一串 `<进程号>-<起始时刻>`（两段十进制，进程号放得进 u32、起始时刻放得进 u64，前后空白不算）。
/// 读进来的那一下解一次；线上仍是那一串（[`std::fmt::Display`] 写回同一串）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct WindowLabel {
    /// 那个 shell 的进程号。
    pub(crate) pid: u32,
    /// 那个进程的起始时刻（进程表里的 `start`）。
    pub(crate) start: u64,
}

impl std::str::FromStr for WindowLabel {
    type Err = ();

    fn from_str(raw: &str) -> Result<Self, ()> {
        let (a, b) = raw.trim().split_once('-').ok_or(())?;
        let digits = |x: &str| !x.is_empty() && x.bytes().all(|c| c.is_ascii_digit());
        if !(digits(a) && digits(b)) {
            return Err(());
        }
        Ok(WindowLabel {
            pid: a.parse().map_err(|_| ())?,
            start: b.parse().map_err(|_| ())?,
        })
    }
}

impl std::fmt::Display for WindowLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}", self.pid, self.start)
    }
}

impl serde::Serialize for WindowLabel {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for WindowLabel {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        raw.parse()
            .map_err(|()| serde::de::Error::custom(format!("not a window label: {raw:?}")))
    }
}

impl WindowLabel {
    /// `Terminal.window` 那一格：是串、形状不对 ⇒ 不带（不当整份认不得）。
    fn lenient<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Self>, D::Error> {
        let raw = <Option<String> as serde::Deserialize>::deserialize(d)?;
        Ok(raw.and_then(|r| r.parse().ok()))
    }
}
