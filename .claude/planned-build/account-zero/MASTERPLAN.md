# 主计划 / MASTERPLAN — account-zero（把「基座」变成受管的「账号 0」）

> 所有功能宏观设计的**单一事实来源**。跨功能的任何决策以此为准。
> 每次修订都在末尾「§7 变更记录」追加一行。
>
> **状态：Phase A 已落盘，等用户审批。未动任何代码。**

---

## §0.0 当前事实（先读这一节）

> 这一节回答「所以现在的事实是什么」。放最前面是 Phase G 文档审阅的结论：
> 自省/推导记录被排在时间线上、而全篇没有一处回答当前事实，
> **读一半停下的人会拿走一个已被推翻的结论**。本工作区的推导过程在 §0.2，
> 这一节只放**已实测核实**的现状。

**布局（2026-07-29 在 aya 上实测）**

```
~/.claude/                     ← 共享库 SHARED_STORE，同时是账号 0 的凭据落点
  .credentials.json            ← 账号 0 的凭据【当前不存在：已全迁 V2】
  projects/ sessions/ history.jsonl CLAUDE.md plugins/ cache/ …   ← 真目录，大家共用
  .claude.json                 ← 【当前不存在，且按本计划它永远不该存在】

~/.claude.json                 ← 账号 0 的状态【在家目录，不在 .claude 里】
                                  当前 590 字节空骨架，mtime 2026-07-28 16:20

~/.claude-accts/               ← ACCTS_DIR，被 lib.sh:129 强制禁止位于 SHARED_STORE 内部
  accounts.json                ← manifest。当前：z（isDefault=true）、b
  z/  .credentials.json + .claude.json + policy-limits.json + backups/ 为真文件
      其余十余项 symlink 回 ~/.claude/
  b/  同构
```

**隔离 / 共享的确切划分**（`lib.sh:114-117` 的默认值）

```
ISOLATE_SET   = .credentials.json  .claude.json  backups  policy-limits.json  stats-cache.json
SHARE_EXCLUDE = accounts  *.bak  *.bak-*
LEGACY_HOME_ITEMS = .claude.json          （源目录 = $HOME，不是 ~/.claude）
其余 ~/.claude/* 一律 symlink 共享
```

- **隔离「你是谁」+「你的本机状态」，不隔离「你做过什么」。** 换号不丢历史、不用重配
  `CLAUDE.md`/插件，但额度与身份分开。
- **这个划分对账号 0 与 z/b 完全一致**，账号 0 不是「少隔离」也不是「多共享」——
  它只是把那 5 项放在了不同的物理位置。
- **账号 0 与 z/b 共享同一份物理 `projects/`**：z/b 是 symlink 指过来，账号 0 的 config dir
  就是 `~/.claude` 本身。所以三者的会话历史互相可见。

**当前的缺陷（本工作区要治的）**

| # | 事实 | 证据 |
|---|---|---|
| 1 | **账号 0 不在 manifest 里，是个幽灵**。往 `~/.claude` 写了凭据，cc-monitor 的账号列表看不到、用量查不到、会话归属不到 | `cmd_which` 在 `CLAUDE_CONFIG_DIR` 为空时 return 1（「裸起模式」）；`remote-launch.ts:74` 明写「用量探针需要显式 configDir（不支持基座/无账号场景）」 |
| 2 | **`verify` 把它定义成违规而不是状态** | `cc-acct-iso:349` `vfail "共享库里仍有 .credentials.json —— 全迁未完成,或有进程没设 CLAUDE_CONFIG_DIR 就起了 claude"` |
| 3 | **这件事已经在 aya 上发生过一次**。`inplace_any=0` 且 `~/.claude.json` 在迁移（07-26 17:39）之后出现（07-28 16:20，590 字节空骨架）⇒ `verify` 现在就会报 `vwarn`。凭据没写进去（登录没走完），**踩到边没踩满** | `cc-acct-iso:351-352` 那条 vwarn 的触发条件逐条成立 |
| 4 | **`LaunchAccount` 只有两个变体**，于是「用户显式选了基座」和「根本没选/没有当前账号」被压成同一个 `{kind:"base"}` | `src/launch-requests.ts:27-28`：`return configDir ? {kind:"account",…} : {kind:"base"}` |
| 5 | **因此 cc-monitor 对「不知道」发 `--base`**，而 `--base` = `unset CLAUDE_CONFIG_DIR` = 起账号 0；全迁后账号 0 没凭据 ⇒ 要求重新登录，而**一旦登录就产生第 1 条那个幽灵** | `shared/ccm:572,598`；`ACCOUNT_DIMENSION.cliFlags` 对 base 恒返回 `["--base"]` |
| 6 | **`--base` 同时关掉了 ccm 唯一正确的 per-machine 回退** | `shared/ccm:320` `elif [ "$use_base" != 1 ] && [ -z "${CLAUDE_CONFIG_DIR:-}" ]` → 落该机器 manifest 的默认号；`ccm:207` `--account 与 --base 互斥` |
| 7 | **屏幕上有一句假话** | `src/settings/remote-section.ts:892` 「不指定（用远端登录的**基座账号**，不注入 CLAUDE_CONFIG_DIR）」——全迁的机器上那儿没有账号 |
| 8 | **迁移不会自动设默认号** | `cc-acct-iso:150` `makedef=0`，只有显式 `--default` 才置 1 ⇒「有账号但无默认号」是不加 `--default` 迁移的**默认结果** |

**已核实的好消息（决定了本工作区范围比预估小）**

- **`share_items()` 会跳过 ISOLATE_SET**（`lib.sh:191-195`）⇒ 将来新建账号时，账号 0 的
  `.credentials.json`/`backups`/`policy-limits.json` **不会**被 symlink 进新账号。
  **「账号 0」这个模型已经被现有隔离集结构性支持了——隔离/共享逻辑一行都不用改。**
- **`list-accounts` 的 email 是现读的**（`cc-acct-iso:264` `live="$(claude_json_email "$c")"`，
  读不到才回落 manifest 静态值）⇒ 在某账号目录里登成另一个邮箱，UI 会自动显示新邮箱，
  不会 manifest 说一套实际另一套。
- **`shared/ccm` 本体一行都不用改。** 它的 `--base`（= unset）语义本来就对，
  `:320` 的 per-machine 回退也本来就对。要改的是 cc-monitor **什么时候**发 `--base`。

---

## §0.1 目标与范围

- **总体目标**：把「基座」从一个**不可见的空值**变成一个**登记在册、可见、可管的「账号 0」**。
  核心动机（用户原话）：「想要即使破了隔离也在我们的机制内。即万一用户还往 `$HOME/.claude.json`
  写账号我们也能知晓。」

- **设计原则（本工作区的立论）**：
  **一条守不住的不变量，不如一个能表达它的模型。**
  「共享库不含账号态」这条不变量 cc-monitor **守不住**（管不住终端里手敲的 `claude`），
  而它被违反时产生的偏偏是一个**不可见**的身份。所以：**吸收 > 检测 > 禁止**。
  代价是用一条可查的检查（`~/.claude/.claude.json` 出现 = 有人用显式路径起过账号 0）
  换掉那条不变量。**这是一笔交易，不是免费的**（见 §6 风险 1）。

