//! 足迹申报表的**词**（原住 `footprint/registry.rs`，10-13 挪来）：一条受管工具（[`ToolSpec`]）· 它的载体与碰的文件 ·
//! 一条不由我们管的环境项（[`UnmanagedEnv`]）· 谁该装（[`Provisioning`]）· 怎么查（[`EnvProbe`]）。
//!
//! 为什么住适配层：每家 agent 用这套词申报落在自己布局里的那几条（`agents/<家>/footprint.rs`，经 [`super::FootprintFace`] 汇总）；
//! 足迹那边（`footprint/registry.rs` 两张表 ＋ 档的派生、`footprint/rows.rs` 的判定）读它。词放在足迹那边的话，适配层就得反过来引足迹
//! （`layering_guard::ONE_WAY`：适配层在功能之下）。
//! 下面的文档里没写全路径的 `environment` · `EnvTier` · `TOOLS` · `UNMANAGED_ENV` 都住 `crate::footprint::registry`。

use copy_core::copy_text;

/// 表里给人看的那一句 —— **取文函数**，不是字面量。
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

/// 内容的来源。**每个变体的使用者数现算**（`TOOLS` 是唯一一份），不写死在这里。
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
    /// 没有这一格，它就只能靠**不进表**来表示 —— 而那正是这一层在治的病。
    NotOurs { who: Text },
}

/// 装到哪。**变体数与使用者数现算**，不写死在这里。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum ToolDestination {
    /// 远端家目录下的相对路径（`~/.local/bin/ccm`）。
    ///
    /// # 🔴 **它 09-12 下午重新有使用者了，而那不是回退**
    ///
    /// `K-R69`（09-12 上午）写过一段「它零使用者、`never constructed` 就是这笔债的存根」，
    /// 并把退役条件写死成「**下一件活里若仍没有「只落在远端」的工具进来**就删」。
    /// [`Carrier`] 这一层立起来之后，那个条件**不成立了**：
    /// `ccm` 的**远端那一份**（shim，`~/.local/bin/ccm`）就是一个只落在远端的载体，
    /// 它与本机那一份不再需要挤在同一个 `destination` 里 ——
    /// 「一个东西两个落点」这件事今天由**载体这一维**表达，不由一个双值变体表达。
    /// ⇒ 没有双值变体（理由在 [`Carrier`] 的头注里）。
    ///
    /// ⚠ **退役条件仍然有效，只是今天不满足**：哪天连 `ccm` 远端那条也没了，
    /// 就连着它在 `config_surface::resolve_by_destination` 的那一臂一起删
    /// （那一臂是今天**唯一**一处「不看 `host` 就断定远端」的地方，
    /// 删掉之后「在哪台机器上」就只剩 `host` 一个住址）。
    RemoteHomeRelative(&'static str),
    /// 本机家目录下的相对路径（`~/.claude/skills/...`）。
    LocalHomeRelative(&'static str),
    /// 用户的 shell profile —— 哪一种由 [`ShellProfile`] 说（PowerShell 的 `$PROFILE` · 用户选定的 POSIX rc）。
    /// 申报路径写那一种的占位符（[`ShellProfile::token`]），不写一个凭印象猜的常量。
    ///
    /// 〔size2 10-11〕从前另有一个「路径由用户配置决定」的变体 `UserConfiguredPath { token, what }`：安装包旁那份后端（`$APP_DIR`）撤掉之后
    /// 它只剩 POSIX rc 一个使用者，而那一格说的就是「用户选的 shell profile」⇒ 按 ≥2 规矩并进这里。
    UserShellProfile(ShellProfile),
    /// 项目目录内的文件（`<dir>/.mcp.json`）。
    ProjectRelative(&'static str),
    /// **我们不装它** —— 这一格回答的不是「装到哪」，而是「它本来在哪」。
    ///
    /// 〔`K-R60` 09-11 加〕别的五个变体都在回答「我们把它放到哪儿去」。
    /// 一个我们**从不安装、只去读**的东西（`~/.claude/projects/`）填任何一个都是在说假话，
    /// 而「声明一个不存在的落点比不声明更坏」这句话本模块自己写过。
    /// ⇒ 显式承认「这不是我们的落点」。
    /// 判据 `installable_tools_declare_where_they_land` 钉住：用了这一格就不许 `installable: true`。
    NotInstalledByUs { whose: Text },
}

/// [`ToolDestination::UserShellProfile`] 是哪一种 shell profile。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ShellProfile {
    /// PowerShell 的 `$PROFILE`：路径由 PowerShell 自己定，这台问得出来（`PathResolution::WindowsProfile`）。
    PowerShell,
    /// 用户在确认卡上选的那一份 POSIX rc（`.bashrc` / `.zshrc` / `config.fish` 写法不同，产品不替人选）：
    /// 路径是配置项，不是常量 ⇒ 解析成「去哪儿看」那一句（[`ShellProfile::what`]）。
    PosixRc,
}

impl ShellProfile {
    /// 申报路径里的占位符（与 `path` 的 ASCII-graphic 判据同一套写法）。
    pub fn token(self) -> &'static str {
        match self {
            ShellProfile::PowerShell => "$PROFILE",
            ShellProfile::PosixRc => "$POSIX_RC",
        }
    }

    /// 用户选的那一份去哪儿看（只有 [`ShellProfile::PosixRc`] 要说；`$PROFILE` 这台问得出来）。
    pub fn what(self) -> String {
        match self {
            ShellProfile::PowerShell => "$PROFILE".to_string(),
            ShellProfile::PosixRc => copy_text("rsToolRegistry.tools.posixAliasesRcWhat", &[]),
        }
    }
}

