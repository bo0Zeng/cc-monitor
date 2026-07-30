# Z06 — 原生身份组成的单点声明（`NATIVE_IDENTITY`）

> 主计划：`../MASTERPLAN.md` §0.1 那条追加需求 + §3 账本第 9 行
> 前置：**G-B**（`0b297ed`）· **Z08**（`60946b0`）· 后继：Z07（版本钉 + 漂移检测，销 E37）
>
> 用户需求原话：「能不能把账号 0 定义为 claude code 默认登录方式，到时候如果 claude code
> 换登录方式了换位置了我们可以方便迁移」

## 1. 开工前复测，把计划里的「6 处」纠正了

计划列的是 6 处，且把 `accounts.rs:49` 当成决策点。**逐处实测后，实际情况不同**：

| 计划说的 | 实测 |
|---|---|
| `accounts.rs:49` 判 `loggedIn` | **只是文档注释**；`:51` 只是 `serde` 反序列化。**不是决策点** |
| —（计划没提） | **daemon `accounts_query.rs:407`** 才是第二处独立判定：`"loggedIn": dir.join(".credentials.json").exists()` |
| bash 侧「6 处之一」 | bash 侧那几个文件名字面量实际有 **~18 处**，但**绝大多数是正当的具体用途**（种这个文件、chmod 那个文件）或**用户可见文案**（usage / warn / info 的散文） |

**真正被独立实现了两遍的判定只有两条**：

| 知识 | 处 1 | 处 2 | 跨越 |
|---|---|---|---|
| 「有 `.credentials.json` = 已登录」 | bash `cc-acct-iso`（`logged=false; [ -f … ] && logged=true`） | **daemon `accounts_query.rs:407`** | **两种语言、两个进程** |
| 「`.claude.json` 住哪」 | bash `LEGACY_HOME_ITEMS` + 2 处用法 | **`mcp.rs` 的三个候选路径** | 同上 |

⇒ **设计据此改了**：目标**不是**「消灭每一处字面量」——那会把
`plan_add SEED "$seed" "$cfgdir/$(ni_item claude_json)"` 这种改得更难读，是机械 churn。
目标是**让三个集合有唯一来源**，外加**跨语言的双写点用守卫钉住**（本仓对
`TMUX_LS_FMT` 就是这么处理的，有现成范式）。

## 2. DoD

- [x] `NATIVE_IDENTITY` 声明表（项名 : 原生根 : 类别）
- [x] `ISOLATE_SET` / `LEGACY_HOME_ITEMS` **从它派生**，派生结果与历史字面量**逐字相同**
- [x] `chmod 600` 的目标从 `ni_secrets` 派生（init + sync 两处）
- [x] **跨语言双写点守卫**：daemon 判 `loggedIn` 用的文件名必须与声明一致，双向即红
- [x] 上游 ↔ vendored lockstep + `.vendor_id` 重算
- [x] 变异验收（改声明 ⇒ 两侧都红）
- [x] G-B 两道网绿 + 全门禁数字不降
- [x] 用户真实 `~/.claude-accts/` 零改动

**明确不做**：
- **不消灭那些"正当具体用途"的字面量**（理由见 §1）
- **不收紧 `verify` 的硬失败范围**（见 §5，会给在野环境一个突然的新红）
- **`mcp.rs` 的 `.claude.json` 三候选那条双写点未钉**（Z06 第二半，见 §6.3）

## 3. 声明与派生

```bash
NATIVE_IDENTITY="\
.credentials.json:cfg:secret
.claude.json:home:secret
backups:cfg:derived
policy-limits.json:cfg:state
stats-cache.json:cfg:state"
```

- **原生根** `cfg` = 未迁移时住 `<config dir>/<项名>`（跟 `CLAUDE_CONFIG_DIR` 走）·
  `home` = 住 `$HOME/<项名>`（**不跟**它走，经典位置）
- **类别** `secret` 身份本体（必须 600、必须隔离、**绝不从别的账号复制**）·
  `state` 本机状态 · `derived` 附属物

