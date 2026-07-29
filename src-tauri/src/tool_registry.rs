//! T01 第 5 步：**受管工具的声明**（`ToolSpec`）。
//!
//! ## 这个结构里为什么**没有**「探测机制」字段
//!
//! 计划 §2 的 DoD 写着「`ToolSpec` 声明五个正交关注点：源 / 落点 / 探测 / 装升卸 /
//! 配置面申报」，并要求「每一项都必须能被现有五套工具中的**至少两套**实例化」。
//!
//! **先更正本文件原先写错的一处事实**（T01 审计 Q3）。原文说 `cc-acct-iso` 的探测是
//! 「比对内容指纹」——不对。`acct_iso_deploy.rs::check_remote_acct_iso` 实际跑的是远端
//! `PATH="$HOME/.local/bin:$PATH" command -v cc-acct-iso` 再解析 stdout，
//! 与 `ccm_probe.rs` **属于同一族**（跑一条命令、解析 stdout）。`.vendor_id` 指纹比对
//! 发生在**部署决策**那一步（`deploy_decision` 读远端 marker 文件），不是探测。
//! 所以原先那句「四种机制彼此不兼容，且**各只有一个使用者**」是**错的**：
//! 「跑命令解析 stdout」这一族至少两个使用者，按 ≥2 判据它反而**够格**。
//! 结论（探测机制不进 `ToolSpec`）仍然成立，但**理由必须换**。
//!
//! 真实理由更硬：**`ToolSpec` 是 `const` 声明式数据，而探测是行为。**
//! `ToolSource::Vendored { repo_path, fingerprint_file }` 是数据——两个字符串，
//! 谁读它都不需要任何能力。一个探测机制不是：它要么需要一条活的 ssh 会话
//! （`ccm` / `cc-acct-iso`），要么需要一次协议握手（remote daemon 的 `hello` 帧），
//! 要么需要读本机文件系统（PowerShell profile 扫围栏）。把这些塞进 `const`
//! 只能塞成「一段命令模板 + 一个解析规则」的小 DSL，那就是把四件不相干的事
//! 装进一个盒子（本工作区反复拒绝的"上帝结构"）。
//!
//! 编译器只帮一半：`Box<dyn Fn>` 在 `const` 里根本构造不出来，但**函数指针是
//! `const`-可构造的**——`probe: fn(&Session) -> ProbeStatus` 能编译通过。
//! 所以这条边界靠测试守：见 `tool_spec_is_declarative_data_not_behavior`。
//! 它的**已知上限**如实写在这里：若有人声明一个 const-可构造的「命令模板」枚举，
//! 守卫拦不住，因为那时它确实是声明式数据——届时得就事论事重新论证，而不是引用本段。
//!
//! ## 原先这里有个 `ProbeStatus`，本轮**删掉了**
//!
//! 它是 `{ installed: bool, version: String, capabilities: Vec<String> }`，
//! 与既有的 `ccm_probe::CcmProbeResult` **同形**、零适配、零生产消费者，
//! 而且 `version: String` 比对方的 `Option<String>` 还丢了「取不到」这一档。
//! 我原先把它当作「机制留各家、结果统一」的落点——**发明第二个同形结构不是统一，
//! 是重复**。按我自己这一轮的尺子（`build_online_cmd` 零调用点被我判为**阻塞**、
//! `WriteVerdict::is_ok` 只有测试在用就删掉），它该删。统一的结果类型**已经存在**，
//! 就是 `CcmProbeResult`；T02 真要消费探测结果时直接用它（不够就给它加字段），
//! 而不是在注册表里再造一个。
//!
//! ## 如实登记：本模块目前**零生产消费者**（T01 审计 I2）
//!
//! `TOOLS` 现在只有本文件的测试在读。同一轮我以「只有测试在用」为由删了
//! `WriteVerdict::is_ok`——尺子确实不一致，这里不拿「T02 会用它」自动豁免。
//! **T02 是紧接着的下一个功能；若 T02 收工时 `TOOLS` 仍无生产消费者，就该删掉本模块**，
//! 而不是留着当纸面资产。提醒不靠我记得：`cargo clippy` 现在会对本模块的 6 个类型
//! 各报一条 `never used`——**那 6 条警告就是这笔债的存根**，T02 接上之后它们会自己消失。
//! （对比：`structural_scan` 的消费者全在 `#[cfg(test)]` 里，它是测试支撑模块，
//! 已在 `lib.rs` 标 `#[cfg(test)]`，不占这笔债。）
//!
//! ## 字段纪律
//!
//! 下面每个字段都必须**至少被两个工具实质实例化**，只有一套需要的东西不进这里。
//! 门禁是 `every_declared_field_has_at_least_two_instantiations`——它**从源码枚举
//! `ToolSpec` 声明的字段**再逐个数 `TOOLS` 里的实质取值，不是对现有字段的硬编码断言。
//! 上一版就是硬编码的，审计塞一个中性命名的单实例化字段 `needs_elevation` 进去，
//! **21 项全绿**；那一版还是个固定 needle，而同一次提交里 `structural_scan.rs`
//! 的文档正在痛批固定 needle。现在审计那条手法被钉成了常驻测试
//! （`the_scan_catches_the_audits_own_single_use_field`，直接变异**真文件**）。

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
///
/// **所有字段必须是 `const`-可构造的声明式数据**（无函数指针、无 `dyn`、无 `String`）。
/// 这不是风格偏好，是上面那条「探测机制不进来」边界的落地形式。
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
    use crate::structural_scan::ScanReport;
    use std::collections::HashSet;

    // ===== 从源码枚举结构（要件 1），而不是硬编码现有字段 =====
    //
    // 前置条件（与 structural_scan 的 comment_prefix 同类，如实写明）：本模块的字符串
    // 字面量里**不含 `//`、也不含花括号/方括号**，否则朴素的注释剥离与括号配对会算错。
    // 这条由下面 parser_actually_sees_the_real_source 的反向自检兜底：真算错了，
    // 字段集合就对不上，测试会红在那里而不是静默放过。

    /// 剥掉 `//` 行注释与整个 `#[cfg(test)]` 段，只留生产代码文本。
    fn production_code(src: &str) -> String {
        let code = src.split(concat!("#[cfg", "(test)]")).next().unwrap_or(src);
        code.lines()
            .map(|l| match l.find("//") {
                Some(i) => &l[..i],
                None => l,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 从 `from` 起找 `opener`，返回它**配对括号内**那段在 `text` 里的下标区间。
    /// 括号种类取 `opener` 的最后一个字符（`{` / `[` / `(`）。
    fn matched_span(text: &str, opener: &str, from: usize) -> Option<(usize, usize)> {
        let p = text[from..].find(opener)? + from;
        let open = opener.trim_end().chars().last()?;
        let close = match open {
            '{' => '}',
            '[' => ']',
            '(' => ')',
            _ => return None,
        };
        let start = p + opener.len();
        let mut depth = 1i32;
        for (i, c) in text[start..].char_indices() {
            if c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    return Some((start, start + i));
                }
            }
        }
        None
    }

    /// `ToolSpec` 声明的字段：`(名, 类型)`，**按源码里实际写的枚举**。
    fn declared_fields(code: &str) -> Vec<(String, String)> {
        let (a, b) = matched_span(code, "pub struct ToolSpec {", 0)
            .expect("取不到 ToolSpec 的声明体——扫描器失效了");
        code[a..b]
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .filter_map(|l| {
                let l = l.strip_prefix("pub ").unwrap_or(l);
                let (name, ty) = l.split_once(':')?;
                let name = name.trim();
                if name.is_empty() || !name.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
                    return None;
                }
                Some((
                    name.to_string(),
                    ty.trim().trim_end_matches(',').trim().to_string(),
                ))
            })
            .collect()
    }

    /// `TOOLS` 里每一个 `ToolSpec { … }` 字面量的**体**文本。
    fn tool_literals(code: &str) -> Vec<&str> {
        let (a, b) = matched_span(code, "pub const TOOLS: &[ToolSpec] = &[", 0)
            .expect("取不到 TOOLS 常量体——扫描器失效了");
        let body = &code[a..b];
        let mut out = Vec::new();
        let mut off = 0usize;
        // 配对之后从**本块结束处**继续找，嵌套的 TouchedFile 块不会被重复计入
        while let Some((s, e)) = matched_span(body, "ToolSpec {", off) {
            out.push(&body[s..e]);
            off = e;
        }
        out
    }

    /// 取字面量里 `field:` 在**顶层**（相对本字面量体）的取值文本。
    /// 嵌套块里的同名字段（如 `TouchedFile { path: … }` 的 `path`）depth>0，不会命中。
    fn field_value<'a>(lit: &'a str, field: &str) -> Option<&'a str> {
        let needle = format!("{field}:");
        let mut depth = 0i32;
        let mut prev: Option<char> = None;
        let mut start: Option<usize> = None;
        for (i, c) in lit.char_indices() {
            if start.is_none()
                && depth == 0
                && lit[i..].starts_with(&needle)
                && !prev.is_some_and(|p| p.is_alphanumeric() || p == '_')
            {
                start = Some(i + needle.len());
                prev = Some(c);
                continue;
            }
            match c {
                '{' | '[' | '(' => depth += 1,
                '}' | ']' | ')' => depth -= 1,
                ',' if depth == 0 => {
                    if let Some(s) = start {
                        return Some(&lit[s..i]);
                    }
                }
                _ => {}
            }
            prev = Some(c);
        }
        start.map(|s| &lit[s..])
    }

    /// 「实质取值」= 不是中性/空值。中性值意味着**这个工具其实不需要这个字段**，
    /// 只是被 Rust 逼着填一个。审计塞的 `needs_elevation` 正是 5 个 `false` + 1 个 `true`。
    fn is_substantive(v: Option<&str>) -> bool {
        match v {
            None => false,
            Some(v) => !matches!(
                v.trim(),
                "" | "false" | "None" | "\"\"" | "&[]" | "0" | "vec![]" | "Default::default()"
            ),
        }
    }

    /// **字段纪律扫描**：枚举声明的每个字段 → 数 `TOOLS` 里的实质取值 → <2 判违规。
    fn field_discipline(code: &str) -> ScanReport {
        let fields = declared_fields(code);
        let lits = tool_literals(code);
        let mut r = ScanReport {
            checked: 0,
            violations: Vec::new(),
        };
        if lits.len() < 2 {
            r.violations
                .push(format!("只找到 {} 个 ToolSpec 字面量", lits.len()));
            return r;
        }
        for (name, ty) in &fields {
            r.checked += 1;
            let users: Vec<&str> = lits
                .iter()
                .filter(|l| is_substantive(field_value(l, name)))
                .map(|l| field_value(l, "id").unwrap_or("?").trim())
                .collect();
            if users.len() < 2 {
                r.violations.push(format!(
                    "字段 `{name}: {ty}` 只被 {} 个工具实质实例化（{users:?}）\
                     ——只有一套需要的东西不进 ToolSpec",
                    users.len()
                ));
            }
            if ty == "bool" && users.len() == lits.len() {
                r.violations.push(format!(
                    "字段 `{name}: bool` 在全部 {} 个工具上都为真，没有区分力",
                    lits.len()
                ));
            }
        }
        r
    }

    /// **声明式数据**扫描：枚举字段类型，白名单放行；行为（函数指针/`dyn`/需分配的容器）判违规。
    fn declarative_only(code: &str) -> ScanReport {
        let mut r = ScanReport {
            checked: 0,
            violations: Vec::new(),
        };
        for (name, ty) in declared_fields(code) {
            r.checked += 1;
            let ok = ty == "&'static str"
                || ty == "bool"
                || (ty.starts_with("&'static [") && ty.ends_with(']'))
                || (ty.chars().all(|c| c.is_alphanumeric() || c == '_')
                    && (code.contains(&format!("pub enum {ty}"))
                        || code.contains(&format!("pub struct {ty}"))));
            if !ok {
                r.violations.push(format!(
                    "字段 `{name}: {ty}` 不是 const-可构造的声明式数据\
                     ——`ToolSpec` 只收数据，探测/装卸这类**行为**留在各工具自己那里"
                ));
            }
        }
        r
    }

    // ===== 反向自检（要件 3）：证明上面这套解析器真看见了真代码 =====

    #[test]
    fn parser_actually_sees_the_real_source() {
        let code = production_code(include_str!("tool_registry.rs"));
        assert!(code.contains("pub struct ToolSpec {"), "剥过头了");
        assert!(!code.contains("fn production_code"), "测试段没剥掉");
        let names: Vec<String> = declared_fields(&code).into_iter().map(|(n, _)| n).collect();
        assert_eq!(
            names,
            vec![
                "id",
                "display_name",
                "source",
                "destination",
                "installable",
                "uninstallable",
                "touches"
            ],
            "解析出的字段集合与源码不符——先查解析器，别改断言"
        );
        assert_eq!(
            tool_literals(&code).len(),
            TOOLS.len(),
            "字面量数应等于 TOOLS 长度"
        );
        // 顶层取值取对了，且不会被嵌套的同名字段污染
        let ccm = tool_literals(&code)[0];
        assert_eq!(field_value(ccm, "id").map(str::trim), Some("\"ccm\""));
        assert_eq!(field_value(ccm, "installable").map(str::trim), Some("true"));
        assert!(
            field_value(ccm, "path").is_none(),
            "`path` 只在嵌套块里，不该被顶层取到"
        );
    }

    /// **防上帝结构的门禁**（计划 §5 P2）。不是形式主义：本会话四次拒绝提前抽象
    /// （R12 registry / R15 passThrough / B02 `--bus-id` / B03 `inbox_id_from_filename`），
    /// 靠的都是"数真实消费者"。
    #[test]
    fn every_declared_field_has_at_least_two_instantiations() {
        let code = production_code(include_str!("tool_registry.rs"));
        field_discipline(&code)
            .require(5, "ToolSpec 字段纪律")
            .unwrap();
    }

    /// **审计那条手法，钉成常驻测试**：直接变异**真文件**，塞一个中性命名的单实例化
    /// 字段 `needs_elevation`。上一版硬编码断言对此**21 项全绿**。
    #[test]
    fn the_scan_catches_the_audits_own_single_use_field() {
        let code = production_code(include_str!("tool_registry.rs"));
        let mutated = code
            .replace(
                "    pub touches: &'static [TouchedFile],",
                "    pub touches: &'static [TouchedFile],\n    pub needs_elevation: bool,",
            )
            .replace(
                "        id: \"",
                "        needs_elevation: false,\n        id: \"",
            )
            .replacen(
                "        needs_elevation: false,\n        id: \"ccm\"",
                "        needs_elevation: true,\n        id: \"ccm\"",
                1,
            );
        // **先确认变异真落进去了**（本会话两次"全绿"其实是变异没写进文件）
        assert_eq!(
            mutated.matches("needs_elevation").count(),
            1 + TOOLS.len(),
            "变异没落到位：声明 1 处 + 每个字面量 1 处"
        );
        assert_eq!(mutated.matches("needs_elevation: true").count(), 1);
        let r = field_discipline(&mutated);
        assert!(
            r.violations.iter().any(|v| v.contains("needs_elevation")),
            "单实例化字段必须被抓，实得 {:?}",
            r.violations
        );
        assert!(r.require(5, "ToolSpec 字段纪律").is_err());
        // 且不能顺手把好字段也误判
        assert_eq!(
            r.violations.len(),
            1,
            "只该有一条违规，实得 {:?}",
            r.violations
        );
    }

    /// 删掉一个字段的实质取值（把 `installable: true` 全改成 `false`）也必须红
    /// ——否则这条扫描只对"新增"敏感，对"退化"是瞎的。
    #[test]
    fn the_scan_also_catches_a_field_degraded_to_neutral() {
        let code = production_code(include_str!("tool_registry.rs"));
        let mutated = code.replace("        installable: true,", "        installable: false,");
        // 自检必须带 8 空格前缀：不带的话 `uninstallable: false` 也会被数进去
        // （第一版就是这么错的，实得 9 而非 6，测试当场红在这一行——**先确认变异落位**再判色）
        assert!(!mutated.contains("        installable: true,"));
        assert_eq!(
            mutated.matches("        installable: false,").count(),
            TOOLS.len(),
            "5 处被改 + cc-bus 原本那 1 处"
        );
        let r = field_discipline(&mutated);
        assert!(
            r.violations.iter().any(|v| v.contains("installable")),
            "实得 {:?}",
            r.violations
        );
    }

    /// **探测机制不进 `ToolSpec`**，且这条守卫不是名字黑名单——上一版列的是
    /// `["probe", "detect", "check_cmd", "fingerprint_cmd"]`，换个名字就穿。
    /// 现在守的是**结构性质**：字段类型必须是 const-可构造的声明式数据。
    #[test]
    fn tool_spec_is_declarative_data_not_behavior() {
        let code = production_code(include_str!("tool_registry.rs"));
        declarative_only(&code)
            .require(5, "ToolSpec 只收声明式数据")
            .unwrap();
    }

    /// 用**改了名的**探测机制验证上一条：叫什么都拦得住，因为拦的是类型。
    #[test]
    fn a_renamed_probe_mechanism_is_still_caught() {
        for smuggled in [
            "pub how_to_look: fn(&str) -> bool,",
            "pub sniff: Box<dyn Fn(&str) -> bool>,",
            "pub tag: String,",
            "pub caps: Vec<String>,",
        ] {
            let synthetic = format!(
                "pub struct ToolSpec {{\n    pub id: &'static str,\n    {smuggled}\n}}\n\
                 pub const TOOLS: &[ToolSpec] = &[\n    ToolSpec {{ id: \"a\" }},\n];\n"
            );
            let r = declarative_only(&synthetic);
            assert_eq!(r.checked, 2, "两个字段都要进枚举：{smuggled}");
            assert_eq!(
                r.violations.len(),
                1,
                "只有 {smuggled} 该违规，实得 {:?}",
                r.violations
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

    /// `TouchEffect` 的每个变体都得有真实使用者——只有一个用户的变体同样是过度设计。
    #[test]
    fn touch_effects_are_all_really_used() {
        let effects: HashSet<_> = TOOLS
            .iter()
            .flat_map(|t| t.touches.iter().map(|f| f.effect))
            .collect();
        assert!(
            effects.len() >= 3,
            "TouchEffect 至少要有三种被真实用到，实得 {effects:?}"
        );
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
