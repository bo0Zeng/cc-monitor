//! T02：**配置面审计视图**的判定——「cc-monitor 到底动过你哪些文件」。
//!
//! 〔一处后端〕从 monitor
//! `config_surface.rs` 搬进后端：那台后端对它自己那台出整份成品（帧命令 `footprint-report`，face 在 [`super`]）；
//! `HostScope::Client` 那一族（monitor 自己那台的东西）的**事实**仍由 monitor 答，判定只在这里。
//! Claude 布局（`~/.claude/…` 以哪个 agent 家为基准 · settings 两个作用域）住 `agents/claudecode/footprint.rs`。
//!
//! ## 它同时是 T01 那笔债的清算
//!
//! T01 收工时 `tool_registry::TOOLS` **零生产消费者**，我当时明写了处置条件：
//! T02 收工若仍无消费者就删掉注册表（同一轮我以「只有测试在用」为由删过
//! `WriteVerdict::is_ok`，尺子得一致）。本模块把 `ToolSpec` 的**七个字段全部用上**：
//! `id`/`display_name` 分组，`source` 进「从哪来」列，`destination` **决定这条路径
//! 在本机还是远端**，`installable`/`uninstallable` 进「能否装/撤」列，`touches` 是表格主体。
//!
//! ## 一条硬纪律：**解析不了就说解析不了，绝不显示成"缺失"**
//!
//! 六个工具申报的路径里有四种本机根本查不到：远端路径（要 SSH）、相对项目目录的
//! `.mcp.json`（得先知道是哪个项目）、Windows 侧 `$PROFILE`（由 PowerShell 决定）、
//! 以及一层 glob（`~/.local/bin/cc-*`，这个能查但要另走一条路）。
//! 把这些一律画成红叉是**对能用的安装报假警报**——B04 审计已经抓过一次同型病
//! （只 `-x` 两个固定路径，于是装在 `/usr/local/bin` 且在 PATH 上的能用安装被报成"指不到"）。
//! 所以 [`SurfaceState`] 里没有"疑似缺失"这一档，只有 `Present` / `Absent` /
//! `Undetermined { why }`，**而 `why` 是必填的**。
//!
//! ## 只读
//!
//! 本模块不写任何用户文件（红线），也**不新增轮询**（红线）——一次按需扫完就返回。

use super::registry::{
    Carrier, EnvBacking, EnvEntry, EnvProbe, EnvTier, HostScope, ToolDestination, ToolSource,
    ToolSpec, TouchEffect, TouchedFile,
};
use copy_core::copy_text;
use std::path::{Path, PathBuf};

/// 一条申报路径在**本机**能被解析到什么程度。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathResolution {
    /// 本机一个确定的路径，可以直接查。
    Local(PathBuf),
    /// 本机一层 glob：在 `dir` 里找 `prefix*suffix`。
    LocalGlob {
        dir: PathBuf,
        prefix: String,
        suffix: String,
    },
    /// 远端路径。本页不连 SSH，所以查不到——**这不是"缺失"**。
    Remote(String),
    /// 相对某个项目目录，得先知道是哪个项目。
    NeedsProjectDir(String),
    /// Windows 侧 `$PROFILE`，路径由 PowerShell 决定。
    WindowsProfile,
    /// 路径由**用户配置**决定，本页查不到——`what` 告诉用户去哪儿看那个值。
    NeedsUserConfig { what: String },
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
    /// 结果那一行从 T04 之前正确的「12 项匹配」退化成
    /// 「未确定 —— 本机 …/cc-* 不存在」——`why` 里陈述了一个**假事实**。
    /// 这正是本模块文档禁止的"对能用的安装报假警报"，只是从红叉降级成了带谎话的灰字。
    ///
    /// 包住内层之后 glob 计数**自动继承**，且"Either 绝不说 Absent"这个性质
    /// 变成一句话就能证明：只把内层的 `Absent` 改写成 `Undetermined`，其余原样透传。
    EitherHost(Box<PathResolution>),
}

/// 现状。**没有"疑似缺失"这一档**（见模块文档）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SurfaceState {
    Present {
        detail: String,
    },
    Absent,
    /// 查不了。`why` **必填**：用户看到"未确定"时必须知道为什么，
    /// 否则它和"缺失"在观感上没区别，而那正是本模块要避免的假警报。
    Undetermined {
        why: String,
    },
}

/// 注入的文件系统探针。做成注入是为了让**解析 + 观测**两步都能纯测
/// ——`sftp.rs` 那次教训：不可注入 = 不可测 = 那行代码没有门禁。
pub struct FsProbe<'a> {
    /// 返回 `(是否目录, 字节数)`；不存在返回 `None`。
    pub meta: &'a dyn Fn(&Path) -> Option<(bool, u64)>,
    /// 列一层目录里的**文件名**；读不了返回 `None`（≠ 空目录）。
    pub list: &'a dyn Fn(&Path) -> Option<Vec<String>>,
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
pub enum Vantage {
    /// monitor 所在那台。
    Monitor,
    /// 某台远端（探针是它的后端答的）。
    Remote,
}

