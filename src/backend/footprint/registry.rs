//! **受管工具的声明**（`ToolSpec`）。
//!
//! # ⚠ 它守什么、**不守什么**
//!
//! 本模块的判据**多数是声明表内部的自洽检查**：字段有没有区分力 · 落点在不在
//! `touches` 里 · 拥有就必须装得了 · 有围栏就必须卸得掉 · 解析器有没有真看见源码。
//! 判据条数不写在这里（本文件判据档里的测试函数，要数就现数）；也不全是表内部自洽 ——
//! `every_tool_declares_install_and_uninstall_as_the_implementations_really_are`
//! 一边读字段值、一边去别的文件里钉那个装 / 卸实现的签名，它跨出了这张表。
//! 两条探针实测它**确实有牙**（把 `ccm` 的落点改成 `.local/bin/ccm-x` ⇒
//! `installable_tools_declare_where_they_land` 点名；把 `installable` 全改成 `true` ⇒
//! 三条同时红，含「在全部 6 个 ToolSpec 上都为真，没有区分力」）。
//!
//! **但它不守一件事：新增一个「会在用户机器上留下东西」的写点，没有任何东西逼它申报。**
//! 人群是「**声明了的**工具」，不是「**真实发生的**安装动作」——
//! 表里那几条看着是全的，而那是**人现在记得**，不是有东西钉着。
//! （**基数不写在散文里**：要数就 `TOOLS.len()`，那是唯一一份。）
//!
//! ## **这张表收的是「app 会碰的环境项」，不只是「装到别处的受管工具」**
//!
//! 只收「我们装到别处的东西」的话，app **装不了、却离不开**的那些
//! （最吃重的一项是 Claude Code 自己写的会话记录）只能靠**不进表**来表示 ——
//! 用缺席表达一个判断。⇒ `installable: false`
//! 的条目是这张表的一等公民，它们与 [`UNMANAGED_ENV`] 一起被 [`environment`]
//! 汇成**唯一一份**环境清单。
//!
//! ## 为什么今天不把它做成机检（量过才这么写）
//!
//! 试过一条口径：扫生产段里的**安装落点字面量**（`.local/bin` / `.claude/skills` /
//! `.claude/settings.json` / `.claude.json`），要求每一个都对应一条 `ToolSpec`。
//! 实测 **21 个命中里绝大多数是用户可见文案与探测命令**
//!（「没读到 ~/.claude/settings.json（文件不存在或读取失败）」这类），
//! 真正的落点只有两三个 ⇒ **噪声压过信号**，做成判据会天天误红，
//! 而误红最省事的消法是往豁免表里塞条目 —— 那会把这条判据变成废纸。
//!
//! ⇒ 登记为诚实边界，**不假装覆盖**。
//! 解锁条件：安装动作先收敛到一个可枚举的落点（比如所有部署都过一个 `install_to()`），
//! 那时人群才有干净的边界。
//!
//! ## 这个结构里为什么**没有**「探测机制」字段
//!
//! 计划 §2 的 DoD 写着「`ToolSpec` 声明五个正交关注点：源 / 落点 / 探测 / 装升卸 /
//! 配置面申报」，并要求「每一项都必须能被现有五套工具中的**至少两套**实例化」。
//!
//! **先更正本文件原先写错的一处事实**（T01 审计 Q3）。原文说当时那个账号工具的探测是
//! 「比对内容指纹」——不对。它实际跑的是一条命令再解析 stdout，
//! 与「问终端的 `PATH`」（`platform/shell/mod.rs::fresh_terminal_path`）**属于同一族**（跑一条命令、解析 stdout）。指纹比对
//! 发生在**部署决策**那一步，不是探测。
//! 所以原先那句「四种机制彼此不兼容，且**各只有一个使用者**」是**错的**：
//! 「跑命令解析 stdout」这一族至少两个使用者，按 ≥2 判据它反而**够格**。
//! 结论（探测机制不进 `ToolSpec`）仍然成立，但**理由必须换**。
//!
//! 真实理由更硬：**`ToolSpec` 是 `const` 声明式数据，而探测是行为。**
//! `ToolSource::RepoDir { repo_path }` 是数据——一个字符串，
//! 谁读它都不需要任何能力。一个探测机制不是：它要么需要一条活的 ssh 会话
//! （`ccm`），要么需要一次协议握手（remote backend 的 `hello` 帧），
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
//! ## 原先这里有个 `ProbeStatus`，已**删掉**
//!
//! ## 如实登记：本模块目前**零生产消费者**（T01 审计 I2）
//!
//! `TOOLS` 现在只有本文件的测试在读。同一轮我以「只有测试在用」为由删了
//! `WriteVerdict::is_ok`——尺子确实不一致，这里不拿「T02 会用它」自动豁免。
//! **T02 是紧接着的下一个功能；若 T02 收工时 `TOOLS` 仍无生产消费者，就该删掉本模块**，
//! 而不是留着当纸面资产。提醒不靠我记得：`cargo clippy` 现在会对本模块的 6 个类型
//! 各报一条 `never used`——**那 6 条警告就是这笔债的存根**，T02 接上之后它们会自己消失。
//! （对比：`structural_scan` 的消费者全在测试段里，它是测试支撑模块，
//! 已在 monitor 的 `lib.rs` 整个挂在测试属性下，不占这笔债。）
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

