//! 「足迹」里的 **Claude 布局**：申报路径里的 `~/.claude/…` 以哪个家为基准 ·
//! 用户级 settings 的两个作用域是哪两份文件。判定本体住 `footprint/rows.rs`（通用层），这里只交布局知识。

use copy_core::copy_text;
use std::path::{Path, PathBuf};

use crate::agents::footprint_decl::{
    Carrier, EnvProbe, HostScope, Provisioning, Text, ToolDestination, ToolSource, ToolSpec,
    TouchEffect, TouchedFile, UnmanagedEnv,
};

use super::assets::PROJECT_MCP_FILE;
use super::paths::HOME_DIR_NAME;

/// 申报路径（`~/` 之后那一段）若落在 agent 家底下，回家底下的那一段；否则 `None`。
pub(crate) fn under_agent_home(rest: &str) -> Option<&str> {
    rest.strip_prefix(HOME_DIR_NAME)?.strip_prefix('/')
}

/// 用户级 settings 的两个作用域（`settings.json` · `settings.local.json`），按优先级从低到高。
pub(crate) fn user_settings_files(agent_home: &Path) -> [PathBuf; 2] {
    [
        agent_home.join("settings.json"),
        agent_home.join("settings.local.json"),
    ]
}

/// 项目级 settings 的两个作用域（`<项目>/.claude/settings.json` · `settings.local.json`），按优先级从低到高；都压过用户级那两份。
pub(crate) fn project_settings_files(project: &Path) -> [PathBuf; 2] {
    user_settings_files(&project.join(HOME_DIR_NAME))
}

// ═══ 足迹申报表（`footprint/registry.rs`）里落在 Claude 布局里的那几条 —— 字面量只住这里 ═══

/// cc-bus 装成 skill 的那个目录（`LocalHomeRelative` 那一格：家目录相对，不带 `~/`）。
pub(crate) const CC_BUS_SKILL_DIR_REL: &str = ".claude/skills/cc-bus";
/// 同上，申报路径那一格（带 `~/`）。
pub(crate) const CC_BUS_SKILL_DIR: &str = "~/.claude/skills/cc-bus";
/// 用户级 settings（cc-bus 钩子那一段住这里）。
pub(crate) const USER_SETTINGS: &str = "~/.claude/settings.json";
/// 多账号那一族的账号库根：住后端的家里（那一段字面量只在契约 crate，这里拼出申报表要的两形）。
pub(crate) const ACCOUNTS_ROOT: &str = concat!("~/", relay_route_core::accounts_dir_rel!(), "/");
pub(crate) const ACCOUNTS_ROOT_REL: &str = concat!(relay_route_core::accounts_dir_rel!(), "/");
/// skill 装到的那一层（家目录相对）。
pub(crate) const SKILLS_DIR_REL: &str = ".claude/skills";
/// 同上，申报路径那一格。
pub(crate) const SKILLS_DIR: &str = "~/.claude/skills";
/// Claude Code 自己写的会话记录树。
pub(crate) const PROJECTS_DIR: &str = "~/.claude/projects/";