- **账号 0 的定义（这条是全局约定，所有功能都按它实现）**：

  > **账号 0 ≡ 「不设 `CLAUDE_CONFIG_DIR`」这个状态本身。**
  > 凭据在 `~/.claude/.credentials.json`，状态在 `$HOME/.claude.json`。
  > 起它 = **什么都不设**（不是设成空串，不是设成 `~/.claude`）。

  **为什么不能给它一个 `configDir = ~/.claude`**（= cc-acct-iso 已有的 V1
  `--default-in-place` 模式，工具自己标「除非你清楚为何要它，否则请用默认(V2)」）：
  那样起它就有**两条路**（`CLAUDE_CONFIG_DIR=~/.claude` → 读 `~/.claude/.claude.json`；
  裸起 → 读 `$HOME/.claude.json`），**同一个账号两份状态**，就是 `cc-acct-iso:110` 那条
  `.claude.json 会分裂` 警告。定义成「不注入」则只有一条路、只有一份文件，分裂不存在。

- **★ 原生身份的「组成」必须只声明一处**（用户 2026-07-30 追加需求：「能不能把账号 0 定义为
  claude code 默认登录方式，到时候如果 claude code 换登录方式了换位置了我们可以方便迁移」）：

  > 账号 0 的**身份判据**（上面那条「不设 `CLAUDE_CONFIG_DIR`」）已经是内涵的、不用改。
  > 没被抽象的是**「Claude Code 原生把身份与本机状态放在哪些文件里」**这份知识。

  **今天它散在 6 处**（2026-07-30 实测）：

  | 处 | 编码了什么 |
  |---|---|
  | `vendor/…/lib.sh:114` | `ISOLATE_SET = .credentials.json .claude.json backups policy-limits.json stats-cache.json` |
  | `vendor/…/lib.sh:117` | `LEGACY_HOME_ITEMS = .claude.json`（源目录 `$HOME`，**不是** `~/.claude`） |
  | `cc-acct-iso:127/184/185/265` | `.credentials.json` 的 chmod / copy / `logged` 判定各写一遍 |
  | `src-tauri/src/mcp.rs:33-44` | `~/.claude.json` 的**三个**候选路径（防御 `CLAUDE_CONFIG_DIR` vs `$HOME` 变体） |
  | `src-tauri/src/accounts.rs:49` | `stat .credentials.json` 判 `loggedIn` |
  | `shared/ccm:314` | 注释「基座 `~/.claude/.credentials.json` 不再存在」 |

  ⇒ Claude Code 改了位置要**一处一处改**，而漏一处的表现是**静默错**
  （`loggedIn` 恒 false；或更坏：两个账号悄悄共用一份身份 ⇒ 你以为在用 z、实际烧 b 的额度）。

  **本需求的验收标准必须这么写**（这是一条重要的边界，别写成做不到的承诺）：

  > **换位置**这一类 ⇒ 一处改完 + 一次迁移。
  > **换机制**（如凭据搬进 OS keyring）⇒ **按目录切身份这条路本身就不成立**，任何抽象都救不了；
  > 此时要求 **100% 当场检测到并停下**，而不是静默共用身份。
  >
  > **所以这一层买到的是「快速失败」，不是「保证可迁」。**

- **范围内**：
  - **原生身份组成的单点声明**（Z06）+ 版本钉与漂移检测（Z07）+ 物理迁移能力（Z08）
  - cc-acct-iso 的 manifest 数据模型 + `list-accounts` / `verify` / `which` / `run` 对账号 0 的处理
  - vendored 副本 lockstep + `.vendor_id` 重算
  - cc-monitor：`LaunchAccount` 三态化 · 账号 0 成为显式可选项 · 「基座」一词从 UI 消失
  - 用量探针与按会话切号支持账号 0
  - 守卫：禁显式路径起账号 0 · `verify` 新增检查 · 删除账号 0 特判
  - 远端版本协商（老 cc-acct-iso 不认账号 0 时怎么降级）

- **范围外**：
  - **不改 `shared/ccm` 本体**（已核实：不需要改）
  - ~~**不改隔离/共享划分**~~ —— **2026-07-30 订正：这条被上面那个追加需求扩掉了。**
    原文的论据（「ISOLATE_SET 已经支持账号 0」）仍然成立，但它只回答了「账号 0 能不能用现有
    划分」，没回答「划分本身写在几处、改起来贵不贵」。Z06 要做的是让 `ISOLATE_SET` /
    `LEGACY_HOME_ITEMS` / `loggedIn` 判据**从声明派生**而不是各写一份 —— 划分的**内容**不变
    （零行为改动），变的是它的**来源**
  - 不走 V1 `--default-in-place`（理由见上）
  - 不动 daemon
  - 不做「让裸 `claude` 自动可用」——那是 rc 片段的事，见 Z05，独立可选

- **整体成功标准**：
  1. 在 aya 上往 `~/.claude` 写入凭据后，**cc-monitor 的账号列表里出现账号 0 且标 `loggedIn`**，
     `verify` 说「账号 0 已登录」而不是 `vfail`。
  2. UI 里搜不到「基座」这个词（`grep -rn "基座" src/ --include=*.ts` 仅剩注释/历史说明）。
  3. **不存在「未选账号」这个可启动状态**：要么账号 0，要么具名账号。
  4. 账号 0 的用量能查、会话能归属、能被按会话切号选中。
  5. `~/.claude/.claude.json` 一旦出现，`verify` 报得出来。
  6. **原生身份组成只声明一处**：上表那 6 处不再各自写死，`ISOLATE_SET` / `LEGACY_HOME_ITEMS` /
     `loggedIn` 判据从声明派生；有守卫扫源码钉住「不许绕过声明再写死一份」。
  7. **换位置这一类能一处改完**：改声明 → `cc-acct-iso migrate` 把 z/b 的物理布局搬过去 →
     `verify` 绿。**用变异验收**：把声明改成一个假位置，必须①`verify` 红 ②migrate 能搬回来。
  8. **换机制会被当场检测到**：声明绑 Claude Code 版本；实际布局与声明不符 ⇒ 报，
     **绝不静默**（这条销掉 BACKLOG **E37**）。
     **⚠ Z07 交付后如实限定**：「绝不静默」**已达成**（四条检测都会打印）；但
     **「把声明改成一个假位置 ⇒ `verify` 当场红」这句要限定**——假位置只会让 D4 报**提示**，
     真正的 FAIL 需要 secret 真的泄漏进共享库（D1b）。原因：声明里好几项是 Claude Code
     **懒创建**的（`policy-limits.json` 撞限速才出现），**「还没被创建」与「改了位置」
     在这个信号层面不可判定**；判致命会让几乎每台干净机器都红（初版写成 vfail，沙盒当场
     误报 4 条）。

---

## §0.2 推导链（为什么是这个方案，不是别的）

> 本节是过程记录，**当前事实看 §0.0**。保留是因为这条链走了四轮、推翻过两次，
> 后来人不看会重走一遍。

