//! T02：**配置面审计视图**的后端——「cc-monitor 到底动过你哪些文件」。
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

use crate::tool_registry::{
    HostScope, ToolDestination, ToolSource, ToolSpec, TouchEffect, TouchedFile, TOOLS,
};
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
    NeedsUserConfig { what: &'static str },
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

/// 把申报路径解析成本机可查的形态。
///
/// **「本机还是远端」从 `dest` 推导，不新增字段**（`TouchedFile` 的文档写了理由）。
/// `~/.claude/...` 走 [`crate::hooks_diag::claude_config_dir`]——那条 `CLAUDE_CONFIG_DIR`
/// 规则只准解释一次。
pub fn resolve_touched_path(
    declared: &str,
    dest: &ToolDestination,
    host: HostScope,
    home: &Path,
    cfg_dir_env: Option<&Path>,
    is_dir: &dyn Fn(&Path) -> bool,
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
        return Err(format!(
            "申报路径含非 ASCII-graphic 字符 {bad:?}（{declared:?}）——\
             给人看的说明请放 `note` 字段，`path` 只放机器可解析的路径"
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
    let by_dest = resolve_by_destination(declared, dest, home, cfg_dir_env, is_dir)?;
    Ok(project_onto_host(by_dest, host, declared))
}

/// `host` 只改写**本机可解析**的那两种结果，其余原样透传。
///
/// 为什么不是"host 说远端就一律返回 Remote"：`UserConfiguredPath` 解析出的
/// `NeedsUserConfig { what }` 比 `Remote` **信息更多**（它告诉用户去哪儿看那个值），
/// 覆盖掉是降级。
fn project_onto_host(by_dest: PathResolution, host: HostScope, declared: &str) -> PathResolution {
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
    declared: &str,
    dest: &ToolDestination,
    home: &Path,
    cfg_dir_env: Option<&Path>,
    is_dir: &dyn Fn(&Path) -> bool,
) -> Result<PathResolution, String> {
    match dest {
        ToolDestination::UserShellProfile => {
            if declared == "$PROFILE" {
                Ok(PathResolution::WindowsProfile)
            } else {
                // 落点是"用户选的 profile"却申报了别的路径 → 声明自相矛盾，宁可报错
                Err(format!(
                    "落点是 UserShellProfile，申报路径却是 {declared:?}（期望 \"$PROFILE\"）"
                ))
            }
        }
        ToolDestination::ProjectRelative(_) => {
            if declared.starts_with('~') || declared.starts_with('/') {
                return Err(format!(
                    "落点是 ProjectRelative，申报路径却是绝对/家目录形态 {declared:?}"
                ));
            }
            Ok(PathResolution::NeedsProjectDir(declared.to_string()))
        }
        // **占位符只对应"落点"那一条，别的 touches 照常解析。**
        // 第一版这条臂要求**每条** touches 都等于占位符，于是 cc-acct-iso 的
        // `~/.claude-accts/`（账号库；**T04 查证：它在远端**，`accounts.rs` 全走 ssh exec，
        // 我这句原先写的"本机账号库"是错的）被判违规——
        // 落点只是这个工具碰的文件之一，不是全部。测试当场红在这里。
        ToolDestination::UserConfiguredPath { token, what } => {
            if declared == *token {
                Ok(PathResolution::NeedsUserConfig { what })
            } else {
                resolve_local_home(declared, home, cfg_dir_env, is_dir)
            }
        }
        ToolDestination::RemoteHomeRelative(_) => Ok(PathResolution::Remote(declared.to_string())),
        ToolDestination::LocalHomeRelative(_) => {
            resolve_local_home(declared, home, cfg_dir_env, is_dir)
        }
    }
}

/// 解析一个 `~/…` 形态的**本机**路径。抽出来是因为两条臂共用它
/// （`LocalHomeRelative`，以及 `UserConfiguredPath` 里那些**不是**落点占位符的 touches）。
fn resolve_local_home(
    declared: &str,
    home: &Path,
    cfg_dir_env: Option<&Path>,
    is_dir: &dyn Fn(&Path) -> bool,
) -> Result<PathResolution, String> {
    let rest = declared
        .strip_prefix("~/")
        .ok_or_else(|| format!("本机路径必须以 `~/` 开头，实得 {declared:?}"))?;
    // `~/.claude/…` 的真实基准目录是 `CLAUDE_CONFIG_DIR`（若它确实是个目录）
    let (base, rel) = match rest.strip_prefix(".claude/") {
        Some(r) => (
            crate::hooks_diag::claude_config_dir(cfg_dir_env, home, is_dir),
            r.to_string(),
        ),
        None => (home.to_path_buf(), rest.to_string()),
    };
    let rel = rel.trim_end_matches('/');
    if rel.is_empty() {
        return Err(format!("申报路径解析后为空：{declared:?}"));
    }
    // glob 只允许出现在**最后一段**，且只允许一个 `*`
    let (dir_part, last) = match rel.rsplit_once('/') {
        Some((d, l)) => (Some(d), l),
        None => (None, rel),
    };
    if dir_part.is_some_and(|d| d.contains('*')) {
        return Err(format!("glob 只允许在最后一段，实得 {declared:?}"));
    }
    if let Some((prefix, suffix)) = last.split_once('*') {
        if suffix.contains('*') {
            return Err(format!("只支持一个 `*`，实得 {declared:?}"));
        }
        let dir = match dir_part {
            Some(d) => base.join(d),
            None => base,
        };
        return Ok(PathResolution::LocalGlob {
            dir,
            prefix: prefix.to_string(),
            suffix: suffix.to_string(),
        });
    }
    Ok(PathResolution::Local(base.join(rel)))
}

/// 展示用：这条解析指向的东西（含 glob pattern）。
fn describe_target(r: &PathResolution) -> String {
    match r {
        PathResolution::Local(p) => p.to_string_lossy().into_owned(),
        PathResolution::LocalGlob {
            dir,
            prefix,
            suffix,
        } => format!("{}/{prefix}*{suffix}", dir.display()),
        PathResolution::EitherHost(inner) => describe_target(inner),
        PathResolution::Remote(p) | PathResolution::NeedsProjectDir(p) => p.clone(),
        PathResolution::NeedsUserConfig { what } => (*what).to_string(),
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
                    detail: format!("目录，{} 项", v.len()),
                },
                // 目录在但列不了（权限）——**不能说成"空目录"**
                None => SurfaceState::Undetermined {
                    why: "目录存在但列不出内容（权限？）".into(),
                },
            },
            Some((false, n)) => SurfaceState::Present {
                detail: format!("文件，{n} 字节"),
            },
        },
        PathResolution::LocalGlob {
            dir,
            prefix,
            suffix,
        } => match (fs.list)(dir) {
            None => SurfaceState::Undetermined {
                why: format!("列不出目录 {}（不存在或无权限）", dir.display()),
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
                        detail: format!("{n} 项匹配"),
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
                why: format!(
                    "本机没找到（{}）——但这套东西装在 Claude Code 跑的那台上，\
                     很可能是某个远端。远端状态请到对应页面按连接查（本页不连 SSH）。",
                    describe_target(inner)
                ),
            },
            SurfaceState::Present { detail } => SurfaceState::Present {
                detail: format!("本机存在（{detail}）"),
            },
            // 内层本来就"不确定"（比如目录列不出来）时，**追加**而不是替换那条理由——
            // 两件事都要说：本机为什么查不了 + 它也可能根本不在本机。
            // （第二个探针一加就红在这里：原先 `other => other` 把 Either 的提示整个吞了。）
            SurfaceState::Undetermined { why } => SurfaceState::Undetermined {
                why: format!(
                    "{why}；而且这套东西装在 Claude Code 跑的那台上，很可能是某个远端\
                     ——本页不连 SSH。"
                ),
            },
        },
        PathResolution::NeedsUserConfig { what } => SurfaceState::Undetermined {
            why: format!("路径由配置决定（{what}）——本页不猜它当前是什么值"),
        },
        PathResolution::Remote(p) => SurfaceState::Undetermined {
            why: format!("远端路径（{p}）——本页不连 SSH，请到部署向导里查"),
        },
        PathResolution::NeedsProjectDir(p) => SurfaceState::Undetermined {
            why: format!("相对项目目录（{p}）——要先选定项目才知道查哪里"),
        },
        // **「不适用」和「查不到」不是一回事**（T02 审计重要 7）。原文一律说
        // 「Windows 侧 $PROFILE，本机无从解析」，在 Linux 上这暗示"可能有东西、只是查不到"
        // ——实际是**这一项根本不适用**。而在 Windows 上仓里已经有能力查它
        // （`profile_installer::scan_path` 给出 path/exists/has_ccm_block/size），
        // 所以那边该指路而不是耸肩。
        PathResolution::WindowsProfile => SurfaceState::Undetermined {
            why: if cfg!(target_os = "windows") {
                "路径由 PowerShell 决定；准确状态见「终端集成」页（那里会读 $PROFILE 并查围栏块）"
                    .into()
            } else {
                "不适用：本机不是 Windows，没有 PowerShell $PROFILE 这个东西".to_string()
            },
        },
    }
}

