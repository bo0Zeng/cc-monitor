# 状态 / STATUS — account-zero（恢复工作的入口，每次先读这里）

> 跨轮对话靠这个文件接着干，不靠记忆。每完成一步就更新。

- **当前阶段**：**B 功能规划**（Phase A 主计划 **2026-07-29 用户已批准**；
  **2026-07-30 按用户追加需求扩范围、加 Z06/Z07/Z08，主计划已修订落地**）
- **已完成功能**：**Z08**（`isolate` 迁移，`60946b0`）· **Z06**（`NATIVE_IDENTITY` 单点声明，`941e13d`）
  · **Z07**（版本钉 + 漂移检测，`e6674d3`，销 E37）· **Z01**（账号 0 登记 + 可见，`7ad1ae3`）
  · **Z04**（守卫，`f97bb76`）· **Z02 部分**（`--base` 跨语言契约守卫，2026-07-30；三态化卡红线）
- **当前功能**：—（Z02 已按「能做的做完 + 卡住的标注」处理；下一个是 **#10 Z03**）
- **当前步骤**：Z02 Phase F 完成（部分交付）
- **下一个功能**：Z03 → Z05 → G-A
- **Z03 开工前的实测（2026-07-30 已测，下轮别重测）**：它能**干净拆成两半**——
  - **(a) 用量探针支持账号 0：可做，不碰 `tabs.ts`。** 面只有 `remote-launch.ts::buildUsageProbePayload`
    （今天 `if (!configDir) throw`，注释写「不支持基座/无账号场景」）+ `account-usage.ts::fetchAccountUsage`
    的 `configDir: string` 签名 + Z01 在 `account-chip.ts`/`settings/accounts-section.ts` 放的两处
    「账号 0 暂不支持用量查询」占位（做完要一并换掉）。载荷前缀要从 `export CLAUDE_CONFIG_DIR=…; `
    换成 `unset CLAUDE_CONFIG_DIR; `——**那条语义现已由 `base-flag-contract-guard.vitest.ts` 钉住**。
  - **(b) 按会话切号切到账号 0：卡 `tabs.ts` 红线**（tab 右键菜单），且承 Z02 的三态化。
- **阻塞 / 待用户确认**：
  - **[已批准] 主计划**（用户 2026-07-29「批准主计划 account zero」）
  - ~~**[待确认] 授权动 `~/.claude/skills/cc-acct-iso/`**~~ → **2026-07-30 用户已授权**
    （且授权改 `z`/`b` 真实账号目录，走「备份 → 改 → verify 复核 → 不符回滚」）。
    **但用户随后说「先不要改，我现在在用 claude code」⇒ 动真账号目录那步等发话。**
  - ~~**[硬前置] G-B 未做**~~ → **2026-07-30 已交付**（`0b297ed`）。那两道网已第一次真正管住
    一个改动（Z08）：shellcheck 覆盖 36 文件 + vendored 自测 171→**197**、地板同步上调。
    **注意 `~/.local/bin/cc-acct-iso` 是 symlink 指向它 ⇒ 改了立刻在 aya 上生效、没有缓冲。**
    未获授权前，Z01/Z04 只改 vendored 副本并生成 diff 给用户自己贴——**但那会造成两份漂移，
    比不做更坏**，所以 Z01 开工前必须先要到这条。
  - **[待确认] `tabs.ts` 红线松不松**（Z02/Z03 绕不开。**实测是 20 处「基座」不是 ~14**）。
    Z01/Z04 不需要。**一旦松开，按 `features/Z02-PARTIAL.md` §6 的七步顺序做**——第 1 步必须是
    `tabs.ts:2283` 那个三元改穷尽判别，否则第 2 步加变体会**静默**走错分支（不报编译错）。
  - ~~**[待确认] 账号 0 的显示名**~~ → **Z01 已定**：manifest 里就叫 `"0"`，且 `"0"` 是**保留名**
    （`add 0` 拒）。消费侧**一律按结构判**（`configDir` 键在不在），**不认名字** ⇒ 将来想改显示名
    只动 UI 文案即可，不会牵动判据。
  - **[Z01 登记的后续，Z02 已订正] 账号 0 暂不可选**：Z01 记的理由「需要 unset 注入形态、而
    launch-plan 只会 export」**是错的** —— unset 形态两条渲染路各有一份（CLI 走 `--base` ⇒ ccm
    `unset`，兜底走 `ENV_RESET_DIMENSION`），且现已由 `base-flag-contract-guard.vitest.ts` 钉住。
    **真正缺的是选择链路**：`accountConfigDir` 对它返回 null ⇒ `resolveAccount` 说不出「显式选了
    账号 0」· 菜单没有这个选项 · `tabs.ts:2283` 加变体会静默走错分支。⇒ 仍卡 `tabs.ts` 红线。
  - **[待用户跑] 真机验证三步**（本会话红线禁止我起真实已认证的 claude）：
    ① `cc-acct-iso verify` 看是否已报「`$HOME/.claude.json` 又出现了」那条 vwarn
    ② 不选账号 resume 一个远端会话，看是否要求重新登录
    ③ Z01 做完后再 `verify` 一次，看账号 0 是否出现且标 `loggedIn`
    —— **Z01 已交付，③ 现在可以跑了**。本机实况：`~/.claude/.credentials.json` 不存在
    ⇒ 预期 `verify` 报「**账号 0 未登录**（共享库无 cfg 根的 secret 项）」、`list` 末尾多一行 `0`。
