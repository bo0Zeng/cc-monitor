//! 记录面：`RecordFace` 一家的会话记录怎么读，与按 sid · 按路径问它的那几个入口。

use super::*;

/// 改写那一形（[`BranchFace::Rewrite`]）：`(记录, 分叉点消息 id, 源 sid, 新 sid)` ⇒ 新会话的记录。
pub(crate) type BranchFn =
    fn(&[serde_json::Value], &str, &str, &str) -> Result<Vec<serde_json::Value>, String>;

/// 一家怎么分叉（[`RecordFace::branch`]）。
#[derive(Clone, Copy)]
pub enum BranchFace {
    /// 通用层读源记录、交那一家改写、`O_EXCL` 落一份新记录。
    Rewrite(BranchFn),
    /// 那一家自己叉：交回新会话的 sid；通用层不写文件。
    Own(fn(&OwnFork<'_>) -> Result<String, ForkFail>),
}

/// 自己叉那一形的入参。叉在的那一条消息对应哪一轮，由那一家自己从记录认。
#[derive(Debug, Clone, Copy)]
pub struct OwnFork<'a> {
    /// 那一家那个号的家（按 sid 找到源会话的那一个，[`SessionAt::home`]）。
    pub home: &'a Path,
    /// 源会话 sid。
    pub sid: &'a str,
    /// 叉在哪一条消息。
    pub message: &'a str,
}

/// 自己叉不成：码 ＋ 必要的身份；人话由核心照码查文案表出。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForkFail {
    /// 源会话里认不出那条消息是哪一轮。
    NoTurn { message: String },
    /// 那一家叉的那一下没成（它自己的命令 / 写不成）。
    Failed,
}

/// 按 sid 找到的那份会话记录：哪一家（照哪一根找到的就是哪一家，读者认家只照它，不照路径另猜）· 那一家那个号的家 ·
/// 在哪一家的记录根下 · 文件本身。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionAt {
    pub kind: &'static str,
    pub home: PathBuf,
    pub root: PathBuf,
    pub file: PathBuf,
}

/// `registry` 里 `kind` 那一家的分叉记录变换 —— 通用层（`control/fork_write.rs`）写分叉记录的唯一入口。
/// 只答改写那一形；那一家自己叉（[`BranchFace::Own`]）⇒ 照实拒（它不写分叉记录，经起会话叉）；那一家不分叉 ⇒ 照实拒。
pub fn build_branch_records(
    registry: &[Adapter],
    kind: &str,
    lines: &[serde_json::Value],
    message_uuid: &str,
    src_sid: &str,
    new_sid: &str,
) -> Result<Vec<serde_json::Value>, String> {
    match adapter_among(registry, kind)
        .and_then(|a| a.records)
        .and_then(|r| r.branch)
    {
        Some(BranchFace::Rewrite(f)) => f(lines, message_uuid, src_sid, new_sid),
        Some(BranchFace::Own(_)) => Err(
            "refuse fork: this agent forks by itself at launch and writes no branch record"
                .to_string(),
        ),
        None => Err("no agent adapter can fork a session".to_string()),
    }
}

