# 功能计划 — T04 五套既有机制收编（第一步：`host` 维度 + `origin` 模型）

> **一句话**：先把「这个文件在哪台机器上」变成注册表能表达的东西，再谈收编——
> 否则收编出来的东西会在生产平台上说假话。

## 0. 第一步不是收编，是 `host` —— 而它**当下就有可见的错**

T02 与 T03 的 Phase E 都把这条记成了 T04 的前置。它不是"为模型而模型"，
现在就能指出一个**在生产平台上会说假话**的具体位置：

`cc-bus` 的 `destination` 是 `LocalHomeRelative(".claude/skills/cc-bus")`，
三条 `touches`（`~/.claude/settings.json` / `~/.local/bin/cc-*` / `~/.cc-bus/`）
于是被 `config_surface` 当**本机路径**去 stat。但：

- **cc-monitor 的生产平台是 Windows**（`ci.yml`/`release.yml` 打包 job 都是 `windows-latest`）；
- **cc-bus 跑在 Claude Code 所在的那台**——`hooks_diag` 为此提供了**两条** IPC
  （`diagnose_local_cc_bus_hooks` / `diagnose_remote_cc_bus_hooks`）；
- `cc_bus.rs::read_cc_bus_state(origin)` 读 `~/.cc-bus/` 是**按 origin 远端 exec** 的。

所以 Windows 客户端上打开「配置面审计」，那三行会显示 **「不存在」**，
而同一个 app 的 cc-bus 驾驶舱正从远端把 inbox 读得好好的。
**这是 T02 专门要防的那类假警报，出现在这一页上格外讽刺**——它的全部价值就是可信告知。

## 1. 模型：`host` 是 `TouchedFile` 的属性，不是工具的属性

```rust
pub enum HostScope {
    /// cc-monitor 自己跑的那台（Windows 客户端）。
    Client,
    /// 一个远端连接（按 origin 选）。本机**不许**替它回答"路径在不在"。
    Remote,
    /// **两端皆可**：Claude Code 跑在哪台，这东西就在哪台。
    /// 关键语义：**"本机没找到" ≠ "不存在"**。
    Either,
    /// 项目目录内（在哪台机器上取决于那个项目）。
    ProjectDir,
}
```

消费者：`Client` = PowerShell profile；`Remote` = ccm ×2 / daemon / cc-acct-iso 部署目录；
`Either` = cc-bus ×3；`ProjectDir` = 项目 MCP。（变体各 ≥1 个真实使用者，
与 `ToolSource` 同型——描述数据的 enum 允许变体各自单用户，这一点 T01 已论证过。）

**T03 那条教训要贯彻进类型**：`Remote` 一律解析成 `PathResolution::Remote`，
`observe` 只会给 `Undetermined`——**远端的"某个路径在不在"必须由远端自己回报，
不能由本机按 basename 猜**（T03 阻塞 3 的根因）。

**`Either` 是这次真正的新东西**：本机找到 → `Present`；本机没找到 → **`Undetermined` 而不是
`Absent`**，理由写明"这套东西可能装在远端"。这一条就是上面那个假警报的解药。

## 2. DoD

- [ ] `TouchedFile` 新增 `host: HostScope`，10 条 touches 全部如实声明
- [ ] `resolve_touched_path` 按 `host` 分派；新增 `PathResolution::EitherHost { local }`
- [ ] **`Either` 本机未命中判 `Undetermined` 而非 `Absent`**，且有变异覆盖
- [ ] `cc-bus` 三条改 `Either`；ccm/daemon/cc-acct-iso 改 `Remote`；MCP `ProjectDir`；PS `Client`
- [ ] **跨字段一致性守卫**：`host == Remote` ⇔ 解析结果是 `PathResolution::Remote`
      （这条**不是**同义反复——`host` 与 `destination` 是两个独立字段，改任一边就红。
      T02 那条被删的"钉子"之所以是同义反复，是因为它断言的两边其实是同一个来源）
- [ ] `installable_tools_declare_where_they_land` 与 `owned_file_implies_installable` 仍绿
- [ ] 行为等价：七套真机套件不动一条断言

**不做**（本轮只做第一步）：五套机制的**装/升/卸**真正走注册表（那是 T04 第二步，
要先有 `host` 才不会写出一个在 Windows 上说假话的部署器）；`shared/ccm` 本体零改。

## 3. 测试策略

- 纯函数：四个 `HostScope` × 命中/未命中的组合
- **反向自检**：把 cc-bus 三条改回 `Client` → `Either` 那条测试必须红
- 结构性守卫：`host == Remote` ⇔ `PathResolution::Remote`，带计数自检
- **上屏**：`Either` 的 `Undetermined` 文案必须出现在 DOM（T02/T03 两次教训）