// `claude_config_dir`〔散文墓碑〕删了：agent 家由调用方按 `agents/claudecode/paths.rs` 那一条规则算好交进来（[`SurfaceEnv::agent_home`]）。

/// 按 `$PATH` 逐目录反查一个裸命令在不在（`exists` 注入）。切分走 `std::env::split_paths`（T03 阻塞 1：
/// 写死 `':'` 在 Windows 上把盘符切碎、给出确定的否定答案）；`path_env` 取不到 / 空 ⇒ `None`（不猜）。
/// 从 `hooks_diag.rs` 挪来。
pub fn resolves_on_path(
    prog: &str,
    path_env: Option<&str>,
    exists: &dyn Fn(&str) -> bool,
) -> Option<bool> {
    let pe = path_env?;
    if pe.trim().is_empty() {
        return None;
    }
    Some(std::env::split_paths(pe).any(|d| {
        let d = d.to_string_lossy();
        let d = d.trim_end_matches(['/', '\\']);
        !d.is_empty() && exists(&format!("{d}/{prog}"))
    }))
}

/// 把申报路径解析成本机可查的形态。
///
/// **「本机还是远端」从 `dest` 推导，不新增字段**（`TouchedFile` 的文档写了理由）。
/// `~/.claude/...` 以 `agent_home` 为基准（Claude 布局那一条：`agents/claudecode/footprint.rs::under_agent_home`）——
/// 那条 `CLAUDE_CONFIG_DIR` 规则只准解释一次，在调用方算 `agent_home` 的那一处。
///
/// 第一个参数是视角（[`Vantage`]）：本机视角与先前逐字同一个行为，判据一律显式写 `Vantage::Monitor`。
pub fn resolve_touched_path(
    vantage: Vantage,
    declared: &str,
    dest: &ToolDestination,
    host: HostScope,
    home: &Path,
    agent_home: &Path,
) -> Result<PathResolution, String> {
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
    if let Some(bad) = declared.chars().find(|c| !c.is_ascii_graphic()) {
        return Err(copy_text(
            "rsConfigSurface.declared.badChar",
            &[
                ("bad", &format!("{:?}", bad)),
                ("declared", &format!("{:?}", declared)),
            ],
        ));
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
    Ok(project_onto_host(vantage, by_dest, host, declared))
}

/// `host` 只改写**本机可解析**的那两种结果，其余原样透传。
///
/// 为什么不是"host 说远端就一律返回 Remote"：`UserConfiguredPath` 解析出的
/// `NeedsUserConfig { what }` 比 `Remote` **信息更多**（它告诉用户去哪儿看那个值），
/// 覆盖掉是降级。
fn project_onto_host(
    vantage: Vantage,
    by_dest: PathResolution,
    host: HostScope,
    declared: &str,
) -> PathResolution {
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
        // 同理 `(Client, *)` 与 `(ProjectDir, *)` 全落 `(_, other)`——这两个变体
        // 今天**纯粹是标签**，不改变任何行为；它们的价值在 `host_label` 上屏那一侧。
        (HostScope::Remote, PathResolution::Local(_))
        | (HostScope::Remote, PathResolution::LocalGlob { .. }) => {
            PathResolution::Remote(declared.to_string())
        }
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
) -> Result<PathResolution, String> {
    match dest {
        ToolDestination::UserShellProfile => {
            if declared == "$PROFILE" {
                Ok(PathResolution::WindowsProfile)
            } else {
                // 落点是"用户选的 profile"却申报了别的路径 → 声明自相矛盾，宁可报错
                Err(copy_text(
                    "rsConfigSurface.declared.profileMismatch",
                    &[("declared", &format!("{:?}", declared))],
                ))
            }
        }
        ToolDestination::ProjectRelative(_) => {
            if declared.starts_with('~') || declared.starts_with('/') {
                return Err(copy_text(
                    "rsConfigSurface.declared.projectMismatch",
                    &[("declared", &format!("{:?}", declared))],
                ));
            }
            Ok(PathResolution::NeedsProjectDir(declared.to_string()))
        }
        // **占位符只对应"落点"那一条，别的 touches 照常解析。**
        // 第一版这条臂要求**每条** touches 都等于占位符，于是 cc-acct-iso 的
        // 账号库那一条（**T04 查证：它在远端**，`accounts.rs` 全走 ssh exec，
        // 我这句原先写的"本机账号库"是错的）被判违规——
        // 落点只是这个工具碰的文件之一，不是全部。测试当场红在这里。
        ToolDestination::UserConfiguredPath { token, what } => {
            if declared == *token {
                Ok(PathResolution::NeedsUserConfig { what: what.get() })
            } else {
                resolve_local_home(declared, home, agent_home)
            }
        }
        // 从远端那台看，「远端家目录相对」就是**这台**的家目录相对 ⇒ 照本机路径解析（`home` 是那台的）。
        ToolDestination::RemoteHomeRelative(_) => match vantage {
            Vantage::Monitor => Ok(PathResolution::Remote(declared.to_string())),
            Vantage::Remote => resolve_local_home(declared, home, agent_home),
        },
        // 🔴 `BothHomeRelative` 那一臂删了 —— 墓碑住 `tool_registry::Carrier`
        //    的头注。一句话：那个变体是为「一个 `destination` 装不下两个落点」造的，
        //    而载体这一维立起来之后那个前提没了（`ccm` 现在是两个载体，
        //    远端那个 `RemoteHomeRelative`、本机那个 `LocalHomeRelative`，各带各的 touch）。
        ToolDestination::LocalHomeRelative(_) => resolve_local_home(declared, home, agent_home),
        // 「不是我们装的」**不等于「查不了」** —— 恰恰相反：
        // 这一档的全部意义就是「我们查得到它在不在，但装不了」。
        // ⇒ 照本机路径解析（`host` 是 `Either` 时 `project_onto_host` 会再包一层，
        //    于是「本机没找到」照样不会被说成「不存在」）。
        ToolDestination::NotInstalledByUs { .. } => resolve_local_home(declared, home, agent_home),
    }
}

/// 解析一个 `~/…` 形态的**本机**路径。抽出来是因为两条臂共用它
/// （`LocalHomeRelative`，以及 `UserConfiguredPath` 里那些**不是**落点占位符的 touches）。
fn resolve_local_home(
    declared: &str,
    home: &Path,
    agent_home: &Path,
) -> Result<PathResolution, String> {
    let rest = declared.strip_prefix("~/").ok_or_else(|| {
        copy_text(
            "rsConfigSurface.home.notTilde",
            &[("declared", &format!("{:?}", declared))],
        )
    })?;
    // `~/.claude/…` 的真实基准目录是 `CLAUDE_CONFIG_DIR`（若它确实是个目录）
    // 哪一段算「agent 家底下」是那一家的布局知识（注册表里的足迹面答，`agents/claudecode/footprint.rs`）。
    let under = crate::agents::footprint_faces().find_map(|f| (f.under_agent_home)(rest));
    let (base, rel) = match under {
        Some(r) => (agent_home.to_path_buf(), r.to_string()),
        None => (home.to_path_buf(), rest.to_string()),
    };
    let rel = rel.trim_end_matches('/');
    if rel.is_empty() {
        return Err(copy_text(
            "rsConfigSurface.home.empty",
            &[("declared", &format!("{:?}", declared))],
        ));
    }
    // glob 只允许出现在**最后一段**，且只允许一个 `*`
    let (dir_part, last) = match rel.rsplit_once('/') {
        Some((d, l)) => (Some(d), l),
        None => (None, rel),
    };
    if dir_part.is_some_and(|d| d.contains('*')) {
        return Err(copy_text(
            "rsConfigSurface.home.globNotLast",
            &[("declared", &format!("{:?}", declared))],
        ));
    }
    if let Some((prefix, suffix)) = last.split_once('*') {
        if suffix.contains('*') {
            return Err(copy_text(
                "rsConfigSurface.home.multiStar",
                &[("declared", &format!("{:?}", declared))],
            ));
        }
        let dir = match dir_part {
            Some(d) => base.join(native_rel(d, std::path::MAIN_SEPARATOR)),
            None => base,
        };
        return Ok(PathResolution::LocalGlob {
            dir,
            prefix: prefix.to_string(),
            suffix: suffix.to_string(),
        });
    }
    Ok(PathResolution::Local(
        base.join(native_rel(rel, std::path::MAIN_SEPARATOR)),
    ))
}

/// 〔WIN3 §2〕申报表里的 `~/` 之后那段用 `/` 写；拼到本机家目录上时换成这台的分隔符 ——
/// 否则 Windows 上足迹页是 `C:\Users\u\.cc-monitor/bin/ccm*` 这样正反斜杠混拼。`sep` 由调用方给（判据喂 `\`）。
pub(crate) fn native_rel(rel: &str, sep: char) -> String {
    rel.replace('/', &sep.to_string())
}

/// 展示用：这条解析指向的东西（含 glob pattern）。
fn describe_target(r: &PathResolution) -> String {
    describe_target_with(r, std::path::MAIN_SEPARATOR)
}

/// [`describe_target`] 的本体，分隔符由调用方给（判据喂 Windows 那一个）。
pub(crate) fn describe_target_with(r: &PathResolution, sep: char) -> String {
    match r {
        PathResolution::Local(p) => p.to_string_lossy().into_owned(),
        PathResolution::LocalGlob {
            dir,
            prefix,
            suffix,
        } => format!("{}{sep}{prefix}*{suffix}", dir.display()),
        PathResolution::EitherHost(inner) => describe_target_with(inner, sep),
        PathResolution::Remote(p) | PathResolution::NeedsProjectDir(p) => p.clone(),
        PathResolution::NeedsUserConfig { what } => what.clone(),
        PathResolution::WindowsProfile => "$PROFILE".to_string(),
    }
}

/// 观测一条已解析的路径。
pub fn observe(res: &PathResolution, fs: &FsProbe) -> SurfaceState {
    match res {
        PathResolution::Local(p) => match (fs.meta)(p) {
            None => SurfaceState::Absent,
            Some((true, _)) => match (fs.list)(p) {
                Some(v) => SurfaceState::Present {
                    detail: copy_text(
                        "rsConfigSurface.observe.dirCount",
                        &[("count", &(v.len()).to_string())],
                    ),
                },
                // 目录在但列不了（权限）——**不能说成"空目录"**
                None => SurfaceState::Undetermined {
                    why: copy_text("rsConfigSurface.observe.dirUnlistable", &[]).into(),
                },
            },
            Some((false, n)) => SurfaceState::Present {
                detail: copy_text(
                    "rsConfigSurface.observe.fileSize",
                    &[("bytes", &n.to_string())],
                ),
            },
        },
        PathResolution::LocalGlob {
            dir,
            prefix,
            suffix,
        } => match (fs.list)(dir) {
            None => SurfaceState::Undetermined {
                why: copy_text(
                    "rsConfigSurface.observe.globDirUnlistable",
                    &[("dir", &(dir.display()).to_string())],
                ),
            },
            Some(names) => {
                let n = names
                    .iter()
                    .filter(|s| {
                        s.len() >= prefix.len() + suffix.len()
                            && s.starts_with(prefix.as_str())
                            && s.ends_with(suffix.as_str())
                    })
                    .count();
                if n == 0 {
                    SurfaceState::Absent
                } else {
                    SurfaceState::Present {
                        detail: copy_text(
                            "rsConfigSurface.observe.globCount",
                            &[("count", &n.to_string())],
                        ),
                    }
                }
            }
        },
        // **本机没找到 ≠ 不存在**：这是 T04 的核心语义。绝不返回 `Absent`。
        //
        // 实现就一句话：**递归观测内层，只把 `Absent` 改写成 `Undetermined`**。
        // 这样 glob 计数、目录列举、"列不出来"那档全部自动继承，
        // 而"Either 绝不说 Absent"这个性质由这个 match 直接可证（第一版自己存 `PathBuf`，
        // glob 被拍成字面 `cc-*` 去 stat，计数分支永远走不到——审计阻塞 1）。
        PathResolution::EitherHost(inner) => match observe(inner, fs) {
            SurfaceState::Absent => SurfaceState::Undetermined {
                why: copy_text(
                    "rsConfigSurface.observe.eitherAbsent",
                    &[("target", &(describe_target(inner)).to_string())],
                ),
            },
            SurfaceState::Present { detail } => SurfaceState::Present {
                detail: copy_text(
                    "rsConfigSurface.observe.eitherPresent",
                    &[("detail", &detail.to_string())],
                ),
            },
            // 内层本来就"不确定"（比如目录列不出来）时，**追加**而不是替换那条理由——
            // 两件事都要说：本机为什么查不了 + 它也可能根本不在本机。
            // （第二个探针一加就红在这里：原先 `other => other` 把 Either 的提示整个吞了。）
            SurfaceState::Undetermined { why } => SurfaceState::Undetermined {
                why: copy_text(
                    "rsConfigSurface.observe.eitherUndetermined",
                    &[("why", &why.to_string())],
                ),
            },
        },
        PathResolution::NeedsUserConfig { what } => SurfaceState::Undetermined {
            why: copy_text(
                "rsConfigSurface.observe.needsConfig",
                &[("what", &what.to_string())],
            ),
        },
        PathResolution::Remote(p) => SurfaceState::Undetermined {
            why: copy_text(
                "rsConfigSurface.observe.remote",
                &[("path", &p.to_string())],
            ),
        },
        PathResolution::NeedsProjectDir(p) => SurfaceState::Undetermined {
            why: copy_text(
                "rsConfigSurface.observe.projectDir",
                &[("path", &p.to_string())],
            ),
        },
        // **「不适用」和「查不到」不是一回事**（T02 审计重要 7）。原文一律说
        // 「Windows 侧 $PROFILE，本机无从解析」，在 Linux 上这暗示"可能有东西、只是查不到"
        // ——实际是**这一项根本不适用**。而在 Windows 上仓里已经有能力查它
        // （`profile_installer::scan_path` 给出 path/exists/has_ccm_block/size），
        // 所以那边该指路而不是耸肩。
        PathResolution::WindowsProfile => SurfaceState::Undetermined {
            why: if cfg!(target_os = "windows") {
                // 从前这句指「终端集成」页 —— AL1c 起那一页就没有了，今天在「本机 → 工具 → 别名」。
                copy_text("rsConfigSurface.observe.profileWindows", &[]).into()
            } else {
                copy_text("rsConfigSurface.observe.profileNotWindows", &[])
            },
        },
    }
}

/// 「这东西从哪来」。用上 `ToolSpec::source`。
pub fn source_label(s: &ToolSource) -> String {
    match s {
        ToolSource::EmbeddedText { .. } => copy_text("rsConfigSurface.source.embeddedText", &[]),
        ToolSource::RepoDir { .. } => copy_text("rsConfigSurface.source.repoDir", &[]),
        ToolSource::EmbeddedBinary { .. } => {
            copy_text("rsConfigSurface.source.embeddedBinary", &[])
        }
        ToolSource::Generated => copy_text("rsConfigSurface.source.generated", &[]),
        // 装不了的那一档：措辞必须**先说清不是我们的**，
        // 否则用户会以为这一行也是 cc-monitor 放上去的。
        ToolSource::NotOurs { who } => {
            copy_text("rsConfigSurface.source.notOurs", &[("who", &who.get())])
        }
    }
}

/// 「在哪台机器上」。**必须上屏**——否则用户看 `$PROFILE` 与 `~/.local/bin/ccm`
/// 分不出说的是哪台机器，而这一页的全部价值是可信告知。
pub fn host_label(h: HostScope) -> String {
    // **短标签，挂在路径行上当徽章**（T04 审计重要 8）。原先是独立一行长句，
    // 而审计核实 10 行里 9 行是**冗余**的——「现状」那一列早就写着
    // 「远端路径（…）——本页不连 SSH」/「装在 Claude Code 跑的那台上」/「相对项目目录」。
    // 真正新增信息的只有 Windows 上的 `$PROFILE` 那行（它的 why 只说"路径由 PowerShell 决定"）。
    // 所以：信息保留，但收成一个词，省掉 10 行灰字。
    // 也顺便更正我 commit 里那句"用户看 $PROFILE 与 ~/.local/bin/ccm 分不出哪台"
    // ——只有一半成立，ccm 那行的 why 早就写着"远端"。
    match h {
        HostScope::Client => copy_text("rsConfigSurface.host.client", &[]),
        HostScope::Remote => copy_text("rsConfigSurface.host.remote", &[]),
        HostScope::Either => copy_text("rsConfigSurface.host.either", &[]),
        HostScope::ProjectDir => copy_text("rsConfigSurface.host.projectDir", &[]),
    }
}

/// 「我们对它做什么」。措辞直接决定用户的危险感知，所以定在后端、UI 不再各写一遍。
pub fn effect_label(e: TouchEffect) -> String {
    match e {
        TouchEffect::ReadOnly => copy_text("rsConfigSurface.effect.readOnly", &[]),
        TouchEffect::FencedBlock => copy_text("rsConfigSurface.effect.fencedBlock", &[]),
        TouchEffect::OwnedFile => copy_text("rsConfigSurface.effect.ownedFile", &[]),
        TouchEffect::GenerateOnly => copy_text("rsConfigSurface.effect.generateOnly", &[]),
        // 措辞必须把「谁动的手」说清：不是 cc-monitor 直接写，但**是你在 cc-monitor 里点的**
        TouchEffect::IndirectWrite => copy_text("rsConfigSurface.effect.indirectWrite", &[]),
    }
}

/// 表格里的一行。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SurfaceRow {
    pub tool_id: &'static str,
    pub tool_name: String,
    /// 🔴 **档进线上形状了。**
    ///
    /// 上一版 `config-surface-section.ts` 的 `describeUndo` 逐字写着
    /// 「⚠ 两类**今天在行上分不开**：`SurfaceRow` 的线上形状里没有档这一格」——
    /// 于是「不该由我们装」与「该我们装而还没写」在前端只能靠一句和稀泥的措辞盖过去。
    /// 今天档在行上，前端按**值**分档，不按措辞猜。
    pub tier: EnvTier,
    pub source_label: String,
    pub path_declared: &'static str,
    /// 解析出的本机路径（远端 / 项目相对 / `$PROFILE` 一律 `None`）。
    pub path_resolved: Option<String>,
    pub note: Option<String>,
    pub host_label: String,
    pub effect_label: String,
    pub state: SurfaceState,
    pub installable: bool,
    pub uninstallable: bool,
}