1. **起点**：Phase G 整体设计视角审阅报了一条阻塞「两个渲染器对 base 不等价」。
2. **第一次推翻**（我说「产品语义决定」）：错。摊开六格发现 4 格强制基座、2 格继承，
   而那 2 格恰好是「没装 ccm + 非 send-into」。
3. **第二次推翻**（我说「4:2，仓里早选定强制基座，只是漏落两格」）：也错。
   数格子数对了，但**没问「基座在全迁之后还是不是一个能起 claude 的地方」**。答案是不是。
   所以那 4 格不是「已经做对的多数」，是同一个错误铺得更广。
4. **审阅也错了一半**：它说兜底渲染器「漏了 unset」。实际兜底路径（继承）**两种机器都对**
   （迁移过+贴了 rc 片段 → 继承到默认号；没迁移 → 落 `~/.claude`）。
   是 CLI 路径发 `--base` 才引入问题。
5. **根因**：`LaunchAccount` 两变体，把「我不知道」和「我要基座」压成一个类型（§0.0 缺陷 4）。
   **这跟本轮修的两条阻塞是同一个形状**——`read_optional` 的 `Option<Vec<u8>>` 把
   「读失败」和「文件是空的」压成同一个 `None`；远端 `classify_command` 用 basename
   回答精确路径问题使 `PathMissing` 永不可达。**一个类型承担两个语义，其中一个是「我不知道」。**
6. **用户的转向**（这一步改变了方案）：动机不是「让裸起可用」，而是**containment**——
   cc-monitor 管不住终端，所以别把破坏隔离定义成违规，要让它落在模型内。
7. **两个被否掉的备选**：
   - 「把 `verify` 的 vfail 显示出来」（我提的）：只是让你看见警告再去手动收拾。**吸收 > 检测。**
   - 「V1 `--default-in-place`」（用户先提的形态）：引入 `.claude.json` 分裂。
     用「账号 0 ≡ 不注入」重新定义即可避开。
   - 「symlink `$HOME/.claude.json` → `~/.claude/.claude.json`」（我提的补救）：
     **大概率撑不过一次写**——原子写（写 tmp + rename）会把符号链接换成普通文件，
     这正是本轮在 `upload_atomic` 上确认过的机制（BACKLOG E20）。已放弃。

---

## §1 功能清单

> 状态：待规划 / 规划中 / 实现中 / 审计中 / 完成

| ID | 功能 | 一句话目标 | 状态 | 依赖 | 优先级 |
|----|------|-----------|------|------|--------|
| Z01 | **账号 0 登记 + 可见** | manifest 认识账号 0；`list-accounts` 报出来；`verify` 从「违规」改判为「状态」；cc-monitor 列表多一行 | 待规划 | — | **P0** |
| Z02 | **「未选账号」消失** | `LaunchAccount` 三态化；账号 0 成为显式可选项；「基座」一词从 UI 移除；`--base` 只在真要账号 0 时发 | **⚠ 部分交付**（features/Z02-PARTIAL.md）：`--base` 跨语言契约守卫已落；**三态化卡 `tabs.ts` 红线**；文案刻意不先改 | Z01 | P0 |
| Z03 | **账号 0 接上既有能力** | 用量探针支持它（**实测拒绝点在 `remote-launch.ts:70`，`:74` 是注释**）；按会话切号能切到它 | **⚠ 部分交付**（features/Z03-account-zero-capabilities.md）：**(a) 用量探针已做**；**(b) 按会话切号卡 `tabs.ts` 红线** | Z01,Z02 | P1 |
| Z04 | **守卫** | ~~禁~~**说清**显式 `CLAUDE_CONFIG_DIR=~/.claude`（`which`/`run` 三处点名「这不是账号 0」；**禁不了**用户的 shell，in-place 又是被支持的逃生口）；`verify` 补 **in-place 盲区**——两份 `.claude.json` 同时存在 = 真分裂（~~新增「`~/.claude/.claude.json` 出现」检查~~ Z01 已以 vfail 落地，别重复加）；「删除账号 0」~~特判~~**早已有两道守卫**，本轮补断言 | **✅ 已交付**（features/Z04-guards.md） | Z01 | P1 |
| Z05 | **rc 片段一键生成**（独立） | 把 `cc-acct-iso shellinit` 接进 T03 的「生成待贴文本」组件，一键复制 | 待规划 | — | P3 |
| **Z08** | **物理迁移能力** `isolate` | `isolate <item>`（copy-then-unlink + CAS + 自检 + 回滚）· `cmd_sync` 的隔离项分支从 **`RM` 改成私有化**（修一个**真实的数据丢失**）· `cmd_add` 认 `ISOLATE_SET` | **✅ 完成，已签收**（`features/Z08-isolate-migration.md`）。**`share <item>` 排第二半**（实测反方向不丢数据、优先级低）；**`migrate` 命令名被 `sync` 吸收**（旧声明无处持久化 ⇒ 差集无从计算） | **G-B** | **P0** |
| **Z06** | **原生身份组成的单点声明** | `NATIVE_IDENTITY`（项名:原生根:类别）；`ISOLATE_SET` / `LEGACY_HOME_ITEMS` / `chmod 600` 目标**从它派生**（派生结果与历史字面量逐字相同）；**跨语言双写点守卫**钉住 daemon 那处独立的 `loggedIn` 判定 | **✅ 完成，已签收**（`features/Z06-native-identity-declaration.md`）。**守卫形态已订正**：原写「断言那些文件名在声明之外零出现」——实测那既做不到也不该做（bash 侧 ~18 处字面量绝大多数是正当具体用途或用户可见文案）。**`mcp.rs` 的 `.claude.json` 三候选那条双写点未钉**（第二半） | **G-B**, Z08 | **P0** |
| **Z07** | **版本钉 + 漂移检测**（销 E37） | 只读版本探测（解析 launcher 路径里的版本，**绝不执行 claude**）+ manifest additive `claudeVersionPinned` + `verify` 四条检测（**D1b 致命**：secret 泄漏进共享库 = 静默串号，零误报；D2/D3/D4 提示） | **✅ 完成，已签收**（`features/Z07-version-pin-drift-detection.md`）。**成功标准 8 的达成范围已如实限定**，见下 | Z06 | **P0** |

### Z08 为什么排 P0 且在 Z06 之前

**一个能力，两个需求都要它**：

1. 本需求（换位置后迁移）——改了声明就得把 z/b 的物理布局搬过去
2. **第三方 API key 的路线乙**（用户 2026-07-30 选定：`apiKeyHelper` 写进**每账号自己的**
   `settings.json`）——`settings.json` 今天是**共享**的（实测 z/b 都软链到 `~/.claude/settings.json`），
   要它私有就得把它从共享搬成私有

而 **cc-acct-iso 今天根本没有这个能力**（2026-07-30 读源码实测）：

| 现有路径 | 为什么不够 |
|---|---|
| `init` 的 `MOVE` | 只在初始化时跑一次，**不会重跑** |
| `cmd_add` | **完全不碰** `ISOLATE_SET`（只 `plan_links_for` + 可选 COPY/SEED） |
| `share_items()` | 加了新隔离项后它会**开始跳过**该项，但**不会拆掉已存在的软链** |
| `verify` (`:380`) | 会开始报「隔离项 X 竟是 symlink(会串号!)」 |