## 4. 风险

- `TouchedFile` 加字段会触发 T02 那条字段纪律扫描（`touched_file_fields_follow_the_same_discipline`）
  ——`host` 必须 ≥2 个工具实质实例化。`Remote` 有 3 个工具、`Either` 有 1 个（cc-bus）：
  **按"字段"数是 6/6 个工具都给了实质取值**，过判据；但要注意扫描器把 enum 取值算不算"实质"
- 改 `TouchedFile` 形状会动 `config_surface` 的 12 处测试

## 5. 代码审计结果（Phase D）
（待填）

## 6. 工程审计结果（Phase E）
（待填）

## 7. 从 T03 接手、归 T07 的两条（别丢）
- A3（`remote-section.ts` 的待贴块）**在任何测试里都没被执行过**（`new RemoteSection` 全文 0 次）
- 三句话必填的 `throw` 到 `main.ts` 整条链**无 try/catch**，将来忘填会白屏

## 8. 签收（第一步 + 第二步，第二步的 Phase D 见 §13）
- [x] 通过代码审计（3 阻塞 + 5 重要已修，2 项如实登记）
- [x] 通过工程审计
- [x] 主计划已据此更新（含变更记录）

---

## 9. 第一步落地记录（`host` 维度，2026-07-29）

### 收益是当下就能看见的错，不是"为模型而模型"

`cc-bus` 三条 touches 原先在 `LocalHomeRelative` 下被当**本机路径**去 stat。而：
生产平台是 Windows（`ci.yml`/`release.yml` 打包 job 都是 `windows-latest`）·
cc-bus 跑在 **Claude Code 所在的那台**（`hooks_diag` 为此有**两条** IPC）·
`cc_bus::read_cc_bus_state(origin)` 读 `~/.cc-bus/` 是**按 origin 远端 exec** 的。
→ Windows 用户打开「配置面审计」，那三行显示 **「不存在」**，
而同一个 app 的驾驶舱正从远端把 inbox 读得好好的。**T02 专门要防的假警报，就在那一页上。**

### 又查出一处我凭印象写错的（第三处同族）

`~/.claude-accts/` 我第一版标了 `Client`。查证 `accounts.rs`：**全部入口都是
`list_remote_accounts(origin)` / `list_remote_session_accounts(origin)`，走 `ssh_source` 远端 exec**
——账号库在**远端**。而我在 T02 的 `config_surface.rs` 注释里还写过
"`~/.claude-accts/`（本机账号库，账号页真的在读它）"，那句也是错的。两处已一并更正。

**这是本轮第三次「以为是本机、其实是远端」**（前两次：T03 阻塞 3 的远端 basename 猜测、
T02 的 `~/.cc-bus/`）。同一个认知偏差反复出现，正说明 `host` 这个字段该有——
**它把一个我一直在犯的错变成了必须逐条声明的东西。**

### `Either` 是这次真正的新语义

`HostScope::Either` 下，**「本机没找到」判 `Undetermined` 而不是 `Absent`**，
理由写明"这套东西装在 Claude Code 跑的那台上，很可能是某个远端；远端状态到 cc-bus 页按连接查"。
本机**真找到了**仍然确定地说存在——它不是"永远说不知道"。

### 顺序纠错：destination 全量校验 → host 投影

第一版写成 host 优先短路，于是 `LocalHomeRelative` 那两条校验
（必须 `~/` 开头、glob 只许在最后一段且只许一个 `*`）对所有 `Remote`/`Either` 的 touches
**完全不再执行**——而 10 条 touches 里 **7 条**是这两种 host。改成
`resolve_by_destination(...)` 全量校验后再 `project_onto_host(...)`。

**并更正我自己在注释里说过头的一句**：我写"`UserConfiguredPath` 的占位符校验变成死代码"
——不对。那条 `Err` 分支在更早一步就已改成"不是占位符就按本机路径解析"，本来就没有可被跳过的校验。
真正被短路掉的是上面那两条。测试也据此改成 `destination_checks_still_run_under_every_host`
（四个 host × 三种坏路径 = 12 组）。

`host` 投影**只改写本机可解析的那两种结果**：`NeedsUserConfig { what }` 比 `Remote`
信息更多（它告诉用户去哪儿看那个值），覆盖掉是降级 → 有 `host_projection_preserves_the_richer_resolution` 钉着。

### 变异验证（三条，全部先 diff 确认落位）

