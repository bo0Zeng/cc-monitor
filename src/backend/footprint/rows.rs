//! 足迹的判定——「cc-monitor 到底动过这台哪些文件、要的东西齐没齐」。
//!
//! 那台后端对它自己那台出整份足迹，`data-report` 照它重排（[`super::data`]）；
//! `HostScope::Client` 那一族（monitor 自己那台的东西）的**事实**仍由 monitor 答，判定只在这里。
//! Claude 布局（`~/.claude/…` 以哪个 agent 家为基准）住 `agents/claudecode/footprint.rs`。
//!
//! ## 一条硬纪律：**解析不了就说解析不了，绝不显示成"缺失"**
//!
//! 六个工具申报的路径里有四种本机根本查不到：远端路径（要 SSH）、相对项目目录的
//! `.mcp.json`（得先知道是哪个项目）、Windows 侧 `$PROFILE`（由 PowerShell 决定）、
//! 以及一层 glob（`~/.local/bin/cc-*`，这个能查但要另走一条路）。
//! 把这些一律画成红叉是**对能用的安装报假警报**——B04 审计已经抓过一次同型病
//! （只 `-x` 两个固定路径，于是装在 `/usr/local/bin` 且在 PATH 上的能用安装被报成"指不到"）。
//! 所以 [`SurfaceState`] 里没有"疑似缺失"这一档，只有 `Present` / `Absent` / `Undetermined`。
//! 三档不带话：`data-report` 只按档分（在的算「改过你的文件」、确认缺的进「要装」），没有读者的那句话不出。
//!
//! ## 只读
//!
//! 本模块不写任何用户文件（红线），也**不新增轮询**（红线）——一次按需扫完就返回。

use super::registry::{EnvBacking, EnvEntry, EnvTier};
use crate::agents::footprint_decl::{
    Carrier, EnvProbe, HostScope, ShellProfile, ToolDestination, ToolSpec, TouchedFile,
    USER_PATH_TOKEN,
};
use std::path::{Path, PathBuf};

/// 一条申报路径在**本机**能被解析到什么程度。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PathResolution {
    /// 本机一个确定的路径，可以直接查。
    Local(PathBuf),
    /// 本机一层 glob：在 `dir` 里找 `prefix*suffix`。
    LocalGlob {
        dir: PathBuf,
        prefix: String,
        suffix: String,
    },
    /// 远端路径。本页不连 SSH，所以查不到——**这不是"缺失"**。
    Remote,
    /// 相对某个项目目录，得先知道是哪个项目。
    NeedsProjectDir,
    /// Windows 侧 `$PROFILE`，路径由 PowerShell 决定。
    WindowsProfile,
    /// 路径由**用户配置**决定（用户选的 POSIX rc），本页查不到。
    NeedsUserConfig,
    /// 这台用户级 `PATH` 里我们那一段（不是文件）：在不在照探针 [`FsProbe::user_path`] 答。
    UserPath,
    /// **两端皆可**（`HostScope::Either`）：可以在本机查，但**"本机没找到" ≠ "不存在"**
    /// ——这东西也可能装在远端（Claude Code 跑在哪台，它就在哪台）。
    ///
    /// 这一个变体就是 T04 要修的那个假警报的解药：`cc-bus` 三条 touches 原先被当纯本机路径，
    /// 于是 Windows 客户端上审计页显示"不存在"，而驾驶舱正从远端读得好好的。
    ///
    /// ## 它**包住**一个本机解析结果，而不是自己存一个 `PathBuf`（T04 审计阻塞 1）
    ///
    /// 第一版是 `EitherHost { local: PathBuf }`，glob 形态被塞成
    /// `dir.join("cc-*")` ——于是 `observe` 去 stat 一个**字面含 `*` 的文件名**，
    /// 计数分支彻底走不到。实测：`~/.local/bin` 下真有 12 条 `cc-*`，
    /// 而 `ls -d ~/.local/bin/'cc-*'` → `No such file or directory`。
    /// 结果那一行从 T04 之前正确的「在」退化成「未确定」——对能用的安装报假警报，只是从红叉降级成了灰字。
    ///
    /// 包住内层之后 glob 计数**自动继承**，且"Either 绝不说 Absent"这个性质
    /// 变成一句话就能证明：只把内层的 `Absent` 改写成 `Undetermined`，其余原样透传。
    EitherHost(Box<PathResolution>),
}

