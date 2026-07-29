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

## 8. 签收
- [ ] 通过代码审计（无阻塞项）
- [ ] 通过工程审计
- [ ] 主计划已据此更新（含变更记录）

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
