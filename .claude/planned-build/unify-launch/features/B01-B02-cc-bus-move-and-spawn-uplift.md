# 功能计划 — B01 cc-bus 搬进仓 + B02 `cc-spawn` 收编

> 两件事写一份计划：B02 必须建立在 B01 之上（文件先进仓才能改），
> 且 B01 的"原样固化"约束正是为了让 B02 的 diff 干净可审。

## 0. 定性（用户 2026-07-28 拍板，决定了它为什么在 unify-launch 而不在 integrate-toolchain）

> cc-bus **不是"又一个要集成的工具"，它是「起会话被写死成 N 套实现」的又一个病灶**——
> `cc-spawn` 内部自己 `tmux new-session` + 送环境 + 送任务。

这正是本工作区账本 `~/.local/bin/cc-spawn` 那一行**未完成的另一半**：
F11 只上提了「预信任」能力进 `ccm`，本体（建会话/送环境/送任务改经 `ccm`）当时因
"文件在仓外、需用户另行授权"收窄。用户现已授权 + 要求搬进仓。

## 1. B01：搬进仓，**原样固化**

### 盘面事实（已核实）
`~/.claude/skills/cc-bus/`：13 个脚本 / **1118 行** / `cc-spawn` 136 行 / `SKILL.md` / `examples/`。
备份物证：**2 个 `scripts.bak-*` 目录 + 2 个 `.bak` 文件**（`cc-whoami.bak-*` / `cc-spawn.bak-*`）
——一直手改、无版本管理。**（订正账本原记的"3 个目录"，实为 2 个；结论不变。）**

### 1.1 「原样搬不会搬坏」已独立核实（B01 最大的隐性风险，实测排除）

搬家最常见的坏法是脚本里硬编码了自身安装路径。**实测：一处都没有。**
13 个脚本**全部**用同一个惯例解析同目录兄弟：

```sh
SELFDIR=$(cd "$(dirname "$(readlink -f "$0")")" && pwd)
```

（`cc-recv:5` / `cc-register:5` / `cc-spawn:7` / `cc-bus-stop-hook:5` / `cc-send:5` /
`cc-broadcast:6` / `cc-busd:11` …；`cc-bus-install.sh:6` 用同款 `SRC=`。）
全仓 grep `~/.claude/skills` 在脚本里**零命中**。

两条推论：
1. **搬到 `shared/cc-bus/scripts/` 零路径改写**——`SELFDIR` 是运行期相对自身解析的。
   这正是 B01 能承诺"逐字节原样搬"的前提；若有硬编码路径，"原样搬"与"能跑"就会矛盾。
2. `readlink -f` **解析 symlink**，所以脚本经 `~/.local/bin/<name>` 这些软链调用时，
   `SELFDIR` 仍指向真实脚本目录 → 装机形态（软链）与仓内形态（真身）行为一致。
   `cc-bus-install.sh` 另有 `CCBUS_BINDIR`/`CC_BUS_HOME` 两个可覆盖变量（供测试），
   B02 的 e2e 正好用它们把副作用关进临时目录。

### DoD
- [ ] 落 `shared/cc-bus/`（与 `shared/ccm` 并列——两者同类：都是"部署到本机/远端的 shell 产物"）
- [ ] **逐字节原样搬**：`git add` 后 `diff -r` 盘上版本与仓内版本必须零差异
      （**不趁搬家重构、不改一个空格**。理由：这份代码一直手改无版本管理，
      搬家 diff 与重构 diff 混在一起将无法审——而它是会 `send-keys` 进别人 CC 的东西）
- [ ] **不搬** `scripts.bak-*` / `*.bak`（那是手改的历史残留，进 git 之后 git 本身就是备份）
- [ ] 部署路径走「备份→写→读回比对→回滚」（同 `acct_iso_deploy.rs` 既有范式，显式 `0o755`）
- [ ] **`shared/cc-bus/scripts/*` 必须带可执行位进 git**——R00 刚踩过 `shared/ccm` 记成 644
      的坑（干净 checkout 上不可执行）。搬完立刻 `git ls-files -s` 核对模式为 `100755`
- [ ] 覆盖用户盘上那份需要**用户显式点一次**，不在安装期静默覆盖（守红线）

## 2. B02：`cc-spawn` 收编——**这是成功标准② 的第二次真实架构验收**

### 2.1 三段靶心（已定位到行）

| 段 | 位置 | 处置 |
|---|---|---|
| 建会话 | `:100` `tmux new-session -d -s "$name" -c "$absdir" **-x 220 -y 50**` | 改经 `ccm --tmux=<name> --cwd <absdir>` |
| 送环境 | `:108-109` `inj=""`；**仅 codex** `inj="CC_BUS_ID=<name> "` | 改经 `ccm` 的新维度（见 2.2） |
| 送任务 | `:111/:113` `send-keys "${inj}$LAUNCH $(printf '%q' "$task")"` | 改经 `ccm … -- "<task>"`（**能力已有**） |
| 预信任 | `:51-96` | **删**——F11 已上提进 `ccm`（claude 侧逐行对齐；codex 侧 `config.toml` 同理） |
| 信任框轮询兜底 | `:122-133` | **删**——F11 已上提（同为 claude-only，语义一致） |

