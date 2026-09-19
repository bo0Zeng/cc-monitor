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
            // 🔴 〔`K-R81` 09-12〕原文在这里**手抄了一份闭集**（「那 6 条是 …」）——
            //    两处同时馊了：`remote-daemon` 改名成了 `backend`，而条数早就不是 6。
            //    ⇒ 按〔`13b`〕只给住址、不复述成员（这一句是**用户看得见的**文案，
            //    在它里面留一个会烂的基数比不写更坏）。
            "planned-build 今天不在 tool_registry 那张表里（那张表收哪几条，\
             唯一住址是 `tool_registry::TOOLS`）。把它变成可装归 devbench F06。",
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
            root: "cc-monitor/src/shared/cc-bus",
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
///    **落点 `~/.claude/skills/cc-bus` 被只读铁律排除**（`src/doc/INVARIANTS.md` 穷举的 6 条例外
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
///    也不许碰 Claude 的 jsonl/pidfile（`src/doc/INVARIANTS.md:11` 那条只读铁律的对象）。
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
             那是 `src/doc/INVARIANTS.md` 只读铁律的对象 —— 声明表把它列进 editable 也不行。",
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
#[path = "../../../tests/bridge/skill_host_tests.rs"]
mod tests;
