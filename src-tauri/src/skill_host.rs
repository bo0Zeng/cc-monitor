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
    /// 归 `tool_registry::TOOLS` 里这个 id（同一张表的两个视图，不是两份数据）。
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
    /// （同 panorama 的 `RepoInfoGetter`），测试里用相对 `CARGO_MANIFEST_DIR` 的推导。
    fn workspace_cwd() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")) // …/cc-monitor/src-tauri
            .parent() // …/cc-monitor
            .and_then(|p| p.parent()) // 工作目录
            .expect("推不出工作目录 —— 目录层级变了，本条会零命中地绿")
            .to_path_buf()
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
    /// ⚠ 它对文件系统有依赖 —— 这是**刻意的**，也是它唯一有价值的原因。
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
            for p in editable_paths(spec, &cwd) {
                assert!(
                    p.canonicalize().is_ok(),
                    "`{}` 声明的可编辑文件算出来是 {}，但它不存在。\n\
                     ⇒ 要么 `artifacts.root` 错了，要么 `editable` 的基准理解错了。\n\
                     ★ F02 收工时正是这个错：基准写成「实例目录」，而收件箱是**项目级**的\n\
                     （planned-build 明写「住计划目录根，不住工作区」）。\n\
                     ⚠ 若本条在别人机器上红，先确认工作目录布局：`artifacts.root` 相对的是\n\
                     **工作目录**（`cc-monitor/` 的上一级），不是仓根。",
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

        // ① 白名单内的真实文件：放行。
        let ok_path = &editable_paths(spec, &cwd)[0];
        assert!(
            resolve_editable(spec, &cwd, ok_path).is_ok(),
            "白名单里的真实文件被拒了 —— 围栏把该放的也拦了"
        );

        // ② 同目录下**没在白名单里**的文件：拒。
        //    用 `STATUS.md` 之类肯定存在、但不在 editable 里的东西才说明问题
        //    （拿一个不存在的文件去试，拦住的是「不存在」而不是「不在白名单」）。
        let root = cwd.join(spec.artifacts.root);
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
}