/// 这个文件在**哪台机器**上。
///
/// ## 为什么这是 `TouchedFile` 的属性，不是工具的属性
///
/// T04 第一步。它不是"为模型而模型"——不加它，`config_surface` 在**生产平台上会说假话**：
/// `cc-bus` 的 `destination` 是 `LocalHomeRelative`，三条 touches 于是被当**本机路径**去 stat。
/// 但 cc-monitor 的生产平台是 Windows（`ci.yml`/`release.yml` 打包 job 都是 `windows-latest`），
/// 而 cc-bus 跑在 **Claude Code 所在的那台**——钩子诊断为此按 origin 问那台后端（帧命令 `hooks-diag`），
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
/// 本结构是 T02 审计视图的直接输入（使用者数现算，不写死在这里）。
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
    IndirectWrite,
}

/// **同一个东西的一种载体** —— 「它这一份怎么产出来、落到哪、碰哪些文件」。
///
/// # 🔴 这一层为什么非有不可
///
/// 用户逐字：「**一个后端要两处使用 / 即远程后端就是远程本地机器的后端**」。
/// 也就是说：**远端那台机器上跑的那一份，是「那台机器的本地后端」**，
/// 不是「远端的后端」——「本机 / 远端」这个二分本身就是从错的名字里长出来的。
///
/// 而在这一层立起来之前，[`ToolSpec`] 是「一个源 + 一个落点 + 一串 touch」：
/// **同一个后端有三种载体、四个落点，而闭集只表达得了一个半**（`K-R68` 现打）。
/// 三种载体逐条是：① 这一份产物自己带着、要用时自释放的那份（`native-backend/`，
/// `byte_table.rs` 的 `include_bytes!`，按这一份产物的 `TARGET` 那一格取）·
/// ② 内嵌、推给远端那台机器的那份（`embedded-backends/`）·
/// ③ 安装包放在 app 可执行文件旁边的那份（Tauri `externalBin`；size2 10-11 撤了）。
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
/// # 一个工具几个落点 ⇒ 几个载体
///
/// 一个 `destination` 不必装下两个落点：`ccm` 是两个载体，远端那个 `RemoteHomeRelative`、
/// 本机那个 `LocalHomeRelative`，各自带各自的 touch（**闭集里本机侧真有一条 `ccm` 落点**，有判据钉着）。
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
/// 唯一的例外是给人看的那几格：[`Text`] 里包一个**不捕获的取文闭包**（话住文案表，
/// 常量里调不了函数）。它不做探测、不读环境，取出来的永远是表里那一句 —— 上面那条边界没破。
/// 这不是风格偏好，是上面那条「探测机制不进来」边界的落地形式。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ToolSpec {
    pub id: &'static str,
    pub display_name: Text,
    /// 能不能装/升。
    ///
    /// 🔴 **这一格是「app 装的」与「app 只查」两档的分界线**（`K-R60`）：
    /// [`environment`](crate::footprint::registry::environment) 就是读它算出每条 `ToolSpec` 属于哪一档的。
    /// ⇒ 它填错了，用户在配置面上读到的「能否装/撤」与清单上的档**同时**是假的。
    /// 守它的是 `every_tool_declares_install_and_uninstall_as_the_implementations_really_are`
    /// （读**字段值**，不是注释里的词频 —— 这一格上一次就是被词频守卫放过去的）。
    /// 上一版守它的那条只服务 `cc-bus` 一个工具，**名字里带工具名**；
    /// 今天这条是**覆盖全表**的性质，两格一起对拍，两个方向都判。
    pub installable: bool,
    /// 能不能卸。
    ///
    /// 🔴 **它与 `installable` 是同一枚硬币，而它先前只有半边被守**：
    /// `fenced_block_implies_uninstallable` 守的是「有围栏 ⇒ 必须声明可卸」（少报那一向），
    /// **多报**（声明可卸而盘上根本没有卸载实现）一条都不红 —— PM 的刀 C 把
    /// 当时账号隔离工具那一格（`cc-acct-iso`，今已删出仓、账号库由后端自己管）由 `false` 翻成 `true`，全表 1379 条一条没响。
    /// ⇒ 今天与 `installable` 同走上面那条性质：字段值 ⇔ 盘上那个符号在不在。
    pub uninstallable: bool,
    /// 🔴 **同一个东西的几种载体**，头注住 [`Carrier`]。
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
    /// 铺平会把这里立起来的那个 key 又丢掉一次。
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

