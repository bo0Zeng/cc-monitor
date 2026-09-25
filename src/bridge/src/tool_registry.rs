//! T01 第 5 步：**受管工具的声明**（`ToolSpec`）。
//!
//! # ⚠ 它守什么、**不守什么**〔audit-0805 08-06 抽样补记〕
//!
//! 本模块的判据**多数是声明表内部的自洽检查**：字段有没有区分力 · 落点在不在
//! `touches` 里 · 拥有就必须装得了 · 有围栏就必须卸得掉 · 解析器有没有真看见源码。
//! 〔`K-R63` 09-11 订正两处：① 原文写死了一个基数（「15 条」），而判据条数只有一份
//!  住址 —— 本文件里的 `#[test]`，要数就现数〔`13b`〕；② 「**全部**是表内部自洽」
//!  今天不成立 —— `every_tool_declares_install_and_uninstall_as_the_implementations_really_are`
//!  一边读字段值、一边去别的文件里钉那个装 / 卸实现的签名，它跨出了这张表。〕
//! 两条探针实测它**确实有牙**（把 `ccm` 的落点改成 `.local/bin/ccm-x` ⇒
//! `installable_tools_declare_where_they_land` 点名；把 `installable` 全改成 `true` ⇒
//! 三条同时红，含「在全部 6 个 ToolSpec 上都为真，没有区分力」）。
//!
//! **但它不守一件事：新增一个「会在用户机器上留下东西」的写点，没有任何东西逼它申报。**
//! 人群是「**声明了的**工具」，不是「**真实发生的**安装动作」——
//! 表里那几条看着是全的，而那是**人现在记得**，不是有东西钉着。
//! （**基数不写在散文里**〔`13b`〕：要数就 `TOOLS.len()`，那是唯一一份。）
//!
//! ## 〔`K-R60` 09-11〕**这张表的语义扩了一格：从「装到别处的受管工具」到「app 会碰的环境项」**
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
//! ⇒ 登记为诚实边界（`ROADMAP §5`），**不假装覆盖**。
//! 解锁条件：安装动作先收敛到一个可枚举的落点（比如所有部署都过一个 `install_to()`），
//! 那时人群才有干净的边界。
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
//! （`ccm` / `cc-acct-iso`），要么需要一次协议握手（remote backend 的 `hello` 帧），
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