/// 一家的记录解释面：函数指针（同 [`Adapter::home`]，不立 trait）。
#[derive(Clone, Copy)]
pub struct RecordFace {
    /// 一行（[`Line`]：原文与它在文件里的起点字节偏移，没有自己身份的记录拿它合成 id，[`line_id`]）⇒ 通用记录那一形
    /// （空行 / 纯 BOM ⇒ `Ok(None)`；连 JSON 都不是 ⇒ `Err`，调用方照占号、不出成品）。
    pub parse: fn(&Line<'_>) -> Result<Option<Translated>, String>,
    /// 会话文件 ⇒ 它的 sid（这一家的文件命名）。
    pub sid: fn(&Path) -> Option<String>,
    /// 这个路径是不是这一家的一份会话记录（按文件形态判：后缀 / 命名）。
    pub is_session_file: fn(&Path) -> bool,
    /// 这一家的记录树（会话按项目目录分、住在家目录下）：根在哪 · 新会话文件怎么命名。`None` ＝ 这一家的会话不住记录树。
    pub(crate) tree: Option<RecordTree>,
    /// 这一行说这条会话此刻进到哪一态（一轮开始 ⇒ 在跑 · 一轮完了 / 中止 ⇒ 闲着）；不说 ⇒ `None`。
    /// 没有进程状态文件的那一家靠它（[`LocalFace::proc_of`] 是有状态文件那一家的同一问）；`None` 这一格 ＝ 这一家的记录不说。
    pub activity: Option<fn(&Line<'_>) -> Option<SessionActivity>>,
    /// 一行原文 ⇒ 它在记录链上的事实（主线外清单由通用层 [`mainline`] 按它算）。`None` ＝ 这一家的记录没有链（清单恒空）。
    pub chain: Option<fn(&Line<'_>) -> Option<mainline::ChainFact>>,
    /// 在这一家的一个记录根（根表上的一根，[`record_roots_among`]）下按 sid 找那份会话文件。每一家都得答：恢复 · 「记录还在不在」· 按 sid 读会话 · 删与归档都经它。
    pub(crate) find_session: fn(&Path, &str) -> Result<PathBuf, String>,
    /// 这一家怎么分叉（[`BranchFace`]：改写记录 · 自己叉）。`None` ＝ 这一家不分叉。
    pub branch: Option<BranchFace>,
    /// 这一家的漂移账（看不懂的记录类型记在哪）⇒ 各面的快照（通用形 [`drift::DriftFaceReport`]）；`None` ＝ 这一家不记。
    pub(crate) drift: Option<fn() -> Vec<drift::DriftFaceReport>>,
    /// 这一家的记录文本面（正文 / 工具内容怎么抽 · CLI 注入怎么剥）；`None` ＝ 这一家不进搜索 / 摘录那几条通用路。
    pub(crate) text: Option<TextFace>,
    /// 删历史会话那一条这一家怎么删（[`SessionDelete`]：删记录文件 · 跑一条命令）；`None` ＝ 这一家不删。
    pub(crate) delete: Option<SessionDelete>,
    /// 归档 · 取消归档一条会话这一家怎么做（[`ArchiveFace`]）；`None` ＝ 这一家的会话不出归档那两项。
    pub archive: Option<ArchiveFace>,
    /// 一条记录与流对账用的键（同一次上游应答写出的记录都带它）。`None` ＝ 这一家的流对不上记录（活卡不定稿，只靠收尾撤）。
    pub(crate) response_id: Option<fn(&serde_json::Value) -> Option<String>>,
    /// 这条记录属于哪个子运行（主运行的记录 ⇒ `None`）。`None` 这一格 ＝ 这一家没有子运行。
    pub run_of: Option<fn(&serde_json::Value) -> Option<RunMark>>,
    /// 父记录里说到子运行的那几条：派出（父侧工具调用 id、标签，知道时也给出子运行是哪个）· 收场（派出那一方说它完了 / 败了 / 被叫停）。
    pub(crate) child_link: Option<fn(&serde_json::Value) -> Vec<ChildLink>>,
    /// 子运行的记录住哪（由父记录路径推出）。`None` ＝ 这一家没有单独存放的子运行记录。
    pub children: Option<ChildFace>,
    /// 一条记录里说到后台命令的那几笔（起了一条 · 它拿到了任务号 · 它收场了）。`None` ＝ 这一家没有后台命令。
    pub(crate) background: Option<fn(&serde_json::Value) -> Vec<BgMark>>,
    /// 会话的项目目录（会话起在哪个目录）：只读记录开头（[`first_in_head`]，有上界）。`None` 这一格 ＝ 这一家的记录里没有这件事。
    pub project_dir: Option<fn(&Path) -> Option<String>>,
    /// 一条已解析的记录 ⇒ 那一家在这一条里说的此刻各 MCP 服务器的样子（[`McpSaid`]）；不是这种记录 ⇒ `None`。
    pub(crate) mcp_said: Option<fn(&serde_json::Value) -> Option<McpSaid>>,
    /// 一条已解析的记录 ⇒ 它是不是「认不出、又不成卡」的那种行（这一家的簿记行）⇒ 它的类型名；不是 ⇒ `None`。
    /// 不成卡是这一家的翻译表判的（`parse` 那一格给它不出记录），这一格只让会话事实照实记几行、哪几种。`None` 这一格 ＝ 这一家没有这种行。
    pub(crate) quiet_unread: Option<fn(&serde_json::Value) -> Option<String>>,
    /// 一行 ⇒ 这一家不读它（不在它认得的那几类里）时它只留下的那几格（身份）；读 ⇒ `None`。
    /// 不读的行通用层不整行建树（[`Line::of`]），几问都吃这一份小的。`None` 这一格 ＝ 这一家每一行都读。
    pub(crate) skim: Option<fn(&Line<'_>) -> Option<serde_json::Value>>,
    /// 一条已解析的记录 ⇒ 它的一拍（[`beat::BeatFace`]：轮次 · 会话事实 · 大纲只吃它）。`None` ＝ 这一家不出轮次 · 事实 · 大纲。
    pub(crate) beats: Option<beat::BeatFace>,
}

/// `kind` 那一家：这条记录的一拍。那一家不出 ⇒ `None`。
pub fn beat_of<'a>(kind: &str, v: &'a serde_json::Value) -> Option<beat::Beat<'a>> {
    record_face(kind).and_then(|r| r.beat(v))
}

impl RecordFace {
    /// 这一家：这条记录的一拍（手里已拿着那一根的面的调用方问这里）。这一家不出 ⇒ `None`。
    pub fn beat<'a>(&self, v: &'a serde_json::Value) -> Option<beat::Beat<'a>> {
        self.beats.and_then(|b| (b.beat)(v))
    }
}

/// `kind` 那一家：这一行有没有可能改动会话事实（快路）。那一家不出一拍 ⇒ 不会。
pub fn beat_hint(kind: &str, line: &[u8], watch: &beat::Watch) -> bool {
    record_face(kind)
        .and_then(|r| r.beats)
        .is_some_and(|b| (b.hint)(line, watch))
}