| 变异 | 结果 |
|---|---|
| cc-bus 三条 `host` 改回 `Client`（退回那个假警报） | 红 `Either 没有任何使用者，那它不该存在` |
| 删掉 `host` 投影里挡住 Remote 本机路径那一支 | 红 `cc-acct-iso/"~/.claude-accts/" 声明在远端，却解析出本机路径` |
| `Either` 未命中改判 `Absent` | 红 `本机没找到不等于不存在，不许判 Absent` |

### 跨字段一致性守卫**不是**同义反复

`remote_host_never_resolves_to_a_local_path` 断言 `host == Remote` ⇔ 解析不出本机路径。
它与 T02 那条被删的"钉子"的区别：那条断言的两边其实**同一个来源**
（`Remote` 只由 `RemoteHomeRelative` 产生且必然产生）；这条的两边是**两个独立字段**
（`host` 与 `destination`），且解析路径是"先按 destination 校验、再按 host 投影"
——改任一边都会红，变异 B 就是证据。

### host 必须上屏

`SurfaceRow.host_label` + `.config-surface-host` 一行 + 诊断文本里也带上。
不上屏的话用户看 `$PROFILE` 与 `~/.local/bin/ccm` 分不出说的是哪台机器
——而这一页的全部价值是可信告知。有 DOM 断言钉着（T02/T03 两次教训）。

### 本轮门禁

cargo test **512**（+7）· cargo fmt 0 · clippy 0 error · tsc 0 · npm test **804** ·
shellcheck 0 · exec-bit guard 过 · **七套真机套件全绿**（26/44/12/15/13/14/11，
行为等价：一条断言都没改）。
tmux 走强制 `-L` 的 shim + canary 双向自检，跑完默认 socket 三个会话逐字未变。

### 下一步（T04 第二步，本轮**没做**）

五套机制的**装/升/卸**真正走注册表。先有 `host` 才不会写出一个在 Windows 上说假话的部署器。
**T03 那条纪律要贯彻**：远端的"某个路径在不在"必须由远端自己回报——
`hooks_diag` 的 `X`/`P` 标记协议是这条的第一个落地样例，第二步按它办。

---

## 10. 第一步的审计闭环（Phase D，2026-07-29）

独立对抗性 agent，60 次工具调用 / 约 19 分钟，收工时工作区干净、512/804 基线核实为真。
它把我的门禁数字全部实跑核过（`cargo test --lib` 512、`vitest` 804、`fmt --check` 0、`tsc` 0），
并**逐条复现了我自称的三条变异，一条不虚**。三条阻塞全部独立复现后才动手。

### 阻塞 1（已修）：`Either` + glob 把「12 项匹配」变成一句假话

`EitherHost { local: PathBuf }` 把 glob 拍成 `dir.join("cc-*")`，`observe` 于是去 stat
一个**字面含 `*` 的文件名**，计数分支彻底走不到。实测：`ls -d ~/.local/bin/'cc-*'`
→ `No such file or directory`，而那个目录下**真有 12 条 `cc-*`**。
那一行从 T04 之前正确的「12 项匹配」退化成「未确定 —— 本机 …/cc-* 不存在」
——**`why` 里陈述了一个假事实**。这正是本模块文档禁止的"对能用的安装报假警报"，
只是从红叉降级成了**带谎话的灰字**。

两条现有测试都盖不住：`either_host_never_says_absent` 用 `empty_probe`，
**bug 的输出恰好就是它想要的 `Undetermined`**（断言与 bug 撞了同一个答案）；
另一条用手搓的 `EitherHost { local: "/h/.cc-bus" }`，不是 glob。

→ 改成 `EitherHost(Box<PathResolution>)`：**包住内层解析**。glob 计数、目录列举、
"列不出来"那档全部自动继承，而「Either 绝不说 Absent」变成一句话可证
（只把内层 `Absent` 改写，其余透传）。新增 `either_host_keeps_the_glob_count`
（12 条匹配必须报「本机存在（12 项匹配）」），并给 `either_host_never_says_absent` 换成**双探针**
（全空 + 目录列得出但零匹配）。

**第二个探针一加就红**，暴露了另一件事：内层本来就"不确定"（目录列不出来）时，
原先 `other => other` 把 `Either` 的提示整个吞了。改成**追加**理由——两件事都要说：
本机为什么查不了 + 它也可能根本不在本机。

### 阻塞 2（已修）：`~/.cc-bus/` 标 `Either` 是错的，且换来一个新的假阳性

