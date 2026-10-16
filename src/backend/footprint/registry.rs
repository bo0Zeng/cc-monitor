//! T01 第 5 步：**受管工具的声明**（`ToolSpec`）。
//!
//! # ⚠ 它守什么、**不守什么**〔audit-0805 08-06 抽样补记〕
//!
//! 本模块的判据**多数是声明表内部的自洽检查**：字段有没有区分力 · 落点在不在
//! `touches` 里 · 拥有就必须装得了 · 有围栏就必须卸得掉 · 解析器有没有真看见源码。
//! 〔`K-R63` 09-11 订正两处：① 原文写死了一个基数（「15 条」），而判据条数只有一份
//!  住址 —— 本文件判据档里的测试函数，要数就现数；② 「**全部**是表内部自洽」
//!  今天不成立 —— `every_tool_declares_install_and_uninstall_as_the_implementations_really_are`
//!  一边读字段值、一边去别的文件里钉那个装 / 卸实现的签名，它跨出了这张表。〕
//! 两条探针实测它**确实有牙**（把 `ccm` 的落点改成 `.local/bin/ccm-x` ⇒
//! `installable_tools_declare_where_they_land` 点名；把 `installable` 全改成 `true` ⇒
//! 三条同时红，含「在全部 6 个 ToolSpec 上都为真，没有区分力」）。
//!
//! **但它不守一件事：新增一个「会在用户机器上留下东西」的写点，没有任何东西逼它申报。**
//! 人群是「**声明了的**工具」，不是「**真实发生的**安装动作」——
//! 表里那几条看着是全的，而那是**人现在记得**，不是有东西钉着。
//! （**基数不写在散文里**：要数就 `TOOLS.len()`，那是唯一一份。）
//!
//! ## **这张表的语义扩了一格：从「装到别处的受管工具」到「app 会碰的环境项」**
//!
//! 原先只收「我们装到别处的东西」，于是 app **装不了、却离不开**的那些
//! （最吃重的一项是 Claude Code 自己写的会话记录）只能靠**不进表**来表示。
//! 用缺席表达一个判断 —— 那正是 `K-R60` 在治的病。⇒ 现在 `installable: false`
//! 的条目是这张表的一等公民，它们与 [`UNMANAGED_ENV`] 一起被 [`environment`]
//! 汇成**唯一一份**环境清单。`NOT_MANAGED`（**刻意不收**的）语义不变。
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
    ToolSpec, TouchEffect, TouchedFile, UnmanagedEnv,
};
use copy_core::copy_text;