/// `kind` 那一家：这个工具在「需手动」里是哪一种。那一家不说 ⇒ 别的。
pub fn call_ask(kind: &str, tool: &str) -> beat::CallAsk {
    record_face(kind)
        .and_then(|r| r.beats)
        .map_or(beat::CallAsk::Plain, |b| (b.ask)(tool))
}

/// 一家的记录树：会话按项目目录分，住在家目录下的一棵树里。
#[derive(Clone, Copy)]
pub struct RecordTree {
    /// 家目录 ⇒ 记录树的根（其下每个项目一个目录）。
    pub(crate) root: fn(&Path) -> PathBuf,
    /// sid ⇒ 新会话文件的文件名（分叉落盘用）。
    pub file_name: fn(&str) -> String,
}

/// `kind` 那一家：一条已解析的记录里说到后台命令的那几笔（[`RecordFace::background`]；那一家没有 ⇒ 空）。
pub fn background_marks(kind: &str, v: &serde_json::Value) -> Vec<BgMark> {
    record_face(kind)
        .and_then(|r| r.background)
        .map(|f| f(v))
        .unwrap_or_default()
}

/// 子运行的记录住哪：父记录路径 ⇒ 此刻在盘上的子运行记录路径们（不在 ⇒ 空）。通用 watcher 拿这些路径走与主记录同一条事件管线。
#[derive(Clone, Copy)]
pub struct ChildFace {
    pub(crate) sources: fn(&Path) -> Vec<PathBuf>,
    /// 一个路径**若是**某份父记录的子运行记录（形状对得上 `sources` 会收的那种）⇒ 那份父记录的路径；否则 `None`。
    /// 文件事件来了只对它答得出的才去找，且只在那一份父记录底下找。
    pub(crate) owner: fn(&Path) -> Option<PathBuf>,
    /// 记录树根底下，id 为这个的子运行的记录（[`Self::owner`] 再对到它的父记录）；没有 ⇒ `None`。计划读面把接手 id 对到会话时用。
    pub(crate) find: fn(&Path, &str) -> Option<PathBuf>,
    /// 父记录的一行原文可能说到子运行（[`RecordFace::child_link`] 会答出东西）—— 便宜的预筛：漏判不许，多判无妨。
    /// 只读尾巴的那条流接上会话时，靠它从父记录已有的那一截里只挑这几行解析。
    pub hint: fn(&str) -> bool,
    /// 一条记录自己写着的时刻（自 1970 起的毫秒；运行表的开始 · 最近动静 · 收场三个时刻用）；记录里没写 ⇒ `None`（通用层退回读到它的时刻）。
    pub(crate) written: fn(&serde_json::Value) -> Option<u64>,
    /// 一份子运行记录（[`Self::sources`] 收的那种）⇒ 派出它的那一刻就落了盘的对上（[`ChildLink`] 的「对上」一形：父侧工具调用 id ＋ 子运行）。
    /// 前台派出的子运行要跑完父侧才回结果，在那之前卡和运行只能靠它对上；还没落盘 / 这一家不落 ⇒ `None`（通用层下次读到这份记录再问）。
    pub(crate) spawned: fn(&Path) -> Option<ChildLink>,
}

/// 通用层要的那一组面（记录 ＋ 子运行住址）：流式 watcher 跟的那一家给；判据喂别的一组（假适配层）。
#[derive(Clone, Copy)]
pub struct RunFaces {
    pub response_id: Option<fn(&serde_json::Value) -> Option<String>>,
    pub run_of: Option<fn(&serde_json::Value) -> Option<RunMark>>,
    pub child_link: Option<fn(&serde_json::Value) -> Vec<ChildLink>>,
    pub children: Option<ChildFace>,
}

impl RunFaces {
    /// 一家的记录面 ⇒ 它的运行面。
    pub fn of(r: &RecordFace) -> Self {
        Self {
            response_id: r.response_id,
            run_of: r.run_of,
            child_link: r.child_link,
            children: r.children,
        }
    }

    /// 什么都不填 ⇒ 没有子运行、流对不上记录。
    pub const NONE: Self = Self {
        response_id: None,
        run_of: None,
        child_link: None,
        children: None,
    };

    pub fn response_id(&self, v: &serde_json::Value) -> Option<String> {
        self.response_id.and_then(|f| f(v))
    }

    pub fn run_of(&self, v: &serde_json::Value) -> Option<RunMark> {
        self.run_of.and_then(|f| f(v))
    }

    pub fn child_links(&self, v: &serde_json::Value) -> Vec<ChildLink> {
        self.child_link.map(|f| f(v)).unwrap_or_default()
    }

    pub fn sources(&self, parent: &Path) -> Vec<PathBuf> {
        self.children
            .map(|c| (c.sources)(parent))
            .unwrap_or_default()
    }

    pub fn owner(&self, p: &Path) -> Option<PathBuf> {
        self.children.and_then(|c| (c.owner)(p))
    }

    pub fn hint(&self, line: &str) -> bool {
        self.children.is_some_and(|c| (c.hint)(line))
    }