use crate::agents::footprint_decl::{
    Carrier, EnvProbe, HostScope, Provisioning, ShellProfile, Text, ToolDestination, ToolSource,
    ToolSpec, TouchEffect, TouchedFile, UnmanagedEnv, USER_PATH_TOKEN,
};
use copy_core::copy_text;

/// 五套既有机制 + cc-bus 的声明。**这里只声明，不改它们任何行为**
/// （第 3 点：先用已知行为的工具验证抽象，再拿它吃新工具）。
pub(crate) const TOOLS: &[ToolSpec] = &[
    // 落在 Claude 布局里的那几条（`cc-bus` · `accounts` · `skill-install` · `claude-code`）住适配层
    //   `agents/claudecode/footprint.rs::TOOLS`，经注册表（`agents::Adapter::footprint`）汇进 [`tools`]。
    ToolSpec {
        id: "ccm",
        display_name: Text(|| copy_text("rsToolRegistry.tools.ccmName", &[])),
        // 🔴 `repo_path` 指那个**入口**的来源：`local_backend::ccm_entry_shim` 现造的三行 `exec` 串
        //    （零实现，把 argv 转给已经部署好的后端）—— 用户逐字「不要有什么 bash 脚本，不要有什么单独的 ccm」。
        //    生成器在后端层：本机也要一条 `ccm` 入口，两条落点取自同一处。
        //    `structural_scan` 那条「源码里点名的符号今天还对不对」的判据守着这个住址。
        //    ⚠ **它今天不是一份「仓里的文件」** —— `EmbeddedText { repo_path }` 这个形状
        //    在这一条上已经不合身了（值是**算出来的**，路径取自用户填的 `backend_path`）。
        //    本模块头注自己写着「零生产消费者、T02 接不上就该删掉本模块」⇒ **不为它改类型**，
        //    如实指到那个函数的住址，并把这一格的形状问题登记在这里。
        installable: true,
        uninstallable: true,
        carriers: &[
            Carrier {
                // 远端 `ccm` 就是推过去的那一份后端字节（三行 shim 与它的生成器删了）。
                source: ToolSource::EmbeddedBinary {
                    repo_path: "embedded-backends",
                },
                // `.local/bin/ccm` → `.cc-monitor/bin/ccm`：远端写只许两处，入口是部署物；
                //   也正是它的落点（本机那一条早就在那儿）。
                destination: ToolDestination::RemoteHomeRelative(
                    relay_route_core::BACKEND_LANDING_REL,
                ),
                touches: &[
                    TouchedFile {
                        path: concat!("~/", relay_route_core::backend_landing_rel!()),
                        host: HostScope::Remote,
                        effect: TouchEffect::OwnedFile,
                    },
                    TouchedFile {
                        path: "~/.bashrc",
                        host: HostScope::Remote,
                        effect: TouchEffect::FencedBlock,
                    },
                ],
            },
            // 🔴 本机那条落点：没有它，闭集里落点是 `…/ccm` 的只有远端那一条，
            //    用户逐字「装了新版后 `~/.local/bin/ccm` 可以干净退役」就**没有承接方**。
            //    落点刻意**不是** `~/.local/bin`：那是用户那份旧的所在，
            //    用户逐字「原本的配置**要手动删除**」。
            Carrier {
                // 本机 `ccm` 就是后端本身（逐字节副本删了）：放它的是 `extract_embedded_to`。
                source: ToolSource::EmbeddedBinary {
                    repo_path: "src/frontend/shell/src/local_backend.rs::extract_embedded_to",
                },
                destination: ToolDestination::LocalHomeRelative(concat!(
                    relay_route_core::backend_landing_rel!(),
                    "*"
                )),
                touches: &[TouchedFile {
                    // ⚠ **末段是 glob 而不是 `ccm`**，而且这不是偷懒：本机那份是要**被起成进程**的，
                    // 在把扩展名当身份的平台上它叫 `ccm.exe`（名字的唯一源头是
                    // `local_backend::local_ccm_entry_name`，后缀由 `build.rs` 按 `TARGET` 算）。
                    // 写死 `ccm` 会让这一行在 Windows 上**恒显示「缺失」** —— 那正是本页
                    // 头注禁的「对能用的安装报假警报」。两边由
                    // `the_declared_local_ccm_path_really_matches_the_name_we_install` 对拍。
                    path: concat!("~/", relay_route_core::backend_landing_rel!(), "*"),
                    host: HostScope::Client,
                    effect: TouchEffect::OwnedFile,
                }],
            },
        ],
    },
    // ═══ 🔴 **这一条叫 `backend`，名字不是清洁工作** ═══
    //
    // 「**一个后端要两处使用 / 即远程后端就是远程本地机器的后端**」。
    // 叫它「远端后端」是**假的**：那一份不是「远端的后端」，是**那台机器的本地后端**
    // （用户逐字「两份后端应该要一样的」·「后端只有一个」）。
    //
    // 🔴 **名字塑造发版流水线的形状**：把它当成「远端」产物，`build-linux` 就会漏铺本机后端那一步
    // ⇒ **Linux 裸 exe 装出来没有本机后端**（用户已裁「那肯定带」）。
    // ⚠ 发版那一步不归这里，这里只把名字与闭集说对。
    ToolSpec {
        id: "backend",
        display_name: Text(|| copy_text("rsToolRegistry.tools.backendName", &[])),
        installable: true,
        uninstallable: true,
        // 🔴 **同一份后端，几种载体、几个落点**（自释放 · 推给远端）。
        // ⚠ 它们是**同一份代码**的几种载体，不是几个工具。
        //   自释放那份记在 `ccm` 的本机载体底下 ⇒ 这里只有推给远端那一条。
        carriers: &[
            // 内嵌那份的本机载体并进了 `ccm` 那一条的本机载体：它放下去的就是 `~/.cc-monitor/bin/ccm`（后端本身），
            //   同一个文件不在两个工具底下各记一遍（那张「本机 `ccm` 落点恰一条」的对拍要这一格）。
            Carrier {
                source: ToolSource::EmbeddedBinary {
                    repo_path: "embedded-backends",
                },
                // 可填的 `backendPath` 删了：落点恒是那台的 `~/.cc-monitor/bin/ccm`（它就是 `ccm`）。
                destination: ToolDestination::RemoteHomeRelative(
                    relay_route_core::BACKEND_LANDING_REL,
                ),
                touches: &[TouchedFile {
                    path: concat!("~/", relay_route_core::backend_landing_rel!()),
                    host: HostScope::Remote,
                    effect: TouchEffect::OwnedFile,
                }],
            },
        ],
    },
    // ═══ **从第三档升上来的第一项** ═══
    //
    // 它昨天还住在 [`UNMANAGED_ENV`]（`app 假设它在`），那一条的说明逐字写着
    // 「加与删两侧都只造 PowerShell 那两条 profile 路径，POSIX rc 一条都不扫」。
    // 载体这一层把那句话变成了假的：`profile_installer::plan_install` 的 `PosixRc` 臂把那个
    // 别名块装进用户**选定**的 rc（内容与远端那个口来自同一个常量 `profile_installer::CCM_WRAPPER_SNIPPET`，
    // 合块与剥块都走 `profile_installer::merge_profile_block` / `strip_profile_block`），
    // `profile_installer::plan_uninstall` 卸得掉，`profile_installer::scan_profile` 查得出。
    //
    // 🔴 **升档本身是一条可验的性质**，不是一句话：`installable: true` ⇒ [`environment`]
    // 把它算成 [`EnvTier::AppInstalls`]，判据是 `posix_rc_aliases_sits_in_the_first_tier_now`。
    // 档没升 = 活没做完 —— 这一格从此有人数着。
    ToolSpec {
        id: "posix-rc-aliases",
        display_name: Text(|| copy_text("rsToolRegistry.tools.posixAliasesName", &[])),
        installable: true,
        uninstallable: true,
        carriers: &[Carrier {
            // 装进去的内容**就是仓里那份文件**（`profile_installer::CCM_WRAPPER_SNIPPET` 是它的
            // `include_str!`）。远端那条 `ccm` 用的是同一份 —— 那正是这里不许出现第二份的东西。
            source: ToolSource::EmbeddedText {
                repo_path: "src/shared/ccm-aliases.sh",
            },
            // 🔴 **路径由人选，产品不猜** —— 这一格用占位符而不是 `~/.bashrc`，
            // 理由：申报一个我们其实没在用的常量，
            // 审计页会拿它去查一个没人写的路径然后言之凿凿地报「缺失」。
            // `.bashrc` / `.zshrc` / `config.fish` 写法不同，替人选一份是最坏的那条路
            // （`account_aliases` 的 `§0e`）。
            destination: ToolDestination::UserShellProfile(ShellProfile::PosixRc),
            touches: &[TouchedFile {
                path: "$POSIX_RC",
                host: HostScope::Client,
                effect: TouchEffect::FencedBlock,
            }],
        }],
    },
    ToolSpec {
        id: "powershell-profile",
        display_name: Text(|| copy_text("rsToolRegistry.tools.powershellName", &[])),
        installable: true,
        uninstallable: true,
        carriers: &[Carrier {
            source: ToolSource::Generated,
            destination: ToolDestination::UserShellProfile(ShellProfile::PowerShell),
            touches: &[TouchedFile {
                path: "$PROFILE",
                host: HostScope::Client,
                effect: TouchEffect::FencedBlock,
            }],
        }],
    },
    // 用户级 `PATH`（Windows 那一台注册表里的那一份）：机器设置「配置」栏那个开关经 `ccm-user-path` 把 `ccm` 所在目录加进去 · 撤出来
    //   （`control/ccm/user_path.rs`）；cmd · Git Bash 也敲得到 `ccm` 靠它。改的是用户的东西 ⇒ 进「改过的用户文件」，撤回指到那个开关。
    //   别的平台那台问不出（不说 PowerShell）⇒ 这一行恒「未确定」，不进「改过的用户文件」。
    //   `Either`：问的就是答这张表的那台后端自己的用户级 `PATH`（本机远端同一条命令）。
    ToolSpec {
        id: "ccm-user-path",
        display_name: Text(|| copy_text("machineAliases.userPath.title", &[])),
        installable: true,
        uninstallable: true,
        carriers: &[Carrier {
            source: ToolSource::Generated,
            destination: ToolDestination::UserPath,
            touches: &[TouchedFile {
                path: USER_PATH_TOKEN,
                host: HostScope::Either,
                effect: TouchEffect::FencedBlock,
            }],
        }],
    },
];