- **最近一次计划回看时间**：2026-07-29（Phase A 落盘 + 用户批准）
- **自动模式（/loop）**：**全自动**（用户 2026-07-29「我要全自动把这些需求跑完」）。
  **但本区 Z01/Z02 各需一条外部授权**（见「阻塞」），未获授权时 loop **跳过本区继续跑别的区**，
  不停在这儿空转。
- **本轮 loop 目标**：n/a
- **loop 停止条件**：n/a
- **备注**：
  - **本工作区的立论**：一条守不住的不变量，不如一个能表达它的模型。**吸收 > 检测 > 禁止。**
  - **账号 0 的定义（全局约定）**：**账号 0 ≡「不设 `CLAUDE_CONFIG_DIR`」这个状态本身**。
    凭据在 `~/.claude/.credentials.json`，状态在 `$HOME/.claude.json`。起它 = **什么都不设**
    （不是空串、不是 `~/.claude`）。给它一个 `configDir` 路径就是 cc-acct-iso 已有的 V1
    `--default-in-place`，会引入 `.claude.json` 分裂。
  - **Z01 开工第一件事是实测「空串 vs 未设」**（`cc-acct-iso:682`
    `exec env CLAUDE_CONFIG_DIR="$cfgdir" …`），结果决定共享面 2 的形态。
  - **顺手要收的门禁欠账**：`vendor/cc-acct-iso/scripts/` 纳入 shellcheck（BACKLOG **E13**，
    实测今天零告警）+ `vendor/cc-acct-iso/scripts/test/run-tests.sh`（424 行，工具自己的测试）
    接进门禁 —— **既然要改这个工具，没有网不能改**。
  - 关联：BACKLOG **E1/R16**（history 缺基座逃生口）按本模型**不是遗漏**，Z02 时关掉该登记项。
    BACKLOG **E15**（两渲染器 base 不等价）由 Z02 闭合。

---

## 2026-07-30 追加的三个功能（主计划已修订）

**用户需求原话**：「能不能把账号 0 定义为 claude code 默认登录方式，到时候如果 claude code
换登录方式了换位置了我们可以方便迁移」