/// 足迹申报表里**这一家**的那一半（从 `footprint/registry.rs::TOOLS` 搬来，逐字未改）：
/// 落点在这一家的布局里的受管工具。经注册表（`agents::Adapter::footprint`）汇进 `footprint::registry::tools`。
pub(crate) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        id: "cc-bus",
        display_name: Text(|| copy_text("rsToolRegistry.tools.ccBusName", &[])),
        // ★★ **不是「实现一下就能翻 true」——落点被只读铁律排除**〔PS1 重摸底 08-12〕。
        //
        // 原注释只写「部署尚未实现（B01 只做了"搬进仓固化为基线"）」，那是**浅一层**的理由，
        // 会让下一个人以为补个递归拷贝就行。真实的墙在 `src/doc/INVARIANTS.md` 开头：
        // 「`monitor` 对 `<claude_dir>/…` **只读**」，后附**穷举**的 6 条例外 ——
        // **没有一条覆盖「往 `~/.claude/skills/` 装东西」**，而第 2 条逐字写着
        // 「只写 cc-monitor 自己的 bin 目录，**绝不碰** `~/.claude/`」。
        //
        // ⇒ 把 cc-bus 装到上面那个 `destination`，需要给那条铁律**开第 7 条豁免** ——
        // 那是**裁定**，不是实现工作。要开的话，配套应照既有 6 条的形状：
        // 用户**显式**动作 + 独立 realpath 白名单 + 幂等 + 可撤销。
        //
        // ★★ **开**。⇒ `installable` 从 `false` 翻成 `true`，
        // 实现在 `cc_bus_deploy.rs`，那四个配套**逐条落地**（模块头注里四个 `★` 一一对应），
        // 例外本身写成 `src/doc/INVARIANTS.md` 的第 7 条。
        // ⚠ `uninstallable` **仍是 `false`** —— 卸载没做，如实声明（不因为「装做了」就顺手标 true）。
        //
        // ⚠ 这堵墙今天已经在收账：`P4b` 改的是仓内那份脚本，而 `~/.local/bin/cc-*`
        // 指向的是 `~/.claude/skills/cc-bus/`（实测两份差 167 行）—— **改动到不了本机**。
        //
        // 🔴 实现是 `cc_bus_install·rs::install_at`；`config_surface` 的「能否装/撤」列**正是读这个字段的**。
        // 守这一格的 `cc_bus_says_why_it_is_not_installable_at_the_real_depth` 是**必需词守卫** ——
        // 它数注释里两个词的出现次数，**看不见字段的值**。
        // ⇒ 字段的值由覆盖全表的一条性质判：
        // `every_tool_declares_install_and_uninstall_as_the_implementations_really_are`
        // （专名判据只把静默从 1 个工具挪走，下一个工具照样静默）。
        installable: true,
        uninstallable: false,
        // 一个载体（仓内目录整份铺过去）—— `cc-bus` 不是「同一份字节两处使用」那一族，
        // 这里一条 `Carrier` 就够；载体这一维只在真有几份的那几条上才多。
        carriers: &[Carrier {
            what: Text(|| copy_text("rsToolRegistry.tools.ccBusWhat", &[])),
            source: ToolSource::RepoDir {
                repo_path: "src/shared/cc-bus",
            },
            destination: ToolDestination::LocalHomeRelative(CC_BUS_SKILL_DIR_REL),
            touches: &[
                TouchedFile {
                    // 🔴 `installable_tools_declare_where_they_land` 要求「装得了就必须申报装到哪」：
                    // 部署真正写的那个目录（`cc_bus_install·rs::install_at` 往 `<claude_dir>/skills/cc-bus/`
                    // 铺内嵌那几个文件）就是这一条。字段写成 `false` 的话这条判据**跳过** cc-bus，
                    // 这个目录在这一页上一行都没有 ⇒ 一处假申报盖住的不止它自己那一格。
                    path: CC_BUS_SKILL_DIR,
                    host: HostScope::Either,
                    note: Some(Text(|| copy_text("rsToolRegistry.tools.ccBusDirNote", &[]))),
                    effect: TouchEffect::OwnedFile,
                },
                TouchedFile {
                    path: USER_SETTINGS,
                    host: HostScope::Either,
                    note: Some(Text(|| {
                        copy_text("rsToolRegistry.tools.ccBusHooksNote", &[])
                    })),
                    effect: TouchEffect::GenerateOnly,
                },
                TouchedFile {
                    path: "~/.local/bin/cc-*",
                    host: HostScope::Either,
                    note: Some(Text(|| copy_text("rsToolRegistry.tools.ccBusBinNote", &[]))),
                    effect: TouchEffect::ReadOnly,
                },
            ],
        }],
    },
    // 账号库：这台后端自己建、自己改（`accounts/manage/`，界面「启用多账号」「新建账号」那几步）。
    //   建库时把这台现在登录的身份文件从 `~/.claude`（与 `~/.claude.json`）搬进默认号的目录，之后每个号的共享项都链回 `~/.claude`。
    ToolSpec {
        id: "accounts",
        display_name: Text(|| copy_text("rsToolRegistry.tools.accountsName", &[])),
        installable: true,
        uninstallable: false,
        carriers: &[Carrier {
            what: Text(|| copy_text("rsToolRegistry.tools.accountsWhat", &[])),
            source: ToolSource::Generated,
            destination: ToolDestination::LocalHomeRelative(ACCOUNTS_ROOT_REL),
            touches: &[
                TouchedFile {
                    path: ACCOUNTS_ROOT,
                    host: HostScope::Either,
                    note: Some(Text(|| {
                        copy_text("rsToolRegistry.tools.accountsVaultNote", &[])
                    })),
                    effect: TouchEffect::OwnedFile,
                },
                // 各号共用的用户级 MCP：共享集合 ＋ 上次同步时各号的样子（这台后端自己的状态，里面有 MCP 的密钥 ⇒ 0600）。
                TouchedFile {
                    path: "~/.cc-monitor/accounts-mcp.json",
                    host: HostScope::Either,
                    note: Some(Text(|| {
                        copy_text("rsToolRegistry.tools.accountsMcpStoreNote", &[])
                    })),
                    effect: TouchEffect::OwnedFile,
                },
                // 额度账：各号最近一次从回包头看到的用量（中转那一路记，这台后端自己的状态）。
                TouchedFile {
                    path: "~/.cc-monitor/quota.json",
                    host: HostScope::Either,
                    note: Some(Text(|| copy_text("rsToolRegistry.tools.quotaNote", &[]))),
                    effect: TouchEffect::OwnedFile,
                },
                // 账号轮换：每个会话在用的号与载入的策略 · 拖出来的先后 · 换号记录 · 新会话默认用哪份。
                TouchedFile {
                    path: "~/.cc-monitor/rotation.json",
                    host: HostScope::Either,
                    note: Some(Text(|| copy_text("rsDataPaths.backend.rotation", &[]))),
                    effect: TouchEffect::OwnedFile,
                },
                // 起会话用的号：每条会话上次用哪个号起的（观测侧记）＋ ccm 起会话时留的便条（认完、进程不在了即清）。
                TouchedFile {
                    path: "~/.cc-monitor/launch-accounts.json",
                    host: HostScope::Either,
                    note: Some(Text(|| {
                        copy_text("rsDataPaths.backend.launchAccounts", &[])
                    })),
                    effect: TouchEffect::OwnedFile,
                },
                TouchedFile {
                    path: "~/.cc-monitor/launch-pending",
                    host: HostScope::Either,
                    note: Some(Text(|| copy_text("rsDataPaths.backend.launchNotes", &[]))),
                    effect: TouchEffect::OwnedFile,
                },
                // 换了启动器起会话时交给 claude 的答题插件（清单 ＋ 答题那一组 hook）。
                TouchedFile {
                    path: "~/.cc-monitor/answer-plugin",
                    host: HostScope::Either,
                    note: Some(Text(|| {
                        copy_text("rsToolRegistry.tools.answerPluginNote", &[])
                    })),
                    effect: TouchEffect::OwnedFile,
                },
                // 同步改写某个号的配置文件之前，那份原文放这里（每个号一份）。
                TouchedFile {
                    path: "~/.cc-monitor/backups/accounts-mcp",
                    host: HostScope::Either,
                    note: Some(Text(|| {
                        copy_text("rsToolRegistry.tools.accountsMcpBackupNote", &[])
                    })),
                    effect: TouchEffect::OwnedFile,
                },
            ],
        }],
    },
    // **skill「装到这台」**：资产目录里别的机器有的 skill，用户点了才装到这台 ——
    //   文件原样从来源那台拷来，判、写、记都在这台后端（`skill_flow.rs::answer_install`）。
    // 每个写点都要在足迹里可见。落点由用户点的那一条决定（这台 skills 下以那个名字为名的目录）⇒ 占位符，不猜。
    // `uninstallable: true`：只删装时写进去的文件 —— 装的时候那台后端记下写了哪几个
    //   （第二条 touch：那台后端自己的装记录），卸口 `skill_flow.rs::answer_uninstall` 只删记着的那几个（装完改过的先问）。
    ToolSpec {
        id: "skill-install",
        display_name: Text(|| copy_text("rsToolRegistry.tools.skillInstallName", &[])),
        installable: true,
        uninstallable: true,
        carriers: &[
            Carrier {
                what: Text(|| copy_text("rsToolRegistry.tools.skillInstallWhat", &[])),
                source: ToolSource::Generated,
                // 落在 skills 目录下（以那个 skill 为名的那一个子目录；名字由你点的那一条定）—— 与 cc-bus 那一格同一个根。
                destination: ToolDestination::LocalHomeRelative(SKILLS_DIR_REL),
                touches: &[
                    TouchedFile {
                        path: SKILLS_DIR,
                        host: HostScope::Either,
                        note: Some(Text(|| {
                            copy_text("rsToolRegistry.tools.skillInstallNote", &[])
                        })),
                        effect: TouchEffect::OwnedFile,
                    },
                    TouchedFile {
                        path: "~/.cc-monitor/skill-installs.json",
                        host: HostScope::Either,
                        note: Some(Text(|| {
                            copy_text("rsToolRegistry.tools.skillInstallsLedgerNote", &[])
                        })),
                        effect: TouchEffect::OwnedFile,
                    },
                    // 卸掉不是 cc-monitor 装的 skill / MCP 之前，原样挪 / 抄到这里（要找回就从这里拿）。
                    TouchedFile {
                        path: "~/.cc-monitor/backups",
                        host: HostScope::Either,
                        note: Some(Text(|| {
                            copy_text("rsToolRegistry.tools.extBackupsNote", &[])
                        })),
                        effect: TouchEffect::OwnedFile,
                    },
                ],
            },
            // 装到某个项目里的那一份：`<项目>/.claude/skills/<名>/`（项目由你在确认卡上选）⇒ 项目相对，同 `.mcp.json` 那一格。
            Carrier {
                what: Text(|| copy_text("rsToolRegistry.tools.projectSkillInstallWhat", &[])),
                source: ToolSource::Generated,
                destination: ToolDestination::ProjectRelative(".claude/skills"),
                touches: &[TouchedFile {
                    path: ".claude/skills",
                    host: HostScope::ProjectDir,
                    note: Some(Text(|| {
                        copy_text("rsToolRegistry.tools.projectSkillsNote", &[])
                    })),
                    effect: TouchEffect::OwnedFile,
                }],
            },
        ],
    },
    // ═══ **这一条我们装不了，而它是这张表最吃重的一项。** ═══
    //
    // `<claude_dir>/projects/*.jsonl` 是**历史面与搜索索引的全部输入**。app 是 Claude Code 的监视器，
    // 被监视对象自己写的那份记录不在表里，这一页就答不了「你要的东西齐了没有」。
    //
    // 🔴 **这一条也让 `installable` 有区分力**：没有它，`installable` 在别的几条上全为真 ⇒
    // `every_declared_field_has_at_least_two_instantiations` **对地**报「没有区分力」。
    // 删掉字段的话，「cc-bus 装不装得了」就再没有任何字段可以申报、也没有任何判据读得到。
    // ⇒ 这张表收**app 会去碰 / 去读、但装不了**的东西，`installable` 分得开两类。
    // ⚠ 代价如实记：这张表的语义是「app 会碰的环境项」，不只是「装到别处的受管工具」。
    //   `NOT_MANAGED`（刻意不收的）语义不变。
    ToolSpec {
        id: "claude-code",
        display_name: Text(|| copy_text("rsToolRegistry.tools.sessionRecordsName", &[])),
        installable: false,
        uninstallable: false,
        carriers: &[Carrier {
            what: Text(|| copy_text("rsToolRegistry.tools.sessionRecordsWhat", &[])),
            source: ToolSource::NotOurs {
                who: Text(|| copy_text("rsToolRegistry.tools.sessionRecordsWho", &[])),
            },
            destination: ToolDestination::NotInstalledByUs {
                whose: Text(|| copy_text("rsToolRegistry.tools.sessionRecordsWhose", &[])),
            },
            touches: &[TouchedFile {
                path: PROJECTS_DIR,
                host: HostScope::Either,
                note: Some(Text(|| {
                    copy_text("rsToolRegistry.tools.sessionRecordsNote", &[])
                })),
                effect: TouchEffect::ReadOnly,
            }],
        }],
    },
    ToolSpec {
        id: "project-mcp",
        display_name: Text(|| copy_text("rsToolRegistry.tools.projectMcpName", &[])),
        installable: true,
        uninstallable: true,
        carriers: &[
            // **推 / 拉**（B）：这份文件唯一的写入来源 —— 条目是从另一台机器那份里**原样**拷来的（`mcp_sync.rs`），
            //   写经那台后端 `Door::put`；卸经 `ext-uninstall-apply`。远端那台的足迹栏按那台机器问（RM1a），`host` 是项目目录。
            Carrier {
                what: Text(|| copy_text("rsToolRegistry.tools.mcpSyncWhat", &[])),
                source: ToolSource::Generated,
                destination: ToolDestination::ProjectRelative(PROJECT_MCP_FILE),
                touches: &[TouchedFile {
                    path: PROJECT_MCP_FILE,
                    host: HostScope::ProjectDir,
                    // 资产目录那一块的「装到这台」（MCP）走的就是这一格（同一条命令 `mcp_sync_apply`，只勾那一条）。
                    note: Some(Text(|| copy_text("rsToolRegistry.tools.mcpSyncNote", &[]))),
                    effect: TouchEffect::OwnedFile,
                }],
            },
        ],
    },
];

/// 同上，环境清单手写那一半里这一家的那一条。
pub(crate) const UNMANAGED_ENV: &[UnmanagedEnv] = &[UnmanagedEnv {
    id: "claude-cli",
    display_name: Text(|| copy_text("rsToolRegistry.env.agentCliName", &[])),
    who: Provisioning::UserProvides,
    probe: EnvProbe::InSessionShell,
    named: "claude",
    host: HostScope::Either,
    why: Text(|| copy_text("rsToolRegistry.env.agentCliWhy", &[])),
    site: "agents/claudecode/resume.rs::DEFAULT_COMMAND",
}];

/// 注册表里这一家的足迹面。
pub(crate) const FACE: crate::agents::FootprintFace = crate::agents::FootprintFace {
    tools: TOOLS,
    env: UNMANAGED_ENV,
    under_agent_home,
    user_settings: user_settings_files,
};