/// 现状。**没有"疑似缺失"这一档**（见模块文档）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceState {
    Present,
    Absent,
    /// 查不了 —— 不是「缺」：`data-report` 不把它报成要装。
    Undetermined,
}

/// 注入的文件系统探针。做成注入是为了让**解析 + 观测**两步都能纯测
/// ——`sftp.rs` 那次教训：不可注入 = 不可测 = 那行代码没有门禁。
pub(crate) struct FsProbe<'a> {
    /// 返回 `(是否目录, 字节数)`；不存在返回 `None`。
    pub meta: &'a dyn Fn(&Path) -> Option<(bool, u64)>,
    /// 列一层目录里的**文件名**；读不了返回 `None`（≠ 空目录）。
    pub list: &'a dyn Fn(&Path) -> Option<Vec<String>>,
    /// `ccm` 所在目录在不在这台的用户级 `PATH` 上；问不出（不是那一台 · 起不了 PowerShell · 读不懂）返回 `None`（≠ 不在）。
    pub user_path: &'a dyn Fn() -> Option<bool>,
}

/// **从哪台机器上看**这张表 —— 「足迹」per-origin 那一维。
///
/// | 取值 | `home` / `agent_home` / 探针是谁的 | 远端落点（`HostScope::Remote`）| monitor 那台的落点（`HostScope::Client`）|
/// |---|---|---|---|
/// | [`Vantage::Monitor`] | monitor 所在那台（本机后端答）| 「远端路径，本页不连 SSH」| 真查（事实由 monitor 答）|
/// | [`Vantage::Remote`] | **那台远端**（它自己的后端答）| 真查（它就在这台上）| **不进表**（它不在这台上，`build_rows` 按人群滤掉）|
///
/// `Either` 两边都「真查，但查不到 ≠ 不存在」（它也可能装在另一台上）。
/// ⚠ 判定只有这一份：两种视角走同一个 [`build_rows`]，差的只是探针从哪来、`Client` 那一族在不在人群里。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Vantage {
    /// monitor 所在那台。
    Monitor,
    /// 某台远端（探针是它的后端答的）。
    Remote,
}

// agent 家由调用方按 `agents/claudecode/paths.rs` 那一条规则算好交进来（[`SurfaceEnv::agent_home`]）。