⇒ 净效果是「**配置说隔离、现实还共享、自检报错**」三方不一致。

**必须是 copy-then-unlink，不能用 MOVE**：MOVE 会把共享库那份搬走 ⇒ 只有第一个账号拿到文件、
其余账号的软链**全部悬空**。共享库那份留作**新账号模板**（用户 2026-07-30 已批）。

**Z05 为什么在这儿但标 P3**：它是 BACKLOG **F14**（「`.bashrc` 迁移用户自跑」），
与账号 0 正交，价值独立（贴了片段裸 `claude` 就走默认号）。放进本工作区只因为它同属账号体验，
**不做也不影响 Z01-Z04**。

---

## §2 架构概览

**三层，各自的职责边界**

| 层 | 住哪 | 对账号 0 的职责 |
|---|---|---|
| **cc-acct-iso**（bash，跑在目标机上） | 上游 `~/.claude/skills/cc-acct-iso/scripts/` + vendored 副本 | **唯一知道那台机器迁移没迁移的地方。** manifest 读写、账号 0 的登记与探测、`verify` 判定 |
| **shared/ccm**（bash，跑在目标机上） | 仓内 `shared/ccm`（`include_str!` 进二进制） | **不改。** `--base` = unset = 起账号 0，语义已对；`:320` 的 per-machine 回退已对 |
| **cc-monitor**（Rust + TS） | `accounts.rs` / IR / UI | 把账号 0 当一个**普通账号**渲染与选择。**不自己判断迁移状态**，一律问目标机 |

**关键契约**：
- **「迁移状态」的判断权归目标机**。cc-monitor 只消费 `list-accounts` 的输出，
  绝不自己 stat 远端路径去猜——这是本轮反复栽的「以为在本机、其实在远端」偏差的防线。
- **`configDir` 为「无」必须是一个显式表达**，不能是空串（见 §3 共享面 2 与 §6 风险 3）。

---

## §3 ★共享面账本

| 共享面 | 涉及功能 | 最终形态设计 | 当前状态 | 备注 |
|---|---|---|---|---|
| **1. cc-acct-iso manifest 数据模型** | Z01,Z02,Z03,Z04 | 账号 0 用**显式标记**表达「不注入」，例如 `"mode":"bare"` + `configDir` 省略/为 `null`。**绝不用空串**。`list-accounts` 输出里账号 0 与其他账号同构（有 `name`/`email`/`isDefault`/`loggedIn`），只是 `configDir` 缺席 | 未动 | **最重要的一条。** 空串会让 `run` 那行 `env CLAUDE_CONFIG_DIR="$cfgdir"` 设出一个空值，而空值 ≠ 未设 |
| **2. `cc-acct-iso run` 的启动路径**（`cc-acct-iso:682`） | Z01,Z02,Z03 | `run 0` 必须走**真 `unset`** 分支（`exec env -u CLAUDE_CONFIG_DIR …` 或直接 `exec "$LAUNCHER"`），而不是 `env CLAUDE_CONFIG_DIR="" …` | 今天是 `exec env CLAUDE_CONFIG_DIR="$cfgdir" "$LAUNCHER" "$@"` | 空串 vs unset 的行为**必须先实测**，见 §6 开放问题 3 |
| **3. `src/launch-plan.ts::LaunchAccount`** | Z02,Z03 | 三态判别联合：`{kind:"account",name?,configDir}` · `{kind:"account0"}`（显式要账号 0 → 发 `--base`） · **不再有表示「没选」的变体**（UI 层保证必选，见共享面 5） | 今天两态，`accountOf` 把「无 configDir」映射成 `{kind:"base"}` | R05 已经把它从裸魔法串 `"__base__"` 升级成判别联合，这次是加变体不是重构 |
| **4. `ACCOUNT_DIMENSION`（`launch-dimensions.ts`）** | Z02,Z03 | `applies` 保持恒真（F05 那条论证仍成立）；`cliFlags`：`account0 → ["--base"]`、具名 → `["--account",name]`；`apply`：`account0` 不推 env op（= 不设，正确） | `cliFlags` 对 `base` 恒返回 `["--base"]` | **`--base` 不删，是把它的含义从「我不知道」收紧成「起账号 0」** |
| **5. UI 里「基座」的全部出现点**〔**Z02 实测订正**：生产代码 **10 个**文件不是 7 个；`tabs.ts` **20 处**不是 ~14；**`views/history.ts` 实测 0 处**，计划列错；计划漏了 `ipc/commands.ts`/`launch-dimensions.ts`/`launcher-diagnostics.ts`/`launch-plan.ts`〕 | Z02 | 「基座」一词只在**历史注释**里出现；用户可见文案统一为「账号 0」（名字见 §6 开放问题 5）。账号选择器**不预选**、不选不让走（保住 `cc-bus-section.ts:229`「不替用户默认花掉某个会花钱的号」那个意图，但换掉机制——**「不选账号」不是安全的空值，它是坏的值**） | 7 个文件：`tabs.ts`(~14 处) · `launch-menu.ts:74` · `views/history.ts` · `settings/cc-bus-section.ts:234,474,516` · `settings/remote-section.ts:890-892` · `accounts.ts:164` · `remote-launch.ts:122` | **`tabs.ts` 撞红线，见 §6 开放问题 2** |
| **6. `verify` 的判定语义** | Z01（改判）,Z04（加检查） | `~/.claude/.credentials.json` 存在 → **「账号 0 已登录」**（正常）；`~/.claude/.claude.json` 存在 → **新增 vwarn**「有人用显式路径起过账号 0，状态可能已分裂」 | 今天前者是 `vfail`、后者无检查 | 这就是 §0.1 那笔交易的落点：**用一条检查换掉一条不变量** |
| **7. 上游 ↔ vendored lockstep** | Z01,Z04 | 上游 `~/.claude/skills/cc-acct-iso/scripts/` 与 `src-tauri/vendor/cc-acct-iso/scripts/` 逐字节一致，`.vendor_id` 按 `VENDOR.md` 菜谱重算，`build.rs` 的软检查不 warn | 今天一致 | `~/.local/bin/cc-acct-iso` 是 symlink 指向上游 ⇒ **改了立刻在 aya 上生效，没有缓冲**。见 §6 风险 2 |
| **9. 原生身份组成的声明** | **Z06**,Z07,Z08 | **✅ 已落地**：`NATIVE_IDENTITY` 表（项名:原生根:类别）+ 四个投影；`ISOLATE_SET`/`LEGACY_HOME_ITEMS`/`chmod 600` 目标从它派生，**派生结果与历史字面量逐字相同**（有对拍断言）。**⚠ 守卫形态已订正**：原文那句「扫源码断言这些名字不再在别处出现字面量」**做不到也不该做**——实测 bash 侧 ~18 处字面量绝大多数是**正当的具体用途**（种这个文件、chmod 那个文件）或**用户可见文案**，硬消掉是把代码改难读。真正被独立实现两遍的判定只有两条，且**跨语言跨进程**（`loggedIn`：bash vs daemon `accounts_query.rs:407`；`.claude.json` 位置：bash vs `mcp.rs` 三候选）⇒ 改用**双写点守卫**（`include_str!` + 锚定声明行，同 `TMUX_LS_FMT` 的范式） | ✅ 凭据那条已钉（daemon 141 测）；**`mcp.rs` 那条未钉**（第二半） | **原计划点名的 `accounts.rs:49` 不是决策点**（只是文档注释）。⚠ **`cp -a` 保留 mtime ⇒ re-vendor 后本地 cargo 可能不重编译 `include_str!`、守卫报陈旧结果**；CI 干净 checkout 不受影响，本地要 `touch` |
| **10. `isolate`/`share` 子命令** | **Z08**,Z06 | **✅ `isolate` 已落地**：新 plan 动词 `ISOLATE` = copy-then-unlink（读共享签名 → `bk_copy` → 同目录 `mktemp` → **CAS 复核签名未变** → **先 `rm` 掉软链**（否则 `mv -f` 会跟随软链**反向覆盖共享库**）→ `mv -f` 原子落位 → `cmp`+非软链自检 → 不符 `ln -s` 回滚 → `undo_restore`）。**共享库那份保留作新账号模板。** `cmd_sync` 的隔离项分支改用它、`cmd_add` 认隔离集。**⚠ `migrate` 那句「按新旧声明求差集」已否掉**：旧声明**没有任何地方持久化**（`ISOLATE_SET` 只有当前值）⇒ 差集无从计算；真正可做的是「让现实对齐当前声明」= **`sync` 的职责**，故不新增该命令名 | ✅ Z08 完成；**`share <item>` 未做**（第二半） | 前置 G-B 已解除。**E36「API key 路线乙」的技术前置就此就位**。dry-run 默认 |
| **11. Claude Code 版本钉** | **Z07** | **✅ 已落地**：manifest additive `claudeVersionPinned`（探不到就省略该键——「没钉」≠「钉了空」）+ `verify` 比对。**版本探测只读**：解析 launcher 可执行文件路径里的版本（原生安装器布局 `.../versions/<semver>`，纯 `readlink`）→ 回落 `.last-update-result.json` → 探不到就明说跳过。**绝不执行 `claude --version`**（`verify` 的契约是「不登录」）。**版本是给人看的上下文，不是自动迁移的触发器** ⇒ 档位是 vwarn | ✅ 完成 | 与它配套的致命档是 **D1b**（secret 泄漏进共享库，从 `ni_secrets` 派生，零误报）。**Z01 必须处理 D1b 的例外**——账号 0 的 config dir **就是**共享库 |
| **8. 远端版本协商** | Z01（建立）,Z03（依赖） | cc-monitor 判断对面 cc-acct-iso 认不认账号 0：**看 `list-accounts` 输出里有没有账号 0 条目**（或 manifest `version` 字段）。不认 → 降级成今天的行为并**明说**「该机器的 cc-acct-iso 版本较旧，账号 0 不可见」，绝不静默 | 未动 | 本仓已有先例：`ccm` 的 `capabilities=` 串 |