`cc_bus.rs` 的全部 5 个 IPC（`read_cc_bus_state` / `check_cc_bus_agent_online` /
`read_cc_bus_inbox` / `cc_bus_send` / `cc_bus_spawn`）都以 `origin` 入参走 ssh 远端 exec，
**一条本机读取路径都没有**；驾驶舱的 origin 下拉来自 `list_remote_mcp_origins`，
连"本机"这一档都没有。而本机 `~/.cc-bus/` **真实存在**（开发机上就有）→
这一行会**确定地**说「本机存在（目录）」，配上 `IndirectWrite` 那句
"你在 cc-monitor 里的操作会让它被写"——**而我们写的是远端那个**。
把用户不关心的那台的目录，冒充成"我们会动的那个"。
**这正是我派单时担心的"用一个新的假阳性换掉旧的假阴性"，当下就能重现。** → 改 `Remote`。

对比：`~/.claude/settings.json` 与 `~/.local/bin/cc-*` 确实有两条真路径
（`diagnose_local_cc_bus_hooks` + `diagnose_remote_cc_bus_hooks`），那两条标 `Either` 是对的。
**cc-bus 三条里两条对、一条错。**

### 阻塞 3（已修）：「改任一边都红」是假的——我上次犯的那个错今天改回去仍然全绿

审计实测：把 `~/.claude-accts/` 的 `host` 改回 **`Client`**（**正是这个 commit 声称刚更正的那条错值**）
→ `cargo test` **512 项全绿**。改 ccm 的 `~/.bashrc` 同样全绿。两条同时改才红
——说明真实 `checked = 5`，而阈值写 `>= 4`，**恰好容忍一次静默降级**。

我 commit 里写的「跨字段守卫不是同义反复…改任一边都红，变异 B 就是证据」**不成立**：
变异 B 改的是 `project_onto_host` 的**代码**，证明的是代码承重，不是两个字段相互约束。
T04 的中心论据是「host 把我一直在犯的错变成了必须逐条声明的东西」——
**声明是有了，门禁没有。**

→ 新增 `every_host_declaration_is_pinned`：逐条钉死 `(tool_id, path) → host` 十条表。
改任何一条都红，报错直接指出改了哪条。**改 `TOOLS` 就必须来改这张表**——
这是有意的摩擦：host 判错过三次（T02 的 `~/.cc-bus/`、T03 的 basename 猜远端、
T04 的 `~/.claude-accts/` **连错两版**），让它必须被显式确认一次。
两个阈值同时改成等号（`checked == 5`、`labels.len() == 4`）。

### 重要 1（已修）+ 重要 2（已修）：`host` 曾经是 `destination` 的纯函数

审计核实第一版是一张 **1:1 表**（`RemoteHomeRelative→Remote`、`LocalHomeRelative→Either`、
`UserConfiguredPath→Remote`、`ProjectRelative→ProjectDir`、`UserShellProfile→Client`），
所以那条"跨字段"断言在真实 `TOOLS` 上**完全可由 destination 推出**——与 T02 被删的那颗钉子同一类。

**审计给出了保留 `host` 最硬的理由，比我 commit 里给的硬**：把两条标错的改对，它才真正独立。
改完之后 `LocalHomeRelative` 同时映到 `Either`（settings.json / cc-*）与 `Remote`（~/.cc-bus/），
`UserConfiguredPath` 同时映到 `Remote`（两个占位符）与 `Either`（~/.claude-accts/）。
→ 新增 `host_is_not_a_function_of_destination`，把这句话变成门禁：
**哪天它退回 1:1，说明 host 退化成冗余标签，那时该删掉这个字段而不是留着装样子。**

**重要 2 是 `~/.claude-accts/` 我连错两版**：第一版 `Client`（错，账号库列举全走 ssh）、
第二版 `Remote`（**也不对**）——本机 `CLAUDE_CONFIG_DIR` 会指进这个目录
（这台就是 `~/.claude-accts/z`），`hooks_diag::claude_config_dir` 与 `config_surface` 自己都在读它，
`ConfigSurfaceReport.claude_config_dir` 更是直接打印它。于是同一页**自相矛盾**：
顶部写着解析基准是 `/home/zbl/.claude-accts/z`，而那一行写着「位置：远端」。→ 改 `Either`。

### 重要 6（已修）：我的数字夸大了一倍

我三处都写"10 条 touches 里 7 条受影响"——是 **8** 条（`Remote` 5 + `Either` 3）。
而且那两条校验只在 `resolve_local_home` 里，8 条中有 4 条本来就不经过它
——**实际被短路掉的是 4 条**。修复是对的，描述夸大了一倍。