**保留在 `cc-spawn` 的（cc-bus 专属，不属于"起会话"）**：
`--new` / 复用判定（`:37-45`）、会话命名与撞名避让（`:33-49`）、`cc-register` 登记（`:117`）、
台账 `spawned.tsv`（`:120`）、收尾提示。

### 2.2 `CC_BUS_ID`：本功能的架构验收点

**已核实：`ccm` 今天已支持 `--` 透传**，故"送任务"不需要新能力
（实测 `ccm --tmux --cwd /p --print -- "分析架构"` 的内层载荷确实带 `'--' '分析架构'`，
且 `e2e/ccm-cli.test.sh:58` 早有覆盖）。**订正我此前把"给 ccm 加透传能力"列为 B02 一项——那是错的。**

真正缺的是 `CC_BUS_ID`。今天 `cc-spawn` 把它作为**env 前缀塞在 send-keys 载荷里**
（`inj=` 拼在命令前），所以它落在 tmux 边界**内侧**、现在是对的。
但若天真地把建会话换成 `ccm --tmux`，`CC_BUS_ID` 就只能靠环境继承——
而 `update-environment` 默认列表同样不含它 → **会被整个吃掉**（R08 完全同型）。

**→ 给 `ccm` 加一个新维度（如 `--bus-id <id>`）。这正好验收成功标准②**：
注册一条 dimension + CLI 加一个 flag + `LaunchModifiers` 加一个字段，
**零改** 9 个函数签名与 6 个透传调用点（R03 刚把这条路铺好）。
若届时发现仍要改这些地方，说明 R03 没做到位，回炉。

**两条必须尊重的既有设计（`cc-spawn:104-107` 的注释写得很清楚，不是随手写的）**：
1. **`CC_BUS_ID` 只对 codex 注入，claude 刻意不设。** 理由：codex 模型的沙箱 shell 工具
   被 Landlock/seccomp 挡着、够不到 tmux 服务端 socket，那里 `cc-whoami` 无法靠 tmux 解析身份，
   而 `CC_BUS_ID` 是它最高优先级、不依赖 tmux 的来源，且**环境变量能透进沙箱**；
   claude 的 shell 工具能用 tmux，强设 `CC_BUS_ID` 会**盖掉** `@cc_id` pane 标签
   （那是用来细分"同一会话里多个 CC"的）。
   → 新维度的 `applies` 必须是**条件式**（只在 agent=codex 且给了 bus-id 时触发），
   判断依据正是 INVARIANTS §37「看这个维度不触发时的沉默是否等价于用户期望」——
   这里沉默 = claude 不设 = **正是期望**。并按 R04② 用 `requiredCaps` 声明能力，
   不要塞进静态 `CLI_REQUIRED_CAPS`（那会误伤所有非 codex 调用）。
2. **值必须到达 agent 进程的环境**（不只是外层 shell），因为要透进沙箱。

### 2.3 另两处不能默默丢的细节

- **`-x 220 -y 50`**：detached tmux 会话默认 80x24，`cc-spawn` 刻意放宽免得 agent 输出被窄折。
  `ccm` 不设窗口尺寸。→ 要么给 `ccm --tmux` 补上（属容器轴细节，**不是**新维度），
  要么显式记录接受这个行为变化。**不能默默丢**——这类"看起来无害的细节"正是 F02 真机测试
  揪出净退化的那一类。
- **会话命名不同**：`cc-spawn` 用 `<basename>_cc`，`ccm` 的 `deriveTmuxName` 用 `cc-<safe-basename>`。
  cc-spawn 的名字**必须保持**（`cc-whoami` 的 resolve 与它对齐，现存会话与 `spawned.tsv` 台账都用它）。
  → 走 `ccm --tmux=<显式名>` 传入，不让 ccm 自己派生。
  `ccm` 的 `--tmux` **不做撞名避让**（MASTERPLAN §2.2 明写：避让属于"灰会话 fresh resume"、
  由调用方显式传名）——正好与 cc-spawn 自己保留避让逻辑吻合。
- **顺带的好处（值得记，接 B03）**：cc-spawn 的 `foo_cc` 会话经 `ccm` 起来后会被打上
  `@ccm_sid_expect`，而 F04 已把 Gate 2 改成 `@ccm_sid` ∪ `cc-*` 的 **union**
  ——于是这些 cc-bus 会话**第一次变得对 cc-monitor 可见、可控**（此前名字不合 `cc-` 前缀、
  又没有身份标记，控制面完全够不着）。这是 B03 驾驶舱的地基，不是附带效果。

### 2.4 「删 cc-spawn 的预信任」这一步已独立核实过等价性（不是假设）

