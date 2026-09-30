//! T01 第 5 步：**受管工具的声明**（`ToolSpec`）。
//!
//! # ⚠ 它守什么、**不守什么**〔audit-0805 08-06 抽样补记〕
//!
//! 本模块的判据**多数是声明表内部的自洽检查**：字段有没有区分力 · 落点在不在
//! `touches` 里 · 拥有就必须装得了 · 有围栏就必须卸得掉 · 解析器有没有真看见源码。
//! 〔`K-R63` 09-11 订正两处：① 原文写死了一个基数（「15 条」），而判据条数只有一份
//!  住址 —— 本文件判据档里的测试函数，要数就现数〔`13b`〕；② 「**全部**是表内部自洽」
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
//! **先更正本文件原先写错的一处事实**（T01 审计 Q3）。原文说当时那个账号工具的探测是
//! 「比对内容指纹」——不对。它实际跑的是一条命令再解析 stdout，
//! 与 `ccm_probe.rs` **属于同一族**（跑一条命令、解析 stdout）。指纹比对
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
//! ## 原先这里有个 `ProbeStatus`，本轮**删掉了**
//!
//! 它是 `{ installed: bool, version: String, capabilities: Vec<String> }`，
//! 与既有的 `ccm_probe::CcmProbeResult` **同形**、零适配、零生产消费者，
//! 而且 `version: String` 比对方的 `Option<String>` 还丢了「取不到」这一档。
//! 我原先把它当作「机制留各家、结果统一」的落点——**发明第二个同形结构不是统一，
//! 是重复**。按我自己这一轮的尺子（`build_online_cmd`〔散文墓碑〕零调用点被我判为**阻塞**、
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

use copy_core::copy_text;

/// 〔CP2b · 4C〕表里给人看的那一句 —— **取文函数**，不是字面量。
///
/// 常量表里调不了函数 ⇒ 放一个不捕获的闭包（`Text(|| copy_text("key", &[]))`），用到时才取文；
/// 话本身住 `src/shared/copy/table.json`。包一层而不是裸放 `Text`，是为了让
/// 比较 / 打印 / 序列化都按「取出来的那句话」走：fn 指针的相等比的是地址（编译器不保证唯一），
/// 也序列化不了。
#[derive(Clone, Copy)]
pub struct Text(pub fn() -> String);

impl Text {
    /// 取出那句话。
    pub fn get(&self) -> String {
        (self.0)()
    }
}

impl std::fmt::Display for Text {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.get())
    }
}

impl std::fmt::Debug for Text {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.get(), f)
    }
}

impl PartialEq for Text {
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
    }
}

impl Eq for Text {}

impl std::hash::Hash for Text {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.get().hash(state);
    }
}

impl serde::Serialize for Text {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.get())
    }
}