---

## §4 依赖图与实现顺序

```
G-B（vendored bash 进 shellcheck + 它自己的测试进 CI）      ← 全区前置，gate-integrity 区
 │
 ├─ Z08（isolate/share/migrate 能力）──┬── Z06（原生身份组成单点声明）── Z07（版本钉+漂移检测，销 E37）
 │                                     └── ★ 也是 BACKLOG E36「API key 路线乙」的前置
 └─ Z01（登记+可见）──┬── Z02（未选账号消失）──── Z03（用量+切号）
                      └── Z04（守卫）
Z05（rc 片段）  独立，任意时点可插
```

**2026-07-30 追加的顺序约束**：

0. **G-B 是整个 cc-acct-iso 半区的前置**（不只是 Z01 的）。理由不是我加的，是 gate-integrity
   主计划本来就定的：那 1348 行 bash 被 `include_bytes!` 打进二进制、部署到远端执行，
   却在 shellcheck 门禁之外，它自己那 424 行测试**从没跑过**。**没有网不能改那个工具**
   —— 而 Z06/Z08 恰恰是对它做**结构性**改动（远比 Z01 的加一个 manifest 条目大）。
0b. **Z08 排在 Z06 之前**：Z06 改了声明就需要能迁移，否则改完落不了地；且 Z08 独立有价值
   （E36 路线乙 直接要它）。
0c. **Z06 与 Z01 可并行**（Z06 只碰 cc-acct-iso 内部的常量来源，Z01 碰 manifest 数据模型），
   但**别同轮改 `verify`** —— Z01 改判定语义、Z07 加版本检查，两者都动 `verify`，
   排开写（同 §4 原文对 Z04/Z02 的处理）。

**顺序与理由**：

1. **Z01 先做**。它是**纯增量、零行为变化**：manifest 多一个条目、列表多一行、`verify` 改判。
   **用户的核心动机（幽灵变可见）在这一步就达成**，且不碰任何启动路径、不碰 `tabs.ts`。
   ⇒ 单独交付有价值，风险最低，先暴露数据模型的问题。
2. **Z02 次之**。它依赖 Z01 提供的「账号 0 是一个真条目」。改动面最大（7 文件含 `tabs.ts`）、
   要推翻四条已记档决策（F01 步骤2 / F09 的阈值订正 / R05 的类型化理由 / U8 文案），
   所以**必须在 Z01 把数据模型稳住之后**。
3. **Z04 可与 Z02 并行**（只碰 cc-acct-iso，不碰 cc-monitor UI），但排在 Z02 后写，
   免得两个功能同时改 `verify`。
4. **Z03 最后**。它要账号 0 已经能被选中（Z02）才有意义。

---

## §5 横切关注点与约定

- **不用 emoji**（用户偏好，全局）。commit **不加** `Co-Authored-By`。`git add` 显式文件清单。
- **测试约定**：沿用本仓既有门禁——`cargo test --all` · `cargo test -p code-picture-core` ·
  `cargo fmt --check` · `cargo clippy --all-targets` · `npx tsc --noEmit` · `npm test` ·
  `npm audit --omit=dev --audit-level=high` ·
  `shellcheck --severity=error e2e/*.sh shared/cc-bus/scripts/* shared/ccm` ·
  `bash e2e/exec-bit-guard.sh` · **8 套真机套件**（实测基线 26/44/12/15/13/21/14/7 = 152 条）。
  基线（本工作区开工时）：**cargo 536 · npm 814 · clippy 0 · tsc 0**。
- **改 cc-acct-iso 要跟着扩门禁**：`vendor/cc-acct-iso/scripts/` 目前**在 shellcheck 门禁之外**
  （BACKLOG **E13**，实测今天零告警 ⇒ 扩进来零成本）。本工作区既然要改它，**Z01 顺手把它纳入**。
  另有 `vendor/cc-acct-iso/scripts/test/run-tests.sh`（424 行，工具自己的测试）在 CI 与
  `package.json` 里都不存在 —— 既然要改工具，**这套测试必须先接进门禁**，否则改动无网。