/// 「这东西从哪来」。用上 `ToolSpec::source`。
pub fn source_label(s: &ToolSource) -> String {
    match s {
        ToolSource::EmbeddedText { repo_path } => format!("仓内文件（编译期内嵌）：{repo_path}"),
        ToolSource::RepoDir { repo_path } => format!("仓内目录：{repo_path}"),
        ToolSource::Vendored {
            repo_path,
            fingerprint_file,
        } => format!("vendored + 指纹：{repo_path}（{fingerprint_file}）"),
        ToolSource::EmbeddedBinary { repo_path } => format!("交叉编译内嵌的二进制：{repo_path}"),
        ToolSource::Generated => "由 cc-monitor 现场生成的文本".to_string(),
    }
}

/// 「在哪台机器上」。**必须上屏**——否则用户看 `$PROFILE` 与 `~/.local/bin/ccm`
/// 分不出说的是哪台机器，而这一页的全部价值是可信告知。
pub fn host_label(h: HostScope) -> &'static str {
    // **短标签，挂在路径行上当徽章**（T04 审计重要 8）。原先是独立一行长句，
    // 而审计核实 10 行里 9 行是**冗余**的——「现状」那一列早就写着
    // 「远端路径（…）——本页不连 SSH」/「装在 Claude Code 跑的那台上」/「相对项目目录」。
    // 真正新增信息的只有 Windows 上的 `$PROFILE` 那行（它的 why 只说"路径由 PowerShell 决定"）。
    // 所以：信息保留，但收成一个词，省掉 10 行灰字。
    // 也顺便更正我 commit 里那句"用户看 $PROFILE 与 ~/.local/bin/ccm 分不出哪台"
    // ——只有一半成立，ccm 那行的 why 早就写着"远端"。
    match h {
        HostScope::Client => "本机",
        HostScope::Remote => "远端",
        HostScope::Either => "本机或远端",
        HostScope::ProjectDir => "项目目录",
    }
}

/// 「我们对它做什么」。措辞直接决定用户的危险感知，所以定在后端、UI 不再各写一遍。
pub fn effect_label(e: TouchEffect) -> &'static str {
    match e {
        TouchEffect::ReadOnly => "只读（诊断用），我们不写",
        TouchEffect::FencedBlock => "插入/更新一个有围栏的块；卸载时按围栏精确剥离",
        TouchEffect::OwnedFile => "整个文件由 cc-monitor 拥有，部署时整体覆盖",
        TouchEffect::GenerateOnly => "只生成待贴文本，由你自己粘贴——我们不写这个文件",
        // 措辞必须把「谁动的手」说清：不是 cc-monitor 直接写，但**是你在 cc-monitor 里点的**
        TouchEffect::IndirectWrite => {
            "我们不直接写它；但你在 cc-monitor 里的操作会让它被写（由被调用的命令追加内容）"
        }
    }
}

/// 表格里的一行。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SurfaceRow {
    pub tool_id: &'static str,
    pub tool_name: &'static str,
    pub source_label: String,
    pub path_declared: &'static str,
    /// 解析出的本机路径（远端 / 项目相对 / `$PROFILE` 一律 `None`）。
    pub path_resolved: Option<String>,
    pub note: Option<&'static str>,
    pub host_label: &'static str,
    pub effect_label: &'static str,
    pub state: SurfaceState,
    pub installable: bool,
    pub uninstallable: bool,
}

fn row(
    t: &'static ToolSpec,
    f: &'static TouchedFile,
    home: &Path,
    cfg_dir_env: Option<&Path>,
    is_dir: &dyn Fn(&Path) -> bool,
    fs: &FsProbe,
) -> SurfaceRow {
    let resolved = resolve_touched_path(f.path, &t.destination, f.host, home, cfg_dir_env, is_dir);
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
                _ => None,
            };
            (shown, observe(r, fs))
        }
        // **声明自相矛盾也要如实显示**，不能静默跳过一行——那会让表格看着很干净而实际漏了东西
        Err(e) => (
            None,
            SurfaceState::Undetermined {
                why: format!("申报路径与落点不自洽：{e}"),
            },
        ),
    };
    SurfaceRow {
        tool_id: t.id,
        tool_name: t.display_name,
        source_label: source_label(&t.source),
        path_declared: f.path,
        path_resolved,
        note: f.note,
        host_label: host_label(f.host),
        effect_label: effect_label(f.effect),
        state,
        installable: t.installable,
        uninstallable: t.uninstallable,
    }
}

/// 遍历注册表建表。纯函数（`is_dir` / `fs` 注入）。
pub fn build_rows(
    home: &Path,
    cfg_dir_env: Option<&Path>,
    is_dir: &dyn Fn(&Path) -> bool,
    fs: &FsProbe,
) -> Vec<SurfaceRow> {
    TOOLS
        .iter()
        .flat_map(|t| {
            t.touches
                .iter()
                .map(move |f| row(t, f, home, cfg_dir_env, is_dir, fs))
        })
        .collect()
}

/// `settings.json` 的一个作用域。**这不是"我们碰的文件"**，而是「会影响钩子诊断结论」的文件
/// ——B04 登记项：那时只看 `<cfg>/settings.json` 一处，而钩子可以定义在别的作用域里，
/// 于是"没装"的结论可能是错的。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SettingsScope {
    pub scope: &'static str,
    pub path: String,
    pub state: SurfaceState,
    /// 文件里有没有 cc-bus 那两个钩子程序的字样。读不到 → `None`（**不猜**）。
    pub has_cc_bus_hooks: Option<bool>,
    pub precedence_note: &'static str,
}

