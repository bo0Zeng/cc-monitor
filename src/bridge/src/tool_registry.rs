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

/// 内容的来源。**每个变体的使用者数现算**（`TOOLS` 是唯一一份），不写死在这里〔`13b`〕。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum ToolSource {
    /// 仓内文件，编译期 `include_str!` 进二进制（`ccm`）。
    EmbeddedText { repo_path: &'static str },
    /// 仓内目录，运行期读（`cc-bus` 的 `src/shared/cc-bus/`）。
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
    /// - `remote-daemon` 原先声明 `RemoteHomeRelative(".local/bin/ccm-daemon")`，
    ///   而这个字符串**全仓只出现在注册表自己里**；真实路径是 `RemoteConfig.daemon_path`，
    ///   每个远端各自配置（`remote_history.rs::run_list_query` 直接 `shell_quote(&cfg.daemon_path)`）。
    /// - `cc-acct-iso` 原先声明 `LocalHomeRelative(".claude/skills/cc-acct-iso")`，
    ///   而 `acct_iso_deploy::deploy_remote_acct_iso(cfg, dest_dir)` 是**远端**部署、
    ///   落点还是**前端传进来的** `dest_dir`。
    ///
    /// 两处都是我凭印象写的常量。**声明一个不存在的常量比不声明更坏**——审计页会拿它去
    /// 查一个没人写的路径，然后言之凿凿地报"缺失"。所以这里显式承认"这是配置项"。
    ///
    /// `token` 是申报路径里用的占位符（形如 `$DAEMON_PATH`，与 `$PROFILE` 同一套写法，
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
/// `"远端 ~/.local/bin/ccm-daemon"`。作为展示文本没问题，但 T02 要**真去查这些文件的现状**，
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
    /// `cc_bus::cc_bus_send`（远端跑 `cc-send`）与 `cc_bus::cc_bus_spawn`（跑 `cc-spawn`），
    /// 而 `cc-bus-lib.sh:221` 是 `printf '%s\n' "$line" >> "$inbox"`、
    /// `cc-spawn:141` 追加 `spawned.tsv`、`cc-register:25` 换掉 `agents.tsv`。
    /// 「我们只是调了别人的命令」不改变**用户的文件因为在我们这儿点了一下而变了**这件事。
    /// 这一页的全部价值是可信告知，在自己的主张上失信比不做这一页更坏。
    IndirectWrite,
}