### 重要 8（已修）：那行灰字 9/10 是冗余的

审计核实真实是 **10 行**（不是我说的 12），每行 5-6 行文字。而「位置」旁边的「现状」
早就写着「远端路径（…）——本页不连 SSH」/「装在 Claude Code 跑的那台上」/「相对项目目录」
——**唯一真正新增信息的是 Windows 上的 `$PROFILE` 那行**。
所以我 commit 里"用户看 `$PROFILE` 与 `~/.local/bin/ccm` 分不出哪台"只有一半成立。
→ 收成**路径行上的一个短徽章**（本机 / 远端 / 本机或远端 / 项目目录），信息保留、省掉 10 行灰字。

### 重要 7（已修）：诊断文本里的位置零覆盖 + 可选链吞掉诊断

审计实测删掉 `formatReportText` 的 `位置:` 一行推送 → 16 项全绿。
DOM 那条有牙，但报错是 `undefined 和 string 的组合无效`——`?.textContent` 把诊断吞了。
→ 补 `formatReportText` 断言；DOM 断言改成先 `expect(...).not.toBeNull()` 再看内容。

### 重要 3/4（如实登记，不改）

- `(Remote, LocalGlob)` **今天走不到**（唯一的 glob 是 `Either`）。留着不是装样子：
  它与上一行是同一条性质的两半，删掉半边会让"远端不许本机作答"在下一个 glob 型远端 touch
  出现时**静默失守**。`(Client, *)` / `(ProjectDir, *)` 全落 `(_, other)`——
  这两个变体今天**纯粹是标签**，价值在 `host_label` 上屏那一侧。
- **同一文件里两把尺子**（`all_host_scopes_are_really_used` 用 ≥1、
  `user_configured_destinations_…` 用 ≥2）。不统一，但**写明理由**：
  描述数据的 enum 允许变体单用户（T01 对 `ToolSource` 已论证），
  而那条 ≥2 守的是"这个我新造的变体值不值得存在"。两把尺子各有其位，并存就该写明。

### 反验证（三条，全部先 diff 确认落位）

| 变异 | 结果 |
|---|---|
| `~/.claude-accts/` 的 host 改回 `Client`（审计原手法） | 红 `host 声明与钉死的表不一致` |
| `~/.cc-bus/` 改回 `Either`（让 destination→host 退回 1:1） | **3 红**，含 `host 就是 destination 的函数…实得 [("UserConfiguredPath", 2)]` |
| `EitherHost` 退回自存 `PathBuf`（丢 glob 计数） | 红 `有 12 条匹配却报 Undetermined{…本机没找到…}——这正是阻塞 1 的形态` |

过程记录：中间那条变异第一次**锚点没对上、没写进文件**，输出的"28 passed"是未变异结果。
当场识别并用 `str.index` 定位重做——**先 diff 确认改动行再判色**这条纪律本会话第三次救场。

### 审计自己声明未验证的

七套真机套件（它明确禁跑 tmux）、clippy、shellcheck、exec-bit guard、Windows 实机渲染。
它还指出一件对的事：`git show --stat` 里没有 e2e 文件**与"行为等价"自洽，但不构成它们被跑过的证据**。
这几项是我自己跑的（见下），本轮我又核了一次 `git diff --stat HEAD -- e2e/` 为空。

### 我核实后认同审计的其余意见

三条自称变异一条不虚 · 顺序纠错有真牙（塞回 host 优先短路 → 双红）·
`NeedsUserConfig` 不被 `Remote` 覆盖的取舍对 · 另三条 host 声明查证无误
（`~/.bashrc = Remote`：`sftp.rs` 确写远端 profile，本机 PS profile 在 `profile_installer.rs` 用
`dirs::document_dir`，两者没混；`$PROFILE = Client`；`.mcp.json = ProjectDir` 且它是四个变体里
**唯一有两套真实现支撑**的）· `cc-*` note 的"计数偏大 1"也对。
**审计不主张用 ≥2 尺子否掉 `HostScope`**，理由与 T01 保留 `ToolSource` 一致——我同意。

### 本轮门禁

cargo test **515**（+3）· cargo fmt 0 · clippy 0 error · tsc 0 · npm test **804** ·
shellcheck 0 · exec-bit guard 过 · **七套真机套件全绿**（26/44/12/15/13/14/11）·
`git diff --stat HEAD -- e2e/` **为空**（行为等价的直接证据，不只是"没改 e2e"这句话）。
tmux 走强制 `-L` 的 shim + canary 双向自检，跑完默认 socket 三个会话逐字未变。

