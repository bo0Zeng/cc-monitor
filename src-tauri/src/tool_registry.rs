//! T01 第 5 步：**受管工具的声明**（`ToolSpec`）。
//!
//! # ⚠ 它守什么、**不守什么**〔audit-0805 08-06 抽样补记〕
//!
//! 本模块 15 条判据**全部是声明表内部的自洽检查**：字段有没有区分力 · 落点在不在
//! `touches` 里 · 拥有就必须装得了 · 有围栏就必须卸得掉 · 解析器有没有真看见源码。
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
    /// 能不能装/升。
    ///
    /// 🔴 **这一格是「app 装的」与「app 只查」两档的分界线**（`K-R60`）：
    /// [`environment`] 就是读它算出每条 `ToolSpec` 属于哪一档的。
    /// ⇒ 它填错了，用户在配置面上读到的「能否装/撤」与清单上的档**同时**是假的。
    /// 守它的是 `cc_bus_installable_matches_whether_the_deploy_really_exists`（读**字段值**，
    /// 不是注释里的词频 —— 这一格上一次就是被词频守卫放过去的）。
    pub installable: bool,
    /// 能不能卸。
    pub uninstallable: bool,
    pub touches: &'static [TouchedFile],
}

/// 五套既有机制 + cc-bus 的声明。**本轮只声明，不改它们任何行为**
/// （MASTERPLAN §4 第 3 点：先用已知行为的工具验证抽象，再拿它吃新工具）。
pub const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        id: "ccm",
        display_name: "ccm 统一启动器（后端本体的一次性模式）",
        // 🔴 〔`K-R48` 第二拍 09-11〕`repo_path` 原来指 `shared/ccm`（那份 1592 行 bash）。
        //    〔用@09-11 `K33`〕「不要有什么 bash 脚本，不要有什么单独的 ccm」⇒ 那个文件删了。
        //    ⇒ 指到远端那个**入口**的来源：`sftp::ccm_entry_shim` 现造的三行 `exec` 串
        //    （零实现，把 argv 转给已经部署好的后端）。
        //    ⚠ **它今天不是一份「仓里的文件」** —— `EmbeddedText { repo_path }` 这个形状
        //    在这一条上已经不合身了（值是**算出来的**，路径取自用户填的 `daemon_path`）。
        //    本模块头注自己写着「零生产消费者、T02 接不上就该删掉本模块」⇒ **不为它改类型**，
        //    如实指到那个函数的住址，并把这一格的形状问题登记在这里。
        source: ToolSource::EmbeddedText {
            repo_path: "src-tauri/src/sftp.rs::ccm_entry_shim",
        },
        destination: ToolDestination::RemoteHomeRelative(".local/bin/ccm"),
        installable: true,
        uninstallable: true,
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
    ToolSpec {
        id: "cc-bus",
        display_name: "cc-bus 多实例消息总线",
        source: ToolSource::RepoDir {
            repo_path: "shared/cc-bus",
        },
        destination: ToolDestination::LocalHomeRelative(".claude/skills/cc-bus"),
        // ★★ **不是「实现一下就能翻 true」——落点被只读铁律排除**〔PS1 重摸底 08-12〕。
        //
        // 原注释只写「部署尚未实现（B01 只做了"搬进仓固化为基线"）」，那是**浅一层**的理由，
        // 会让下一个人以为补个递归拷贝就行。真实的墙在 `doc/INVARIANTS.md` 开头：
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
        // 例外本身写成 `doc/INVARIANTS.md` 的第 7 条。
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
        // ⇒ 本轮补了 `cc_bus_installable_matches_whether_the_deploy_really_exists`：
        // 它左边读这个字段、右边钉 `cc_bus_deploy.rs` 的函数签名，两边必须相等。
        installable: true,
        uninstallable: false,
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
    },
    ToolSpec {
        id: "cc-acct-iso",
        display_name: "cc-acct-iso 多账号隔离",
        source: ToolSource::Vendored {
            repo_path: "src-tauri/vendor/cc-acct-iso",
            fingerprint_file: ".vendor_id",
        },
        destination: ToolDestination::UserConfiguredPath {
            token: "$ACCT_ISO_DEST",
            what: "部署时在账号页填的「部署目录」",
        },
        installable: true,
        uninstallable: false,
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
    },
    ToolSpec {
        id: "remote-daemon",
        display_name: "远端 daemon",
        source: ToolSource::EmbeddedBinary {
            repo_path: "embedded-daemons",
        },
        destination: ToolDestination::UserConfiguredPath {
            token: "$DAEMON_PATH",
            what: "每个远端连接的「daemon 路径」配置项",
        },
        installable: true,
        uninstallable: false,
        touches: &[TouchedFile {
            path: "$DAEMON_PATH",
            host: HostScope::Remote,
            note: Some(
                "远端，路径由该连接的「daemon 路径」配置项决定——**不是**固定的 ~/.local/bin/ccm-daemon",
            ),
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
            path: ".mcp.json",
            host: HostScope::ProjectDir,
            note: Some("相对你选定的项目目录"),
            effect: TouchEffect::OwnedFile,
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
        // 装进去的内容**就是仓里那份文件**（`sftp::CCM_WRAPPER_SNIPPET` 是它的
        // `include_str!`）。远端那条 `ccm` 用的是同一份 —— 那正是本件不许出现第二份的东西。
        source: ToolSource::EmbeddedText {
            repo_path: "shared/ccm-aliases.sh",
        },
        // 🔴 **路径由人选，产品不猜** —— 这一格用占位符而不是 `~/.bashrc`，
        // 理由与 `remote-daemon` 的 `$DAEMON_PATH` 逐字同源：申报一个我们其实没在用的常量，
        // 审计页会拿它去查一个没人写的路径然后言之凿凿地报「缺失」。
        // `.bashrc` / `.zshrc` / `config.fish` 写法不同，替人选一份是最坏的那条路
        // （`account_aliases` 的 `§0e`）。
        destination: ToolDestination::UserConfiguredPath {
            token: "$POSIX_RC",
            what: "「按账号生成命令」那一块里那个 rc 下拉 —— 从盘上真实存在的 .bashrc / .zshrc / .bash_profile / .profile 里由你自己选",
        },
        installable: true,
        uninstallable: true,
        touches: &[TouchedFile {
            path: "$POSIX_RC",
            host: HostScope::Client,
            note: Some(
                "本机（cc-monitor 跑着的这台）的那份 shell rc，具体哪一份由界面上的人选。\
                 围栏与内容都与远端那个口共用一份 —— 本机与远端装进 rc 的是同一个东西（K15 / K36）",
            ),
            effect: TouchEffect::FencedBlock,
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
            host: HostScope::Client,
            note: Some("Windows 客户端侧，具体路径由 PowerShell 决定"),
            effect: TouchEffect::FencedBlock,
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
        source: ToolSource::NotOurs {
            who: "Claude Code 自己建、自己写",
        },
        destination: ToolDestination::NotInstalledByUs {
            whose: "Claude Code 的数据根（`adapter/claude_code.rs` 的 CLAUDE_LAYOUT.sessions_subdir）",
        },
        installable: false,
        uninstallable: false,
        touches: &[TouchedFile {
            path: "~/.claude/projects/",
            host: HostScope::Either,
            note: Some(
                "历史面与搜索索引的全部输入（每个项目一个目录、每个会话一份 jsonl）——\
                 我们只读；装不了，也不该我们装",
            ),
            effect: TouchEffect::ReadOnly,
        }],
    },
];

// ═══════════════════════════════════════════════════════════════════════════
// `K-R60`：**环境清单的闭集** —— 「app 要的东西齐了没有」这个问题的人群
// ═══════════════════════════════════════════════════════════════════════════

/// app 与一个环境项的关系。**三档穷举。**
///
/// 🔴 **第三档必须在清单里有一格，不许靠「没列出来」表示。**
/// 〔`K-R57` 摸底现打：`TOOLS` 只有 6 条，而 app 用的时候直接假设在的至少 9 项
/// （`claude` · `tmux` · 终端出口 · `git` · `ssh` · `pgrep` · `xdg-open` · `bash` ·
/// MCP server 本体）—— 它们一条都没进任何一张表。于是「app 依赖本机环境」这个判断
/// **在代码里没有住址**，只能从「表里没有」倒推 —— 用缺席表达一个判断，
/// 正是本工作区一整天在治的那族病。〕
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum EnvTier {
    /// **app 装的** —— 产品自己有安装动作。
    AppInstalls,
    /// **app 只查** —— 查得到它在不在，装不了。
    AppOnlyChecks,
    /// **app 假设它在** —— 既不装也不查，用的时候直接假设它已经在。
    AppAssumesPresent,
}

impl EnvTier {
    /// 三档的**闭集**本身。现算用（`len()` 就是「几档」，不许在别处写死一个基数）。
    pub const ALL: &'static [EnvTier] = &[
        EnvTier::AppInstalls,
        EnvTier::AppOnlyChecks,
        EnvTier::AppAssumesPresent,
    ];

    /// 给人看的档名。措辞定在这里，UI 与诊断文本不再各写一遍。
    pub fn label(self) -> &'static str {
        match self {
            EnvTier::AppInstalls => "app 装的",
            EnvTier::AppOnlyChecks => "app 只查",
            EnvTier::AppAssumesPresent => "app 假设它在",
        }
    }
}

/// 闭集里**派生不出来**的那一半：`TOOLS` 里没有对应条目的环境项。
///
/// # 为什么只有这一半是手写的
///
/// 有 [`ToolSpec`] 的那一半**能派生**：它的 `touches` 一定会被 `config_surface` 逐条解析并观测
/// 一次（⇒ 至少是「只查」），再读它的 `installable` 就分得出「装」还是「只查」。
/// 而这一半派生不出来 —— 盘上**没有任何字段**能把「只查」与「假设它在」分开，
/// 那是一个**设计判断**，不是读数。⇒ 判断必须有住址，这张表就是它的住址。
///
/// ⚠ **别把「今天这台机器上恰好有」写成「app 假设它在」** —— 前者是读数（`K-R57` 量具 A 量的那种），
/// 后者是设计判断。本表只收后者，所以每一条的 `why` 要给**代码里的住址**，不是一句形容。
pub struct UnmanagedEnv {
    pub id: &'static str,
    pub display_name: &'static str,
    /// 这一项属于哪一档。**必须写出来** —— 第三档就是靠这一格存在的。
    pub tier: EnvTier,
    /// 用什么名字指认它：PATH 上的命令名、一条 `~/` 路径、或一个 `$占位符`。
    pub named: &'static str,
    /// 它在哪台机器上（与 [`TouchedFile::host`] 同一套语义）。
    pub host: HostScope,
    /// **app 在哪儿用到它** —— 结尾必须是一个 `<相对 src 的路径>.rs::<符号>` 形态的住址。
    /// 判据只判「**有没有**住址」；那个住址今天解析不解析得到，由 `structural_scan` 里
    /// 那条扫全仓代码住址的判据管（它会报「找不到这个符号 / 符号搬家了」）。
    pub why: &'static str,
}

/// 闭集里手写的那一半。**今天全是第三档** —— 这不是巧合：
/// 「只查」那一档今天唯一的成员是 `TOOLS` 里 `installable: false` 的 `claude-code`，
/// 由 [`environment`] 从字段派生出来，不在这张表里。
pub const UNMANAGED_ENV: &[UnmanagedEnv] = &[
    UnmanagedEnv {
        id: "claude-cli",
        display_name: "Claude Code 的可执行文件",
        tier: EnvTier::AppAssumesPresent,
        named: "claude",
        host: HostScope::Either,
        why: "起会话时拿它当启动器直接用；装不装、在哪个版本，app 一概不问 —— \
              adapter/claude_code.rs::default_launcher",
    },
    UnmanagedEnv {
        id: "tmux",
        display_name: "tmux（会话容器）",
        tier: EnvTier::AppAssumesPresent,
        named: "tmux",
        host: HostScope::Either,
        why: "tmux 容器那条起法要它；缺了只在回绝里报一句能力名 —— \
              backend/control/ccm_invocation.rs::Refusal",
    },
    UnmanagedEnv {
        id: "terminal-exit",
        display_name: "POSIX 终端出口",
        tier: EnvTier::AppAssumesPresent,
        named: "xdg-terminal-exec",
        host: HostScope::Client,
        why: "POSIX 上开一个会话窗口只认这一个规范化出口，表里今天就它一项 —— \
              launch.rs::TERMINAL_EXITS",
    },
    UnmanagedEnv {
        id: "git",
        display_name: "git",
        tier: EnvTier::AppAssumesPresent,
        named: "git",
        host: HostScope::Client,
        why: "认 skill 所在的工作树与主检出要跑它 —— skill_host.rs::git_common_dir",
    },
    UnmanagedEnv {
        id: "ssh",
        display_name: "ssh 客户端（含密钥与主机配置）",
        tier: EnvTier::AppAssumesPresent,
        named: "ssh",
        host: HostScope::Client,
        why: "远端一整侧都经它；app 只探它在不在 PATH 上，装不了 —— \
              launch.rs::ssh_client_available",
    },
    UnmanagedEnv {
        id: "pgrep",
        display_name: "pgrep",
        tier: EnvTier::AppAssumesPresent,
        named: "pgrep",
        host: HostScope::Either,
        why: "数 cc-bus 的 agent 在不在用它 —— cc_bus.rs::count_now",
    },
    UnmanagedEnv {
        id: "xdg-open",
        display_name: "xdg-open",
        tier: EnvTier::AppAssumesPresent,
        named: "xdg-open",
        host: HostScope::Client,
        why: "开链接 / 开日志目录走它 —— lib.rs::open_with_os",
    },
    UnmanagedEnv {
        id: "login-shell",
        display_name: "bash 登录 shell",
        tier: EnvTier::AppAssumesPresent,
        named: "bash",
        host: HostScope::Either,
        why: "探 ccm 能力时要一个登录 shell 把用户的 rc 读进来 —— ccm_probe.rs::probe_with",
    },
    UnmanagedEnv {
        id: "mcp-server",
        display_name: "MCP server 本体（`.mcp.json` 里那个 command）",
        tier: EnvTier::AppAssumesPresent,
        named: "$MCP_COMMAND",
        host: HostScope::ProjectDir,
        why: "我们写得了那份配置，**被它指到的可执行本体不装也不查** —— \
              mcp.rs::write_project_mcp_server",
    },
    UnmanagedEnv {
        id: "cc-acct-iso-local",
        display_name: "cc-acct-iso 本机那份",
        tier: EnvTier::AppAssumesPresent,
        named: "~/.local/bin/cc-acct-iso",
        host: HostScope::Client,
        why: "本机侧零装口、零查口；有口的只有远端那半 —— \
              acct_iso_deploy.rs::check_remote_acct_iso",
    },
    // 🔴 〔`K-R62` 09-11〕**`posix-rc-aliases` 从这里搬走了 —— 这是它的墓碑。**
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
    /// 没有 —— 只有一个名字和它在哪台机器上。
    Named {
        named: &'static str,
        host: HostScope,
    },
}

/// 闭集里的一项。
pub struct EnvEntry {
    pub id: &'static str,
    pub display_name: &'static str,
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
/// - `TOOLS` 里每一条自动进来一项，**档读它的 `installable` 字段算出来**：
///   `true ⇒ AppInstalls`；`false ⇒ AppOnlyChecks`（有 `ToolSpec` ⇒ 它的每条 `touches`
///   都会被 `config_surface` 解析并观测一次 ⇒ 至少查得到）。
/// - `TOOLS` 里没有的那一半住 [`UNMANAGED_ENV`]，档只能显式声明 —— 盘上没有任何字段
///   能把「只查」与「假设它在」分开，那是设计判断不是读数。
///
/// ⚠ **这个派生的已知上限，写在这里别被读大一格**：`installable: false ⇒ 只查` 买到的是
/// 「本页会去解析并观测它申报的每条路径」。远端那几条观测出来是 `Undetermined`
/// （本页不连 SSH）—— 那仍是「查了、只是查不动」，不是「没查」，但它**不等于**
/// 「app 有一个真能回答它在不在的口」。要那一格得另立判据。
pub fn environment() -> Vec<EnvEntry> {
    let mut out: Vec<EnvEntry> = TOOLS
        .iter()
        .map(|t| EnvEntry {
            id: t.id,
            display_name: t.display_name,
            tier: if t.installable {
                EnvTier::AppInstalls
            } else {
                EnvTier::AppOnlyChecks
            },
            why: "有 ToolSpec ⇒ 档由它的 installable 字段派生 —— tool_registry.rs::environment",
            backing: EnvBacking::Managed(t),
        })
        .collect();
    out.extend(UNMANAGED_ENV.iter().map(|u| EnvEntry {
        id: u.id,
        display_name: u.display_name,
        tier: u.tier,
        why: u.why,
        backing: EnvBacking::Named {
            named: u.named,
            host: u.host,
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
         二进制**的 crate（`src-tauri/vendor/code-picture-core`），`panorama.rs` 直接 \
         `use Engine` 调它画图。没有 `destination`、没有安装动作、卸载它等于重新编译 monitor。\n\
         ⚠ 它另有一个身份是 **MCP server（code-picture 的 Agent head，给 Claude 用）**，\
         那一个确实「装到别处」—— 但**装它走已有的 `project-mcp` 机制**（往 `.mcp.json` 加一个 \
         server 条目），是**用法**不是新工具。仓里今天对那个 MCP head 零实现（`mcp.rs` / \
         `config_surface.rs` 里 `code-picture` 零命中）。\n\
         ⇒ 真要做「一键装 code-picture 的 MCP」属 **issue #51 第 1 部分**，\
         用户 08-10 明说「cc-bus 和 code-picture 后面再增强，现在先不做」。",
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
                "    pub touches: &'static [TouchedFile],",
                "    pub touches: &'static [TouchedFile],\n    pub needs_elevation: bool,",
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
        assert!(
            ccbus.installable,
            "cc-bus 的部署 08-13 就实现了（`cc_bus_deploy.rs`），声明位必须跟上"
        );
        assert!(!ccbus.uninstallable, "卸载没做，不得声明可卸");
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
            if t.touches.iter().any(|f| f.effect == TouchEffect::OwnedFile) {
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
    /// 而真实的墙是 `doc/INVARIANTS.md` 那条只读铁律 —— 落点 `~/.claude/skills/` 不在
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

    #[test]
    fn installable_tools_declare_where_they_land() {
        for t in TOOLS {
            if !t.installable {
                continue;
            }
            let want: String = match &t.destination {
                ToolDestination::RemoteHomeRelative(p) | ToolDestination::LocalHomeRelative(p) => {
                    format!("~/{p}")
                }
                ToolDestination::ProjectRelative(p) => (*p).to_string(),
                ToolDestination::UserShellProfile => "$PROFILE".to_string(),
                ToolDestination::UserConfiguredPath { token, .. } => (*token).to_string(),
                // 〔`K-R60`〕跨字段：「这不是我们的落点」与「装得了」不许同时成立。
                ToolDestination::NotInstalledByUs { whose } => panic!(
                    "{} 声明 installable: true，落点却写着「不是我们装的」（{whose}）——\
                     两句话有一句是假的",
                    t.id
                ),
            };
            assert!(
                t.touches.iter().any(|f| f.path == want),
                "{} 可安装，但 touches 里没有它的落点 {want:?}（实得 {:?}）",
                t.id,
                t.touches.iter().map(|f| f.path).collect::<Vec<_>>()
            );
        }
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

    /// 🔴 `KR60D1` ②：**三档都必须有人 —— 第三档不许靠「没列出来」表示。**
    ///
    /// 这一条就是本件的正题的门禁：把 `AppAssumesPresent` 那几项从 [`UNMANAGED_ENV`]
    /// 里删光（回到本件之前那个「不写进去就算第三档」的盘面）⇒ 本条红。
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
    /// 「app 假设它在」是一个**设计判断**，判断得指得出它长在哪段代码上；
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
                 「app 假设它在」是一个**设计判断**（不是「这台机器上恰好有」这个读数），\
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
    ///   ① 它**不在** [`UNMANAGED_ENV`] 里了（留在那儿就还是「app 假设它在」）；
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
             留在「app 假设它在」那一档就是盘上写着一句假话"
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
        assert!(!t.touches.is_empty(), "`{ID}` 装得了却没申报落点");
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

    /// 手写那一半**不许声明「app 装的」** —— 装得了就该有一条 [`ToolSpec`]
    /// 说清源 / 落点 / 碰哪些文件，不能只留一个名字。
    #[test]
    fn a_hand_written_entry_is_never_app_installs() {
        for u in UNMANAGED_ENV {
            assert_ne!(
                u.tier,
                EnvTier::AppInstalls,
                "`{}` 声明「app 装的」却没有 ToolSpec —— 装得了就得申报装到哪、碰哪些文件",
                u.id
            );
        }
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

    /// 🔴 `KR60D3`：**`cc-bus` 的 `installable` 与实现一致，而本条真的读那个字段。**
    ///
    /// 〔`K-R57` 摸底逮到的假申报：字段现打 `false`，而**同一注释块**逐字写着
    /// 「08-13 用户裁：开 ⇒ `installable` 从 `false` 翻成 `true`，实现在 `cc_bus_deploy.rs`」，
    /// 而那个实现**真的在**。一条用户裁定落在注释里、没落到字段上。〕
    ///
    /// 🔴 **为什么补这一条**：守它的老判据 `cc_bus_says_why_it_is_not_installable_at_the_real_depth`
    /// 是**必需词守卫** —— 它数的是注释里两个词的出现次数，**看不见字段的值**。
    /// 字段翻成任何值它都绿，于是字段与注释各说各话了一个月，一格都没红。
    ///
    /// 本条两边都是**现读**的，不抄一份被测逻辑：
    /// - 左边读 `TOOLS` 里那条的 `installable` 字段；
    /// - 右边用 `pin_definition` 去 `cc_bus_deploy.rs` 里钉那个部署函数的签名
    ///   （它同时守住「只被定义一次」，改成别的名字或删掉都会让右边变 `false`）。
    ///
    /// ⇒ 两边**必须相等**。把字段翻回 `false` ⇒ 本条红；把实现删掉而不改字段 ⇒ 也红。
    #[test]
    fn cc_bus_installable_matches_whether_the_deploy_really_exists() {
        let deploy_impl_exists = crate::structural_scan::pin_definition(
            include_str!("cc_bus_deploy.rs"),
            "pub async fn deploy_local_cc_bus() -> Result<CcBusDeployReport, String> {",
            "pub async fn deploy_local_cc_bus",
            "cc-bus 本机部署的实现",
        )
        .is_ok();
        let ccbus = TOOLS
            .iter()
            .find(|t| t.id == "cc-bus")
            .expect("TOOLS 里应有 cc-bus");
        assert_eq!(
            ccbus.installable,
            deploy_impl_exists,
            "cc-bus 的 `installable` 申报为 {}，而 `cc_bus_deploy.rs` 里那个部署实现{}。\n\
             这两句话必须一致 —— 那张表的「能否装/撤」列直接印到用户眼前，\n\
             申报错了用户看到的就是一句假话。\n\
             ⚠ 只改注释没有用：本条读的是**字段值**，不是注释里的词频。",
            ccbus.installable,
            if deploy_impl_exists { "在" } else { "不在" }
        );
    }
}
