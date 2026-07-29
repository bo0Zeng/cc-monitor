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
    /// **路径由用户配置决定，不是常量。**
    ///
    /// T02 审计追问「注册表与真写入方零耦合」时查出来的（比审计报的更严重）：
    /// - `remote-daemon` 原先声明 `RemoteHomeRelative(".local/bin/ccm-daemon")`，
    ///   而这个字符串**全仓只出现在注册表自己里**；真实路径是 `RemoteConfig.daemon_path`，
    ///   每个远端各自配置（`remote_history.rs:46` 直接 `shell_quote(&cfg.daemon_path)`）。
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
/// **6 个实例化** —— 这是本结构里最扎实的一项，也是 T02 审计视图的直接输入。
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
        // 部署尚未实现（B01 只做了"搬进仓固化为基线"）——**如实声明 false**，
        // 不因为"计划里写了要做"就先标 true。
        installable: false,
        uninstallable: false,
        touches: &[
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
                // **`Remote` 而不是 `Either`**（T04 审计阻塞 2）：`cc_bus.rs` 的全部 5 个 IPC
                // （`read_cc_bus_state` / `check_cc_bus_agent_online` / `read_cc_bus_inbox` /
                //  `cc_bus_send` / `cc_bus_spawn`）都以 `origin` 入参走 `cfg_of` → ssh 远端 exec，
                // **一条本机读取路径都没有**；驾驶舱的 origin 下拉来自 `list_remote_mcp_origins`，
                // 连"本机"这一档都没有。
                //
                // 标 `Either` 的后果是**用一个新的假阳性换掉旧的假阴性**：本机恰好有
                // `~/.cc-bus/`（开发机上就有）时，这一行会**确定地**说「本机存在（目录）」，
                // 配上 `IndirectWrite` 那句"你在 cc-monitor 里的操作会让它被写"——
                // 而我们写的是**远端**那个。把用户不关心的那台的目录冒充成"我们会动的那个"。
                host: HostScope::Remote,
                note: Some(
                    "运行期状态：inbox / 名册 / 队列 / 日志。                     驾驶舱读它；但你在驾驶舱点「发消息」/「派活」会让 cc-send / cc-spawn 往这里追加",
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
                // `/home/zbl/.claude-accts/z`，而这一行写着「位置：远端」。
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