/// 🔴 **多收一个 `c`（载体），而那正是本件买到的东西。**
///
/// 先前这里拿的是 `t.destination` 与 `t.source` —— 一个工具一份。
/// 于是「同一个后端的三个落点」这件事**在这一行里根本表达不出来**：
/// 每条 touch 都被配上同一个 `destination`（1:N，不需要 key），
/// 而真相是 M 个落点 × N 条 touch。今天 touch 挂在载体下 ⇒ **配对是天然的**，
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
    let resolved = resolve_touched_path(vantage, f.path, &c.destination, f.host, home, agent_home);
    let (path_resolved, state) = match &resolved {
        Ok(r) => {
            let shown = match r {
                PathResolution::Local(p) => Some(p.to_string_lossy().into_owned()),
                PathResolution::LocalGlob {
                    dir,
                    prefix,
                    suffix,
                } => Some(format!("{}/{prefix}*{suffix}", dir.display())),
                PathResolution::EitherHost(_) => Some(describe_target(r)),
                // **兜底臂换成四条显式臂**（零行为变更）。
                // 原来是 `_ => None`：新增一种解析形态会**静默显示为空**，
                // 而这一页是「配置面审计」——「显示为空」与「这一项不存在」
                // 在界面上长得一模一样，用户读不出区别。
                // 写成具名臂之后，第 8 个变体会**编译失败**，逼人回答它该显示什么。
                PathResolution::Remote(_) => None,
                PathResolution::NeedsProjectDir(_) => None,
                PathResolution::WindowsProfile => None,
                PathResolution::NeedsUserConfig { .. } => None,
            };
            (shown, observe(r, fs))
        }
        // **声明自相矛盾也要如实显示**，不能静默跳过一行——那会让表格看着很干净而实际漏了东西
        Err(e) => (
            None,
            SurfaceState::Undetermined {
                why: copy_text("rsConfigSurface.row.declMismatch", &[("e", &e.to_string())]),
            },
        ),
    };
    SurfaceRow {
        tool_id: t.id,
        tool_name: t.display_name.get(),
        tier: e.tier,
        // 🔴 读的是**这一份载体**的来源，不是「这个工具的来源」——
        //    `ccm` 那一行先前只能填一个，而它的注释自己承认「本机那一半的来源不是这个」。
        source_label: source_label(&c.source),
        path_declared: f.path,
        path_resolved,
        note: f.note.map(|n| n.get()),
        host_label: host_label(f.host),
        effect_label: effect_label(f.effect),
        state,
        installable: t.installable,
        uninstallable: t.uninstallable,
    }
}