    pub fn written(&self, v: &serde_json::Value) -> Option<u64> {
        self.children.and_then(|c| (c.written)(v))
    }

    pub fn spawned(&self, child: &Path) -> Option<ChildLink> {
        self.children.and_then(|c| (c.spawned)(child))
    }
}

/// `kind` 那一家的运行面（没有 ⇒ [`RunFaces::NONE`]）。
pub fn run_faces(kind: &str) -> RunFaces {
    record_face(kind).map_or(RunFaces::NONE, |r| RunFaces::of(&r))
}

/// `kind` 那一家的一条已解析记录在会话里属于哪个运行（主运行 ⇒ `None`）—— 通用层（大纲 · 骨架索引）判「这条是不是子运行的」的唯一入口。
pub fn run_of_record(kind: &str, v: &serde_json::Value) -> Option<RunMark> {
    run_faces(kind).run_of(v)
}

/// 删历史会话那一条这一家怎么删 —— 那是记录布局的知识，文件管理写面不认；由门（命令注册那一处）照这一格的形分流。
#[derive(Clone, Copy)]
pub(crate) enum SessionDelete {
    /// 删本机那一份记录文件：两问经 [`locate_session_for_delete`] · [`is_session_record`] 这一个窄口递给文件管理写面。
    File {
        /// （会话所在那个号的家, sid）⇒ 本机要删的那一份（找 → 解到底 → 恰是那一形）。家由通用层照根表给
        /// （[`locate_session_for_delete_among`]），这一格不自己取默认家。
        locate: fn(&Path, &str) -> Result<PathBuf, String>,
        /// 这一份是不是会话记录的形状（写面删之前再问一次）。
        is_record: fn(&Path) -> bool,
    },
    /// 跑这一家自己的一条命令删（它的状态跟着改，后端不碰记录文件）：（会话所在那个号的家, sid）⇒ 那一条命令（[`DeleteCommand`]）；
    /// 那个家里没有这个 sid 的会话 ⇒ `None`。家由通用层照根表找到那一份时给（[`session_delete_command_among`]），这一格不自己取默认家。
    Command(fn(&Path, &str) -> Option<DeleteCommand>),
}

/// 删一条会话的那一条命令（[`SessionDelete::Command`]，那一家给）：起哪个程序 · 带哪几个参数 · 显式带哪几格环境；
/// 那一份记录在哪（删成了回包交它）。起它 · 等它 · 读退出码是门的事。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteCommand {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(&'static str, std::ffi::OsString)>,
    pub record: PathBuf,
}

/// 窄口之一：sid ⇒ 要删的那一份（[`locate_session_for_delete_among`] 在生产注册表 · 这台的根表上）。
pub fn locate_session_for_delete(sid: &str) -> Result<PathBuf, String> {
    locate_session_for_delete_among(REGISTRY, &record_roots_of(REGISTRY), sid)
}

/// 删记录文件那一形照根表（`roots`）找：表序逐根问，那一家声明了删记录文件那一形 ⇒ 照那一根所在的家问它，谁找到算谁
/// （不自己取默认家）；都答不出 ⇒ 第一家说的那一句；表上没有哪一根答得了 ⇒ 照实拒。
pub fn locate_session_for_delete_among(
    registry: &[Adapter],
    roots: &[RecordRoot],
    sid: &str,
) -> Result<PathBuf, String> {
    let mut first_err = None;
    for r in roots {
        let Some(SessionDelete::File { locate, .. }) =
            adapter_among(registry, r.kind).and_then(|a| a.records?.delete)
        else {
            continue;
        };
        match locate(&r.home, sid) {
            Ok(p) => return Ok(p),
            Err(e) => {
                first_err.get_or_insert(e);
            }
        }
    }
    Err(first_err
        .unwrap_or_else(|| copy_core::copy_text("beFilesWrite.session.noReader", &[("id", sid)])))
}

/// 窄口之二：这一份是不是会话记录 —— 这份记录归哪一家（[`record_face_of`]）就问哪一家；那一家不删记录文件 / 答不了 ⇒ 不是。
pub fn is_session_record(p: &Path) -> bool {
    match record_face_of(p).and_then(|r| r.delete) {
        Some(SessionDelete::File { is_record, .. }) => is_record(p),
        _ => false,
    }
}

/// [`session_delete_command_among`] 在生产注册表 · 这台的根表上。
pub fn session_delete_command(sid: &str) -> Option<DeleteCommand> {
    session_delete_command_among(REGISTRY, &record_roots_of(REGISTRY), sid)
}

/// 删会话交命令那一形：根表（`roots`）上头一根找得到这个 sid、那一家声明了跑命令那一形 ⇒ 那一家照会话所在那个家给的那一条；
/// 谁都不认 ⇒ `None`（照删记录文件那一形走）。
pub fn session_delete_command_among(
    registry: &[Adapter],
    roots: &[RecordRoot],
    sid: &str,
) -> Option<DeleteCommand> {
    let (face, at) = find_session_on(registry, roots, sid, |f| {
        matches!(f.delete, Some(SessionDelete::Command(_)))
    })
    .ok()?;
    let Some(SessionDelete::Command(ask)) = face.delete else {
        return None;
    };
    ask(&at.home, sid)
}

