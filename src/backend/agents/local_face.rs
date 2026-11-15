//! 本机布局与判活：`LocalFace` 一家在本机的布局，`LiveBy` 会话怎么判活。

use super::*;

/// 这个 tmux 前台命令是不是注册表里某一家的进程（[`Adapter::processes`]）—— 窗格前台判法只此一种：批量停 / 起认窗格时每一行的 `agent` · 终端名单那一格的状态 · 往会话说一句那道门都读它。
pub fn is_agent_process(command: &str) -> bool {
    REGISTRY
        .iter()
        .filter_map(|a| a.processes)
        .any(|names| names.contains(&command))
}

/// 一家在本机的布局：家目录怎么解析 · 会话进程留的 pidfile 住哪 · 进程 cmdline 怎么认是它。
#[derive(Clone, Copy)]
pub struct LocalFace {
    /// 家目录：给了账号配置目录就是它；没给 ⇒ `from_env` 时照进程环境，否则默认家目录。
    pub home_at: fn(config_dir: Option<&Path>, from_env: bool) -> PathBuf,
    /// 家目录 ⇒ pidfile 目录（判活 · 账号归属读它）。
    pub pidfile_dir: fn(&Path) -> PathBuf,
    /// 小写后的 cmdline 说这个进程是不是这一家的一条会话（判活那一判，[`CmdlineSays`]；怎么用见 `history::live_table::add_time_verdict`）。
    pub cmdline_may_be_agent: fn(&str) -> CmdlineSays,
    /// 任务列表那一面（[`TasksFace`]）。`None` ＝ 这一家没有任务列表。
    pub tasks: Option<TasksFace>,
    /// 进程状态文件（已解析）⇒ 它说的那一份事实（[`ProcFact`]）。通用层只读这一份，不认那一家的字段名。
    pub(crate) proc_of: fn(&serde_json::Value) -> ProcFact,
}

/// 一家的任务列表：住在哪 · 一个任务文件（已解析的对象）怎么读。通用层只走目录、照码写字，不认那一家的字段名。
#[derive(Clone, Copy)]
pub struct TasksFace {
    /// 家目录 ⇒ 任务列表的根（其下每个会话一个目录）。
    pub dir: fn(&Path) -> PathBuf,
    /// 一个任务对象 ⇒ 一格任务（`status_text` 留空，核心照 `status` 写）；解不成任务 ⇒ `None`。
    pub task: fn(&serde_json::Value) -> Option<cmd_wire::tasks::Task>,
}

/// 一个进程的 cmdline 说它是不是这一家的一条会话（[`LocalFace::cmdline_may_be_agent`] 答）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmdlineSays {
    /// 像这一家（读出来是空也落这里：空不算证据，放行）。
    Session,
    /// 是这一家的进程、可不是一条会话（例：守护池停泊的备用进程）：启动时刻核上了也不算活。
    NotSession,
    /// 明显不像这一家：启动时刻没核上时判冒名；核上了照活（身份证据更硬）。
    Unlike,
}

/// 一个会话进程留下的那份进程状态文件说了什么（[`LocalFace::proc_of`] 从那一家的格式翻好）：
/// 判活 · 宣告 · 账号归属 · 「在等什么」都只读它。哪一家写的跟着这一行走（`kind`），不另问注册表。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcFact {
    /// 写它的那一家（线上 `agent_kind`）。
    pub kind: &'static str,
    /// 它说的会话 id（原样，可能是空串；不是串 ⇒ 没有）。
    pub sid: Option<String>,
    /// 那一家写下的工作目录。
    pub cwd: Option<String>,
    /// 会话名（那一家给后台任务起的名字之类）。
    pub name: Option<String>,
    /// 那个进程的启动时刻（与 `/proc/<pid>/stat` 的 starttime 同一个数，挡 PID 复用）。
    pub proc_start: Option<u64>,
    /// 是后台会话（不是人坐在终端里对话的那种，不成 tab）。
    pub background: bool,
    /// 此刻在干什么；说不清 ⇒ `None`。
    pub activity: Option<SessionActivity>,
    /// 此刻在干什么的那个词原样（只拿来比「变没变」，不按它分支）。
    pub status_word: Option<String>,
    /// 在等人时等的是什么框；没说 / 说不清 ⇒ `None`。
    pub wait: Option<WaitOn>,
    /// 能不能接进它的终端（只认真布尔；没写 ⇒ `None`，照能）。
    pub attachable: Option<bool>,
    /// 此刻这一态从何时起（epoch ms）。
    pub status_since_ms: Option<u64>,
    /// 那个进程何时起的（epoch ms）。
    pub started_ms: Option<u64>,
}