/// 把申报路径解析成本机可查的形态。
///
/// **「本机还是远端」从 `dest` 推导，不新增字段**（`TouchedFile` 的文档写了理由）。
/// `~/.claude/...` 以 `agent_home` 为基准（Claude 布局那一条：`agents/claudecode/footprint.rs::under_agent_home`）——
/// 那条 `CLAUDE_CONFIG_DIR` 规则只准解释一次，在调用方算 `agent_home` 的那一处。
///
/// 第一个参数是视角（[`Vantage`]）：判据一律显式写 `Vantage::Monitor`。
/// `None` ＝ 申报自相矛盾（散文 · 落点与路径对不上 · glob 写法不许）：那一行报「查不动」，不静默跳过。
pub(crate) fn resolve_touched_path(
    vantage: Vantage,
    declared: &str,
    dest: &ToolDestination,
    host: HostScope,
    home: &Path,
    agent_home: &Path,
) -> Option<PathResolution> {
    // **先把散文挡在门外。** 这条是被自己的反向自检抓出来加的：
    // `~/.local/bin/cc-*（12 条软链）` 原先能"成功"解析成
    // `LocalGlob { prefix: "cc-", suffix: "（12 条软链）" }`——glob 分支把散文吞进了 suffix，
    // 于是审计页会去找一个名叫 `cc-*（12 条软链）` 的东西，永远 0 匹配，
    // 而表格上显示的是干干净净的"缺失"。**比报错更坏**。
    //
    // 判据是白名单而不是"不许出现哪些坏字符"：申报路径的每个字符必须是
    // **ASCII graphic**（字母数字 + 标点，不含空白）。本仓六个工具的真实路径全部满足；
    // 散文一定不满足（全角括号、汉字、空格任一即出局）。
    // **已知代价如实写明**：真含空格或非 ASCII 的路径也会被拒——那种情况得显式加支持，
    // 而不是靠这条判据放水，因为放水就等于把散文一起放进来。
    if !declared.chars().all(|c| c.is_ascii_graphic()) {
        return None;
    }
    // **顺序要紧：先按 `destination` 全量校验，再用 `host` 做投影。**
    // 第一版是 host 优先短路，于是 `LocalHomeRelative` 那条"必须以 `~/` 开头"
    // 与"glob 只许在最后一段、只许一个 `*`"的校验，对所有 `Remote` / `Either` 的
    // touches **完全不再执行**。
    //
    // **数字更正**（T04 审计重要 6）：我原先三处都写"10 条里 7 条"——是 **8** 条
    // （`Remote` 5 + `Either` 3）。而且真正受影响的更少：那两条校验只在
    // `resolve_local_home` 里，8 条中有 4 条（`RemoteHomeRelative`×2 + 占位符×2）
    // 本来就不经过它 —— **实际被短路掉的是 4 条**。修复是对的，描述夸大了一倍。
    // host 是"在哪台机器上"，destination 是"装到哪"，两者独立；
    // 但**校验属于后者，不能被前者跳过**。
    //
    // （更正我自己上一版注释里说过头的一句：我写"`UserConfiguredPath` 的占位符校验
    //  变成死代码"——不对。那条 `Err` 分支在更早一步就已经改成了"不是占位符就按本机路径解析"，
    //  本来就没有可被跳过的校验。真正被短路掉的是上面那两条。）
    let by_dest = resolve_by_destination(vantage, declared, dest, home, agent_home)?;
    Some(project_onto_host(vantage, by_dest, host))
}

/// `host` 只改写**本机可解析**的那两种结果，其余原样透传。
///
/// 为什么不是"host 说远端就一律返回 Remote"：本机解析不了的那几种（`NeedsUserConfig` · `NeedsProjectDir` · `WindowsProfile`）
/// 本来就答「查不动」，改写成 `Remote` 只会丢掉「为什么查不动」那一层区分。
fn project_onto_host(vantage: Vantage, by_dest: PathResolution, host: HostScope) -> PathResolution {
    // 从远端那台看：`Remote` 落点**就在这台上**，本机解析的结果原样留下（探针是这台的后端）。
    //   `Client` 那一族在这个视角下不进人群（`build_rows` 滤掉），走不到这里；`Either` 与下面同一条。
    if vantage == Vantage::Remote && host == HostScope::Remote {
        return by_dest;
    }
    match (host, by_dest) {
        // 远端：本机**不许**替它回答"路径在不在"（T03 阻塞 3 的根因）。
        //
        // **如实登记**（T04 审计重要 3）：`(Remote, LocalGlob)` 今天**走不到**
        // ——唯一的 glob（`~/.local/bin/cc-*`）是 `Either`。留着这一支不是装样子：
        // 它和上一行是同一条性质的两半，删掉半边会让"远端不许本机作答"这句话
        // 在下一个 glob 型远端 touch 出现时**静默失守**。
        // 同理 `(Client, *)` 全落 `(_, other)`：解析不看它（`Client` 管的是谁来答，见 `build_rows`）。
        (HostScope::Remote, PathResolution::Local(_))
        | (HostScope::Remote, PathResolution::LocalGlob { .. }) => PathResolution::Remote,
        // 两端皆可：可以在本机查（**含 glob 计数**，因为内层原样保留），但查不到 ≠ 不存在
        (HostScope::Either, inner @ PathResolution::Local(_))
        | (HostScope::Either, inner @ PathResolution::LocalGlob { .. }) => {
            PathResolution::EitherHost(Box::new(inner))
        }
        (_, other) => other,
    }
}