/// 一家怎么归档 · 取消归档一条会话（[`RecordFace::archive`]）：两条命令 ＋ 一份判定。起它 · 等它 · 读退出码是门的事。
/// 两条命令收（会话所在那个号的家, sid）：家由通用层照根表找到那一份时给，这一家不自己取默认家。
#[derive(Clone, Copy)]
pub struct ArchiveFace {
    /// （家, sid）⇒ 归档它的那一条命令；那个家里没有这个 sid 的会话 ⇒ `None`。
    pub(crate) archive: fn(&Path, &str) -> Option<ArchiveCommand>,
    /// （家, sid）⇒ 取消归档它的那一条命令；同上。
    pub(crate) unarchive: fn(&Path, &str) -> Option<ArchiveCommand>,
    /// 这份会话记录此刻是不是已归档。
    pub(crate) archived: fn(&Path) -> bool,
}

/// 归档 · 取消归档的那一条命令（[`ArchiveFace`]，那一家给）：起哪个程序（照这台的 `PATH` 找）· 带哪几个参数 · 显式带哪几格环境。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveCommand {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(&'static str, std::ffi::OsString)>,
}

/// 归档那一边（[`session_archive_command_among`] 问哪一条）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveSide {
    Archive,
    Unarchive,
}

/// 归档 · 取消归档交命令：根表（`roots`）上头一根找得到这个 sid、那一家声明了归档面 ⇒ 那一家照会话所在那个家给的那一条；
/// 谁都不认 · 那一家不给 ⇒ `None`（照实拒）。
pub fn session_archive_command_among(
    registry: &[Adapter],
    roots: &[RecordRoot],
    sid: &str,
    side: ArchiveSide,
) -> Option<ArchiveCommand> {
    let (face, at) = find_session_on(registry, roots, sid, |f| f.archive.is_some()).ok()?;
    let f = face.archive?;
    let ask = match side {
        ArchiveSide::Archive => f.archive,
        ArchiveSide::Unarchive => f.unarchive,
    };
    ask(&at.home, sid)
}

/// 这份会话记录归的那一家（`registry` 里）声明了归档面 ⇒ `Some(此刻是不是已归档)`；没声明 ⇒ `None`（不出归档那两项）。
pub fn archive_state_among(registry: &[Adapter], p: &Path) -> Option<bool> {
    record_face_among(registry, p)
        .and_then(|r| r.archive)
        .map(|f| (f.archived)(p))
}

/// 一行原文翻成的那一形 —— 适配层给，通用层只搬（[`record::Record`] 的格通用层一个都不读）。
#[derive(Debug, Clone, PartialEq)]
pub struct Translated {
    /// 这一行在界面里是什么；缺 ＝ 不进界面（照占号、不出成品）。
    pub record: Option<record::BackRecord>,
    /// 这条记录自己的 `cwd`（带它的那一类才有）。
    pub cwd: Option<String>,
    /// 这一行里官方客户端自报的额度窗口（带它的那一家、那一类行才有）：读记录那一路照记录认出号交额度账，来源 `usage`。
    pub quota_said: Option<Vec<QuotaWindow>>,
    /// 这一行里只给后端看的那几格（不上线）。
    pub unsent: Unsent,
}

/// 一行记录里只给后端看、不上线的那几格：适配层翻那一行时一并交，后端在出口之前读完。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Unsent {
    /// 这一行里各张图的字节（base64），按 [`record::Block::Image`] 的 `n` 排：取图那一条（`history-image`）照 `n` 取。
    pub images: Vec<String>,
    /// 这一行里读文件那一类结果的调用 id：右侧那一句只对它们把 `lines` 说成「读了几行」（`step_clock::said`）。
    pub reads: std::collections::BTreeSet<String>,
}

/// **一行记录**，交给适配层那几问（成品 · 链事实 · 轮次结束 · 此刻在干什么 · 运行簿）的同一份：原文（去掉 BOM 与两端空白）＋
/// 起点字节 ＋ 惰性解析的 `Value`。头一个要整行的那一问解一次，之后的都吃这一份 —— 一行只从文本解一次（瓶颈 #8）。
/// 只要几格的那几问（[`Line::read`]）在没人解过整行时直接从文本解那几格（预读那一路只要链事实，不必整行）。
///
/// 按那一家建（[`Line::of`]）：那一家不读的行（[`RecordFace::skim`]）不整行建树，几问（[`Line::value`] · [`Line::read`]）
/// 吃的都是它只留下的那几格。
pub struct Line<'a> {
    text: &'a str,
    start: u64,
    value: std::cell::OnceCell<Option<serde_json::Value>>,
}

impl<'a> Line<'a> {
    /// `raw` ＝ 一行原文（不含换行；BOM 与两端空白这里去掉），`start` ＝ 它在文件里的起点字节偏移。
    pub fn new(raw: &'a str, start: u64) -> Self {
        Line {
            text: raw.trim_start_matches('\u{feff}').trim(),
            start,
            value: std::cell::OnceCell::new(),
        }
    }