/// 建一次表要用的那一套基准与探针。**全部注入** —— 本模块不自己去摸环境，
/// 那是它从第一天起就可纯测的原因。
///
/// 〔`K-R65` 09-11 抽出来〕在此之前这四样是 `build_rows` 的四个位置参数，
/// 而本件要再加一个（`path_env`）⇒ 五个位置参数往下传两层，谁也读不出哪个是哪个。
/// 收成一个具名结构之后，加第六样不会再让每个调用点都改一遍。
pub struct SurfaceEnv<'a> {
    pub home: &'a Path,
    /// agent 家（`~/.claude/…` 的基准），调用方按 `agents/claudecode/paths.rs` 那一条规则算好交进来。
    pub agent_home: &'a Path,
    pub fs: &'a FsProbe<'a>,
    /// `$PATH` 原样。
    ///
    /// 🔴 **`None` = 取不到，不是「空的」** —— 那时 [`EnvProbe::OnPath`] 那一族一律
    /// 「查不动」（`Undetermined`），**绝不说成「不存在」**。
    /// 这条纪律不是新写的：[`resolves_on_path`] 的头注记着它在生产平台上
    /// 曾经「既没取到、又给了一个确定的否定答案」。
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
/// # 🔴 这个函数的正题变了：从「不查」变成「查」
///
/// 上一版这里逐字写着：
///
/// > `state` 一律 `Undetermined` **并说明为什么没查** …… 而「我们压根没去查」
/// > 比「查不动」还要更早一步
///
/// 以及 `path_resolved: None` 旁边那句「**不解析** —— 解析了就等于查了」。
/// `K38` 把那一档判掉了：通用工具是「**你自己装，而我会看、缺了我要说**」
/// ⇒ 这里**真去查**，而查出来的三种答案在这一页上是三回事：
///
/// | 探针答什么 | 这一行显示成 | 前端 `readiness.ts` 里的同一条分法 |
/// |---|---|---|
/// | 在 | `Present` | —— |
/// | **查了、确认没有** | `Absent` | `missing`（可以理直气壮说「缺」） |
/// | **查不动** | `Undetermined { why }` | `unknown`（说「缺」就是替用户下一个他没做过的结论） |
///
/// ⚠ 中间那一行是本件买到的东西：在此之前它和最后一行**长得一模一样**。
fn unmanaged_row(
    e: &EnvEntry,
    named: &'static str,
    host: HostScope,
    probe: EnvProbe,
    env: &SurfaceEnv,
) -> SurfaceRow {
    // **措辞按档分**，且**没有兜底臂**：`EnvTier` 加一档会编译失败，
    // 逼人回答它在这一页上该显示什么（同本模块 `PathResolution` 那条既定做法）。
    let (source_label, effect_label) = match e.tier {
        EnvTier::AppInstalls => (
            copy_text("rsConfigSurface.unmanaged.appInstallsSource", &[]),
            copy_text("rsConfigSurface.unmanaged.appInstallsEffect", &[]),
        ),
        // 🔴 `K38` 裁的那一档：**我们不装，但我们看；缺了我们说。**
        EnvTier::UserInstallsWePrompt => (
            copy_text("rsConfigSurface.unmanaged.promptSource", &[]),
            copy_text("rsConfigSurface.unmanaged.promptEffect", &[]),
        ),
        // 🔴 `KR65D2` 的那一格：**该我们装，而装口还欠着。** 措辞必须两半都说 ——
        // 只说「装不了」会被读成「不该我们装」，那正是 `K38` 反对的那句话。
        EnvTier::AppShipsNoInstallerYet => (
            copy_text("rsConfigSurface.unmanaged.owedSource", &[]),
            copy_text("rsConfigSurface.unmanaged.owedEffect", &[]),
        ),
        EnvTier::AppOnlyChecks => (
            copy_text("rsConfigSurface.unmanaged.checkOnlySource", &[]),
            copy_text("rsConfigSurface.unmanaged.checkOnlyEffect", &[]),
        ),
    };
    let (path_resolved, state) = observe_unmanaged(named, probe, env);
    SurfaceRow {
        tool_id: e.id,
        tool_name: e.display_name.clone(),
        tier: e.tier,
        source_label,
        path_declared: named,
        path_resolved,
        note: Some(e.why.clone()),
        host_label: host_label(host),
        effect_label,
        state,
        installable: false,
        uninstallable: false,
    }
}

