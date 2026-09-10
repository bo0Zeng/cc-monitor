//! **skill 接入面：一份声明（数据）+ 一个通用宿主（代码）**〔devbench F02, 08-10〕。
//!
//! # 它治什么
//!
//! 用户原话：「**这些 cc-bus、code-picture 的开发要充分解耦，只要是 skill 的集成都要好好解耦，
//! 因为 skill 容易改变，而且再怎么样他都是一个 skill**」。
//!
//! ⇒ 把「会变的」与「不变的」切开：
//!
//! | 会变 | 不变 |
//! |---|---|
//! | 命令名 · 文件名 · schema · 阶段名 · 判据清单 | 它住在一个**目录**里 · 它产**文本产物** · 产物里有**人要改的那个文件** · 它得先**装得上** |
//!
//! 会变的一律**不进宿主**；不变的那四样就是 [`SkillSpec`] 的四段。
//!
//! # cc-monitor **不调用** skill
//!
//! 〔用 08-10〕「**不调用任何 skill，只是集成 skill 功能**……但是调用还是让 agent 来调用。
//! **只提供 skill 安装功能**」⇒ 本模块**没有** `actions` 段、**不 spawn 任何 skill 命令**。
//! 唯一会碰外部世界的地方是 [`Discover`] 的存在性探测（`Path::exists`，不执行任何东西）。
//! cc-monitor 是 skill 的**装配台 + 产物编辑台**；跑 skill 是 agent 的活。
//!
//! # 为什么是编译期常量，不是 `*.toml`
//!
//! 摸底实测两条（devbench F02 §0）：① 本仓**没有 TOML 依赖**（只有 `serde`/`serde_json`）；
//! ② 本仓所有「注册表」都是编译期 Rust 常量（`tool_registry::TOOLS` ·
//! `polling_registry::REGISTERED` · `rust_timer_registry::REGISTERED`）。
//!
//! ★ 而编译期常量让 C3 那条要求**更强**而不是更弱：判据的人群是**遍历数组**，
//! 不是「运行时读得到几个文件」。后者正是 F07 变异 3 逮到的那一族失效
//! （人群来自文件系统 ⇒ 目录没了就零命中地绿）。
//!
//! ⚠ 代价如实记：用户自己加 skill 要改代码仓（`ROADMAP §5` 的 5b 已登记）。
//! 将来若要支持「用户数据目录里的声明」，**那时再加一层运行时来源**，
//! 而不是现在就为它把形态选歪。
//!
//! # 本模块**不做**什么
//!
//! 不做 IPC、不做 UI、**不落盘**。[`editable_paths`] 只**算出**允许写的路径集合；
//! 「写必须过这个集合 + 过 `sftp_pool::is_protected_claude_data_path` + 过
//! `verified_write`」那条围栏归 **F03**。⇒ 本模块是纯函数层，**它算得对不等于没人绕过它**。

use std::path::{Path, PathBuf};

/// 一个 skill 在 cc-monitor 里的接入声明。**恰好四段**。
///
/// 加一个 skill = 往 [`SKILLS`] 加一项。**宿主的三个函数里不许出现任何 `id` 字面量**
/// （由 `every_host_fn_is_blind_to_skill_names` 钉住，人群从本表派生）。
pub struct SkillSpec {
    /// 稳定标识。**只在本表里出现**，宿主逻辑不认它。
    pub id: &'static str,
    /// UI 上显示的名字（F03 用）。
    pub label: &'static str,
    /// ① 怎么发现它 + 前提探测。
    pub discover: Discover,
    /// ② 产物落点。
    pub artifacts: Artifacts,
    /// ③ 可编辑白名单：**相对 [`Artifacts::root`]** 的文件名。写面围栏的唯一来源。
    ///
    /// ⚠⚠ **08-10（F03 摸底）订正：基准从「实例目录」改成「root」。**
    /// F02 里我写的是「相对每个实例目录」，而第一个真实使用者（F03 的收件箱入口）
    /// 一接就发现指错了：`INBOX.txt` 是**项目级**的进件口（planned-build skill 明写
    /// 「住计划目录根，**不住工作区**，一个项目一份」），算成 `<工作区>/INBOX.txt`
    /// 那个文件根本不存在。
    /// ★ **F02 的判据当时全绿** —— 因为它只验「集合来自声明」，**没验那些路径指得对**。
    /// 纯函数判据看不出「集合整体指错地方」。⇒ 已补 `the_editable_paths_point_at_real_files`
    /// 这条**接线层**判据（在真实工作目录上算一次并断言文件真的在）。
    /// ⇒ 教训：**「用第二份声明验 schema」验的是「装得下」，验不了「接上去对不对」**，
    /// 后者只有真实使用者能验。
    pub editable: &'static [&'static str],
    /// ④ 装 / 升 / 卸。**只指向装法的住址，不复制装法**。
    pub install: Install,
}