计划 §2.1 表里写「预信任 :51-96 与轮询兜底 :122-133 → **删**，F11 已上提」。
删之前必须确认两边真的等价，否则是回归。**已逐条对拍**（2026-07-28）：

| 要素 | `cc-spawn` | `shared/ccm` | 结论 |
|---|---|---|---|
| claude：`jq` 读改 `hasTrustDialogAccepted` + 备份 + 校验后 `mv` | `:55-67` | 有 | 等价 |
| codex：路径含**控制字符**时跳过（否则破坏 TOML） | `:77` | `:417` | 等价 |
| codex：TOML basic-string 转义（先 `\` 再 `"`） | `:82` | `:420` | 等价 |
| codex：`flock` 串行化 + 幂等 `grep -qF` 防重复 stanza | `:83-84` | `:421-422` | 等价 |
| 轮询兜底（抓信任框文本自动按 Enter，仅 claude） | `:124-133` | 有 | 等价 |
| **绝对路径规范化** | `absdir` 恒经 `cd && pwd` | `cwd_abs` 专用规范化（F11 Phase D 阻塞项修复） | 等价 |
| `CCM_NO_PRETRUST` 逃生口 | **无** | **有** | **有意差异**（见下） |

两处需要写进 commit message 的说明：

1. **`CCM_NO_PRETRUST` 是 `ccm` 独有的**，`shared/ccm` 的注释已明确标注这是有意差异
   （受众判断不同：`ccm --tmux` 是 cc-monitor 主 UI 的默认实现，受众比 cc-spawn 这类窄众协作
   工具宽得多，不该照搬一个无退出口的行为）。→ B02 之后 `cc-spawn` 的用户**顺带获得**这个逃生口，
   是行为变化但方向是增强，需在 commit 里说明。
2. **B02 顺带消灭一处跨仓重复**：`shared/ccm` 里那段注释写着"这段逻辑与 cc-spawn 里的同名实现是
   **有意的代码重复**……若未来在 cc-spawn 那边修了 bug，这里要手动跟着补一遍——不会自动同步，
   也没有测试能跨仓库检测这种漂移"。B02 让 `cc-spawn` 改经 `ccm` 之后，
   **这个维护危险直接消失**（只剩一份实现）。这条收益原计划没写，补记于此。

## 3. 测试策略

- **B01 的验收就是"零差异"**：`diff -r ~/.claude/skills/cc-bus/scripts shared/cc-bus/scripts`
  （排除 `.bak`）必须为空；`git ls-files -s shared/cc-bus/scripts` 全为 `100755`。
- **B02 必须有真机 e2e**（教训清单第 1 条：门禁只锁字符串形状不锁行为）：
  新增 `e2e/cc-spawn-uplift.sh`，隔离 `-L` socket + 假 launcher，验：
  ① 会话真的建出来且名字是 `<basename>_cc`（不是 `cc-<name>`）；
  ② **codex 路径下 `CC_BUS_ID` 真的到达了 agent 进程**（假 launcher 落盘自己看到的 env）
  ——这是本功能的核心断言，等价于 `ccm-acceptance` 场景1 对 `CLAUDE_CONFIG_DIR` 做的事；
  ③ claude 路径下 `CC_BUS_ID` **未设**（负向断言，守 2.2 的设计）；
  ④ 初始任务经 `--` 透传到达；⑤ 复用路径（同目录第二次）不新建、把任务发给已有会话；
  ⑥ 窗口尺寸符合 2.3 的决定。
  **等待用固定 sleep 会在慢机器上假红**——照 R00 的做法用轮询（`wait_grep`/`wait_any_session`）。
- **不启动真实 claude/codex**（红线）：用 `CCSPAWN_LAUNCH` 这个既有可测性钩子指向假 launcher。
- `ccm-print-parity` 加一条场景覆盖新维度的渲染（它是对 `shared/ccm` 的外部预言机）。

## 4. 风险

| # | 风险 | 缓解 |
|---|---|---|
| K1 | 改 `cc-spawn` 会影响**正在运行的**协作 agent（这台机上就住着 cc-bus 实例） | 只改仓内副本；部署到 `~/.claude/skills/` 需用户显式点。e2e 一律隔离 `-L` socket，**绝不碰默认 socket**（R03 审计 agent 也主动避开了这条） |
| K2 | `CC_BUS_ID` 维度做成恒真 → 误伤 claude 路径、盖掉 `@cc_id` | `applies` 条件式 + 负向 e2e 断言（3-③）钉住 |
| K3 | 搬家 diff 与重构 diff 混淆 | B01 强制"零差异"，B02 单独 commit |
| K4 | `ccm` 不做撞名避让，而 cc-spawn 依赖避让 | 避让逻辑留在 cc-spawn，传显式 `--tmux=<名>` |

## 5. 代码审计结果（Phase D）
（待填）

## 6. 工程审计结果（Phase E）
（待填）

## 7. 签收
- [ ] 通过代码审计（无阻塞项）
- [ ] 通过工程审计
- [ ] 主计划已据此更新（含变更记录）