/// 🔴 〔`K38` / `KR65D2`〕**「谁该装」** —— 与「今天装得了吗」是两个问题，两格分开装。
///
/// # 为什么非拆不可
///
/// 拆之前只有一个 `ToolSpec::installable`，而它同时被当成两句话读：
///   · 「**今天盘上有没有一个装口**」—— 读的是**实现**，`K-R63` 那条对拍表钉死了它；
///   · 「**这东西该不该由我们装**」—— 读的是**判断**，`K38` 裁的正是它。
///
/// 于是当时 `cc-acct-iso-local` 那一格（今已随那个工具退役）那种「**该由 app 装（`K38`），而实现还没有**」的状态
/// **一格都申报不了**：写 `true` 是假申报（`K-R63` 当场逮到），
/// 写 `false` 等于说「不该我们装」（与 `K38` 矛盾）。⇒ 两句话各给一格。
///
/// # 🔴 「app 自带的是哪几样」这个人群的**唯一住址**就是 [`Provisioning::AppShips`]
///
/// `KR65D3`：要人数就 [`environment`](crate::footprint::registry::environment) 里现算（`filter(who == AppShips)`），
/// **别处不许再手抄一张自带清单** —— 那是第二个。
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
    /// 三值的**闭集**本身。现算用〔报一个基数也是复述〕。今天只有判据用它与 [`Self::label`]。
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
    /// # 🔴 多载体之后的**聚合规则**，写在这里而不是靠人记得
    ///
    /// 一个工具现在有**一串**载体（[`Carrier`]）⇒ 「它是不是我们装的」要从一串
    /// `destination` 聚合出来。**规则：全部载体都是 `NotInstalledByUs` 才算「不是装出来的」。**
    /// 理由：只要有**一份**是我们放下去的，这东西对用户就是「app 装的」——
    /// 把它算成「别人的产物」会让配置面那一列直接说假话。
    ///
    /// ⚠ **混合的那一形今天盘上不存在**（判据 `carriers_do_not_mix_ours_and_not_ours`
    /// 钉着，两向都判）。它**本来就该变的时候去哪儿重裁**：真出现一个「一半我们装、
    /// 一半别人的」的工具，那条判据会当场红并点名 —— 那时重裁的是这条聚合规则本身，
    /// 不是把判据放宽。
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
    /// 🔴 **「谁该装」** —— 这一格取代了原来那个手填的 `tier`。
    ///
    /// 原文逐字：「`tier: EnvTier`／这一项属于哪一档。**必须写出来** —— 第三档就是靠这一格
    /// 存在的。」那时档是**手填**的，于是「档」这一个字段同时装着「谁该装」与
    /// 「今天装得了吗」两件事。今天档由 [`EnvTier::of`](crate::footprint::registry::EnvTier::of) 从这一格 ＋「有没有 `ToolSpec`」
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
    /// 它原先一格装两件事：给人看的话 ＋ 结尾一个 `<相对 src 的路径>.rs::<符号>` 代码住址。
    /// 这一格会上配置面（`config_surface` 的 `note`），而界面文字不露源码住址
    /// ⇒ 拆成两格：话在这里，住址在 [`Self::site`]。
    pub why: Text,
    /// **那个判断长在哪段代码上** —— 必须是一个 `<相对 src 的路径>.rs::<符号>` 形态的住址。
    /// 判据只判「**有没有**住址」；那个住址今天解析不解析得到，由 `structural_scan` 里
    /// 那条扫全仓代码住址的判据管（它会报「找不到这个符号 / 符号搬家了」）。
    /// 从 `why` 的结尾拆出来的那一半 —— 不上界面。
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
    /// 这个后端进程 `PATH` 上的一个裸命令 —— 走 `platform::shell::find_session_command`（按 `PATH` 找命令只住那一处：
    /// 切分走 `std::env::split_paths`、Windows 的后缀次序、「取不到 `PATH` 就判不了（**不猜**）」都在那里）。
    OnPath,
    /// 起会话那个 shell 的 `PATH` 上的一个裸命令（agent 命令行：会话在用户登录 shell 里起，按那里的 `PATH` 找它）。
    /// 走 `platform::shell::find_session_command`（起新会话框「这台能起哪几家」、`ccm` 起它那一下也问它）：`PATH` 换成那一份，
    /// 文件名按那个 shell 的找法（Windows 上 `.exe` · `.cmd` · `.bat` · `.ps1` 都算）；问不出来 ⇒ 「查不动」，绝不说「缺」。
    InSessionShell,
    /// ssh 客户端：走开远端终端用的同一个找法（`platform::ssh_client::locate_with`：Windows 先查 System32\OpenSSH，再按 PATH 找 `ssh.exe`；别处按 PATH 找 `ssh`）。
    SshClient,
    /// 查不动，**理由必填**：值由别处决定（占位符 / 用户配置），本页不猜。
    ///
    /// ⚠ 填这一支之前先问一遍：是真的查不动，还是**懒得查**？后者写在这里就是
    /// 拿「查不动」当「不查」的遮羞布 —— 那正是这一层在治的病。
    CannotProbe { why: Text },
}