/// 内容的来源。**每个变体的使用者数现算**（`TOOLS` 是唯一一份），不写死在这里〔`13b`〕。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum ToolSource {
    /// 仓内文件，编译期 `include_str!` 进二进制（`ccm`）。
    EmbeddedText { repo_path: &'static str },
    /// 仓内目录，运行期读（`cc-bus` 的 `src/shared/cc-bus/`）。
    RepoDir { repo_path: &'static str },
    /// 交叉编译后内嵌的二进制（remote backend）。
    EmbeddedBinary { repo_path: &'static str },
    /// 由 cc-monitor 现场生成的文本片段（PowerShell profile 块、shell 别名块、钩子片段）。
    Generated,
    /// **不是我们提供的** —— 内容由别人放在那儿，我们只读它。`who` 说清是谁放的。
    ///
    /// 〔`K-R60` 09-11 加〕上面五个变体都预设「这东西的内容出自本仓」，
    /// 而 app 最吃重的那一项（Claude Code 自己写的会话记录）根本不出自本仓。
    /// 没有这一格，它就只能靠**不进表**来表示 —— 而那正是本件在治的病。
    NotOurs { who: Text },
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
    ///   而当年的 `deploy_remote_acct_iso(cfg, dest_dir)`〔散文墓碑〕是**远端**部署、
    ///   落点还是**前端传进来的** `dest_dir`。
    ///
    /// 两处都是我凭印象写的常量。**声明一个不存在的常量比不声明更坏**——审计页会拿它去
    /// 查一个没人写的路径，然后言之凿凿地报"缺失"。所以这里显式承认"这是配置项"。
    ///
    /// `token` 是申报路径里用的占位符（形如 `$APP_DIR`，与 `$PROFILE` 同一套写法，
    /// 因此仍满足 `path` 的 ASCII-graphic 判据）；`what` 是给用户看的「去哪儿改」。
    UserConfiguredPath { token: &'static str, what: Text },
    /// **我们不装它** —— 这一格回答的不是「装到哪」，而是「它本来在哪」。
    ///
    /// 〔`K-R60` 09-11 加〕别的五个变体都在回答「我们把它放到哪儿去」。
    /// 一个我们**从不安装、只去读**的东西（`~/.claude/projects/`）填任何一个都是在说假话，
    /// 而「声明一个不存在的落点比不声明更坏」这句话本模块自己写过。
    /// ⇒ 显式承认「这不是我们的落点」。
    /// 判据 `installable_tools_declare_where_they_land` 钉住：用了这一格就不许 `installable: true`。
    NotInstalledByUs { whose: Text },
}

/// 这个文件在**哪台机器**上。
///
/// ## 为什么这是 `TouchedFile` 的属性，不是工具的属性
///
/// T04 第一步。它不是"为模型而模型"——不加它，`config_surface` 在**生产平台上会说假话**：
/// `cc-bus` 的 `destination` 是 `LocalHomeRelative`，三条 touches 于是被当**本机路径**去 stat。
/// 但 cc-monitor 的生产平台是 Windows（`ci.yml`/`release.yml` 打包 job 都是 `windows-latest`），
/// 而 cc-bus 跑在 **Claude Code 所在的那台**——钩子诊断为此按 origin 问那台后端（〔MIG-3b〕帧命令 `hooks-diag`），
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
    pub note: Option<Text>,
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
/// `local_backend::ccm_entry_shim`〔散文墓碑〕现造的 shim，而**本机那一份的来源是后端二进制
/// 自己的改名副本**（`local_backend::install_local_ccm_entry`〔散文墓碑〕）——
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
    pub what: Text,
    pub source: ToolSource,
    pub destination: ToolDestination,
    pub touches: &'static [TouchedFile],
}

/// 一个受管工具的完整声明。
///
/// **所有字段必须是 `const`-可构造的声明式数据**（无函数指针、无 `dyn`、无 `String`）。
/// 〔CP2b · 4C〕唯一的例外是给人看的那几格：[`Text`] 里包一个**不捕获的取文闭包**（话住文案表，
/// 常量里调不了函数）。它不做探测、不读环境，取出来的永远是表里那一句 —— 上面那条边界没破。
/// 这不是风格偏好，是上面那条「探测机制不进来」边界的落地形式。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ToolSpec {
    pub id: &'static str,
    pub display_name: Text,
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
    #[cfg_attr(not(test), allow(dead_code))]
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
    // 〔MIG-3b 续〕落在 Claude 布局里的那几条（`cc-bus` · `accounts` · `skill-install` · `claude-code`）住适配层
    //   `agents/claudecode/footprint.rs::TOOLS`，经注册表（`agents::Adapter::footprint`）汇进 [`tools`]。
    ToolSpec {
        id: "ccm",
        display_name: Text(|| copy_text("rsToolRegistry.tools.ccmName", &[])),
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
        //    改名副本（`local_backend::install_local_ccm_entry`〔散文墓碑〕）。`ToolSource` 一个字段
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
                what: Text(|| copy_text("rsToolRegistry.tools.ccmRemoteWhat", &[])),
                // 〔E2 · V28〕远端 `ccm` 就是推过去的那一份后端字节（三行 shim 与它的生成器删了）。
                source: ToolSource::EmbeddedBinary {
                    repo_path: "embedded-backends",
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
                        note: Some(Text(|| {
                            copy_text("rsToolRegistry.tools.ccmRemoteProfileNote", &[])
                        })),
                        host: HostScope::Remote,
                        effect: TouchEffect::FencedBlock,
                    },
                    // 〔GP1 · 第四波〕`设计/01 §6.7b` 迁移 ② ③：旧版入口落在这儿（09-11 前是 bash 启动器、之后是三行 shim）。
                    TouchedFile {
                        path: "~/.local/bin/ccm",
                        note: Some(Text(|| {
                            copy_text("rsToolRegistry.tools.ccmLegacyNote", &[])
                        })),
                        host: HostScope::Remote,
                        effect: TouchEffect::RetiredLegacy,
                    },
                    // 〔E2 · E-c〕旧默认 `backendPath` 落下的那份后端字节：部署时 ＋ 每次连上各扫一次，身份戳认得出才删
                    //   （〔MIG-3b〕本机常驻后端出计划时判，`sftp.rs::apply_legacy` 照计划删）。
                    TouchedFile {
                        path: "~/.cc-monitor/bin/cc-monitor-backend",
                        note: Some(Text(|| {
                            copy_text("rsToolRegistry.tools.backendLegacyNote", &[])
                        })),
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
                what: Text(|| copy_text("rsToolRegistry.tools.ccmLocalWhat", &[])),
                // 〔E2 · V28〕本机 `ccm` 就是后端本身（逐字节副本删了）：放它的是 `extract_embedded_to`。
                source: ToolSource::EmbeddedBinary {
                    repo_path: "src/frontend/shell/src/local_backend.rs::extract_embedded_to",
                },
                destination: ToolDestination::LocalHomeRelative(".cc-monitor/bin/ccm*"),
                touches: &[
                    TouchedFile {
                        // ⚠ **末段是 glob 而不是 `ccm`**，而且这不是偷懒：本机那份是要**被起成进程**的，
                        // 在把扩展名当身份的平台上它叫 `ccm.exe`（名字的唯一真相源是
                        // `local_backend::local_ccm_entry_name`，后缀由 `build.rs` 按 `TARGET` 算）。
                        // 写死 `ccm` 会让这一行在 Windows 上**恒显示「缺失」** —— 那正是本页
                        // 头注禁的「对能用的安装报假警报」。两边由
                        // `the_declared_local_ccm_path_really_matches_the_name_we_install` 对拍。
                        path: "~/.cc-monitor/bin/ccm*",
                        note: Some(Text(|| copy_text("rsToolRegistry.tools.ccmLocalNote", &[]))),
                        host: HostScope::Client,
                        effect: TouchEffect::OwnedFile,
                    },
                    // 〔E2 · E-c〕旧版释放的 `cc-monitor-backend-<build_id>` 们：放好 `ccm` 之后扫一次，身份戳认得出才删。
                    TouchedFile {
                        path: "~/.cc-monitor/bin/cc-monitor-backend-*",
                        host: HostScope::Client,
                        note: Some(Text(|| {
                            copy_text("rsToolRegistry.tools.backendLegacyExtractNote", &[])
                        })),
                        effect: TouchEffect::RetiredLegacy,
                    },
                ],
            },
        ],
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
        display_name: Text(|| copy_text("rsToolRegistry.tools.backendName", &[])),
        installable: true,
        // 🔴 〔`K-R63` 09-11〕**这一格原先是 `false`，而它是一处假申报** —— 本件那条新性质
        // 落地的当场把它逮出来的（不是人看出来的）。卸载实现一直在：
        // `sftp.rs::uninstall_remote_backend` 是**设置面板「卸载后端」按钮**背后那条命令
        // （删后端二进制；〔E2〕落点是固定的 `~/.cc-monitor/bin/ccm`，`is_safe_remote_backend_path`〔散文墓碑〕那道守卫随之删了）。
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
                what: Text(|| copy_text("rsToolRegistry.tools.backendBesideWhat", &[])),
                source: ToolSource::EmbeddedBinary {
                    repo_path: "src/frontend/shell/binaries/cc-monitor-backend",
                },
                // **路径不是常量，也不是家目录相对** —— 它跟着 app 装到哪儿走，
                // 而那个目录是装机时由人选的。同 `$ACCT_ISO_DEST` 那一格的理由：
                // 申报一个我们其实没在用的常量，审计页会拿它去查一个没人写的路径
                // 然后言之凿凿地报「缺失」。
                destination: ToolDestination::UserConfiguredPath {
                    token: "$APP_DIR",
                    what: Text(|| copy_text("rsToolRegistry.tools.backendBesideDestWhat", &[])),
                },
                touches: &[TouchedFile {
                    path: "$APP_DIR",
                    host: HostScope::Client,
                    note: Some(Text(|| {
                        copy_text("rsToolRegistry.tools.backendBesideNote", &[])
                    })),
                    effect: TouchEffect::OwnedFile,
                }],
            },
            // 〔E2 · V28〕内嵌那份的本机载体并进了 `ccm` 那一条的本机载体：它放下去的就是 `~/.cc-monitor/bin/ccm`（后端本身），
            //   同一个文件不在两个工具底下各记一遍（那张「本机 `ccm` 落点恰一条」的对拍要这一格）。
            Carrier {
                what: Text(|| copy_text("rsToolRegistry.tools.backendRemoteWhat", &[])),
                source: ToolSource::EmbeddedBinary {
                    repo_path: "embedded-backends",
                },
                // 〔E2 · V28〕可填的 `backendPath` 删了：落点恒是那台的 `~/.cc-monitor/bin/ccm`（它就是 `ccm`）。
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
    // 〔TL1 · 4C〕**代码全景小程序**（`cc-monitor-panorama`，`src/panorama-engine`）—— RM1f 起它是**有落点的部署物**：
    //   monitor 摘掉了内嵌引擎（V108 后半句），本机与远端的全景都由那台机器的后端经插件口起它。
    //   同 `backend` 那一形：一份代码、两个载体 —— 本机那份由 `panorama_bytes::place_local` 放、远端那份由 `panorama_bytes::push_to` 推，
    //   都只在那台后端答「没装 / 太旧」时才放（V108「只传给开过远端全景的机器」）。
    //   〔FIX4 · `97 §8` · 主会话 09-28 裁〕`uninstallable: true`：卸口是那台后端的 `panorama-uninstall`（认身份、只删装时放下的那一份，
    //   机器页「工具」栏的「代码全景组件」那一格点）。〔墓碑 —— 这之前是 `false`：「今天没有卸掉全景组件这条口」。〕
    //   〔墓碑 —— 这之前它住 [`NOT_MANAGED`]，理由是「vendored 进 monitor 二进制、没有落点」；那个身份 RM1f 起没了，见那一条。〕
    ToolSpec {
        id: "panorama",
        display_name: Text(|| copy_text("rsToolRegistry.tools.panoramaName", &[])),
        installable: true,
        uninstallable: true,
        carriers: &[
            Carrier {
                what: Text(|| copy_text("rsToolRegistry.tools.panoramaLocalWhat", &[])),
                source: ToolSource::EmbeddedBinary {
                    repo_path: "src/frontend/shell/native-backend/cc-monitor-panorama",
                },
                destination: ToolDestination::LocalHomeRelative(
                    ".cc-monitor/bin/cc-monitor-panorama",
                ),
                touches: &[TouchedFile {
                    path: "~/.cc-monitor/bin/cc-monitor-panorama",
                    host: HostScope::Client,
                    note: Some(Text(|| {
                        copy_text("rsToolRegistry.tools.panoramaLocalNote", &[])
                    })),
                    effect: TouchEffect::OwnedFile,
                }],
            },
            Carrier {
                what: Text(|| copy_text("rsToolRegistry.tools.panoramaRemoteWhat", &[])),
                source: ToolSource::EmbeddedBinary {
                    repo_path: "embedded-backends",
                },
                destination: ToolDestination::RemoteHomeRelative(
                    ".cc-monitor/bin/cc-monitor-panorama",
                ),
                touches: &[TouchedFile {
                    path: "~/.cc-monitor/bin/cc-monitor-panorama",
                    host: HostScope::Remote,
                    note: Some(Text(|| {
                        copy_text("rsToolRegistry.tools.panoramaRemoteNote", &[])
                    })),
                    effect: TouchEffect::OwnedFile,
                }],
            },
        ],
    },
    ToolSpec {
        id: "project-mcp",
        display_name: Text(|| copy_text("rsToolRegistry.tools.projectMcpName", &[])),
        installable: true,
        uninstallable: true,
        carriers: &[
            Carrier {
                what: Text(|| copy_text("rsToolRegistry.tools.projectMcpWhat", &[])),
                source: ToolSource::Generated,
                destination: ToolDestination::ProjectRelative(".mcp.json"),
                touches: &[TouchedFile {
                    path: ".mcp.json",
                    host: HostScope::ProjectDir,
                    note: Some(Text(|| {
                        copy_text("rsToolRegistry.tools.projectMcpNote", &[])
                    })),
                    effect: TouchEffect::OwnedFile,
                }],
            },
            // 〔AS1 · 第四波 4B〕**推 / 拉**（`设计/96` 的 B，用户 09-24 V111 · V112）：同一份文件的**第二个写入来源** ——
            //   内容不是这台机器上现场编的，是从另一台机器那份里**原样**拷来的条目（`mcp_sync.rs`）。
            //   落点、写法（经那台后端 `files-put`）与上一格同一个；单列一格是为了让「这个 app 动过你哪些文件」
            //   那一页说得出「有些条目是从别的机器搬来的」（`96 §4`：每个写点都要在足迹里可见）。
            //   远端那台的足迹栏按那台机器问（RM1a），这一格的 `host` 与上一格同是项目目录 —— 在哪台上就算哪台的。
            Carrier {
                what: Text(|| copy_text("rsToolRegistry.tools.mcpSyncWhat", &[])),
                source: ToolSource::Generated,
                destination: ToolDestination::ProjectRelative(".mcp.json"),
                touches: &[TouchedFile {
                    path: ".mcp.json",
                    host: HostScope::ProjectDir,
                    // 〔AS2 · 4B〕资产目录那一块的「装到这台」（MCP）走的就是这一格（同一条命令 `mcp_sync_apply`，只勾那一条）。
                    note: Some(Text(|| copy_text("rsToolRegistry.tools.mcpSyncNote", &[]))),
                    effect: TouchEffect::OwnedFile,
                }],
            },
        ],
    },
    // ═══ 〔`K-R62` 09-11〕**从第三档升上来的第一项** ═══
    //
    // 它昨天还住在 [`UNMANAGED_ENV`]（`app 假设它在`），`why` 那一格逐字写着
    // 「加与删两侧都只造 PowerShell 那两条 profile 路径，POSIX rc 一条都不扫」。
    // 本件把那句话变成了假的：`profile_installer::plan_install` 的 `PosixRc` 臂把那个
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
            // `include_str!`）。远端那条 `ccm` 用的是同一份 —— 那正是本件不许出现第二份的东西。
            source: ToolSource::EmbeddedText {
                repo_path: "src/shared/ccm-aliases.sh",
            },
            // 🔴 **路径由人选，产品不猜** —— 这一格用占位符而不是 `~/.bashrc`，
            // 理由与后端那条 `$APP_DIR` 逐字同源：申报一个我们其实没在用的常量，
            // 审计页会拿它去查一个没人写的路径然后言之凿凿地报「缺失」。
            // `.bashrc` / `.zshrc` / `config.fish` 写法不同，替人选一份是最坏的那条路
            // （`account_aliases` 的 `§0e`）。
            destination: ToolDestination::UserConfiguredPath {
                token: "$POSIX_RC",
                what: Text(|| copy_text("rsToolRegistry.tools.posixAliasesRcWhat", &[])),
            },
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
            destination: ToolDestination::UserShellProfile,
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

// 〔MIG-3b 续〕`local_ccm_bin_dir_rel` / `remote_ccm_bin_dir_rel`〔散文墓碑〕（从本表现算 `ccm` 两个载体落点的目录）搬进了判据档：
//   生产上要那个目录的只有 monitor 补 PATH 那一步，它今天取共享 crate 里后端的落点（`relay_route_core::BACKEND_LANDING_REL`，
//   本机远端同一个，V28）；本表申报的那两条与它相等由 `tests/backend/footprint/registry_tests.rs` 对拍。

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
    /// 三值的**闭集**本身。现算用〔`13b`：报一个基数也是复述〕。〔MIG-3b 续〕今天只有判据用它与 [`Self::label`]。
    #[cfg_attr(not(test), allow(dead_code))]
    pub const ALL: &'static [Provisioning] = &[
        Provisioning::AppShips,
        Provisioning::UserProvides,
        Provisioning::NotAnInstall,
    ];

    /// 给人看的措辞。定在这里，UI 与诊断文本不再各写一遍。
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn label(self) -> String {
        match self {
            Provisioning::AppShips => copy_text("rsToolRegistry.provisioning.appShips", &[]),
            Provisioning::UserProvides => {
                copy_text("rsToolRegistry.provisioning.userProvides", &[])
            }
            Provisioning::NotAnInstall => {
                copy_text("rsToolRegistry.provisioning.notAnInstall", &[])
            }
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
    /// 四档的**闭集**本身。现算用（`len()` 就是「几档」，不许在别处写死一个基数）。〔MIG-3b 续〕今天只有判据用它与 [`Self::label`]。
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
#[derive(Clone)]
pub struct UnmanagedEnv {
    pub id: &'static str,
    pub display_name: Text,
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
    /// **app 在哪儿用到它** —— 给人看的那一句（住文案表，这里放取它的函数）。
    ///
    /// 〔CP2b · 4C〕它原先一格装两件事：给人看的话 ＋ 结尾一个 `<相对 src 的路径>.rs::<符号>` 代码住址。
    /// 这一格会上配置面（`config_surface` 的 `note`），而界面文字不露源码住址（`设计/91 §2.1`）
    /// ⇒ 拆成两格：话在这里，住址在 [`Self::site`]。
    pub why: Text,
    /// **那个判断长在哪段代码上** —— 必须是一个 `<相对 src 的路径>.rs::<符号>` 形态的住址。
    /// 判据只判「**有没有**住址」；那个住址今天解析不解析得到，由 `structural_scan` 里
    /// 那条扫全仓代码住址的判据管（它会报「找不到这个符号 / 符号搬家了」）。
    /// 〔CP2b〕从 `why` 的结尾拆出来的那一半 —— 不上界面。
    pub site: &'static str,
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
    /// `PATH` 上的一个裸命令 —— 走 `rows.rs::resolves_on_path`。
    ///
    /// **用已有那一把，不新写一个 `which`**：它已经把两条坑填了 ——
    /// 切分必须走 `std::env::split_paths`（Windows 的 `;` 与盘符冒号），
    /// 以及「取不到 `PATH` 就返回 `None`（**不猜**）」。
    OnPath,
    /// 查不动，**理由必填**：值由别处决定（占位符 / 用户配置），本页不猜。
    ///
    /// ⚠ 填这一支之前先问一遍：是真的查不动，还是**懒得查**？后者写在这里就是
    /// 拿「查不动」当「不查」的遮羞布 —— 那正是本件在治的病。
    CannotProbe { why: Text },
}

/// 闭集里手写的那一半。
///
/// 🔴 〔`K-R65` 09-11〕**这张表原来「全是第三档」，今天一条都不是** ——
/// `K38` 之后它分成了两群：9 项通用工具是 [`Provisioning::UserProvides`]，
/// [`Provisioning::AppShips`] 那一群今天是空的（原先那一项是外部账号工具的本机那份；账号库改由后端自己建，它出了表）。
pub const UNMANAGED_ENV: &[UnmanagedEnv] = &[
    // 〔MIG-3b 续〕`claude-cli` 那一条住适配层（`agents/claudecode/footprint.rs::UNMANAGED_ENV`）。
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
        site: "platform/terminal.rs::TERMINAL_EXITS",
    },
    UnmanagedEnv {
        id: "git",
        display_name: Text(|| "git".to_string()),
        who: Provisioning::UserProvides,
        probe: EnvProbe::OnPath,
        named: "git",
        host: HostScope::Client,
        why: Text(|| copy_text("rsToolRegistry.env.gitWhy", &[])),
        site: "dial_home_registry_tests.rs::git_read",
    },
    UnmanagedEnv {
        id: "ssh",
        display_name: Text(|| copy_text("rsToolRegistry.env.sshName", &[])),
        who: Provisioning::UserProvides,
        probe: EnvProbe::OnPath,
        named: "ssh",
        host: HostScope::Client,
        why: Text(|| copy_text("rsToolRegistry.env.sshWhy", &[])),
        site: "platform/terminal.rs::ssh_client_available",
    },
    // 〔SH1 · V136〕`pgrep` 那一行摘了：它唯一的 Rust 住址是驾驶舱 shell 读那条「超时不留孤儿」判据的数进程助手，随那条读一起退役。
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
        site: "ccm_probe.rs::probe_with",
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
        site: "mcp_edit.rs::answer_put",
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
    pub display_name: String,
    /// **谁该装**（`K38`）。「app 自带」这个人群就是 `who == AppShips` 的那几项。上界面的是由它派生的 [`Self::tier`] ⇒ 生产段零读者。
    #[cfg_attr(not(test), allow(dead_code))]
    pub who: Provisioning,
    /// **档** —— 由 `who` ×「今天有没有装口」两格**派生**（[`EnvTier::of`]），不是手填的。
    pub tier: EnvTier,
    /// 给人看的那一句（〔CP2b〕取过文的）。
    pub why: String,
    /// 代码住址（〔CP2b〕从 `why` 拆出来的那一半，见 [`UnmanagedEnv::site`]）。不上界面 ⇒ 生产段零读者，只有判据核它。
    #[cfg_attr(not(test), allow(dead_code))]
    pub site: &'static str,
    pub backing: EnvBacking,
}

/// 〔MIG-3b 续〕受管工具的**全集**：本表 [`TOOLS`] ＋ 注册表里各家足迹面带来的那一半（落在那一家布局里的），按注册表顺序。
pub fn tools() -> Vec<&'static ToolSpec> {
    TOOLS
        .iter()
        .chain(crate::agents::footprint_faces().flat_map(|f| f.tools.iter()))
        .collect()
}

/// 〔MIG-3b 续〕环境清单手写那一半的**全集**：本表 [`UNMANAGED_ENV`] ＋ 各家足迹面带来的那几条。
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