/// **同一个东西的一种载体** —— 「它这一份怎么产出来、落到哪、碰哪些文件」。
///
/// # 🔴 〔`K-R81` 09-12〕这一层为什么非有不可（用户 09-12 逐字逼出来的）
///
/// 用户逐字：「**一个后端要两处使用 / 即远程 daemon 就是远程本地机器的后端**」。
/// 也就是说：**远端那台机器上跑的那一份，是「那台机器的本地后端」**，
/// 不是「远端的 daemon」——「本机 / 远端」这个二分本身就是从错的名字里长出来的。
///
/// 而在这一层立起来之前，[`ToolSpec`] 是「一个源 + 一个落点 + 一串 touch」：
/// **同一个后端有三种载体、四个落点，而闭集只表达得了一个半**（`K-R68` 现打）。
/// 三种载体逐条是：① 这一份产物自己带着、要用时自释放的那份（`native-daemon/`，
/// `local_backend::native_embedded_daemon` 的 `include_bytes!`）·
/// ② 内嵌、推给远端那台机器的那份（`embedded-daemons/`）·
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
        //    在这一条上已经不合身了（值是**算出来的**，路径取自用户填的 `daemon_path`）。
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
                destination: ToolDestination::RemoteHomeRelative(".local/bin/ccm"),
                touches: &[
                    TouchedFile {
                        path: "~/.local/bin/ccm",
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
                // ⚠ 但 `IndirectWrite` 那句仍要留神：**写**面（`cc_bus_send`/`_spawn`/
                // `_broadcast`/`_kill`）至今**只动远端**（`refuse_local_write`），
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
                path: "~/.claude-accts/",
                // **`Either`**——这一条我改了两次，第二次也不对（T04 审计重要 2）。
                //
                // 第一版标 `Client`：错，`accounts.rs` 的账号库列举全是
                // `list_remote_accounts(origin)` / `list_remote_session_accounts(origin)`，走 ssh exec。
                // 第二版改 `Remote`：也不对——本机 `CLAUDE_CONFIG_DIR` 会**指进这个目录**
                // （这台机器上就是 `~/.claude-accts/z`），`hooks_diag::claude_config_dir` 与
                // `config_surface` 自己都在读它，`ConfigSurfaceReport.claude_config_dir` 更是
                // 直接把它打印出来。于是同一页会**自相矛盾**：顶部写着解析基准是
                // `<用户家目录>/.claude-accts/<账号>`，而这一行写着「位置：远端」。
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
    // 用户 09-12 逐字：「**一个后端要两处使用 / 即远程 daemon 就是远程本地机器的后端**」。
    // 这一条先前叫 `remote-daemon` / 「远端 daemon」，而它说的是**假的**：
    // 那一份不是「远端的 daemon」，是**那台机器的本地后端**（`K36` 逐字
    // 「两份后端应该要一样的」·`K33` 逐字「后端只有一个」）。
    //
    // 🔴 **这个名字已经花过一次真钱**：`build-linux` 没有 `Stage native daemon for
    // self-extract` 那一步而 `build-windows` 有 ⇒ **Linux 裸 exe 装出来没有本机后端**
    // （`ROADMAP#KU26`，用户已裁「那肯定带」）。★ 那正是「把它当成『远端』产物」的直接后果 ——
    // **名字塑造了发版流水线的形状**。⚠ 发版那一步归 `K-R42`，本件只把名字与闭集说对。
    ToolSpec {
        id: "backend",
        display_name: "cc-monitor 后端（一份实现，本机与远端各跑一份）",
        installable: true,
        // 🔴 〔`K-R63` 09-11〕**这一格原先是 `false`，而它是一处假申报** —— 本件那条新性质
        // 落地的当场把它逮出来的（不是人看出来的）。卸载实现一直在：
        // `sftp.rs::uninstall_remote_daemon` 是**设置面板「卸载 daemon」按钮**背后那条命令
        // （删 daemon 二进制 + 同目录 `.build_id`，`is_safe_remote_daemon_path` 守着）。
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
                    repo_path: "src/bridge/binaries/cc-monitor-remote",
                },
                // **路径不是常量，也不是家目录相对** —— 它跟着 app 装到哪儿走，
                // 而那个目录是装机时由人选的。同 `$DAEMON_PATH` 那一格的理由：
                // 申报一个我们其实没在用的常量，审计页会拿它去查一个没人写的路径
                // 然后言之凿凿地报「缺失」。
                destination: ToolDestination::UserConfiguredPath {
                    token: "$APP_DIR",
                    what: "装机时安装向导里选的那个安装目录（sidecar 与 cc-monitor 主程序同目录）",
                },
                touches: &[TouchedFile {
                    path: "$APP_DIR",
                    host: HostScope::Client,
                    note: Some(
                        "本机，随安装包落盘；文件名带 target triple（`cc-monitor-remote-<triple>`，\
                         Windows 上再带 `.exe`）—— 名字的真相源是 `local_backend::resolve_with`",
                    ),
                    effect: TouchEffect::OwnedFile,
                }],
            },
            Carrier {
                what: "这一份产物**自己带着**、旁边没有 sidecar 时自释放出来的那一份（`build.rs::embed_native_daemon` ⇒ `local_backend::native_embedded_daemon` 的 `include_bytes!`）",
                source: ToolSource::EmbeddedBinary {
                    repo_path: "src/bridge/native-daemon/cc-monitor-native",
                },
                // 落点带 build_id（`local_backend::local_extract_name`）—— 那不是命名品味：
                // 远端自部署落的也是这个目录，两边对同一个文件名有不同期望就会互判 stale、
                // 无限重装。glob 只在末段、只有一个 `*`（`resolve_local_home` 的两条校验）。
                destination: ToolDestination::LocalHomeRelative(".cc-monitor/bin/cc-monitor-local-*"),
                touches: &[TouchedFile {
                    path: "~/.cc-monitor/bin/cc-monitor-local-*",
                    host: HostScope::Client,
                    note: Some(
                        "本机自释放出来的那份后端，文件名带 build_id（`local_extract_name`）——\
                         同一个 build_id 不会重复写；**旧版本不回收**，那笔债记在 `local_extract_name` 头注",
                    ),
                    effect: TouchEffect::OwnedFile,
                }],
            },
            Carrier {
                what: "推给远端那台机器、在**那台机器上当本地后端**跑的那一份（`embedded-daemons/`，交叉编译的 musl 二进制）",
                source: ToolSource::EmbeddedBinary {
                    repo_path: "embedded-daemons",
                },
                destination: ToolDestination::UserConfiguredPath {
                    token: "$DAEMON_PATH",
                    what: "每个远端连接的「daemon 路径」配置项",
                },
                touches: &[TouchedFile {
                    path: "$DAEMON_PATH",
                    host: HostScope::Remote,
                    note: Some(
                        "路径由该连接的「daemon 路径」配置项决定——**不是**固定的 ~/.local/bin/ccm-daemon。\
                         ⚠ 它在那台机器上就是**那台机器的本地后端**（`K36`），\
                         「远端」说的是「相对这台 monitor」，不是它的身份",
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
        carriers: &[Carrier {
            what: "现场生成的一段 JSON，写进你选定的那个项目目录",
            source: ToolSource::Generated,
            destination: ToolDestination::ProjectRelative(".mcp.json"),
            touches: &[TouchedFile {
                path: ".mcp.json",
                host: HostScope::ProjectDir,
                note: Some("相对你选定的项目目录"),
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
        display_name: "POSIX rc 里的 ccm 别名块（cc / cct / zcc …）",
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
            // 理由与后端那条 `$DAEMON_PATH` 逐字同源：申报一个我们其实没在用的常量，
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
        why: "认 skill 所在的工作树与主检出要跑它 —— skill_host.rs::git_common_dir",
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
        why: "数 cc-bus 的 agent 在不在用它 —— cc_bus.rs::count_now",
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
    // 来历如实记：`K38` 逐字只举了两样（`account` · `cc-bus`），PM 拟 `KR65D3` 时也只
    // 数出这两样，**这一项是用户当拍凭记忆点出来的**（「不是还有 code picture 吗」）。
    // ⇒ 「app 自带的是哪几样」这个人群此前**真的没有住址**，这一条就是那句话的证据。
    //
    // 它的身份来自 `src/backend/sidecars/` 那一层的头注逐字：
    // 「**我们自己出、我们自己装、我们自己调**的那几个独立进程……
    //   这一层装的是**我们随产品分发**的东西」⇒ 这就是 `Provisioning::AppShips` 的定义。
    //
    // ⚠ **别读成「该给它接线」**：那一层今天零生产调用方，而那是一次**有代价的发布决策**
    // （`sidecar_fetch_guard` 那条「今天恰好 0 个生产调用点」的判据红的那一刻就是接线那一刻）。
    // 本条只申报「它属于自带那一群，而 app 里今天没有装口」，**不动那一层**。
    //
    // ⚠ 与 [`NOT_MANAGED`] 里那条 `code-picture` **不是同一个东西**：那条说的是
    // **vendored 进 monitor 二进制的 crate**（没有落点、卸载它等于重新编译）。
    // 这一条说的是**独立进程那一份**。同名三身份，那条反向表已补记。
    UnmanagedEnv {
        id: "code-picture-sidecar",
        display_name: "代码全景 sidecar（独立进程那一份）",
        who: Provisioning::AppShips,
        // 落点由调用方给（那一层头注逐字：「`dir` 是入参，本层不知道「落点在哪」」），
        // 而今天**没有调用方** ⇒ 连「往哪儿查」都还没有答案。
        // 🔴 这是「查不动」的第二个活体，而且它**查不动的理由与 `$MCP_COMMAND` 那条不同**：
        //    那条是「值住在用户的配置里」，这条是「值住在一段还没写的接线里」。
        probe: EnvProbe::CannotProbe {
            why: "落点由调用方给（那一层刻意不知道落点在哪），而今天它零生产调用方 —— \
                  接线那天才会有一个可查的路径",
        },
        named: "$CODEPICTURE_LANDING",
        host: HostScope::Either,
        why: "我们自己出、自己装、自己调的独立进程，随产品分发；取件那一跳已经写好、\
              只是还没接线 —— sidecars/codepicture/acquire.rs::obtain",
    },
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
        "**不是「装到别处的工具」，所以不属本表的语义** —— 它是 **vendored 进 cc-monitor \
         二进制**的 crate（`src/bridge/vendor/code-picture-core`），`panorama.rs` 直接 \
         `use Engine` 调它画图。没有 `destination`、没有安装动作、卸载它等于重新编译 monitor。\n\
         ⚠ 它另有一个身份是 **MCP server（code-picture 的 Agent head，给 Claude 用）**，\
         那一个确实「装到别处」—— 但**装它走已有的 `project-mcp` 机制**（往 `.mcp.json` 加一个 \
         server 条目），是**用法**不是新工具。仓里今天对那个 MCP head 零实现（`mcp.rs` / \
         `config_surface.rs` 里 `code-picture` 零命中）。\n\
         ⇒ 真要做「一键装 code-picture 的 MCP」属 **issue #51 第 1 部分**，\
         用户 08-10 明说「cc-bus 和 code-picture 后面再增强，现在先不做」。\n\
         🔴 **〔`K-R65` 09-11 补〕这个名字今天有第三个身份，本条此前一个字都没提**：\
         `src/backend/sidecars/codepicture/` 那一层的**独立进程**——\
         那一层头注逐字「我们自己出、我们自己装、我们自己调……随产品分发」。\
         它**是** app 自带的东西，已经进环境闭集（id `code-picture-sidecar`，\
         `Provisioning::AppShips`）。⇒ 本条那句「不是「装到别处的工具」」\
         **只对 vendored 那一份成立**，别拿它读那一份独立进程。\
         〔纪律 ⑲：写下「表上没有它」的同一拍，把那张表改对。〕",
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
mod not_managed_tests {
    use super::{NOT_MANAGED, TOOLS};

    /// ★ **反向表不许变成许愿池：每条都要论证「为什么不属这张表的语义」。**
    ///
    /// ⚠ 判据能判的只有「有没有在论证那件事」，**判不了论证对不对** —— 如实记在这里，
    /// 别把它读成「这些分类判断都被验证过了」。
    #[test]
    fn every_not_managed_entry_argues_why_it_is_out_of_scope() {
        assert!(
            !NOT_MANAGED.is_empty(),
            "反向表空了 —— 要么真没有刻意排除的东西（那就删掉这张表与本条），\
             要么有人清空了它。本条不许零命中地绿。"
        );
        for (id, why) in NOT_MANAGED {
            assert!(!id.is_empty(), "反向表里有空 id");
            // 「不属本表语义」的论证，至少要谈到这张表管的是什么。
            // ⚠ 关键词是**或**关系：不同的东西有不同的出局理由（不装 / 没落点 / 归别处）。
            let argues = why.contains("语义")
                || why.contains("不由 cc-monitor 安装")
                || why.contains("vendored")
                || why.contains("装到别处");
            assert!(
                argues,
                "`{id}` 的理由没有论证「为什么它不属这张表的语义」，\
                 只说了「还没做」之类 —— 那种东西属于 `installable: false`（如 cc-bus），\
                 不属反向表。\n理由原文：{why}"
            );
            assert!(
                why.len() > 80,
                "`{id}` 的理由只有 {} 字节 —— 分类判断要写清楚，否则下一个人还得重新查一遍",
                why.len()
            );
        }
    }

    /// ★ **反向表与正表不许重叠** —— 一个 id 只能在一边。
    #[test]
    fn not_managed_never_overlaps_the_managed_table() {
        for (id, _) in NOT_MANAGED {
            assert!(
                !TOOLS.iter().any(|t| t.id == *id),
                "`{id}` 同时在 `TOOLS` 与 `NOT_MANAGED` 里 —— \
                 「受管」与「刻意不收」是互斥的，两边都写等于没有判断"
            );
        }
    }
}

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
    ///
    /// **顺序不能反：先剥注释，再切测试段。** 第一版是反的，于是本模块文档里那句
    /// 「已在 `lib.rs` 标 `#[cfg(test)]`」——一句**散文**——把切点提到了结构声明**之前**，
    /// `production_code` 只返回前 50 行文档注释，5 条测试全红。
    /// 是 `parser_actually_sees_the_real_source` 的反向自检（`assert!(code.contains(
    /// "pub struct ToolSpec {"), "剥过头了")`）报出来的——**要件 3 又救了一次**。
    /// 附带教训：我提交 `a6d4b63` 前改了这句文档却**没重跑 cargo test**，
    /// 于是那个 commit 的 message 写着「cargo test 474」而实际是 469+5 红。
    fn production_code(src: &str) -> String {
        let no_comments: String = src
            .lines()
            .map(|l| match l.find("//") {
                Some(i) => &l[..i],
                None => l,
            })
            .collect::<Vec<_>>()
            .join("\n");
        // 切点还要求 `#[cfg(test)]` **顶格**（模块级属性），免得将来被缩进的同名属性骗到
        let code = no_comments
            .split(concat!("\n#[cfg", "(test)]"))
            .next()
            .unwrap_or(&no_comments)
            .to_string();
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

    /// 某个结构声明的字段：`(名, 类型)`，**按源码里实际写的枚举**。
    ///
    /// `struct_name` 是参数而不是硬编码 needle（T02 审计重要 3）：原先只扫 `ToolSpec`，
    /// 于是**同一套审计手法下移一层仍然有效**——审计给 `TouchedFile` 加一个
    /// `pub needs_sudo: bool`（10 个字面量里 1 真 9 假）→ **492 全绿、零 warning**
    /// （`pub` 字段在 lib crate 里连 `dead_code` 都不报，连 T01 依赖的"clippy 存根"都没有）。
    /// 参数化之后 `TouchedFile` 与 `ToolSpec` 走同一条纪律。
    fn declared_fields_of(code: &str, struct_name: &str) -> Vec<(String, String)> {
        let (a, b) = matched_span(code, &format!("pub struct {struct_name} {{"), 0)
            .unwrap_or_else(|| panic!("取不到 {struct_name} 的声明体——扫描器失效了"));
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

    fn declared_fields(code: &str) -> Vec<(String, String)> {
        declared_fields_of(code, "ToolSpec")
    }

    /// `TOOLS` 里每一个 `ToolSpec { … }` 字面量的**体**文本。
    fn literals_of<'a>(code: &'a str, type_name: &str) -> Vec<&'a str> {
        let (a, b) = matched_span(code, "pub const TOOLS: &[ToolSpec] = &[", 0)
            .expect("取不到 TOOLS 常量体——扫描器失效了");
        let body = &code[a..b];
        let mut out = Vec::new();
        let mut off = 0usize;
        // 配对之后从**本块结束处**继续找：找 `ToolSpec {` 时嵌套的 `TouchedFile` 块
        // 不会被重复计入；找 `TouchedFile {` 时则是逐个取那些嵌套块本身。
        let opener = format!("{type_name} {{");
        while let Some((s, e)) = matched_span(body, &opener, off) {
            out.push(&body[s..e]);
            off = e;
        }
        out
    }

    fn tool_literals(code: &str) -> Vec<&str> {
        literals_of(code, "ToolSpec")
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
    fn field_discipline_of(code: &str, struct_name: &str, literal_name: &str) -> ScanReport {
        let fields = declared_fields_of(code, struct_name);
        let lits = literals_of(code, literal_name);
        let mut r = ScanReport {
            checked: 0,
            violations: Vec::new(),
        };
        if lits.len() < 2 {
            r.violations
                .push(format!("只找到 {} 个 {literal_name} 字面量", lits.len()));
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
                    "字段 `{name}: {ty}` 只被 {} 个 {struct_name} 字面量实质实例化（{users:?}）\
                     ——只有一套需要的东西不进 {struct_name}",
                    users.len()
                ));
            }
            if ty == "bool" && users.len() == lits.len() {
                r.violations.push(format!(
                    "字段 `{name}: bool` 在全部 {} 个 {struct_name} 上都为真，没有区分力",
                    lits.len()
                ));
            }
        }
        r
    }

    fn field_discipline(code: &str) -> ScanReport {
        field_discipline_of(code, "ToolSpec", "ToolSpec")
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
                "installable",
                "uninstallable",
                "carriers"
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

    /// **文档里提到 `#[cfg(test)]` 不许把切点提前**（这是真踩过的：5 条测试当场全红）。
    #[test]
    fn prose_mentioning_the_test_attribute_does_not_truncate_the_scan() {
        let src = concat!(
            "//! 已在 `lib.rs` 标 `#[cfg",
            "(test)]`，不占这笔债。\n",
            "pub struct ToolSpec {\n    pub id: &'static str,\n}\n",
            "\n#[cfg",
            "(test)]\nmod tests { fn helper() {} }\n"
        );
        let code = production_code(src);
        assert!(code.contains("pub struct ToolSpec {"), "散文把切点提前了");
        assert!(!code.contains("fn helper"), "测试段没被切掉");
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

    /// **同一条纪律也管 `TouchedFile`**（T02 审计重要 3）。
    ///
    /// 原先字段纪律只扫 `ToolSpec`，于是 T01 那条审计手法**下移一层仍然有效**——
    /// 审计给 `TouchedFile` 加 `pub needs_sudo: bool`（10 个字面量里 1 真 9 假）→
    /// **492 全绿、零 warning**。`pub` 字段在 lib crate 里连 `dead_code` 都不报，
    /// 所以连 T01 依赖的"clippy 存根"这条兜底都没有。
    ///
    /// 顺带**更正我自己文档里说反的一句**：`TouchedFile` 的文档写着「`note` 的 ≥2 判据是
    /// 人工数的，不谎称有门禁」——低估了。`note` 其实有一条机器门禁
    /// （`config_surface` 的 `rows_cover_…` 里 `with_note.len() >= 2`），
    /// 真正一条门禁都没有的是 `path` / `effect` 和**将来新增的字段**。现在这条补上了。
    #[test]
    fn touched_file_fields_follow_the_same_discipline() {
        let code = production_code(include_str!("tool_registry.rs"));
        field_discipline_of(&code, "TouchedFile", "TouchedFile")
            .require(3, "TouchedFile 字段纪律")
            .unwrap();
    }

    /// 用审计那条**下移一层**的手法验证上一条：给 `TouchedFile` 塞一个单实例化字段必须红。
    #[test]
    fn the_scan_catches_a_single_use_field_on_touched_file_too() {
        let code = production_code(include_str!("tool_registry.rs"));
        let lit_count = code.matches("            TouchedFile {").count()
            + code.matches("        touches: &[TouchedFile {").count();
        assert!(lit_count >= 6, "字面量锚点数不对：{lit_count}");
        let mutated = code
            .replace(
                "    pub effect: TouchEffect,\n}",
                "    pub effect: TouchEffect,\n    pub needs_sudo: bool,\n}",
            )
            .replace(
                "                effect: TouchEffect::",
                "                needs_sudo: false,\n                effect: TouchEffect::",
            )
            .replace(
                "            effect: TouchEffect::",
                "            needs_sudo: false,\n            effect: TouchEffect::",
            )
            .replacen("needs_sudo: false", "needs_sudo: true", 1);
        // **先确认变异真落位**（本会话两次"全绿"其实是变异没写进文件）
        let n = mutated.matches("needs_sudo").count();
        assert!(
            n >= 1 + 10,
            "变异没落到位：声明 1 处 + 每个 TouchedFile 一处，实得 {n}"
        );
        assert_eq!(mutated.matches("needs_sudo: true").count(), 1);
        let r = field_discipline_of(&mutated, "TouchedFile", "TouchedFile");
        assert!(
            r.violations.iter().any(|v| v.contains("needs_sudo")),
            "TouchedFile 上的单实例化字段必须被抓，实得 {:?}",
            r.violations
        );
    }

    /// **审计那条手法，钉成常驻测试**：直接变异**真文件**，塞一个中性命名的单实例化
    /// 字段 `needs_elevation`。上一版硬编码断言对此**21 项全绿**。
    #[test]
    fn the_scan_catches_the_audits_own_single_use_field() {
        let code = production_code(include_str!("tool_registry.rs"));
        let mutated = code
            .replace(
                "    pub carriers: &'static [Carrier],",
                "    pub carriers: &'static [Carrier],\n    pub needs_elevation: bool,",
            )
            // 🔴 **锚点必须只认 `ToolSpec` 那一族的 `id:`**〔`K-R60` 09-11 现打〕：
            // 上一版的锚点是裸的 `"        id: \""` —— 它认的是「本文件里任何 8 空格缩进的
            // `id:` 行」，而那时**本文件只有 `TOOLS` 一张表**，所以它看起来是对的。
            // `K-R60` 往本文件加了第二张表（`UNMANAGED_ENV`，同样的缩进）之后，
            // 这一刀当场打到 17 处、计数自检红在「变异没落到位」。
            // ⇒ 收窄成 `ToolSpec {` + 下一行的 `id:`，只认该打的那一族。
            // ⚠ 这不是放水：命中数**仍然**由下面那条 `1 + TOOLS.len()` 的等号自检守着。
            .replace(
                "    ToolSpec {\n        id: \"",
                "    ToolSpec {\n        needs_elevation: false,\n        id: \"",
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
            .flat_map(|t| t.touches().map(|f| f.effect))
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
        // ⚠⚠ 〔`PS1` 08-13〕**这条断言今天落后于代码一格，理由如实写在这里**：
        // `U10b`〔用@08-13〕裁「开」之后，cc-bus 的部署**真的实现了**
        // （`cc_bus_deploy.rs`，6 条行为判据：围栏拒软链 / 幂等不重写 / 覆盖留备份 /
        //  装完可执行 / 内嵌清单与仓对拍）。按本条的名字（「声明要配现实」），
        // `installable` 该翻成 `true`。
        //
        // **而翻了它当场撞上另一条判据** —— `every_declared_field_has_at_least_two_instantiations`：
        // cc-bus 是最后一个 `false`，翻掉之后 `installable` 在**全部 6 个** `ToolSpec` 上都为真
        // ⇒ 「没有区分力」。那条报得**对**：字段退化成常量，按本仓的既定准则就该**删字段**
        // （同 `P4c` 撞 `host_is_not_a_function_of_destination` 那次的处置）。
        // 而删它要动 `config_surface` 的行、UI 文案（`config-surface-section.ts:50`）与
        // `skill_host` 里那条钉着 `"installable: false"` 字面量的判据 —— **是一次跨文件重构**。
        //
        // ⇒ 本件**不顺手做那次重构**（`PS1` 的正题是部署，不是字段治理），
        // 声明位暂留 `false`，代价如实登记在这里：**声明表比代码晚一格**。
        // ★ 解锁条件：删掉 `installable` 字段（或给它找回区分力）之后，把这条断言反过来。
        //
        // 🔴 **09-11 `K-R60`：上面那个解锁条件兑现了，本条按它自己写的话反过来。**
        // 走的是「**给它找回区分力**」那一支，不是删字段 —— 删了就再没有任何字段能申报
        // 「cc-bus 装不装得了」，而这一格恰恰是本件在治的。
        // 区分力从哪儿回来的：`claude-code` 那条（装不了、只读）进表，
        // 理由写在它自己那个字面量上头。
        // ⚠ 那一整段「暂留 false」的理由**留着不删**：它是这处假申报活了一个月的来路，
        //   而本条的名字（「声明要配现实」）说的正是那件事。
        //
        // 🔴 **09-11 `K-R63`：本条原先在这里有两条专名断言，已经收走了。**
        // 原文逐字是 `assert!(ccbus.installable, …)` 与 `assert!(!ccbus.uninstallable, "卸载没做，不得声明可卸")`。
        // 它们判的正是「申报 ↔ 现实」，而那件事今天由一条**覆盖全表**的性质判
        // （`every_tool_declares_install_and_uninstall_as_the_implementations_really_are`）。
        // 留着它们不是双保险，是两个坏处：
        //   ① 专名钉子只把静默从一个工具挪走，下一个工具照样静默（件文件 `§0c` 的正题）；
        //   ② 第二条会**在事情变好的那天错红** —— 真给 cc-bus 补上卸载实现并如实把字段翻成
        //      `true`，它会拦一次，而那时它拦的是一句真话。
        // ⇒ 本条今天只剩下**不属于那条性质**的那一格：`settings.json` 的 effect。
        // ⚠ 如实登记：本条的**名字**因此比它现在做的事宽了一格（改名要连带跑生成命令，
        //   PM 的窗口开着时不许跑）⇒ 改名的事走上报口交回 PM，不在这一拍自批。
        // settings.json 只生成待贴文本，绝不写
        let hooks = ccbus
            .touches()
            .find(|f| f.path.contains("settings.json"))
            .expect("cc-bus 应声明它需要 settings.json 的钩子");
        assert_eq!(
            hooks.effect,
            TouchEffect::GenerateOnly,
            "settings.json 是共享全局配置，只能生成待贴文本"
        );
    }

    /// **声明「整个文件由我们拥有」就必须真的装得了它**（T02 审计阻塞 2）。
    ///
    /// 原先 cc-bus 的 `~/.local/bin/cc-*` 是 `OwnedFile` 而 `installable: false`
    /// ——审计页于是同时显示「12 项匹配」+「由 cc-monitor 拥有、部署时整体覆盖」+
    /// 「尚未支持部署，也就无所谓撤销」。用户读到的是：cc-monitor 宣称拥有 12 个
    /// 它没建、装不了也撤不了的文件。真机核实：那 12 条软链是用户自己的安装脚本
    /// 于 7/17 与 7/26 建的，cc-monitor 侧**一行创建代码都没有**。
    #[test]
    fn owned_file_implies_installable() {
        for t in TOOLS {
            if t.touches().any(|f| f.effect == TouchEffect::OwnedFile) {
                assert!(
                    t.installable,
                    "{} 声称拥有某个文件却装不了它——那这个「拥有」是假的",
                    t.id
                );
            }
        }
    }

    /// **装得了，就必须申报装到哪**（替换掉那条同义反复的测试，见下）。
    ///
    /// 这一条替代原先的 `locality_is_derivable_from_destination_today`。审计实测那条是
    /// **同义反复**：`PathResolution::Remote` 只由 `RemoteHomeRelative` 臂产生且必然产生，
    /// 所以断言恒真——把 `ccm` 的 `destination` 翻成 `LocalHomeRelative`（会让两行从
    /// "远端未确定"变成去 stat 本机 `~/.bashrc`）**492 项照样全绿**。
    /// 而它承诺守的那件事（"本机落点却申报远端文件"）在类型上根本表达不出来，
    /// 永远不会红。**不留永远不会红的钉子。**
    ///
    /// 换成这条有牙的跨字段一致性：`installable` 的工具，其 `destination` 指的那个路径
    /// 必须出现在 `touches` 里。改任一边就会红。
    /// （`installable: false` 的 cc-bus 豁免——它的 `destination` 目前是**愿景**，
    ///  部署还没实现，硬要它出现在 touches 里就得给一个假的 effect，那正是阻塞 2 的病。）
    /// ★ PS1（重摸底 08-12）：cc-bus 的 `installable: false` **必须写着深一层的理由**。
    ///
    /// 浅理由（「部署尚未实现」）会让下一个人以为补个递归拷贝就能翻 true；
    /// 而真实的墙是 `src/doc/INVARIANTS.md` 那条只读铁律 —— 落点 `~/.claude/skills/` 不在
    /// 它穷举的 6 条例外里。**那是裁定，不是实现工作。**
    ///
    /// 这是「禁词守卫」的反面：**必需词**守卫。删掉那段话的人会被拦一次。
    #[test]
    fn cc_bus_says_why_it_is_not_installable_at_the_real_depth() {
        let me = include_str!("tool_registry.rs");
        // ⚠ **数次数，不是 `contains`** —— 本判据自己的字面量也在这个文件里
        //（本会话已经栽过三次：`P3s-Y2` / `P4d-Y4` / `P4b-Y3`）。
        for must in ["开第 7 条豁免", "绝不碰"] {
            assert!(
                me.matches(must).count() >= 2,
                "cc-bus 那条 `installable: false` 的注释里少了 {must:?}。\n\
                 只写「部署尚未实现」是**浅一层**的理由 —— 真实的墙是只读铁律\n\
                 （`~/.claude/skills/` 不在它穷举的 6 条例外里）。\n\
                 删掉它，下一个人会以为补个递归拷贝就能翻 true。"
            );
        }
    }

    /// 一个载体的**申报落点**（`destination` ⇒ 那条 touch 该写成什么）。
    ///
    /// **唯一一份口径**〔`13b`〕：下面三条判据（落点在不在 touches 里 · 同一个东西有几个
    /// 落点 · 那几个落点逐条钉死）都从这里取，别在第二处再写一份 `match`。
    /// `None` = 这一格回答的不是「装到哪」（[`ToolDestination::NotInstalledByUs`]）。
    fn landing_path_of(d: &ToolDestination) -> Option<String> {
        match d {
            ToolDestination::RemoteHomeRelative(p) | ToolDestination::LocalHomeRelative(p) => {
                Some(format!("~/{p}"))
            }
            ToolDestination::ProjectRelative(p) => Some((*p).to_string()),
            ToolDestination::UserShellProfile => Some("$PROFILE".to_string()),
            ToolDestination::UserConfiguredPath { token, .. } => Some((*token).to_string()),
            ToolDestination::NotInstalledByUs { .. } => None,
        }
    }

    /// **这个工具说得出几个落点** —— 就是本件那条 dod 判的那个「关系」。
    ///
    /// 🔴 **为什么数的是载体而不是枚举值**（`KR81D1` 逐字写死的失效方向）：
    /// `destination` 是单值的时候这个数**恒等于 1**，往 `ToolDestination` 里
    /// **加多少个枚举值它都还是 1** —— 那只是「值多了一个」，不是
    /// 「一个东西对多个落点」。这个数 >1 当且仅当**载体这一维真的存在**。
    fn landing_sites_of(t: &ToolSpec) -> Vec<String> {
        t.carriers
            .iter()
            .filter_map(|c| landing_path_of(&c.destination))
            .collect()
    }

    #[test]
    fn installable_tools_declare_where_they_land() {
        for t in TOOLS {
            if !t.installable {
                continue;
            }
            // 🔴 〔`K-R81` 09-12〕**逐载体判，而这一步比先前严**。
            //    `K-R69` 那一版是「一个工具一个 `destination`、一串期望落点、去这个工具的
            //    **全部** touches 里找」—— 那时 M 个落点 × N 条 touch **没有 key**，
            //    一个落点被另一个载体的 touch「凑巧接住」也照样绿。
            //    今天落点与 touch 都挂在同一个载体下 ⇒ 接住它的必须是**它自己那一份**。
            for c in t.carriers {
                // 〔`K-R60`〕跨字段：「这不是我们的落点」与「装得了」不许同时成立。
                let Some(want) = landing_path_of(&c.destination) else {
                    panic!(
                        "{} 声明 installable: true，而载体「{}」的落点写着「不是我们装的」——\
                         两句话有一句是假的",
                        t.id, c.what
                    )
                };
                assert!(
                    c.touches.iter().any(|f| f.path == want),
                    "{} 的载体「{}」可安装，但它自己的 touches 里没有它的落点 {want:?}（实得 {:?}）",
                    t.id,
                    c.what,
                    c.touches.iter().map(|f| f.path).collect::<Vec<_>>()
                );
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 🔴 `K-R81` / `KR81D1` ＋ `KR81D3`：**一个后端，几处使用**
    //
    // 用户 09-12 逐字：「一个后端要两处使用 / 即远程 daemon 就是远程本地机器的后端」。
    // 下面四条判的是**这句话在闭集里说得出来**，不是「表好看一点」。
    // ═══════════════════════════════════════════════════════════════════════

    /// 后端那条 `ToolSpec` 的 id —— **只许有一个住址**〔`13b`〕，
    /// 下面几条判据与别处引用它的地方都从这里取。
    const BACKEND_ID: &str = "backend";

    fn backend() -> &'static ToolSpec {
        TOOLS
            .iter()
            .find(|t| t.id == BACKEND_ID)
            .unwrap_or_else(|| panic!("闭集里找不到 id 为 `{BACKEND_ID}` 的那一条"))
    }

    /// ★ `KR81D1` **正面**：闭集说得出「**同一个后端**落在哪几处」。
    ///
    /// # 死值验（`KR81D1` 逐字要的那一向）
    ///
    /// 把三个落点里**任意一个**从闭集里摘掉 ⇒ 本条红，**并点名是哪一个没了**。
    ///
    /// # 🔴 失效方向写死在这里（`KR81D1` 逐字）
    ///
    /// 「给 `destination` 加第二个枚举值就算多落点」——**不算**。
    /// [`landing_sites_of`] 数的是**载体**：`destination` 单值时它恒等于 1，
    /// 往 `ToolDestination` 里加多少个变体它都还是 1。
    /// 本条要的是那个数 **> 1**，也就是「一个东西对多个落点」这个**关系**存在。
    ///
    /// # 它守什么、**不守什么**
    ///
    /// 守的是**申报**（这张表说不说得出那三份）。「那三份是不是同一次构建出来的」
    /// **本条不判，而且今天没有任何东西判得了** —— `.build_id` 三者从同一处源码常量抠，
    /// 恒等，那句恒等一格证据都不提供（`DECISIONS.md#R26` 裁定零）。
    #[test]
    fn the_backend_is_one_thing_landing_in_several_places() {
        let t = backend();

        // ① 关系存在：不是「一个落点」，也不是「一个值多了几个枚举变体」。
        let sites = landing_sites_of(t);
        assert!(
            sites.len() > 1,
            "`{BACKEND_ID}` 只说得出 {} 个落点（实得 {sites:?}）——\n\
             用户 09-12 逐字「一个后端要两处使用」，而闭集里它还是一处。\n\
             ⚠ 往 `ToolDestination` 里加枚举值买不到这一格：这个数数的是**载体**。",
            sites.len()
        );

        // ②b 反向自检：这把尺子在**单载体**的工具上真的给 1（否则上面那条是空真）。
        let single: Vec<usize> = TOOLS
            .iter()
            .filter(|t| t.carriers.len() == 1)
            .map(landing_sites_of)
            .map(|v| v.len())
            .collect();
        assert!(
            single.iter().any(|n| *n <= 1),
            "尺子失准：单载体的工具也数出 >1 个落点（实得 {single:?}）——\n\
             那说明它数的不是载体，上面那条 `>1` 于是恒真"
        );

        // ② 逐条钉死：`(这一份是从哪来的, 它落到哪, 在哪台机器上)`。
        //    改 `TOOLS` 就要来改这张表 —— 这是**有意的摩擦**（同
        //    `config_surface::every_host_declaration_is_pinned` 那张表的理由）。
        let mut got: Vec<(String, String, HostScope)> = t
            .carriers
            .iter()
            .map(|c| {
                let src = match &c.source {
                    ToolSource::EmbeddedBinary { repo_path } => (*repo_path).to_string(),
                    other => panic!(
                        "后端的载体来源不该是 {other:?} —— 三份都是**二进制**（`K25`：\
                         一份代码、每个平台一份原生产物）"
                    ),
                };
                let dst = landing_path_of(&c.destination)
                    .unwrap_or_else(|| panic!("后端的载体「{}」没有落点", c.what));
                let host = c
                    .touches
                    .iter()
                    .find(|f| f.path == dst)
                    .unwrap_or_else(|| panic!("载体「{}」的落点不在它自己的 touches 里", c.what))
                    .host;
                (src, dst, host)
            })
            .collect();
        let mut want: Vec<(String, String, HostScope)> = vec![
            // ③ 安装包放在 app 可执行文件旁边的那份（`tauri.sidecar.conf.json` 的 `externalBin`）
            (
                "src/bridge/binaries/cc-monitor-remote".into(),
                "$APP_DIR".into(),
                HostScope::Client,
            ),
            // ① 这一份产物自己带着、旁边没有 sidecar 时自释放的那份
            (
                "src/bridge/native-daemon/cc-monitor-native".into(),
                "~/.cc-monitor/bin/cc-monitor-local-*".into(),
                HostScope::Client,
            ),
            // ② 推给远端那台机器、在那台机器上当**它的本地后端**跑的那份
            (
                "embedded-daemons".into(),
                "$DAEMON_PATH".into(),
                HostScope::Remote,
            ),
        ];
        // `HostScope` 没有 `Ord`（它是描述型 enum），按前两栏排 —— 同
        // `config_surface::every_host_declaration_is_pinned` 那张表的写法。
        got.sort_by(|x, y| (&x.0, &x.1).cmp(&(&y.0, &y.1)));
        want.sort_by(|x, y| (&x.0, &x.1).cmp(&(&y.0, &y.1)));
        assert_eq!(
            got, want,
            "\n后端的载体清单与钉死的表对不上 —— 少一份就是「闭集说不出它」，\n\
             而那正是 `K-R68` 立件时的读数（3 种载体 4 个落点，闭集表达 1 个半）。\n\
             改 `TOOLS` 就要来改这张表，并说清为什么。"
        );
    }

    /// ★ `KR81D3`：**`ccm` 那两份的来源不是同一个，而闭集今天说得出来。**
    ///
    /// # 立件时这一条是红的（那正是这半格的题面）
    ///
    /// `R26` 裁定二逐字：`ToolSource` 与 `destination` 是**同一个形状问题的两半**。
    /// 在载体这一维立起来之前，`ccm` 只有一个 `source`，而它的注释**自己承认是假的**
    /// （逐字：「本机那一半的来源不是这个 shim —— 是后端二进制自己的改名副本……
    /// `ToolSource` 一个字段同样装不下两个来源」）—— **用散文顶替一个字段**。
    ///
    /// # 死值验（`KR81D3` 逐字要的那一向）
    ///
    /// 把本机那份的 `source` 改回今天那个假值（＝ 与远端那份同一个 `ccm_entry_shim`）
    /// ⇒ 本条红。
    #[test]
    fn the_two_ccm_carriers_do_not_share_one_false_source() {
        let ccm = TOOLS
            .iter()
            .find(|t| t.id == "ccm")
            .expect("闭集里没有 `ccm` 那一条");
        assert_eq!(
            ccm.carriers.len(),
            2,
            "`ccm` 今天是**两份**：远端那条 shim + 本机那份后端二进制的改名副本"
        );
        let srcs: Vec<&ToolSource> = ccm.carriers.iter().map(|c| &c.source).collect();
        assert_ne!(
            srcs[0], srcs[1],
            "`ccm` 两个载体的 `source` 逐字相同 —— 那正是本件治的那句假话：\n\
             远端那条是 `local_backend::ccm_entry_shim` 现造的三行 `exec` 串，\n\
             本机那条是**后端二进制自己的改名副本**（`install_local_ccm_entry`）。\n\
             一个 `source` 装不下两个来源，而「在注释里如实写清」不是一个字段。"
        );
        // 本机那一份的来源必须指到那个**放二进制**的符号，不是那个造 shim 的符号。
        let local = ccm
            .carriers
            .iter()
            .find(|c| matches!(c.destination, ToolDestination::LocalHomeRelative(_)))
            .expect("`ccm` 本机那个载体不见了（`K-R69` 建的那条）");
        match &local.source {
            ToolSource::EmbeddedBinary { repo_path } => assert!(
                repo_path.ends_with("::install_local_ccm_entry"),
                "本机那条 `ccm` 的来源指到了 {repo_path:?} —— 它该指到真把那份字节\
                 放下去的那个符号（`local_backend::install_local_ccm_entry`）"
            ),
            other => {
                panic!("本机那条 `ccm` 是**一份二进制**（后端本体的改名副本），不是 {other:?}")
            }
        }
    }

    /// **一个工具的几个载体，不许一半是我们的、一半是别人的。**
    ///
    /// 这一条是 [`Provisioning::of_tool`] 那条聚合规则（「全部载体都是
    /// `NotInstalledByUs` 才算不是装出来的」）的门禁：混合的那一形一出现，
    /// 那条规则就得**重裁**，而不是让它静默地选一边
    /// （`references/testing.md` 硬规则 11：钉「今天恰好如此」的判据要写清去哪儿重裁）。
    #[test]
    fn carriers_do_not_mix_ours_and_not_ours() {
        for t in TOOLS {
            let n = t
                .carriers
                .iter()
                .filter(|c| matches!(c.destination, ToolDestination::NotInstalledByUs { .. }))
                .count();
            assert!(
                n == 0 || n == t.carriers.len(),
                "`{}` 的 {} 个载体里有 {n} 个写着「不是我们装的」——\n\
                 `Provisioning::of_tool` 那条聚合规则（全是才算）此刻在**替你选一边**。\n\
                 ⇒ 去 `Provisioning::of_tool` 的头注重裁那条规则，别把这条判据放宽。",
                t.id,
                t.carriers.len()
            );
        }
        // 反向自检：混合的那一形真的判得出来（合成一条，不动真表）。
        const MIXED: &[Carrier] = &[
            Carrier {
                what: "自检：我们装的那一份",
                source: ToolSource::Generated,
                destination: ToolDestination::LocalHomeRelative(".x/y"),
                touches: &[],
            },
            Carrier {
                what: "自检：别人的那一份",
                source: ToolSource::NotOurs { who: "自检" },
                destination: ToolDestination::NotInstalledByUs { whose: "自检" },
                touches: &[],
            },
        ];
        let n = MIXED
            .iter()
            .filter(|c| matches!(c.destination, ToolDestination::NotInstalledByUs { .. }))
            .count();
        assert!(
            n != 0 && n != MIXED.len(),
            "自检夹具没造出混合那一形 —— 上面那条断言此刻是空真"
        );
    }

    /// **每个载体都说得出自己是哪一份**，而同一个工具里两份不许说同一句话。
    ///
    /// 没有这一格，多载体的工具在配置面上就是几行长得一样的字 ——
    /// 「说得出几个落点」于是退化成「表里多了几行」。
    #[test]
    fn every_carrier_says_which_one_it_is() {
        let mut n_multi = 0;
        for t in TOOLS {
            let mut seen: HashSet<&str> = HashSet::new();
            for c in t.carriers {
                assert!(
                    !c.what.trim().is_empty(),
                    "`{}` 有一个载体没说自己是哪一份",
                    t.id
                );
                assert!(
                    seen.insert(c.what),
                    "`{}` 有两个载体说着同一句话（{:?}）—— 那就分不出是哪一份了",
                    t.id,
                    c.what
                );
            }
            if t.carriers.len() > 1 {
                n_multi += 1;
            }
        }
        // 地板反向自检：真表里得有**多载体**的工具，否则上面那条唯一性是空真。
        assert!(
            n_multi >= 2,
            "闭集里多载体的工具只有 {n_multi} 个 —— 唯一性那半此刻几乎是空真。\n\
             今天该有两个：`ccm`（远端 shim / 本机改名副本）与 `{BACKEND_ID}`（三种载体）"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 🔴 `K-R81` / `KR81D2`：**那个名字改到哪儿了 —— 一张可核的账**
    //
    // dod 逐字：「**不要求一次全改**，但**要求给出「改了哪些、没改哪些、为什么」的
    // 可核读数**」，失效方向逐字：「**只改闭集那个 id，而 70 份 `.rs` 照旧** ——
    // 那是把账做平，不是把名字改对」。
    // ⇒ 下面两条各买一半：
    //   · 第一条是**零命中守卫**，射程 = 闭集那张表的**数据**本身（改回旧 id ⇒ 当场红）；
    //   · 第二条是**登记 ＋ 递减棘轮**，射程 = `src/bridge/src` ＋ `src/bridge/crates` 两棵树 ——
    //     没登记就不许带旧名，登记了就只许变少。**那张表就是那份读数**，不是一句话。
    // ═══════════════════════════════════════════════════════════════════════

    /// 旧名字今天还留在哪儿，**按「改它要动什么」分档**。闭集。
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum Why {
        /// **符号名**（那几个 `*_daemon` 的 `pub async fn`、以及对它们的逐字引用）。
        /// 改它要与 `structural_scan` 的逐字签名钉、`sftp_move_ledger`、
        /// `parity_ledger`、`remote_write_registry` 那几张登记表**同拍**改 ——
        /// 那是一件纯符号改名件，与本件的正题（名字说错了「它**是什么**」）不同轴。
        /// **解锁条件**：另立一件「符号改名」，把那几张表一起带上。
        SymbolName,
        /// **措辞**（注释 / 文档 / 印给用户的串里那句「远端的那个后台进程」）。
        /// 🔴 **这些句子今天多数不假** —— 那确实是远端那台上的那一份；
        /// 用户裁的是它的**身份**（那是**那台机器的本地后端**，`K36`）。
        /// 订正措辞要连**前端那一面**一起过（`src/**/*.ts` 现打 30 余处），
        /// 只改 Rust 半边会让两边说两种话。**解锁条件**：另立一件「文案面」，两侧同拍。
        Wording,
        /// **逐字引用旧的闭集 id** —— 全部是订正段 / 墓碑 / 病史
        /// （「本条落地当场逮到 `<旧 id>`」这一族）。
        /// 🔴 **刻意保留，不改**：一句话写下时真、后来被别的裁定推翻，
        /// **那是历史，不是错误**（同 `MASTERPLAN#K25` 对 `C18` 三处的处置）。
        /// 改掉它等于抹掉推翻的过程。**没有解锁条件 —— 它本来就不该被改。**
        OldId,
    }

    /// **旧名字的存量账。闭集。**
    ///
    /// `(相对 `src/bridge/` 的路径, 哪一档, 今天的处数)`
    ///
    /// 🔴 表名叫 `SITES` 不是随手起的：`scanning_guard_registry::TABLE_DECLS`
    /// 那条纪律逐字「新写一条『扫描面 ＋ 常量表』型的判据，那张表要起成
    /// `TABLE_DECLS` 里已有的名字之一」——起别的名字，那条元判据**看不见本文件**，
    /// 而看不见与「合规」在输出上一模一样。
    ///
    /// ⚠ **这张表不是愿望清单，是读数**：每一行都由下面那条判据在**真树**上对拍，
    /// 多一处红、少一处也红（少 ⇒ 那一行该删了，账不许挂着空号）。
    const SITES: &[(&str, Why, usize)] = &[
        ("crates/acct-core/src/lib.rs", Why::Wording, 1),
        ("crates/branch-core/src/lib.rs", Why::Wording, 1),
        ("crates/creds-core/src/lib.rs", Why::Wording, 1),
        ("src/accounts.rs", Why::Wording, 8),
        ("src/acct_iso_deploy.rs", Why::SymbolName, 3),
        ("src/backend/control/daemon_route.rs", Why::Wording, 1),
        ("src/config_surface.rs", Why::OldId, 4),
        ("src/cross_half_edge_registry.rs", Why::OldId, 1),
        ("src/daemon_control.rs", Why::Wording, 3),
        ("src/doc_copy_registry.rs", Why::Wording, 1),
        ("src/drift_ledger.rs", Why::Wording, 2),
        ("src/fenced_block.rs", Why::OldId, 1),
        ("src/fenced_block.rs", Why::SymbolName, 1),
        ("src/history.rs", Why::Wording, 1),
        ("src/inbound_client.rs", Why::Wording, 2),
        ("src/lib.rs", Why::SymbolName, 2),
        ("src/lib.rs", Why::Wording, 3),
        ("src/local_accounts.rs", Why::Wording, 1),
        ("src/local_read_surface_registry.rs", Why::Wording, 1),
        ("src/parity_ledger.rs", Why::SymbolName, 2),
        ("src/parity_ledger.rs", Why::Wording, 2),
        ("src/remote_branch.rs", Why::Wording, 1),
        ("src/remote_history.rs", Why::Wording, 2),
        ("src/remote_write_registry.rs", Why::SymbolName, 3),
        ("src/remote_write_registry.rs", Why::Wording, 1),
        ("src/session_map.rs", Why::Wording, 1),
        ("src/sftp.rs", Why::SymbolName, 22),
        ("src/sftp.rs", Why::Wording, 6),
        ("src/sftp_move_ledger.rs", Why::SymbolName, 2),
        ("src/sftp_move_ledger.rs", Why::Wording, 1),
        ("src/skill_host.rs", Why::OldId, 2),
        ("src/ssh_source.rs", Why::Wording, 6),
        ("src/structural_scan.rs", Why::OldId, 1),
        ("src/structural_scan.rs", Why::SymbolName, 3),
        ("src/tool_registry.rs", Why::OldId, 6),
        ("src/tool_registry.rs", Why::SymbolName, 8),
        ("src/tool_registry.rs", Why::Wording, 1),
        ("src/usage.rs", Why::Wording, 2),
    ];

    /// 三档各自的处数。**针全部运行期拼**〔同 `scanning_guard_registry` 头注里
    /// `attr` 那一处的写法〕—— 本文件自己在扫描面里（见下），字面量写在这儿
    /// 会把量具自己算进被测量。
    ///
    /// **顺序即口径**：先把「crate 目录 / 包名的那个拼写」整个剥掉再数 ——
    /// 那是**住址**，不是名字（本拍刻意不碰，理由在下面那条判据的头注里）。
    fn old_name_counts(text: &str) -> [usize; 3] {
        let d = "-";
        let u = "_";
        let stem_id = format!("remote{d}daemon");
        let stem_sym = format!("remote{u}daemon");
        // 住址拼写：crate 目录 `…-proto` 与它的 Rust 模块名 `…_proto`
        let rest = text
            .replace(&format!("{stem_id}{d}proto"), "")
            .replace(&format!("{stem_sym}{u}proto"), "");
        let sym =
            rest.matches(&stem_sym).count() + rest.matches(&format!("Remote{}aemon", "D")).count();
        let old_id = rest.matches(&stem_id).count();
        let wording = rest.matches(&format!("远端 {}", "daemon")).count()
            + rest.matches(&format!("远端{}", "daemon")).count();
        [sym, old_id, wording]
    }

    fn count_of(text: &str, w: Why) -> usize {
        let [sym, old_id, wording] = old_name_counts(text);
        match w {
            Why::SymbolName => sym,
            Why::OldId => old_id,
            Why::Wording => wording,
        }
    }

    /// `pub const TOOLS` 那个常量的**体**（配对方括号之间那一段），注释已剥掉。
    ///
    /// ⚠ 剥注释是**有意的**：闭集的**数据**不许再叫旧名字，而注释里的墓碑与病史
    /// （`Why::OldId` 那一档）**刻意保留**。两件事分开判。
    ///
    /// 🔴 剥法**借共享原语** [`guard_core::strip_comment_lines`]，不自己写第二份 ——
    /// `structural_scan.rs` 那张 `TRANSFORMERS` 登记表背后的判据逐字
    /// 「换个名字的同一份剥法仍然是第二份剥法」，**本函数第一版就是那样，当场被它逮到**。
    /// 顺序也照它的纪律：**先剥整份，再切块**（`strip_comment_lines` 头注的 `K-R25` 那一段）。
    fn tools_literal_data() -> String {
        let me = guard_core::strip_comment_lines(include_str!("tool_registry.rs"));
        let opener = "pub const TOOLS: &[ToolSpec] = &[";
        let at = me.find(opener).expect("取不到 TOOLS 常量 —— 扫描器失效了");
        let start = at + opener.len();
        let mut depth = 1i32;
        let mut end = start;
        for (i, c) in me[start..].char_indices() {
            match c {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        end = start + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        assert!(end > start, "配对没找到收尾的 `]`");
        me[start..end].to_string()
    }

    /// ★ `KR81D2` **正面（零命中守卫）**：闭集那张表的**数据里**，旧名字一处都没有。
    ///
    /// # 死值验（`KR81D2` 逐字要的那一向）
    ///
    /// 把改过的任意一处改回旧名（`id:` 那一格、或 `display_name:` 那一格）⇒ 本条红。
    ///
    /// # 它守什么、**不守什么**
    ///
    /// 守的是**闭集的数据**。注释里的墓碑、别处 `.rs` 里的存量，本条一概不管 ——
    /// 那一半归下面那条登记 ＋ 棘轮。**两条合起来才是那一格，单独任何一条都不够。**
    #[test]
    fn the_old_backend_name_is_gone_from_the_closed_set_itself() {
        let data = tools_literal_data();
        // 反向自检①：尺子够得着 —— 取到的真是那张表，不是一段空串或半截。
        assert!(
            data.len() > 8_000 && data.matches("ToolSpec {").count() == TOOLS.len(),
            "取到的 `TOOLS` 体不对：{} 字节 / {} 个 `ToolSpec {{`（应为 {} 个）——\
             先查提取器，别改断言",
            data.len(),
            data.matches("ToolSpec {").count(),
            TOOLS.len()
        );
        // 反向自检②：阳性对照 —— 把旧名字塞回一份副本里，量具必须数得出来。
        let poisoned = data.replace(
            &format!("id: \"{BACKEND_ID}\""),
            &format!("id: \"remote{}daemon\"", "-"),
        );
        assert_ne!(
            poisoned, data,
            "变异没落地：`id: \"{BACKEND_ID}\"` 没在那段里"
        );
        assert_eq!(
            old_name_counts(&poisoned)[1],
            1,
            "量具在阳性对照上数不出来 —— 它此刻无效，下面那条断言是空真"
        );
        // 正题
        assert_eq!(
            old_name_counts(&data),
            [0, 0, 0],
            "\n闭集那张表的**数据**里还留着旧名字（[符号名, 旧 id, 措辞]）。\n\
             用户 09-12 逐字：「一个后端要两处使用 / 即远程 daemon 就是远程本地机器的后端」——\n\
             远端那台上跑的那一份是**那台机器的本地后端**，不是「远端的 daemon」。\n\
             ⚠ 这一格已经花过一次真钱（`ROADMAP#KU26`：Linux 裸 exe 装出来没有本机后端）。"
        );
    }

    /// ★ `KR81D2` **另一半（登记 ＋ 递减棘轮）**：还没改的每一处都登记着，而且只许变少。
    ///
    /// # 它买的是「改了哪些 / 没改哪些 / 为什么」这句话**有分母**
    ///
    /// 人群 = `src/bridge/src` ＋ `src/bridge/crates` 两棵树的 `.rs`（**现算**，不写死份数）。
    /// 三向都判：
    ///   ① 盘上带旧名而 [`SITES`] 里没有 ⇒ 红（**别再往盘上加旧名**）；
    ///   ② `SITES` 里有而盘上已经没有 ⇒ 红（**账不许挂空号**）；
    ///   ③ 盘上比登记的多 ⇒ 红（棘轮只许降）。
    ///
    /// # 🔴 射程与**刻意不管**的两样，写死在这里
    ///
    /// - **crate 目录（`…-proto`）与包名（`cc-monitor-remote`）本拍不碰**，
    ///   而且它们**根本不进这把尺子**（`old_name_counts` 第一步就把那个拼写剥掉了）。
    ///   理由不是嫌麻烦：改那两样要**同拍**改发版流水线的产物名、`embedded-daemons/`
    ///   的文件名约定、`tauri.sidecar.conf.json` 的 `externalBin`、`build.rs` 的清单
    ///   与 CI —— 而件文件 `§0d` 逐字「**不改发版流水线**（`KU26` 那一步归 `K-R42`）」。
    ///   ⇒ 它是**住址**，不是名字；名字改对了，住址跟着搬是另一件事。
    /// - **`src/backend/` 那棵树**（另一个 workspace）与**前端 `src/**.ts`**
    ///   不在本尺子的面里。⚠ 这是**判不了**，不是「那边干净」——
    ///   现打：前端 30 余处、daemon 树 26 处，逐条读数落在 `evidence/K-R81-….md`。
    ///
    /// # ⚠ 本文件自己在面里（`K-R31` 那一形，`scanning_guard_registry` 登记为「第五形」）
    ///
    /// `scan_tree!` 按构造摘掉调用者那一份 —— 而调用者恰恰是**闭集的家**，
    /// 摘掉等于在最该看的那一份上瞎掉 ⇒ 用 `include_str!` 把自己显式加回来。
    /// 对价是本文件的针**全部运行期拼**（见 [`old_name_counts`]），
    /// 否则量具自己会被自己数进去。
    #[test]
    fn every_place_that_still_says_the_old_name_is_registered_and_only_shrinks() {
        use std::collections::BTreeMap;
        use std::path::{Path, PathBuf};

        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let src_root = manifest.join("src");
        let crates_root = manifest.join("crates");
        let mut files: Vec<(PathBuf, String)> = guard_core::scan_tree!(&src_root, &["rs"]);
        files.extend(guard_core::scan_tree!(&crates_root, &["rs"]));
        // 把自己那一份加回来（上面头注那一段说的就是这里）。
        files.push((
            manifest.join("src").join("tool_registry.rs"),
            include_str!("tool_registry.rs").to_string(),
        ));
        // 地板：尺子真的够得着一棵树（空集会让下面三向全部空真）。
        assert!(
            files.len() > 100,
            "扫描面只有 {} 份 `.rs` —— 剥法坏了，下面三向都是空真",
            files.len()
        );

        let mut got: BTreeMap<(String, Why), usize> = BTreeMap::new();
        for (p, text) in &files {
            let rel = p
                .strip_prefix(manifest)
                .unwrap_or(p)
                .to_string_lossy()
                .replace('\\', "/");
            for w in [Why::SymbolName, Why::OldId, Why::Wording] {
                let n = count_of(text, w);
                if n > 0 {
                    got.insert((rel.clone(), w), n);
                }
            }
        }
        let want: BTreeMap<(String, Why), usize> = SITES
            .iter()
            .map(|(p, w, n)| (((*p).to_string(), *w), *n))
            .collect();

        let mut unregistered = Vec::new();
        let mut grown = Vec::new();
        for (k, n) in &got {
            match want.get(k) {
                None => unregistered.push(format!("{} · {:?} × {n}", k.0, k.1)),
                // 🔴 **逐格等号，不是 `<=`** —— 只判「涨了」的话，
                //    「把上限调上去让今天好过」这一手是**静默通过**的
                //    （`references/testing.md` 硬规则 12 明禁那一手，而纪律这一档
                //     在本仓已经被证伪过）。等号让那一手当场红。
                Some(cap) if n != cap => grown.push(format!(
                    "{} · {:?}：登记 {cap}，盘上 {n}（{}）",
                    k.0,
                    k.1,
                    if n > cap { "涨了" } else { "少了" }
                )),
                Some(_) => {}
            }
        }
        let stale: Vec<String> = want
            .keys()
            .filter(|k| !got.contains_key(*k))
            .map(|k| format!("{} · {:?}", k.0, k.1))
            .collect();

        assert!(
            unregistered.is_empty(),
            "\n这几处带着后端的**旧名字**而 `SITES` 里没有登记：\n  {}\n\n\
             ⇒ 两条出路：**把名字改对**（它是「后端」，在两台机器上各跑一份），\n\
             或者往 `SITES` 里加一行、并在 `Why` 那个闭集里说清**为什么这一拍改不动**。\n\
             ⚠ 不许为了变绿就往表里塞一行了事 —— 那正是这条账要防的。",
            unregistered.join("\n  ")
        );
        assert!(
            stale.is_empty(),
            "\n`SITES` 里这几行在盘上已经没有对应物了：\n  {}\n\n\
             ⇒ 那一处已经改完了，**把这一行删掉**（账不许挂空号：\n\
             一张挂着空号的表会让人以为债还在那儿，而它其实早还了）。",
            stale.join("\n  ")
        );
        assert!(
            grown.is_empty(),
            "\n旧名字的处数与登记对不上（这张账逐格按**等号**认）：\n  {}\n\n\
             ⇒ **涨了**：别再往盘上加旧名，也别把上限调上去让今天好过\n\
             （`references/testing.md` 硬规则 12 逐字：把上限调上去不是出路）。\n\
             ⇒ **少了**：好事 —— 把那一行的数**改小**，让这张账继续说真话。",
            grown.join("\n  ")
        );

        // 读数印出来 —— 「改了哪些 / 没改哪些」这句话要有一个**数**（现算，不写死）。
        let total: usize = got.values().sum();
        let by_kind: Vec<String> = [Why::SymbolName, Why::OldId, Why::Wording]
            .iter()
            .map(|w| {
                let n: usize = got.iter().filter(|(k, _)| k.1 == *w).map(|(_, v)| v).sum();
                format!("{w:?} {n}")
            })
            .collect();
        println!(
            "【KR81D2 存量读数】面 = `src/bridge/src` ＋ `src/bridge/crates` 共 {} 份 `.rs`（现算）· \
             还带旧名的 {} 份 / {total} 处（{}）· 登记 {} 行 · \
             ⚠ 面外判不了：`remote{}daemon{}proto/` 那棵树与前端 `src/**/*.ts`",
            files.len(),
            got.keys().map(|k| k.0.clone()).collect::<std::collections::HashSet<_>>().len(),
            by_kind.join(" · "),
            SITES.len(),
            "-",
            "-"
        );
    }

    /// **同一条纪律也管 [`Carrier`]** —— 同 `TouchedFile` 那条的理由：
    /// 字段纪律只扫上面两层，审计那条手法**再下移一层仍然有效**。
    #[test]
    fn carrier_fields_follow_the_same_discipline() {
        let code = production_code(include_str!("tool_registry.rs"));
        field_discipline_of(&code, "Carrier", "Carrier")
            .require(4, "Carrier 字段纪律")
            .unwrap();
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 🔴 `K-R69` / `KR69D1`：**本机有一条 `ccm` 落点，而且它与远端那条同源**
    // ═══════════════════════════════════════════════════════════════════════

    /// 闭集里所有**末段是 `ccm` 那个词**的落点，按「在哪台机器上」分。
    ///
    /// ⚠ 人群**现算**、不写死一个名单〔`13b`〕：`TOOLS` 是唯一一份，
    /// 而 `ccm` 那个词的唯一住址是 `local_backend::CCM_ENTRY_WORD`。
    /// 末段允许带一个尾 `*`（本机那条要盖住 Windows 上的 `.exe`，见它自己的 note）。
    fn ccm_landing_sites() -> Vec<(&'static str, HostScope)> {
        let word = crate::backend::control::local_backend::CCM_ENTRY_WORD;
        TOOLS
            .iter()
            .flat_map(|t| t.touches())
            .filter(|f| {
                let last = f.path.rsplit('/').next().unwrap_or(f.path);
                last == word || last == format!("{word}*")
            })
            .map(|f| (f.path, f.host))
            .collect()
    }

    /// `KR69D1` 正面：**本机侧有落点，远端侧也有，而且它们不是同一条。**
    ///
    /// # 立件时这一条是红的（那正是本件的题面）
    ///
    /// 09-12 现打（量于 `79bf97d`）：闭集里末段是 `ccm` 的落点**恰好 1 条** ——
    /// `TOOLS` 里 id 为 `ccm` 那条声明的 `~/.local/bin/ccm`，`host: Remote` ⇒ **本机侧 0 条**。
    /// 而别名生成器（`launcher-diagnostics.ts::buildAliasLine`）吐的是**裸 `ccm`**，
    /// 靠 PATH 解析 ⇒ 用户贴上去之后解析到的仍是他自己那份旧的。
    /// ⇒ 用户 `K34` 逐字「装了新版后 `~/.local/bin/ccm` 可以干净退役」
    /// **在结构上做不到**：退役没有承接方。
    ///
    /// # 死值验（`KR69D1` 逐字要的第一向）
    ///
    /// 把本机那条 `TouchedFile` 摘掉 ⇒ 本条**必须红**。
    ///
    /// # ⚠ 它守什么、**不守什么**
    ///
    /// 守的是**申报**（这张表里有没有这条落点、在哪台机器上）。
    /// 「那个文件真的被放下去了吗」「放下去的是不是后端本体」由
    /// `local_backend` 那两条管（`the_local_ccm_entry_is_a_copy_of_the_backend_itself`
    /// 与 `the_resolution_path_really_puts_the_local_ccm_entry_down`）。
    /// **三条合起来才是那一格，单独任何一条都不够。**
    #[test]
    fn the_closed_set_declares_a_ccm_landing_site_on_this_machine_too() {
        let sites = ccm_landing_sites();
        let local: Vec<&str> = sites
            .iter()
            .filter(|(_, h)| matches!(h, HostScope::Client | HostScope::Either))
            .map(|(p, _)| *p)
            .collect();
        let remote: Vec<&str> = sites
            .iter()
            .filter(|(_, h)| matches!(h, HostScope::Remote))
            .map(|(p, _)| *p)
            .collect();
        // 反向自检：人群塌了（没有任何 `ccm` 落点）时下面两条会**零命中地**分别红/绿，
        // 先把「尺子还够得着被测对象」这件事断出来。
        assert!(
            sites.len() >= 2,
            "闭集里末段是 `ccm` 的落点只有 {} 条（分母 = `TOOLS` 全部 touches，现算）——\n\
             本条要的是**两条**：本机一条、远端一条。实得 {sites:?}",
            sites.len()
        );
        assert!(
            !local.is_empty(),
            "闭集里**本机侧一条 `ccm` 落点都没有**（远端侧 {remote:?}）。\n\
             这正是 `K-R69` 立件时的读数：app 从来没有在本机装过 `ccm`，\n\
             于是用户 `K34` 逐字「装了新版后 `~/.local/bin/ccm` 可以干净退役」\n\
             **没有承接方** —— 缺的不是一次真机读数，是这个入口本身。"
        );
        assert!(
            !remote.is_empty(),
            "远端那条 `ccm` 落点没了 —— 那是 `sftp::install_remote_ccm_helper` 推过去的那份，\n\
             本件只**加**本机那条，不许把远端那条顺手弄丢（实得本机 {local:?}）"
        );
        // 两条不许是同一个路径：同一个串出现两次说明有人把 host 抄错了，
        // 而那时「本机有一条」是**靠一条远端的记录冒充的**。
        for l in &local {
            assert!(
                !remote.contains(l),
                "本机那条与远端那条是同一个路径 {l:?} —— 落点撞在一起，\n\
                 而本机那份**不许**写进 `~/.local/bin`：那是用户旧 `ccm` 住的地方，\n\
                 `K34` 逐字「原本的配置**要手动删除**」，产品一个字节都不动它。"
            );
        }
    }

    /// `KR69D1` 的**同源那一半（申报侧）**：申报的那条本机路径，
    /// 真的能盖住我们生产上放下去的那个文件名。
    ///
    /// # 没有这一条会怎样
    ///
    /// 名字的唯一真相源是 `local_backend::local_ccm_entry_name()`（后缀由 `build.rs`
    /// 按 `TARGET` 算，Windows 上是 `ccm.exe`）。这张表里写的是一个**常量串**。
    /// 两边一漂，审计页会在 Windows 上对着一个**我们真的装了**的东西显示「缺失」——
    /// 那正是本模块头注禁的「对能用的安装报假警报」。
    #[test]
    fn the_declared_local_ccm_path_really_matches_the_name_we_install() {
        let name = crate::backend::control::local_backend::local_ccm_entry_name();
        let declared: Vec<&str> = ccm_landing_sites()
            .into_iter()
            .filter(|(_, h)| matches!(h, HostScope::Client | HostScope::Either))
            .map(|(p, _)| p)
            .collect();
        assert_eq!(
            declared.len(),
            1,
            "本机侧的 `ccm` 落点不是恰好一条（实得 {declared:?}）——\
             多一条就是多一份要跟着改的东西（`K33`：所有命令只许有一处）"
        );
        let last = declared[0].rsplit('/').next().unwrap_or(declared[0]);
        let ok = match last.strip_suffix('*') {
            Some(prefix) => name.starts_with(prefix),
            None => last == name,
        };
        assert!(
            ok,
            "闭集里申报的本机落点末段是 {last:?}，而生产上真放下去的名字是 {name:?} —— \n\
             两边漂了。名字的唯一真相源是 `local_backend::local_ccm_entry_name()`；\n\
             这一格漂开的后果不是编译错，是审计页在**装得好好的**机器上显示「缺失」。"
        );
    }

    /// 有围栏的块必须可卸载——否则用户没法干净地退出。
    #[test]
    fn fenced_block_implies_uninstallable() {
        for t in TOOLS {
            if t.touches().any(|f| f.effect == TouchEffect::FencedBlock) {
                assert!(
                    t.uninstallable,
                    "{} 往用户文件里插了围栏块，就必须能按围栏剥离",
                    t.id
                );
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // `K-R63`：**申报 ↔ 现实** —— 覆盖全表的一条性质，不是逐工具一条 `assert`
    //
    // # 病理（件文件 `§0b`）
    //
    // 这张表上两个 `bool`，先前**各只有半边被守**：
    //   · `installable` —— 一条**专名**判据（只服务 `cc-bus` 一个工具）；
    //   · `uninstallable` —— 上面那条 `fenced_block_implies_uninstallable` 只守
    //     「有围栏 ⇒ 必须声明可卸」（**少报**那一向），**多报**（声明可卸而盘上
    //     根本没有卸载实现）一条判据都没有。
    // PM 09-11 的刀 C 实打（量于 `cd26954`）：把 `cc-acct-iso` 的 `uninstallable`
    // 由 `false` 翻成 `true` ⇒ 全表 **1379 条一条没红**。
    // 〔`K-R63` 实现方 09-11 在本件分支尖上复打同一刀：**红 1 条，就是下面这一条**
    //  （`-p monitor` 基线 1384 → 1383 passed / 1 failed）。〕
    //
    // # 为什么处方不是「再补一条专名 `assert`」
    //
    // 那只是把静默从 1 个工具挪到下一个工具（`§0c` 逐字写死的失效方向）。
    // ⇒ 做成**一条性质**：左边现读字段值，右边现钉盘上那个符号，两边 `assert_eq!`。
    // ⚠ 失效方向也写死在判据的**名字**上：名字里出现任何一个工具 id ⇒ 不算兑现。
    //    这一条自己也有判据（见下面那条自守）。
    // ═══════════════════════════════════════════════════════════════════════

    /// 一处**实现的住址**：给人读的 `<文件>.rs::<符号>` ＋ 给机器钉的**逐字签名**。
    ///
    /// 两格互相校验（下面那条性质会断言符号名与签名里那个 `fn` 名逐字相等）：
    /// 只留住址是一句没人核的话；只留签名，改了名没人读得出它指哪儿。
    /// 而 `addr` 这一格**同时**被全仓那条符号地址判据盯着
    /// （`structural_scan.rs::symbol_addresses` 抽、全仓解析）—— 实现改名 / 删掉，那一条先红。
    struct ImplSite {
        addr: &'static str,
        definition: &'static str,
    }

    /// 一个工具的装 / 卸实现**住在哪份文件**。
    ///
    /// `text` 是 `include_str!` 现取的整份内容，**不是一个路径串** —— 路径串会烂，
    /// 而 `include_str!` 指错了地方**编都编不过**。
    struct ImplHome {
        addr: &'static str,
        text: &'static str,
    }

    /// `TOOLS` 每一条的两格申报，各自该去盘上哪儿对拍。
    ///
    /// `None` = **今天盘上根本没有这么一处**（形状抄 `fenced_block.rs::FENCE_SHAPES`
    /// 的 `uninstall_site`）。
    ///
    /// ⚠ **为什么缺口只能写 `None`，不能写一个「它将来会住哪」的地址**：一个不存在的
    /// 符号一旦写成 `<文件>.rs::<符号>`，全仓那条符号地址判据当场把它判成
    /// 「找不到这个符号」。⇒ 缺口用 `None` 表示，而「`None` 今天还成不成立」
    /// 由下面那道**负向扫描**守着，不靠人记得 —— `remote-daemon` 正是栽在这一格上。
    struct Claim {
        tool: &'static str,
        home: Option<ImplHome>,
        install: Option<ImplSite>,
        uninstall: Option<ImplSite>,
    }

    /// **唯一一份**对拍表。覆盖由下面那条性质的第 ① 步钉死（多一条少一条都红）。
    fn claims() -> Vec<Claim> {
        const SFTP: &str = include_str!("sftp.rs");
        const PROFILE_INSTALLER: &str = include_str!("profile_installer.rs");
        const MCP: &str = include_str!("mcp.rs");
        const CC_BUS_DEPLOY: &str = include_str!("cc_bus_deploy.rs");
        const ACCT_ISO_DEPLOY: &str = include_str!("acct_iso_deploy.rs");
        let sftp = || ImplHome {
            addr: "sftp.rs",
            text: SFTP,
        };
        let profile = || ImplHome {
            addr: "profile_installer.rs",
            text: PROFILE_INSTALLER,
        };
        // 两条 profile 系工具走的是**同一台安装器**（分岔在 `plan_install` / `plan_uninstall`，
        // 落盘那一整套共用）—— 与 `fenced_block.rs::FENCE_SHAPES` 里那两行同源。
        let profile_install = || {
            ImplSite {
            addr: "profile_installer.rs::install_to_profile",
            definition: "pub fn install_to_profile(\n    path: &PathBuf,\n    command_name: &str,\n    include_cc_function: bool,\n) -> Result<(), String> {",
        }
        };
        let profile_uninstall = || ImplSite {
            addr: "profile_installer.rs::uninstall_from_profile",
            definition: "pub fn uninstall_from_profile(path: &PathBuf) -> Result<(), String> {",
        };
        vec![
            Claim {
                tool: "ccm",
                home: Some(sftp()),
                install: Some(ImplSite {
                    addr: "sftp.rs::install_remote_ccm_helper",
                    definition: "pub async fn install_remote_ccm_helper(\n    cfg: RemoteConfig,\n    profile: String,\n) -> Result<String, String> {",
                }),
                uninstall: Some(ImplSite {
                    addr: "sftp.rs::uninstall_remote_ccm_helper",
                    definition: "pub async fn uninstall_remote_ccm_helper(\n    cfg: RemoteConfig,\n    profile: String,\n) -> Result<String, String> {",
                }),
            },
            Claim {
                tool: "cc-bus",
                home: Some(ImplHome {
                    addr: "cc_bus_deploy.rs",
                    text: CC_BUS_DEPLOY,
                }),
                install: Some(ImplSite {
                    addr: "cc_bus_deploy.rs::deploy_local_cc_bus",
                    definition: "pub async fn deploy_local_cc_bus() -> Result<CcBusDeployReport, String> {",
                }),
                uninstall: None,
            },
            Claim {
                tool: "cc-acct-iso",
                home: Some(ImplHome {
                    addr: "acct_iso_deploy.rs",
                    text: ACCT_ISO_DEPLOY,
                }),
                install: Some(ImplSite {
                    addr: "acct_iso_deploy.rs::deploy_remote_acct_iso",
                    definition: "pub async fn deploy_remote_acct_iso(cfg: RemoteConfig, dest_dir: String) -> Result<String, String> {",
                }),
                uninstall: None,
            },
            Claim {
                tool: "backend",
                home: Some(sftp()),
                install: Some(ImplSite {
                    addr: "sftp.rs::deploy_remote_daemon",
                    definition: "pub async fn deploy_remote_daemon(cfg: RemoteConfig) -> Result<String, String> {",
                }),
                uninstall: Some(ImplSite {
                    addr: "sftp.rs::uninstall_remote_daemon",
                    definition: "pub async fn uninstall_remote_daemon(cfg: RemoteConfig) -> Result<String, String> {",
                }),
            },
            Claim {
                tool: "project-mcp",
                home: Some(ImplHome {
                    addr: "mcp.rs",
                    text: MCP,
                }),
                install: Some(ImplSite {
                    addr: "mcp.rs::write_project_mcp_server",
                    definition: "pub async fn write_project_mcp_server(\n    project_dir: String,\n    name: String,\n    server: Value,\n) -> Result<(), String> {",
                }),
                uninstall: Some(ImplSite {
                    addr: "mcp.rs::remove_project_mcp_server",
                    definition: "pub async fn remove_project_mcp_server(project_dir: String, name: String) -> Result<(), String> {",
                }),
            },
            Claim {
                tool: "posix-rc-aliases",
                home: Some(profile()),
                install: Some(profile_install()),
                uninstall: Some(profile_uninstall()),
            },
            Claim {
                tool: "powershell-profile",
                home: Some(profile()),
                install: Some(profile_install()),
                uninstall: Some(profile_uninstall()),
            },
            // **我们从不装它** ⇒ 没有「它的实现该住哪」这回事。这一行的 `home: None`
            // 不是手写的豁免：下面那条性质断言 `home.is_none()` **当且仅当**
            // `destination` 是 `NotInstalledByUs` —— 换句话说这一格由类型系统里那个
            // 变体说了算，不由填表的人说了算。
            Claim {
                tool: "claude-code",
                home: None,
                install: None,
                uninstall: None,
            },
        ]
    }

    /// 从逐字签名里抠出 `pin_definition` 要的**赋值前缀**（签名到第一个 `(` 为止）。
    /// **算出来的，不再写第二份字面量**〔`13b`〕。
    fn assign_prefix_of(definition: &str) -> &str {
        definition
            .split('(')
            .next()
            .unwrap_or(definition)
            .trim_end()
    }

    /// 从逐字签名里抠出那个 `fn` 名。
    fn fn_name_of(definition: &str) -> &str {
        assign_prefix_of(definition)
            .rsplit(' ')
            .next()
            .unwrap_or_default()
    }

    /// ★★ `KR63D1` ＋ `KR63D2`：**`TOOLS` 每一条的两格申报，都必须与盘上真有没有那个实现一致。**
    ///
    /// 左边现读字段值（`installable` / `uninstallable`），右边用
    /// `structural_scan.rs::pin_definition` 去钉那个装 / 卸实现的**逐字签名**
    /// （它同时守住「只被定义一次」：追加一个同名定义也会红）。两边 `assert_eq!`。
    ///
    /// **两个方向都判**：
    ///   · 多报（声明可装 / 可卸而实现不在）⇒ 右边 `false`、左边 `true` ⇒ 红；
    ///   · 少报（实现在而字段写着不行）⇒ 反过来 ⇒ 红。
    ///     少报**不是假想** —— 本条落地当场逮到 `remote-daemon`：`uninstall_remote_daemon`
    ///     是设置面板上的按钮，而字段写着 `uninstallable: false`。
    ///
    /// **`None` 那一侧靠负向扫描兜底**（不然「今天没有卸口」这句话永远没人核）：
    /// 那个工具的家里出现一个**没人认领**的 `fn uninstall… / remove… / strip… / purge…`
    /// ⇒ 红。动词表的分母如实写在这里：**登记过的就这四个**，不是穷举 ——
    /// 一个叫别的名字的卸载实现今天扫不到，那一格判不了，不假装覆盖。
    ///
    /// ⚠ **装那一侧今天没有负向扫描，而这不是漏写**：`install: None` 的行今天只有
    /// `claude-code` 一条，它连家都没有（`NotInstalledByUs`）⇒ 人群是空的，
    /// 写一条扫不到任何东西的扫描买不到牙。真出现「有家而声明装不了」的行，
    /// 下面那一支会 `panic!` 点名，**不会静默放过**。
    #[test]
    fn every_tool_declares_install_and_uninstall_as_the_implementations_really_are() {
        use crate::structural_scan::pin_definition;

        let claims = claims();

        // ① 覆盖：与 `TOOLS` 一一对应，多一条少一条都红（这一步买的是「全表」二字）。
        let mut got: Vec<&str> = claims.iter().map(|c| c.tool).collect();
        let mut want: Vec<&str> = TOOLS.iter().map(|t| t.id).collect();
        got.sort_unstable();
        want.sort_unstable();
        assert_eq!(
            got, want,
            "对拍表与 TOOLS 对不上 —— 加了工具却没登记它的装 / 卸实现住哪，\
             那一条就悄悄不在这条性质的射程里了"
        );

        // ② 反向自检：`pin_definition` 真的会说「不在」（否则下面全是空真）。
        assert!(pin_definition("fn a() {}\n", "fn b() {}", "fn b", "自检").is_err());
        assert!(pin_definition("fn a() {}\n", "fn a() {}", "fn a", "自检").is_ok());
        // ②b 反向自检：负向扫描在**真树**上不是零命中的（零命中 ⇒ 那一半是空真）。
        assert!(
            crate::structural_scan::fn_names_starting_with(include_str!("sftp.rs"), &["uninstall"])
                .contains(&"uninstall_remote_daemon".to_string()),
            "负向扫描在真树上零命中 —— 它此刻无效，先查剥法别改断言"
        );

        // 认领集：哪些实现符号已经被某一行认走了（负向扫描要用）。
        let claimed: HashSet<&str> = claims
            .iter()
            .flat_map(|c| [c.install.as_ref(), c.uninstall.as_ref()])
            .flatten()
            .map(|s| fn_name_of(s.definition))
            .collect();

        let mut checked = 0usize;
        for t in TOOLS {
            let c = claims.iter().find(|c| c.tool == t.id).expect("① 已经钉过");

            // ③ 「有没有家」不由填表的人说了算，由 `destination` 那个变体说了算。
            //    🔴 〔`K-R81`〕多载体之后走 `Provisioning::of_tool` 的**聚合规则**
            //    （全部载体都是 `NotInstalledByUs` 才算「不是装出来的」）——
            //    那条规则只许有一个住址，这里不再手写第二份 `matches!`〔`13b`〕。
            assert_eq!(
                c.home.is_none(),
                Provisioning::of_tool(t) == Provisioning::NotAnInstall,
                "`{}`：`home` 那一格与 `destination` 打架 —— 「我们不装它」与\
                 「它的装 / 卸实现住在某份文件里」有一句是假的",
                t.id
            );

            for (field, declared, site, verbs) in [
                ("installable", t.installable, c.install.as_ref(), &[][..]),
                (
                    "uninstallable",
                    t.uninstallable,
                    c.uninstall.as_ref(),
                    &["uninstall", "remove", "strip", "purge"][..],
                ),
            ] {
                checked += 1;
                let real = match (c.home.as_ref(), site) {
                    (Some(home), Some(s)) => {
                        // 住址与签名互相校验：符号名必须逐字相等，文件必须就是那个家。
                        assert_eq!(
                            s.addr,
                            format!("{}::{}", home.addr, fn_name_of(s.definition)),
                            "`{}` 的 `{field}` 那一格：住址与逐字签名对不上 —— \
                             其中一格是摆设",
                            t.id
                        );
                        pin_definition(
                            home.text,
                            s.definition,
                            assign_prefix_of(s.definition),
                            &format!("`{}` 的 `{field}` 背后那个实现", t.id),
                        )
                        .is_ok()
                    }
                    (_, None) => {
                        // 负向扫描：家里躺着一个没人认领的同族实现 ⇒ 「今天没有」这句话是假的。
                        if let Some(home) = c.home.as_ref() {
                            if verbs.is_empty() {
                                panic!(
                                    "`{}` 声明 `{field}: {declared}` 而没有登记实现住址，\
                                     它却有家（{}）—— 这一形今天没有负向扫描，\
                                     写一条再走（别静默放过）",
                                    t.id, home.addr
                                );
                            }
                            let stray: Vec<String> =
                                crate::structural_scan::fn_names_starting_with(home.text, verbs)
                                    .into_iter()
                                    .filter(|n| !claimed.contains(n.as_str()))
                                    .collect();
                            assert!(
                                stray.is_empty(),
                                "`{}` 的 `{field}` 登记着「今天盘上没有这么一处」，\
                                 而它家（{}）里躺着没人认领的 {stray:?} —— \
                                 要么它就是那个实现（那就登记进对拍表并把字段翻过来），\
                                 要么它不是（那就说清它是什么）。\n\
                                 ⚠ `remote-daemon` 就是这么假申报了一个月的。",
                                t.id,
                                home.addr
                            );
                        }
                        false
                    }
                    (None, Some(s)) => panic!(
                        "`{}` 没有家，却给 `{field}` 登记了实现住址 {:?}",
                        t.id, s.addr
                    ),
                };
                assert_eq!(
                    declared,
                    real,
                    "`{}` 的 `{field}` 申报为 {declared}，而盘上那个实现{}。\n\
                     这两句话必须一致 —— 配置面那一列「能否装/撤」直接印到用户眼前：\n\
                     多报 = 一个点了没反应的按钮；少报 = 按钮就在旁边而页面写着做不到。\n\
                     ⚠ 只改注释没有用：本条读的是**字段值**，不是注释里的词频。",
                    t.id,
                    if real { "在" } else { "不在" }
                );
            }
        }
        // 计数自检：两格 × 全表，一格都没跳过。
        assert_eq!(
            checked,
            2 * TOOLS.len(),
            "只对拍了 {checked} 格，而全表应有 {} 格",
            2 * TOOLS.len()
        );
    }

    /// ★★ `KR63D1` / `KR63D2` 把**失效方向**写死在名字上：
    /// **判据的名字里出现任何一个工具 id ⇒ 本件不算兑现。**
    ///
    /// 一条名字里带工具名的判据，读的人会以为「那一格有人守」，而它守的只有那一个工具 ——
    /// `K-R60` 留下的就是这样一颗钉子，`K-R63` 把它收掉了。这一条让那件事**别再回来**。
    #[test]
    fn the_property_that_pins_both_declarations_is_not_named_after_any_tool() {
        const NAME: &str =
            "every_tool_declares_install_and_uninstall_as_the_implementations_really_are";
        let me = include_str!("tool_registry.rs");
        // 反向自检：那条判据真的叫这个名字（改了名而没改这里 ⇒ 本条先红）。
        assert_eq!(
            me.matches(&format!("fn {NAME}(")).count(),
            1,
            "本文件里找不到（或不止一个）`{NAME}` —— 它改名了，本条此刻在空转"
        );
        for t in TOOLS {
            let snake = t.id.replace('-', "_");
            assert!(
                !NAME.contains(t.id) && !NAME.contains(&snake),
                "判据名 `{NAME}` 里出现了工具 id `{}`（或它的 snake 形 `{snake}`）—— \
                 那就又是一颗只服务一个工具的钉子",
                t.id
            );
        }
    }
}

#[cfg(test)]
mod environment_tests {
    use super::*;
    use std::collections::BTreeSet;

    /// `KR60D1` ③ 用的抽取器 —— **直接用盘上已有那一把**，不自己再写一个。
    ///
    /// 〔`K-R60` 09-11 自抓〕本条第一版手写了一个同形的抠取器（找 `.rs::`、向前吃路径、
    /// 向后吃符号），写完才发现 `structural_scan::symbol_addresses` 逐字就是这件事，
    /// 而且它那一侧**更强**：还会去全仓解析那个符号今天在不在、有没有搬家。
    /// 抄一份被测逻辑正是本仓反复判过的那族病 ⇒ 删掉自己那份，改调它。
    fn addresses_in(why: &str) -> Vec<(usize, String, String, bool)> {
        crate::structural_scan::symbol_addresses(why)
    }

    /// `KR60D1` ①：**清单是一个闭集，而且只有一份。**
    ///
    /// 两半（`TOOLS` 派生的 + [`UNMANAGED_ENV`] 手写的）**不许重叠**，id 不许重名 ——
    /// 重了就等于同一个环境项有两个住址，而那正是本件要治的病。
    #[test]
    fn the_environment_is_one_closed_list() {
        let env = environment();
        let ids: BTreeSet<&str> = env.iter().map(|e| e.id).collect();
        assert_eq!(
            ids.len(),
            env.len(),
            "闭集里有重名的 id —— 同一个环境项两个住址，实得 {:?}",
            env.iter().map(|e| e.id).collect::<Vec<_>>()
        );
        assert_eq!(
            env.len(),
            TOOLS.len() + UNMANAGED_ENV.len(),
            "闭集的人数 ≠ 两半之和 —— environment() 漏了一半还是加了第三份"
        );
        for u in UNMANAGED_ENV {
            assert!(
                !TOOLS.iter().any(|t| t.id == u.id),
                "`{}` 同时在 TOOLS 与 UNMANAGED_ENV 里 —— 有 ToolSpec 的不许再手写一条",
                u.id
            );
        }
    }

    /// 🔴 `KR60D1` ②：**每一档都必须有人 —— 一档不许靠「没列出来」表示。**
    ///
    /// 这一条就是 `K-R60` 那件正题的门禁：把手写那一半从 [`UNMANAGED_ENV`]
    /// 里删光（回到那件之前「不写进去就算另一档」的盘面）⇒ 本条红。
    ///
    /// 🔴 〔`K-R65` 09-11〕**本条自己的报错逐字兑现过一次**：`K38` 把 10 项从
    /// 「app 假设它在」搬空之后那一档空了，而报错逐字写着「要么给它一个成员，
    /// **要么把这一档从 EnvTier 里删掉**」⇒ 删掉了那一档，档数从三变四（新增两档）。
    /// 一条判据把自己的两条出路都写出来，走的是哪一条**有记录**，这就是那次记录。
    ///
    /// 分母现算（`EnvTier::ALL`），不写死一个基数〔`13b`：报一个基数也是复述〕。
    #[test]
    fn every_tier_has_members_so_absence_never_encodes_a_judgement() {
        let env = environment();
        for tier in EnvTier::ALL {
            let n = env.iter().filter(|e| e.tier == *tier).count();
            assert!(
                n > 0,
                "「{}」这一档在闭集里一个成员都没有（共 {} 档 · 闭集 {} 项）——\n\
                 空的那一档等于**用缺席表达一个判断**，而读者分不出「没有这种东西」\n\
                 与「有人忘了写」。要么给它一个成员，要么把这一档从 EnvTier 里删掉。",
                tier.label(),
                EnvTier::ALL.len(),
                env.len()
            );
        }
    }

    /// `KR60D1` ③：**「标了档」与「随手填的档」要分得开。**
    ///
    /// 手写那一半的每一条，`why` 里**必须有**一个 `<路径>.rs::<符号>` 形态的代码住址 ——
    /// 「这东西该由谁装」是一个**设计判断**，判断得指得出它长在哪段代码上；
    /// 一句形容词（「常用工具」「一般都有」）过不去这一格。
    ///
    /// **分工写清，别让人以为这一条买到了两件事**：
    /// - 本条只判「**有没有**住址」（缺席这件事只有本条看得见 —— 下面那一条对
    ///   「一个住址都没写」的条目是**静默放过**的）；
    /// - 「那个住址今天**解析不解析得到**」由 `structural_scan` 里那条扫全仓代码住址的
    ///   判据管，它会报「找不到这个符号 / 符号搬家了」。本轮它真的逮到过一条
    ///   （第一版把 `tmux` 那条指到了一个枚举**变体**上）。
    ///
    /// ⚠ **诚实边界**：两条加起来买到的是「这条住址指得到一处真代码」，
    /// **判不了「这处代码真的就是这一项该指的那处」** —— 那要读语义，机器读不了。
    #[test]
    fn every_unmanaged_entry_names_a_code_address() {
        // 反向自检（要件 3）：抽取器不是恒真的
        assert!(
            addresses_in("常用工具，一般机器上都有").is_empty(),
            "抽取器把散文当住址了"
        );
        assert!(
            !addresses_in("launch.rs::TERMINAL_EXITS").is_empty(),
            "抽取器连一个真住址都抠不出来 —— 先查抽取器，别改断言"
        );

        let mut checked = 0usize;
        for u in UNMANAGED_ENV {
            let addrs = addresses_in(u.why);
            assert!(
                !addrs.is_empty(),
                "`{}` 的 why 里没有 `<路径>.rs::<符号>` 形态的住址 —— \
                 「这东西该由谁装」是一个**设计判断**（不是「这台机器上恰好有」这个读数），\
                 判断必须指得出它长在哪段代码上。\n实得：{}",
                u.id,
                u.why
            );
            checked += 1;
        }
        assert!(
            checked >= 5 && checked == UNMANAGED_ENV.len(),
            "计数自检：扫到 {checked} 条，而表里 {} 条",
            UNMANAGED_ENV.len()
        );
    }

    /// ★★ `KR62D1` 的第二条死值验：**`posix-rc-aliases` 升到了第一档，而且是真升。**
    ///
    /// 「档没升 = 活没做完」这句话本身可验 —— 这一条就是它。三格一起断，缺一格都能装样子：
    ///   ① 它**不在** [`UNMANAGED_ENV`] 里了（留在那儿就没有 `ToolSpec`，也就永远升不到第一档）；
    ///   ② 它在 [`TOOLS`] 里且 `installable` / `uninstallable` **都为真**（装得了也卸得掉）；
    ///   ③ [`environment`] 把它算成 [`EnvTier::AppInstalls`]（档是**派生**出来的，不是手填的）。
    ///
    /// **死值验**：把 `installable` 翻回 `false` ⇒ ②③ 双双红；
    /// 把这一条搬回 `UNMANAGED_ENV` ⇒ ① 红。
    #[test]
    fn posix_rc_aliases_sits_in_the_first_tier_now() {
        const ID: &str = "posix-rc-aliases";
        assert!(
            !UNMANAGED_ENV.iter().any(|u| u.id == ID),
            "`{ID}` 还留在 UNMANAGED_ENV 里 —— `K-R62` 之后它有装口也有卸口了，\
             留在手写那一半（没有 ToolSpec ⇒ 永远算作「今天没有装口」）就是盘上写着一句假话"
        );
        let t = TOOLS
            .iter()
            .find(|t| t.id == ID)
            .unwrap_or_else(|| panic!("`{ID}` 不在 TOOLS 里 —— 本机 POSIX 那一格没人申报"));
        assert!(
            t.installable,
            "`{ID}` 申报成装不了 —— 那 `environment()` 会把它算进「app 只查」"
        );
        assert!(t.uninstallable, "`{ID}` 申报成卸不掉 —— 有围栏就必须卸得掉");
        assert!(t.touches().next().is_some(), "`{ID}` 装得了却没申报落点");
        let tier = environment()
            .into_iter()
            .find(|e| e.id == ID)
            .map(|e| e.tier)
            .expect("闭集里找不到它");
        assert_eq!(
            tier,
            EnvTier::AppInstalls,
            "`{ID}` 在闭集里的档不是「{}」—— `K-R62` 那一格没做完",
            EnvTier::AppInstalls.label()
        );
    }

    // 🔴 〔`K-R65` 09-11〕〔散文墓碑〕**`a_hand_written_entry_is_never_app_installs` 删了。**
    //
    // 它逐字断言 `UNMANAGED_ENV` 每一条的 `tier != EnvTier::AppInstalls`，
    // 理由是「装得了就该有一条 `ToolSpec`」。那时 `tier` 是**手填**的 ⇒ 它真有牙。
    //
    // 今天 `tier` 由 [`EnvTier::of`] 派生，而手写那一半的第二格在 [`environment`] 里
    // **恒 `false`** ⇒ `EnvTier::of(_, false)` 一辈子返回不了 `AppInstalls`
    // ⇒ 本条**在算术上不可能红**。留着就是一颗「永远不会红的钉子」——
    // `K-R60` 09-11 刚以同一条理由删过一颗（那条断言「档 == `if installable {…}`」，
    // 是拿实现自己核自己），本仓更早还删过一颗按 `destination` 推 locality 的。
    // ⇒ 同一把尺子，删。
    //
    // ⚠ **它守的那件事没有丢，只是换了守法**：从「断言一个手填格不许是某个值」
    // 变成「那个格子根本不存在」——`UnmanagedEnv` 上今天**没有** `tier` 字段可填。

    /// 🔴 `KR65D2` ①：**「谁该装」与「今天装得了吗」不是同一个字段** —— 两格都有区分力，
    /// 而且**互不函数**（知道一格答不出另一格）。
    ///
    /// 形抄本仓已有的 `config_surface::host_is_not_a_function_of_destination`：
    /// 两个字段合成一个的病，靠「找得到两对反例」证伪。
    ///
    /// **死值验**：把 `who` 改成 `if has_installer { AppShips } else { UserProvides }`
    /// 那种派生（也就是把两格又合回去）⇒ 下面两组反例必有一组空 ⇒ 红。
    #[test]
    fn who_should_install_is_not_a_function_of_whether_we_can_install_today() {
        let env = environment();
        // 「今天有没有装口」这一格在闭集里的读法：只有 `AppInstalls` 那一档是「有」。
        let has_installer = |e: &EnvEntry| e.tier == EnvTier::AppInstalls;

        // ① 同一个 `who`，两种「有没有装口」—— 否则 `who` 就是那一格的同义词。
        let ships: Vec<&EnvEntry> = env
            .iter()
            .filter(|e| e.who == Provisioning::AppShips)
            .collect();
        assert!(
            ships.iter().any(|e| has_installer(e)) && ships.iter().any(|e| !has_installer(e)),
            "「app 自带」这一群里，「今天有装口」与「今天没有装口」**没有同时出现** ——\n\
             那说明这两格今天是同一个字段的两个名字，而 `KR65D2` 的题面正是它们不是。\n\
             实得：{:?}",
            ships
                .iter()
                .map(|e| (e.id, e.tier.label()))
                .collect::<Vec<_>>()
        );

        // ② 同一种「没有装口」，两个不同的 `who` —— 否则「没装口」就唯一决定了「谁该装」，
        //    那正是本件之前的盘面（没装口 ⇒ 只能写成「不该我们装」）。
        let no_installer: BTreeSet<Provisioning> = env
            .iter()
            .filter(|e| !has_installer(e))
            .map(|e| e.who)
            .collect();
        assert!(
            no_installer.len() >= 2,
            "「今天没有装口」的那一群里，「谁该装」只有一个取值（{:?}）——\n\
             那就等于说「没装口」= 「不该我们装」，而 `K38` 裁的恰恰相反：\n\
             `cc-acct-iso-local` **该由 app 装**，只是实现还欠着。",
            no_installer.iter().map(|w| w.label()).collect::<Vec<_>>()
        );
    }

    /// 🔴 `KR65D2` ②：**「app 该自带、而今天还没有装口」那一格数得出来，且不是空的。**
    ///
    /// 死值验的两侧（件计划 `KR65D2` 逐字）：
    ///   · 把 `cc-acct-iso-local` 标成「app 该装」而不给实现 ⇒ **能被数出来**（就是本条）；
    ///   · 把它标成「不该我们装」⇒ **必须红**（那一格由
    ///     `everything_the_charter_named_as_ours_is_in_the_shipped_population` 判）。
    ///
    /// ⚠ 本条**不判「这一格里该有几项」** —— 那要读语义。它判的是这一格**存在、非空、
    /// 且每一项都答得出「欠的是什么」**（`why` 里那个代码住址由另一条判据管）。
    #[test]
    fn the_tier_for_owed_installers_is_countable_and_not_empty() {
        let env = environment();
        let owed: Vec<&EnvEntry> = env
            .iter()
            .filter(|e| e.tier == EnvTier::AppShipsNoInstallerYet)
            .collect();
        assert!(
            !owed.is_empty(),
            "「{}」这一档一个成员都没有 —— 要么本件的活退回去了（`cc-acct-iso-local` \
             又被写成「不该我们装」），要么装口真补上了（那它该升到「{}」，\
             同轮把这一条改成新的下界）",
            EnvTier::AppShipsNoInstallerYet.label(),
            EnvTier::AppInstalls.label()
        );
        for e in &owed {
            assert_eq!(
                e.who,
                Provisioning::AppShips,
                "`{}` 落在「欠装口」那一档，而它的 `who` 不是「{}」—— 派生坏了",
                e.id,
                Provisioning::AppShips.label()
            );
            // 「有名字、看得见」：档名本身必须说清是**欠的实现**，不是「不该我们装」。
            assert!(
                e.tier.label().contains("该自带") && e.tier.label().contains("还没有装口"),
                "这一档的档名读不出「该我们装、而今天还没有装口」两半，实得 {:?} —— \
                 用户会把它读成「不该我们装」",
                e.tier.label()
            );
        }
    }

    /// 🔴 `KR65D3`：**「app 自带的是哪几样」是一个数出来的人群，不是写死的一张单子。**
    ///
    /// # 人群住哪儿
    ///
    /// 唯一住址是 [`Provisioning::AppShips`]，**现算**：`environment()` 里 `who` 是它的那几项。
    /// `TOOLS` 那一半由 `destination` 派生，手写那一半显式声明 —— 两侧都不在这条判据里。
    ///
    /// # 下面这张 `NAMED_AS_OURS` **不是自带清单**，别读错
    ///
    /// 它收的是**定框与用户逐字点过名的那几样**，用途是**下界对拍**：
    /// 点了名的，派生出来的人群里必须有。它不会变成人群的第二个住址 ——
    /// 下面第 ④ 条自检就是钉这件事的：**人群必须严格大于这张表**，
    /// 否则「派生」这句话是假的。
    ///
    /// ⚠ 〔来历，别删 —— 这张表自己就是「点名清单不等于人群」的证据〕
    /// - `pb skill` 曾经在这张表上（`K38` 初版逐字点了它的名）——**用户当拍改口撤掉了**：
    ///   它不自带，它在 `skill_host.rs::SKILLS`（app 的 skill 仓库）那条线上。
    ///   ⇒ 那条线本件不碰，两张表**本来就是不同人群**，不许拿相等去守。
    /// - `code-picture-sidecar` **不在 `K38` 的举例里**，是用户当拍凭记忆追问出来的
    ///   （PM 拟这条 dod 时只数出两样）⇒ **点名清单本身就会漏**，
    ///   这正是「人群必须是数出来的」那句话的来历。
    ///
    /// **死值验**：把 `cc-bus` 从「自带」里摘掉（例如在 [`environment`] 里给它硬写
    /// `Provisioning::NotAnInstall`）⇒ 本条红。
    /// 第二条死值验（`sidecars/` 那一层）住
    /// `the_layer_we_ship_binaries_from_is_pinned_to_the_population`。
    #[test]
    fn everything_the_charter_named_as_ours_is_in_the_shipped_population() {
        /// `(闭集里的 id, 谁在什么时候点的名)`。**只收逐字点过名的**，不收推断出来的。
        const NAMED_AS_OURS: &[(&str, &str)] = &[
            ("cc-bus", "K38 逐字：「cc-bus」"),
            ("cc-acct-iso", "K38 逐字：「account」—— 远端那半"),
            (
                "cc-acct-iso-local",
                "K38 逐字：「account」—— 本机那半（件计划 §0c 明裁它是 app 独有的）",
            ),
            (
                "code-picture-sidecar",
                "用户 09-11 当拍追问：「不是还有 code picture 吗」—— \
                 现打落在 sidecars/ 那一层（「我们自己出、我们自己装、我们自己调」）",
            ),
        ];
        let env = environment();
        // ① 人群**现算**，不抄名单。
        let shipped: BTreeSet<&str> = env
            .iter()
            .filter(|e| e.who == Provisioning::AppShips)
            .map(|e| e.id)
            .collect();
        assert!(
            !shipped.is_empty(),
            "「app 自带」这个人群是空的 —— 先查 `Provisioning::of_tool` 与 `environment`，别改断言"
        );

        // ② 保鲜自检：定框点过名的 id 必须还在闭集里（改了名 / 删了 ⇒ 本表当场腐）。
        let all: BTreeSet<&str> = env.iter().map(|e| e.id).collect();
        for (id, src) in NAMED_AS_OURS {
            assert!(
                all.contains(id),
                "`{id}` 在闭集里已经找不到了（它当初的来历：{src}）—— \
                 要么它改名了（这张表跟着改），要么它真没了（那 `K38` 那一格要重裁）"
            );
        }

        // ③ 下界对拍：定框点了名的，必须在派生出来的人群里。
        let missing: Vec<&str> = NAMED_AS_OURS
            .iter()
            .filter(|(id, _)| !shipped.contains(id))
            .map(|(id, _)| *id)
            .collect();
        assert!(
            missing.is_empty(),
            "`K38` 逐字点名要自带的东西，在「app 自带」这个人群里**找不到**：{missing:?}\n\
             人群现算于 `environment()` 的 `who == AppShips`，今天是 {shipped:?}。\n\
             ⇒ 要么那几项的申报错了（改申报），要么 `K38` 那一格要重裁（改定框，不是改这里）。"
        );

        // ④ **人群不许退化成这张表** —— 严格大于，才说明它是派生出来的。
        assert!(
            shipped.len() > NAMED_AS_OURS.len(),
            "「app 自带」这个人群（{} 项）没有比定框举的例子（{} 项）更大 —— \
             那说明它其实是照这张表抄的，而不是数出来的。人群实得 {shipped:?}",
            shipped.len(),
            NAMED_AS_OURS.len()
        );
    }

    /// 🔴 `KR65D3` 的**第二条死值验**：**我们随产品分发二进制的那一层，在人群里数得到。**
    ///
    /// # 为什么非得钉盘上那一层，光在闭集里加一行不够
    ///
    /// 闭集里那一行是**申报**。申报可以在那一层被掏空之后照样绿着 ——
    /// 那正是本仓治过的「声明缺口」那一族（`remote-daemon` 的 `uninstallable: false`
    /// 假申报活了一个月）。⇒ 右边去钉**盘上那一层真有那一跳**，两边一起断。
    ///
    /// **死值验**：把 `sidecars/codepicture/acquire.rs` 那一层掏空（`obtain` 那一跳删掉
    /// 或改签名）⇒ 本条红。
    /// ⚠ **如实写明它的边界**：把整个文件**删掉**是 `include_str!` 编译失败，
    /// 那是 **CRASH 不是红** —— 两件事别混着报。
    ///
    /// ⚠ 它**判不了**「这一层今天接没接线」（那一格由 daemon 那侧的
    /// `sidecar_fetch_guard` 管，红的那一刻就是接线那一刻）。本条只判
    /// 「这一层还在盘上，而闭集里申报了它」。
    #[test]
    fn the_layer_we_ship_binaries_from_is_pinned_to_the_population() {
        use crate::structural_scan::pin_definition;

        /// 那一层的取件实现。**跨 crate 读源码在本仓有先例**
        /// （`usage.rs` / `polling_registry.rs` 都这么钉 `src/backend` 那侧）。
        const SIDECAR_ACQUIRE: &str = include_str!("../../backend/sidecars/codepicture/acquire.rs");

        // 反向自检：`pin_definition` 真的会说「不在」（否则下面是空真）。
        assert!(pin_definition("fn a() {}\n", "fn b() {}", "fn b", "自检").is_err());

        // 右边：盘上那一层真有「把它拿到这台机器上来」的那一跳，且只有一处。
        pin_definition(
            SIDECAR_ACQUIRE,
            "pub fn obtain<O: Origin>(",
            "pub fn obtain",
            "代码全景 sidecar 的取件实现",
        )
        .expect("sidecars/ 那一层被掏空了 —— 而闭集里仍申报着它是「app 自带」的一员");

        // 左边：闭集里那一项，而且它算在「app 自带」那个人群里。
        const ID: &str = "code-picture-sidecar";
        let e = environment()
            .into_iter()
            .find(|e| e.id == ID)
            .unwrap_or_else(|| {
                panic!(
                    "`{ID}` 不在环境闭集里 —— 盘上有一层专门用来「随产品分发二进制」，\
                     而「app 自带的是哪几样」这个人群里数不到它"
                )
            });
        assert_eq!(
            e.who,
            Provisioning::AppShips,
            "`{ID}` 没算在「{}」那一群里 —— 那一层的头注逐字写着「我们自己出、\
             我们自己装、我们自己调……随产品分发」",
            Provisioning::AppShips.label()
        );
    }

    /// 「谁该装」三值**都真有人用** —— 一个只有一个取值的字段没有区分力。
    /// 形抄 `config_surface::all_host_scopes_are_really_used`。
    #[test]
    fn every_provisioning_value_is_really_used() {
        let env = environment();
        for w in Provisioning::ALL {
            assert!(
                env.iter().any(|e| e.who == *w),
                "「{}」这个取值在闭集里一项都没有（共 {} 值 · 闭集 {} 项）—— \
                 没人用的取值要么删掉，要么它就是漏了",
                w.label(),
                Provisioning::ALL.len(),
                env.len()
            );
        }
    }

    /// **「不该我们装」的东西不许有装口** —— [`EnvTier::of`] 里那两支 `_` 会把这种
    /// 自相矛盾**吸收成一个正常档**，所以矛盾本身要在这里单独判红，不能靠那个 `match`。
    #[test]
    fn nobody_declares_an_installer_for_something_we_should_not_install() {
        for t in TOOLS {
            let who = Provisioning::of_tool(t);
            if who != Provisioning::AppShips {
                assert!(
                    !t.installable,
                    "`{}` 的落点说它「{}」，而 `installable: true` 说我们装得了 —— \
                     两句话有一句是假的",
                    t.id,
                    who.label()
                );
            }
        }
    }

    /// `KR65D1` 的申报侧：**「你自己装」那一档的每一项都要说得出「怎么查」** ——
    /// 而且**不许整档都是「查不动」**（那就等于把「不查」换了个名字，正是失效方向）。
    ///
    /// ⚠ 行为那一半（真去查、缺了显示成 `Absent` 而不是 `Undetermined`）**不在这里** ——
    /// 在 `config_surface::the_prompt_tier_really_looks_before_it_speaks`。
    /// 本条只判申报，别把两条读成一条。
    #[test]
    fn the_prompt_tier_declares_how_it_will_look() {
        let env = environment();
        let mut probeable = 0usize;
        let mut blind = 0usize;
        for e in env
            .iter()
            .filter(|e| e.tier == EnvTier::UserInstallsWePrompt)
        {
            let EnvBacking::Named { probe, .. } = e.backing else {
                panic!(
                    "`{}` 在「{}」这一档，却有 ToolSpec —— 那一档今天只该住手写那一半",
                    e.id,
                    EnvTier::UserInstallsWePrompt.label()
                )
            };
            match probe {
                EnvProbe::OnPath | EnvProbe::HomePath => probeable += 1,
                EnvProbe::CannotProbe { why } => {
                    blind += 1;
                    assert!(
                        why.len() > 30,
                        "`{}` 申报「查不动」而理由只有 {} 字节 —— \
                         「查不动」与「懒得查」在表上长得一模一样，理由是唯一分得开的东西",
                        e.id,
                        why.len()
                    );
                }
            }
        }
        assert!(
            probeable + blind > 0,
            "「{}」这一档一个成员都没有 —— 那 9 项没搬过来",
            EnvTier::UserInstallsWePrompt.label()
        );
        assert!(
            probeable > blind,
            "「{}」这一档里查得动的 {probeable} 项、查不动的 {blind} 项 —— \
             查不动的过半就等于这一档只是「app 假设它在」换了个名字，\
             而那正是 `KR65D1` 写死的失效方向",
            EnvTier::UserInstallsWePrompt.label()
        );
    }

    /// 派生那一半：**`TOOLS` 的每一条都必须进闭集**，一条都不许漏。
    ///
    /// 🔴 **本条上一版还断言了「档 == `if installable {…} else {…}`」，那是同义反复，已删。**
    /// 〔`K-R60` 09-11 自抓，是本轮 `7u` 那一刀逼出来的：把实现整个退掉之后它**仍然绿**——
    /// 因为 `environment()` 的档就是那个表达式算的，判据再算一遍等于拿它自己核它自己。〕
    /// 本仓删过一颗同形的钉子（`config_surface` 那条按 `destination` 推 locality 的），
    /// 理由逐字：「不留永远不会红的钉子」。
    /// ⇒ 留下的是**够得着的那一半**：覆盖。漏掉一条 `TOOLS` ⇒ 本条红。
    /// 而「字段填错了」那一格**不由本条守**，由读字段的那条（它右边钉的是实现，不是同一个表达式）。
    #[test]
    fn every_managed_tool_reaches_the_closed_set() {
        let env = environment();
        for t in TOOLS {
            let n = env.iter().filter(|e| e.id == t.id).count();
            assert_eq!(
                n, 1,
                "`{}` 在 TOOLS 里，而闭集里出现 {n} 次（应为 1）——\
                 闭集漏了它，这一页上就看不见它",
                t.id
            );
        }
    }

    // 🔴 〔`K-R63` 09-11〕**`KR60D3` 那条判据从这里搬走了，而不是删掉。**
    //
    // 它做的事（左边读 `cc-bus` 的 `installable` 字段、右边用 `pin_definition` 钉
    // `cc_bus_deploy.rs` 里那个部署函数的签名、两边 `assert_eq!`）今天由
    // `every_tool_declares_install_and_uninstall_as_the_implementations_really_are`
    // 做，而那一条**对全表每一条的两格都做**。
    //
    // 为什么非搬不可（`KR63D2` 的正题）：那一条的**名字里带工具名** ——
    // 读的人会以为「申报与实现对不对得上」这一格有人守，而它只守 `cc-bus` 一个工具。
    // `K-R60` 收窗口时 PM 的刀 γ 就现打过同一件事的另一半；`K-R63` 的刀 C 更直接：
    // 〔PM 09-11 现打，量于 `cd26954`〕翻 `cc-acct-iso` 的 `uninstallable`
    // ⇒ 全表 1379 条一条没红。
    // ⇒ **别再在这里加第二颗专名钉子**；要加就加进那张对拍表。
}