- **测试纪律**（本会话固化，逐条适用）：变异**先 diff 确认落位、再确认它编译得过**，然后才判色 ·
  反向自检 · 计数自检用 `==` 不用 `>=` · **守卫范围要恰好等于性质范围**（本会话栽过三次）·
  **源码文本扫描 ≠ 行为测试** · commit message 里每句「已有测试守着」都要先跑变异证明。
- **绝不启动真实已认证的 `claude`/`codex` 子进程**。凡涉及真登录的验证，
  由用户自己跑（本计划 §6 开放问题 6 列出具体步骤）。
- **绝不写 `~/.claude/settings.json`、`~/.bashrc`、任何 PowerShell profile。**
  `~/.claude-accts/accounts.json` 是**用户真实数据**：任何改它的代码路径必须
  「备份 → 写 → 读回比对 → 不符回滚」，且测试一律注入闭包、绝不真写盘。
- **tmux 一律走强制 `-L` 的守卫 shim + 起飞前 canary 双向自检 + 跑完核对默认 socket 会话清单**。
  裸 `tmux kill-server` 是禁用词。

---

## §6 风险与开放问题

**风险**

1. **换掉了「共享库不含账号态」这条不变量。** 今天安全，因为 `.credentials.json` 与
   `.claude.json` 都在 ISOLATE_SET 里、不会被 symlink 共享。但这**依赖 Claude Code
   未来不把身份态放进某个已共享的文件**。V2 那条不变量正是为了不依赖这个假设。
   **缓解**：Z04 的两条检查 + `verify` 每次都跑。**这是一笔明知代价的交易，不是疏漏。**
2. **上游 cc-acct-iso 改坏会立刻影响 aya 的真实账号操作**（`~/.local/bin/cc-acct-iso`
   是 symlink 指向 `~/.claude/skills/cc-acct-iso/scripts/cc-acct-iso`，无缓冲）。
   **缓解**：改前把上游整目录复制一份带时间戳的备份；先在 vendored 副本上改+测，
   过了 `run-tests.sh` 再同步上游。
3. **空串 ≠ 未设，且未实测。** `cc-acct-iso:682` 是 `exec env CLAUDE_CONFIG_DIR="$cfgdir" …`。
   若账号 0 的 `cfgdir` 为空，会设出 `CLAUDE_CONFIG_DIR=""`。Claude Code 大概会当未设处理，
   **也可能当成一个空路径直接坏掉**。⇒ **Z01 的第一步就是实测这一条**，结果决定共享面 2 的形态。
4. **远端版本错配**：老 cc-acct-iso 不认账号 0。缓解见共享面 8（明说降级，不静默）。
5. **要推翻四条已记档决策**（F01 步骤2 给 tabs 加基座项 / F09 把 account 组阈值从 ≥2 订正成 ≥1
   就为了它 / R05 的类型化理由 / U8 的「不指定」文案）。Phase F 必须逐条回写，
   不能只在新工作区写「已改」而让原文档继续声称旧决策——**这正是 Phase G 文档审阅
   报为阻塞 B1/重要 I1 的那个失效模式。**

**待用户确认的开放问题（Phase A 审批时一并定）**

| # | 问题 | 我的建议 |
|---|---|---|
| 1 | **授权动 `~/.claude/skills/cc-acct-iso/`**（你的家目录，上游本体）？ | 需要。若不给，Z01/Z04 只能改 vendored 副本 → 两份漂移，**比不做更坏**。替代方案：我改 vendored + 生成一份 diff 给你自己贴到上游 |
| 2 | **`tabs.ts` 红线松不松？** Z02 绕不开（~14 处） | 建议松。只做 history + 设置页会让四个入口行为不一致，我不推荐 |
| 3 | 账号 0 的**显示名**：「账号 0」/「主账号」/ 让你命名？ | 建议 manifest 里存一个可改的 `name`，默认 `"0"`，UI 显示「账号 0（主）」。理由：你可能想叫它别的，而 manifest 已经有 `name` 字段 |
| 4 | aya 上**现在就把账号 0 登记进 manifest** 吗（它未登录，纯加一行）？ | 建议 Z01 做完后由你手动跑一次 `cc-acct-iso` 的登记命令，我不代你改 `accounts.json` |
| 5 | Z05（rc 片段一键生成）**这轮做不做**？ | 建议做，独立价值，且顺手把 BACKLOG F14 收掉 |
| 6 | **真机验证谁跑**？需要真起 claude + 真登录，本会话红线禁止我做 | 你跑三步：① `cc-acct-iso verify` 看当前是否已报那条 vwarn ② 不选账号 resume 一个远端会话，看是否要求登录 ③ Z01 做完后再 verify 一次，看账号 0 是否出现 |

---

### 2026-07-30 追加的两条风险

| # | 风险 | 处置 |
|---|---|---|
| **R-新1** | **「换机制」是这层抽象救不了的**。若 Claude Code 把凭据搬进 OS keyring / 走系统钥匙串，**按目录切身份这条路整体失效**——keyring 不是 per-directory 的。最坏的失败模式是**静默共用身份**：两个账号都指向同一个 keyring 条目，你以为在用 z、实际烧 b 的额度，而且 UI 上看不出来 | **不承诺可迁，承诺可检测。** Z07 的漂移检测必须覆盖「声明里的身份文件在实际布局里找不到」这一格 ⇒ 当场报、要求人复核，绝不降级成 warn。**验收要用变异**：把声明改成假位置，`verify` 必须红 |
| **R-新2** | **Z06 的守卫本身可能范围不等于性质**：声明建立后，别处再写死一份就白搭。而「写死一份」的形态不止字面量（可能是拼接、可能是另起一个常量） | 守卫断言的是**那几个文件名字面量在声明之外零出现**（`.credentials.json` / `.claude.json` / `policy-limits.json` / `stats-cache.json`），并带反向自检（扫到的文件数 > 0）。这是范围略窄于性质的守卫 —— **如实记录**，不假装它证明了全部 |

## §7 变更记录