四个投影：`ni_items` / `ni_isolate_default` / `ni_home_rooted` / `ni_secrets` + `ni_is_secret`。
**实测派生结果与历史字面量逐字相同**（有断言钉住）：

```
ISOLATE_SET       派生 = .credentials.json .claude.json backups policy-limits.json stats-cache.json  ✓
LEGACY_HOME_ITEMS 派生 = .claude.json                                                                ✓
secrets           派生 = .credentials.json .claude.json
```

**覆盖语义逐字不变**：`${ISOLATE_SET:-$(ni_isolate_default)}` —— 环境变量/配置文件照旧优先。

## 4. 一个把 197 条里 115 条打红的坑（值得单独记）

三个投影初版写成 `while IFS=: read …; do cond && printf …; done | sed`。
本脚本开了 **`set -euo pipefail`**：

> 最后一行的条件为假 ⇒ `while` 以 **1** 退出 ⇒ **`pipefail` 把整条管线判失败**
> ⇒ `set -e` 在赋值之后**就地退出**。

现象极具迷惑性：`bash -x` 里看到 **派生出的值全对**（`ISOLATE_SET=…` 打印得一字不差），
脚本却在下一步死掉，197 条红了 115 条。

改成 `if … fi`（条件为假且无 else 时返回 0）。**有三条断言专门钉住**
「三个投影在 `set -euo pipefail` 下 rc=0」。

## 5. 单点声明的第一个收益：两处不一致自己浮出来了

收成一处之后，立刻看见两处此前不可见的不一致：

| # | 不一致 | 本轮处置 |
|---|---|---|
| 1 | **`init` 的 MOVE 路径只 chmod 600 了 `.credentials.json`**，`.claude.json`（同样含 `oauthAccount`、`--seed-claude-json` 那条路确实 chmod 了）搬进来后保持原模式 | **修了**（从 `ni_secrets` 派生）。不修的话下面第 2 条会让 `sync` 不收敛 |
| 2 | **`sync` 的权限修复只管 `.credentials.json`** ⇒ `.claude.json` 的权限漂了再没人修 | **修了**（从 `ni_secrets` 派生）。有断言：改成 644 后 `sync --apply` 会修回 600，且修完再跑 `sync` 报「无需改动」 |
| 3 | **`verify` 的硬失败也只针对 `.credentials.json`** | **刻意不改**：在野环境里 `.claude.json` 若是 644，收紧后会给用户一个**突然的新红**。风险不对称 ⇒ 登记不改 |

第 1、2 条**是行为改动**（DoD 原写「零行为改动」）。修它们的理由：不修则
`sync` 第一次跑必然产生一次性权限修复 ⇒ **不收敛**，直接打红既有的
「3 次 sync 后备份目录数没增长」那条断言（实测就是这么发现的）。
所以这不是顺手扩范围，是**派生本身逼出来的自洽要求**。

## 6. 变异验收（Phase D）

**强度：中高风险**（改多账号核心契约的来源）⇒ 变异 + G-B 两道网 + 全门禁。
**如实标注**：planned-build 铁律 8 要求并行多 agent 审计；本会话有常驻指令「除非用户要求
不开 agent」⇒ 主线程变异代替。**这是欠了铁律 8 的账，不是强度裁剪。**

### 6.1 变异

| 变异 | 做了什么 | 结果 |
|---|---|---|
| **A** | 把声明里的 `.credentials.json:cfg:secret` 改成 `.creds-v2.json:cfg:secret`（模拟 Claude Code 换了凭据文件名、只改一边） | **双向成立**：① Rust 侧双写点守卫红，诊断「Z06 双写点漂移：…声明里找不到 `.credentials.json:cfg:secret`」② bash 侧行为断言红 4 条（`共享库里已无凭据` / `凭据权限 600` / `verify 退出码 0` / `输出含 PASS`） |

每次都先 `bash -n` 确认语法过（判色三步②）。

### 6.2 ★ 一个会让守卫报**陈旧结果**的坑（本轮踩到并记档）