/// 五套既有机制 + cc-bus 的声明。**这里只声明，不改它们任何行为**
/// （第 3 点：先用已知行为的工具验证抽象，再拿它吃新工具）。
pub const TOOLS: &[ToolSpec] = &[
    // 落在 Claude 布局里的那几条（`cc-bus` · `accounts` · `skill-install` · `claude-code`）住适配层
    //   `agents/claudecode/footprint.rs::TOOLS`，经注册表（`agents::Adapter::footprint`）汇进 [`tools`]。
    ToolSpec {
        id: "ccm",
        display_name: Text(|| copy_text("rsToolRegistry.tools.ccmName", &[])),
        // 🔴 `repo_path` 原来指 `shared/ccm`（那份 1592 行 bash）。
        //    〔用@09-11 `K33`〕「不要有什么 bash 脚本，不要有什么单独的 ccm」⇒ 那个文件删了。
        //    ⇒ 指到那个**入口**的来源：`local_backend::ccm_entry_shim` 现造的三行 `exec` 串
        //    （零实现，把 argv 转给已经部署好的后端）。
        //    〔`K-R69` 09-12 订正住址：这句话原先写 `sftp::ccm_entry_shim`，而那个生成器
        //     已**搬进后端层** —— 本机也要一条 `ccm` 入口，两条落点要取自同一处。
        //     逮到它的是 `structural_scan` 那条「源码里点名的符号今天还对不对」的判据
        //     （报文逐字「符号还在，但**搬家了**」），不是人。〕
        //    ⚠ **它今天不是一份「仓里的文件」** —— `EmbeddedText { repo_path }` 这个形状
        //    在这一条上已经不合身了（值是**算出来的**，路径取自用户填的 `backend_path`）。
        //    本模块头注自己写着「零生产消费者、T02 接不上就该删掉本模块」⇒ **不为它改类型**，
        //    如实指到那个函数的住址，并把这一格的形状问题登记在这里。
        installable: true,
        uninstallable: true,
        carriers: &[
            Carrier {
                what: Text(|| copy_text("rsToolRegistry.tools.ccmRemoteWhat", &[])),
                // 远端 `ccm` 就是推过去的那一份后端字节（三行 shim 与它的生成器删了）。
                source: ToolSource::EmbeddedBinary {
                    repo_path: "embedded-backends",
                },
                // `.local/bin/ccm` → `.cc-monitor/bin/ccm`：远端写只许两处，入口是部署物；
                //   也正是它的落点（本机那一条早就在那儿）。
                destination: ToolDestination::RemoteHomeRelative(".cc-monitor/bin/ccm"),
                touches: &[
                    TouchedFile {
                        path: "~/.cc-monitor/bin/ccm",
                        note: None,
                        host: HostScope::Remote,
                        effect: TouchEffect::OwnedFile,
                    },
                    TouchedFile {
                        path: "~/.bashrc",
                        note: Some(Text(|| {
                            copy_text("rsToolRegistry.tools.ccmRemoteProfileNote", &[])
                        })),
                        host: HostScope::Remote,
                        effect: TouchEffect::FencedBlock,
                    },
                ],
            },
            // 🔴 本机那条落点是这一件建出来的。
            //    立件时现打：闭集里落点是 `…/ccm` 的只有远端那一条 ⇒ **本机 0 条**，
            //    于是用户 `K34` 逐字「装了新版后 `~/.local/bin/ccm` 可以干净退役」
            //    **没有承接方** —— 不是没验过，是本机压根没有新的那一份。
            //    落点刻意**不是** `~/.local/bin`：那是用户那份旧的住的地方，
            //    `K34` 逐字「原本的配置**要手动删除**」。
            Carrier {
                what: Text(|| copy_text("rsToolRegistry.tools.ccmLocalWhat", &[])),
                // 本机 `ccm` 就是后端本身（逐字节副本删了）：放它的是 `extract_embedded_to`。
                source: ToolSource::EmbeddedBinary {
                    repo_path: "src/frontend/shell/src/local_backend.rs::extract_embedded_to",
                },
                destination: ToolDestination::LocalHomeRelative(".cc-monitor/bin/ccm*"),
                touches: &[TouchedFile {
                    // ⚠ **末段是 glob 而不是 `ccm`**，而且这不是偷懒：本机那份是要**被起成进程**的，
                    // 在把扩展名当身份的平台上它叫 `ccm.exe`（名字的唯一源头是
                    // `local_backend::local_ccm_entry_name`，后缀由 `build.rs` 按 `TARGET` 算）。
                    // 写死 `ccm` 会让这一行在 Windows 上**恒显示「缺失」** —— 那正是本页
                    // 头注禁的「对能用的安装报假警报」。两边由
                    // `the_declared_local_ccm_path_really_matches_the_name_we_install` 对拍。
                    path: "~/.cc-monitor/bin/ccm*",
                    note: Some(Text(|| copy_text("rsToolRegistry.tools.ccmLocalNote", &[]))),
                    host: HostScope::Client,
                    effect: TouchEffect::OwnedFile,
                }],
            },
        ],
    },
    // ═══ 🔴 **这一条改名了，而改名不是清洁工作** ═══
    //
    // 「**一个后端要两处使用 / 即远程后端就是远程本地机器的后端**」。
    // 这一条先前叫 `remote-daemon` / 「远端后端」，而它说的是**假的**：
    // 那一份不是「远端的后端」，是**那台机器的本地后端**（`K36` 逐字
    // 「两份后端应该要一样的」·`K33` 逐字「后端只有一个」）。
    //
    // 🔴 **这个名字已经花过一次真钱**：`build-linux` 没有 `Stage native backend for
    // self-extract` 那一步而 `build-windows` 有 ⇒ **Linux 裸 exe 装出来没有本机后端**
    // （用户已裁「那肯定带」）。★ 那正是「把它当成『远端』产物」的直接后果 ——
    // **名字塑造了发版流水线的形状**。⚠ 发版那一步归 `K-R42`，这里只把名字与闭集说对。
    ToolSpec {
        id: "backend",
        display_name: Text(|| copy_text("rsToolRegistry.tools.backendName", &[])),
        installable: true,
        uninstallable: true,
        // 🔴 **同一份后端，几种载体、几个落点** —— `K-R68` 摸底现打三种（安装包旁边 · 自释放 · 推给远端）。
        // ⚠ 它们是**同一份代码**的几种载体，不是几个工具（`R24` 裁定一 / `K25` / `K33` / `K36`）。
        //   size2（10-11）撤了安装包旁边那份（与内嵌那份逐字节相同、只是字节来源）；自释放那份记在 `ccm` 的本机载体底下 ⇒ 这里只剩推给远端那一条。
        carriers: &[
            // 内嵌那份的本机载体并进了 `ccm` 那一条的本机载体：它放下去的就是 `~/.cc-monitor/bin/ccm`（后端本身），
            //   同一个文件不在两个工具底下各记一遍（那张「本机 `ccm` 落点恰一条」的对拍要这一格）。
            Carrier {
                what: Text(|| copy_text("rsToolRegistry.tools.backendRemoteWhat", &[])),
                source: ToolSource::EmbeddedBinary {
                    repo_path: "embedded-backends",
                },
                // 可填的 `backendPath` 删了：落点恒是那台的 `~/.cc-monitor/bin/ccm`（它就是 `ccm`）。
                destination: ToolDestination::RemoteHomeRelative(".cc-monitor/bin/ccm"),
                touches: &[TouchedFile {
                    path: "~/.cc-monitor/bin/ccm",
                    host: HostScope::Remote,
                    note: Some(Text(|| {
                        copy_text("rsToolRegistry.tools.backendRemoteNote", &[])
                    })),
                    effect: TouchEffect::OwnedFile,
                }],
            },
        ],
    },
    // ═══ **从第三档升上来的第一项** ═══
    //
    // 它昨天还住在 [`UNMANAGED_ENV`]（`app 假设它在`），`why` 那一格逐字写着
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
            what: Text(|| copy_text("rsToolRegistry.tools.posixAliasesWhat", &[])),
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
                note: Some(Text(|| {
                    copy_text("rsToolRegistry.tools.posixAliasesNote", &[])
                })),
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
            what: Text(|| copy_text("rsToolRegistry.tools.powershellWhat", &[])),
            source: ToolSource::Generated,
            destination: ToolDestination::UserShellProfile(ShellProfile::PowerShell),
            touches: &[TouchedFile {
                path: "$PROFILE",
                host: HostScope::Client,
                note: Some(Text(|| {
                    copy_text("rsToolRegistry.tools.powershellNote", &[])
                })),
                effect: TouchEffect::FencedBlock,
            }],
        }],
    },
];

