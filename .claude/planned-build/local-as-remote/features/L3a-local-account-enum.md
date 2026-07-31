# L3a — 本机账号枚举（只读）**已交付**

> 主计划：`../MASTERPLAN.md` §1 L3a（**P1**）· §0.2 · §3 账本 · §4 顺序表第 5 位
> 前置：L2（`949e98f`）· L5 对账表（`5e85959`）

## 1. 开工复测：三条断言的结果

| 计划写的 | 复测 |
|---|---|
| manifest 格式「已定」 | ✅ **属实**，而且比计划说的更好：daemon 侧 `accounts_query.rs` 有一份**完整的 Rust 读取器**（1383 行，直接读文件系统），格式、账号 0 规则、安全判据全都现成 |
| 账号 0 语义（空值 ≠ 未设） | ✅ **已在类型上钉死**：`RemoteAccount.config_dir: Option<String>`，注释自陈「`Some("")` 是非法拼法」。本模块照此实现，并有专测 |
| 「本地立刻有账号列表 / 选号 / **账号注入** / per-account model」 | ⚠ **越界**：注入与 per-account model 要改启动路径与前端。本轮**只做只读枚举那半**（§6 登记） |

## 2. ★ 最要紧的一条：**复用不了，只能做第三份实现**

第一反应是复用 daemon 的 `accounts_query.rs`。**做不到，理由是结构性的**：

- 那个 crate 是 **bin-only**（无 `[lib]`），`Cargo.toml` 注释写明**刻意不进 workspace**：
  「a workspace would pull this Linux-only daemon into the Windows CI `cargo test --all`
  and break the build」。
- monitor **必须在 Windows 上构建**。让它依赖一个 Linux-only crate，正是那条注释在防的事。

⇒ 这份数据于是有 **三个读者**：`cc-acct-iso`（bash，写侧）· daemon（远端读）· 本模块（本地读）。

**这正是 L2 上一轮刚在防的「平行世界」**，而这次躲不掉。处置照本仓既有纪律 ——
**双写点必须有守卫**（同 `TMUX_LS_FMT` / 观测取值 / Z06 凭据文件名那几条）：
`contract_matches_the_daemon_implementation` **读 daemon 的源文件**，钉住四条契约
（账号库目录名 · manifest 文件名 · 凭据文件名 · schema 版本）。

## 3. ★ 与 daemon 那份**故意不同**的一处：路径绝对性

daemon 的 `is_safe_config_dir` 第一条是 `p.starts_with('/')`。**照抄会让每个 Windows 账号
都被判不安全、列表恒空** —— 那条在 daemon 里对（它只跑 Linux），在 monitor 里不对。

拆开看这个判据在**防什么**：

| 组成 | 性质 | 处置 |
|---|---|---|
| shell 元字符 + 视觉欺骗字符（零宽 / 双向覆盖 / 异常空白） | **平台无关的安全性质** | **逐字照搬** |
| 「是绝对路径」 | **平台相关的形式** | 各写各的（POSIX `/` · Windows 盘符 / UNC） |

**判据落在性质上，不落在表面特征上** —— 照抄 `starts_with('/')` 是抄了形式、丢了性质。
另：反斜杠**不进**元字符黑名单（Windows 路径分隔符就是它；本模块产出的值不进任何 shell，
本地注入走 env、不拼命令）。

## 4. 交付

`src-tauri/src/local_accounts.rs` + Tauri 命令 `list_local_accounts` + 前端
`accounts.ts::fetchLocalAccounts`。

**输出类型直接复用 `accounts.rs` 的 `AccountsResult`/`AccountsMeta`/`RemoteAccount`**
⇒ 前端拿到的形状与远端**逐字段相同**。那就是 §40 在这一格上的意思，也让上层不必分叉。

几处刻意的行为对齐（与远端同义）：

- **「本机没启用多账号」是正常状态**，回 `available:true` + `meta.enabled:false` + 人话原因，
  **不是** `Err`（`Err` 只留给「取不到 HOME」）。
- **单条坏不拖垮整表**（与 `cc-acct-iso` 写侧「丢单条」策略一致）。
- **账号 0**：`configDir` 键缺席 ⇒ 对外出 `None`（不是空串）· 恒 `exists`（「裸起」永远可达）·
  登录态查 `sharedStore`。**判据是结构性的（键在不在），不认名字。**
- **`logged_in` 只 stat 存在性、绝不读内容**；探不到目录 ⇒ `false`（那是「不知道」，不假装已登录）。
- `account_zero_aware` 恒 `true` —— 本实现原生认得账号 0，不像旧 daemon 要降级提示。

### 4.1 对账表：真的补齐了一条