    /// 同 [`Self::new`]，按 `kind` 那一家读：它说这一行它不读 ⇒ 整行不建树，只留它给的那几格。
    /// 认不出是哪一家的（`None`）⇒ 不问哪一家，同 [`Self::new`]。
    pub fn of(kind: Option<&str>, raw: &'a str, start: u64) -> Self {
        Self::by(kind.and_then(record_face).as_ref(), raw, start)
    }

    /// 同 [`Self::of`]，那一家的记录面由调用方交（手里已拿着那一根的面，家与面同一个来源）。
    /// 说不出是哪一家的（`None`）⇒ 不问哪一家，同 [`Self::new`]。
    fn by(face: Option<&RecordFace>, raw: &'a str, start: u64) -> Self {
        let mut line = Self::new(raw, start);
        if let Some(v) = face.and_then(|r| r.skim).and_then(|skim| skim(&line)) {
            line.value = std::cell::OnceCell::from(Some(v));
        }
        line
    }

    /// 同 [`Self::by`]，当场把整行解好（那一家不读的行仍只是它留下的那几格）：几问都要整行的那一路（实时 `send_line`）用它，
    /// 只要几格的那几问也从这一份里取。说不出是哪一家的（`None`）⇒ 不问哪一家。
    pub fn whole(face: Option<&RecordFace>, raw: &'a str, start: u64) -> Self {
        let line = Self::by(face, raw, start);
        line.value();
        line
    }

    /// 原文（预筛用：认得出不是这一类的行就不解）。
    pub fn text(&self) -> &'a str {
        self.text
    }

    /// 起点字节偏移（没有自己身份的记录拿它合成 id，[`line_id`]）。
    pub fn start(&self) -> u64 {
        self.start
    }

    /// 是不是空行（纯空白 / 纯 BOM）。
    pub(crate) fn is_blank(&self) -> bool {
        self.text.is_empty()
    }

    /// 整行（解过就共用那一份）；连 JSON 都不是 ⇒ `None`。
    pub fn value(&self) -> Option<&serde_json::Value> {
        self.value
            .get_or_init(|| {
                count_parse();
                TREE_BUILDS.with(|c| c.set(c.get() + 1));
                serde_json::from_str(self.text).ok()
            })
            .as_ref()
    }

    /// 整行拿走（解过就是那一份，没解过当场解）；连 JSON 都不是 ⇒ `None`。
    pub fn into_value(self) -> Option<serde_json::Value> {
        self.value();
        self.value.into_inner().flatten()
    }

    /// 只要几格：整行解过（或那一家只留下了几格）就从那一份里取（树上走，不再解文本），没解过就从文本直接解那几格。
    pub(crate) fn read<T: serde::de::DeserializeOwned>(&self) -> serde_json::Result<T> {
        match self.value.get() {
            Some(Some(v)) => T::deserialize(v),
            _ => {
                count_parse();
                serde_json::from_str(self.text)
            }
        }
    }
}

