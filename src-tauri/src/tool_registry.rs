//! T01 第 5 步：**受管工具的声明**（`ToolSpec`）。
//!
//! ## 这个结构里为什么**没有**「探测机制」字段
//!
//! 计划 §2 的 DoD 写着「`ToolSpec` 声明五个正交关注点：源 / 落点 / 探测 / 装升卸 /
//! 配置面申报」，并要求「每一项都必须能被现有五套工具中的**至少两套**实例化」。
//! 动手时逐个数了真代码，**「探测」这一项过不了这条判据**：
//!
//! | 工具 | 探测机制 |
//! |---|---|
//! | `ccm` | 跑一条命令，解析 `capabilities=` 文本（`ccm_probe.rs`） |
//! | `cc-acct-iso` | 比对内容指纹（本地 `.vendor_id` vs 远端 marker 文件） |
//! | remote daemon | **带内协议帧**（`hello` 里的 `capabilities` 字段） |
//! | PowerShell 集成 | 扫 profile 找围栏标记 |
//!
//! 四种机制**彼此不兼容，且各只有一个使用者**。做成 enum 的话就是四个变体各一个用户
//! ——那不是共享抽象，是把四件不相干的事装进一个盒子（本工作区反复拒绝的"上帝结构"）。
//!
//! **但探测的「结果」是共享的**：四者都在回答「装没装 / 什么版本 / 有哪些能力」。
//! 所以这里的切法是——**机制留在各工具自己那里，结果统一成 [`ProbeStatus`]**。
//! 注册表只**消费**结果，不规定怎么探。这条边界还有一个附带好处：
//! 账本明写「不改 `ccm` 本体」（12 条 print-parity + 39 条 ccm-cli 是外部预言机），
//! 而"只消费它已有的输出"天然满足这一条。
//!
//! ## 字段纪律
//!
//! 下面每个字段都必须**至少被两个工具实例化**，只有一套需要的东西不进这里。
//! 有一条测试（`every_field_has_at_least_two_instantiations`）把这条变成门禁——
//! 不是靠我记得。