`accounts.list` 从 `ParityDebt` 变成**对称**（远端 `list_remote_accounts` + 本地
`list_local_accounts`，同样只读、同样的输出类型）⇒ 理由表里那条被删，形状钉死数
`121 / 50 / **20** / 7·**11**·2`、`checked 67`。

**只解一半的没动**：`accounts.session-accounts` / `accounts.trust` / `usage.per-account` /
`acct-iso.*` 仍是 ParityDebt —— L3a 只做了枚举。

## 5. ★ 变异验收：一条「本该红却没红」，当场把守卫修了

| 变异 | 结果 |
|---|---|
| **A** 让空串 `configDir` 通过（空值 = 未设） | **成立**：红 2 条（账号 0 专测 + 安全判据测），报「空值 ≠ 未设」 |
| **B** 账号 0 的 `exists` 判成 false | **成立且隔离**：只红账号 0 专测 |
| **C**（跨 crate）daemon 侧改凭据文件名 | **初版没红 ⇒ 守卫是安慰剂** |
| **C-重打** 改 daemon **生产段**的字面量 | **成立**：契约守卫精确报「凭据文件名…在 daemon 侧找不到」 |
| **C-对照** 只改 daemon **测试段**的同名常量 | **正确地不红** —— 那不是契约 |
| **D** daemon 生产段改 manifest 文件名 | **成立** |

**C 的第一版为什么是安慰剂**：我用 `contains` 扫**整个** daemon 文件（含注释、含 `cfg(test)`），
于是改掉测试模块里的一个同名常量后，生产字面量还在，守卫照样绿。

**当场修**（判色规则⑧）：只扫**生产段**（剥 `#[cfg(test)]` 之后）+ **剥行注释**，
并加一条「剥完必须比原文短且仍够长」的反向自检。修完之后 C-重打红、C-对照绿 ——
**两个方向都对了才算数**。

**如实标注**：铁律 8 要求并行多 agent 审计；本会话常驻指令「除非用户要求不开 agent」
⇒ 主线程变异 + 全门禁代替。**这是欠账，不是强度裁剪。**

## 6. 门禁与红线

| 门禁 | 前 | 后 |
|---|---|---|
| monitor `cargo test --all` | 632 | **638**（+6） |
| monitor clippy lib / fmt | 36 / clean | **同左** |
| Tauri 命令数 | 120 | **121**（`parity_ledger` 121/50/20/7·11·2 + `checked 67` · C04a 121；**包装层仍 110** —— 本地那条走 `accounts.ts` 直接 `invoke`，与远端同一写法） |
| npm / tsc / check:types | 872 · 0 · 67 | **同左** |
| daemon | 173 | **同左** |

**红线核对**：测试全部在 `std::env::temp_dir()` 造的沙盒里跑（`Drop` 清理）；
收尾实测用户真实 `~/.claude-accts/accounts.json` **mtime 07-26 17:41:02 / size 413 未变**、
z/b 的 `settings.json` 仍是两个指向 `~/.claude/settings.json` 的软链、`/tmp/l3a-*` 零残留。

## 7. 签收

- [x] 复测三条：manifest 格式属实 · 账号 0 语义已在类型上钉死 · **「注入」越界 ⇒ 只做只读那半**
- [x] **说清为什么只能做第三份实现**（bin-only + 刻意不进 workspace + monitor 要上 Windows），并**用守卫兜住**
- [x] **平台差异拆到性质层**：安全字符判据照搬，绝对路径判据各写各的
- [x] 输出类型与远端**逐字段相同**；前端 `fetchLocalAccounts` 与 `fetchAccounts` 同形
- [x] **对账表真的结清一条**（`accounts.list` 从欠账变对称），其余账号欠账**没顺手改绿**
- [x] 五组变异（含跨 crate 双向）；**一条「本该红却没红」已当场修守卫并复验两个方向**

## 8. 没做的（登记）

| # | 事项 | 为什么 |
|---|---|---|
| 1 | **本地账号注入**（起会话时设 `CLAUDE_CONFIG_DIR`） | 要改 `local_launch_choice`/`launch_local` + 前端选号 UI。主计划把它写进 L3a 那行，但它不是「枚举」⇒ **单独一步**，且验收要能起会话（撞红线） |
| 2 | **per-account model** | 同上，且依赖注入先落地 |
| 3 | **UI 没有入口** | 后端 + 前端取数函数都在了，但没有哪个面板去调 `fetchLocalAccounts` ⇒ 用户暂时看不到。**登记，别当成已交付** |
| 4 | `accounts.session-accounts` / `.trust` / `usage.per-account` / `acct-iso.*` 仍是欠账 | L3a 范围外；**只解一半就只标一半** |