/// 内容的来源。**每个变体的使用者数现算**（`TOOLS` 是唯一一份），不写死在这里〔`13b`〕。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum ToolSource {
    /// 仓内文件，编译期 `include_str!` 进二进制（`ccm`）。
    EmbeddedText { repo_path: &'static str },
    /// 仓内目录，运行期读（`cc-bus` 的 `src/shared/cc-bus/`）。
    RepoDir { repo_path: &'static str },
    /// vendored 目录 + 指纹（`cc-acct-iso`；〔TL1〕`code-picture-core` 从前也列在这里，它从没以这个形状进过 `TOOLS`，今天全景小程序那一条是 `EmbeddedBinary`）。
    Vendored {
        repo_path: &'static str,
        fingerprint_file: &'static str,
    },
    /// 交叉编译后内嵌的二进制（remote backend）。
    EmbeddedBinary { repo_path: &'static str },
    /// 由 cc-monitor 现场生成的文本片段（PowerShell profile 块、shell 别名块、钩子片段）。
    Generated,
    /// **不是我们提供的** —— 内容由别人放在那儿，我们只读它。`who` 说清是谁放的。
    ///
    /// 〔`K-R60` 09-11 加〕上面五个变体都预设「这东西的内容出自本仓」，
    /// 而 app 最吃重的那一项（Claude Code 自己写的会话记录）根本不出自本仓。
    /// 没有这一格，它就只能靠**不进表**来表示 —— 而那正是本件在治的病。
    NotOurs { who: &'static str },
}

/// 装到哪。**变体数与使用者数现算**，不写死在这里〔`13b`〕。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum ToolDestination {
    /// 远端家目录下的相对路径（`~/.local/bin/ccm`）。
    ///
    /// # 🔴 〔`K-R81` 09-12〕**它 09-12 下午重新有使用者了，而那不是回退**
    ///
    /// `K-R69`（09-12 上午）写过一段「它零使用者、`never constructed` 就是这笔债的存根」，
    /// 并把退役条件写死成「**下一件活里若仍没有「只落在远端」的工具进来**就删」。
    /// 本件把 [`Carrier`] 这一层立起来之后，那个条件**不成立了**：
    /// `ccm` 的**远端那一份**（shim，`~/.local/bin/ccm`）就是一个只落在远端的载体，
    /// 它与本机那一份不再需要挤在同一个 `destination` 里 ——
    /// 「一个东西两个落点」这件事今天由**载体这一维**表达，不由一个双值变体表达。
    /// ⇒ `BothHomeRelative` 那个变体同拍**删掉**（墓碑写在 [`Carrier`] 的头注里）。
    ///
    /// ⚠ **退役条件仍然有效，只是今天不满足**：哪天连 `ccm` 远端那条也没了，
    /// 就连着它在 `config_surface::resolve_by_destination` 的那一臂一起删
    /// （那一臂是今天**唯一**一处「不看 `host` 就断定远端」的地方，
    /// 删掉之后「在哪台机器上」就只剩 `host` 一个住址）。
    RemoteHomeRelative(&'static str),
    /// 本机家目录下的相对路径（`~/.claude/skills/...`）。
    LocalHomeRelative(&'static str),
    /// 用户的 shell profile（`$PROFILE` / `~/.bashrc`）——路径由用户选。
    UserShellProfile,
    /// 项目目录内的文件（`<dir>/.mcp.json`）。
    ProjectRelative(&'static str),
    /// **路径由用户配置决定，不是常量。**
    ///
    /// T02 审计追问「注册表与真写入方零耦合」时查出来的（比审计报的更严重）：
    /// - `remote-daemon` 原先声明 `RemoteHomeRelative(".local/bin/ccm-backend")`，
    ///   而这个字符串**全仓只出现在注册表自己里**；真实路径是 `RemoteConfig.backend_path`，
    ///   每个远端各自配置（当年的逐次拨号那条路 `run_list_query`〔散文墓碑〕直接 `shell_quote(&cfg.backend_path)`，C4d 已删）。
    /// - `cc-acct-iso` 原先声明 `LocalHomeRelative(".claude/skills/cc-acct-iso")`，
    ///   而 `acct_iso_deploy::deploy_remote_acct_iso(cfg, dest_dir)` 是**远端**部署、
    ///   落点还是**前端传进来的** `dest_dir`。
    ///
    /// 两处都是我凭印象写的常量。**声明一个不存在的常量比不声明更坏**——审计页会拿它去
    /// 查一个没人写的路径，然后言之凿凿地报"缺失"。所以这里显式承认"这是配置项"。
    ///
    /// `token` 是申报路径里用的占位符（形如 `$BACKEND_PATH`，与 `$PROFILE` 同一套写法，
    /// 因此仍满足 `path` 的 ASCII-graphic 判据）；`what` 是给用户看的「去哪儿改」。
    UserConfiguredPath {
        token: &'static str,
        what: &'static str,
    },
    /// **我们不装它** —— 这一格回答的不是「装到哪」，而是「它本来在哪」。
    ///
    /// 〔`K-R60` 09-11 加〕别的五个变体都在回答「我们把它放到哪儿去」。
    /// 一个我们**从不安装、只去读**的东西（`~/.claude/projects/`）填任何一个都是在说假话，
    /// 而「声明一个不存在的落点比不声明更坏」这句话本模块自己写过。
    /// ⇒ 显式承认「这不是我们的落点」。
    /// 判据 `installable_tools_declare_where_they_land` 钉住：用了这一格就不许 `installable: true`。
    NotInstalledByUs { whose: &'static str },
}

/// 这个文件在**哪台机器**上。
///
/// ## 为什么这是 `TouchedFile` 的属性，不是工具的属性
///
/// T04 第一步。它不是"为模型而模型"——不加它，`config_surface` 在**生产平台上会说假话**：
/// `cc-bus` 的 `destination` 是 `LocalHomeRelative`，三条 touches 于是被当**本机路径**去 stat。
/// 但 cc-monitor 的生产平台是 Windows（`ci.yml`/`release.yml` 打包 job 都是 `windows-latest`），
/// 而 cc-bus 跑在 **Claude Code 所在的那台**——`hooks_diag` 为此有**两条** IPC
/// （`diagnose_local_cc_bus_hooks` / `diagnose_remote_cc_bus_hooks`），
/// `cc_bus::read_cc_bus_state(origin)` 读 `~/.cc-bus/` 更是**按 origin 远端 exec** 的。
/// 于是 Windows 用户打开「配置面审计」会看到那三行写着**「不存在」**，
/// 而同一个 app 的驾驶舱正从远端把 inbox 读得好好的。
/// **这正是 T02 专门要防的那类假警报，出现在那一页上格外讽刺。**
/// ## Phase G 用 ≥2 尺子重新论证 `Client` / `ProjectDir`（本会话第 12 次用这把尺子）
///
/// 事实先摆清，两条都不利于保留：
/// - `Client` **1 个使用者**（`$PROFILE`）· `ProjectDir` **1 个**（`.mcp.json`）
/// - 两者在 `config_surface::project_onto_host` 里都落 `(_, other)`，**零行为影响**；
///   而且它们的 `destination` 臂（`UserShellProfile → WindowsProfile`、
///   `ProjectRelative → NeedsProjectDir`）已经独立于 host 决定了解析结果
///   ——**连合并进 `Either` 都不会改变任何解析行为**。
///
/// **结论仍是保留，但理由不是"用了 ≥2 次"（它们没有）。** 理由是：
/// **合并会让屏幕上的话变成假的。** `host_label` 是用户可见事实：
/// `$PROFILE` **确定在客户端**、`.mcp.json` **确定在项目目录**。合进 `Either` 后标签变成
/// 「本机或远端」——对这两条都是**错的**。而这一页的全部价值就是可信告知
/// （T02 立项时那个「Windows 上说不存在而驾驶舱正从远端读」的假警报就是同一件事）。
///
/// **≥2 那把尺子量的是「字段与抽象」，不是「描述型 enum 的变体」** —— T01 已经立过这条界：
/// `ToolSource` 5 个变体里 4 个单用户、`TouchEffect` 的门禁只要求 ≥3 种被用到，
/// 都保留了，因为**变体差异是数据的本性**。把 `HostScope` 按前一把尺子砍掉，
/// 反而是尺子用错了地方。
///
/// 下面 `host_labels_are_distinct_and_truthful` 把这条钉住：四个标签必须互不相同，
/// 且 `Client`/`ProjectDir` 的标签不许含「远端」二字（含了就是在说假话）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum HostScope {
    /// cc-monitor 自己跑的那台（Windows 客户端）。
    Client,
    /// 一个远端连接（按 origin 选）。
    ///
    /// **本机不许替它回答"这个路径在不在"**——T03 阻塞 3 的根因就是本机按 basename 猜远端，
    /// 结果把"装在 /usr/local/bin 且在 PATH 上"错判成"$HOME 那个路径存在"，
    /// 于是该警示的形态不警示，用户贴上去正是一个 path-missing 钩子。
    Remote,
    /// **两端皆可**：Claude Code 跑在哪台，这东西就在哪台。
    ///
    /// 关键语义：**「本机没找到」≠「不存在」**。这一条就是上面那个假警报的解药。
    Either,
    /// 项目目录内（在哪台机器上取决于那个项目是本地还是远端）。
    ProjectDir,
}

/// 这个工具会碰用户的哪个文件，以及**碰它意味着什么**。
/// 本结构是 T02 审计视图的直接输入（使用者数现算，不写死在这里〔`13b`〕）。
///
/// ## `path` 与 `note` 为什么拆开（T02 一上手就撞到的计划≠现实）
///
/// 第一版把两件事写在同一个字符串里：`"~/.bashrc（或所选 profile）"`、
/// `"~/.claude/settings.json 的 hooks 段"`、`"~/.local/bin/cc-*（12 条软链）"`、
/// `"远端 ~/.local/bin/ccm-backend"`。作为展示文本没问题，但 T02 要**真去查这些文件的现状**，
/// 那些散文进不了 `Path`——于是拆成机器可解析的 `path` + 给人看的 `note`。
///
/// 「本机还是远端」**没有新增字段**：从 [`ToolSpec::destination`] 推导
/// （`RemoteHomeRelative` → 远端）。
///
/// **上一版这里说"由 `config_surface` 的测试把这条推导钉住"——那条测试是同义反复，
/// 已删**（T02 审计重要 1；审计实测把 `ccm` 的 destination 翻成本机，492 项照样全绿）。
/// 如实登记：**远端性没有门禁**，它只是 `resolve_touched_path` 的实现约定 + 这段文档。
/// 真要门禁得加 `host` 字段，而它已经有一个真实的第二消费者在等：`~/.cc-bus/` 被本页
/// 解析成**本机**，可 `cc_bus.rs` 是按 `origin` 在**可能是远端**的主机上读它——
/// 一个 `const destination` 表达不了"按运行期 origin 跨主机"。留给 T04 连 origin 模型一起做。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TouchedFile {
    /// **机器可解析**的路径：可含 `~/` 前缀、可含**最后一段**的 glob（`cc-*`）、
    /// 可以是 `$PROFILE` 这种由外部决定的占位。**不放散文**——那是 `note` 的事。
    pub path: &'static str,
    /// 给人看的补充说明（"或用户所选的其它 profile"、"11 条软链"）。没有就 `None`。
    ///
    /// **更正上一版这段话**（T02 审计重要 3）：原文写「字段纪律扫描不覆盖 `TouchedFile`，
    /// 所以 `note` 的 ≥2 判据是人工数的」——说反了两头。`note` 当时**已经有**一条机器门禁
    /// （`config_surface` 的 `rows_cover_…` 里 `with_note.len() >= 2`）；
    /// 真正一条门禁都没有的是 `path` / `effect` 和**将来新增的字段**，
    /// 而审计正是从那个口子进来的（塞 `pub needs_sudo: bool`，492 全绿零 warning）。
    /// 现在 `declared_fields_of` 参数化了，`TouchedFile` 与 `ToolSpec` 走同一条纪律
    /// （`touched_file_fields_follow_the_same_discipline`）。
    pub note: Option<&'static str>,
    /// 这个文件在哪台机器上。见 [`HostScope`]——**不加它，审计页在 Windows 上会说假话**。
    pub host: HostScope,
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
    /// **我们不直接写这个文件，但用户在 cc-monitor 里的动作会导致它被写。**
    ///
    /// 这一档是 T02 审计的阻塞项逼出来的：`~/.cc-bus/` 原先声明成 [`Self::ReadOnly`]，
    /// 于是审计页渲染出「只读（诊断用），我们不写」——**假话**。
    /// cc-monitor 的 cc-bus 驾驶舱有两个按钮走的是
    /// `cc_bus::cc_bus_send`（远端跑 `cc-send`）与 `cc_bus::cc_bus_spawn`（跑 `cc-spawn`）〔散文墓碑〕
    /// （〔C4e〕今天是界面经通道直接说那台后端的 `bus-send` / `bus-spawn`，后端照旧调那两份脚本 —— 这一档的理由不变），
    /// 而 `cc-bus-lib.sh:221` 是 `printf '%s\n' "$line" >> "$inbox"`、
    /// `cc-spawn:141` 追加 `spawned.tsv`、`cc-register:25` 换掉 `agents.tsv`。
    /// 「我们只是调了别人的命令」不改变**用户的文件因为在我们这儿点了一下而变了**这件事。
    /// 这一页的全部价值是可信告知，在自己的主张上失信比不做这一页更坏。
    IndirectWrite,
    /// 〔GP1 · 第四波〕**cc-monitor 旧版放在这儿的那一份，今天要清掉**：认出是它放的（记号见 `ccm_legacy`）就删，
    /// 认不出的一个字节都不动。它不是「拥有」（[`Self::OwnedFile`] 那句「部署时整体覆盖」对它是假话）——
    /// 我们不再往这儿写，只在看见旧的那一份时收回它。`设计/01 §6.7b`「三件都要在足迹里有入口」。
    RetiredLegacy,
}

/// **同一个东西的一种载体** —— 「它这一份怎么产出来、落到哪、碰哪些文件」。
///
/// # 🔴 〔`K-R81` 09-12〕这一层为什么非有不可（用户 09-12 逐字逼出来的）
///
/// 用户逐字：「**一个后端要两处使用 / 即远程后端就是远程本地机器的后端**」。
/// 也就是说：**远端那台机器上跑的那一份，是「那台机器的本地后端」**，
/// 不是「远端的后端」——「本机 / 远端」这个二分本身就是从错的名字里长出来的。
///
/// 而在这一层立起来之前，[`ToolSpec`] 是「一个源 + 一个落点 + 一串 touch」：
/// **同一个后端有三种载体、四个落点，而闭集只表达得了一个半**（`K-R68` 现打）。
/// 三种载体逐条是：① 这一份产物自己带着、要用时自释放的那份（`native-backend/`，
/// `byte_table.rs` 的 `include_bytes!`，〔DP1〕按这一份产物的 `TARGET` 那一格取）·
/// ② 内嵌、推给远端那台机器的那份（`embedded-backends/`）·
/// ③ 安装包放在 app 可执行文件旁边的那份（`tauri.sidecar.conf.json` 的 `externalBin`）。
///
/// # 为什么不是「`destination` 改成多值」
///
/// 那条路（`K-R68` 摸底的「甲」）有一个**配对问题**：`destination` 是每工具一个、
/// `host` 是每 touch 一个 ⇒ M 个落点 × N 条 touch，**「哪条 touch 归哪个落点」
/// 没有任何东西表达得出来**。出路要么给 `TouchedFile` 加一个回指字段
/// （那就是在 touch 上重新发明「载体」这个维度），要么让解析对每条 touch
/// 试遍所有落点（**那会静默地选一个** —— 本模块头注禁的「言之凿凿」的另一面）。
/// ⇒ touch 挂在载体下，**key 天然就有**。
///
/// # 它同时治掉 `ToolSource` 那一半（`R26` 裁定二：两者是同一个形状问题的两半）
///
/// `ccm` 那一行的 `source` 先前**自己的注释逐字承认是假的**：远端那一份的来源是
/// `local_backend::ccm_entry_shim` 现造的 shim，而**本机那一份的来源是后端二进制
/// 自己的改名副本**（`local_backend::install_local_ccm_entry`）——
/// 一个 `source` 字段装不下两个来源，于是那条注释只能写「不为它再开一个变体，
/// 在这里如实写清」。载体这一维立起来之后，两个来源各归各的载体，**注释里那句
/// 「这一格是假的」不再需要**。
///
/// # 🪦 `ToolDestination::BothHomeRelative` 的墓碑（`K-R69` 09-12 上午 → `K-R81` 09-12 下午）
///
/// 那个变体是为「一个 `destination` 装不下两个落点」造的，逐字理由是
/// 「`ccm` 立件时申报的是 `RemoteHomeRelative(…)`……一个 `destination`
/// 就装不下两个事实了」。**本件把「装不下」这个前提本身拆掉了** ⇒ 它同拍删除：
/// `ccm` 现在是两个载体，远端那个 `RemoteHomeRelative`、本机那个 `LocalHomeRelative`，
/// 各自带各自的 touch。⚠ 这不是把 `K-R69` 退回去 —— 它买到的那件事
/// （**闭集里本机侧真有一条 `ccm` 落点**）一个字节没动，钉它的判据也没动。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Carrier {
    /// 这一份**是哪一份** —— 给人读的一句话，同一个工具的几个载体靠它区分。
    ///
    /// ⚠ 它不是 `note`：`note` 说的是「这个**文件**是怎么回事」，
    /// 这一格说的是「这**一份产物**是怎么回事」。
    pub what: &'static str,
    pub source: ToolSource,
    pub destination: ToolDestination,
    pub touches: &'static [TouchedFile],
}

/// 一个受管工具的完整声明。
///
/// **所有字段必须是 `const`-可构造的声明式数据**（无函数指针、无 `dyn`、无 `String`）。
/// 这不是风格偏好，是上面那条「探测机制不进来」边界的落地形式。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ToolSpec {
    pub id: &'static str,
    pub display_name: &'static str,
    /// 能不能装/升。
    ///
    /// 🔴 **这一格是「app 装的」与「app 只查」两档的分界线**（`K-R60`）：
    /// [`environment`] 就是读它算出每条 `ToolSpec` 属于哪一档的。
    /// ⇒ 它填错了，用户在配置面上读到的「能否装/撤」与清单上的档**同时**是假的。
    /// 守它的是 `every_tool_declares_install_and_uninstall_as_the_implementations_really_are`
    /// （读**字段值**，不是注释里的词频 —— 这一格上一次就是被词频守卫放过去的）。
    /// 〔`K-R63` 09-11〕上一版守它的那条只服务 `cc-bus` 一个工具，**名字里带工具名**；
    /// 今天这条是**覆盖全表**的性质，两格一起对拍，两个方向都判。
    pub installable: bool,
    /// 能不能卸。
    ///
    /// 🔴 **它与 `installable` 是同一枚硬币，而它先前只有半边被守**〔`K-R63` 09-11〕：
    /// `fenced_block_implies_uninstallable` 守的是「有围栏 ⇒ 必须声明可卸」（少报那一向），
    /// **多报**（声明可卸而盘上根本没有卸载实现）一条都不红 —— PM 的刀 C 把
    /// `cc-acct-iso` 由 `false` 翻成 `true`，全表 1379 条一条没响。
    /// ⇒ 今天与 `installable` 同走上面那条性质：字段值 ⇔ 盘上那个符号在不在。
    pub uninstallable: bool,
    /// 🔴 〔`K-R81` 09-12〕**同一个东西的几种载体**，头注住 [`Carrier`]。
    ///
    /// 先前这里是 `source` / `destination` / `touches` 三个平铺字段 ——
    /// 那个形状说得出「这个工具落在**一个**地方」，说不出
    /// 「**同一个后端**落在哪几处」。今天说得出。
    pub carriers: &'static [Carrier],
}

impl ToolSpec {
    /// 这个工具碰的**全部**文件（跨载体铺平）。
    ///
    /// ⚠ 只在「不关心是哪个载体」时用它；关心的时候用 [`Self::carrier_touches`] ——
    /// 铺平会把本件刚立起来的那个 key 又丢掉一次。
    pub fn touches(&self) -> impl Iterator<Item = &'static TouchedFile> {
        self.carriers.iter().flat_map(|c| c.touches.iter())
    }

    /// `(载体, 它碰的文件)` —— **配对问题的答案就是这个迭代器**。
    pub fn carrier_touches(
        &self,
    ) -> impl Iterator<Item = (&'static Carrier, &'static TouchedFile)> {
        self.carriers
            .iter()
            .flat_map(|c| c.touches.iter().map(move |f| (c, f)))
    }
}

/// 五套既有机制 + cc-bus 的声明。**本轮只声明，不改它们任何行为**
/// （MASTERPLAN §4 第 3 点：先用已知行为的工具验证抽象，再拿它吃新工具）。
pub const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        id: "ccm",
        display_name: "ccm 统一启动器（后端本体的一次性模式）",
        // 🔴 〔`K-R48` 第二拍 09-11〕`repo_path` 原来指 `shared/ccm`（那份 1592 行 bash）。
        //    〔用@09-11 `K33`〕「不要有什么 bash 脚本，不要有什么单独的 ccm」⇒ 那个文件删了。
        //    ⇒ 指到那个**入口**的来源：`local_backend::ccm_entry_shim` 现造的三行 `exec` 串
        //    （零实现，把 argv 转给已经部署好的后端）。
        //    〔`K-R69` 09-12 订正住址：这句话原先写 `sftp::ccm_entry_shim`，而那个生成器
        //     本轮**搬进了后端层** —— 本机也要一条 `ccm` 入口，两条落点要取自同一处。
        //     逮到它的是 `structural_scan` 那条「源码里点名的符号今天还对不对」的判据
        //     （报文逐字「符号还在，但**搬家了**」），不是人。〕
        //    ⚠ **它今天不是一份「仓里的文件」** —— `EmbeddedText { repo_path }` 这个形状
        //    在这一条上已经不合身了（值是**算出来的**，路径取自用户填的 `backend_path`）。
        //    本模块头注自己写着「零生产消费者、T02 接不上就该删掉本模块」⇒ **不为它改类型**，
        //    如实指到那个函数的住址，并把这一格的形状问题登记在这里。
        // 🔴 〔`K-R81` 09-12〕**上面那段话里「一个 `source` 装不下两个来源」那一格，今天没了。**
        //    `K-R69` 当时逐字写的是：「本机那一半的来源不是这个 shim —— 是后端二进制自己的
        //    改名副本（`local_backend::install_local_ccm_entry`）。`ToolSource` 一个字段
        //    同样装不下两个来源……**不为它再开一个变体**，在这里如实写清」。
        //    ⇒ 那句「在注释里如实写清」是**用散文顶替一个字段**。载体这一维立起来之后，
        //    两个来源各归各的载体，注释不再承重（`R26` 裁定二：`ToolSource` 与
        //    `destination` 是同一个形状问题的两半，本件同拍治）。
        //    ⚠ 两条载体仍是**同一个东西**：`control::ccm::intercept` 认 `argv[0]` 的 basename，
        //    两边都进那一处解析（`K33` 逐字「所有命令只许有一处」）。
        installable: true,
        uninstallable: true,
        carriers: &[
            Carrier {
                what: "远端那台机器上的那条 `ccm` —— 三行 `exec` 的 shim，把 argv 转给那台机器上已经部署好的后端",
                source: ToolSource::EmbeddedText {
                    repo_path: "src/bridge/src/backend/control/local_backend.rs::ccm_entry_shim",
                },
                // 〔SR1b · 2026-09-24〕`.local/bin/ccm` → `.cc-monitor/bin/ccm`：远端写只许两处（V89），入口是部署物；
                //   也正是 `设计/01 §6.7b` 的落点（本机那一条早就在那儿）。
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
                        note: Some("或用户在部署向导里选的其它 profile"),
                        host: HostScope::Remote,
                        effect: TouchEffect::FencedBlock,
                    },
                    // 〔GP1 · 第四波〕`设计/01 §6.7b` 迁移 ② ③：旧版入口落在这儿（09-11 前是 bash 启动器、之后是三行 shim）。
                    TouchedFile {
                        path: "~/.local/bin/ccm",
                        note: Some(
                            "旧版 cc-monitor 放的入口（今天入口在 ~/.cc-monitor/bin/ccm）：部署后端时、连上那台时各看一眼，\
                             认出是 cc-monitor 放的就删，认不出的不动",
                        ),
                        host: HostScope::Remote,
                        effect: TouchEffect::RetiredLegacy,
                    },
                ],
            },
            // 🔴 〔`K-R69` 09-12〕本机那条落点是这一件建出来的。
            //    立件时现打：闭集里落点是 `…/ccm` 的只有远端那一条 ⇒ **本机 0 条**，
            //    于是用户 `K34` 逐字「装了新版后 `~/.local/bin/ccm` 可以干净退役」
            //    **没有承接方** —— 不是没验过，是本机压根没有新的那一份。
            //    落点刻意**不是** `~/.local/bin`：那是用户那份旧的住的地方，
            //    `K34` 逐字「原本的配置**要手动删除**」。
            Carrier {
                what: "本机（monitor 跑着的这台）上的那条 `ccm` —— **后端二进制自己的改名副本**，不是壳、不是第二份实现",
                source: ToolSource::EmbeddedBinary {
                    repo_path: "src/bridge/src/backend/control/local_backend.rs::install_local_ccm_entry",
                },
                destination: ToolDestination::LocalHomeRelative(".cc-monitor/bin/ccm*"),
                touches: &[TouchedFile {
                    // ⚠ **末段是 glob 而不是 `ccm`**，而且这不是偷懒：本机那份是要**被起成进程**的，
                    // 在把扩展名当身份的平台上它叫 `ccm.exe`（名字的唯一真相源是
                    // `local_backend::local_ccm_entry_name`，后缀由 `build.rs` 按 `TARGET` 算）。
                    // 写死 `ccm` 会让这一行在 Windows 上**恒显示「缺失」** —— 那正是本页
                    // 头注禁的「对能用的安装报假警报」。两边由
                    // `the_declared_local_ccm_path_really_matches_the_name_we_install` 对拍。
                    path: "~/.cc-monitor/bin/ccm*",
                    note: Some(
                        "🔴 `K-R69`：**本机那条 `ccm` 入口** —— 后端二进制自己的改名副本\
                         （`control::ccm::intercept` 认 `argv[0]` 的 basename）。\
                         放在 monitor 自己的目录里，**刻意不碰你 `~/.local/bin` 下那份旧的** —— \
                         那一份要不要删由你自己定（`K34` 逐字：原本的配置要手动删除）",
                    ),
                    host: HostScope::Client,
                    effect: TouchEffect::OwnedFile,
                }],
            },
        ],
    },
    ToolSpec {
        id: "cc-bus",
        display_name: "cc-bus 多实例消息总线",
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
        // ★★ **08-13 用户裁：开**（`U10b`）。⇒ `installable` 从 `false` 翻成 `true`，
        // 实现在 `cc_bus_deploy.rs`，那四个配套**逐条落地**（模块头注里四个 `★` 一一对应），
        // 例外本身写成 `src/doc/INVARIANTS.md` 的第 7 条。
        // ⚠ `uninstallable` **仍是 `false`** —— 卸载没做，如实声明（不因为「装做了」就顺手标 true）。
        //
        // ⚠ 这堵墙今天已经在收账：`P4b` 改的是仓内那份 `cc-spawn`，而 `~/.local/bin/cc-*`
        // 指向的是 `~/.claude/skills/cc-bus/`（实测两份差 167 行）—— **改动到不了本机**。
        //
        // 🔴 **09-11 `K-R60`：上面那句「翻成 true」到今天才真的落到字段上。**
        // `K-R57` 摸底逮到：字段一直是 `false`，而同一个注释块逐字写着要翻成 `true`，
        // 实现（`deploy_local_cc_bus`）**一直在**。⇒ 那张表对「cc-bus 装不装得了」的申报
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
            what: "仓里那份 `src/shared/cc-bus`（整目录），装到 Claude Code 那台机器的 skills 下",
            source: ToolSource::RepoDir {
                repo_path: "src/shared/cc-bus",
            },
            destination: ToolDestination::LocalHomeRelative(".claude/skills/cc-bus"),
            touches: &[
            TouchedFile {
                // 🔴 **这一条是 `K-R60` 补的，而它是被上面那次翻字段逼出来的。**
                // `installable_tools_declare_where_they_land` 要求「装得了就必须申报装到哪」；
                // 字段一直是 `false` ⇒ 这条判据一直**跳过** cc-bus ⇒ 部署真正写的那个目录
                // （`deploy_local_cc_bus` 往 `<claude_dir>/skills/cc-bus/` 铺 17 个文件）
                // **在这一页上一行都没有**。翻成 `true` 的当场它就红了。
                // ⇒ 一处假申报盖住的不止它自己那一格。
                path: "~/.claude/skills/cc-bus",
                host: HostScope::Either,
                note: Some(
                    "部署真正写的那个目录（整目录铺、覆盖前改名备份）；\
                     装的口只有本机那一个（deploy_local_cc_bus），而 cc-bus 本身跟着 Claude Code 走",
                ),
                effect: TouchEffect::OwnedFile,
            },
            TouchedFile {
                path: "~/.claude/settings.json",
                host: HostScope::Either,
                note: Some("只碰 hooks 段，且我们不写——只生成待贴文本"),
                effect: TouchEffect::GenerateOnly,
            },
            TouchedFile {
                path: "~/.local/bin/cc-*",
                host: HostScope::Either,
                note: Some(
                    "cc-bus 自己的安装脚本软链的 11 条命令——**不是 cc-monitor 建的**，                     我们只在钩子诊断时查 cc-register / cc-bus-stop-hook 存不存在。                     注意本页这个 glob 还会数到 cc-acct-iso 的同前缀软链，所以计数偏大 1",
                ),
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
                // 〔C4e 订正〕这半句早已不成立（P4f / BS1b 起写面本机也走后端，〔C4e〕起由界面经通道直接说，本机与远端同一条路）；
                // 所以 note 里把「读」与「写」分开说，别让人以为本机那个也会被写。
                host: HostScope::Either,
                note: Some(
                    "运行期状态：inbox / 名册 / 队列 / 日志。驾驶舱**读**它（P4a 起本机也读）；\
                     但**写**面（发消息 / 派活 / 广播 / 收掉）至今只动**远端**那份 —— 本机没有对侧",
                ),
                effect: TouchEffect::IndirectWrite,
            },
            ],
        }],
    },
    ToolSpec {
        id: "cc-acct-iso",
        display_name: "cc-acct-iso 多账号隔离",
        installable: true,
        uninstallable: false,
        carriers: &[Carrier {
            what: "vendored 那一份（带 `.vendor_id` 指纹），部署到远端你自己填的那个目录",
            source: ToolSource::Vendored {
                repo_path: "src/bridge/vendor/cc-acct-iso",
                fingerprint_file: ".vendor_id",
            },
            destination: ToolDestination::UserConfiguredPath {
                token: "$ACCT_ISO_DEST",
                what: "部署时在账号页填的「部署目录」",
            },
            touches: &[
            TouchedFile {
                path: "$ACCT_ISO_DEST",
                host: HostScope::Remote,
                note: Some(
                    "远端，部署目录由你在账号页填的那个值决定（deploy_remote_acct_iso 的 dest_dir）",
                ),
                effect: TouchEffect::OwnedFile,
            },
            TouchedFile {
                path: "~/.claude-alt/",
                // **`Either`**——这一条我改了两次，第二次也不对（T04 审计重要 2）。
                //
                // 第一版标 `Client`：错，`accounts.rs` 的账号库列举全是
                // `list_remote_accounts(origin)` 与「某会话属哪个账号」那一条（〔C4a〕今天经通道 `accounts-sessions`），走 ssh exec。
                // 第二版改 `Remote`：也不对——本机 `CLAUDE_CONFIG_DIR` 会**指进这个目录**
                // （这台机器上就是 `~/.claude-alt/z`），`hooks_diag::claude_config_dir` 与
                // `config_surface` 自己都在读它，`ConfigSurfaceReport.claude_config_dir` 更是
                // 直接把它打印出来。于是同一页会**自相矛盾**：顶部写着解析基准是
                // `<用户家目录>/.claude-alt/<账号>`，而这一行写着「位置：远端」。
                //
                // 按 `Either` 的定义（"Claude Code 跑在哪台，这东西就在哪台"）它本就是两端皆可。
                host: HostScope::Either,
                note: Some(
                    "账号库：列举走远端 ssh；本机 CLAUDE_CONFIG_DIR 也可能指进来（两端都可能有）",
                ),
                effect: TouchEffect::ReadOnly,
            },
            ],
        }],
    },
    // ═══ 🔴 〔`K-R81` 09-12〕**这一条改名了，而改名不是清洁工作** ═══
    //
    // 用户 09-12 逐字：「**一个后端要两处使用 / 即远程后端就是远程本地机器的后端**」。
    // 这一条先前叫 `remote-daemon` / 「远端后端」，而它说的是**假的**：
    // 那一份不是「远端的后端」，是**那台机器的本地后端**（`K36` 逐字
    // 「两份后端应该要一样的」·`K33` 逐字「后端只有一个」）。
    //
    // 🔴 **这个名字已经花过一次真钱**：`build-linux` 没有 `Stage native backend for
    // self-extract` 那一步而 `build-windows` 有 ⇒ **Linux 裸 exe 装出来没有本机后端**
    // （`ROADMAP#KU26`，用户已裁「那肯定带」）。★ 那正是「把它当成『远端』产物」的直接后果 ——
    // **名字塑造了发版流水线的形状**。⚠ 发版那一步归 `K-R42`，本件只把名字与闭集说对。
    ToolSpec {
        id: "backend",
        display_name: "cc-monitor 后端（一份实现，本机与远端各跑一份）",
        installable: true,
        // 🔴 〔`K-R63` 09-11〕**这一格原先是 `false`，而它是一处假申报** —— 本件那条新性质
        // 落地的当场把它逮出来的（不是人看出来的）。卸载实现一直在：
        // `sftp.rs::uninstall_remote_backend` 是**设置面板「卸载后端」按钮**背后那条命令
        // （删后端二进制 + 同目录 `.build_id`，`is_safe_remote_backend_path` 守着）。
        // ⇒ 少报一格的后果与多报同向：配置面那一列「能否装/撤」直接印给用户看，
        //   写着「卸不掉」而按钮就在旁边。这正是 `K-R60` 在 `installable` 上治过的同一族病，
        //   只是这一次错在**少报**那一边（`K-R60` 那次是多报）。
        uninstallable: true,
        // 🔴 〔`K-R81` 09-12〕**同一份后端，三种载体、三个落点** —— `K-R68` 摸底现打
        // （`tests/evidence/K-R68-三种载体摸底.md#§B`）。在本件之前闭集里只有中间那一条，
        // 另外两条**一格都没有**：读者分不出「没有」与「有人忘了写」。
        // ⚠ 三者是**同一份代码**的三种载体，不是三个工具（`R24` 裁定一 / `K25` / `K33` / `K36`）；
        //   ①③ 是同一次 `cargo build` 的同一个文件拷两份，② 结构上不可能同字节
        //   （另一个 job / musl target）—— **那正是 `K25` 裁的形状，不是缺陷**。
        carriers: &[
            Carrier {
                what: "安装包放在 app 可执行文件旁边的那一份（`tauri.sidecar.conf.json` 的 `externalBin`）—— 解析次序里**第一个**被采用的就是它（`local_backend::resolve_beside_this_exe`）",
                source: ToolSource::EmbeddedBinary {
                    repo_path: "src/bridge/binaries/cc-monitor-backend",
                },
                // **路径不是常量，也不是家目录相对** —— 它跟着 app 装到哪儿走，
                // 而那个目录是装机时由人选的。同 `$BACKEND_PATH` 那一格的理由：
                // 申报一个我们其实没在用的常量，审计页会拿它去查一个没人写的路径
                // 然后言之凿凿地报「缺失」。
                destination: ToolDestination::UserConfiguredPath {
                    token: "$APP_DIR",
                    what: "装机时安装向导里选的那个安装目录（local_backend 与 cc-monitor 主程序同目录）",
                },
                touches: &[TouchedFile {
                    path: "$APP_DIR",
                    host: HostScope::Client,
                    note: Some(
                        "本机，随安装包落盘；文件名带 target triple（`cc-monitor-backend-<triple>`，\
                         Windows 上再带 `.exe`）—— 名字的真相源是 `local_backend::resolve_with`",
                    ),
                    effect: TouchEffect::OwnedFile,
                }],
            },
            Carrier {
                what: "这一份产物**自己带着**、旁边没有本机后端时自释放出来的那一份（`build.rs::embed_native_backend` ⇒ `byte_table.rs` 的 `include_bytes!`）",
                source: ToolSource::EmbeddedBinary {
                    repo_path: "src/bridge/native-backend/cc-monitor-native",
                },
                // 落点带 build_id（`local_backend::local_extract_name`）—— 那不是命名品味：
                // 远端自部署落的也是这个目录，两边对同一个文件名有不同期望就会互判 stale、
                // 无限重装。glob 只在末段、只有一个 `*`（`resolve_local_home` 的两条校验）。
                destination: ToolDestination::LocalHomeRelative(".cc-monitor/bin/cc-monitor-backend-*"),
                touches: &[TouchedFile {
                    path: "~/.cc-monitor/bin/cc-monitor-backend-*",
                    host: HostScope::Client,
                    note: Some(
                        "本机自释放出来的那份后端，文件名带 build_id（`local_extract_name`）——\
                         同一个 build_id 不会重复写；**旧版本不回收**，那笔债记在 `local_extract_name` 头注",
                    ),
                    effect: TouchEffect::OwnedFile,
                }],
            },
            Carrier {
                what: "推给远端那台机器、在**那台机器上当本地后端**跑的那一份（`embedded-backends/`，交叉编译的 musl 二进制）",
                source: ToolSource::EmbeddedBinary {
                    repo_path: "embedded-backends",
                },
                destination: ToolDestination::UserConfiguredPath {
                    token: "$BACKEND_PATH",
                    what: "每个远端连接的「backend 路径」配置项",
                },
                touches: &[TouchedFile {
                    path: "$BACKEND_PATH",
                    host: HostScope::Remote,
                    note: Some(
                        "路径由该连接的「backend 路径」配置项决定——**不是**固定的 ~/.local/bin/ccm-backend。\
                         ⚠ 它在那台机器上就是**那台机器的本地后端**（`K36`），\
                         「远端」说的是「相对这台 monitor」，不是它的身份",
                    ),
                    effect: TouchEffect::OwnedFile,
                }],
            },
        ],
    },
    // 〔TL1 · 4C〕**代码全景小程序**（`cc-monitor-panorama`，`src/panorama-engine`）—— RM1f 起它是**有落点的部署物**：
    //   monitor 摘掉了内嵌引擎（V108 后半句），本机与远端的全景都由那台机器的后端经插件口起它。
    //   同 `backend` 那一形：一份代码、两个载体 —— 本机那份由 `panorama_bytes::place_local` 放、远端那份由 `panorama_bytes::push_to` 推，
    //   都只在那台后端答「没装 / 太旧」时才放（V108「只传给开过远端全景的机器」）。
    //   `uninstallable: false`：今天没有「卸掉全景组件」这条口（卸后端那条 `sftp.rs::uninstall_remote_backend` 不碰它），如实声明。
    //   〔墓碑 —— 这之前它住 [`NOT_MANAGED`]，理由是「vendored 进 monitor 二进制、没有落点」；那个身份 RM1f 起没了，见那一条。〕
    ToolSpec {
        id: "panorama",
        display_name: "代码全景组件（只装全景引擎的小程序，本机与远端各放一份）",
        installable: true,
        uninstallable: false,
        carriers: &[
            Carrier {
                what: "放在本机的那一份：这一份产物按本机系统带着的原生小程序（Linux 上没带原生的就用 musl 那份），本机后端第一次答「没装」时放下来（`panorama_bytes::place_local`）",
                source: ToolSource::EmbeddedBinary {
                    repo_path: "src/bridge/native-backend/cc-monitor-panorama",
                },
                destination: ToolDestination::LocalHomeRelative(".cc-monitor/bin/cc-monitor-panorama"),
                touches: &[TouchedFile {
                    path: "~/.cc-monitor/bin/cc-monitor-panorama",
                    host: HostScope::Client,
                    note: Some(
                        "本机后端找它的第二个候选（后端 `control/panorama.rs::fixed_candidates`）；Windows 上文件名带 `.exe`。\
                         字节与盘上那份逐字节相同就不重写（`local_backend::place_local_panorama`）",
                    ),
                    effect: TouchEffect::OwnedFile,
                }],
            },
            Carrier {
                what: "推给远端那台机器的那一份（`embedded-backends/` 里交叉编译的 musl 小程序，按那台的系统与架构挑），那台后端答「没装 / 太旧」时推过去（`panorama_bytes::push_to`）",
                source: ToolSource::EmbeddedBinary {
                    repo_path: "embedded-backends",
                },
                destination: ToolDestination::RemoteHomeRelative(".cc-monitor/bin/cc-monitor-panorama"),
                touches: &[TouchedFile {
                    path: "~/.cc-monitor/bin/cc-monitor-panorama",
                    host: HostScope::Remote,
                    note: Some(
                        "只推给开过远端代码全景的机器；经那台的本机常驻后端文件链路写，写完读回逐字节比对（`sftp::upload_verified`）",
                    ),
                    effect: TouchEffect::OwnedFile,
                }],
            },
        ],
    },
    ToolSpec {
        id: "project-mcp",
        display_name: "项目 MCP 配置",
        installable: true,
        uninstallable: true,
        carriers: &[
            Carrier {
                what: "现场生成的一段 JSON，写进你选定的那个项目目录",
                source: ToolSource::Generated,
                destination: ToolDestination::ProjectRelative(".mcp.json"),
                touches: &[TouchedFile {
                    path: ".mcp.json",
                    host: HostScope::ProjectDir,
                    note: Some("相对你选定的项目目录"),
                    effect: TouchEffect::OwnedFile,
                }],
            },
            // 〔AS1 · 第四波 4B〕**推 / 拉**（`设计/96` 的 B，用户 09-24 V111 · V112）：同一份文件的**第二个写入来源** ——
            //   内容不是这台机器上现场编的，是从另一台机器那份里**原样**拷来的条目（`mcp_sync.rs`）。
            //   落点、写法（经那台后端 `files-put`）与上一格同一个；单列一格是为了让「这个 app 动过你哪些文件」
            //   那一页说得出「有些条目是从别的机器搬来的」（`96 §4`：每个写点都要在足迹里可见）。
            //   远端那台的足迹栏按那台机器问（RM1a），这一格的 `host` 与上一格同是项目目录 —— 在哪台上就算哪台的。
            Carrier {
                what: "推 / 拉：另一台机器那份 .mcp.json 里你勾的条目，原样合进这台机器上你选定的项目目录",
                source: ToolSource::Generated,
                destination: ToolDestination::ProjectRelative(".mcp.json"),
                touches: &[TouchedFile {
                    path: ".mcp.json",
                    host: HostScope::ProjectDir,
                    // 〔AS2 · 4B〕资产目录那一块的「装到这台」（MCP）走的就是这一格（同一条命令 `mcp_sync_apply`，只勾那一条）。
                    note: Some("相对你在推 / 拉那一块（或资产目录的「装到这台」）里选定的项目目录；只加 / 盖你勾的那几条，别的条目不动"),
                    effect: TouchEffect::OwnedFile,
                }],
            },
        ],
    },
    // 〔AS2 · 第四波 4B · V113〕**skill「装到这台」**：资产目录里别的机器有的 skill，用户点了才装到这台 ——
    //   文件原样从来源那台拷来（V112），写经这台后端 `files-put`（带 `expect`，`skill_install.rs`）。
    //   `96 §4`：每个写点都要在足迹里可见。落点由用户点的那一条决定（这台 skills 下以那个名字为名的目录）⇒ 占位符，不猜。
    //   〔SU1 · 第四波 4C · V116〕`uninstallable: true`：用户裁「要，只删装时写进去的文件」—— 装的时候那台后端记下写了哪几个
    //   （第二条 touch：那台后端自己的装记录），卸口 `skill_install.rs::skill_uninstall_apply` 只删记着的那几个（装完改过的先问）。
    //   〔墓碑 —— AS2 那一版这里是 `uninstallable: false`（「没有卸掉装来的 skill 这条口，如实声明」）。〕
    ToolSpec {
        id: "skill-install",
        display_name: "从别的机器装来的 skill",
        installable: true,
        uninstallable: true,
        carriers: &[Carrier {
            what: "资产目录里你点了「装到这台」的那个 skill：另一台机器上那个 skill 目录里的文件（你勾的那几个），原样写进这台",
            source: ToolSource::Generated,
            // 落在 skills 目录下（以那个 skill 为名的那一个子目录；名字由你点的那一条定）—— 与 cc-bus 那一格同一个根。
            destination: ToolDestination::LocalHomeRelative(".claude/skills"),
            touches: &[TouchedFile {
                path: "~/.claude/skills",
                host: HostScope::Either,
                note: Some(
                    "装到哪台就写哪台，只写 skills 下以你点的那个 skill 为名的那一个目录；\
                     只写你勾的那几个文件（不同的要你点了「盖」才盖），别的文件不动；\
                     卸的时候只删装时写进去的那几个（装完你改过的、装之前就在的先问你），目录本身留着",
                ),
                effect: TouchEffect::OwnedFile,
            }, TouchedFile {
                path: "~/.cc-monitor/skill-installs.json",
                host: HostScope::Either,
                note: Some(
                    "装到哪台就记在哪台：那台后端自己的装记录（每个装写进去的文件的摘要 ＋ 装之前在不在），卸只认这里记着的；\
                     卸掉的从这里摘掉",
                ),
                effect: TouchEffect::OwnedFile,
            }],
        }],
    },
    // ═══ 〔`K-R62` 09-11〕**从第三档升上来的第一项** ═══
    //
    // 它昨天还住在 [`UNMANAGED_ENV`]（`app 假设它在`），`why` 那一格逐字写着
    // 「加与删两侧都只造 PowerShell 那两条 profile 路径，POSIX rc 一条都不扫」。
    // 本件把那句话变成了假的：`profile_installer::plan_install` 的 `PosixRc` 臂把那个
    // 别名块装进用户**选定**的 rc（内容与远端那个口来自同一个常量 `sftp::CCM_WRAPPER_SNIPPET`，
    // 合块与剥块都借 `sftp::merge_profile_block` / `sftp::strip_profile_block`），
    // `profile_installer::plan_uninstall` 卸得掉，`profile_installer::scan_profile` 查得出。
    //
    // 🔴 **升档本身是一条可验的性质**，不是一句话：`installable: true` ⇒ [`environment`]
    // 把它算成 [`EnvTier::AppInstalls`]，判据是 `posix_rc_aliases_sits_in_the_first_tier_now`。
    // 档没升 = 活没做完 —— 这一格从此有人数着。
    ToolSpec {
        id: "posix-rc-aliases",
        display_name: "POSIX rc 里的 ccm 别名块（cc / cct / alphacc …）",
        installable: true,
        uninstallable: true,
        carriers: &[Carrier {
            what: "仓里那份别名脚本（`src/shared/ccm-aliases.sh`），合进你自己选的那份 rc",
            // 装进去的内容**就是仓里那份文件**（`sftp::CCM_WRAPPER_SNIPPET` 是它的
            // `include_str!`）。远端那条 `ccm` 用的是同一份 —— 那正是本件不许出现第二份的东西。
            source: ToolSource::EmbeddedText {
                repo_path: "src/shared/ccm-aliases.sh",
            },
            // 🔴 **路径由人选，产品不猜** —— 这一格用占位符而不是 `~/.bashrc`，
            // 理由与后端那条 `$BACKEND_PATH` 逐字同源：申报一个我们其实没在用的常量，
            // 审计页会拿它去查一个没人写的路径然后言之凿凿地报「缺失」。
            // `.bashrc` / `.zshrc` / `config.fish` 写法不同，替人选一份是最坏的那条路
            // （`account_aliases` 的 `§0e`）。
            destination: ToolDestination::UserConfiguredPath {
                token: "$POSIX_RC",
                what: "「按账号生成命令」那一块里那个 rc 下拉 —— 从盘上真实存在的 .bashrc / .zshrc / .bash_profile / .profile 里由你自己选",
            },
            touches: &[TouchedFile {
                path: "$POSIX_RC",
                host: HostScope::Client,
                note: Some(
                    "本机（cc-monitor 跑着的这台）的那份 shell rc，具体哪一份由界面上的人选。\
                     围栏与内容都与远端那个口共用一份 —— 本机与远端装进 rc 的是同一个东西（K15 / K36）",
                ),
                effect: TouchEffect::FencedBlock,
            }],
        }],
    },
    ToolSpec {
        id: "powershell-profile",
        display_name: "PowerShell 集成",
        installable: true,
        uninstallable: true,
        carriers: &[Carrier {
            what: "现场生成的那段 PowerShell 围栏块，合进用户的 `$PROFILE`",
            source: ToolSource::Generated,
            destination: ToolDestination::UserShellProfile,
            touches: &[TouchedFile {
                path: "$PROFILE",
                host: HostScope::Client,
                note: Some("Windows 客户端侧，具体路径由 PowerShell 决定"),
                effect: TouchEffect::FencedBlock,
            }],
        }],
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
        display_name: "Claude Code 的会话记录",
        installable: false,
        uninstallable: false,
        carriers: &[Carrier {
            what: "别人的产物 —— Claude Code 自己建、自己写的那份会话记录，我们只读",
            source: ToolSource::NotOurs {
                who: "Claude Code 自己建、自己写",
            },
            destination: ToolDestination::NotInstalledByUs {
                whose: "Claude Code 的数据根（`adapter/claude_code.rs` 的 CLAUDE_LAYOUT.sessions_subdir）",
            },
            touches: &[TouchedFile {
                path: "~/.claude/projects/",
                host: HostScope::Either,
                note: Some(
                    "历史面与搜索索引的全部输入（每个项目一个目录、每个会话一份 jsonl）——\
                     我们只读；装不了，也不该我们装",
                ),
                effect: TouchEffect::ReadOnly,
            }],
        }],
    },
];