Rust 侧守卫用 `include_str!` 读 **vendored** 的 `lib.sh`。而 `VENDOR.md` 的 re-vendor 菜谱用
**`cp -a`**——**它保留源文件的 mtime**。于是 vendored 文件的 mtime 可能**比上次构建还旧**
⇒ cargo 的指纹判定「没变」⇒ **`include_str!` 不重编译** ⇒ **守卫报的是上一次的结果**。

本轮实测：re-vendor 之后守卫仍红（读的是旧内容），`touch` 一下引用它的 Rust 文件才变绿。

- **CI 不受影响**（干净 checkout，一切都是新的）
- **本地 re-vendor 后必须 `touch` 引用它的 Rust 文件**（或 `cargo clean -p`），否则判色无效
- 这条已写进本文件；`TMUX_LS_FMT` 那条既有守卫不受影响（它读的是**就地编辑**的
  `watcher.rs`，mtime 必然更新）

### 6.3 测试规模与守卫

- vendored 自测 **197 → 215**（+18）：派生结果逐字对拍 · 五项计数 · `ni_is_secret` 四态 ·
  **三个投影在 `pipefail` 下 rc=0** · 覆盖语义仍生效 · init/sync 的 secret 权限收敛
- daemon **140 → 141**：跨语言双写点守卫
- 两道地板同步上调：脚本内 `MIN_ASSERTS` 197 → **215**；`ci.yml` 双保险 197 → **215**

## 7. 工程审计（Phase E）

### 7.1 账本对账

| 账本行 | 本功能做了什么 | 状态 |
|---|---|---|
| **9 原生身份组成的声明** | 声明落地；`ISOLATE_SET`/`LEGACY_HOME_ITEMS`/`chmod 600` 目标三者派生；**守卫改成跨语言双写点**（原计划写的是「断言那些文件名在声明之外零出现」——实测那**做不到也不该做**，见 §1） | ✅ 主体到位，守卫形态已订正 |
| **7 上游 ↔ vendored lockstep** | 同步 6 文件 + `.vendor_id` `740a68b11a71ce27` → `15b1ca8ccfb7c9c1`；逐个 `diff -q` 一致；`build.rs` 不 warn | ✅ |
| 10 `isolate`/`share` | 未触及（Z08 已交付 `isolate`；`share` 仍是第二半） | — |

### 7.2 守卫范围 vs 性质，如实记

- 钉住的是**「daemon 判 `loggedIn` 用的文件名」与「声明里的凭据项」一致**。
- **没钉住**：第三个地方再写一份（比如将来某个新模块自己 stat 一次）。
  守卫**范围窄于性质**，不假装它是严格证明。
- **`mcp.rs` 的 `.claude.json` 三候选未钉**（Z06 第二半）。它比凭据那条更微妙：
  `mcp.rs` 是**防御性地**列三个候选路径（`$CLAUDE_CONFIG_DIR/`、其父目录、`$HOME/`），
  而声明只说「原生根是 home」。两者不是简单相等关系，钉之前要先想清断言什么。

### 7.3 对后续的影响

- **Z07（版本钉 + 漂移检测，销 E37）现在有对象了**：声明就是那个要绑版本、要拿去和实际
  布局对拍的东西。没有 Z06，Z07 无从谈起。
- **Z01/Z04 不受影响**（未碰 manifest 数据模型；`verify` 的硬失败判据刻意没动）。
- **给「换位置」那条路的实际操作**：改这张表 → `cc-acct-iso sync --apply`（Z08 的 `ISOLATE`
  把现实迁过去）。**换机制（keyring）仍然救不了**，那是 Z07 要「当场检测到」的。

### 7.4 本轮**没有**做的事

- 用户真实 `~/.claude-accts/` **零改动**（收尾逐条核过）
- `verify` 的硬失败范围未收紧（§5 第 3 条）
- `mcp.rs` 那条双写点未钉（§7.2）
- 上游改动已备份 `scratchpad/z06-upstream-backup/`（6 文件）可整体回退

## 8. 签收

- [x] 通过代码审计（变异双向成立 + 既有 197 条先验不破 + G-B 两道网 + 全门禁）
- [x] 通过工程审计（账本第 9 行主体到位并订正守卫形态、第 7 行 lockstep 完成、两处不一致如实处置）
- [x] 主计划已据此更新