// ═══════════════════════════════════════════════════════════════════════════
// **环境清单的闭集** —— 「app 要的东西齐了没有」这个问题的人群
// ═══════════════════════════════════════════════════════════════════════════

/// app 与一个环境项的关系。**四档穷举，而且是从两格派生出来的，不是手填的。**
///
/// | | 今天有装口 | 今天没有装口 |
/// |---|---|---|
/// | [`Provisioning::AppShips`] | [`EnvTier::AppInstalls`] | [`EnvTier::AppShipsNoInstallerYet`] |
/// | [`Provisioning::UserProvides`] | —— | [`EnvTier::UserInstallsWePrompt`] |
/// | [`Provisioning::NotAnInstall`] | —— | [`EnvTier::AppOnlyChecks`] |
///
/// 🔴 **每一档都必须在清单里有一格，不许靠「没列出来」表示。**
/// app 用的时候直接假设在的那些（`claude` · `tmux` · 终端出口 · `git` · `ssh` · `pgrep` · `xdg-open` · `bash` ·
/// MCP server 本体）不进表，「app 依赖本机环境」这个判断就**在代码里没有家**，只能从「表里没有」倒推。
///
/// # 没有「app 假设它在、既不装也不查」那一档
///
/// 通用工具**不是**「我不看」，是「**用户自己装，而我们会看、缺了要说**」⇒ 它们在
/// [`EnvTier::UserInstallsWePrompt`]。每一档都要有成员（`every_tier_has_members_so_absence_never_encodes_a_judgement`）：
/// 没有成员的档不留。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum EnvTier {
    /// **app 装的** —— 该我们装，而且今天真有装口。
    AppInstalls,
    /// **app 该自带，而今天还没有装口**。
    ///
    /// ⚠ 它**不许被读成「不该我们装」**：那是 `UserProvides` / `NotAnInstall` 的意思。
    /// 这一格是**欠的实现**，看得见、数得出来。
    AppShipsNoInstallerYet,
    /// **你自己装，缺了我们提示你** —— app 不装，但**真去查**。
    UserInstallsWePrompt,
    /// **app 只查** —— 谁都不「装」它，我们查得到它在不在。
    AppOnlyChecks,
}