/// 一条会话在等人时，**等的是什么框**（与哪一家无关；适配层从那一家的词翻过来，翻不出 ⇒ 不给）。
/// 「要人做哪种事」由它配上记录里没结果的那一步判（`observe::recognize::facts::needs_of`），不在这里判。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitOn {
    /// 批准框（工具调用 · 提问 · 计划都弹在这一类里，分哪种看那一步是什么工具）。
    Permission,
    /// 沙箱里的命令要联网，等放行。
    Network,
    /// 协作的另一个运行（worker）发来的批准请求。
    Worker,
    /// 它提了一个会话目标，等确认。
    Goal,
    /// 要填 / 要答（提问 · MCP 那一侧要的输入）。
    Input,
    /// 别的对话框开着（选项 · 提示 · 设置），等选。
    Dialog,
}

/// `kind` 那一家在本机的布局（[`Adapter::local`]）。认不出 / 没有 ⇒ `None`。
pub fn local_face_among(registry: &[Adapter], kind: &str) -> Option<LocalFace> {
    adapter_among(registry, kind).and_then(|a| a.local)
}

/// 一家在这台的家：哪一家 ＋ 它的家目录。整台的事拿一串（[`local_homes`]），对每一家各做一遍；
/// 问哪一家的布局只照 `kind` 问（[`AgentHome::local`]），不问「后端盯着的是哪一家」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentHome {
    pub kind: &'static str,
    pub home: PathBuf,
}

/// 有本机布局的每一家（注册序），家目录照进程环境解（[`LocalFace::home_at`]）。
pub fn local_homes() -> Vec<AgentHome> {
    local_homes_among(REGISTRY)
}

/// [`local_homes`] 的可喂夹具那一半。
pub fn local_homes_among(registry: &[Adapter]) -> Vec<AgentHome> {
    registry
        .iter()
        .filter_map(|a| {
            a.local.map(|f| AgentHome {
                kind: a.kind,
                home: (f.home_at)(None, true),
            })
        })
        .collect()
}

/// `kind` 那一家的家目录（[`LocalFace::home_at`]）：给了账号配置目录就是它；没给 ⇒ `from_env` 时照进程环境，否则默认家目录。
/// 那一家没有本机布局 · 认不出 ⇒ `None`。
pub fn home_of(kind: &str, config_dir: Option<&Path>, from_env: bool) -> Option<PathBuf> {
    local_face_among(REGISTRY, kind).map(|f| (f.home_at)(config_dir, from_env))
}

/// 交家的那几处收的形：一串家（各家各自的，[`local_homes`]）或一家的家；测试档另收一个目录 ——
/// 那个目录当作注册表里有本机布局或住记录树的每一家的家（只给判据夹具）。
pub trait IntoHomes {
    /// `registry` 只在「一个目录」那一形里用（照它列有本机布局的那几家）。
    fn into_homes_among(self, registry: &[Adapter]) -> Vec<AgentHome>;

    /// [`Self::into_homes_among`] 在生产注册表上。
    fn into_homes(self) -> Vec<AgentHome>
    where
        Self: Sized,
    {
        self.into_homes_among(REGISTRY)
    }
}

impl IntoHomes for &[AgentHome] {
    fn into_homes_among(self, _: &[Adapter]) -> Vec<AgentHome> {
        self.to_vec()
    }
}

impl IntoHomes for &AgentHome {
    fn into_homes_among(self, _: &[Adapter]) -> Vec<AgentHome> {
        vec![self.clone()]
    }
}