/// cc-bus 钩子在 settings 里的两个程序名。**这是全文粗匹配，不是解析**——
/// `permissions.allow` 里一条 `Bash(cc-register)`、被改了事件名的钩子、
/// 甚至一句 `"description": "装 cc-register 用"` 都会命中。
///
/// 所以本模块**只回答「文件里有没有这个字样」**，绝不声称"装上了"；
/// 准确判定是 `hooks_diag::diagnose_event` 的事（它按 `hooks.<事件>.command` 走）。
/// 两页对同一文件给出不同话是**设计如此**：一页说"有字样"，一页说"装没装"。
/// 真机核实过当前两页不矛盾（`~/.claude/settings.json` 里 2 处命中都在
/// `hooks.*.command` 里），但假阳性面是真实的，措辞必须先把这一点讲明。
const HOOK_PROGRAMS: [&str; 2] = ["cc-register", "cc-bus-stop-hook"];

fn scope_row(
    scope: &'static str,
    path: PathBuf,
    precedence_note: &'static str,
    read: &dyn Fn(&Path) -> Option<String>,
    fs: &FsProbe,
) -> SettingsScope {
    let raw = read(&path);
    let has = raw
        .as_deref()
        .map(|s| HOOK_PROGRAMS.iter().any(|p| s.contains(p)));
    SettingsScope {
        scope,
        path: path.to_string_lossy().into_owned(),
        state: observe(&PathResolution::Local(path.clone()), fs),
        has_cc_bus_hooks: has,
        precedence_note,
    }
}

/// 列出**用户级**的两个 settings 作用域，并把「项目级没查」如实写成一行。
pub fn build_settings_scopes(
    home: &Path,
    cfg_dir_env: Option<&Path>,
    is_dir: &dyn Fn(&Path) -> bool,
    read: &dyn Fn(&Path) -> Option<String>,
    fs: &FsProbe,
) -> Vec<SettingsScope> {
    let cfg = crate::hooks_diag::claude_config_dir(cfg_dir_env, home, is_dir);
    vec![
        scope_row(
            "用户级",
            cfg.join("settings.json"),
            "钩子诊断读的就是这一份；本页只做字样粗匹配，装没装看「cc-bus 钩子」页",
            read,
            fs,
        ),
        scope_row(
            "用户级 local",
            cfg.join("settings.local.json"),
            "优先级高于上一行；这里定义的钩子同样生效",
            read,
            fs,
        ),
        SettingsScope {
            scope: "项目级",
            path: "<项目目录>/.claude/settings.json 与 settings.local.json".into(),
            // **明说没查**，不假装查过（B04 登记项）
            state: SurfaceState::Undetermined {
                why: "本页不猜项目目录，所以没查。若钩子定义在项目级，\
                      上面那两行「没装」的结论可能是错的"
                    .into(),
            },
            has_cc_bus_hooks: None,
            precedence_note: "优先级最高",
        },
    ]
}

/// 一次审计的完整回报。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ConfigSurfaceReport {
    pub rows: Vec<SurfaceRow>,
    pub settings_scopes: Vec<SettingsScope>,
    /// 解析基准，展示用（让用户知道 `~/.claude` 被解释成了哪里）。
    pub claude_config_dir: String,
    pub home: String,
}