impl EnvTier {
    /// 🔴 **档由两格派生**：「谁该装」× 「今天有没有装口」。**没有兜底臂** ——
    /// 任何一侧加变体都会编译失败，逼人回答那一格该落哪一档。
    pub(crate) fn of(who: Provisioning, has_installer_today: bool) -> EnvTier {
        match (who, has_installer_today) {
            (Provisioning::AppShips, true) => EnvTier::AppInstalls,
            (Provisioning::AppShips, false) => EnvTier::AppShipsNoInstallerYet,
            // 这两支的 `_` 是**有意的**：用户裁了「不该我们装」，那么有没有装口都不改变档。
            // 而「不该我们装却真有装口」是自相矛盾，由
            // `nobody_declares_an_installer_for_something_we_should_not_install` 单独判红 ——
            // 不在这里悄悄吸收掉。
            (Provisioning::UserProvides, _) => EnvTier::UserInstallsWePrompt,
            (Provisioning::NotAnInstall, _) => EnvTier::AppOnlyChecks,
        }
    }
}

/// 闭集里手写的那一半。
///
/// 🔴 它分两群：9 项通用工具是 [`Provisioning::UserProvides`]，
/// [`Provisioning::AppShips`] 那一群今天是空的（账号库由后端自己建，不在表里）。
///
/// 每一条登记时带上它的平台范围（[`OnPlatform`]）：只在 unix 那一台上有意义的几条不进 Windows 那一台的闭集（[`environment_on`]），
/// 页面与「待办 · 要装」跟着不出 —— 平台范围只住这里，显示侧不另判。
pub(crate) const UNMANAGED_ENV: &[(OnPlatform, UnmanagedEnv)] = &[
    // `claude-cli` 那一条住适配层（`agents/claudecode/footprint.rs::UNMANAGED_ENV`），每台都要。
    (
        OnPlatform::Every,
        UnmanagedEnv {
            id: "tmux",
            display_name: Text(|| copy_text("mobTerms.row.tmux", &[])),
            who: Provisioning::UserProvides,
            probe: EnvProbe::OnPath,
            named: "tmux",
            host: HostScope::Either,
            site: "backend/control/launch_render/ccm_invocation.rs::Refusal",
        },
    ),
    // Windows 那一台开终端走系统自带的那一口，没有 `xdg-terminal-exec`。
    (
        OnPlatform::UnixOnly,
        UnmanagedEnv {
            id: "terminal-exit",
            display_name: Text(|| copy_text("rsToolRegistry.env.terminalExitName", &[])),
            who: Provisioning::UserProvides,
            probe: EnvProbe::OnPath,
            named: "xdg-terminal-exec",
            host: HostScope::Client,
            site: "platform/terminal.rs::EMULATORS",
        },
    ),
    (
        OnPlatform::Every,
        UnmanagedEnv {
            id: "git",
            display_name: Text(|| "git".to_string()),
            who: Provisioning::UserProvides,
            probe: EnvProbe::OnPath,
            named: "git",
            host: HostScope::Client,
            site: "scanning_guard_registry_tests.rs::git_read",
        },
    ),
    (
        OnPlatform::Every,
        UnmanagedEnv {
            id: "ssh",
            display_name: Text(|| copy_text("rsToolRegistry.env.sshName", &[])),
            who: Provisioning::UserProvides,
            probe: EnvProbe::SshClient,
            named: "ssh",
            host: HostScope::Client,
            site: "platform/ssh_client.rs::locate",
        },
    ),
    // `pgrep` 那一行摘了：它唯一的 Rust 住址是驾驶舱 shell 读那条「超时不留孤儿」判据的数进程助手，随那条读一起退役。
    // Windows 那一台开文件走系统自带的那一口，没有 `xdg-open`。
    (
        OnPlatform::UnixOnly,
        UnmanagedEnv {
            id: "xdg-open",
            display_name: Text(|| "xdg-open".to_string()),
            who: Provisioning::UserProvides,
            probe: EnvProbe::OnPath,
            named: "xdg-open",
            host: HostScope::Client,
            site: "lib.rs::open_with_os",
        },
    ),
    // Windows 那一台的终端是 PowerShell：新终端的 `PATH` 读注册表，不经登录 bash。
    (
        OnPlatform::UnixOnly,
        UnmanagedEnv {
            id: "login-shell",
            display_name: Text(|| copy_text("rsToolRegistry.env.loginShellName", &[])),
            who: Provisioning::UserProvides,
            probe: EnvProbe::OnPath,
            named: "bash",
            host: HostScope::Either,
            site: "platform/shell/mod.rs::fresh_terminal_path",
        },
    ),
    (
        OnPlatform::Every,
        UnmanagedEnv {
            id: "mcp-server",
            display_name: Text(|| copy_text("rsToolRegistry.env.mcpServerName", &[])),
            who: Provisioning::UserProvides,
            // 🔴 **这一条是「查不动」而不是「不查」的活体** —— 它不是懒：
            // 那个 command 是**用户在 `.mcp.json` 里写的一行**，本页连是哪个项目都不知道
            // （`project-mcp` 的落点是 `ProjectRelative`）⇒ 名字本身就是个占位符，
            // 拿 `$MCP_COMMAND` 去 `PATH` 上找只会恒答「没有」，那是一句**自信的错答案**。
            probe: EnvProbe::CannotProbe,
            named: "$MCP_COMMAND",
            host: HostScope::Either,
            site: "mcp_edit.rs::upsert_mcp_server_value",
        },
    ),
];