// ═══════════════════════════════════════════════════════════════════════════
// `K-R60`：**环境清单的闭集** —— 「app 要的东西齐了没有」这个问题的人群
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
/// 〔`K-R57` 摸底现打：`TOOLS` 只有 6 条，而 app 用的时候直接假设在的至少 9 项
/// （`claude` · `tmux` · 终端出口 · `git` · `ssh` · `pgrep` · `xdg-open` · `bash` ·
/// MCP server 本体）—— 它们一条都没进任何一张表。于是「app 依赖本机环境」这个判断
/// **在代码里没有住址**，只能从「表里没有」倒推 —— 用缺席表达一个判断，
/// 正是本工作区一整天在治的那族病。〕
///
/// # 没有「app 假设它在、既不装也不查」那一档
///
/// 通用工具**不是**「我不看」，是「**你自己装，而我会看、缺了我要说**」（`K38`）⇒ 它们在
/// [`EnvTier::UserInstallsWePrompt`]。每一档都要有成员（`every_tier_has_members_so_absence_never_encodes_a_judgement`）：
/// 没有成员的档不留。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum EnvTier {
    /// **app 装的** —— 该我们装，而且今天真有装口。
    AppInstalls,
    /// **app 该自带，而今天还没有装口** —— `KR65D2` 要的那一格。
    ///
    /// ⚠ 它**不许被读成「不该我们装」**：那是 `UserProvides` / `NotAnInstall` 的意思。
    /// 这一格是**欠的实现**，看得见、数得出来。
    AppShipsNoInstallerYet,
    /// **你自己装，缺了我们提示你** —— app 不装，但**真去查**（`KR65D1`）。
    UserInstallsWePrompt,
    /// **app 只查** —— 谁都不「装」它，我们查得到它在不在。
    AppOnlyChecks,
}