/// 🔴 `K-R132`：本机那条 `ccm` 入口**所在的目录**（home 相对，`/` 分隔，不带末尾斜杠）。
///
/// # 它**不是**一个新的字面量〔`13b`：闭集只许有一个住址〕
///
/// 它从 [`TOOLS`] 里 `ccm` 那条**本机载体**的落点**现算**
/// （`LocalHomeRelative(".cc-monitor/bin/ccm*")` ⇒ `".cc-monitor/bin"`）。
/// 本函数体内一个 `.cc-monitor` 都没有 —— 改了上面那张表，这里跟着变。
///
/// # 为什么要有它：**PATH 上要写的那个目录，必须与我们真放下去的那个是同一个**
///
/// `K-R129` 在真机上证实的缺陷是「装上了、能跑、用户敲不到」——
/// `ccm.exe` 真在 `%USERPROFILE%\.cc-monitor\bin\`，而那个目录不在 PATH 上。
/// 补 PATH 的那一步（`profile_installer::render_cc_code`）**不许自己写一个目录字面量**：
/// 写了，它与真落点就是两个住址，哪天落点搬家，PATH 会指着一个空目录，
/// 而「指着空目录」与「根本没补」在终端上一模一样。
///
/// # 取不到就是 `None`，**不兜默认值**
///
/// 兜一个默认值 = 上面那张表被改坏了也看不出来，而那正是本函数要买的东西。
/// 调用方拿到 `None` 时**不发明一个目录**：`profile_installer` 那两条用户级 PATH 命令
/// 会原样往上传 `None`（**一条命令都不吐**），界面那一格显示「拿不到落点」
/// 而不是一个编出来的目录 —— **指错目录与根本没补，在用户终端上是同一个结果**。
///
/// ⚠ **诚实边界**：它答的是「**表里申报的**本机落点在哪个目录」，
/// 不是「盘上那份**真的**在哪」。两者对不对得上由
/// `profile_installer` 那条跨文件判据钉住（它去读真正调
/// `install_local_ccm_entry` 的那一行源码）。
pub fn local_ccm_bin_dir_rel() -> Option<&'static str> {
    let mut found: Option<&'static str> = None;
    for spec in TOOLS {
        // ⚠ `"ccm"` 这里是**这张表自己的键**（上面那条 `ToolSpec { id: "ccm", … }`），
        //    **不是** `local_backend::CCM_ENTRY_WORD` 那个命令名的第二个住址。
        //    两者今天字面相同是巧合 —— 拿命令名来查表，是把「注册表的键」与
        //    「终端里敲的那个词」当成同一件事，那正是本工作区最贵的那个病
        //    （一个值装了两件事）。
        if spec.id != "ccm" {
            continue;
        }
        for carrier in spec.carriers {
            if let ToolDestination::LocalHomeRelative(p) = carrier.destination {
                // 末段是文件名（可能带 glob，见那一条的 `path` 注释）⇒ 砍掉它。
                let (dir, _last) = p.rsplit_once('/')?;
                if found.is_some() {
                    // 同一个工具声明了两条本机落点 ⇒ 「那个目录」这个问题没有唯一答案，
                    // 而**猜一个**正是这一格不许做的事。
                    return None;
                }
                found = Some(dir);
            }
        }
    }
    found
}