| ID | 功能 | 前置 | 为什么 |
|---|---|---|---|
| **Z08** | `isolate <item>` / `share <item>` / `migrate` —— 把某项在共享 ⇄ 私有之间搬（**copy-then-unlink**，绝不用 MOVE） | **G-B** | **一个能力两个需求都要**：① 改了身份声明要能把 z/b 迁过去 ② **BACKLOG E36「API key 路线乙」**（`apiKeyHelper` 进每账号 `settings.json`）直接要它。cc-acct-iso 今天**根本没有**这个能力 |
| **Z06** | 原生身份组成的**单点声明**；`ISOLATE_SET` / `LEGACY_HOME_ITEMS` / `loggedIn` 判据从它派生 | G-B, Z08 | 这份知识今天散在 **6 处**（主计划 §0.1 新表列了行号），改一处漏一处的表现是**静默错** |
| **Z07** | **版本钉 + 漂移检测**（销 BACKLOG **E37**） | Z06 | `CLAUDE_CONFIG_DIR` 零官方文档而整套隔离压在它上面，今天**没有任何东西会响** |

**一处订正**（记在这里免得后人重走）：我起初把现有的账号 0 定义说成「按物理位置的外延定义」
——**说错了**。「账号 0 ≡ 不设 `CLAUDE_CONFIG_DIR` 这个状态本身」本来就是内涵的，
主计划 §0.1 还有专门一段论证为什么**不能**给它 `configDir`（两条路 ⇒ 状态分裂）。**那条不动。**
真正没被抽象的是「原生身份由哪些文件组成」。

**验收边界（写死，别写成做不到的承诺）**：
- **换位置**这一类 ⇒ 一处改完 + 一次迁移。变异验收：声明改成假位置 ⇒ ① `verify` 红 ② migrate 搬得回来
- **换机制**（凭据搬进 OS keyring）⇒ **按目录切身份整体失效，任何抽象都救不了**；
  只承诺 **100% 当场检测到**，绝不静默共用身份（那是最坏失败模式：以为在用 z、实际烧 b 的额度）

## 实际执行路径

```
G-B（建网，无阻塞，可立刻做）
 └─ Z08（迁移能力）─┬─ Z06（单点声明）─ Z07（版本钉+漂移检测，销 E37）
                    └─ BACKLOG E36 API key 路线乙（等用户「可以动了」）
Z01/Z04 可与 Z06 并行（但别同轮改 verify）· Z02 仍卡 tabs.ts 红线 · Z05 独立可插
```

---

## Z08 签收（2026-07-30）

**交付**：新 plan 动词 `ISOLATE`（copy-then-unlink + CAS + 自检 + 回滚）· `cmd_sync` 的隔离项
分支从 `RM` 改成私有化 · `cmd_add` 认 `ISOLATE_SET` · 定向命令 `isolate <项名>`（dry-run 默认）。
上游 ↔ vendored lockstep 完成（`.vendor_id` `e5475b0c140ebbe1` → `740a68b11a71ce27`）。

**开工前实测订正了我自己的记档**：不是「配置说隔离、现实还共享」，而是
**`sync` 把每个账号的软链直接删掉** ⇒ 两个号根本没有该文件（`settings.json` 那就是
hooks/model/permissions/theme 全丢、静默回落默认值）。**Z08 修的是数据丢失。**

**测试 171 → 197**（+26）。三条变异全成立（改回 `RM` ⇒ 3 条红 · 加 `rm 共享库那份` ⇒ 6 条红
含「绝不 MOVE」那条 · 删整组 ⇒ 脚本内地板红）。

**未做，如实登记**：
- **`share <item>`（私有 → 共享，Z08 第二半）** —— 反方向今天只是 `warn`、不丢数据，优先级低
- **没往用户真实 `~/.claude-accts/` 落地任何迁移**（z/b 的 `settings.json` 仍是 07-26 那两个
  软链、`accounts.json` mtime 未变、`ISOLATE_SET` 仍是默认值）。上游改动已备份在
  `scratchpad/z08-upstream-backup/`（6 文件）可整体回退