/// 手写那一半**真去查**的那一步。抽出来是因为它是 `KR65D1` 的死值验落点：
/// 同一个名字换一种 [`EnvProbe`]，出来的必须是不同的一格。
///
/// 返回 `(解析出来的东西, 现状)`。
fn observe_unmanaged(
    named: &'static str,
    probe: EnvProbe,
    env: &SurfaceEnv,
) -> (Option<String>, SurfaceState) {
    match probe {
        // `PATH` 上的裸命令。**复用 [`resolves_on_path`]，不新写一个 `which`** ——
        // 它已经把「切分必须走 `split_paths`」与「取不到 PATH 就不猜」两条填好了。
        EnvProbe::OnPath | EnvProbe::InSessionShell => {
            let exists = |p: &str| (env.fs.meta)(Path::new(p)).is_some();
            let (path, blind) = match probe {
                EnvProbe::InSessionShell => (
                    env.session_path,
                    copy_text("rsConfigSurface.onPath.noSessionPath", &[("named", named)]),
                ),
                _ => (
                    env.path_env,
                    copy_text("rsConfigSurface.onPath.noPath", &[("named", named)]),
                ),
            };
            match resolves_on_path(named, path, &exists) {
                // 🔴 **查不动**：`PATH` 读不到。绝不说成「不存在」——
                // 那会对一台装得好好的机器报假警报（本模块头注那条硬纪律）。
                None => (None, SurfaceState::Undetermined { why: blind }),
                // **查了、确认没有** —— 这一格才是「缺」，前端据此劝人去装。
                Some(false) => (None, SurfaceState::Absent),
                Some(true) => (
                    Some(copy_text(
                        "rsConfigSurface.onPath.resolved",
                        &[("named", &named.to_string())],
                    )),
                    SurfaceState::Present {
                        detail: copy_text(
                            "rsConfigSurface.onPath.present",
                            &[("named", &named.to_string())],
                        ),
                    },
                ),
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
                crate::platform::ssh_client::SshClient::At(p) => {
                    let detail = copy_text("rsConfigSurface.ssh.found", &[("path", &p)]);
                    (Some(p), SurfaceState::Present { detail })
                }
                crate::platform::ssh_client::SshClient::Missing => (None, SurfaceState::Absent),
                crate::platform::ssh_client::SshClient::Unknown(why) => (
                    None,
                    SurfaceState::Undetermined {
                        why: copy_text("beTerminal.ssh.unknown", &[("why", &why)]),
                    },
                ),
            }
        }
        EnvProbe::CannotProbe { why } => (
            None,
            SurfaceState::Undetermined {
                why: copy_text("rsConfigSurface.probe.cannot", &[("why", &why.get())]),
            },
        ),
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
pub fn build_rows(env: &SurfaceEnv, client: Option<&SurfaceEnv>) -> Vec<SurfaceRow> {
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
                .map(|env| unmanaged_row(e, named, host, probe, env))
                .into_iter()
                .collect(),
        })
        .collect()
}