/// 🔴 `KR135D3`：**远端那条 `ccm` 落点的目录**（`$HOME` 相对，末段文件名已砍掉）。
///
/// 与 [`local_ccm_bin_dir_rel`] 是**同一个问题的另一边**，所以形状逐字照它：
/// 从这张表现算，不写死目录字面量（`13b`：闭集只许有一个住址）。
///
/// # 为什么本机那个不够用，非要把远端这个也取出来
///
/// `src/shared/ccm-aliases.sh` 是**一份文件、两个消费者**（本机 rc 与远端 rc 合的是
/// 逐字同一份文本），而两边的落点**不是同一个目录** ⇒ 那一行里两个目录都得在。
/// 判据要判「两个都在」，就得两个都能从这张表问出来 ——
/// 在判据里手抄一个 `.local/bin` 就是第二个住址，而那正是本病的成因。
///
/// 两条同名落点 ⇒ 回 `None`（「那个目录」没有唯一答案时**不许猜一个**，同本机那条）。
// 🔴 〔`K-R135` 09-15〕**它今天的使用者只有判据，所以住在判据档里，而不是挂一个
// `#[allow(dead_code)]` 把警告压掉。** 两者的差别是**下一个人读得出什么**：
// `allow` 说的是「有人用，只是编译器看不见」，而这一档说的是「**今天只有判据用它**」——
// 后者才是实话。⚠ 它不是可有可无的：判据要证「那一行把**两边申报的**目录都放上了 PATH」，
// 而在判据里手抄一个 `.local/bin` 就是那个落点的第二个住址 —— 正是本病的成因。
// ⇒ 哪天生产侧真要问「远端那个目录是哪个」，把这一行 `#[cfg(test)]` 摘掉即可。
#[cfg(test)]
pub fn remote_ccm_bin_dir_rel() -> Option<&'static str> {
    let mut found: Option<&'static str> = None;
    for spec in TOOLS {
        // 同 `local_ccm_bin_dir_rel`：`"ccm"` 是**这张表自己的键**，
        // 不是 `local_backend::CCM_ENTRY_WORD` 那个命令名的第二个住址。
        if spec.id != "ccm" {
            continue;
        }
        for carrier in spec.carriers {
            if let ToolDestination::RemoteHomeRelative(p) = carrier.destination {
                let (dir, _last) = p.rsplit_once('/')?;
                if found.is_some() {
                    return None;
                }
                found = Some(dir);
            }
        }
    }
    found
}