fn resolve_by_destination(
    vantage: Vantage,
    declared: &str,
    dest: &ToolDestination,
    home: &Path,
    agent_home: &Path,
) -> Option<PathResolution> {
    match dest {
        ToolDestination::UserShellProfile(p) => {
            // 落点是"用户选的 profile"却申报了别的路径 → 声明自相矛盾，宁可报错
            if declared != p.token() {
                return None;
            }
            Some(match p {
                ShellProfile::PowerShell => PathResolution::WindowsProfile,
                // 用户选的那一份：路径是配置项 ⇒ 查不动，不发明一条路径。
                ShellProfile::PosixRc => PathResolution::NeedsUserConfig,
            })
        }
        ToolDestination::ProjectRelative(_) => {
            if declared.starts_with('~') || declared.starts_with('/') {
                return None;
            }
            Some(PathResolution::NeedsProjectDir)
        }
        // 从远端那台看，「远端家目录相对」就是**这台**的家目录相对 ⇒ 照本机路径解析（`home` 是那台的）。
        ToolDestination::RemoteHomeRelative(_) => match vantage {
            Vantage::Monitor => Some(PathResolution::Remote),
            Vantage::Remote => resolve_local_home(declared, home, agent_home),
        },
        // 一个工具两个落点 ⇒ 两个载体（`ccm`：远端 `RemoteHomeRelative`、本机 `LocalHomeRelative`，各带各的 touch），
        //    这里没有双值那一臂。理由住 `agents::footprint_decl::Carrier` 的头注。
        ToolDestination::LocalHomeRelative(_) => resolve_local_home(declared, home, agent_home),
        // 「不是我们装的」**不等于「查不了」** —— 恰恰相反：
        // 这一档的全部意义就是「我们查得到它在不在，但装不了」。
        // ⇒ 照本机路径解析（`host` 是 `Either` 时 `project_onto_host` 会再包一层，
        //    于是「本机没找到」照样不会被说成「不存在」）。
        ToolDestination::NotInstalledByUs => resolve_local_home(declared, home, agent_home),
        // 不是文件：申报路径只许是那一个占位（注册表里的位置），别的写法 ⇒ 自相矛盾。
        ToolDestination::UserPath => {
            (declared == USER_PATH_TOKEN).then_some(PathResolution::UserPath)
        }
    }
}

/// 解析一个 `~/…` 形态的**本机**路径。抽出来是因为两条臂共用它
/// （`LocalHomeRelative` 与 `NotInstalledByUs`）。
fn resolve_local_home(declared: &str, home: &Path, agent_home: &Path) -> Option<PathResolution> {
    let rest = declared.strip_prefix("~/")?;
    // `~/.claude/…` 的真实基准目录是 `CLAUDE_CONFIG_DIR`（若它确实是个目录）
    // 哪一段算「agent 家底下」是那一家的布局知识（注册表里的足迹面答，`agents/claudecode/footprint.rs`）。
    let under = crate::agents::footprint_faces().find_map(|f| (f.under_agent_home)(rest));
    let (base, rel) = match under {
        Some(r) => (agent_home.to_path_buf(), r.to_string()),
        None => (home.to_path_buf(), rest.to_string()),
    };
    let rel = rel.trim_end_matches('/');
    if rel.is_empty() {
        return None;
    }
    // glob 只允许出现在**最后一段**，且只允许一个 `*`
    let (dir_part, last) = match rel.rsplit_once('/') {
        Some((d, l)) => (Some(d), l),
        None => (None, rel),
    };
    if dir_part.is_some_and(|d| d.contains('*')) {
        return None;
    }
    if let Some((prefix, suffix)) = last.split_once('*') {
        if suffix.contains('*') {
            return None;
        }
        let dir = match dir_part {
            Some(d) => base.join(native_rel(d, std::path::MAIN_SEPARATOR)),
            None => base,
        };
        return Some(PathResolution::LocalGlob {
            dir,
            prefix: prefix.to_string(),
            suffix: suffix.to_string(),
        });
    }
    Some(PathResolution::Local(
        base.join(native_rel(rel, std::path::MAIN_SEPARATOR)),
    ))
}

/// 〔WIN3 §2〕申报表里的 `~/` 之后那段用 `/` 写；拼到本机家目录上时换成这台的分隔符 ——
/// 否则 Windows 上足迹页是 `C:\Users\u\.cc-monitor/bin/ccm*` 这样正反斜杠混拼。`sep` 由调用方给（判据喂 `\`）。
pub(crate) fn native_rel(rel: &str, sep: char) -> String {
    rel.replace('/', &sep.to_string())
}