/// `settings.json` 的一个作用域。**这不是"我们碰的文件"**，而是「会影响钩子诊断结论」的文件
/// ——B04 登记项：那时只看 `<cfg>/settings.json` 一处，而钩子可以定义在别的作用域里，
/// 于是"没装"的结论可能是错的。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SettingsScope {
    pub scope: String,
    pub path: String,
    pub state: SurfaceState,
    /// 文件里有没有 cc-bus 那两个钩子程序的字样。读不到 → `None`（**不猜**）。
    pub has_cc_bus_hooks: Option<bool>,
    pub precedence_note: String,
}

/// cc-bus 钩子在 settings 里的两个程序名 —— 住址是 `observe/cc_bus_hooks.rs::PROGRAMS`（钩子诊断那一份），这里只用。
/// **这是全文粗匹配，不是解析**：`permissions.allow` 里一条 `Bash(cc-register)` 也会命中 ⇒ 本页只答「文件里有没有这个字样」，
/// 装没装是 `hooks-diag` 的事（两页对同一文件给出不同话是设计如此）。
pub(crate) const HOOK_PROGRAMS: [&str; 2] = crate::observe::cc_bus_hooks::PROGRAMS;

fn scope_row(
    scope: &str,
    path: PathBuf,
    precedence_note: &str,
    has_hooks: &dyn Fn(&Path) -> Option<bool>,
    fs: &FsProbe,
) -> SettingsScope {
    let has = has_hooks(&path);
    SettingsScope {
        scope: scope.to_string(),
        path: path.to_string_lossy().into_owned(),
        state: observe(&PathResolution::Local(path.clone()), fs),
        has_cc_bus_hooks: has,
        precedence_note: precedence_note.to_string(),
    }
}