thread_local! {
    /// 本线程到此刻从**文本**解析了几次记录行（[`Line::value`] · [`Line::read`] 记）：判据数「一行解几遍」。
    pub static LINE_PARSES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// 其中把整行建成 `Value` 的那几次（[`Line::value`] 记）：判据数「哪几行建了树」。
    pub static TREE_BUILDS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

fn count_parse() {
    LINE_PARSES.with(|c| c.set(c.get() + 1));
}

/// 一行在文件里的起点字节偏移 ⇒ 没有自己身份的记录用的那个 id（会话内唯一、各条读路给出的都一样）。
pub(crate) fn line_id(start: u64) -> String {
    format!("@{start}")
}

/// `kind` 那一家的记录解释面。认不出 ⇒ `None`。
pub fn record_face(kind: &str) -> Option<RecordFace> {
    adapter_among(REGISTRY, kind).and_then(|a| a.records)
}

/// `kind` 那一家的记录树（[`RecordFace::tree`]）。认不出 / 那一家的会话不住记录树 ⇒ `None`。
pub fn record_tree_among(registry: &[Adapter], kind: &str) -> Option<RecordTree> {
    adapter_among(registry, kind).and_then(|a| a.records?.tree)
}

/// `kind` 那一家家目录 `home` 下记录树的根。那一家没有记录树 ⇒ `None`。
pub fn records_root_among(registry: &[Adapter], kind: &str, home: &Path) -> Option<PathBuf> {
    record_tree_among(registry, kind).map(|t| (t.root)(home))
}

/// [`AgentHome::child_parent_sid`] 的可喂夹具那一半：问 `kind` 那一家。
pub(crate) fn child_parent_sid_among(
    registry: &[Adapter],
    kind: &str,
    records_root: &Path,
    child_id: &str,
) -> Option<String> {
    let r = adapter_among(registry, kind)?.records?;
    let ch = r.children?;
    let parent = (ch.owner)(&(ch.find)(records_root, child_id)?)?;
    (r.sid)(&parent)
}

impl AgentHome {
    /// 那一家家目录下记录树的根。那一家的会话不住记录树 ⇒ `None`。
    pub fn records_root(&self) -> Option<PathBuf> {
        records_root_among(REGISTRY, self.kind, &self.home)
    }

    /// 那一家认不认这份文件是它的会话记录（[`RecordFace::is_session_file`]）。
    pub fn is_session_file(&self, p: &Path) -> bool {
        is_session_file_among(REGISTRY, self.kind, p)
    }

    /// 那一家记录树根 `records_root` 底下 id 为 `child_id` 的子运行挂在哪个会话底下（父记录的会话 id）；对不上 ⇒ `None`。
    pub fn child_parent_sid(&self, records_root: &Path, child_id: &str) -> Option<String> {
        child_parent_sid_among(REGISTRY, self.kind, records_root, child_id)
    }
}

/// 按 sid 找会话文件 —— 通用层按 sid 找记录的唯一入口：照根表（[`record_roots_among`]）逐根在**那一家自己的**根下问，谁认得算谁。
/// 住记录树的那几家照 `homes` 里各自的家；合成历史那几家照各号的家（[`homes_among`]）。没有哪一家答得了 ⇒ 照实拒（都答不出 ⇒ 第一根说的那一句）。
pub fn find_session_in(homes: impl IntoHomes, sid: &str) -> Result<SessionAt, String> {
    find_session_in_among(REGISTRY, homes, sid)
}

/// [`find_session_in`] 的可喂夹具那一半。
pub fn find_session_in_among(
    registry: &[Adapter],
    homes: impl IntoHomes,
    sid: &str,
) -> Result<SessionAt, String> {
    let homes = homes.into_homes_among(registry);
    let homes = |a: &Adapter| match a.history {
        Some(_) => homes_among(a),
        None => homes
            .iter()
            .filter(|h| h.kind == a.kind)
            .map(|h| AccountHome {
                account: None,
                home: h.home.clone(),
            })
            .collect(),
    };
    find_session_on(registry, &record_roots_among(registry, &homes), sid, |_| {
        true
    })
    .map(|(_, at)| at)
}

/// 按 sid 找会话的那一圈（[`find_session_in_among`] 与删 · 归档交命令都只经它）：根表（`roots`）上逐根问，
/// 那一根的那一家记录面满足 `has`、又认得这个 sid 才算 ⇒ 那一家的记录面 ＋ 找到的那一份。
/// 谁都不认 ⇒ 照实拒（都答不出 ⇒ 第一根说的那一句）。
fn find_session_on(
    registry: &[Adapter],
    roots: &[RecordRoot],
    sid: &str,
    has: impl Fn(&RecordFace) -> bool,
) -> Result<(RecordFace, SessionAt), String> {
    let mut first_err = None;
    for r in roots {
        let Some(face) = adapter_among(registry, r.kind)
            .and_then(|a| a.records)
            .filter(|f| has(f))
        else {
            continue;
        };
        match (face.find_session)(&r.root, sid) {
            Ok(file) => {
                let at = SessionAt {
                    kind: r.kind,
                    home: r.home.clone(),
                    root: r.root.clone(),
                    file,
                };
                return Ok((face, at));
            }
            Err(e) => {
                first_err.get_or_insert(e);
            }
        }
    }
    Err(first_err.unwrap_or_else(|| format!("no agent adapter can locate session {sid:?}")))
}

/// [`RecordTree::file_name`]：`kind` 那一家新会话文件的文件名。那一家没有记录树 ⇒ `None`。
pub fn session_file_name(kind: &str, sid: &str) -> Option<String> {
    record_tree_among(REGISTRY, kind).map(|t| (t.file_name)(sid))
}

/// 这份会话记录归的那一家（[`record_kind_of`]）给 `sid` 起的文件名。那一家没有记录树 ⇒ `None`。
pub fn session_file_name_of(p: &Path, sid: &str) -> Option<String> {
    session_file_name(record_kind_of(p)?, sid)
}

/// `kind` 那一家认不认这份文件是它的会话记录（[`RecordFace::is_session_file`]）。认不出这一家 ⇒ `false`。
pub fn is_session_file_among(registry: &[Adapter], kind: &str, p: &Path) -> bool {
    adapter_among(registry, kind)
        .and_then(|a| a.records)
        .is_some_and(|r| (r.is_session_file)(p))
}
/// 一份已过围栏的会话记录是哪一家的：落在根表上某一家合成历史的某一根下（各号 · 日期树与归档根都算）⇒ 那一家；
/// 否则落在根表上某一家某个号的记录树下 ⇒ 那一家；再不然 ⇒ 住记录树、认得这个文件名的那一家（[`RecordFace::is_session_file`]），
/// 不止一家认得 ⇒ 认不出（`None`）。
pub fn record_kind_of(path: &Path) -> Option<&'static str> {
    record_kind_among(REGISTRY, path)
}