/// 观测一条已解析的路径。
pub(crate) fn observe(res: &PathResolution, fs: &FsProbe) -> SurfaceState {
    match res {
        PathResolution::Local(p) => match (fs.meta)(p) {
            None => SurfaceState::Absent,
            // 目录在但列不了（权限）——**不能说成"空目录"**，也不算在
            Some((true, _)) if (fs.list)(p).is_none() => SurfaceState::Undetermined,
            Some(_) => SurfaceState::Present,
        },
        PathResolution::LocalGlob {
            dir,
            prefix,
            suffix,
        } => match (fs.list)(dir) {
            None => SurfaceState::Undetermined,
            Some(names) => {
                let hit = names.iter().any(|s| {
                    s.len() >= prefix.len() + suffix.len()
                        && s.starts_with(prefix.as_str())
                        && s.ends_with(suffix.as_str())
                });
                if hit {
                    SurfaceState::Present
                } else {
                    SurfaceState::Absent
                }
            }
        },
        // **本机没找到 ≠ 不存在**：这是 T04 的核心语义。绝不返回 `Absent`。
        //
        // 实现就一句话：**递归观测内层，只把 `Absent` 改写成 `Undetermined`**。
        // 这样 glob 匹配、目录列举、"列不出来"那档全部自动继承，
        // 而"Either 绝不说 Absent"这个性质由这个 match 直接可证（第一版自己存 `PathBuf`，
        // glob 被拍成字面 `cc-*` 去 stat，匹配分支永远走不到——审计阻塞 1）。
        PathResolution::EitherHost(inner) => match observe(inner, fs) {
            SurfaceState::Absent | SurfaceState::Undetermined => SurfaceState::Undetermined,
            SurfaceState::Present => SurfaceState::Present,
        },
        // 用户配置 · 远端 · 项目目录 · `$PROFILE`：本机都查不动（`$PROFILE` 在别的平台上是不适用，同样不说缺）。
        PathResolution::NeedsUserConfig => SurfaceState::Undetermined,
        PathResolution::Remote => SurfaceState::Undetermined,
        PathResolution::NeedsProjectDir => SurfaceState::Undetermined,
        PathResolution::WindowsProfile => SurfaceState::Undetermined,
        PathResolution::UserPath => match (fs.user_path)() {
            Some(true) => SurfaceState::Present,
            Some(false) => SurfaceState::Absent,
            None => SurfaceState::Undetermined,
        },
    }
}

/// 足迹里的一行：`data-report` 照它重排（改过你的文件 · 要装），不上线。
#[derive(Debug, Clone)]
pub struct SurfaceRow {
    pub tool_id: &'static str,
    pub tool_name: String,
    /// 档：「不该由我们装」与「该我们装而还没写」按**值**分，不按措辞猜。
    pub tier: EnvTier,
    pub path_declared: &'static str,
    pub state: SurfaceState,
}

/// 🔴 **多收一个 `c`（载体），而那正是这一格买到的东西。**
///
/// 只拿 `t.destination` 与 `t.source`（一个工具一份）的话，
/// 「同一个后端的几个落点」这件事**在这一行里根本表达不出来**：
/// 每条 touch 都被配上同一个 `destination`，而真相是 M 个落点 × N 条 touch。
/// touch 挂在载体下 ⇒ **配对是天然的**，
/// 这个函数拿到的 `(c, f)` 一定是同一份产物的落点与它碰的文件。
fn row(
    e: &EnvEntry,
    t: &'static ToolSpec,
    c: &'static Carrier,
    f: &'static TouchedFile,
    env: &SurfaceEnv,
) -> SurfaceRow {
    let SurfaceEnv {
        home,
        agent_home,
        fs,
        vantage,
        ..
    } = *env;
    // **声明自相矛盾也要如实报**（查不动），不能静默跳过一行——那会让表看着很干净而实际漏了东西
    let state = resolve_touched_path(vantage, f.path, &c.destination, f.host, home, agent_home)
        .map_or(SurfaceState::Undetermined, |r| observe(&r, fs));
    SurfaceRow {
        tool_id: t.id,
        tool_name: t.display_name.get(),
        tier: e.tier,
        path_declared: f.path,
        state,
    }
}