/// 列出**用户级**的两个 settings 作用域，并把「项目级没查」如实写成一行。
///
/// 第三个参数是「有没有钩子字样」（[`HOOK_PROGRAMS`]；原文不出这台）。
/// 两个作用域的文件名是那一家的布局（注册表里的足迹面答，`agents/claudecode/footprint.rs::user_settings_files`）。
pub fn build_settings_scopes(
    agent_home: &Path,
    has_hooks: &dyn Fn(&Path) -> Option<bool>,
    fs: &FsProbe,
) -> Vec<SettingsScope> {
    // 没有哪一家登记了 settings 的住址 ⇒ 用户级那两格不出（不猜文件名），只剩「项目级没查」那一行。
    let mut out: Vec<SettingsScope> = crate::agents::footprint_faces()
        .next()
        .map(|f| {
            let [user, user_local] = (f.user_settings)(agent_home);
            vec![
                scope_row(
                    &copy_text("rsConfigSurface.scope.user", &[]),
                    user,
                    &copy_text("rsConfigSurface.scope.userNote", &[]),
                    has_hooks,
                    fs,
                ),
                scope_row(
                    &copy_text("rsConfigSurface.scope.userLocal", &[]),
                    user_local,
                    &copy_text("rsConfigSurface.scope.userLocalNote", &[]),
                    has_hooks,
                    fs,
                ),
            ]
        })
        .unwrap_or_default();
    out.push(SettingsScope {
        scope: copy_text("rsConfigSurface.scope.project", &[]),
        path: copy_text("rsConfigSurface.scope.projectPath", &[]).into(),
        // **明说没查**，不假装查过（B04 登记项）
        state: SurfaceState::Undetermined {
            why: copy_text("rsConfigSurface.scope.projectNotChecked", &[]).into(),
        },
        has_cc_bus_hooks: None,
        precedence_note: copy_text("rsConfigSurface.scope.projectNote", &[]),
    });
    out
}

/// 一次审计的完整回报。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ConfigSurfaceReport {
    pub rows: Vec<SurfaceRow>,
    pub settings_scopes: Vec<SettingsScope>,
    /// 解析基准，展示用（让用户知道 `~/.claude` 被解释成了哪里）。
    pub claude_config_dir: String,
    pub home: String,
    // `origin` 那一格不上线了：界面经通道问哪台自己知道，回声校验那一格由读者挂上。
}

// Tauri 命令 `config_surface_report`〔散文墓碑〕与 monitor 自己那台的探针（`with_monitor_probe`〔散文墓碑〕）删了：
//   成品由那台后端的 `footprint-report` 出（[`super::answer`]），monitor 那一族的事实由 monitor `footprint_client_facts` 答。

#[cfg(test)]
#[path = "../../../tests/backend/footprint/rows_tests.rs"]
mod tests;