// ═══════════════════════════════════════════════════════════════════════════
// `K-R60`：**环境清单的闭集** —— 「app 要的东西齐了没有」这个问题的人群
// ═══════════════════════════════════════════════════════════════════════════

/// 🔴 〔`K38` / `KR65D2`〕**「谁该装」** —— 与「今天装得了吗」是两个问题，两格分开装。
///
/// # 为什么非拆不可
///
/// 拆之前只有一个 `ToolSpec::installable`，而它同时被当成两句话读：
///   · 「**今天盘上有没有一个装口**」—— 读的是**实现**，`K-R63` 那条对拍表钉死了它；
///   · 「**这东西该不该由我们装**」—— 读的是**判断**，`K38` 裁的正是它。
///
/// 于是 `cc-acct-iso-local` 那种「**该由 app 装（`K38`），而实现还没有**」的状态
/// **一格都申报不了**：写 `true` 是假申报（`K-R63` 当场逮到），
/// 写 `false` 等于说「不该我们装」（与 `K38` 矛盾）。⇒ 两句话各给一格。
///
/// # 🔴 「app 自带的是哪几样」这个人群的**唯一住址**就是 [`Provisioning::AppShips`]
///
/// `KR65D3`：要人数就 [`environment`] 里现算（`filter(who == AppShips)`），
/// **别处不许再手抄一张自带清单** —— 那是第二个住址〔`13b`〕。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
pub enum Provisioning {
    /// **app 自带** —— 「和这个 app 相关的东西、独特的东西」（`K38` 逐字）。
    AppShips,
    /// **用户自己装** —— 通用工具（`K38` 逐字点名 `claude` / `tmux` / `git` / `ssh`）。
    /// app **不装**，但**要去查**；缺了要出声（`KR65D1`）——「提示」不是「假设它在」。
    UserProvides,
    /// **谁都不「装」它** —— 别人的产物（Claude Code 自己建自己写的那份会话记录）。
    /// 与 `UserProvides` 的差别是真的：那一档缺了该劝人去装，这一档缺了没人装得出来。
    NotAnInstall,
}

