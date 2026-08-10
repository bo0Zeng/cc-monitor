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

// ⚠ **处置条件（照 `tool_registry` 的先例）**：本模块今天**零生产消费者** ——
// 声明表与三个纯函数都齐了，但消费它们的 UI/IPC 归 **F03**。
// ⇒ F03 接上之后**删掉这行 `allow`**；若 F03 收工时它仍然零消费者，
// **就该删掉整个模块**，而不是让它留成装饰。判据钉不住「有没有人用」，所以写在这里。
#![allow(dead_code)]

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
    /// ③ 可编辑白名单：**相对每个实例目录**的文件名。写面围栏的唯一来源。
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

/// ③ 这个实例里**允许编辑**的路径集合。
///
/// ⚠ 返回的是**集合**，不是「判断一个路径行不行」的谓词 —— 因为集合可以被判据整体检查，
/// 而谓词只能被逐例试探。写面围栏（F03）要拿这个集合做 `contains` 判定，
/// 且必须在**路径解析之后**判（符号链接与 `..` 都要先解析掉）。
///
/// ⚠ **本函数算得对 ≠ 没人绕过它** —— 强制「写必须过它」归 F03。
pub fn editable_paths(spec: &SkillSpec, instance: &Instance) -> Vec<PathBuf> {
    spec.editable.iter().map(|f| instance.dir.join(f)).collect()
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
        let inst = Instance {
            name: "w".into(),
            dir: PathBuf::from("/tmp/x/w"),
        };
        for spec in SKILLS {
            let got = editable_paths(spec, &inst);
            assert_eq!(
                got.len(),
                spec.editable.len(),
                "`{}` 的可编辑集合大小与声明不符 —— 宿主凭空加了或漏了路径",
                spec.id
            );
            for (p, f) in got.iter().zip(spec.editable) {
                assert_eq!(
                    p,
                    &inst.dir.join(f),
                    "`{}` 的可编辑路径不是「实例目录 + 声明里的文件名」",
                    spec.id
                );
            }
        }
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
