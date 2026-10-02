//! 「足迹」里的 **Claude 布局**：申报路径里的 `~/.claude/…` 以哪个家为基准 ·
//! 用户级 settings 的两个作用域是哪两份文件。判定本体住 `footprint/rows.rs`（通用层），这里只交布局知识。

use copy_core::copy_text;
use std::path::{Path, PathBuf};

use crate::footprint::registry::{
    Carrier, EnvProbe, HostScope, Provisioning, Text, ToolDestination, ToolSource, ToolSpec,
    TouchEffect, TouchedFile, UnmanagedEnv,
};

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
        // ⚠ 这堵墙今天已经在收账：`P4b` 改的是仓内那份 `cc-spawn`，而 `~/.local/bin/cc-*`
        // 指向的是 `~/.claude/skills/cc-bus/`（实测两份差 167 行）—— **改动到不了本机**。
        //
        // 🔴 **09-11 `K-R60`：上面那句「翻成 true」到今天才真的落到字段上。**
        // `K-R57` 摸底逮到：字段一直是 `false`，而同一个注释块逐字写着要翻成 `true`，
        // 实现（今天是 `cc_bus_install·rs::install_at`）**一直在**。⇒ 那张表对「cc-bus 装不装得了」的申报
        // 假了一个月，而 `config_surface` 的「能否装/撤」列**正是读这个字段的**。
        // 🔴 **为什么一个月没红**：守这一格的 `cc_bus_says_why_it_is_not_installable_at_the_real_depth`
        // 是**必需词守卫** —— 它数注释里两个词的出现次数，**看不见字段的值**。
        // ⇒ `K-R60` 那一轮补了一条真读字段值的判据：左边读这个字段、右边钉
        // `cc_bus_deploy.rs` 的函数签名，两边必须相等。
        // 🔴 **09-11 `K-R63`：那一条是专名的（名字里带工具名），本轮把它收成了覆盖全表的
        // 一条性质** —— `every_tool_declares_install_and_uninstall_as_the_implementations_really_are`。
        // 理由：专名判据只把静默从 1 个工具挪走，下一个工具照样静默（件文件 `§0c`）。
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
                    // 🔴 **这一条是 `K-R60` 补的，而它是被上面那次翻字段逼出来的。**
                    // `installable_tools_declare_where_they_land` 要求「装得了就必须申报装到哪」；
                    // 字段一直是 `false` ⇒ 这条判据一直**跳过** cc-bus ⇒ 部署真正写的那个目录
                    // （`cc_bus_install·rs::install_at` 往 `<claude_dir>/skills/cc-bus/` 铺内嵌那几个文件）
                    // **在这一页上一行都没有**。翻成 `true` 的当场它就红了。
                    // ⇒ 一处假申报盖住的不止它自己那一格。
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
                TouchedFile {
                    path: "~/.cc-bus/",
                    // ★★ **P4c 订正（08-12）：这段理由整个过期了，而过期的是 `P4a` 那一刀造成的。**
                    //
                    // 原文（T04 审计阻塞 2）写「`cc_bus.rs` 的全部 5 个 IPC……都以 `origin` 入参走
                    // `cfg_of` → ssh 远端 exec，**一条本机读取路径都没有**；驾驶舱的 origin 下拉
                    // 来自 `list_remote_mcp_origins`，连"本机"这一档都没有」。
                    //
                    // 两句今天都不成立：`P4a`（08-12）把**读面三条**做成了本机可用
                    // （同一条命令串，只是不包进 ssh），并给驾驶舱的下拉**加了「本机」那一档**。
                    //
                    // ⇒ 改 `Either`。原文担心的那个「用新的假阳性换掉旧的假阴性」今天不成立了：
                    // 本机确实会被读（`P4a`），所以说「本机存在」不再是冒充。
                    // ⚠ 但 `IndirectWrite` 那句仍要留神：**写**面（`cc_bus_send`/`_spawn`/〔散文墓碑〕
                    // `_broadcast`/`_kill`）至今**只动远端**（`refuse_local_write`），
                    // 这半句早已不成立（P4f / BS1b 起写面本机也走后端，起由界面经通道直接说，本机与远端同一条路）；
                    // 所以 note 里把「读」与「写」分开说，别让人以为本机那个也会被写。
                    host: HostScope::Either,
                    note: Some(Text(|| {
                        copy_text("rsToolRegistry.tools.ccBusStateNote", &[])
                    })),
                    effect: TouchEffect::IndirectWrite,
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
    //   〔墓碑 —— AS2 那一版这里是 `uninstallable: false`（「没有卸掉装来的 skill 这条口，如实声明」）。〕
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
    // ═══ 〔`K-R60` 09-11 加〕**这一条我们装不了，而它是这张表最吃重的一项。** ═══
    //
    // 来历：`K-R57` 摸底现打 —— 这一页「根本看不见」的 9 项里，`<claude_dir>/projects/*.jsonl`
    // 是**历史面与搜索索引的全部输入**。app 是 Claude Code 的监视器，
    // 被监视对象自己写的那份记录不在表里，这一页就答不了「你要的东西齐了没有」。
    //
    // 🔴 **这一条同时把 `installable` 的区分力带了回来，理由如实写在这里，别让它看起来很巧**：
    // cc-bus 翻成 `true` 之后，`installable` 在**全部 6 条**上都为真 ⇒
    // `every_declared_field_has_at_least_two_instantiations` **对地**报「没有区分力」
    // （`K-R60` 实打过那一趟红）。按本仓准则那时该**删字段**，
    // 而删掉它，「cc-bus 装不装得了」就再没有任何字段可以申报、也没有任何判据读得到 ——
    // 那恰好是本件要治的病的反面。
    // ⇒ 处置不是删字段，是**让这张表收进本来就该收的那一档**：
    //   app 会去碰 / 去读、但**装不了**的东西。`installable` 于是重新分得开两类人。
    // ⚠ 代价如实记：这张表的语义从「装到别处的受管工具」扩到了「app 会碰的环境项」，
    //   模块头注那句话本轮已改。`NOT_MANAGED`（刻意不收的）语义不变。
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
];

/// 同上，环境清单手写那一半里这一家的那一条。
pub(crate) const UNMANAGED_ENV: &[UnmanagedEnv] = &[UnmanagedEnv {
    id: "claude-cli",
    display_name: Text(|| copy_text("rsToolRegistry.env.agentCliName", &[])),
    who: Provisioning::UserProvides,
    probe: EnvProbe::OnPath,
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