impl Provisioning {
    /// 三值的**闭集**本身。现算用〔`13b`：报一个基数也是复述〕。
    pub const ALL: &'static [Provisioning] = &[
        Provisioning::AppShips,
        Provisioning::UserProvides,
        Provisioning::NotAnInstall,
    ];

    /// 给人看的措辞。定在这里，UI 与诊断文本不再各写一遍。
    pub fn label(self) -> &'static str {
        match self {
            Provisioning::AppShips => "app 自带",
            Provisioning::UserProvides => "你自己装",
            Provisioning::NotAnInstall => "不是装出来的",
        }
    }

    /// `TOOLS` 那一半 —— **派生，不手填**：落点是 [`ToolDestination::NotInstalledByUs`]
    /// 就是「不是装出来的」，其余一律「app 自带」（`TOOLS` 收的本来就是
    /// **app 自己往别处放的东西**）。
    ///
    /// ⚠ 这一格与 `installable` **读的不是同一个东西**：这里读 `destination`（判断），
    /// 那里读实现（`K-R63` 的对拍表）。`claude-code` 两格恰好同向，那是巧合不是同义。
    ///
    /// # 🔴 〔`K-R81` 09-12〕多载体之后的**聚合规则**，写在这里而不是靠人记得
    ///
    /// 一个工具现在有**一串**载体（[`Carrier`]）⇒ 「它是不是我们装的」要从一串
    /// `destination` 聚合出来。**规则：全部载体都是 `NotInstalledByUs` 才算「不是装出来的」。**
    /// 理由：只要有**一份**是我们放下去的，这东西对用户就是「app 装的」——
    /// 把它算成「别人的产物」会让配置面那一列直接说假话。
    ///
    /// ⚠ **混合的那一形今天盘上不存在**（判据 `carriers_do_not_mix_ours_and_not_ours`
    /// 钉着，两向都判）。它**本来就该变的时候去哪儿重裁**：真出现一个「一半我们装、
    /// 一半别人的」的工具，那条判据会当场红并点名 —— 那时重裁的是这条聚合规则本身，
    /// 不是把判据放宽（`references/testing.md` 硬规则 11 / 12）。
    pub fn of_tool(t: &ToolSpec) -> Provisioning {
        let all_not_ours = t
            .carriers
            .iter()
            .all(|c| matches!(c.destination, ToolDestination::NotInstalledByUs { .. }));
        if all_not_ours {
            Provisioning::NotAnInstall
        } else {
            Provisioning::AppShips
        }
    }
}

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
/// # 🔴 〔`K-R65` 09-11〕`AppAssumesPresent` 那一档**删了 —— 这是它的墓碑**
///
/// 原文逐字：「**app 假设它在** —— 既不装也不查，用的时候直接假设它已经在。」
/// 而 `config_surface::unmanaged_row` 把这句话实现成了「`state` 恒 `Undetermined`、
/// `path_resolved` **故意不解析**（解析了就等于查了）」。
///
/// `K38` 把这一档判掉了：通用工具**不是**「我不看」，是「**你自己装，而我会看、缺了我要说**」
/// ⇒ 原来住在那一档的 10 项一项不剩（9 项去 [`EnvTier::UserInstallsWePrompt`]、
/// `cc-acct-iso-local` 去 [`EnvTier::AppShipsNoInstallerYet`]），
/// 而 `every_tier_has_members_so_absence_never_encodes_a_judgement` 自己的报错逐字写着
/// 「要么给它一个成员，**要么把这一档从 EnvTier 里删掉**」⇒ 删。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
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
    /// 四档的**闭集**本身。现算用（`len()` 就是「几档」，不许在别处写死一个基数）。
    pub const ALL: &'static [EnvTier] = &[
        EnvTier::AppInstalls,
        EnvTier::AppShipsNoInstallerYet,
        EnvTier::UserInstallsWePrompt,
        EnvTier::AppOnlyChecks,
    ];

    /// 给人看的档名。措辞定在这里，UI 与诊断文本不再各写一遍。
    pub fn label(self) -> &'static str {
        match self {
            EnvTier::AppInstalls => "app 装的",
            EnvTier::AppShipsNoInstallerYet => "app 该自带 —— 今天还没有装口",
            EnvTier::UserInstallsWePrompt => "你自己装 —— 缺了我们提示你",
            EnvTier::AppOnlyChecks => "app 只查",
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

/// 闭集里**派生不出来**的那一半：`TOOLS` 里没有对应条目的环境项。
///
/// # 为什么只有这一半是手写的
///
/// 有 [`ToolSpec`] 的那一半**能派生**：它的落点（`destination`）说得出「谁该装」，
/// 它的 `installable`（`K-R63` 钉在真实现上）说得出「今天有没有装口」。
/// 而这一半派生不出来 —— 盘上**没有任何字段**能把「这东西该由我们装」与
/// 「该用户自己装」分开，那是一个**设计判断**，不是读数。
/// ⇒ 判断必须有住址，这张表就是它的住址。
///
/// ⚠ **别把「今天这台机器上恰好有」写成一个 `who`** —— 前者是读数（`K-R57` 量具 A 量的那种），
/// 后者是设计判断。本表只收后者，所以每一条的 `why` 要给**代码里的住址**，不是一句形容。
pub struct UnmanagedEnv {
    pub id: &'static str,
    pub display_name: &'static str,
    /// 🔴 〔`K-R65`〕**「谁该装」** —— 这一格取代了原来那个手填的 `tier`。
    ///
    /// 原文逐字：「`tier: EnvTier`／这一项属于哪一档。**必须写出来** —— 第三档就是靠这一格
    /// 存在的。」那时档是**手填**的，于是「档」这一个字段同时装着「谁该装」与
    /// 「今天装得了吗」两件事。今天档由 [`EnvTier::of`] 从这一格 ＋「有没有 `ToolSpec`」
    /// 派生出来，**手填不了**。
    pub who: Provisioning,
    /// **怎么查它在不在。** `KR65D1` 的正题住这一格 —— 见 [`EnvProbe`]。
    pub probe: EnvProbe,
    /// 用什么名字指认它：PATH 上的命令名、一条 `~/` 路径、或一个 `$占位符`。
    pub named: &'static str,
    /// 它在哪台机器上（与 [`TouchedFile::host`] 同一套语义）。
    pub host: HostScope,
    /// **app 在哪儿用到它** —— 结尾必须是一个 `<相对 src 的路径>.rs::<符号>` 形态的住址。
    /// 判据只判「**有没有**住址」；那个住址今天解析不解析得到，由 `structural_scan` 里
    /// 那条扫全仓代码住址的判据管（它会报「找不到这个符号 / 符号搬家了」）。
    pub why: &'static str,
}

/// 手写那一半**怎么查**。
///
/// # 🔴 `KR65D1` 的正题住这里：「提示」= **我看，而且缺了我要说**
///
/// `K-R60` 把第三档的语义写死成「**我们压根没去查**」（`state` 恒 `Undetermined`、
/// `path_resolved` **故意不解析**，理由逐字「解析了就等于查了」）。
/// `K38` 之后那一档不存在了 ⇒ 这一格申报的是**查法**，不是「查不查」。
///
/// # ⚠ [`EnvProbe::CannotProbe`] **不是「不查」**
///
/// 它是「**查了，查不动**」。两者在这一页上显示成两回事：
/// 「查了、确认没有」= `SurfaceState::Absent`；「查不动」= `SurfaceState::Undetermined { why }`。
/// 这条分法**是现成的** —— 前端 `readiness.ts` 的 `missing` / `unknown` 就是它，
/// **不许再造一套**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvProbe {
    /// `PATH` 上的一个裸命令 —— 走 `hooks_diag.rs::resolves_on_path`。
    ///
    /// **用已有那一把，不新写一个 `which`**：它已经把两条坑填了 ——
    /// 切分必须走 `std::env::split_paths`（Windows 的 `;` 与盘符冒号），
    /// 以及「取不到 `PATH` 就返回 `None`（**不猜**）」。
    OnPath,
    /// 一条 `~/` 路径 —— 走 `config_surface.rs::resolve_touched_path` 那条既有的本机解析。
    HomePath,
    /// 查不动，**理由必填**：值由别处决定（占位符 / 用户配置），本页不猜。
    ///
    /// ⚠ 填这一支之前先问一遍：是真的查不动，还是**懒得查**？后者写在这里就是
    /// 拿「查不动」当「不查」的遮羞布 —— 那正是本件在治的病。
    CannotProbe { why: &'static str },
}

/// 闭集里手写的那一半。
///
/// 🔴 〔`K-R65` 09-11〕**这张表原来「全是第三档」，今天一条都不是** ——
/// `K38` 之后它分成了两群：9 项通用工具是 [`Provisioning::UserProvides`]，
/// `cc-acct-iso-local` 是 [`Provisioning::AppShips`]（app 独有、该我们装，而装口还欠着）。
pub const UNMANAGED_ENV: &[UnmanagedEnv] = &[
    UnmanagedEnv {
        id: "claude-cli",
        display_name: "Claude Code 的可执行文件",
        who: Provisioning::UserProvides,
        probe: EnvProbe::OnPath,
        named: "claude",
        host: HostScope::Either,
        why: "起会话时拿它当启动器直接用；装不装、在哪个版本，app 一概不问 —— \
              adapter/claude_code.rs::default_launcher",
    },
    UnmanagedEnv {
        id: "tmux",
        display_name: "tmux（会话容器）",
        who: Provisioning::UserProvides,
        probe: EnvProbe::OnPath,
        named: "tmux",
        host: HostScope::Either,
        why: "tmux 容器那条起法要它；缺了只在回绝里报一句能力名 —— \
              backend/control/ccm_invocation.rs::Refusal",
    },
    UnmanagedEnv {
        id: "terminal-exit",
        display_name: "POSIX 终端出口",
        who: Provisioning::UserProvides,
        probe: EnvProbe::OnPath,
        named: "xdg-terminal-exec",
        host: HostScope::Client,
        why: "POSIX 上开一个会话窗口只认这一个规范化出口，表里今天就它一项 —— \
              launch.rs::TERMINAL_EXITS",
    },
    UnmanagedEnv {
        id: "git",
        display_name: "git",
        who: Provisioning::UserProvides,
        probe: EnvProbe::OnPath,
        named: "git",
        host: HostScope::Client,
        why: "认 skill 所在的工作树与主检出要跑它 —— skill_host_tests.rs::git_common_dir",
    },
    UnmanagedEnv {
        id: "ssh",
        display_name: "ssh 客户端（含密钥与主机配置）",
        who: Provisioning::UserProvides,
        probe: EnvProbe::OnPath,
        named: "ssh",
        host: HostScope::Client,
        why: "远端一整侧都经它；app 只探它在不在 PATH 上，装不了 —— \
              launch.rs::ssh_client_available",
    },
    UnmanagedEnv {
        id: "pgrep",
        display_name: "pgrep",
        who: Provisioning::UserProvides,
        probe: EnvProbe::OnPath,
        named: "pgrep",
        host: HostScope::Either,
        why: "数 cc-bus 的 agent 在不在用它 —— cc_bus_tests.rs::count_now",
    },
    UnmanagedEnv {
        id: "xdg-open",
        display_name: "xdg-open",
        who: Provisioning::UserProvides,
        probe: EnvProbe::OnPath,
        named: "xdg-open",
        host: HostScope::Client,
        why: "开链接 / 开日志目录走它 —— lib.rs::open_with_os",
    },
    UnmanagedEnv {
        id: "login-shell",
        display_name: "bash 登录 shell",
        who: Provisioning::UserProvides,
        probe: EnvProbe::OnPath,
        named: "bash",
        host: HostScope::Either,
        why: "探 ccm 能力时要一个登录 shell 把用户的 rc 读进来 —— ccm_probe.rs::probe_with",
    },
    UnmanagedEnv {
        id: "mcp-server",
        display_name: "MCP server 本体（`.mcp.json` 里那个 command）",
        who: Provisioning::UserProvides,
        // 🔴 **这一条是「查不动」而不是「不查」的活体** —— 它不是懒：
        // 那个 command 是**用户在 `.mcp.json` 里写的一行**，本页连是哪个项目都不知道
        // （`project-mcp` 的落点是 `ProjectRelative`）⇒ 名字本身就是个占位符，
        // 拿 `$MCP_COMMAND` 去 `PATH` 上找只会恒答「没有」，那是一句**自信的错答案**。
        probe: EnvProbe::CannotProbe {
            why: "这个名字由你项目里的 `.mcp.json` 那行 command 决定，而本页不猜是哪个项目 —— \
                  要查得先选定项目再读那份配置",
        },
        named: "$MCP_COMMAND",
        host: HostScope::ProjectDir,
        why: "我们写得了那份配置，**被它指到的可执行本体不装也不查** —— \
              mcp.rs::write_project_mcp_server",
    },
    // ═══ 🔴 〔`K-R65` 09-11〕**这一条是 `KR65D2` 的题面本身** ═══
    //
    // 它是 app 独有的东西（`K38` 逐字点名的「account」的**本机那半**）⇒ `AppShips`。
    // 而它今天**零装口** ⇒ [`EnvTier::of`] 把它派生成 [`EnvTier::AppShipsNoInstallerYet`]。
    //
    // ⚠ **`who` 与「有没有装口」是两格，别合回去**：写 `AppShips` 不等于说「装得了」，
    // 也不许被读成「不该我们装」。本件**不补那个装口**（`§0d`）—— 本件治的是
    // 「这个状态申报不出来」。
    UnmanagedEnv {
        id: "cc-acct-iso-local",
        display_name: "cc-acct-iso 本机那份",
        who: Provisioning::AppShips,
        // 零装口**不等于**零查口 —— 它是一条实打实的 `~/` 路径，查得动。
        // 〔`K-R60` 那句「本机侧零装口、零查口」里的后半句，正是本件要改掉的行为。〕
        probe: EnvProbe::HomePath,
        named: "~/.local/bin/cc-acct-iso",
        host: HostScope::Client,
        why: "本机侧零装口（`K38` 裁了该由 app 装，实现还欠着）；有装口的只有远端那半 —— \
              acct_iso_deploy.rs::check_remote_acct_iso",
    },
    // ═══ 🔴 〔`K-R65` 09-11〕**第三样「随产品分发的东西」—— 它此前一张表都没进** ═══
    //
    // 🔴 **〔条 67 · 2026-09-18〕`code-picture-sidecar` 这一项摘掉了。**
    //
    // 它申报的是「app 随产品分发的独立进程那一份代码全景」，身份来自
    // `src/backend/sidecars/` 那一层的头注。用户逐字「**不在现在设计里的全部删掉**」
    // ⇒ 那一层（2 008 行）整棵删了 ⇒ **这条申报变成了假话**，不是「暂时没接线」。
    //
    // ⚠ 留墓碑是因为它的来历本身是一条教训：`K38` 逐字只举了两样（`account` · `cc-bus`），
    //   这一项是**用户当拍凭记忆点出来的**（「不是还有 code picture 吗」）——
    //   「app 自带的是哪几样」这个人群此前真的没有住址。那个教训今天仍然成立。
    // ⚠ 与 [`NOT_MANAGED`] 里那条 `code-picture` **不是同一个东西**：那条当时说的是
    //   **vendored 进 monitor 二进制的 crate**（21 条命令、14 条在用），**它当时还在，没删**。
    //   同名两身份，删掉的是「独立进程那一份」。〔TL1 · 4C〕RM1f 起 vendored 那一身份也没了：引擎住进
    //   独立小程序 `cc-monitor-panorama`，由后端经插件口起 —— 那是上面 `id: "panorama"` 那一条（有落点的部署物），
    //   与这里删掉的「独立进程那一层」不是一回事（那一层是 app 随产品起的常驻侧车，这一份是按需起的一问一进程）。
    // 🔴 〔`K-R62` 09-11〕**`posix-rc-aliases` 从这里搬走了 —— 这是它的墓碑。**
    //
    // ⚠ 〔`K-R65` 09-11 补一句〕下面这段原文里那个档名**今天已经不存在了**（`K38` 删了
    // 「app 假设它在」那一档，墓碑在 `EnvTier` 的头注上）。原文照留，别改成今天的写法 ——
    // 这段墓碑记的就是「它当初住在哪一档」。
    //
    // 原文逐字：`tier: EnvTier::AppAssumesPresent` · `named: "~/.bashrc"` ·
    // `why: "加与删两侧都只造 PowerShell 那两条 profile 路径，POSIX rc 一条都不扫 ——
    //        profile_installer.rs::discover_profiles"`。
    //
    // 那句话今天是假的：`profile_installer::plan_install` 的 `PosixRc` 臂真装、
    // `profile_installer::plan_uninstall` 真卸、`profile_installer::scan_legacy_rc_lines`
    // 真查（而且够得着裸行 —— 围栏那条路够不着，那正是 `KR62D2` 的题面）。
    // ⇒ 它上面有了 `ToolSpec` ⇒ 档由 [`environment`] 读 `installable` 派生成
    // [`EnvTier::AppInstalls`]，**不再手写**。留这段墓碑是因为「它曾经在第三档」
    // 是这张表存在理由的最好例子：一个判断当初只能靠「进这张表」表达，
    // 做完之后它自己会从这张表里消失。
    //
    // ★ 同一档里 `cc-acct-iso-local` 仍在（「本机侧零装口、零查口」）——
    // 那是**二进制**不是 rc 行，不在 `K-R62` 射程（`§0e`）。
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
        /// 〔`K-R65`〕查法从 [`UnmanagedEnv::probe`] 原样带过来 ——
        /// 这一档从此**真去查**，不再是「我们压根没去查」。
        probe: EnvProbe,
    },
}