impl EnvTier {
    /// 四档的**闭集**本身。现算用（`len()` 就是「几档」，不许在别处写死一个基数）。今天只有判据用它与 [`Self::label`]。
    #[cfg_attr(not(test), allow(dead_code))]
    pub const ALL: &'static [EnvTier] = &[
        EnvTier::AppInstalls,
        EnvTier::AppShipsNoInstallerYet,
        EnvTier::UserInstallsWePrompt,
        EnvTier::AppOnlyChecks,
    ];

    /// 给人看的档名。措辞定在这里，UI 与诊断文本不再各写一遍。
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn label(self) -> String {
        match self {
            EnvTier::AppInstalls => copy_text("rsToolRegistry.envTier.appInstalls", &[]),
            EnvTier::AppShipsNoInstallerYet => {
                copy_text("rsToolRegistry.envTier.appShipsNoInstallerYet", &[])
            }
            EnvTier::UserInstallsWePrompt => {
                copy_text("rsToolRegistry.envTier.userInstallsWePrompt", &[])
            }
            EnvTier::AppOnlyChecks => copy_text("rsToolRegistry.envTier.appOnlyChecks", &[]),
        }
    }

    /// 🔴 **档由两格派生**：「谁该装」× 「今天有没有装口」。**没有兜底臂** ——
    /// 任何一侧加变体都会编译失败，逼人回答那一格该落哪一档。
    pub fn of(who: Provisioning, has_installer_today: bool) -> EnvTier {
        match (who, has_installer_today) {
            (Provisioning::AppShips, true) => EnvTier::AppInstalls,
            (Provisioning::AppShips, false) => EnvTier::AppShipsNoInstallerYet,
            // 这两支的 `_` 是**有意的**：`K38` 裁了「不该我们装」，那么有没有装口都不改变档。
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
/// 🔴 **这张表原来「全是第三档」，今天一条都不是** ——
/// `K38` 之后它分成了两群：9 项通用工具是 [`Provisioning::UserProvides`]，
/// [`Provisioning::AppShips`] 那一群今天是空的（原先那一项是外部账号工具的本机那份；账号库改由后端自己建，它出了表）。
pub const UNMANAGED_ENV: &[UnmanagedEnv] = &[
    // `claude-cli` 那一条住适配层（`agents/claudecode/footprint.rs::UNMANAGED_ENV`）。
    UnmanagedEnv {
        id: "tmux",
        display_name: Text(|| copy_text("rsToolRegistry.env.tmuxName", &[])),
        who: Provisioning::UserProvides,
        probe: EnvProbe::OnPath,
        named: "tmux",
        host: HostScope::Either,
        why: Text(|| copy_text("rsToolRegistry.env.tmuxWhy", &[])),
        site: "backend/control/ccm_invocation.rs::Refusal",
    },
    UnmanagedEnv {
        id: "terminal-exit",
        display_name: Text(|| copy_text("rsToolRegistry.env.terminalExitName", &[])),
        who: Provisioning::UserProvides,
        probe: EnvProbe::OnPath,
        named: "xdg-terminal-exec",
        host: HostScope::Client,
        why: Text(|| copy_text("rsToolRegistry.env.terminalExitWhy", &[])),
        site: "platform/terminal.rs::EMULATORS",
    },
    UnmanagedEnv {
        id: "git",
        display_name: Text(|| "git".to_string()),
        who: Provisioning::UserProvides,
        probe: EnvProbe::OnPath,
        named: "git",
        host: HostScope::Client,
        why: Text(|| copy_text("rsToolRegistry.env.gitWhy", &[])),
        site: "scanning_guard_registry_tests.rs::git_read",
    },
    UnmanagedEnv {
        id: "ssh",
        display_name: Text(|| copy_text("rsToolRegistry.env.sshName", &[])),
        who: Provisioning::UserProvides,
        probe: EnvProbe::SshClient,
        named: "ssh",
        host: HostScope::Client,
        why: Text(|| copy_text("rsToolRegistry.env.sshWhy", &[])),
        site: "platform/ssh_client.rs::locate",
    },
    // `pgrep` 那一行摘了：它唯一的 Rust 住址是驾驶舱 shell 读那条「超时不留孤儿」判据的数进程助手，随那条读一起退役。
    UnmanagedEnv {
        id: "xdg-open",
        display_name: Text(|| "xdg-open".to_string()),
        who: Provisioning::UserProvides,
        probe: EnvProbe::OnPath,
        named: "xdg-open",
        host: HostScope::Client,
        why: Text(|| copy_text("rsToolRegistry.env.xdgOpenWhy", &[])),
        site: "lib.rs::open_with_os",
    },
    UnmanagedEnv {
        id: "login-shell",
        display_name: Text(|| copy_text("rsToolRegistry.env.loginShellName", &[])),
        who: Provisioning::UserProvides,
        probe: EnvProbe::OnPath,
        named: "bash",
        host: HostScope::Either,
        why: Text(|| copy_text("rsToolRegistry.env.loginShellWhy", &[])),
        site: "platform/shell/mod.rs::fresh_terminal_path",
    },
    UnmanagedEnv {
        id: "mcp-server",
        display_name: Text(|| copy_text("rsToolRegistry.env.mcpServerName", &[])),
        who: Provisioning::UserProvides,
        // 🔴 **这一条是「查不动」而不是「不查」的活体** —— 它不是懒：
        // 那个 command 是**用户在 `.mcp.json` 里写的一行**，本页连是哪个项目都不知道
        // （`project-mcp` 的落点是 `ProjectRelative`）⇒ 名字本身就是个占位符，
        // 拿 `$MCP_COMMAND` 去 `PATH` 上找只会恒答「没有」，那是一句**自信的错答案**。
        probe: EnvProbe::CannotProbe {
            why: Text(|| copy_text("rsToolRegistry.env.mcpServerCannotProbe", &[])),
        },
        named: "$MCP_COMMAND",
        host: HostScope::ProjectDir,
        why: Text(|| copy_text("rsToolRegistry.env.mcpServerWhy", &[])),
        site: "mcp_edit.rs::upsert_mcp_server_value",
    },
];

/// 闭集里一项的**来路**。两态，没有第三种 —— 一项要么有 [`ToolSpec`]，要么没有。
///
/// 做成枚举而不是两个 `Option`：两个 `Option` 有四种组合，其中两种是不该存在的状态
/// （都有 / 都没有），而那两种状态一旦能被构造出来，早晚有人构造。
pub enum EnvBacking {
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
pub struct EnvEntry {
    pub id: &'static str,
    pub display_name: String,
    /// **谁该装**（`K38`）。「app 自带」这个人群就是 `who == AppShips` 的那几项。上界面的是由它派生的 [`Self::tier`] ⇒ 生产段零读者。
    #[cfg_attr(not(test), allow(dead_code))]
    pub who: Provisioning,
    /// **档** —— 由 `who` ×「今天有没有装口」两格**派生**（[`EnvTier::of`]），不是手填的。
    pub tier: EnvTier,
    /// 给人看的那一句（取过文的）。
    pub why: String,
    /// 代码住址（从 `why` 拆出来的那一半，见 [`UnmanagedEnv::site`]）。不上界面 ⇒ 生产段零读者，只有判据核它。
    #[cfg_attr(not(test), allow(dead_code))]
    pub site: &'static str,
    pub backing: EnvBacking,
}

/// 受管工具的**全集**：本表 [`TOOLS`] ＋ 注册表里各家足迹面带来的那一半（落在那一家布局里的），按注册表顺序。
pub fn tools() -> Vec<&'static ToolSpec> {
    TOOLS
        .iter()
        .chain(crate::agents::footprint_faces().flat_map(|f| f.tools.iter()))
        .collect()
}

/// 环境清单手写那一半的**全集**：本表 [`UNMANAGED_ENV`] ＋ 各家足迹面带来的那几条。
pub fn unmanaged() -> Vec<&'static UnmanagedEnv> {
    UNMANAGED_ENV
        .iter()
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
///   `installable`（`K-R63` 那条对拍表把它钉在真实现上）⇒ 档由 [`EnvTier::of`] 算出来。
/// - `TOOLS` 里没有的那一半住 [`UNMANAGED_ENV`]：**「谁该装」只能显式声明**
///   （盘上没有任何字段能把它算出来，那是设计判断不是读数），
///   而「今天有没有装口」**不用声明** —— 没有 `ToolSpec` 就是没有装口，恒 `false`。
///
/// ⚠ **这个派生的已知上限，写在这里别被读大一格**：`installable: false` 买到的是
/// 「本页会去解析并观测它申报的每条路径」。远端那几条观测出来是 `Undetermined`
/// （本页不连 SSH）—— 那仍是「查了、只是查不动」，不是「没查」，但它**不等于**
/// 「app 有一个真能回答它在不在的口」。要那一格得另立判据。
pub fn environment() -> Vec<EnvEntry> {
    let mut out: Vec<EnvEntry> = tools()
        .into_iter()
        .map(|t| {
            let who = Provisioning::of_tool(t);
            EnvEntry {
                id: t.id,
                display_name: t.display_name.get(),
                who,
                tier: EnvTier::of(who, t.installable),
                why: copy_text("rsToolRegistry.environment.managedWhy", &[]),
                site: "footprint/registry.rs::environment",
                backing: EnvBacking::Managed(t),
            }
        })
        .collect();
    out.extend(unmanaged().into_iter().map(|u| EnvEntry {
        id: u.id,
        display_name: u.display_name.get(),
        who: u.who,
        // 🔴 **第二格是 `false` 而不是一个字段** —— 手写那一半没有 `ToolSpec`，
        // 也就没有落点、没有 `touches`、没有装 / 卸实现可对拍 ⇒ 「今天有没有装口」
        // 这个问题在这一半上**有唯一答案**，不该再开一个能填错的格子。
        tier: EnvTier::of(u.who, false),
        why: u.why.get(),
        site: u.site,
        backing: EnvBacking::Named {
            named: u.named,
            host: u.host,
            probe: u.probe,
        },
    }));
    out
}

/// ★ **反向登记：考虑过、但刻意不收进 [`TOOLS`] 的东西**〔devbench F06, 08-10〕。
///
/// # 为什么要有这张表
///
/// [`TOOLS`] 收的是「**app 会碰的东西**」——装得了的、以及装不了但我们要去读/去查的
/// （`K-R60` 起，见模块头注那一节）。而「某个东西不在表里」有两种截然不同的原因：
///
/// | 原因 | 该怎么办 |
/// |---|---|
/// | **没人想起来** | 那是洞，补进 `TOOLS` |
/// | **它压根不是「装到别处的工具」** | 那是分类正确 —— 但**没有任何地方记着这个判断** |
///
/// 第二种今天全靠口口相传：devbench F01 的清单就把 `code-picture` 记成了「不在受管工具
/// 注册表」这个洞，而实测它**不该在**。⇒ 把判断落成表，下次有人问就有答案，
/// 且判据钉住那个理由还在。
///
/// ⚠ **这张表最容易变成许愿池**（什么都往里塞、理由写「暂不支持」）。
/// 所以判据要求每条理由**说清它为什么不属这张表的语义**，而不只是「还没做」。
/// ⚠ 但判据**判不了论证的质量** —— 只能判「有没有在论证那件事」。如实记。
#[cfg_attr(not(test), allow(dead_code))]
pub const NOT_MANAGED: &[(&str, &str)] = &[
    (
        "code-picture",
        "**它在 cc-monitor 里零实现，是一个扩展**（一个 skill ＋ 一个 MCP server）—— 它确实「装到别处」，\
         但装它走扩展页已有的资产机制（skill 照 skill 装、MCP 那一项只往目标机写一个 server 条目，\
         程序本身由用户放到那台机器上），是**用法**不是新工具；monitor 与后端都不带它的任何字节。\n\
         ⚠ `K-R65` 当初补的那句道理**仍然成立**：一个名字可以同时是好几样东西，写「表上没有它」的同一拍要把那张表改对（纪律 ⑲）。",
    ),
    (
        "planned-build",
        "**不由 cc-monitor 安装** —— 它是用户自己装在 `<agent 家>/skills/planned-build/` 的 \
         skill，cc-monitor 只**读它的产物**（计划文件）。\n\
         ⇒ 它**不在**本表里（受管工具）。",
    ),
];

#[cfg(test)]
#[path = "../../../tests/backend/footprint/registry_not_managed_tests.rs"]
mod not_managed_tests;

#[cfg(test)]
#[path = "../../../tests/backend/footprint/registry_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/backend/footprint/registry_environment_tests.rs"]
mod environment_tests;