## 5. 代码审计结果（Phase D）
见 §10。3 阻塞 + 5 重要已修，2 项如实登记。

## 6. 工程审计结果（Phase E）
- **主计划自洽。** `host` 现在真正独立于 `destination`，且有门禁盯着它别退化回去。
- **给第二步的硬约束**（比第一步写的更具体）：**"某个路径在不在"必须由那台机器自己回报。**
  `hooks_diag` 的 `X`/`P` 标记协议是第一个样例；`Either` 的"本机命中才说存在、
  未命中只说不确定"是第二个。部署器按这两条办，别再让本机替远端作答。
- **`(Client,*)` / `(ProjectDir,*)` 今天纯是标签**——第二步若它们仍不改变任何行为，
  要重新论证是否该合并进 `Remote`/`Either`（现在留着的理由只是上屏措辞）。

---

## 11. 第二步：**计划 ≠ 现实 —— 统一部署器不该建**（2026-07-29）

计划 §2「不做」那一栏原写「五套机制的**装/升/卸**真正走注册表 = T04 第二步」。
按铁律 4 先数清真实共同结构，**结论是那个抽象不该建**，记录在此而不是默默照做：

| 范式 | 真实使用者 | 现状 |
|---|---|---|
| 指纹判过期 → 装/升/跳过 | daemon（`sftp.rs:274,390`）+ cc-acct-iso（`acct_iso_deploy.rs:131`）= **2** | **早就共享**：`sftp::deploy_decision` |
| 备份 → 写 → 读回比对 → 回滚 | **5 处** | **早就共享**：`verified_write::verify_readback`（T01 建的） |
| 围栏块插入/替换/剥离 | ccm 远端 profile + PowerShell 本机 profile = **2** | **两套独立实现** ← 本轮做这个 |
| 整份 JSON 覆写 | 项目 MCP = **1** | 单例，不抽 |

**三个范式里两个早就共享了，第四个只有一个使用者。** 再套一层 `install(tool_id, ctx)`
分派器只会把五件形状不同的事装进一个盒子——正是本工作区已九次拒绝的形状。
真正剩下的重复只有围栏块这一族。

## 12. 而围栏块这一族藏着一个**会吃掉用户内容**的 bug

两侧对「有 BEGIN 但其后找不到配对的 END」（上次安装中断 / 用户手改坏）处置**不一致**：

- **远端**（`sftp::merge_profile_block`）：**Err 中止**。F10 审计 B1 专门加的，原话
  「绝不用独立 `find` 误配前面的 END 而吞掉用户内容；宁可报错让用户手修，也不破坏文件」。
- **本机**（`profile_installer::find_block_range`）：返回 `None` → 走**追加**分支。

本机那条的后果我写了复现测试、**跑出真实输出**：

```text
原始：   # my stuff
         # === cc-monitor BEGIN v1 ===      ← 损坏（无 END）
         function cc { }                     ← 用户自己的代码

装一次： # my stuff / …BEGIN… / function cc { } / (空行) / …BEGIN… / NEW / …END…
         ← 追加了第二个块，用户代码还在

装两次： # my stuff / …BEGIN… / NEW / …END…
         ← **function cc { } 没了**
```

第二次安装时，**损坏的那个 BEGIN 与新块的 END 配上了对**，两者之间的东西被整段替换。
写的是用户的 PowerShell `$PROFILE`，和远端 `.bashrc` 同性质——写坏了下次开终端就炸。
**远端侧当初被要求防的正是这一幕，本机侧漏了。**

（过程记录：我第一版复现测试断言的是"块会累积"，跑出来 `BEGIN 计数=1` 判错方向——
真实后果比我猜的严重。**断言打在错的地方会让人以为没事**，改成断言用户内容才看清。）

### 修法：抽出配对判定，取两者中最强的那一档

新增 `fenced_block::find_pair(text, begin, end, what) -> Result<Option<(usize,usize)>, String>`，
**2 个生产消费者**：
- `profile_installer::find_block_range`（本机）— 连带 `replace_or_append_block` 与
  `strip_block` 都改成 `Result`；**卸载路径也走同一判定**（原先围栏损坏时"当作没有块、
  原样返回"，看着无害，实则让用户以为卸载干净了，而那个悬空 BEGIN 还在，
  下次安装就吃掉它下面的内容）。
- `sftp::merge_profile_block`（远端）— 判定本身没变（它一直是对的），
  但改走同一函数后**两侧不可能再漂移**。

这与 T01 对 `verified_write` 的做法同型：四处实现里本机侧只比长度，统一到内容级比对。
**取最强那一档，不是取交集。**

### 变异验证

