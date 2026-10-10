//! 「这台机器上现在有哪些 tmux 会话」的那一张快照 —— 全 crate 只有一份（watcher 要差分「这一轮谁没了」，Gate 要判活 ＋ 铸名避让）。
//!
//! 两条纪律都在 API 形状上：
//! 1. 问它一次 = 它更新一次。[`SessionSnapshot::query`] 没有「只读缓存」这条路 —— 先探一次、再把探回来的那份存下来并返回，拿到的值恒是这一刻的。
//!    watcher 用 [`SessionSnapshot::publish`] 把它焐热，但焐热的值不会被 `query` 交出去。退化成读缓存 = 拿陈值判活 = 把刚死的会话报成活的
//!    （`bus-list` 会把敲门文字打进陌生占用者的屏幕）。
//! 2. 铸名避让只能问它：[`TakenNames`] 的字段是模块私有的 ⇒ 本模块之外造不出一份「已占用的名字」（`DECISIONS.md#R52`）。
//!
//! 住 `common/`：control（Gate 判活与铸名）与 observe（watcher 发布）都要，而 `control/` 不许引用 `observe/`；
//! 平台无关（起的是 `tmux` 这个跨平台程序）、无域知识（只认「会话名 + `@ccm_sid`」两个字段）。
//!
//! # 不承诺的
//!
//! - 它不是 `probe()` 的替代品：`control/gate.rs::probe` 取的是 `#{session_id}` 句柄（关 TOCTOU 窗口）；本模块交出来的是名字，
//!   名字会被重新绑定 ⇒ 不许拿它去下破坏性命令。
//! - 不带 `#{session_id}`：两个消费者（`cc_bus::join_identity` 与 `ccm` 的铸名避让）都不问那一列，watcher 那条路（`TMUX_LS_FMT`）里也没有它。

use crate::platform::child::{Child, Deadline};
use copy_core::copy_text;
use std::sync::{Mutex, OnceLock};

use crate::common::tmux_utf8::{tab_underflow, UTF8_CLIENT_FLAG};

/// 命令级错误：`(code, message)`。与 [`crate::control::gate`] 同型。
pub(crate) type CmdErr = (&'static str, String);

/// 快照里的一行：**只有名字与 `@ccm_sid`**（为什么不带 `session_id` 见模块头注）。
///
/// `@ccm_sid` 打在**窗格**上（一个会话里可以跑几个 claude）⇒ 一行 = 一个会话 × 它里面挂着的一个 sid：
/// 挂着几个就几行（活动窗格那个在前），一个都没挂 ⇒ 一行空 sid（会话本身还在）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionRow {
    /// `#{session_name}`。
    pub(crate) name: String,
    /// 这个会话某个窗格上 `@ccm_sid` 的值。一个都没挂 ⇒ 空串（= 不是 `ccm` 起的，或还没绑 sid）。
    pub(crate) ccm_sid: String,
}

/// 一个会话里挂着的 sid：活动窗格那个（`active` ＝ 会话上下文里的 `#{@ccm_sid}`）在前，其余按窗口 / 窗格序
/// （`all` ＝ `#{W:#{P:#{@ccm_sid} }}` 的展开：逐窗口、逐窗格，空格分隔，没挂的窗格是空段；窗格上没有自己的值时
/// tmux 往上取会话那一级的，那也算挂着）；去重；只认 sid 字符集（极老 tmux 不展开、原样留 `#{…}` 字面量 ⇒ 不当 sid）。
pub(crate) fn pane_sids(active: &str, all: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in std::iter::once(active).chain(all.split(' ')).map(str::trim) {
        let shaped = !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if shaped && !out.iter().any(|o| o == s) {
            out.push(s.to_string());
        }
    }
    out
}

/// 一个会话 ⇒ 快照行（见 [`SessionRow`]）。
pub(crate) fn rows_of(name: &str, sids: Vec<String>) -> Vec<SessionRow> {
    if sids.is_empty() {
        return vec![SessionRow {
            name: name.to_string(),
            ccm_sid: String::new(),
        }];
    }
    sids.into_iter()
        .map(|ccm_sid| SessionRow {
            name: name.to_string(),
            ccm_sid,
        })
        .collect()
}

/// 已被占用的会话名。字段刻意是模块私有的：铸名避让（`ccm::plan::build`）只收这个类型，「另起一份名字集合」编译不过。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TakenNames(Vec<String>, Vec<SessionRow>);

impl TakenNames {
    /// 给避让算法用的那一份。
    pub(crate) fn as_slice(&self) -> &[String] {
        &self.0
    }

    /// claude 会话 `sid` 正在哪个 tmux 会话里跑（某个窗格的 `@ccm_sid` 对上）；没有 ⇒ `None`。
    /// 与避让同一份快照（同一次探测）⇒ `ccm --ccm-print` 与真跑对「在跑」也同答。
    pub(crate) fn running(&self, sid: &str) -> Option<&str> {
        self.1
            .iter()
            .find(|r| !sid.is_empty() && r.ccm_sid == sid)
            .map(|r| r.name.as_str())
    }
}

/// 探回来一份的那个动作。测试用假的替掉，生产用 [`probe_tmux`]。
type Prober = Box<dyn Fn() -> Result<Vec<SessionRow>, CmdErr> + Send + Sync>;

/// 那一张快照。
pub(crate) struct SessionSnapshot {
    rows: Mutex<Vec<SessionRow>>,
    probe: Prober,
}

impl SessionSnapshot {
    fn new(probe: Prober) -> Self {
        Self {
            rows: Mutex::new(Vec::new()),
            probe,
        }
    }