/// 建一次表要用的那一套基准与探针。**全部注入** —— 本模块不自己去摸环境，
/// 那是它从第一天起就可纯测的原因。
///
/// 它是具名结构而不是 `build_rows` 的一串位置参数：五六个位置参数往下传两层，谁也读不出哪个是哪个；
/// 具名结构加一样不会让每个调用点都改一遍。
pub(crate) struct SurfaceEnv<'a> {
    pub home: &'a Path,
    /// agent 家（`~/.claude/…` 的基准），调用方按 `agents/claudecode/paths.rs` 那一条规则算好交进来。
    pub agent_home: &'a Path,
    pub fs: &'a FsProbe<'a>,
    /// `$PATH` 原样。
    ///
    /// 🔴 **`None` = 取不到，不是「空的」** —— 那时 [`EnvProbe::OnPath`] 那一族一律
    /// 「查不动」（`Undetermined`），**绝不说成「不存在」**（找法 `platform::shell::find_session_command` 答「判不了」）。
    pub path_env: Option<&'a str>,
    /// 起会话那个 shell 的 `PATH`（[`EnvProbe::InSessionShell`] 那一族按它判）。`None` = 问不出来 ⇒ 「查不动」。
    pub session_path: Option<&'a str>,
    /// `%SystemRoot%`（[`EnvProbe::SshClient`] 在 Windows 上先查它下面的 OpenSSH）。`None` = 取不到。
    pub system_root: Option<&'a str>,
    /// 这台是不是 Windows（[`EnvProbe::SshClient`] 按它找 `ssh.exe` 还是 `ssh`）。
    pub windows: bool,
    /// 从哪台机器上看（见 [`Vantage`]）。探针与上面几样必须是**同一台**的。
    pub vantage: Vantage,
}

/// 清单里**没有 `ToolSpec`** 的那一项，在这一页上长什么样。
///
/// # 🔴 这个函数的正题是「查」，不是「不查」
///
/// 通用工具是「**用户自己装，而我们会看、缺了要说**」
/// ⇒ 这里**真去查**，而查出来的三种答案在这一页上是三回事：
///
/// | 探针答什么 | 这一行显示成 | 能不能说「缺」 |
/// |---|---|---|
/// | 在 | `Present` | —— |
/// | **查了、确认没有** | `Absent` | 能（「待办」据此劝人去装） |
/// | **查不动** | `Undetermined` | 不能（说「缺」就是替用户下一个他没做过的结论） |
///
/// ⚠ 中间那一行是载体这一格买到的东西：没有它时它和最后一行**长得一模一样**。
fn unmanaged_row(
    e: &EnvEntry,
    named: &'static str,
    probe: EnvProbe,
    env: &SurfaceEnv,
) -> SurfaceRow {
    SurfaceRow {
        tool_id: e.id,
        tool_name: e.display_name.clone(),
        tier: e.tier,
        path_declared: named,
        state: observe_unmanaged(named, probe, env),
    }
}