把共用判定退回「有 BEGIN 无 END 就当没有」→ **4 条红，横跨三个模块**：
`fenced_block::begin_without_end_is_an_error_not_an_append` ·
`fenced_block::end_before_begin_does_not_pair` ·
`profile_installer::damaged_fence_aborts_instead_of_eating_user_content` ·
`sftp::merge_profile_block_aborts_on_orphan_begin`
——**跨模块同时红，正是"两侧真的共用"的证据**。

（过程记录：第一次变异写成了编译错误，`grep FAILED` 得 0。**编译失败不等于测试有牙**，
当场识别并重做成干净变异。这条加进纪律。）

### 第二步**没做**的、如实登记

- **T01 的 P6（远端写入路径缺失败注入测试）与「ccm CLI 写坏不回滚」仍未收。**
  它们要给 `sftp.rs` 的三处 async 写入注入可失败的 SFTP 桩，那是一套 fixture 工程，
  不是本轮"抽一条判定"能顺手带的。**不假称已收。**
- `(Client,*)` / `(ProjectDir,*)` 仍纯是标签（Phase E 第二条问的那个）——本轮没有引入
  依赖它们的行为，所以**问题原样保留**：若 T06/T07 结束时它们仍不改变任何行为，
  该重新论证是否合并进 `Remote`/`Either`。

### 本轮门禁

cargo test **522**（+7）· cargo fmt 0 · clippy 0 error · tsc 0 · npm test **804** ·
shellcheck 0 · exec-bit guard 过 · **七套真机套件全绿**（26/44/12/15/13/14/11）·
`git diff --stat HEAD -- e2e/` **0 行**（行为等价的直接证据）。
tmux 走强制 `-L` 的 shim + canary 双向自检，跑完默认 socket 三个会话逐字未变。

---

## 13. 第二步的审计闭环（Phase D，2026-07-29）

审计 47 次工具调用 / 13 分钟，工作区还原干净，**门禁数字全部实跑核过**，并**逐条复现了我自称的三条变异，一条不虚**。

### 阻塞（已修）：我自己造的漂移——远端卸载会谎报成功

`sftp::strip_profile_block` 没迁移：悬空 BEGIN 时 `return existing.to_string()` →
调用方判 `stripped == existing` → 打印「远端 {profile} 里没有 ccm 块，无需卸载」。
**那正是我在同一个 commit 里定义为 bug 的形态**，而且比本机那边更糟——它主动告诉用户没问题。

更要紧：**这是我新造的漂移**。`af21ffb~1` 时两侧卸载都"原样返回"（一致）；
`af21ffb` 之后本机 Err、远端静默 no-op（不一致）。我 commit 里那句
**「两侧不可能再漂移」只对 install 半边成立，对 uninstall 半边方向相反**。
→ 迁进 `find_pair`（第 3 个消费者），现在两侧的装与卸**四条路**全走同一判定。
**并且原来那条测试 `strip_noop_on_malformed_begin_without_end` 把 bug 编码进去了**
（断言 no-op），已改成断言中止。

### ①（已修，审计说比我抽的判定影响面更大——同意）两条 deploy 路径零读回

`deploy_remote_daemon` + `deploy_remote_acct_iso` 的**全部** `upload_atomic`
——1 个 daemon 可执行二进制 + 6 个远端脚本（含 0755 的 `cc-acct-iso`/`lib.sh`/install.sh）
——写完**直接写版本标记**，中间没有任何读回（`upload_atomic` 只做 flush/shutdown/rename，实测 `grep -c` = 0）。

**我那套"五套机制"框架恰好把这个洞盖住了**：我论证「备份→写→读回比对→回滚这个范式已共享（5 处），
所以不用抽」——而**那 5 处全在 profile/CLI 那条线上，压根没覆盖这两条 deploy 路**。
把"范式已共享"当成了"范式已覆盖"。

后果具体：传输损坏的 daemon 二进制照样被写上正确的 `.build_id` → 下次 `deploy_decision`
判「已是最新，跳过」→ **坏二进制永久驻留**，而用户看到的是部署成功。
→ 新增 `upload_atomic_verified`（上传 + 读回**逐字节**比对）+ 纯函数判据 `verify_uploaded_bytes`。
按字节而非字符串：daemon 是可执行文件，`String::from_utf8` 会失败——判据同源、载体不同，
所以没直接复用 `verified_write::verify_readback`。**标记仍走裸上传：它是"校验通过"的凭证，必须最后写。**