impl IntoHomes for &Vec<AgentHome> {
    fn into_homes_among(self, _: &[Adapter]) -> Vec<AgentHome> {
        self.clone()
    }
}

impl<const N: usize> IntoHomes for &[AgentHome; N] {
    fn into_homes_among(self, _: &[Adapter]) -> Vec<AgentHome> {
        self.to_vec()
    }
}

/// 测试档夹具那一形：一个临时目录当每一家的家。生产里一律逐家给家（[`local_homes`] 或调用方拼好的 [`AgentHome`]）。
#[cfg(any(test, feature = "test-support"))]
mod fixture_homes {
    use super::*;

    impl IntoHomes for &Path {
        fn into_homes_among(self, registry: &[Adapter]) -> Vec<AgentHome> {
            registry
                .iter()
                .filter(|a| a.local.is_some() || a.records.is_some_and(|r| r.tree.is_some()))
                .map(|a| AgentHome {
                    kind: a.kind,
                    home: self.to_path_buf(),
                })
                .collect()
        }
    }

    impl IntoHomes for &PathBuf {
        fn into_homes_among(self, registry: &[Adapter]) -> Vec<AgentHome> {
            self.as_path().into_homes_among(registry)
        }
    }
}

impl AgentHome {
    /// 那一家在本机的布局。
    pub fn local(&self) -> Option<LocalFace> {
        self.local_among(REGISTRY)
    }

    /// [`Self::local`] 照给定注册表（交注册表下来的那几处照它问）。
    pub fn local_among(&self, registry: &[Adapter]) -> Option<LocalFace> {
        local_face_among(registry, self.kind)
    }

    /// 那一家家目录下的 pidfile 目录（[`LocalFace::pidfile_dir`]）；没有本机布局 ⇒ `None`（读出零份）。
    pub fn pidfile_dir(&self) -> Option<PathBuf> {
        self.pidfile_dir_among(REGISTRY)
    }

    /// [`Self::pidfile_dir`] 照给定注册表。
    pub fn pidfile_dir_among(&self, registry: &[Adapter]) -> Option<PathBuf> {
        self.local_among(registry)
            .map(|f| (f.pidfile_dir)(&self.home))
    }

    /// [`Self::proc_fact_among`] 在生产注册表上（只给判据直接翻一份）。
    #[cfg(any(test, feature = "test-support"))]
    pub fn proc_fact(&self, bytes: &[u8]) -> Option<ProcFact> {
        self.proc_fact_among(REGISTRY, bytes)
    }

    /// pidfile 目录里的一份进程状态文件（原样的字节）⇒ 照给定注册表里那一家翻出的那一份事实（[`LocalFace::proc_of`]）。读不成 JSON ⇒ `None`。
    pub fn proc_fact_among(&self, registry: &[Adapter], bytes: &[u8]) -> Option<ProcFact> {
        let v: serde_json::Value = serde_json::from_slice(bytes).ok()?;
        Some((self.local_among(registry)?.proc_of)(&v))
    }
}

/// 一家的会话怎么判活（[`Adapter::live`]）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveBy {
    /// 每个活会话的进程在 pidfile 目录（[`LocalFace::pidfile_dir`]）里留一份进程状态文件：pidfile 在、进程在 ⇒ 活。
    Pidfiles,
    /// 没有 pidfile：会话在世期间写记录的那个进程一直开着那份记录 ⇒ 有进程开着它写就是活，
    /// 那个进程退了或把它关了就是走了（`platform::writers`）。
    HeldOpen,
    /// 判不了（这一家既不留 pidfile、也不一直开着记录写，或没人考据过）。
    Unknown,
}

/// 某一家（wire 上的 kind）在给定注册表里怎么判活；认不出 ⇒ [`LiveBy::Unknown`]。
pub fn live_by_among(registry: &[Adapter], kind: &str) -> LiveBy {
    adapter_among(registry, kind).map_or(LiveBy::Unknown, |a| a.live)
}

/// 生产注册表里那一家怎么判活（[`live_by_among`]）。
pub fn live_by(kind: &str) -> LiveBy {
    live_by_among(REGISTRY, kind)
}