    /// 向快照发一次询问 ⇒ 快照更新一次。没有「只读」这条路：交出去的恒是刚探回来的那一份。改成「先看缓存、有就直接回」是本模块唯一会静默失效的改法。
    pub(crate) fn query(&self) -> Result<Vec<SessionRow>, CmdErr> {
        let fresh = (self.probe)()?;
        // 存一份供 `publish` 的同居者读（并让「刚探到的」与「焐着的」始终是同一份）。
        if let Ok(mut g) = self.rows.lock() {
            g.clone_from(&fresh);
        }
        Ok(fresh)
    }

    /// 铸名避让要的那份「已占用的名字」。**同样会触发一次更新**（它就是 `query` 的投影）。
    pub(crate) fn taken_names(&self) -> Result<TakenNames, CmdErr> {
        let rows = self.query()?;
        let mut names: Vec<String> = rows.iter().map(|r| r.name.clone()).collect();
        names.dedup();
        Ok(TakenNames(names, rows))
    }

    /// 观测方把刚看到的一份**焐进来**。
    ///
    /// ⚠ **焐热不等于「问过了」**：`query` 一定重探，绝不把这里存的值交出去。
    /// 它买的是「同一张表」这件事本身 —— 观测与判活从此看的是一个数据结构，
    /// 而不是两条各自去起 tmux 的路。
    pub(crate) fn publish(&self, rows: Vec<SessionRow>) {
        if let Ok(mut g) = self.rows.lock() {
            *g = rows;
        }
    }

    /// 最后一次存进来的那份（**陈值**）。
    ///
    /// 🔴 生产代码一处都不许调它 —— 它只是让「陈值确实与这一刻不同」这件事**测得出来**。
    /// 由 `KR96D1` 的判据钉住「生产段零调用」。
    #[cfg(test)]
    pub(crate) fn peek(&self) -> Vec<SessionRow> {
        self.rows.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// 测试用：拿一个脚本化的探测器造一份快照。
    #[cfg(test)]
    pub(crate) fn with_prober(
        probe: impl Fn() -> Result<Vec<SessionRow>, CmdErr> + Send + Sync + 'static,
    ) -> Self {
        Self::new(Box::new(probe))
    }
}

/// 进程内那一份。
pub(crate) fn global() -> &'static SessionSnapshot {
    static ONE: OnceLock<SessionSnapshot> = OnceLock::new();
    ONE.get_or_init(|| SessionSnapshot::new(Box::new(probe_tmux)))
}

/// 探测格式串。**三列**用 TAB 分隔：会话名（tmux 转义成字面 `\t`）· 活动窗格的 `@ccm_sid` · 各窗格的 `@ccm_sid`
/// （见 [`pane_sids`]，sid 字符集 `[A-Za-z0-9_-]`）⇒ 下溢与过溢都不会由合法内容触发，下溢只可能是打印通道被改写。
const LIST_FMT: &str = "#{session_name}\t#{@ccm_sid}\t#{W:#{P:#{@ccm_sid} }}";

/// `LIST_FMT` 的列数 —— [`tab_underflow`] 的 N。**改格式串必须同步这个数。**
const LIST_FMT_FIELDS: usize = 3;

/// 全 crate 唯一一处「一次列全部 tmux 会话」的起进程点：一次调用列全部，不是每个成员探一次。
/// 不看退出码（同 `control/gate.rs::probe`）：没有任何会话时 `tmux ls` 是非零 + 空输出，那是「一个都没有」。
/// `-u` 必须排在子命令之前：放到后面是 `rc=1 + unknown flag -u`，会被压成「一个会话都没有」（本模块测试段那条判据钉住次序）。
/// 期限：tmux 一发 5 s（同 watcher 探测 tmux 的期限；界面等它的命令预算都在 10 s 以上）。
const LIST_SESSIONS_WITHIN: Deadline = Deadline::secs(5);

fn probe_tmux() -> Result<Vec<SessionRow>, CmdErr> {
    let out = Child::new("tmux")
        .args([UTF8_CLIENT_FLAG, "list-sessions", "-F", LIST_FMT])
        .run(LIST_SESSIONS_WITHIN)
        .map_err(|e| {
            // 句子只说原因词；系统原话记一行日志（快照的失败形是「码 ＋ 一句」，没有原话位）。
            let (c, s) = e.into_cmd_said("no_tmux", |why| {
                copy_text("beSessionSnapshot.probeTmux.noTmux", &[("why", why)])
            });
            (c, crate::common::said::IntoNote::into_note(s))
        })?;
    Ok(parse_rows(&String::from_utf8_lossy(&out.stdout)))
}

/// 把 `LIST_FMT` 的 stdout 切成行。段数下溢 ⇒ 打印通道被改写 ⇒ 出声 + 整行不当好数据。空行不算下溢（本来就该被 `name.is_empty()` 丢掉）。
fn parse_rows(text: &str) -> Vec<SessionRow> {
    text.lines()
        .filter_map(|line| {
            if !line.trim().is_empty() && tab_underflow(line, LIST_FMT_FIELDS) {
                tracing::warn!(
                    "CCM_TMUX_UNPARSABLE list-sessions 切出 {} 段 < {LIST_FMT_FIELDS} —— \
                     tmux 打印通道被改写（K-R12：客户端不是 UTF-8 ⇒ TAB 变 `_`），\
                     整行不当好数据。原样回包：{line:?}",
                    line.split('\t').count()
                );
                return None;
            }
            let mut it = line.split('\t');
            let name = it.next()?.trim();
            if name.is_empty() {
                return None;
            }
            let active = it.next().unwrap_or_default();
            Some(rows_of(
                name,
                pane_sids(active, it.next().unwrap_or_default()),
            ))
        })
        .flatten()
        .collect()
}

#[cfg(test)]
#[path = "../../../tests/backend/common/session_snapshot_tests.rs"]
mod tests;