**过程记录：我第一次批量替换扩了范围**——把 ccm helper 那条**故意**用裸上传的路
（下游紧接着自己的读回+回滚）和 best-effort 回滚也改了，9 处越界，已全部回退。
只留真正零读回的 8 处（2 daemon + 6 acct-iso）。
**结构性守卫也因此收窄**：第一版写成"全文件不许裸 `upload_atomic`"，当场被自己抓红
——**守卫范围比性质宽 = 假红 = 会被人关掉**。现在只覆盖那两个 deploy 函数体，带计数自检（7）。

### ②（已修）远端 3 个边界语义变了，我原话"判定没变"被证伪

审计把旧 byte-find 实现逐字复制成 `old_merge` 并列对拍，9 个边界里 **3 个变了**：

| 边界 | 旧 | 新 |
|---|---|---|
| 行内 marker（`echo "…BEGIN…"`） | **切断该行 + 吃掉第二个 echo 行** | 不命中 → 追加 |
| BEGIN 与 END 同一行 | 能正确替换该行 | **直接 Err（退化）** |
| 缩进 marker | 保留 BEGIN 缩进、丢 END 缩进（不自洽） | 归一到列 0 |

第一条意味着**我未申报就修掉了远端一个数据丢失**——比我声称的更有价值，但也证明"判定没变"是错的。
→ 三条全部补测试锁死（`remote_merge_boundary_semantics_after_migration`），
文档如实改成"判定变了"并逐条列出，**包括那条退化**。

### ③（已修）`find_block_version` 缺 `trim_start`，口径不一致被提升为用户可见矛盾

缩进的悬空 BEGIN → `has_ccm_block=false` → `cc_integration.ts:453` **隐藏卸载按钮**、
UI 说"未安装"，而点安装却 Err 报行号。→ 加 `trim_start`，与 `find_pair` 同口径。

### ⑤（已修）我确实漏了第 5 个共性

`is_safe_remote_daemon_path` 与 `is_safe_remote_acct_iso_dir` **5 个条件里 4 个逐字相同**，
只差标记词，**2 个消费者未共享——正好达到我为 `find_pair` 立的那条 ≥2 门槛**。
→ 抽成 `sftp::is_safe_remote_managed_path(path, markers)`，测试钉住"标记词是必需条件"
（那是防误删的关键：把"这是 cc-monitor 管的目录"变成路径本身的性质）。
**但审计也确认这不支持建统一部署器**——它自己数了 7 个入口逐个列步骤序列后认同该结论。

### ⑥⑦（已修）文案

`what` 从类别名改成**真路径**（两侧调用方手里一直有它；原测试注释写"要说清是哪个文件"
而断言的是类别名，断言与注释不符）· 去掉用户可见 toast 里的字面 `**`（前端纯文本渲染），
并加断言禁止它回来。

### ④ 部分做（如实登记）

修正我 commit 里「4 条独立」的暗示：实际是 **1 个语义性质 + 2 个接线见证 + 1 个本变异下冗余**
（`merge_profile_block_aborts_on_orphan_begin` 在 `af21ffb~1` 就存在、不是本轮新增）。
"跨模块同时红证明两侧真共用"这半句**站得住**（接线证明就该那样），但别暗示 4 条独立。
**审计要的"真文件级损坏围栏测试"本轮没做**——`install_to_profile` 仍只有纯函数测试。
要写它得起临时目录真写盘，而红线是"部署器测试一律注入闭包、绝不真写盘"，
需要先给它做一层可注入的 fs。**如实登记，不假称已做。**

### ⑧ 登记不改

卸载缺"强制清理 cc-monitor 块"入口。审计核实**用户有出路**（错误文案带行号 +
面板有"打开 profile"按钮），③ 修掉之后缩进场景的按钮隐藏也没了。判定：可等 T07。

### 反验证（两条，先 diff 确认落位且确认编译得过）

| 变异 | 结果 |
|---|---|
| 远端 strip 退回悬空 BEGIN 时 no-op（阻塞形态） | 红 `strip_aborts_on_malformed_begin_without_end` |
| acct-iso 一处 verified 退回裸 `upload_atomic` | 红 `deploy_remote_acct_iso: 内容上传仍走裸 upload_atomic —— …lib.sh…` |

### 本轮门禁

cargo test **528**（+6）· cargo fmt 0 · clippy 0 error · tsc 0 · npm test **804** ·
shellcheck 0 · exec-bit guard 过 · **七套真机套件全绿**（26/44/12/15/13/14/11）·
`git diff --stat HEAD -- e2e/` **0 行**。
tmux 走强制 `-L` 的 shim + canary 双向自检，跑完默认 socket 三个会话逐字未变。