/// 闭集里的一项。
pub struct EnvEntry {
    pub id: &'static str,
    pub display_name: &'static str,
    /// **谁该装**（`K38`）。「app 自带」这个人群就是 `who == AppShips` 的那几项。
    pub who: Provisioning,
    /// **档** —— 由 `who` ×「今天有没有装口」两格**派生**（[`EnvTier::of`]），不是手填的。
    pub tier: EnvTier,
    pub why: &'static str,
    pub backing: EnvBacking,
}

/// 🔴 **环境清单的闭集 —— 唯一一份，现算。**
///
/// 「app 要的东西这台机器上齐了没有」这个问题的**人群**就是它。
/// `config_surface` 的那张表照它建（`the_view_population_is_exactly_the_closed_set` 钉住），
/// 别处要这份名单一律调它，**不许再手抄一张**〔`13b`：闭集只许有一个住址〕。
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
    let mut out: Vec<EnvEntry> = TOOLS
        .iter()
        .map(|t| {
            let who = Provisioning::of_tool(t);
            EnvEntry {
                id: t.id,
                display_name: t.display_name,
                who,
                tier: EnvTier::of(who, t.installable),
                why: "有 ToolSpec ⇒ 「谁该装」由 destination 派生、「今天有没有装口」读 \
                      installable —— tool_registry.rs::environment",
                backing: EnvBacking::Managed(t),
            }
        })
        .collect();
    out.extend(UNMANAGED_ENV.iter().map(|u| EnvEntry {
        id: u.id,
        display_name: u.display_name,
        who: u.who,
        // 🔴 **第二格是 `false` 而不是一个字段** —— 手写那一半没有 `ToolSpec`，
        // 也就没有落点、没有 `touches`、没有装 / 卸实现可对拍 ⇒ 「今天有没有装口」
        // 这个问题在这一半上**有唯一答案**，不该再开一个能填错的格子。
        tier: EnvTier::of(u.who, false),
        why: u.why,
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
pub const NOT_MANAGED: &[(&str, &str)] = &[
    (
        "code-picture",
        "**这个名字今天只剩一个身份不属本表：MCP server（code-picture 的 Agent head，给 Claude 用）** —— \
         它确实「装到别处」，但**装它走已有的 `project-mcp` 机制**（往 `.mcp.json` 加一个 server 条目），\
         是**用法**不是新工具。仓里今天对那个 MCP head 零实现（`mcp.rs` / `config_surface.rs` 里 `code-picture` 零命中）。\n\
         ⇒ 真要做「一键装 code-picture 的 MCP」属 **issue #51 第 1 部分**，\
         用户 08-10 明说「cc-bus 和 code-picture 后面再增强，现在先不做」。\n\
         〔TL1 · 4C〕它的另外两个身份都不在这里了：① **vendored 进 monitor 二进制的 crate**（原先本条的主理由：\
         「没有 `destination`、没有安装动作、卸载它等于重新编译 monitor」）—— RM1f 起 monitor 摘掉了内嵌引擎（V108 后半句），这个身份没了；\
         它变成只装引擎的独立小程序 `cc-monitor-panorama`（`src/panorama-engine`），本机放到 `~/.cc-monitor/bin/`、远端推到那台的 \
         `~/.cc-monitor/bin/` —— 那是**有落点的部署物**，进了 [`TOOLS`]（`id: \"panorama\"`，两个载体）。\
         ② **独立进程那一层**（`src/backend/sidecars/codepicture/`，条 67 · 2026-09-18 整棵删了，环境闭集里那条 `code-picture-sidecar` 同拍摘了）。\n\
         ⚠ `K-R65` 当初补的那句道理**仍然成立**：一个名字可以同时是好几样东西，写「表上没有它」的同一拍要把那张表改对（纪律 ⑲）。",
    ),
    (
        "planned-build",
        "**不由 cc-monitor 安装** —— 它是用户自己装在 `<claude_dir>/skills/planned-build/` 的 \
         skill，cc-monitor 只**读它的产物**（计划文件）并允许编辑那个收件箱。\n\
         ⇒ 它在 `skill_host::SKILLS` 里（接入的 skill），**不在**本表里（受管工具）。\
         ★ 这两个集合**有交集但不是同一张表**（今天交集只有 `cc-bus`）——\
         devbench 的账本 L4 原写「同一张表的两个视图」是**错的**，已订正。\n\
         它的 `SkillSpec.install` 如实记 `NotSupported` 并说明归 F06。",
    ),
];

#[cfg(test)]
#[path = "../../../tests/bridge/tool_registry_not_managed_tests.rs"]
mod not_managed_tests;

#[cfg(test)]
#[path = "../../../tests/bridge/tool_registry_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/bridge/tool_registry_environment_tests.rs"]
mod environment_tests;