/// [`record_kind_of`] 的可喂夹具那一半。
pub fn record_kind_among(registry: &[Adapter], path: &Path) -> Option<&'static str> {
    // 根解开链接再比（Windows 上那一形带扩展长度前缀）⇒ 两边照同一种规范化比（`is_under`）。
    let under = |root: PathBuf| {
        let root = Ro::canonicalize(&root).unwrap_or(root);
        crate::platform::paths::is_under(path, &root)
    };
    history_roots_among(registry)
        .into_iter()
        .find(|(_, root)| under(root.clone()))
        .map(|(kind, _)| kind)
        .or_else(|| {
            record_roots_of(registry)
                .into_iter()
                .filter(|r| adapter_among(registry, r.kind).is_some_and(|a| a.history.is_none()))
                .map(|r| (r.kind, r.root))
                .find(|(_, root)| under(root.clone()))
                .map(|(kind, _)| kind)
        })
        .or_else(|| {
            // 照文件名认：恰一家认得才算；两家都认得 ⇒ 认不出（不照注册序取头一家）。
            let mut owners = registry.iter().filter(|a| {
                a.history.is_none()
                    && a.records
                        .is_some_and(|r| r.tree.is_some() && (r.is_session_file)(path))
            });
            match (owners.next(), owners.next()) {
                (Some(a), None) => Some(a.kind),
                _ => None,
            }
        })
}

/// 一份会话记录的 sid：照它归的那一家（[`record_kind_of`]）的文件命名（[`RecordFace::sid`]）。通用层拿 sid 只经它。
pub fn sid_of(path: &Path) -> Option<String> {
    record_face_of(path).and_then(|r| (r.sid)(path))
}

/// 这份会话记录归的那一家的记录解释面（[`record_kind_of`]）。
pub fn record_face_of(path: &Path) -> Option<RecordFace> {
    record_face_among(REGISTRY, path)
}

/// [`record_face_of`] 的可喂夹具那一半。
pub fn record_face_among(registry: &[Adapter], path: &Path) -> Option<RecordFace> {
    record_kind_among(registry, path)
        .and_then(|k| adapter_among(registry, k))
        .and_then(|a| a.records)
}

/// 找项目目录时记录开头至多读多少字节。带它的那一条实测在前一 KiB 内；上界是给几百 MB 的大会话的：不整读。
pub const HEAD_CAP: cap_core::Cap = cap_core::Cap {
    bytes: 1 << 20,
    over: cap_core::Over::SkipAndSay,
};

/// 记录开头逐条交给 `pick`，第一个给出值的胜；读满 [`HEAD_CAP`] 字节就停（截在半截的那一行解析不出、不算）。
/// 读满了还没找到 ⇒ `None`，并在日志里说是哪份文件（调用方退到「没有这一格」）。
pub(crate) fn first_in_head(
    p: &Path,
    pick: impl Fn(&serde_json::Value) -> Option<String>,
) -> Option<String> {
    use std::io::{BufRead, Read};
    let file = Ro::open(p).ok()?;
    let mut r = std::io::BufReader::new(file.take(HEAD_CAP.bytes));
    let mut line = Vec::new();
    let mut read = 0u64;
    loop {
        line.clear();
        let n = r.read_until(b'\n', &mut line).ok()?;
        if n == 0 {
            if read >= HEAD_CAP.bytes {
                tracing::warn!(
                    "{}: nothing found within the head cap; giving up",
                    p.display()
                );
            }
            return None;
        }
        read += n as u64;
        let one = line.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&line);
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(one) {
            if let Some(got) = pick(&v) {
                return Some(got);
            }
        }
    }
}

/// 这份会话记录的项目目录：记录归 `registry` 里哪一家就问哪一家（[`RecordFace::project_dir`]）；那一家不声明 / 开头里没有 ⇒ `None`。
/// 读者照自己手上那一份注册表问（生产交 `REGISTRY`）。
pub fn project_dir_among(registry: &[Adapter], p: &Path) -> Option<String> {
    project_dir_as(registry, record_kind_among(registry, p)?, p)
}

/// 已知是哪一家的（按 sid 找到它的那一家）：照 `kind` 那一家要项目目录，不照路径另认；注册表里没有这一家 ⇒ `None`。
pub fn project_dir_as(registry: &[Adapter], kind: &str, p: &Path) -> Option<String> {
    adapter_among(registry, kind)?
        .records?
        .project_dir
        .and_then(|f| f(p))
}

/// 注册表里每一家的漂移账并成一份（注册序；不记的跳过）—— 帧命令 `drift-report` 的读法入口。
pub fn drift_report() -> drift::DriftReport {
    drift::DriftReport::of(
        REGISTRY
            .iter()
            .filter_map(|a| a.records.and_then(|r| r.drift))
            .flat_map(|f| f())
            .collect(),
    )
}

/// `kind` 那一家：这条记录是认不出、又不成卡的那种行 ⇒ 它的类型名（会话事实 `quietUnread` 记数用）。
pub fn quiet_unread_of(kind: &str, v: &serde_json::Value) -> Option<String> {
    record_face(kind)
        .and_then(|r| r.quiet_unread)
        .and_then(|f| f(v))
}