**解锁**：BACKLOG **E36「API key 路线乙」**的技术前置就位。落地两步、**性质不同**：
① 把 `settings.json` 加进 `ISOLATE_SET` 再 `sync --apply` ——**对在用的会话不可观测**
（同目录 `mv` 原子替换、内容逐字节相同）② 往目标账号那份里写 `apiKeyHelper` + `env`
——**这才是真内容变更，要等用户不在用那个号**。

---

## Z06 签收（2026-07-30）

**交付**：`NATIVE_IDENTITY` 声明表 + 四个投影；`ISOLATE_SET` / `LEGACY_HOME_ITEMS` /
`chmod 600` 目标从它派生（派生结果与历史字面量**逐字相同**）；**跨语言双写点守卫**钉住
daemon `accounts_query.rs:407` 那处独立的 `loggedIn` 判定。lockstep 完成
（`.vendor_id` `740a68b11a71ce27` → `15b1ca8ccfb7c9c1`）。测试 197 → **215**，daemon 140 → **141**。

**开工前复测纠正了计划**：`accounts.rs:49` 只是注释不是决策点；bash 侧 ~18 处字面量绝大多数
是正当具体用途或用户文案 ⇒ **守卫从「字面量零出现」改成「跨语言双写点」**。

**单点声明的第一个收益**：两处此前不可见的不一致自己浮出来（`init`/`sync` 都只 chmod
`.credentials.json`，`.claude.json` 的权限没人管）—— 两条都修了，第三条（`verify` 硬失败
范围）刻意不改（会给在野环境突然的新红）。

**未做，如实登记**：
- **`mcp.rs` 的 `.claude.json` 三候选那条双写点未钉**（Z06 第二半）。它比凭据那条微妙：
  `mcp.rs` 是**防御性**列三个候选，与声明不是简单相等关系，钉之前要先想清断言什么
- `verify` 的硬失败范围未收紧
- 用户真实 `~/.claude-accts/` **零改动**（z/b 的 `settings.json` 仍是 07-26 那两个软链、
  `accounts.json` mtime 未变）。上游备份在 `scratchpad/z06-upstream-backup/`

**两个坑，记在这里免得后人重踩**：
1. `set -o pipefail` 下 `while … cond && printf … done | sed`：**最后一行条件为假就让整条
   管线失败** ⇒ `set -e` 就地退出。现象极迷惑（派生值全对、却在赋值后死掉，197 条红 115 条）
2. `VENDOR.md` 菜谱的 `cp -a` **保留 mtime** ⇒ re-vendor 后 cargo 可能不重编译
   `include_str!` ⇒ **守卫报陈旧结果**。CI 干净 checkout 不受影响；**本地 re-vendor 后要
   `touch` 引用它的 Rust 文件**

---

## Z07 签收（2026-07-30）—— BACKLOG E37 已销

**交付**：只读版本探测（解析 launcher 路径里的 `.../versions/<semver>`，纯 `readlink`；
回落 `.last-update-result.json`；探不到就明说跳过。**绝不执行 `claude --version`**）+
manifest additive `claudeVersionPinned` + `verify` 四条检测。测试 215 → **231**，
`.vendor_id` `15b1ca8ccfb7c9c1` → `3416ab2260e55d74`。

**一处自己犯的错，被既有套件当场接住**：D1「声明项哪儿都找不到」初版写成 vfail，沙盒误报
4 条——`policy-limits.json` 这类是**懒创建**的，「还没创建」与「改了位置」**不可判定**。
降级 vwarn；致命档换成 **D1b**（secret 泄漏进共享库，零误报）。

**成功标准 8 的达成范围已如实限定**：「绝不静默」达成；但「改成假位置 ⇒ verify 当场红」
要限定为「报提示」，真正的 FAIL 需要 secret 真的泄漏。

**给 Z01 的硬提醒**：账号 0 的 config dir **就是**共享库 ⇒ **D1b 对它必须有例外**，
否则 Z01 一登记账号 0，`verify` 就会对它恒报「secret 泄漏进共享库」。**别直接沿用。**