/// 手写的一条环境项在哪种平台的那台上有意义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OnPlatform {
    Every,
    /// Windows 那一台上没有这回事（终端是 PowerShell · 开终端 / 开文件走系统那一口）。
    UnixOnly,
}

impl OnPlatform {
    /// 这一条在不在那一台的闭集里（`windows` ＝ 那台是 Windows）。
    pub(crate) fn holds(self, windows: bool) -> bool {
        match self {
            OnPlatform::Every => true,
            OnPlatform::UnixOnly => !windows,
        }
    }
}

/// 闭集里一项的**来路**。两态，没有第三种 —— 一项要么有 [`ToolSpec`]，要么没有。
///
/// 做成枚举而不是两个 `Option`：两个 `Option` 有四种组合，其中两种是不该存在的状态
/// （都有 / 都没有），而那两种状态一旦能被构造出来，早晚有人构造。
pub(crate) enum EnvBacking {
    /// 有 `ToolSpec` —— **路径 · effect · host 只住 `TOOLS` 那一份**，这里不复述。
    Managed(&'static ToolSpec),
    /// 没有 —— 只有一个名字、它在哪台机器上、以及**怎么查它**。
    Named {
        named: &'static str,
        host: HostScope,
        /// 查法从 [`UnmanagedEnv::probe`] 原样带过来 ——
        /// 这一档从此**真去查**，不再是「我们压根没去查」。
        probe: EnvProbe,
    },
}

/// 闭集里的一项。
pub(crate) struct EnvEntry {
    pub id: &'static str,
    pub display_name: String,
    /// **谁该装**。「app 自带」这个人群就是 `who == AppShips` 的那几项。上界面的是由它派生的 [`Self::tier`] ⇒ 生产段零读者。
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "上界面的是由它派生的 `tier`，生产段零读者")
    )]
    pub who: Provisioning,
    /// **档** —— 由 `who` ×「今天有没有装口」两格**派生**（[`EnvTier::of`]），不是手填的。
    pub tier: EnvTier,
    pub backing: EnvBacking,
}