/// 扫一次配置面。**只读、一次性**（不新增轮询）。
#[tauri::command]
pub async fn config_surface_report() -> Result<ConfigSurfaceReport, String> {
    tokio::task::spawn_blocking(|| {
        let home = dirs::home_dir().ok_or_else(|| "取不到 HOME".to_string())?;
        let cfg_env = std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from);
        let is_dir = |p: &Path| p.is_dir();
        let meta = |p: &Path| {
            std::fs::metadata(p)
                .ok()
                .map(|m| (m.is_dir(), if m.is_dir() { 0 } else { m.len() }))
        };
        let list = |p: &Path| {
            std::fs::read_dir(p).ok().map(|it| {
                it.filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
            })
        };
        let read = |p: &Path| std::fs::read_to_string(p).ok();
        let fs = FsProbe {
            meta: &meta,
            list: &list,
        };
        let cfg_dir = crate::hooks_diag::claude_config_dir(cfg_env.as_deref(), &home, &is_dir);
        Ok(ConfigSurfaceReport {
            rows: build_rows(&home, cfg_env.as_deref(), &is_dir, &fs),
            settings_scopes: build_settings_scopes(&home, cfg_env.as_deref(), &is_dir, &read, &fs),
            claude_config_dir: cfg_dir.to_string_lossy().into_owned(),
            home: home.to_string_lossy().into_owned(),
        })
    })
    .await
    .map_err(|e| format!("spawn_blocking: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structural_scan::ScanReport;

    fn home() -> PathBuf {
        PathBuf::from("/h")
    }
    fn no_dir(_: &Path) -> bool {
        false
    }
    fn yes_dir(_: &Path) -> bool {
        true
    }

    /// 一个什么都没有的文件系统。
    fn empty_probe<'a>() -> FsProbe<'a> {
        FsProbe {
            meta: &|_| None,
            list: &|_| None,
        }
    }

    // ===== 解析：五种结果各一条 =====

    #[test]
    fn resolves_local_home_paths() {
        let r = resolve_touched_path(
            "~/.local/bin/ccm",
            &ToolDestination::LocalHomeRelative("x"),
            HostScope::Client,
            &home(),
            None,
            &no_dir,
        )
        .unwrap();
        assert_eq!(r, PathResolution::Local(PathBuf::from("/h/.local/bin/ccm")));
    }

    /// `~/.claude/…` **必须**走 `CLAUDE_CONFIG_DIR` 那条规则，而且只解释一次。
    #[test]
    fn claude_paths_honor_config_dir_and_fall_back() {
        let acct = PathBuf::from("/h/.claude-accts/z");
        let with = resolve_touched_path(
            "~/.claude/settings.json",
            &ToolDestination::LocalHomeRelative("x"),
            HostScope::Client,
            &home(),
            Some(&acct),
            &yes_dir,
        )
        .unwrap();
        assert_eq!(
            with,
            PathResolution::Local(PathBuf::from("/h/.claude-accts/z/settings.json"))
        );
        // 环境变量指向的不是目录 → 回落 `~/.claude`（与 hooks_diag 同一条规则）
        let without = resolve_touched_path(
            "~/.claude/settings.json",
            &ToolDestination::LocalHomeRelative("x"),
            HostScope::Client,
            &home(),
            Some(&acct),
            &no_dir,
        )
        .unwrap();
        assert_eq!(
            without,
            PathResolution::Local(PathBuf::from("/h/.claude/settings.json"))
        );
    }

    #[test]
    fn resolves_one_level_glob() {
        let r = resolve_touched_path(
            "~/.local/bin/cc-*",
            &ToolDestination::LocalHomeRelative("x"),
            HostScope::Client,
            &home(),
            None,
            &no_dir,
        )
        .unwrap();
        assert_eq!(
            r,
            PathResolution::LocalGlob {
                dir: PathBuf::from("/h/.local/bin"),
                prefix: "cc-".into(),
                suffix: String::new(),
            }
        );
    }

    #[test]
    fn remote_project_and_profile_are_not_local() {
        assert_eq!(
            resolve_touched_path(
                "~/.local/bin/ccm-daemon",
                &ToolDestination::RemoteHomeRelative("x"),
                HostScope::Remote,
                &home(),
                None,
                &no_dir
            )
            .unwrap(),
            PathResolution::Remote("~/.local/bin/ccm-daemon".into())
        );
        assert_eq!(
            resolve_touched_path(
                ".mcp.json",
                &ToolDestination::ProjectRelative("x"),
                HostScope::ProjectDir,
                &home(),
                None,
                &no_dir
            )
            .unwrap(),
            PathResolution::NeedsProjectDir(".mcp.json".into())
        );
        assert_eq!(
            resolve_touched_path(
                "$PROFILE",
                &ToolDestination::UserShellProfile,
                HostScope::Client,
                &home(),
                None,
                &no_dir
            )
            .unwrap(),
            PathResolution::WindowsProfile
        );
    }

    /// 申报路径与落点**不自洽**时要报错，而不是猜一个。
    #[test]
    fn declaration_inconsistency_is_an_error_not_a_guess() {
        for (declared, dest) in [
            (
                "/etc/passwd",
                ToolDestination::LocalHomeRelative("x"), // 不以 ~/ 开头
            ),
            ("~/somewhere", ToolDestination::ProjectRelative("x")),
            ("~/.bashrc", ToolDestination::UserShellProfile),
            (
                "~/.local/*/bin",
                ToolDestination::LocalHomeRelative("x"), // glob 不在最后一段
            ),
            (
                "~/.local/bin/*-*",
                ToolDestination::LocalHomeRelative("x"), // 两个 *
            ),
        ] {
            assert!(
                resolve_touched_path(declared, &dest, HostScope::Client, &home(), None, &no_dir)
                    .is_err(),
                "{declared:?} 配 {dest:?} 应判不自洽"
            );
        }
    }

    // ===== 观测：不确定必须带理由，且绝不冒充"缺失" =====

    #[test]
    fn undetermined_always_carries_a_reason() {
        for res in [
            PathResolution::Remote("~/x".into()),
            PathResolution::NeedsProjectDir(".mcp.json".into()),
            PathResolution::WindowsProfile,
        ] {
            match observe(&res, &empty_probe()) {
                SurfaceState::Undetermined { why } => {
                    assert!(!why.trim().is_empty(), "{res:?} 的理由不能是空的");
                }
                other => panic!("{res:?} 不该被判成 {other:?}——那是对能用的安装报假警报"),
            }
        }
    }

    #[test]
    fn present_absent_and_dir_listing() {
        let f = FsProbe {
            meta: &|p| match p.to_string_lossy().as_ref() {
                "/h/f" => Some((false, 42)),
                "/h/d" => Some((true, 0)),
                _ => None,
            },
            list: &|p| {
                if p == Path::new("/h/d") {
                    Some(vec!["a".into(), "b".into()])
                } else {
                    None
                }
            },
        };
        assert_eq!(
            observe(&PathResolution::Local("/h/f".into()), &f),
            SurfaceState::Present {
                detail: "文件，42 字节".into()
            }
        );
        assert_eq!(
            observe(&PathResolution::Local("/h/d".into()), &f),
            SurfaceState::Present {
                detail: "目录，2 项".into()
            }
        );
        assert_eq!(
            observe(&PathResolution::Local("/h/nope".into()), &f),
            SurfaceState::Absent
        );
    }

    /// 目录在但**列不出来**（权限）≠ 空目录。混掉的话用户会以为东西被删了。
    #[test]
    fn unlistable_dir_is_undetermined_not_empty() {
        let f = FsProbe {
            meta: &|_| Some((true, 0)),
            list: &|_| None,
        };
        match observe(&PathResolution::Local("/h/d".into()), &f) {
            SurfaceState::Undetermined { why } => assert!(why.contains("列不出")),
            other => panic!("实得 {other:?}"),
        }
    }

    #[test]
    fn glob_counts_only_real_matches() {
        let f = FsProbe {
            meta: &|_| None,
            list: &|_| {
                Some(vec![
                    "cc-send".into(),
                    "cc-recv".into(),
                    "ccm".into(), // 不匹配 cc-*（缺连字符）
                    "other".into(),
                ])
            },
        };
        let g = PathResolution::LocalGlob {
            dir: "/h/.local/bin".into(),
            prefix: "cc-".into(),
            suffix: String::new(),
        };
        assert_eq!(
            observe(&g, &f),
            SurfaceState::Present {
                detail: "2 项匹配".into()
            }
        );
        // 一个都不匹配 → 是真的没有，可以说 Absent
        let none = FsProbe {
            meta: &|_| None,
            list: &|_| Some(vec!["x".into()]),
        };
        assert_eq!(observe(&g, &none), SurfaceState::Absent);
        // 目录列不出来 → **不能说 Absent**
        assert!(matches!(
            observe(&g, &empty_probe()),
            SurfaceState::Undetermined { .. }
        ));
    }

    // ===== 结构性守卫：注册表里**每一条**申报路径都必须可解析 =====

    /// 要件 1+2+3：枚举 `TOOLS` 里每一条 `touches[].path`，逐条要求它能被解析成五种之一，
    /// 落进 `Err` 就是违规；`require` 拿计数自检。
    ///
    /// 这条守的是一个具体的退化：有人图省事把散文写回 `path`
    /// （第一版 6 个工具里就有 4 条是散文），于是审计页那一行永远显示"申报路径与落点不自洽"。
    fn every_declared_path_resolves() -> ScanReport {
        let mut r = ScanReport {
            checked: 0,
            violations: Vec::new(),
        };
        for t in TOOLS {
            for f in t.touches {
                r.checked += 1;
                if let Err(e) =
                    resolve_touched_path(f.path, &t.destination, f.host, &home(), None, &no_dir)
                {
                    r.violations.push(format!("{}/{:?}：{e}", t.id, f.path));
                }
            }
        }
        r
    }

    #[test]
    fn all_registry_paths_are_machine_resolvable() {
        every_declared_path_resolves()
            .require(10, "TOOLS 的申报路径")
            .unwrap();
    }

    /// **反向自检**：守卫真的会抓散文。把 T01 第一版那四条散文路径各喂一次，必须全红。
    /// （不是构造的边角——它们是这个文件昨天的真实内容。）
    #[test]
    fn prose_paths_are_rejected() {
        for declared in [
            "~/.bashrc（或所选 profile）",
            "~/.claude/settings.json 的 hooks 段",
            "~/.local/bin/cc-*（12 条软链）",
            "<项目目录>/.mcp.json",
        ] {
            let dest = if declared.starts_with('<') {
                ToolDestination::ProjectRelative("x")
            } else {
                ToolDestination::LocalHomeRelative("x")
            };
            let r =
                resolve_touched_path(declared, &dest, HostScope::Client, &home(), None, &no_dir);
            // **必须直接 Err。** 第一版这里写的是"Err 或者解析出带括号的假路径都算抓到"，
            // 于是 `~/.local/bin/cc-*（12 条软链）` 溜了过去——它成功解析成
            // `LocalGlob { prefix: "cc-", suffix: "（12 条软链）" }`，`dir` 干干净净，
            // 我的谓词只看了 `dir`。测试当场红，才补上 `resolve_touched_path` 开头那道
            // ASCII-graphic 白名单。**断言要打在"解析必须失败"上，不是打在结果长相上。**
            assert!(
                r.is_err(),
                "散文路径 {declared:?} 必须被判不自洽，实得 {r:?}"
            );
        }
        // 而现在真文件里一条散文都没有
        assert!(every_declared_path_resolves().violations.is_empty());
    }

    // 原先这里有一条 `locality_is_derivable_from_destination_today`，**删了**（T02 审计重要 1）。
    // 审计实测它是**同义反复**：`PathResolution::Remote` 只由 `RemoteHomeRelative` 臂产生、
    // 且必然产生，所以 `matches!(res, Remote(_)) == remote` 对任意输入恒真——把 `ccm` 的
    // `destination` 翻成 `LocalHomeRelative`（会让两行从"远端未确定"变成去 stat 本机 `~/.bashrc`）
    // **492 项照样全绿**。而它自称守的那件事（"本机落点却申报远端文件"）在类型上根本
    // 表达不出来（`TouchedFile` 没有 host 字段），永远不会红。
    //
    // **如实登记：远端性没有门禁。** 它现在只是 `resolve_touched_path` 的一条实现约定 +
    // 文档。真要门禁得给 `TouchedFile` 加 `host`，而那件事有个真实的第二消费者在等着：
    // `~/.cc-bus/` 被本页解析成**本机**，可 `cc_bus.rs` 是按 `origin` 在**可能是远端**的
    // 主机上读它——一个 `const destination` 表达不了"按运行期 origin 跨主机"。
    // 这条留给 T04（五套机制收编）时连着 origin 模型一起做，不在 T02 硬塞。
    // 替代的有牙测试放在 `tool_registry.rs`：`installable_tools_declare_where_they_land`
    // 与 `owned_file_implies_installable`（两条都是跨字段一致性，改任一边就红）。

    // ===== T04：`host` 维度 =====

    /// **这一条是 T04 存在的理由。** `cc-bus` 三条 touches 原先被当纯本机路径，
    /// 于是在**生产平台 Windows** 上审计页会说"不存在"，而同一个 app 的驾驶舱
    /// 正从远端把 inbox 读得好好的——T02 专门要防的假警报，出现在那一页上格外讽刺。
    #[test]
    fn either_host_never_says_absent() {
        // 审计指出：原先只用 `empty_probe`（meta/list 恒 None），而 bug 的输出恰好就是
        // 它想要的 `Undetermined` —— 断言与 bug 撞了同一个答案。现在两种探针都跑：
        // ① 全空（本机什么都没有）② 目录列得出但一条都不匹配。两种都不许说 Absent。
        for f in [
            empty_probe(),
            FsProbe {
                meta: &|_| None,
                list: &|_| Some(vec!["unrelated".to_string()]),
            },
        ] {
            either_never_absent_with(&f);
        }
    }

    fn either_never_absent_with(probe: &FsProbe) {
        let nothing = probe;
        for f in TOOLS
            .iter()
            .flat_map(|t| t.touches.iter().map(move |f| (t, f)))
            .filter(|(_, f)| f.host == HostScope::Either)
            .map(|(t, f)| {
                resolve_touched_path(f.path, &t.destination, f.host, &home(), None, &no_dir)
                    .unwrap()
            })
        {
            assert!(
                matches!(f, PathResolution::EitherHost { .. }),
                "Either 的路径必须解析成 EitherHost，实得 {f:?}"
            );
            match observe(&f, &nothing) {
                SurfaceState::Undetermined { why } => {
                    assert!(
                        why.contains("Claude Code 跑的那台"),
                        "理由要说清为什么：{why}"
                    );
                    assert!(why.contains("不连 SSH"), "要指路：{why}");
                }
                other => panic!("本机没找到不等于不存在，不许判 {other:?}"),
            }
        }
    }

    /// 但本机**真找到了**就该确定地说存在——`Either` 不是"永远说不知道"。
    #[test]
    fn either_host_reports_present_when_found_locally() {
        let f = FsProbe {
            meta: &|_| Some((false, 7)),
            list: &|_| None,
        };
        let r = PathResolution::EitherHost(Box::new(PathResolution::Local("/h/.cc-bus".into())));
        match observe(&r, &f) {
            SurfaceState::Present { detail } => {
                assert!(detail.contains("本机存在"), "实得 {detail}");
                assert!(detail.contains("7 字节"), "内层细节要保住：{detail}");
            }
            other => panic!("实得 {other:?}"),
        }
    }

    /// **`Either` + glob 必须保住计数**（T04 审计阻塞 1）。
    ///
    /// 第一版 `EitherHost { local: PathBuf }` 把 glob 拍成 `dir.join("cc-*")`，
    /// 于是 `observe` 去 stat 一个**字面含 `*` 的文件名**、计数分支永远走不到，
    /// 那一行从 T04 之前正确的「12 项匹配」退化成「未确定 —— 本机 …/cc-* 不存在」
    /// ——`why` 里陈述了一个**假事实**（实测 `ls -d ~/.local/bin/'cc-*'` 报不存在，
    /// 而那个目录下真有 12 条 `cc-*`）。
    #[test]
    fn either_host_keeps_the_glob_count() {
        // 真实盘面：`~/.local/bin` 下 12 条 cc-*（其中一条属于 cc-acct-iso）
        let names: Vec<String> = (0..11)
            .map(|i| format!("cc-{i}"))
            .chain(["cc-acct-iso".to_string()])
            .collect();
        let f = FsProbe {
            meta: &|_| None, // 目录本身 stat 不到也不影响 glob 走 list
            list: &|_| Some(names.clone()),
        };
        let ccbus = TOOLS.iter().find(|t| t.id == "cc-bus").unwrap();
        let glob = ccbus
            .touches
            .iter()
            .find(|f| f.path.contains('*'))
            .expect("cc-bus 应有一条 glob touch");
        assert_eq!(glob.host, HostScope::Either, "前提：这条是 Either");
        let r = resolve_touched_path(
            glob.path,
            &ccbus.destination,
            glob.host,
            &home(),
            None,
            &no_dir,
        )
        .unwrap();
        match observe(&r, &f) {
            SurfaceState::Present { detail } => {
                assert!(detail.contains("12 项匹配"), "计数丢了：{detail}");
                assert!(detail.contains("本机存在"), "要标明是本机：{detail}");
            }
            other => panic!("有 12 条匹配却报 {other:?}——这正是阻塞 1 的形态"),
        }
    }

    /// **跨字段一致性**：`host == Remote` ⇔ 解析结果不含任何本机路径。
    ///
    /// 这条**不是**同义反复（T02 那条被删的"钉子"是）：`host` 与 `destination` 是
    /// **两个独立字段**，解析要先按 destination 全量校验、再用 host 投影。
    /// 改任一边都会红——变异验证见 §5。
    #[test]
    fn remote_host_never_resolves_to_a_local_path() {
        let mut checked = 0;
        for t in TOOLS {
            for f in t.touches {
                let r =
                    resolve_touched_path(f.path, &t.destination, f.host, &home(), None, &no_dir)
                        .unwrap();
                let local = matches!(
                    r,
                    PathResolution::Local(_)
                        | PathResolution::LocalGlob { .. }
                        | PathResolution::EitherHost { .. }
                );
                if f.host == HostScope::Remote {
                    checked += 1;
                    assert!(
                        !local,
                        "{}/{:?} 声明在远端，却解析出本机路径 {r:?}——本机不许替远端回答\
                         「这个路径在不在」（T03 阻塞 3 的根因）",
                        t.id, f.path
                    );
                }
            }
        }
        // 计数自检：一条 Remote 都没扫到 = 守卫空转
        // **等号而不是 `>=`**（T04 审计重要 5）：真实是 5 条，写 `>= 4` 恰好容忍一次
        // 静默降级——审计实测单独改一条 host 就是全绿。改 TOOLS 时要来改这个数。
        assert_eq!(
            checked, 5,
            "Remote 条目数变了（真实应为 5）——改 TOOLS 就要来确认这个数"
        );
    }

    /// **逐条钉死 `(tool_id, path) → host`**（T04 审计阻塞 3）。
    ///
    /// 审计实测：把 `~/.claude-accts/` 的 host 改回**我上一版刚更正的那个错值**
    /// （`Remote → Client`），`cargo test` **512 项全绿**。也就是说 commit message 里
    /// 「跨字段守卫…改任一边都红」**是假的**——那条守卫只守 `destination` 那一边
    /// （变异 B 改的是 `project_onto_host` 的代码，证明的是代码承重，不是两个字段相互约束）。
    ///
    /// T04 的中心论据是「host 把我一直在犯的错变成了必须逐条声明的东西」。声明是有了，
    /// **门禁没有**——我上一次犯的那个错今天改回去仍然全绿。这张表就是那道门禁：
    /// 改任何一条 host 都会红，且报错直接指出改了哪一条。
    ///
    /// 改 `TOOLS` 时**必须来改这张表**——这是有意的摩擦：host 判错过三次
    /// （T02 的 `~/.cc-bus/`、T03 的 basename 猜远端、T04 的 `~/.claude-accts/` 连错两版），
    /// 让它必须被显式确认一次。
    #[test]
    fn every_host_declaration_is_pinned() {
        use HostScope::*;
        let want: &[(&str, &str, HostScope)] = &[
            ("ccm", "~/.local/bin/ccm", Remote),
            ("ccm", "~/.bashrc", Remote),
            // 钩子诊断真有本机+远端两条路径（`diagnose_local_/remote_cc_bus_hooks`）
            ("cc-bus", "~/.claude/settings.json", Either),
            ("cc-bus", "~/.local/bin/cc-*", Either),
            // 但 `cc_bus.rs` 的 5 个 IPC 全走 origin+ssh，**零本机读取路径** → Remote
            ("cc-bus", "~/.cc-bus/", Remote),
            ("cc-acct-iso", "$ACCT_ISO_DEST", Remote),
            // 列举走远端 ssh，但本机 CLAUDE_CONFIG_DIR 会指进来 → 两端皆可
            ("cc-acct-iso", "~/.claude-accts/", Either),
            ("remote-daemon", "$DAEMON_PATH", Remote),
            ("project-mcp", ".mcp.json", ProjectDir),
            ("powershell-profile", "$PROFILE", Client),
        ];
        let mut actual: Vec<(&str, &str, HostScope)> = TOOLS
            .iter()
            .flat_map(|t| t.touches.iter().map(move |f| (t.id, f.path, f.host)))
            .collect();
        let mut expect = want.to_vec();
        actual.sort_by_key(|(a, b, _)| (*a, *b));
        expect.sort_by_key(|(a, b, _)| (*a, *b));
        assert_eq!(
            actual, expect,
            "host 声明与钉死的表不一致——改 TOOLS 就要来改这张表，并说清为什么"
        );
    }

    /// **`host` 必须携带 `destination` 之外的信息**（T04 审计重要 1 的机器化）。
    ///
    /// 审计核实：T04 第一版的 `destination → host` 是一张 **1:1 表**
    /// （`RemoteHomeRelative→Remote`、`LocalHomeRelative→Either`、
    ///  `UserConfiguredPath→Remote`、`ProjectRelative→ProjectDir`、`UserShellProfile→Client`），
    /// 于是那条"跨字段"断言在真实 `TOOLS` 上**可以完全由 destination 推出**
    /// ——与 T02 被删的那颗钉子同一类。
    ///
    /// 把两条标错的改对之后它才真正独立：`LocalHomeRelative` 同时映到
    /// `Either`（settings.json / cc-*）与 `Remote`（~/.cc-bus/），
    /// `UserConfiguredPath` 同时映到 `Remote`（两个占位符）与 `Either`（~/.claude-accts/）。
    /// **这条测试就是"host 不是 destination 的函数"这句话的门禁**——
    /// 哪天它变回 1:1，说明 host 退化成了冗余标签，那时该删掉这个字段而不是留着装样子。
    #[test]
    fn host_is_not_a_function_of_destination() {
        use std::collections::HashMap;
        let mut by_dest: HashMap<String, std::collections::HashSet<HostScope>> = HashMap::new();
        for t in TOOLS {
            let key = match &t.destination {
                ToolDestination::RemoteHomeRelative(_) => "RemoteHomeRelative",
                ToolDestination::LocalHomeRelative(_) => "LocalHomeRelative",
                ToolDestination::UserShellProfile => "UserShellProfile",
                ToolDestination::ProjectRelative(_) => "ProjectRelative",
                ToolDestination::UserConfiguredPath { .. } => "UserConfiguredPath",
            };
            for f in t.touches {
                by_dest.entry(key.to_string()).or_default().insert(f.host);
            }
        }
        let multi: Vec<_> = by_dest
            .iter()
            .filter(|(_, hosts)| hosts.len() >= 2)
            .map(|(d, hosts)| (d.clone(), hosts.len()))
            .collect();
        assert!(
            multi.len() >= 2,
            "至少要有两个 destination 变体各映到 ≥2 个 host，否则 host 就是 destination 的函数、\
             那条跨字段守卫等于同义反复（T02 删掉的那颗钉子就是这个病）。实得 {multi:?}，\
             全表 {by_dest:?}"
        );
    }

    /// **Phase G：保留 `Client`/`ProjectDir` 的理由是"合并会说假话"，这条把它钉住。**
    ///
    /// 它们各只有 1 个使用者、零行为影响（都落 `project_onto_host` 的 `(_, other)`）
    /// ——按 ≥2 尺子本该砍掉。保留的唯一理由是 `host_label` 是**用户可见事实**：
    /// `$PROFILE` 确定在客户端、`.mcp.json` 确定在项目目录，合进 `Either` 后标签变成
    /// 「本机或远端」，对这两条都是**假的**。
    #[test]
    fn host_labels_are_distinct_and_truthful() {
        use HostScope::*;
        let all = [Client, Remote, Either, ProjectDir];
        let labels: Vec<&str> = all.iter().map(|h| host_label(*h)).collect();
        // 四个标签互不相同——相同就说明该合并了
        let uniq: std::collections::HashSet<_> = labels.iter().collect();
        assert_eq!(uniq.len(), 4, "四个 host 标签必须互不相同，实得 {labels:?}");
        // **确定在客户端 / 确定在项目目录的，标签里不许出现"远端"**
        assert!(
            !host_label(Client).contains("远端"),
            "$PROFILE 确定在客户端，标签不许含「远端」：{}",
            host_label(Client)
        );
        assert!(
            !host_label(ProjectDir).contains("远端"),
            ".mcp.json 确定在项目目录，标签不许含「远端」：{}",
            host_label(ProjectDir)
        );
        // 而 Either 必须**明说**两端皆可（那是它存在的理由）
        assert!(host_label(Either).contains("本机") && host_label(Either).contains("远端"));
    }

    /// `host` 的四个变体都得有真实使用者。
    ///
    /// **如实登记一处尺子不一致**（T04 审计重要 4）：这条用 **≥1**，而同文件
    /// `user_configured_destinations_declare_a_placeholder_not_a_guess` 用 **≥2**。
    /// 变体真实用户数：`Client` 1 条/1 工具、`ProjectDir` 1/1、`Either` 4 条/2 工具、`Remote` 4/3。
    ///
    /// 为什么**不**统一到 ≥2：同文件的 `ToolSource` 5 个变体里 4 个是单用户（T01 保留了它），
    /// `TouchEffect` 的门禁也只要求 `>= 3` 种被用到。**描述数据的 enum 允许变体各自单用户**
    /// ——这一点 T01 论证过（变体差异是数据的本性，不是过度设计）。
    /// 而 `UserConfiguredPath` 那条 ≥2 守的是**别的东西**：那是"这个变体值不值得存在"的判据，
    /// 因为它是我为了不写死一个假常量而**新造**的。两把尺子各有其位，但**同一文件里并存
    /// 就该写明**，不能让人以为是疏忽。
    #[test]
    fn all_host_scopes_are_really_used() {
        let used: std::collections::HashSet<_> = TOOLS
            .iter()
            .flat_map(|t| t.touches.iter().map(|f| f.host))
            .collect();
        for want in [
            HostScope::Client,
            HostScope::Remote,
            HostScope::Either,
            HostScope::ProjectDir,
        ] {
            assert!(
                used.contains(&want),
                "{want:?} 没有任何使用者，那它不该存在"
            );
        }
    }

    /// **`host` 投影不许吞掉 `NeedsUserConfig`**：那个结果比 `Remote` 信息更多
    /// （它告诉用户去哪儿看那个值），覆盖掉是降级。
    #[test]
    fn host_projection_preserves_the_richer_resolution() {
        let daemon = TOOLS.iter().find(|t| t.id == "remote-daemon").unwrap();
        let f = &daemon.touches[0];
        assert_eq!(f.host, HostScope::Remote);
        let r = resolve_touched_path(f.path, &daemon.destination, f.host, &home(), None, &no_dir)
            .unwrap();
        match r {
            PathResolution::NeedsUserConfig { what } => {
                assert!(what.contains("daemon"), "实得 {what}");
            }
            other => panic!("远端投影把 NeedsUserConfig 吞成了 {other:?}"),
        }
    }

    /// **destination 的校验不许被 host 短路。**
    ///
    /// 第一版 host 优先短路，于是 `LocalHomeRelative` 的"必须 `~/` 开头"与
    /// "glob 只许在最后一段 / 只许一个 `*`"对所有 `Remote`/`Either` 的 touches
    /// **完全不再执行**——而 10 条 touches 里 7 条是这两种 host。
    #[test]
    fn destination_checks_still_run_under_every_host() {
        for host in [
            HostScope::Client,
            HostScope::Remote,
            HostScope::Either,
            HostScope::ProjectDir,
        ] {
            for bad in ["/etc/passwd", "~/.local/*/bin", "~/.local/bin/*-*"] {
                let e = resolve_touched_path(
                    bad,
                    &ToolDestination::LocalHomeRelative("x"),
                    host,
                    &home(),
                    None,
                    &no_dir,
                );
                assert!(
                    e.is_err(),
                    "host={host:?} 时 {bad:?} 仍该判不自洽，实得 {e:?}"
                );
            }
        }
    }

    // ===== 注册表 ↔ 真写入方对齐（T02 审计重要 6：此前零耦合） =====

    /// **申报的落点必须与真正执行写入的那段代码逐字一致。**
    ///
    /// 审计说得对：此前没有一条测试把 `TOOLS` 的申报与 `sftp.rs` / `mcp.rs` /
    /// `acct_iso_deploy` 对齐，所以这张告知页可以自信地说错而门禁不会红。
    /// 追查下去比审计报的更严重——**六条声明里三条没有任何代码支撑**：
    /// `remote-daemon` 的 `.local/bin/ccm-daemon` 全仓只出现在注册表自己里
    /// （真实是 `RemoteConfig.daemon_path`）、`cc-acct-iso` 声明成本机而实际是远端 +
    /// 前端传的 `dest_dir`、`cc-bus` 的落点是未实现的愿景。
    /// 前两条已改成 `ToolDestination::UserConfiguredPath`（承认"这是配置项"），
    /// 剩下**真有常量**的两条在这里用 `pin_definition` 钉死。
    #[test]
    fn declared_destinations_are_pinned_to_the_real_writers() {
        use crate::structural_scan::pin_definition;

        // ① ccm：`sftp.rs` 里那个常量就是真落点
        let sftp = include_str!("sftp.rs");
        pin_definition(
            sftp,
            r#"const CCM_CLI_REMOTE_PATH: &str = ".local/bin/ccm";"#,
            "const CCM_CLI_REMOTE_PATH",
            "ccm 远端落点",
        )
        .unwrap();
        let ccm = TOOLS.iter().find(|t| t.id == "ccm").unwrap();
        assert_eq!(
            ccm.destination,
            ToolDestination::RemoteHomeRelative(".local/bin/ccm"),
            "注册表声明的 ccm 落点与 sftp.rs 的 CCM_CLI_REMOTE_PATH 不一致"
        );

        // ② 项目 MCP：`mcp.rs` 真正 join 的就是这个文件名
        let mcp = include_str!("mcp.rs");
        let joins = mcp.matches(r#"join(".mcp.json")"#).count();
        assert!(
            joins >= 1,
            "mcp.rs 里找不到 join(\".mcp.json\")——落点变了还是扫描器失效了？"
        );
        let pm = TOOLS.iter().find(|t| t.id == "project-mcp").unwrap();
        assert_eq!(
            pm.destination,
            ToolDestination::ProjectRelative(".mcp.json")
        );

        // ③ 反向自检：确认上面读到的是真源码，不是空串
        assert!(
            sftp.len() > 10_000 && mcp.len() > 5_000,
            "include_str! 读空了"
        );
    }

    /// **配置项型落点不许再冒充常量**：`UserConfiguredPath` 的申报路径必须是占位符
    /// （`$` 开头），否则就是又一次"凭印象写个常量"。
    #[test]
    fn user_configured_destinations_declare_a_placeholder_not_a_guess() {
        let mut n = 0;
        for t in TOOLS {
            if let ToolDestination::UserConfiguredPath { token, what } = &t.destination {
                n += 1;
                assert!(token.starts_with('$'), "{}: {token:?} 不像占位符", t.id);
                assert!(
                    !what.trim().is_empty(),
                    "{}: 得告诉用户去哪儿看这个值",
                    t.id
                );
                // 这个占位符必须真出现在 touches 里，否则表格上那一行会显示别的东西
                assert!(
                    t.touches.iter().any(|f| f.path == *token),
                    "{}: touches 里没有 {token:?}",
                    t.id
                );
            }
        }
        // 计数自检：≥2 个使用者才配有这个变体（本工作区的 ≥2 判据）
        assert!(
            n >= 2,
            "UserConfiguredPath 只有 {n} 个使用者——不够 2 个就不该是一个变体"
        );
    }

    // ===== 建表：七个字段都真被用上（T01 审计 I2 的验收点） =====

    #[test]
    fn rows_cover_every_touched_file_and_use_all_spec_fields() {
        let f = empty_probe();
        let rows = build_rows(&home(), None, &no_dir, &f);
        let expected: usize = TOOLS.iter().map(|t| t.touches.len()).sum();
        assert_eq!(rows.len(), expected, "每条 touches 都要有一行");
        for r in &rows {
            assert!(!r.tool_id.is_empty() && !r.tool_name.is_empty()); // id / display_name
            assert!(!r.source_label.is_empty()); // source
            assert!(!r.effect_label.is_empty()); // touches[].effect
            assert!(!r.path_declared.is_empty()); // touches[].path
        }
        // destination：远端那条必须解析不出本机路径
        let daemon = rows.iter().find(|r| r.tool_id == "remote-daemon").unwrap();
        assert!(daemon.path_resolved.is_none());
        // installable / uninstallable：cc-bus 两者都 false，ccm 两者都 true
        let ccbus = rows.iter().find(|r| r.tool_id == "cc-bus").unwrap();
        assert!(!ccbus.installable && !ccbus.uninstallable);
        let ccm = rows.iter().find(|r| r.tool_id == "ccm").unwrap();
        assert!(ccm.installable && ccm.uninstallable);
        // note：至少两个工具用上了
        let with_note: std::collections::HashSet<_> = rows
            .iter()
            .filter(|r| r.note.is_some())
            .map(|r| r.tool_id)
            .collect();
        assert!(
            with_note.len() >= 2,
            "note 至少两个工具用上，实得 {with_note:?}"
        );
    }

    /// `host` 必须进到行里（T04）——不上屏的话用户分不出说的是哪台机器。
    #[test]
    fn rows_carry_the_host_label() {
        let rows = build_rows(&home(), None, &no_dir, &empty_probe());
        for r in &rows {
            assert!(!r.host_label.is_empty(), "{} 缺 host 标签", r.path_declared);
        }
        // 四档措辞各不相同，且能看出"哪台"
        let labels: std::collections::HashSet<_> = rows.iter().map(|r| r.host_label).collect();
        assert!(
            labels.len() >= 3,
            "至少三种 host 出现在表里，实得 {labels:?}"
        );
        let ps = rows.iter().find(|r| r.path_declared == "$PROFILE").unwrap();
        assert_eq!(ps.host_label, "本机");
        let ccm = rows
            .iter()
            .find(|r| r.path_declared == "~/.local/bin/ccm")
            .unwrap();
        assert_eq!(ccm.host_label, "远端");
    }

    /// `GenerateOnly` 的措辞必须**明确说我们不写**——这是用户定的调，写错了就是失信。
    #[test]
    fn generate_only_wording_says_we_do_not_write() {
        let l = effect_label(TouchEffect::GenerateOnly);
        assert!(l.contains("不写"), "实得 {l:?}");
        assert!(l.contains("待贴文本"));
        assert!(effect_label(TouchEffect::ReadOnly).contains("不写"));
    }

    // ===== B04 登记项：settings 的多个作用域 =====

    #[test]
    fn settings_scopes_include_local_and_admit_project_is_unchecked() {
        let f = FsProbe {
            meta: &|p| {
                if p.to_string_lossy().ends_with("settings.json") {
                    Some((false, 10))
                } else {
                    None
                }
            },
            list: &|_| None,
        };
        let read = |p: &Path| {
            if p.to_string_lossy().ends_with("settings.local.json") {
                Some("{\"hooks\":{\"SessionStart\":\"cc-register\"}}".to_string())
            } else {
                Some("{}".to_string())
            }
        };
        let s = build_settings_scopes(&home(), None, &no_dir, &read, &f);
        assert_eq!(s.len(), 3);
        assert!(s[0].path.ends_with("/.claude/settings.json"));
        assert!(s[1].path.ends_with("/.claude/settings.local.json"));
        // local 里有钩子字样 → 必须报 true（B04 的病：只看第一份会说"没装"）
        assert_eq!(s[1].has_cc_bus_hooks, Some(true));
        assert_eq!(s[0].has_cc_bus_hooks, Some(false));
        // 项目级：**明说没查**，且不给 has_cc_bus_hooks 一个假答案
        assert_eq!(s[2].scope, "项目级");
        assert_eq!(s[2].has_cc_bus_hooks, None);
        match &s[2].state {
            SurfaceState::Undetermined { why } => {
                assert!(why.contains("没查"), "实得 {why}");
                assert!(why.contains("可能是错的"), "要点明结论可能错，实得 {why}");
            }
            other => panic!("项目级不该有确定结论，实得 {other:?}"),
        }
    }

    /// 读不到文件时 `has_cc_bus_hooks` 必须是 `None`（**不猜 false**）。
    #[test]
    fn unreadable_settings_does_not_claim_absence_of_hooks() {
        let s = build_settings_scopes(&home(), None, &no_dir, &|_| None, &empty_probe());
        assert_eq!(s[0].has_cc_bus_hooks, None);
        assert_eq!(s[1].has_cc_bus_hooks, None);
    }

    /// **本模块只准读，不准写**（红线）。守法是**白名单**而不是"不许出现哪些写法"
    /// ——本会话的教训：黑名单版本被审计用五种我没想到的写法绕过，
    /// 而白名单枚举每一处 `fs::` 用法并要求它们**都**在允许集合里，新写法自动被拦。
    #[test]
    fn this_module_only_reads() {
        let src = include_str!("config_surface.rs");
        let code = src.split(concat!("#[cfg", "(test)]")).next().unwrap_or(src);
        // 剥掉注释与文档：注释里出现 `fs::write` 这个词不该判红（本会话踩过：
        // 把注释当代码，一条守卫数出 3 处而实际只有 1 处）
        let stripped: String = code
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                !t.starts_with("//")
            })
            .collect::<Vec<_>>()
            .join("\n");
        // 反向自检：剥完还得看得见真代码，否则守卫在空转
        assert!(
            stripped.contains("pub fn resolve_touched_path"),
            "剥过头了，守卫在空转"
        );
        // **扫任意前缀的 `fs::`，不只 `std::fs::`**（T02 审计重要 2 实测可绕）：
        // 注入 `use std::fs;` + `fs::write(...)` 后旧守卫 17/17 全绿，因为它只找字面
        // `std::fs::` 前缀、而 `write` 也不在那 4 个禁用词里。同类绕法还有
        // `tokio::fs::write`、`std::os::unix::fs::symlink`。
        let mut uses: Vec<String> = Vec::new();
        for (i, _) in stripped.match_indices("fs::") {
            let rest = &stripped[i + "fs::".len()..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                uses.push(name);
            }
        }
        // 允许集合就这三个，全部只读
        for u in &uses {
            assert!(
                matches!(u.as_str(), "metadata" | "read_dir" | "read_to_string"),
                "本模块只准只读的 fs 调用，发现 fs::{u}"
            );
        }
        // 计数自检（要件 3）：一处都没扫到 = 守卫失效了，而不是代码变干净了
        assert!(
            uses.len() >= 3,
            "只扫到 {} 处 fs:: 用法——守卫可能失效了（期望 metadata/read_dir/read_to_string 各至少一处）",
            uses.len()
        );
        // **钉死 `use` 列表**（要件 4：逃生口的定义必须逐字钉住）。不钉的话
        // `use tokio::fs as fs;` 之类能把上面的白名单整体架空。
        let uses_lines: Vec<&str> = stripped
            .lines()
            .map(|l| l.trim())
            .filter(|l| l.starts_with("use "))
            .collect();
        assert_eq!(
            uses_lines,
            vec![
                "use crate::tool_registry::{",
                "use std::path::{Path, PathBuf};",
            ],
            "本模块的 use 列表被改了——它是上面那条 fs:: 白名单的前提，改了要重新论证"
        );
        // 且明确不许出现这些（即便将来换成别的前缀写法，上面的白名单也已经兜住 std::fs::）
        for bad in [
            "OpenOptions",
            "create_dir",
            "remove_file",
            "set_permissions",
        ] {
            assert!(!stripped.contains(bad), "本模块不得出现 {bad}——这一页只读");
        }
    }
}