| # | 日期 | 改了什么 / 为什么 |
|---|---|---|
| **新8** | 2026-07-30 | **Z03 部分交付**：**(a) 用量探针支持账号 0** 已做，**(b) 按会话切号卡 `tabs.ts` 红线**。载荷从此有且只有两种形态：具名账号 `export CLAUDE_CONFIG_DIR=…`、**账号 0 `unset CLAUDE_CONFIG_DIR; `**；**空串仍然 throw**（它不是账号 0,是坏数据）。**fail-closed 是这条的要害**：账号 0 绝不能退化成「什么前缀都不加」——裸载荷会继承远端 rc 里那句 `export CLAUDE_CONFIG_DIR=<默认账号>`（`shellinit` 生成的就是它）⇒ 探针探到别的号,而 UI 把结果标成账号 0 的用量 = **数字看着正常、指的是别人**（E37 最坏形态）。两层各钉一条断言（纯函数 + 真实点击到 IPC 的 payload）。**顺手消掉一个正在长出来的双写点**：`unset CLAUDE_CONFIG_DIR; ` 提成 `shell-quote.ts::UNSET_CONFIG_DIR_PREFIX`,与 `launch-render-fallback` 共用（逐字节不变,e2e 那条精确子串断言不受影响）。换掉 Z01 在 chip/设置面留的两处「暂不支持」占位；订正 `account_usage.rs` 两处描述载荷形态的过时注释（Rust 只透传不校验,已核）；订正计划里的行号（拒绝点是 `:70` 不是 `:74`）。两条变异全部成立（裸载荷回归 / 空串也当账号 0）,其中后者还红了**一条既有断言** ⇒ 老套件本来就在守这条。vitest 855→**860** |
| **新7** | 2026-07-30 | **Z02 部分交付（三态化卡 `tabs.ts` 红线）。★ 本轮最重要的是订正了 Z01 的一句错记录**：Z01 在 `isSelectable` 上方写死「从 UI 起账号 0 需要显式 unset 的注入形态，而 `launch-plan` 今天只会 export」——**错的**。unset 形态**两条渲染路各有一份**：CLI 路径 `cliFlags` 吐 `--base`、`shared/ccm` 收到会 `unset CLAUDE_CONFIG_DIR`（两处落点 `:572`/`:598`）；兜底路径 `ENV_RESET_DIMENSION` 推 `unset-config-dir` op。**我错在只查了 `ACCOUNT_DIMENSION.apply` 就下结论，没往下查 `cliFlags` 与 ccm** —— 与 Z07 把 D2 推理照搬给 D1b 同类：**一个层面的事实被当成整条链路的事实**。真正卡的是**选择链路**（`accountConfigDir` 返 null ⇒ `resolveAccount` 说不出「显式选了账号 0」· 菜单无此选项 · **`tabs.ts:2283` 那个三元加变体不报编译错地走错分支**），只有第三条卡红线。**本轮交付**：`--base` 的跨语言契约守卫（8 条）—— monitor 整套「基座 = 不注入」压在「ccm 会照 `--base` unset」这个假设上，而**全仓无人钉**；漂了就是「以为起账号 0、实际烧默认账号」的静默串号（E37 同类）。`shared/ccm` 是红线 ⇒ 四条 ccm 侧变异在 scratchpad 副本上做，monitor 侧那条真改再回滚，**五条全部成立**。**文案刻意不先改**：菜单还不能真选账号 0 时把「基座（不隔离）」改叫「账号 0」，就是把**「没选」标成「账号 0」**，方向反了。**计划文件清单实测订正**：生产代码 10 个不是 7 个；`tabs.ts` **20 处**不是 ~14；**`views/history.ts` 实测 0 处**（计划列错）。vitest 847→**855**（57 文件）。红线松开后的七步顺序已写进 `features/Z02-PARTIAL.md` §6 |
| **新6** | 2026-07-30 | **Z04 交付签收（守卫）。计划的三条逐条实测后全部订正**——这是本区连续第六轮开工复测抓出计划与实际不符。① 「**禁**显式 `CLAUDE_CONFIG_DIR=~/.claude`」**做不到**：工具管不了用户的 shell，而 in-place 又是它自己提供并明确支持的逃生口 ⇒ 改成**说清**（`which` 分「已登记的 in-place 逃生口」与「**未登记地手工指向共享库**」两种措辞，后者才是「误以为这就是账号 0」那种危险；`run <in-place>` 把两份状态的代价说在前面，**不禁**）。② 「`verify` 新增『`~/.claude/.claude.json` 出现』检查」**Z01 已以 vfail 落地**（比计划的 vwarn 更强）⇒ 本轮不重复加，改补它的**盲区**。③ 「删除账号 0 特判防连带删共享库」**早就存在**（`cmd_rm` 的 `is_under`）⇒ 本轮补的是断言（此前零覆盖）。**真正的洞（计划没提）**：纯 in-place 库里 `.claude.json` 的**真分裂完全不可见**——泛泛的模式警告不查实况 + Z01 那条 vfail 在 `isolated_any=0` 时整条 skip + 「隔离项是本账号私有实体」还把共享库那份报成绿灯。⇒ 新增「两份 `.claude.json` 同时存在」检测，点名两个路径与字节数、哪条路读哪份、**凭据却是同一份**（一个登录身份、两套本机状态）。**档位 vwarn 不是 vfail**：判致命会让在野所有 in-place 用户突然红。**一条断言我自己写错又改正**：初版写「纯 in-place 库里 verify 仍单独报『账号 0 已登录』」——错的，两者读同一份凭据，分开报会让人以为是两个登录。**变异 B 反而证出一件好事**：拿掉 `cmd_rm` 的 `is_under` 后「共享库还在」**没有变红，而那是正确的**——`lib.sh` 的 `_exec_op RM)` 分支自己也查一遍，纵深防御生效；第二道从 CLI 走不到 ⇒ 用扫源码的方式钉住。四条变异全部成立。测试 268→**294**，两道地板同步上调，`.vendor_id` `bf3d3a798d095162` → `e24bfd164014351a`。cc-monitor 侧零改动（Z04 只碰 cc-acct-iso）。**用户真实数据零改动** |
| **新5** | 2026-07-30 | **Z01 交付签收（账号 0 登记 + 可见）。** **开工第一件事就推翻了前一个功能的一句话**：Z07 给 D1b 写的理由「secret 在共享库 ⇒ 会被**自动 symlink 给每个账号** = 静默串号」**是错的** —— `share_items()` 的两个分支都有 `is_isolate && continue`，而 secret **全在隔离集里** ⇒ 隔离项**从不**被 symlink 出去（沙盒实测：放回 `.credentials.json` 再 `sync`，z 保持私有实体、b 根本没有、零软链）。根因是**把 D2 的推理整段照搬给了 D1b，而前提没照搬得过去**。⇒ 判据换成 Z06 声明里的 **`root` 字段**：`root=cfg`（`.credentials.json`）= 共享库**就是**账号 0 的 config dir ⇒ **状态不是违规**；`root=home`（`.claude.json`）= 不是任何账号的原生位置 ⇒ **仍致命**。四份文档同步订正。**顺带核实**：`sync` 的 ISOLATE 是 **copy-only**（共享库那份保留作模板）+ `add`/`sync` 都跳过 secret ⇒ **不会把账号 0 登出**。**空值 ≠ 未设**这条支点在四层各钉一遍（bash 省略键 / Rust `Option<String>` / TS `string \| null` / `run 0` 用 **`env -u`**）。**新增**：`run 0` · `which` 未设时报账号 0 且 **rc=0** · `shellinit` 的 `0cc` 逃生口（那句 `export CLAUDE_CONFIG_DIR=<默认>` 是全局的，不给逃生口就回不去）· daemon 裸起会话**归属账号 0** · 新动词 `--account-trust-zero`（路径写死、不收参数 ⇒ 没有任意文件读的面）· 能力标记 `accountZeroAware` + monitor `degraded_notice`（旧 daemon / 旧 cc-acct-iso **分开说**，绝不静默）。**三个坑**：`chk 'cmd \| grep -q'` 在 pipefail 下恒红（Z06 那个坑的第三次新装扮）· `acct_zero_logged` 初版 printf true/false ⇒ 条件恒真且污染 stdout · `init --apply` 会把凭据搬走 ⇒ 沙盒要显式造共享库凭据。**账号 0 暂不可选**（要 unset 注入形态，卡 `launch-plan`+`tabs.ts` 红线）**如实登记为部分达成**。测试 231→**268** / daemon 141→**149** / monitor 611→**618** / vitest 837→**847**；`.vendor_id` `3416ab2260e55d74` → `bf3d3a798d095162`；**用户真实数据零改动**（`accounts.json` mtime/size 未变、z/b 的 `settings.json` 仍是 07-26 两个软链；`b/` 的 11:01 变动是用户另一个在跑的 claude，不是本轮） |
| **新4** | 2026-07-30 | **Z07 交付签收，BACKLOG E37 已销。** 四条检测 + 只读版本探测（解析 launcher 路径里的版本，**零执行**；`verify` 契约是「不登录」，在里面跑 `claude --version` 是越界）。**一处自己犯的错、被既有套件当场接住**：D1「声明项哪儿都找不到」初版写成 **vfail**，沙盒误报 4 条——声明里好几项是 Claude Code **懒创建**的，「还没被创建」与「改了位置」**在这个信号层面不可判定**，判致命会让几乎每台干净机器都红 ⇒ 降级 vwarn，致命档换成 **D1b**（secret 泄漏进共享库，**零误报**；⚠ 当时给的理由「会被自动 symlink 给每个账号 = 静默串号」**是错的，Z01 已订正**，见 features/Z07 §8，且从 `ni_secrets` 派生后顺带覆盖了此前漏查的 `.claude.json`）。**成功标准 8 的达成范围已如实限定**（见 §0.1）。**两次测试泄漏真机状态**：往共享库放文件后没 `sync` ⇒ 触发的是既有检查；沙盒 PATH 不遮蔽 `claude` ⇒ 读到真机 2.1.220。**给 Z01 的提醒**：账号 0 的 config dir 就是共享库 ⇒ **D1b 对它必须有例外**，别直接沿用。测试 215→**231**，两道地板同步上调，`.vendor_id` `15b1ca8ccfb7c9c1` → `3416ab2260e55d74` |
| **新3** | 2026-07-30 | **Z06 交付签收。开工前复测把计划的「6 处」纠正了**：`accounts.rs:49` 只是文档注释、不是决策点；真正被独立实现两遍的只有两条判定，且**跨语言跨进程**（`loggedIn` 在 bash 与 **daemon `accounts_query.rs:407`** 各一份；`.claude.json` 位置在 bash 与 `mcp.rs` 三候选各一份）。而 bash 侧 ~18 处字面量**绝大多数是正当的具体用途或用户可见文案** ⇒ **守卫形态从「字面量零出现」改成「跨语言双写点」**（`include_str!` + 锚定声明行，同 `TMUX_LS_FMT` 范式）。**单点声明的第一个收益是让两处不一致自己浮出来**：`init` 的 MOVE 路径只 chmod 600 了 `.credentials.json`、`sync` 的权限修复也只管它 ⇒ `.claude.json`（同样含 `oauthAccount`）的权限漂了没人修。**两条都修了**（不修则 `sync` 第一次跑必然产生一次性修复、不收敛，直接打红既有的幂等断言）；第三条（`verify` 硬失败范围）**刻意不改**——在野 `.claude.json` 若是 644 会给用户突然的新红。**一个把 197 条打红 115 条的坑**：投影里 `cond && printf` 惯用法在 `set -o pipefail` 下，最后一行条件为假会让 `while` 以 1 退出 ⇒ 整条管线判失败 ⇒ `set -e` 就地退出（派生值全对却在赋值后死掉），改用 `if … fi` 并加三条 `pipefail` 断言。**另一个坑**：`VENDOR.md` 菜谱的 `cp -a` **保留 mtime** ⇒ re-vendor 后 cargo 指纹判「没变」⇒ `include_str!` 不重编译 ⇒ **守卫报陈旧结果**（本地要 `touch`，CI 不受影响）。测试 197→**215**，daemon 140→**141**，两道地板同步上调 |
| **新2** | 2026-07-30 | **Z08 交付签收。三处对计划的实测订正**：① **我此前记的失效模式是错的** —— 不是「配置说隔离、现实还共享」，实测是 **`cmd_sync` 把每个账号的软链直接 `RM` 删掉** ⇒ 两个账号**根本没有该文件**（对 `settings.json` = hooks/model/permissions/theme 全部静默丢失、回落默认值）。**Z08 修的是数据丢失，不是不一致。** ② **`migrate` 那句「按新旧声明求差集」否掉**：旧声明无处持久化 ⇒ 差集无从计算；改成「让现实对齐当前声明」并**吸收进 `sync`**（那本就是 `sync` 的职责，`verify` 今天就在报这条不变量违反），**不新增命令名**。③ **`share <item>` 排到第二半**，理由是实测的：反方向今天只是一句 `warn`、**不丢数据**，而 isolate 方向真丢。**G-B 的网第一次真正管住一个改动**（shellcheck 36 文件 + vendored 自测 171→**197**、地板同步上调）。**断言地板搬进脚本自己**（`MIN_ASSERTS`，补 gate-integrity §2「同源」；CI 侧那条留作双保险）。lockstep 完成：`.vendor_id` `e5475b0c140ebbe1` → `740a68b11a71ce27`，`build.rs` 过期检查不 warn。**用户真实 `~/.claude-accts/` 零改动**（z/b 的 `settings.json` 仍是 07-26 那两个软链、`accounts.json` mtime 未变、`ISOLATE_SET` 仍是默认值）—— 本轮只交付能力，落地真机等用户发话 |
| **新** | 2026-07-30 | **按用户追加需求扩了本区范围：加 Z06/Z07/Z08 三个功能。** 用户原话「能不能把账号 0 定义为 claude code 默认登录方式，到时候如果 claude code 换登录方式了换位置了我们可以方便迁移」。**一处订正**：我起初把现有定义说成「按物理位置的外延定义」——**说错了**，「账号 0 ≡ 不设 `CLAUDE_CONFIG_DIR` 这个状态本身」本来就是内涵的，而且 §0.1 有专门论证为什么不能给它 `configDir`（两条路 ⇒ 状态分裂）。**那条定义不动。** 真正没被抽象的是「原生身份由哪些文件组成」，它硬编码在 `ISOLATE_SET`/`LEGACY_HOME_ITEMS` 并散抄 6 处（§0.1 新增那张表列了行号）⇒ 加一层**声明**（Z06）、一个**版本钉+漂移检测**（Z07，销 BACKLOG E37）、一个**物理迁移能力**（Z08）。**范围外那条「不改隔离/共享划分」据此订正**——划分的内容零改动，改的是它的来源。**验收边界写死**：换位置这类一处改完 + 一次迁移；**换机制（keyring）救不了，只承诺 100% 当场检测到而不是静默共用身份**（§6 R-新1）。**顺序**：G-B 是整个 cc-acct-iso 半区的前置（不只 Z01）；**Z08 排 Z06 之前**且它同时是 BACKLOG **E36「API key 路线乙」**的前置——一个能力两个需求都要 |

- 01 — 2026-07-29 — 初版，Phase A 主规划完成 — 由 Phase G 审阅的阻塞 E15 起，经四轮推导
  （两次自我推翻 + 一次用户转向）收敛到「基座 → 受管账号 0」。等用户审批。