/// 受管工具的**全集**：本表 [`TOOLS`] ＋ 注册表里各家足迹面带来的那一半（落在那一家布局里的），按注册表顺序。
pub(crate) fn tools() -> Vec<&'static ToolSpec> {
    TOOLS
        .iter()
        .chain(crate::agents::footprint_faces().flat_map(|f| f.tools.iter()))
        .collect()
}

/// 手写那一半里在这一种平台的那台上有意义的几条（[`OnPlatform::holds`]；各家足迹面带来的每台都要）。
fn unmanaged_on(windows: bool) -> Vec<&'static UnmanagedEnv> {
    UNMANAGED_ENV
        .iter()
        .filter(|(on, _)| on.holds(windows))
        .map(|(_, u)| u)
        .chain(crate::agents::footprint_faces().flat_map(|f| f.env.iter()))
        .collect()
}

/// 🔴 **环境清单的闭集 —— 唯一一份，现算。**
///
/// 「app 要的东西这台机器上齐了没有」这个问题的**人群**就是它。
/// `config_surface` 的那张表照它建（`the_view_population_is_exactly_the_closed_set` 钉住），
/// 别处要这份名单一律调它，**不许再手抄一张**〔闭集只许有一个住址〕。
///
/// # 能派生的就派生，手写的只有派生不出来的那一半
///
/// - `TOOLS` 里每一条自动进来一项，**两格都派生**：
///   「谁该装」读 `destination`（[`Provisioning::of_tool`]）、「今天有没有装口」读
///   `installable`（那条申报 ↔ 实现的对拍表把它钉在真实现上）⇒ 档由 [`EnvTier::of`] 算出来。
/// - `TOOLS` 里没有的那一半住 [`UNMANAGED_ENV`]：**「谁该装」只能显式声明**
///   （盘上没有任何字段能把它算出来，那是设计判断不是读数），
///   而「今天有没有装口」**不用声明** —— 没有 `ToolSpec` 就是没有装口，恒 `false`。
///
/// ⚠ **这个派生的已知上限，写在这里别被读大一格**：`installable: false` 买到的是
/// 「本页会去解析并观测它申报的每条路径」。远端那几条观测出来是 `Undetermined`
/// （本页不连 SSH）—— 那仍是「查了、只是查不动」，不是「没查」，但它**不等于**
/// 「app 有一个真能回答它在不在的口」。要那一格得另立判据。
///
/// 闭集是**这台平台上**的那一份（[`environment_on`]）：登记成 [`OnPlatform::UnixOnly`] 的几条不进 Windows 那一台的。
pub(crate) fn environment() -> Vec<EnvEntry> {
    environment_on(cfg!(windows))
}