/// 内容的来源。**6 个实例化**（五套工具 + cc-bus）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum ToolSource {
    /// 仓内文件，编译期 `include_str!` 进二进制（`ccm`）。
    EmbeddedText { repo_path: &'static str },
    /// 仓内目录，运行期读（`cc-bus` 的 `shared/cc-bus/`）。
    RepoDir { repo_path: &'static str },
    /// vendored 目录 + 指纹（`cc-acct-iso`、`code-picture-core`）。
    Vendored {
        repo_path: &'static str,
        fingerprint_file: &'static str,
    },
    /// 交叉编译后内嵌的二进制（remote daemon）。
    EmbeddedBinary { repo_path: &'static str },
    /// 由 cc-monitor 现场生成的文本片段（PowerShell profile 块、shell 别名块、钩子片段）。
    Generated,
}

/// 装到哪。**5 个实例化**。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum ToolDestination {
    /// 远端家目录下的相对路径（`~/.local/bin/ccm`）。
    RemoteHomeRelative(&'static str),
    /// 本机家目录下的相对路径（`~/.claude/skills/...`）。
    LocalHomeRelative(&'static str),
    /// 用户的 shell profile（`$PROFILE` / `~/.bashrc`）——路径由用户选。
    UserShellProfile,
    /// 项目目录内的文件（`<dir>/.mcp.json`）。
    ProjectRelative(&'static str),
}

/// 探测的**结果**（不是机制）。**4 个实例化**。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ProbeStatus {
    pub installed: bool,
    /// 版本/指纹，取不到就空串（各工具口径不同，注册表不解释它，只展示）。
    pub version: String,
    /// 能力 token，没有这个概念的工具给空 vec。
    pub capabilities: Vec<String>,
}

/// 这个工具会碰用户的哪个文件，以及**碰它意味着什么**。
/// **6 个实例化** —— 这是本结构里最扎实的一项，也是 T02 审计视图的直接输入。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TouchedFile {
    /// 展示给用户的路径（可含 `~` / `$PROFILE` 这类占位，注册表不展开）。
    pub path: &'static str,
    /// 我们对它做什么。**这决定了 T02 审计页里那一行的措辞与危险程度。**
    pub effect: TouchEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum TouchEffect {
    /// 只读（诊断用）。
    ReadOnly,
    /// 在文件里插入/更新一个**有围栏的块**，卸载时按围栏精确剥离。
    FencedBlock,
    /// 整个文件由我们拥有（部署时整体覆盖）。
    OwnedFile,
    /// **只生成待贴文本，由用户自己粘贴**——我们不写。
    /// （`~/.claude/settings.json` 的 cc-bus 钩子走这条：用户定调 + cc-bus 安装脚本
    ///  第 3 行同样拒绝改它。）
    GenerateOnly,
}

/// 一个受管工具的完整声明。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ToolSpec {
    pub id: &'static str,
    pub display_name: &'static str,
    pub source: ToolSource,
    pub destination: ToolDestination,
    /// 能不能装/升。**5 个实例化**（cc-bus 的部署尚未做 → false）。
    pub installable: bool,
    /// 能不能卸。**3 个实例化**（ccm / MCP / PowerShell 有；其余无）。
    pub uninstallable: bool,
    pub touches: &'static [TouchedFile],
}

/// 五套既有机制 + cc-bus 的声明。**本轮只声明，不改它们任何行为**
/// （MASTERPLAN §4 第 3 点：先用已知行为的工具验证抽象，再拿它吃新工具）。
pub const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        id: "ccm",
        display_name: "ccm 统一启动器",
        source: ToolSource::EmbeddedText {
            repo_path: "shared/ccm",
        },
        destination: ToolDestination::RemoteHomeRelative(".local/bin/ccm"),
        installable: true,
        uninstallable: true,
        touches: &[
            TouchedFile {
                path: "~/.local/bin/ccm",
                effect: TouchEffect::OwnedFile,
            },
            TouchedFile {
                path: "~/.bashrc（或所选 profile）",
                effect: TouchEffect::FencedBlock,
            },
        ],
    },
    ToolSpec {
        id: "cc-bus",
        display_name: "cc-bus 多实例消息总线",
        source: ToolSource::RepoDir {
            repo_path: "shared/cc-bus",
        },
        destination: ToolDestination::LocalHomeRelative(".claude/skills/cc-bus"),
        // 部署尚未实现（B01 只做了"搬进仓固化为基线"）——**如实声明 false**，
        // 不因为"计划里写了要做"就先标 true。
        installable: false,
        uninstallable: false,
        touches: &[
            TouchedFile {
                path: "~/.claude/settings.json 的 hooks 段",
                effect: TouchEffect::GenerateOnly,
            },
            TouchedFile {
                path: "~/.local/bin/cc-*（12 条软链）",
                effect: TouchEffect::OwnedFile,
            },
            TouchedFile {
                path: "~/.cc-bus/（运行期状态）",
                effect: TouchEffect::ReadOnly,
            },
        ],
    },
    ToolSpec {
        id: "cc-acct-iso",
        display_name: "cc-acct-iso 多账号隔离",
        source: ToolSource::Vendored {
            repo_path: "src-tauri/vendor/cc-acct-iso",
            fingerprint_file: ".vendor_id",
        },
        destination: ToolDestination::LocalHomeRelative(".claude/skills/cc-acct-iso"),
        installable: true,
        uninstallable: false,
        touches: &[
            TouchedFile {
                path: "~/.claude/skills/cc-acct-iso",
                effect: TouchEffect::OwnedFile,
            },
            TouchedFile {
                path: "~/.claude-accts/",
                effect: TouchEffect::ReadOnly,
            },
        ],
    },
    ToolSpec {
        id: "remote-daemon",
        display_name: "远端 daemon",
        source: ToolSource::EmbeddedBinary {
            repo_path: "embedded-daemons",
        },
        destination: ToolDestination::RemoteHomeRelative(".local/bin/ccm-daemon"),
        installable: true,
        uninstallable: false,
        touches: &[TouchedFile {
            path: "远端 ~/.local/bin/ccm-daemon",
            effect: TouchEffect::OwnedFile,
        }],
    },
    ToolSpec {
        id: "project-mcp",
        display_name: "项目 MCP 配置",
        source: ToolSource::Generated,
        destination: ToolDestination::ProjectRelative(".mcp.json"),
        installable: true,
        uninstallable: true,
        touches: &[TouchedFile {
            path: "<项目目录>/.mcp.json",
            effect: TouchEffect::OwnedFile,
        }],
    },
    ToolSpec {
        id: "powershell-profile",
        display_name: "PowerShell 集成",
        source: ToolSource::Generated,
        destination: ToolDestination::UserShellProfile,
        installable: true,
        uninstallable: true,
        touches: &[TouchedFile {
            path: "$PROFILE",
            effect: TouchEffect::FencedBlock,
        }],
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// **防上帝结构的门禁**（计划 §5 P2）。这条不是形式主义：本会话我四次拒绝过
    /// 提前抽象（R12 registry / R15 passThrough / B02 `--bus-id` / B03
    /// `inbox_id_from_filename`），靠的都是"数真实消费者"。把它变成测试，
    /// 下次有人（包括我）想往 `ToolSpec` 里塞一个只有一套需要的字段时会当场红。
    #[test]
    fn every_field_has_at_least_two_instantiations() {
        // 每个字段：统计"有多少个工具给出了实质取值"
        let source_kinds: HashSet<_> = TOOLS
            .iter()
            .map(|t| std::mem::discriminant(&t.source))
            .collect();
        assert!(
            TOOLS.len() >= 2 && source_kinds.len() >= 2,
            "source 必须被 ≥2 个工具实例化且形态不止一种"
        );

        let dest_kinds: HashSet<_> = TOOLS
            .iter()
            .map(|t| std::mem::discriminant(&t.destination))
            .collect();
        assert!(dest_kinds.len() >= 2, "destination 形态不止一种");

        assert!(
            TOOLS.iter().filter(|t| t.installable).count() >= 2,
            "installable 至少两个 true"
        );
        assert!(
            TOOLS.iter().filter(|t| !t.installable).count() >= 1,
            "installable 也要有 false 的，否则这个字段没有区分力"
        );
        assert!(
            TOOLS.iter().filter(|t| t.uninstallable).count() >= 2,
            "uninstallable 至少两个 true"
        );
        assert!(
            TOOLS.iter().filter(|t| !t.uninstallable).count() >= 2,
            "uninstallable 也要有 false 的"
        );
        assert!(
            TOOLS.iter().filter(|t| !t.touches.is_empty()).count() >= 2,
            "touches 至少两个非空"
        );
        // TouchEffect 的每个变体都得有真实使用者——只有一个用户的变体同样是过度设计
        let effects: HashSet<_> = TOOLS
            .iter()
            .flat_map(|t| t.touches.iter().map(|f| f.effect))
            .collect();
        assert!(
            effects.len() >= 3,
            "TouchEffect 至少要有三种被真实用到，实得 {effects:?}"
        );
    }

    /// **这个结构里不得出现「探测机制」**。四种机制各只有一个使用者，
    /// 装进来就是四变体各一用户的上帝结构。注册表只消费 `ProbeStatus`（结果）。
    #[test]
    fn probe_mechanism_is_not_part_of_the_spec() {
        let src = include_str!("tool_registry.rs");
        let code = src.split(concat!("#[cfg", "(test)]")).next().unwrap_or(src);
        // 反向自检：守卫真的看到了代码
        assert!(code.contains("pub struct ToolSpec"), "剥过头了");
        assert!(code.len() > 1500, "扫到的代码太少");
        // ToolSpec 的字段列表里不得出现 probe/detect 之类的机制字段
        let spec = code
            .split("pub struct ToolSpec {")
            .nth(1)
            .and_then(|x| x.split('}').next())
            .expect("取不到 ToolSpec 字段列表");
        for bad in ["probe", "detect", "check_cmd", "fingerprint_cmd"] {
            assert!(
                !spec.contains(bad),
                "ToolSpec 不该含探测机制字段 {bad:?}——四种机制各只有一个使用者"
            );
        }
    }

    #[test]
    fn ids_are_unique_and_stable() {
        let ids: HashSet<_> = TOOLS.iter().map(|t| t.id).collect();
        assert_eq!(ids.len(), TOOLS.len(), "id 必须唯一（T02 会拿它当键）");
        for t in TOOLS {
            assert!(!t.id.is_empty() && !t.display_name.is_empty());
            // id 用于持久化/UI dataset，限制字符集免得以后踩 B03 那种 `--help` 的坑
            assert!(
                t.id.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "id {:?} 只允许小写字母与连字符",
                t.id
            );
        }
    }

    /// cc-bus 的部署**还没实现**，声明必须如实为 false。
    /// 「计划里写了要做」不等于「已经能做」——注册表是给 UI 看的，标错了 UI 就会给出
    /// 一个点了没反应的按钮。
    #[test]
    fn declarations_match_reality_not_intent() {
        let ccbus = TOOLS.iter().find(|t| t.id == "cc-bus").unwrap();
        assert!(!ccbus.installable, "cc-bus 部署尚未实现，不得声明可装");
        assert!(!ccbus.uninstallable);
        // settings.json 只生成待贴文本，绝不写
        let hooks = ccbus
            .touches
            .iter()
            .find(|f| f.path.contains("settings.json"))
            .expect("cc-bus 应声明它需要 settings.json 的钩子");
        assert_eq!(
            hooks.effect,
            TouchEffect::GenerateOnly,
            "settings.json 是共享全局配置，只能生成待贴文本"
        );
    }

    /// 有围栏的块必须可卸载——否则用户没法干净地退出。
    #[test]
    fn fenced_block_implies_uninstallable() {
        for t in TOOLS {
            if t.touches
                .iter()
                .any(|f| f.effect == TouchEffect::FencedBlock)
            {
                assert!(
                    t.uninstallable,
                    "{} 往用户文件里插了围栏块，就必须能按围栏剥离",
                    t.id
                );
            }
        }
    }
}