/// 怎么确认这个 skill 在场。
///
/// ⚠ **只做存在性检查，不执行任何程序**（见模块头注：cc-monitor 不调用 skill）。
pub enum Discover {
    /// Claude 数据目录下的 skill 库：`<claude_dir>/skills/<dir>/<probe_file>` 必须在。
    ///
    /// ⚠⚠ **入参是 `claude_dir` 而不是 `home`，这一点是被判据逼出来的、且是实质的**：
    /// 本仓有多账号隔离（`cc-acct-iso`：每账号一个 `CLAUDE_CONFIG_DIR`，
    /// `skills`/`memory` 全部 symlink 回同一共享库）⇒ **写死 `~/.claude/skills` 在切号后会指错**。
    /// 真相源是 [`crate::paths::resolve_claude_dir`]，本模块只负责在它下面拼 `skills/`。
    /// ★ 这个缺陷是 `local_read_surface_registry` 报「多一处未登记的本机读面」时才暴露的 ——
    /// 那条判据的价值不止于「记上账」。
    ClaudeSkill {
        dir: &'static str,
        probe_file: &'static str,
    },
    /// 工作目录下的相对路径：`<cwd>/<path>` 必须在。
    CwdPath { path: &'static str },
}

/// 产物在哪、什么算一个「实例」。
///
/// 「实例」是 skill 的工作单位：planned-build 的一个工作区、cc-bus 的一条总线目录……
/// 形状同构 ⇒ 宿主用同一套逻辑列它们。
pub struct Artifacts {
    /// 相对工作目录的根。
    pub root: &'static str,
    /// 判别子目录是不是一个实例：这个文件在，就算。
    ///
    /// ⚠ 用**标记文件**而不是「所有子目录都算」：后者会把 `hooks/`、`.git/` 这类
    /// 非实例目录也列出来（实测计划目录里就有这两个）。
    pub instance_marker: &'static str,
}

/// 装法的住址。**本模块不实现装，只说它归谁。**
pub enum Install {
    /// 归 `tool_registry::TOOLS` 里这个 id —— **只指向装法的住址，不复制装法**。
    ///
    /// ⚠⚠ **F06 订正**：这里原写「同一张表的两个视图，不是两份数据」，**那句是错的**。
    /// [`SKILLS`]（接入的 skill）与 `TOOLS`（受管工具：装到别处的东西）是
    /// **两个不同集合，有交集** —— 今天交集只有 `cc-bus`。`planned-build` 在这边不在那边
    /// （它不由 cc-monitor 装），而 `ccm`/`cc-acct-iso`/… 在那边不在这边（它们不是 skill）。
    /// ⇒ 该钉的是「`ManagedTool(id)` ⇒ id ∈ TOOLS」这**一个方向**，反方向是假命题。
    ManagedTool(&'static str),
    /// 本轮不支持装，带理由。**如实登记，不假装可装。**
    NotSupported(&'static str),
}

/// 接入声明全表。**这是「加一个 skill = 加一份声明」的那个「一份」。**
pub const SKILLS: &[SkillSpec] = &[
    SkillSpec {
        id: "planned-build",
        label: "计划",
        // 它的入口是 `bin/pb.py`：那个文件在，就说明这个 skill 装好了。
        discover: Discover::ClaudeSkill {
            dir: "planned-build",
            probe_file: "bin/pb.py",
        },
        artifacts: Artifacts {
            root: ".claude/planned-build",
            // 一个工作区必有 STATUS.md（它是 skill 规定的恢复入口）。
            // ⚠ 用它当标记正好排除 `hooks/`（git 钩子目录，不是工作区）。
            instance_marker: "STATUS.md",
        },
        // ★ 唯一可编辑的就是收件箱 —— 那是「结构化注入」的落点（定框 C2）。
        // ⚠ 这里**刻意只有一个文件名**：写面越窄越好论证。
        editable: &["INBOX.txt"],
        install: Install::NotSupported(
            "planned-build 今天不在 tool_registry 那张表里（那 6 条是 ccm / cc-bus / \
             cc-acct-iso / remote-daemon / project-mcp / powershell-profile）。\
             把它变成可装归 devbench F06。",
        ),
    },
    // ★★ **第二份声明的作用是验 schema 装不装得下，不是实现它的 UI**（devbench F02 DoD）。
    // 一个抽象只有一个使用者时，它是猜的；两个样本推出来的才不是。
    // ⚠ cc-bus 这份**今天没有 UI 消费者**，这是刻意的 —— 见 F02 §2「不做什么」。
    SkillSpec {
        id: "cc-bus",
        label: "总线",
        discover: Discover::ClaudeSkill {
            dir: "cc-bus",
            probe_file: "SKILL.md",
        },
        artifacts: Artifacts {
            // cc-bus 的运行时目录在家目录下，不在工作目录 —— 但 `root` 的语义是
            // 「相对工作目录」。★ **这正是第二份声明验出来的第一个 schema 缺口**，
            // 已如实记进 F02 的摸底：本轮用仓内那份脚本目录当 root（它确实在工作目录下），
            // 而「运行时总线目录」那个概念本 schema today 装不下。
            root: "cc-monitor/shared/cc-bus",
            instance_marker: "SKILL.md",
        },
        // cc-bus 没有「人手写的注入文件」这种东西 —— 它的 inbox 是程序写的 jsonl。
        // ⇒ 空数组是**有意义的值**，不是没填。
        editable: &[],
        install: Install::ManagedTool("cc-bus"),
    },
];

/// # 为什么本仓**没有** skill 的「装/卸」UI —— 那是**不许装**，不是「还没写」〔`PS2` 08-12〕
///
/// 被指派「做 skill 的装卸面」的人会落在本文件。先读这段，省下几天：
///
/// 1. **两条 skill 一条都装不了。** `planned-build` 是 `Install::NotSupported`
///    （用户自己装的，我们只读它的产物）；`cc-bus` 是 `ManagedTool("cc-bus")`，
///    而 `TOOLS` 里那条 `installable: false` —— 深理由写在 `tool_registry.rs` 那条上：
///    **落点 `~/.claude/skills/cc-bus` 被只读铁律排除**（`doc/INVARIANTS.md` 穷举的 6 条例外
///    没有一条覆盖它，第 2 条逐字「**绝不碰** `~/.claude/`」）。
///    ⇒ 做出来的按钮**恒灰或骗人**。要开口子得先裁 —— 待决 `U10b`。
///
/// 2. **「已装 / 未装 / 版本不符」的第三态今天在类型上就不存在**（见下面的 [`Presence`]）。
///    它不是加个 UI 分支的事：得先有「装着的那份是哪个版本」这个量，而那个量**没有真相源** ——
///    仓内那份与 `~/.claude/skills/` 那份实测**差 167 行**，拿哪份当真相源是 `U9` 第二问，**未裁**。
///    **先画三态再去凑数据，顺序是反的。**
///
/// 3. ⇒ 结论：**不做那个按钮**，也不做一个恒灰的占位 —— 一个永远点不动的入口
///    比没有入口更糟，它让人以为「功能在，只是坏了」。
///
/// 探测结果。**失败必须说清是哪个 skill 的哪条前提**（定框 C6）。
#[derive(Debug, PartialEq, Eq)]
pub enum Presence {
    Found,
    /// 带身份的缺席：`(skill id, 期望的那条路径)`。
    ///
    /// ⚠ 不许退化成「不可用」三个字 —— UI 要能告诉用户**哪一条前提没满足**，
    /// 否则 skill 改版之后按钮还在、点了没反应，那正是 C6 禁的形状。
    Missing {
        skill: String,
        expected: PathBuf,
    },
}

impl Presence {
    /// 给 UI / 日志的一行人话。**必然含 skill 身份与那条路径。**
    pub fn describe(&self) -> String {
        match self {
            Presence::Found => "在场".to_string(),
            Presence::Missing { skill, expected } => {
                format!("{skill} 的前提没满足：找不到 {}", expected.display())
            }
        }
    }
}

/// 一个 skill 的一个实例（planned-build 的一个工作区 / …）。
#[derive(Debug, PartialEq, Eq)]
pub struct Instance {
    /// 目录名（如工作区名）。
    pub name: String,
    /// 绝对路径。
    pub dir: PathBuf,
}

/// ① 这个 skill 在不在。
///
/// `claude_dir` 与 `cwd` 都是**入参**（不去读进程环境）—— 那让它可测，也让它不依赖启动方式。
/// 生产调用方应传 [`crate::paths::resolve_claude_dir`] 的结果，**不要自己拼 `~/.claude`**
/// （多账号隔离下那是错的，见 [`Discover::ClaudeSkill`] 的说明）。
pub fn discover(spec: &SkillSpec, claude_dir: &Path, cwd: &Path) -> Presence {
    let expected = match &spec.discover {
        Discover::ClaudeSkill { dir, probe_file } => {
            claude_dir.join("skills").join(dir).join(probe_file)
        }
        Discover::CwdPath { path } => cwd.join(path),
    };
    if expected.exists() {
        Presence::Found
    } else {
        Presence::Missing {
            skill: spec.id.to_string(),
            expected,
        }
    }
}

/// ② 列出这个 skill 的实例（按名字排序，稳定输出）。
pub fn instances(spec: &SkillSpec, cwd: &Path) -> Vec<Instance> {
    let root = cwd.join(spec.artifacts.root);
    let Ok(rd) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut out: Vec<Instance> = rd
        .flatten()
        .filter_map(|e| {
            let dir = e.path();
            if !dir.is_dir() || !dir.join(spec.artifacts.instance_marker).exists() {
                return None;
            }
            Some(Instance {
                name: dir.file_name()?.to_string_lossy().into_owned(),
                dir,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// ③ 这个 skill **允许编辑**的路径集合（相对 [`Artifacts::root`]）。
///
/// ⚠ 返回的是**集合**，不是「判断一个路径行不行」的谓词 —— 因为集合可以被判据整体检查，
/// 而谓词只能被逐例试探。写面围栏要拿这个集合做 `contains` 判定，
/// 且必须在**路径解析之后**判（符号链接与 `..` 都要先解析掉）。
///
/// ⚠ **本函数算得对 ≠ 没人绕过它** —— 强制「写必须过它」由 [`resolve_editable`] 承担。
pub fn editable_paths(spec: &SkillSpec, cwd: &Path) -> Vec<PathBuf> {
    let root = cwd.join(spec.artifacts.root);
    spec.editable.iter().map(|f| root.join(f)).collect()
}

/// ★ **写面围栏的唯一入口**〔F03〕：把一个「用户想编辑的路径」解析并判定。
///
/// # 三道，缺一不可
///
/// 1. **解析后**再判（`canonicalize`）—— 符号链接与 `..` 都解掉。判字符串是可绕的：
///    `<root>/../../etc/passwd` 在字符串上「以 root 开头」，解析后就不是了。
/// 2. **集合判定**，不是一串 `if` —— 集合来自声明（[`editable_paths`]），
///    所以「能写哪些」这件事的真相源只有声明表一处。
/// 3. **过 `is_protected_claude_data_path`** —— 纵深防御。即使声明写歪了，
///    也不许碰 Claude 的 jsonl/pidfile（`doc/INVARIANTS.md:11` 那条只读铁律的对象）。
///
/// ⚠ 目标文件**必须已存在**才让写：本功能是「编辑收件箱」，不是「创建任意文件」。
/// 不存在就拒 —— 那让写面严格等于「声明里那几个真实文件」，而不是「那几个路径名」。
pub fn resolve_editable(spec: &SkillSpec, cwd: &Path, requested: &Path) -> Result<PathBuf, String> {
    let real = requested.canonicalize().map_err(|e| {
        format!(
            "解析路径失败（文件必须已存在）：{} — {e}",
            requested.display()
        )
    })?;

    let allowed: Vec<PathBuf> = editable_paths(spec, cwd)
        .iter()
        .filter_map(|p| p.canonicalize().ok())
        .collect();

    if !allowed.contains(&real) {
        return Err(format!(
            "拒绝写入：{} 不在 `{}` 的可编辑集合里。\n\
             该 skill 声明的可编辑文件是 {:?}（相对 {}）。\n\
             ⚠ 这是集合判定、且在路径解析之后 —— 符号链接与 `..` 都已解开。",
            real.display(),
            spec.id,
            spec.editable,
            spec.artifacts.root
        ));
    }

    // 纵深防御：即使上面放行，也不许碰 Claude 的数据文件。
    let as_str = real.to_string_lossy();
    if crate::sftp_pool::is_protected_claude_data_path(&as_str) {
        return Err(format!(
            "拒绝写入：{} 是 Claude 的数据文件（jsonl/pidfile）。\n\
             那是 `doc/INVARIANTS.md` 只读铁律的对象 —— 声明表把它列进 editable 也不行。",
            real.display()
        ));
    }
    Ok(real)
}

// ───────────────────────── IPC（F03：收件箱编辑入口的后端那半）─────────────────────────
//
// ⚠ 这一层**只做三件事**：列出 skill 与它的实例 · 读那个可编辑文件 · 写那个可编辑文件。
// 写必须过 [`resolve_editable`] 的三道围栏 + `verified_write` 的读回比对。
// **仍然不跑任何 skill 命令**（模块头注那条红线由 `the_host_never_spawns_anything` 钉着）。

/// 一个 skill 的当前状态（给 UI 用）。
#[derive(serde::Serialize)]
pub struct SkillView {
    pub id: String,
    pub label: String,
    /// `null` = 在场；否则是**带身份的**缺席原因（C6）。
    pub missing_reason: Option<String>,
    /// 这个 skill 的实例名（planned-build 的工作区名…）。
    pub instances: Vec<String>,
    /// 可编辑文件的绝对路径（已算好，UI 直接拿去请求读/写）。
    pub editable: Vec<String>,
}

/// 生产路径上的 cwd：**活跃 tab 的 cwd**（与 panorama 的取法一致）。
///
/// ⚠ 它是**入参**而不是进程 cwd —— cc-monitor 的进程 cwd 与用户在看的那个项目无关。
fn views(cwd: &Path) -> Vec<SkillView> {
    let claude_dir = crate::paths::resolve_claude_dir().unwrap_or_default();
    SKILLS
        .iter()
        .map(|spec| SkillView {
            id: spec.id.to_string(),
            label: spec.label.to_string(),
            missing_reason: match discover(spec, &claude_dir, cwd) {
                Presence::Found => None,
                m @ Presence::Missing { .. } => Some(m.describe()),
            },
            instances: instances(spec, cwd).into_iter().map(|i| i.name).collect(),
            editable: editable_paths(spec, cwd)
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
        })
        .collect()
}

/// 列出所有接入的 skill 及其状态。
#[tauri::command]
pub async fn list_skills(cwd: String) -> Result<Vec<SkillView>, String> {
    Ok(views(Path::new(&cwd)))
}

/// 读一个可编辑文件。**必须先过围栏** —— 读也过，免得它变成一个任意文件读取口。
#[tauri::command]
pub async fn read_skill_file(
    cwd: String,
    skill_id: String,
    path: String,
) -> Result<String, String> {
    let spec = SKILLS
        .iter()
        .find(|s| s.id == skill_id)
        .ok_or_else(|| format!("未知 skill：{skill_id}"))?;
    let real = resolve_editable(spec, Path::new(&cwd), Path::new(&path))?;
    std::fs::read_to_string(&real).map_err(|e| format!("读取失败：{} — {e}", real.display()))
}

/// 写一个可编辑文件：**围栏 → 备份 → 写 → 读回比对 → 不符就回滚**。
///
/// ⚠ 复用 `verified_write::verify_and_rollback`，**不自己造第四份写入实现** ——
/// 那个模块的头注逐字记着本仓曾有 4 处独立实现且校验强度不一致（两处只比长度）。
#[tauri::command]
pub async fn write_skill_file(
    cwd: String,
    skill_id: String,
    path: String,
    content: String,
) -> Result<(), String> {
    let spec = SKILLS
        .iter()
        .find(|s| s.id == skill_id)
        .ok_or_else(|| format!("未知 skill：{skill_id}"))?;
    let real = resolve_editable(spec, Path::new(&cwd), Path::new(&path))?;

    // 备份：读不到就当空（文件必然存在 —— 围栏要求 canonicalize 成功）。
    let backup = std::fs::read_to_string(&real).unwrap_or_default();
    std::fs::write(&real, &content).map_err(|e| format!("写入失败：{} — {e}", real.display()))?;

    let read_target = real.clone();
    let rollback_target = real.clone();
    crate::verified_write::verify_and_rollback(
        &content,
        || std::fs::read_to_string(&read_target).map_err(|e| format!("{e}")),
        || {
            let _ = std::fs::write(&rollback_target, &backup);
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 本模块源码（剥掉测试段）—— 三条钉法都在它上面取样。
    fn host_src() -> String {
        guard_core::production_code(include_str!("skill_host.rs"))
    }

    /// 从生产段里切出一个具名函数的函数体。切不到就 panic（**抽取器自检**：
    /// 切歪了下面几条会零命中地绿）。
    fn fn_body(src: &str, sig: &str) -> String {
        let at = src
            .find(sig)
            .unwrap_or_else(|| panic!("找不到 `{sig}` —— 抽取器坏了，本条会零命中地绿"));
        let rest = &src[at..];
        let end = rest
            .find("\n}\n")
            .unwrap_or_else(|| panic!("找不到 `{sig}` 的结尾 —— 抽取器坏了"));
        rest[..end].to_string()
    }

    /// ★★ **钉法 1（C3 的正题）：宿主逻辑里不许出现任何具体 skill 名。**
    ///
    /// # 人群从声明表派生，不是手写禁词表
    ///
    /// 这是本仓反复吃亏的那个病根（「判据的人群总是从**已经有名字**的那批派生」）的**对治**：
    /// 禁词表写死 `["planned-build","cc-bus"]` 的话，加第三个 skill 就漏了；
    /// 遍历 `SKILLS` 则**加一份声明自动进人群**。
    #[test]
    fn every_host_fn_is_blind_to_skill_names() {
        let src = host_src();
        // 抽取器自检：表本身必须非空，否则下面整条空转。
        assert!(
            SKILLS.len() >= 2,
            "声明表少于 2 份 —— 本件的 DoD 要求用两份声明验 schema"
        );
        let bodies = [
            fn_body(&src, "pub fn discover("),
            fn_body(&src, "pub fn instances("),
            fn_body(&src, "pub fn editable_paths("),
        ];
        for spec in SKILLS {
            for (i, body) in bodies.iter().enumerate() {
                assert!(
                    !body.contains(spec.id),
                    "宿主的第 {} 个函数体里出现了 skill 名 `{}` —— \n\
                     宿主必须对具体 skill 一无所知（定框 C3）。要按 skill 分叉的行为，\n\
                     应该变成声明里的一个字段，而不是宿主里的一个 `if`。",
                    i + 1,
                    spec.id
                );
            }
        }
    }

    /// ★ **钉法 3（C6）：`Missing` 必须带身份 —— 阴性对照式。**
    ///
    /// 把 probe 指向一个不存在的路径，断言输出里**能看到是哪个 skill 的哪条前提**。
    /// ⚠ 只断言「返回了 Missing」是不够的：那不能区分「带身份」与「只说不可用」。
    #[test]
    fn a_missing_skill_says_which_one_and_which_path() {
        let nowhere = Path::new("/nonexistent-claude-dir-for-tests");
        let cwd = Path::new("/nonexistent-cwd-for-tests");
        for spec in SKILLS {
            let got = discover(spec, nowhere, cwd);
            let msg = got.describe();
            assert!(
                msg.contains(spec.id),
                "缺席信息里没有 skill 身份：{msg:?} —— C6 要求说清是哪一条前提没满足"
            );
            match got {
                Presence::Missing { expected, .. } => {
                    let p = expected.to_string_lossy();
                    assert!(
                        p.contains("nonexistent-claude-dir-for-tests")
                            || p.contains("nonexistent-cwd-for-tests"),
                        "期望路径没落在传入的根下：{p} —— discover 可能读了进程环境而不是入参"
                    );
                    assert!(msg.contains(&*p), "缺席信息里没有那条期望路径：{msg}");
                }
                Presence::Found => {
                    panic!("在一个不存在的 claude_dir 下竟然报 Found —— probe 形同虚设")
                }
            }
        }
    }

    /// ★ **钉法 2（写面）：`editable_paths` 是集合，且恰好来自声明。**
    ///
    /// ⚠ 本条只验「集合算得对」。**「写必须过这个集合」要等 F03** ——
    /// 如实登记为诚实边界，不假装本件已经把写面围住了。
    #[test]
    fn editable_paths_come_from_the_spec_and_nowhere_else() {
        let cwd = Path::new("/tmp/x");
        for spec in SKILLS {
            let got = editable_paths(spec, cwd);
            assert_eq!(
                got.len(),
                spec.editable.len(),
                "`{}` 的可编辑集合大小与声明不符 —— 宿主凭空加了或漏了路径",
                spec.id
            );
            let root = cwd.join(spec.artifacts.root);
            for (p, f) in got.iter().zip(spec.editable) {
                assert_eq!(
                    p,
                    &root.join(f),
                    "`{}` 的可编辑路径不是「artifacts.root + 声明里的文件名」",
                    spec.id
                );
            }
        }
    }

    /// 本仓的**工作目录**（`cc-monitor/` 的上一级）—— `artifacts.root` 相对的就是它。
    ///
    /// ⚠ 这是一个**假设**，写出来免得它变成隐含知识：计划目录住工作目录的 `.claude/`，
    /// 而 `cc-monitor` 是工作目录下的子仓。生产路径上这个 cwd 由**活跃 tab** 决定
    /// （同 panorama 的 `RepoInfoGetter`），测试里从**这个 crate 属于哪个仓**推出来。
    ///
    /// # 它**问 git**，不从路径往上数〔`K-R13` 09-01〕
    ///
    /// 原来的算式是 `CARGO_MANIFEST_DIR` 往上跳两级。
    /// 主树上那是 `…/cc-monitor/src-tauri` ⇒ 跳两级恰好是工作目录，**算对了**；
    /// git 工作树上那是 `…/worktrees/<名>/src-tauri` ⇒ 跳两级是 `…/worktrees`，
    /// 那不是任何项目的工作目录，**算错了**。
    ///
    /// 而 08-26 有人在那个错落点上补了一个同名的东西 ⇒ **错的算式指到了一个真实存在的
    /// 目录**，靠它的两条判据从此在几十棵树上一起绿，一条也没出声
    /// （09-01 现打：56/56 棵工作树的老落点是**同一个**目录）。
    ///
    /// ⇒ 换成问**权威**：`git rev-parse --git-common-dir` 给的是主仓的 `.git`，
    /// 在链接工作树里问也一样 ⇒ **主树与工作树同一个答案、同一个理由**，
    /// 而不是一边靠算对、一边靠盘上碰巧有个同名的东西。
    ///
    /// ⚠ **推不出来就 panic（fail-closed），不回落旧算式** —— 见 [`workspace_cwd_from`]。
    fn workspace_cwd() -> PathBuf {
        workspace_cwd_from(Path::new(env!("CARGO_MANIFEST_DIR")))
            .unwrap_or_else(|why| panic!("推不出工作目录，本条判不了（不许当成绿）—— {why}"))
    }

    /// 问 git 要「本 crate 属于**哪个仓**」，再上跳两级 = 工作目录。
    ///
    /// `rev-parse --git-common-dir` 给的是**主仓**的 `.git`（在链接工作树里问也一样）——
    /// 这是本条唯一的权威。`<仓>/.git` → `<仓>` → 工作目录，恰好两级。
    ///
    /// ⚠ **推不出来一律 `Err`，不猜、不回落**。回落到「往上跳两级」等于把今天这个 bug
    /// 原样搬进 `unwrap_or` 的右边，而且从此连报错都没有。
    ///
    /// 参数是目录而不是写死 `CARGO_MANIFEST_DIR`，为的是能**对着一棵不是工作树的目录跑一次**
    /// （`the_workspace_cwd_fails_closed_when_git_cannot_answer` 就是那一格）。
    fn workspace_cwd_from(dir: &Path) -> Result<PathBuf, String> {
        let common = git_common_dir(dir)?;
        let repo = common
            .parent()
            .ok_or_else(|| format!("git 给的 {} 没有上一级 —— 层级不够，不猜", common.display()))?;
        let ws = repo
            .parent()
            .ok_or_else(|| format!("仓根 {} 没有上一级 —— 层级不够，不猜", repo.display()))?;
        Ok(ws.to_path_buf())
    }

    /// 一次只读的 `git rev-parse`。**两种「问不到」都要判**，它们在类型上不是一回事：
    /// 机器上没有 `git` 时 [`std::process::Command`] 给的是 `io::Error(NotFound)`，
    /// **不是**一个非零退出码（09-01 现打，两侧都量过）。
    fn git_common_dir(dir: &Path) -> Result<PathBuf, String> {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
            .output()
            .map_err(|e| {
                format!(
                    "起不来 `git`（{e}）—— 本条靠 git 当权威，问不到就不许猜一个出来。\n\
                     ⚠ 这一支不是「git 说不知道」，是**进程都没起来**（PATH 里没有它）。"
                )
            })?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            // 把 fail-closed 的两种来路分开 —— 它们在 git 的报错里长得一模一样，
            // 而处置完全不同：一种是这棵树自己的登记没了，一种是压根没在 git 树里。
            let why = if linked_worktree_pointer(dir).is_some() {
                "这棵树的 worktree 登记没了：它的 `.git` 还是那行指回主仓的指针，\
                 而主仓里对应的那份登记已经被 prune 掉了（盘上的树还在，版本控制里没有它了）。\n\
                 ⇒ 这不是本判据坏了，是这棵树本身已经不在版本控制里；\
                 它的门禁在更早的格子上就已经红了。"
            } else {
                "这里不在任何 git 树里（连指回主仓的那行指针都没有）。"
            };
            return Err(format!(
                "`git rev-parse` 在 {} 上退出码 {:?} —— {why}\ngit 自己说：{stderr}",
                dir.display(),
                out.status.code()
            ));
        }
        let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let p = PathBuf::from(&raw);
        if !p.is_absolute() {
            return Err(format!(
                "git 给的 common-dir 不是绝对路径（{raw}）—— `--path-format=absolute` 被拿掉了？\
                 相对路径在这里没有意义：它相对的是 git 进程的 cwd，不是我们问的那个目录。"
            ));
        }
        Ok(p)
    }

    /// 从 `from` 往上找：这棵树的 `.git` 是不是「一行指回主仓的指针」（链接工作树的形状）。
    ///
    /// 先撞到目录形的 `.git` ⇒ 不是链接工作树，`None`。
    fn linked_worktree_pointer(from: &Path) -> Option<PathBuf> {
        for d in from.ancestors() {
            let g = d.join(".git");
            if g.is_file() {
                return Some(g);
            }
            if g.is_dir() {
                return None;
            }
        }
        None
    }

    /// **第二条权威路**：`git worktree list --porcelain` 的第一条 = 主工作树的住址。
    ///
    /// ⚠ 刻意与 [`workspace_cwd_from`] 走**不同的查询** —— 把同一条查询抄两遍是自证，
    /// 不是对拍：那样改一处 flag 两边一起变，判据一声不吭。
    fn main_checkout_from_git(dir: &Path) -> Result<PathBuf, String> {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["worktree", "list", "--porcelain"])
            .output()
            .map_err(|e| format!("起不来 `git`（{e}）—— 对拍的那一侧也问不到权威了"))?;
        if !out.status.success() {
            return Err(format!(
                "`git worktree list` 在 {} 上退出码 {:?}：{}",
                dir.display(),
                out.status.code(),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        for l in text.lines() {
            // porcelain 格式：主工作树恒排第一条，字段名逐字是 `worktree <绝对路径>`。
            if let Some(rest) = l.strip_prefix("worktree ") {
                return Ok(PathBuf::from(rest));
            }
        }
        Err(
            "`git worktree list --porcelain` 里一条 `worktree ` 字段都没有 —— \
             抽取器坏了，本条会零命中地绿"
                .to_string(),
        )
    }

    /// ★★ **P3：`workspace_cwd()` 算得**对**不对 —— 直接断言，不再借「盘上有」。**
    ///
    /// # 为什么非要另立这一格〔`K-R13` 09-01，本件第一条验收〕
    ///
    /// 下面 [`the_editable_paths_point_at_real_files`] 的断言正文是
    /// `p.canonicalize().is_ok()` —— **纯粹「盘上有」**。它同时透过这一个观测手段
    /// 守着三件互相独立的事：
    ///
    /// | | 性质 | 工作树里还守不守 |
    /// |---|---|---|
    /// | P1 | `editable` 的基准是 `artifacts.root` 不是实例目录（F02 那个原缺陷） | 守 |
    /// | P2 | `artifacts.root` 的字面与真实盘上目录名对得上 | 守 |
    /// | P3 | `workspace_cwd()` **推得对** | **不守** |
    ///
    /// P3 出错之后盘上**仍然有**（08-26 有人在那个错落点上补了一个同名的东西）⇒
    /// 观测手段照旧满足，两条判据照旧绿。**判据没有坏，是它从来没有 P3 那一格。**
    /// 08-26 之前工作树里 P3 一错 P1 跟着红，那是**巧合的耦合**，不是有人在守。
    ///
    /// ⇒ 换算法只是把今天这个答案改对；**只有这一格会在它下次算错时出声。**
    ///
    /// # 它怎么判（两条**不同的** git 查询对拍）
    ///
    /// 实现问的是 `rev-parse --git-common-dir`（本 crate 属于哪个仓）；
    /// 本条问的是 `worktree list --porcelain` 的第一条（主工作树在哪）。
    /// 少跳一级 / 换个 flag / 退回「往上跳两级」，本条都红。
    ///
    /// ⚠ P3 **在词法上判不了**：`workspace_cwd()` 想要的「工作目录」是**项目**定义的
    /// （生产路径上由活跃 tab 给），不是 crate 位置的函数，而它也不是任何一级祖先所独有的特征。
    /// ⇒ 要判它只能问一个权威。这就是本条为什么起进程。
    #[test]
    fn the_workspace_cwd_is_derived_from_git_not_guessed_from_the_path() {
        let got = workspace_cwd();
        let main = main_checkout_from_git(Path::new(env!("CARGO_MANIFEST_DIR")))
            .unwrap_or_else(|why| panic!("问不到主工作树的住址，本条判不了 —— {why}"));
        let want = main
            .parent()
            .unwrap_or_else(|| panic!("主工作树 {} 没有上一级", main.display()))
            .to_path_buf();
        // 两边都 canonicalize：换个等价写法不该让本条红，**算错才该让它红**。
        let g = got.canonicalize().unwrap_or_else(|e| {
            panic!(
                "算出来的工作目录 {} 打不开（{e}）—— 算式推出了一个盘上没有的地方",
                got.display()
            )
        });
        let w = want
            .canonicalize()
            .unwrap_or_else(|e| panic!("权威给的 {} 打不开（{e}）", want.display()));
        assert_eq!(
            g,
            w,
            "工作目录**算错了**。\n\
             算出来 : {}\n\
             权威说 : {}\n\
             ⇒ 这一格判的是「算式推得对」，不是「算出来的地方盘上有东西」。\n\
             两者今天分得开：一个算错的路径完全可能指到一个真实存在的同名目录，\n\
             那时「盘上有」照样满足，而这一格会红。",
            g.display(),
            w.display()
        );
    }

    /// ★ **fail-closed 那一侧自己也要有一格** —— 「推不出来就不许猜」不许只写在注释里。
    ///
    /// 夹具形状 = 一棵**登记被 prune 掉**的树：`.git` 还是那行指回主仓的指针，
    /// 而它指向的 gitdir 不存在。09-01 现打，盘上真有 6 棵是这个形状。
    ///
    /// ⚠ 断言的子串取自**我们自己的诊断**，不取自夹具的目录名 —— 后者会让这一格靠路径恒真。
    #[test]
    fn the_workspace_cwd_fails_closed_when_git_cannot_answer() {
        let base = std::env::temp_dir().join(format!("wc-probe-{}", std::process::id()));
        let deep = base.join("a").join("b");
        std::fs::create_dir_all(&deep).expect("造夹具失败 —— 本条会零命中地绿");
        std::fs::write(base.join(".git"), "gitdir: /no-such-gitdir-for-this-case\n")
            .expect("造夹具失败 —— 本条会零命中地绿");

        let err = workspace_cwd_from(&deep).expect_err(
            "git 答不上来，它竟然还给出了一个工作目录 —— 那正是本件治的那个形状：\
             推错了还猜一个看起来很合理的东西出来",
        );
        let says_prune = err.contains("登记没了");
        std::fs::remove_dir_all(&base).ok();
        assert!(
            says_prune,
            "fail-closed 的正文没说清是哪一种「问不到」。\n\
             这一格要的不是「红」，是**红得说得清**：一棵树登记被 prune（盘上还在、\
             版本控制里没了）与「压根不在 git 树里」在 git 自己的报错里长得一样，\n\
             而两者的处置完全不同。实际拿到的是：\n{err}"
        );
    }

    /// ★ **git 给的必须是绝对路径，否则拒收** —— `--path-format=absolute` 那一段是承重的。
    ///
    /// # 为什么它值单独一格（而不是靠上面两条顺带守住）
    ///
    /// 09-01 两侧现打：`rev-parse --git-common-dir` **在链接工作树里本来就回绝对路径**
    /// （`…/cc-monitor/.git`），只有在**主工作树**里才回相对的 `../.git`。
    /// ⇒ 把那段 flag 拿掉，**在工作树上一格都不红**，而在主树上「往上跳两级」
    /// 会从一个相对路径起跳，跳到哪儿全看 git 进程的 cwd。
    ///
    /// 那正是本仓最贵那族病的形状：**在我这棵树上量不到，换一棵树才炸**。
    /// ⇒ 本条自己 `git init` 一棵**主工作树形状**的仓当夹具，
    /// 于是这一刀在**任何**树上跑都逮得到，不再靠「碰巧跑在哪棵树上」。
    #[test]
    fn a_relative_answer_from_git_is_refused_not_patched_up() {
        let base = std::env::temp_dir().join(format!("wc-repo-{}", std::process::id()));
        std::fs::remove_dir_all(&base).ok();
        std::fs::create_dir_all(&base).expect("造夹具失败 —— 本条会零命中地绿");
        let init = std::process::Command::new("git")
            .arg("-C")
            .arg(&base)
            .args(["init", "-q"])
            .status()
            .expect("起不来 `git` —— 本条判不了，不许当成绿");
        assert!(init.success(), "夹具仓建不起来 —— 本条会零命中地绿");

        let got = git_common_dir(&base);
        std::fs::remove_dir_all(&base).ok();
        let p = got.unwrap_or_else(|why| {
            panic!(
                "在一棵刚建好的仓上都问不到 common-dir —— {why}\n\
                 ⇒ 多半是问法变了（`--path-format=absolute` 被拿掉，git 回了相对路径，\
                 而我们**拒收**相对路径）。拒收是对的：相对路径相对的是 git 进程的 cwd，\
                 拿它往上跳两级跳到哪儿没人说得准。"
            )
        });
        assert!(
            p.is_absolute(),
            "git 回了一个非绝对路径而它竟然被收下了：{}\n\
             ⇒ 那道 `is_absolute` 的闸被拆了。",
            p.display()
        );
    }

    /// ★★ **接线层判据：算出来的路径必须真的指向存在的文件。**
    ///
    /// # 它为什么存在（这条是被一次真缺陷逼出来的）
    ///
    /// F02 收工时 `editable` 的基准是「实例目录」，算出 `<工作区>/INBOX.txt` —— 而那个
    /// 文件根本不存在（收件箱是**项目级**的，住计划目录根）。**F02 的判据当时全绿**：
    /// 它们只验「集合来自声明」「大小对得上」，**没有一条去看那些路径指得对不对**。
    ///
    /// ⇒ 纯函数判据看不出「集合**整体**指错地方」。这条补的就是那一层：
    /// 在**真实工作目录**上算一次，断言每条路径都能 `canonicalize`。
    ///
    /// ⚠ 它对**某个真实文件系统**有依赖 —— 这是**刻意的**。
    ///
    /// ⚠⚠ **这句话原来的后半截是错的，09-01 现打推翻**〔`K-R13` `§0a` 四③〕。
    /// 原文逐字：「它对文件系统有依赖……**也是它唯一有价值的原因**」。
    /// 实测不成立：换成**夹具目录**（在 `tempdir` 里搭一份同形的产物树）之后，
    /// 本条**照样守得住 P1**（`editable` 的基准写成实例目录，变异在夹具上照红）——
    /// 丢掉的只是 P2（`artifacts.root` 的字面与真实盘上目录名对得上），
    /// 而且只在「夹具从声明生成」那一版才丢。
    /// ⇒ 「真实」这个词买到的是 P2，**不是**本条的全部价值。把它写成「唯一」，
    /// 会让下一个想换夹具的人以为那等于把这条判据整个废掉。
    ///
    /// ⚠ 它**没有** P3（`workspace_cwd()` 推得对）那一格 —— 那一格另立在
    /// [`the_workspace_cwd_is_derived_from_git_not_guessed_from_the_path`]，
    /// 理由写在那条上：本条的断言正文是「盘上有」，而一个算错的路径完全可能指到
    /// 一个真实存在的同名目录，那时「盘上有」照样满足。
    #[test]
    fn the_editable_paths_point_at_real_files() {
        let cwd = workspace_cwd();
        // 抽取器自检：至少有一个 skill 声明了可编辑文件，否则整条空转。
        let total: usize = SKILLS.iter().map(|s| s.editable.len()).sum();
        assert!(
            total >= 1,
            "没有任何 skill 声明 editable —— 本条会零命中地绿（F03 的写面就没有对象了）"
        );
        for spec in SKILLS {
            // ★ **前置条件**〔09-09 补，云端首跑逼出来的〕：这几套产物树**不在版本控制里**
            //   —— `.claude/planned-build/` 住在**仓的上一级**（工作目录），
            //   仓里 `git ls-files` 对 `.claude/` 零命中 ⇒ **任何 checkout 上都没有它**。
            //   而本条原来的报错会把这一形说成「`artifacts.root` 错了 / `editable` 的基准
            //   理解错了」——**两条都是假话**，正是本条头注里记着的那种误诊。
            //   ⚠ 不许改成「不存在就跳过」：那是把「没跑」伪装成「跑了」。
            //   ⇒ 这一格只把**真因**说出来，红照旧红。
            if spec.editable.is_empty() {
                continue;
            }
            let root = cwd.join(spec.artifacts.root);
            assert!(
                root.is_dir(),
                "本条的前置条件不成立：`{}` 的产物根算出来是 {}，而它不存在。\n\
                 ⇒ 这套产物**不在版本控制里**（`.claude/planned-build/` 住在仓的上一级），\
                 所以任何 checkout 上都没有它 —— 本条在那种环境里**判不了**。\n\
                 ⚠ 它与「路径算错了」长得一样，只有这句话分得开。\n\
                 🔴 不许改成「不存在就跳过」。",
                spec.id,
                root.display()
            );
            for p in editable_paths(spec, &cwd) {
                assert!(
                    p.canonicalize().is_ok(),
                    "`{}` 声明的可编辑文件算出来是 {}，但它不存在。\n\
                     ⇒ 要么 `artifacts.root` 错了，要么 `editable` 的基准理解错了。\n\
                     ★ F02 收工时正是这个错：基准写成「实例目录」，而收件箱是**项目级**的\n\
                     （planned-build 明写「住计划目录根，不住工作区」）。\n\
                     ⚠ 本条红**不代表算式错了** —— 算式对不对由\n\
                     `the_workspace_cwd_is_derived_from_git_not_guessed_from_the_path` 单独判。\n\
                     那一格绿而本条红 ⇒ 工作目录是对的，是声明或盘上的产物对不上；\n\
                     两格一起红 ⇒ 先修那一格，本条多半是被它带红的。\n\
                     〔09-01 订正：这里原写「若本条红先确认工作目录布局」，而 08-26 起\n\
                     那句提醒被盘上一个同名目录消音了整整六天 —— 本条那时**没有红**。〕",
                    spec.id,
                    p.display()
                );
            }
        }
    }

    /// ★ **写面围栏：三道各自要能拦住东西。**
    #[test]
    fn the_write_fence_rejects_what_it_should() {
        let cwd = workspace_cwd();
        let spec = SKILLS
            .iter()
            .find(|s| !s.editable.is_empty())
            .expect("没有带 editable 的 skill —— 本条会零命中地绿");

        // ★ **前置条件**（同上一条，09-09 补）：这套产物树不在版本控制里
        //   ⇒ 任何 checkout 上都没有它，本条在那种环境里**判不了**。
        //   ⚠ 不许改成「不存在就跳过」：那是把「没跑」伪装成「跑了」。
        let root = cwd.join(spec.artifacts.root);
        assert!(
            root.is_dir(),
            "本条的前置条件不成立：`{}` 的产物根算出来是 {}，而它不存在 ——\n\
             这套产物不在版本控制里（`.claude/planned-build/` 住在仓的上一级），\n\
             本条在这个环境里判不了。🔴 不许改成「不存在就跳过」。",
            spec.id,
            root.display()
        );

        // ① 白名单内的真实文件：放行。
        let ok_path = &editable_paths(spec, &cwd)[0];
        assert!(
            resolve_editable(spec, &cwd, ok_path).is_ok(),
            "白名单里的真实文件被拒了 —— 围栏把该放的也拦了"
        );

        // ② 同目录下**没在白名单里**的文件：拒。
        //    用 `STATUS.md` 之类肯定存在、但不在 editable 里的东西才说明问题
        //    （拿一个不存在的文件去试，拦住的是「不存在」而不是「不在白名单」）。
        //    ⚠ `root` 在上面那条前置条件里已经算过一次，这里直接用。
        let sibling = root.join("README.md");
        if sibling.exists() {
            let err = resolve_editable(spec, &cwd, &sibling)
                .expect_err("同目录下不在白名单的文件竟然被放行");
            assert!(
                err.contains("不在") && err.contains(spec.id),
                "拒绝理由没说清是「不在可编辑集合里」以及是哪个 skill：{err}"
            );
        }

        // ③ 用 `..` 逃出去：拒。
        let escape = root.join("../../etc/hostname");
        if escape.canonicalize().is_ok() {
            assert!(
                resolve_editable(spec, &cwd, &escape).is_err(),
                "`..` 逃逸没被拦住"
            );
        }

        // ④ ★ **等价写法必须被接受** —— 这条才是「解析后判定」与「判字符串」的真分界。
        //
        // ⚠ **写下这条的经过值得记**：我原本用 ③（`..` 逃逸）当那个分界的阴性对照，
        // 实测**变异没红** —— 把 `canonicalize` 全去掉、改成纯字符串比较，8 条判据照样绿。
        // 查清之后发现不是判据弱，是**我的用例选错了**：集合判定用的是**精确相等**
        // 而不是「以 root 开头」，所以 `..` 逃逸在字符串下**同样不在集合里**、同样被拒。
        //
        // ⇒ 顺带修正了我对这个围栏的理解，如实记下强度分布：
        //   · **集合精确相等** = 主防线（很强：白名单是具体文件名，不是目录前缀）
        //   · `canonicalize` = ① 让等价写法可用（本条验的就是它）
        //                      ② 让第三道 `is_protected_claude_data_path` 看到**符号链接的真实目标**
        //                         而不是链接名
        //   · 第三道 = 纵深（即使声明写歪也不许碰 Claude 数据）
        let equivalent = root.join("devbench").join("..").join(spec.editable[0]);
        assert!(
            resolve_editable(spec, &cwd, &equivalent).is_ok(),
            "等价写法 {} 被拒了 —— 围栏在判字符串而不是判解析后的真实路径",
            equivalent.display()
        );
    }

    /// ★ **两个集合的交集项，两边 id 必须逐字一致**〔devbench F06〕。
    ///
    /// # 它们不是「同一张表的两个视图」
    ///
    /// [`SKILLS`]（接入的 skill：cc-monitor 显示它的产物、编辑它的注入文件）与
    /// `tool_registry::TOOLS`（受管工具：**装到别处**的东西）是**两个不同集合，有交集**。
    /// 今天交集只有 `cc-bus` 一个：`planned-build` 在这边不在那边（它不由 cc-monitor 装），
    /// 而 `ccm`/`cc-acct-iso`/`remote-daemon`/`project-mcp`/`powershell-profile`
    /// 在那边不在这边（它们不是 skill）。
    ///
    /// ⚠ **反方向刻意不钉**（「TOOLS 里的每个工具都该是一个 skill」）—— 那句话是假的，
    /// 钉它等于把一个错误的概念做成判据。devbench 的账本 L4 原写「同一张表的两个视图」
    /// 就是这个错，F06 已订正。
    #[test]
    fn the_intersection_uses_the_same_id_on_both_sides() {
        let reg = guard_core::production_code(include_str!("tool_registry.rs"));
        let mut intersect = 0usize;
        for spec in SKILLS {
            if reg.contains(&format!("id: {:?}", spec.id)) {
                intersect += 1;
            }
        }
        // 抽取器自检：交集为 0 说明要么抽取坏了，要么两张表真的毫无关系
        // （那时 `Install::ManagedTool` 那条判据也会红，两条互相印证）。
        assert!(
            intersect >= 1,
            "`SKILLS` 与 `TOOLS` 交集为 0 —— 抽取器坏了，或 `cc-bus` 从某一边消失了。\n\
             本条会零命中地绿，所以它必须先红。"
        );
    }

    /// ★ **跨语言对拍：TS 那份手写类型的字段必须与本结构体一致。**
    ///
    /// `SkillView` 在 TS 侧是**手写**的（不是 ts-rs 生成，照 `launch-cli-wire.ts` 的先例）
    /// ⇒ 没有编译器管着它。这条判据读 TS 源码逐字段对拍，漏一个就红。
    ///
    /// ⚠ 它**只钉字段名**，不钉类型 —— 那是本仓「名字钉死是普遍的、类型生成是按需的」
    /// 那条成文规则的档位。如实记，别读成「类型也对上了」。
    #[test]
    fn the_ts_view_type_matches_this_struct() {
        let ts = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/ipc/commands.ts"),
        )
        .expect("读不到 `src/ipc/commands.ts` —— 抽取器坏了，本条会零命中地绿");
        let at = ts
            .find("export interface SkillView {")
            .expect("TS 侧找不到 `SkillView` 接口 —— 它被改名或删了");
        let body = &ts[at..at + ts[at..].find('}').expect("接口没闭合")];

        // 人群从 Rust 这一侧派生：改结构体就自动进人群，不用记得回来加。
        for field in ["id", "label", "missing_reason", "instances", "editable"] {
            assert!(
                body.contains(field),
                "TS 的 `SkillView` 缺字段 `{field}`。\n\
                 它是手写类型（没有编译器管），Rust 侧 `skill_host::SkillView` 改了字段\n\
                 就必须来这里同步 —— 这条判据就是那个「必须」。"
            );
        }
        // 抽取器自检：真的切到了接口体，而不是切了个空串。
        assert!(
            body.len() > 60,
            "切出来的 TS 接口体只有 {} 字节 —— 切歪了，本条会零命中地绿",
            body.len()
        );
    }

    /// ★ **每一段都要语义非空** —— 「装得下」不等于「装得对」（F02 DoD Y21）。
    ///
    /// 没有这条，我可以给第二份声明填一堆空串让它编过，然后声称 schema 验过了。
    #[test]
    fn every_spec_section_is_semantically_filled() {
        for spec in SKILLS {
            assert!(!spec.id.is_empty() && !spec.label.is_empty(), "id/label 空");
            match &spec.discover {
                Discover::ClaudeSkill { dir, probe_file } => {
                    assert!(
                        !dir.is_empty() && !probe_file.is_empty(),
                        "{}: discover 空",
                        spec.id
                    );
                }
                Discover::CwdPath { path } => {
                    assert!(!path.is_empty(), "{}: discover 空", spec.id);
                }
            }
            assert!(
                !spec.artifacts.root.is_empty() && !spec.artifacts.instance_marker.is_empty(),
                "{}: artifacts 段有空字段 —— 空 root 会让 instances() 去列工作目录本身",
                spec.id
            );
            // ⚠ `editable` **允许为空**（cc-bus 就没有「人手写的注入文件」）——
            // 空数组是有意义的值。但每一项都不许是空串。
            for f in spec.editable {
                assert!(!f.is_empty(), "{}: editable 里有空文件名", spec.id);
            }
            match &spec.install {
                Install::ManagedTool(id) => {
                    assert!(!id.is_empty(), "{}: install 指向空 id", spec.id)
                }
                Install::NotSupported(why) => assert!(
                    why.len() > 20,
                    "{}: install 标 NotSupported 但理由太短（{} 字节）—— \
                     如实登记要说清为什么、归谁",
                    spec.id,
                    why.len()
                ),
            }
        }
    }

    /// ★ **`install` 指向的必须是 `tool_registry` 里真实存在的 id。**
    ///
    /// 这条把「两个视图」钉成「不是两份数据」（账本 L4）：指一个不存在的工具 ⇒ 当场红。
    #[test]
    fn managed_tool_ids_exist_in_the_tool_registry() {
        let reg = guard_core::production_code(include_str!("tool_registry.rs"));
        // 抽取器自检：那份源码里必须真的有 id 字段，否则下面恒绿。
        assert!(
            reg.contains("id:"),
            "`tool_registry.rs` 里读不到 `id:` —— 抽取器坏了，本条会零命中地绿"
        );
        for spec in SKILLS {
            if let Install::ManagedTool(tool_id) = &spec.install {
                let needle = format!("id: {tool_id:?}");
                assert!(
                    reg.contains(&needle),
                    "`{}` 的 install 指向 `tool_registry` 里不存在的 id `{tool_id}`\n\
                     （找的是字面 {needle}）—— 装法的住址只有一个，别在这里另立一份",
                    spec.id
                );
            }
        }
    }

    /// ★ **`actions` 段不许回潮**〔用 08-10：cc-monitor 不调用任何 skill〕。
    ///
    /// 本模块唯一碰外部世界的地方是存在性探测。一旦有人加回「跑 skill 命令」那条路，
    /// 诚实边界 5a 那个口子（外部命令间接写盘）就回来了。
    #[test]
    fn the_host_never_spawns_anything() {
        let src = host_src();
        for forbidden in ["Command::new", "std::process", "spawn("] {
            assert!(
                !src.contains(forbidden),
                "宿主里出现了 `{forbidden}` —— cc-monitor **不调用任何 skill**（用户 08-10）：\n\
                 它只做装配台与产物编辑台，跑 skill 是 agent 的活。\n\
                 若真要加，先回定框 C4 加一行理由，并把 ROADMAP 的 5a 从「靶子转移」改回「活的」。"
            );
        }
    }

    /// `PS2-Y1`：「为什么没有装卸面」那段边界必须留在本文件上。
    ///
    /// 它省的是**几天**：下一个被指派这件事的人若不知道两条 skill 一条都装不了，
    /// 会先去写 UI、再在联调时撞上 `installable: false`，最后才找到只读铁律那堵墙。
    ///
    /// ⚠ 读 `production_source`（**只剥测试段、保留注释**）—— 上一件 `P8b` 首跑就栽在
    /// 用错剥法上：`production_code` 连 `//` 一起剥，而这类判据钉的**恰恰是注释**。
    /// 剥测试段仍是必须的：否则本条自己这几个字面量会把自己喂绿（本会话第六次防同一个自伤）。
    #[test]
    fn why_there_is_no_install_ui_is_written_down_here() {
        let prod = guard_core::production_source(include_str!("skill_host.rs"));
        for needle in ["U10b", "U9", "installable: false", "恒灰"] {
            assert!(
                prod.contains(needle),
                "宿主头注里少了「{needle}」—— 那段边界是 `PS2` 唯一的交付物"
            );
        }
        // ★ 钉**说法本身**：把「不许装」写成「还没实现」是最可能的腐坏形态，
        // 而两者的处置完全不同（一个要裁定、一个要工时）。
        assert!(
            prod.contains("那是**不许装**，不是「还没写」"),
            "那句区分被改掉了 —— 它正是本件的正题"
        );
    }

    /// `PS2-Y2`：`Presence` 今天**恰好两态**。
    ///
    /// ★★ 本条**不是禁止加第三态** —— 它是个**提问点**：加之前先答「装着的那份是哪个版本」
    /// 从哪来（`U9` 第二问，实测两份差 167 行）。答了就把这条改掉，连同上面那段头注。
    #[test]
    fn presence_still_has_exactly_two_states() {
        // 用穷举 match 钉：加了变体**编译期**就红在这里，比数字符串可靠。
        let sample = Presence::Missing {
            skill: "x".into(),
            expected: PathBuf::from("/x"),
        };
        let n = match sample {
            Presence::Found => 1,
            Presence::Missing { .. } => 2,
        };
        assert_eq!(
            n, 2,
            "`Presence` 的变体变了。**不是不许加第三态**（「版本不符」正是 `PS2` 想要的），\
             但加之前先答：装着的那份是**哪个版本**、这个量从哪来？—— 那是 `U9` 第二问，\
             今天未裁，且实测仓内那份与 `~/.claude/skills/` 那份差 167 行。\
             答了就把这条判据与 `skill_host` 头注那段一起改掉。"
        );
    }
}