/// 见 [`environment`]：`windows` ⇒ Windows 那一台的闭集。
pub(crate) fn environment_on(windows: bool) -> Vec<EnvEntry> {
    let mut out: Vec<EnvEntry> = tools()
        .into_iter()
        .map(|t| {
            let who = Provisioning::of_tool(t);
            EnvEntry {
                id: t.id,
                display_name: t.display_name.get(),
                who,
                tier: EnvTier::of(who, t.installable),
                backing: EnvBacking::Managed(t),
            }
        })
        .collect();
    out.extend(unmanaged_on(windows).into_iter().map(|u| EnvEntry {
        id: u.id,
        display_name: u.display_name.get(),
        who: u.who,
        // 🔴 **第二格是 `false` 而不是一个字段** —— 手写那一半没有 `ToolSpec`，
        // 也就没有落点、没有 `touches`、没有装 / 卸实现可对拍 ⇒ 「今天有没有装口」
        // 这个问题在这一半上**有唯一答案**，不该再开一个能填错的格子。
        tier: EnvTier::of(u.who, false),
        backing: EnvBacking::Named {
            named: u.named,
            host: u.host,
            probe: u.probe,
        },
    }));
    out
}

#[cfg(test)]
#[path = "../../../tests/backend/footprint/registry_not_managed_tests.rs"]
mod not_managed_tests;

#[cfg(test)]
#[path = "../../../tests/backend/footprint/registry_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/backend/footprint/registry_environment_tests.rs"]
mod environment_tests;