/// 手写那一半**真去查**的那一步。抽出来是因为它是死值验的落点：
/// 同一个名字换一种 [`EnvProbe`]，出来的必须是不同的一格。
fn observe_unmanaged(named: &'static str, probe: EnvProbe, env: &SurfaceEnv) -> SurfaceState {
    match probe {
        // `PATH` 上的裸命令：与起新会话框、`ccm` 起它那一下同一个找法（`platform::shell::find_session_command`，
        // Windows 上 `.exe` / `.cmd` / `.ps1` … 的规矩只住那里）；两族只差按哪一份 `PATH` 找。
        // agent 命令行在起会话那个 shell 里敲 ⇒ 那个 shell 的 `PATH`；别的按这个后端进程的。
        EnvProbe::OnPath | EnvProbe::InSessionShell => {
            let path = match probe {
                EnvProbe::InSessionShell => env.session_path,
                _ => env.path_env,
            };
            match crate::platform::shell::find_session_command(named, path, env.windows, &|p| {
                matches!((env.fs.meta)(p), Some((false, _)))
            }) {
                // 🔴 **查不动**：`PATH` 读不到。绝不说成「不存在」——
                // 那会对一台装得好好的机器报假警报（本模块头注那条硬纪律）。
                crate::platform::shell::SessionCommand::Unknown => SurfaceState::Undetermined,
                // **查了、确认没有** —— 这一格才是「缺」，「待办」据此劝人去装。
                crate::platform::shell::SessionCommand::Absent => SurfaceState::Absent,
                crate::platform::shell::SessionCommand::Found { .. } => SurfaceState::Present,
            }
        }
        // ssh 客户端：与开远端终端同一个找法，查文件那一下用本页注入的探针。
        EnvProbe::SshClient => {
            let probe = |p: &Path| Ok(matches!((env.fs.meta)(p), Some((false, _))));
            match crate::platform::ssh_client::locate_with(
                env.windows,
                env.system_root.map(std::ffi::OsStr::new),
                env.path_env.map(std::ffi::OsStr::new),
                &probe,
            ) {
                crate::platform::ssh_client::SshClient::At(_) => SurfaceState::Present,
                crate::platform::ssh_client::SshClient::Missing => SurfaceState::Absent,
                crate::platform::ssh_client::SshClient::Unknown(_) => SurfaceState::Undetermined,
            }
        }
        EnvProbe::CannotProbe => SurfaceState::Undetermined,
    }
}

/// 遍历**环境清单的闭集**建表。纯函数（探针全从 [`SurfaceEnv`] 注入）。
///
/// 🔴 **人群从 `TOOLS` 换成了 [`environment`]。**
/// 原来它只遍历 `TOOLS` 的 `touches`，于是这一页能答的是模块头注那句
/// 「cc-monitor 动过你哪些文件」，而**答不了**「app 要的东西齐了没有」——
/// 后者的人群里有一整档是 app 装不了也不查的东西，它们一条都不在 `TOOLS` 里。
/// 拿前者当后者用是**分母对不上**。
/// 钉住它的是 `the_view_population_is_exactly_the_closed_set`。
///
/// `client` 有 ⇒ `HostScope::Client` 那一族改由它答：本机那一栏的事实问本机后端，
/// 只有 monitor 自己那台的东西留在 monitor 进程里查。
pub(crate) fn build_rows(env: &SurfaceEnv, client: Option<&SurfaceEnv>) -> Vec<SurfaceRow> {
    // 从远端那台看：住 monitor 所在那台的那一族（`HostScope::Client`）**不在这台上** ⇒ 不进人群。
    //   不是「查不到」，是「这一格不属于这台机器」—— 画成一行「未确定」反而是在说一句关于这台的假话。
    let pick = |h: HostScope| match (h, client) {
        (HostScope::Client, Some(c)) => Some(c),
        (HostScope::Client, None) if env.vantage == Vantage::Remote => None,
        _ => Some(env),
    };
    super::registry::environment()
        .iter()
        .flat_map(|e| match e.backing {
            // 有 ToolSpec ⇒ 路径 / effect / host 全从那一份读，这里一个字都不复述
            // 🔴 人群从「工具 × touch」变成「工具 × **载体** × touch」——
            //    行数不变（touch 总数没变过），变的是**每一行知道自己属于哪一份产物**。
            EnvBacking::Managed(t) => t
                .carrier_touches()
                .filter_map(|(c, f)| pick(f.host).map(|env| row(e, t, c, f, env)))
                .collect::<Vec<_>>(),
            EnvBacking::Named { named, host, probe } => pick(host)
                .map(|env| unmanaged_row(e, named, probe, env))
                .into_iter()
                .collect(),
        })
        .collect()
}

/// 这台的整份足迹（`data-report` 的原料，不上线）：各行 ＋ 这台的家目录（「改过你的文件」照它写 `~/…`）。
#[derive(Debug, Clone)]
pub struct ConfigSurfaceReport {
    pub rows: Vec<SurfaceRow>,
    pub home: String,
}

#[cfg(test)]
#[path = "../../../tests/backend/footprint/rows_tests.rs"]
mod tests;
