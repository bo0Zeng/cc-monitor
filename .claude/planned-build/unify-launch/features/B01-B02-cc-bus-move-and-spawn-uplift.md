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
`~/.claude/skills/cc-bus/`：13 个脚本 / **951 行**（全部 17 个文件合计 1107 行）/
`cc-spawn` 136 行 / `SKILL.md` / `examples/`（3 个）。
（原写 "1118 行" 有误，且挂在"已核实"标题下——B01 审计 S6 实测纠正为 951/1107。）
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
- [x] 落 `shared/cc-bus/`（与 `shared/ccm` 并列——两者同类：都是"部署到本机/远端的 shell 产物"）
- [x] **逐字节原样搬**：`git add` 后 `diff -r` 盘上版本与仓内版本必须零差异
      （**不趁搬家重构、不改一个空格**。理由：这份代码一直手改无版本管理，
      搬家 diff 与重构 diff 混在一起将无法审——而它是会 `send-keys` 进别人 CC 的东西）
- [x] **不搬** `scripts.bak-*` / `*.bak`（那是手改的历史残留，进 git 之后 git 本身就是备份）
- [ ] 部署路径走「备份→写→读回比对→回滚」（同 `acct_iso_deploy.rs` 既有范式，显式 `0o755`）
- [x] **`shared/cc-bus/scripts/*` 必须带可执行位进 git**——R00 刚踩过 `shared/ccm` 记成 644
      的坑（干净 checkout 上不可执行）。搬完立刻 `git ls-files -s` 核对模式为 `100755`
- [ ] 覆盖用户盘上那份需要**用户显式点一次**，不在安装期静默覆盖（守红线）

### 1.2 B01 实施记录（2026-07-28）

**验收三条全过**：
1. `diff -r`（排除备份物）盘上 vs 仓内 **零差异** —— 逐字节原样，未改一个空格。
2. `git ls-files -s shared/cc-bus/scripts` **全部 100755**；`examples/*` 与 `SKILL.md` 保持 644（正确，它们不是可执行文件）。
3. 干净 checkout 实测：脚本可执行，`cc-spawn` 无参调用正确报用法。

**根因坐实并结构化防住**：`git config core.fileMode` = **`false`** ——git **忽略文件系统的可执行位**。
所以 13 个脚本盘上是 755、`git add` 之后**全部被记成 100644**。
这与 R00 那次 `shared/ccm` 记成 644 是**同一个根因**，不是两次巧合。
最恶劣的地方在于**本地永远看不出来**（本地跑的是盘上那份），只有干净 checkout 才炸——
R00 那次为此让 e2e 在 CI 上连红三轮，最后靠 pane dump 才定位到。

→ 新增 `e2e/exec-bit-guard.sh` + CI `e2e-smoke` job 一步：
**`shared/**` 下任何带 shebang 的文件，在 git 里必须是 100755**。
变异验证：把 `cc-spawn` 退回 644 → 守卫 exit 1 并指名道姓给出修法；还原 → exit 0。
它同时覆盖了 `shared/ccm`（现为 100755）。**从此这条不再依赖"记得 `git update-index --chmod=+x`"。**

**未搬的**：2 个 `scripts.bak-*` 目录 + 2 个 `.bak` 文件（手改历史残留，进 git 后 git 本身就是备份）。
**未做的**：部署机制（cc-monitor 侧覆盖用户盘上那份）留给 B02 之后——B01 只固化基线。

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

独立对抗性审计（2026-07-28，xhigh，48 次工具调用）。**结论：无阻塞项；搬家本身无懈可击，
全部发现集中在我为它新写的 37 行 `e2e/exec-bit-guard.sh` 上。**

审计的独立复核比我原先的自证更强：它不是比工作树，而是把**提交对象里的 blob** 逐一
`git cat-file blob <sha> | cmp - <盘上文件>`，17/17 全同——这排除了"工作树对但 index/tree 错"
这一类 `diff -r` 根本看不见的失败。`diff -r` 的 5 类盲区（权限位/隐藏文件/空目录/symlink/
行尾与编码）它逐项查过：隐藏文件 0、symlink 0、空目录 0、CRLF 0、非 UTF-8 0，17/17 以 `\n` 结尾，
`git check-ignore` 全部无命中。备份物（2 个 `scripts.bak-*` + 2 个 `.bak-*` 文件）确认未进仓。
它还用**运行时**证明了 §1.1 的 `SELFDIR` 论断（隔离 `CC_BUS_HOME` 跑通仓内那份的
`cc-send`→`cc-recv` 全链路），并在结束时重跑红线验收：`diff -r` 零差异、盘上那份未被触碰。

| # | 发现 | 处置 |
|---|---|---|
| **I1** | 守卫**两处静默恒绿**（实测复现）：既不校验 `git ls-files` 的 rc，也不校验受检文件数。`shared/` 改名 → 恒绿并打印"全部 100755"；CI 上 git 因任何原因失败（dubious ownership 等）→ 同样恒绿。**这才是"守卫变摆设"的真实入口** | **已修**。git rc 落地 + 受检数自检。**并且自查发现审计的修法还不够**：只统计总数时，白名单那 2 条恒在，会把"shared/ 整个消失"盖成绿的（实测 rc=0）——故改为**按作用域分开计数**，`shared/` 必须自证非 0 |
| **I2** | 非 ASCII 文件名被**静默跳过**：`git ls-files` 默认 `core.quotePath=true`，中文名输出成 C 转义 → `head -c 2 "$f"` 找不到文件 → `\|\| continue`。本仓工作目录就叫「文档/」，非理论风险 | **已修**：`git ls-files -z` + `read -r -d ''` |
| **I3** | `shared/ccm:376-379` 被这次搬家**改成了假话**（"分属不同仓库/不在这个仓库里/没有测试能跨仓检测漂移"三句全错），而 §2.4 正引用它当 B02 的收益论据 | **已修**。改它不污染搬家 diff——"不改一个空格"的作用域是**被搬的代码**，`shared/ccm` 是被这次搬家改变了事实前提的既有代码 |
| **I4** | `cc-bus-install.sh:74/83/87` 引用 `examples/codex-hooks.json`、`examples/codex-config.toml`——**两个文件盘上和仓里都不存在**；`:70-73` 宣称的 `.codex-plugin/plugin.json` 同样不存在。Codex 激活路径照做会撞空 | **登记不修**（§6 遗留清单）。这是搬进来**之前就存在**的缺陷，按"逐字节原样搬"不该趁机修 |
| **I5** | 新进仓的 1107 行 shell **零 lint 覆盖** | **已修**：CI shellcheck 扩到 `shared/cc-bus/scripts/* shared/ccm`，实测零告警。**审计的范围建议我收窄了**：它顺带提的 `shared/ccm-aliases.sh` 实测 SC2148 红（无 shebang 的 source 片段），而该文件会写进用户 shell profile 并在 UI 展示供复制，塞 lint 指令等于往用户配置掺噪音 → 显式排除并写明理由 |
| **I6** | `e2e/fake-claude`、`e2e/daemon-wrapper.sh` 是**同一失效类**（被直接 exec、退回 644 只有 CI 才炸）却在守卫 glob 之外；但放宽到 `e2e/` 会误伤约 20 个**故意** 644、一律经 `bash "$X"` 调用的脚本 | **已修**：显式 `ALLOWLIST`，不放宽 glob |
| S1 | symlink（120000）会触发**不可满足的 FAIL**——给出的 `--chmod=+x` 对 symlink 无效 | **已修**：显式 SKIP 并说明，不给无效修法 |
| S3 | 判据取自工作树、断言取自 index，两者不一致时（sparse-checkout）静默失效 | **已修**：判据改从 index 取（`git cat-file blob :path`） |
| S2 | 无 shebang 的二进制被静默跳过（本仓有内嵌 daemon 二进制的历史） | 登记。当前 `shared/` 无二进制 |
| S4 | `SKILL.md` 含机器特定内容（`aya`、仓外私人文档路径、Obsidian wikilink、写死 `~/.claude/skills/...` 的安装命令，干净 clone 里是错的）。**审计逐文件扫过：无任何密钥/凭证/token** | 登记，进 B02+ 清单 |
| S5 | `cc-bus-install.sh:21/32` 会 `chmod +x "$SRC/$s"`——源目录一旦是仓内路径，跑安装器就改版本控制文件的权限位 | 登记，B02 设计部署路径时处理 |
| S6 | 计划 §1 的"1118 行"错了（实测 951 / 全部 17 文件 1107），且挂在"已核实"标题下 | **已修** |

**我自己在修复过程中额外发现并修掉的一处**（审计未提）：判 shebang 若写成
`git cat-file blob :$f | head -c 2 | grep -q '#!'`，`head` 读够即退会给 `git cat-file` 一个
SIGPIPE（rc=141），叠加 `set -o pipefail` 就被 `|| continue` 当成"没有 shebang"**静默跳过**
——正是本脚本要消灭的那类沉默失效。今天文件都小于管道缓冲区碰不到，但不能把正确性
寄托在文件大小上 → 改为**内容比较**而非管道 rc。

**变异验收**（隔离 clone，`core.fileMode=true` 即 CI 默认）：4 条变异全部由绿转红，
未变异时仍绿，`shellcheck --severity=error` rc=0。

| 变异 | 结果 |
|---|---|
| `git rm -r --cached shared`（旧版恒绿） | `守卫自检失败：shared/ 下 0 个带 shebang 的受检文件` rc=1 |
| 非 ASCII 名带 shebang 且 644（旧版静默跳过） | `FAIL \| shared/probe/脚本-中文.sh …` rc=1 |
| 白名单条目 `e2e/fake-claude` 退回 644 | `FAIL \| e2e/fake-claude …` rc=1 |
| 在非 git 目录跑（模拟 CI git 失败） | `守卫自检失败：git ls-files 执行失败（rc≠0）` rc=1 |

## 6. 工程审计结果（Phase E）

**漂移风险：现在就防，但只用一条约 10 行的软警告。** 审计把成本量化得很清楚，我采纳它的判断：

- 仓内 `shared/cc-bus/` 今天是**死副本**——`git grep` 全部命中都是注释，无 `include_str!`、
  无部署代码、无测试引用。所以漂移**现在不可能让任何东西变红或变错**。这与 `shared/ccm`
  处境完全不同（那份被 `sftp.rs:538` 编进二进制，结构上就没有第二份可漂）。
- 但漂移方向是危险的那一边：盘上那份是**活的**（`~/.local/bin` 12 条软链指着它、本机此刻
  就有 cc-bus 实例在跑），而这份代码的历史正是"一直手改、无版本管理"。
- **真正会被咬到的是 B02，不是仓库**：§2.1/§2.4 是一张**按行号写死**的对拍表
  （`cc-spawn:51-96`、`:100`、`:108-109`、`:124-133`），用来论证"删掉预信任那 46 行不是回归"。
  若两份之间发生手改，B02 就是在对着一个**已经不是运行态**的基线做等价性证明。
  **这条警告保护的是那份论证，不是文件本身。**

实现照抄仓里既有范式 `src-tauri/build.rs::check_vendor_freshness`：上游缺席 → no-op；
有差异 → 打印警告但**不改退出码**。追加在 `e2e/exec-bit-guard.sh` 末尾，CI 上目录不存在
→ 零影响；开发机上是唯一能漂的地方，正好命中——而且命中的时机正是有人准备跑门禁做 B02 时。

**刻意不做**：把盘上那份改成指向仓内工作树的 symlink。`git checkout` 到别的分支/旧 commit
会**当场把用户正在运行的消息总线换掉**——那比漂移糟得多。

**遗留清单（进 B02+，不在 B01 修）**：I4 悬空的 codex 示例文件与插件形态声明 · S4 `SKILL.md`
机器特定内容与干净 clone 里错误的安装命令 · S5 安装器会 chmod 自己的源目录 · S2 无 shebang
二进制不在守卫覆盖内。

**流程上的自我批评**（审计提出，我认同）：commit message 说"B01 只做搬不做改"，但同一个
commit 里还塞了 CI 步骤 + npm script + 37 行新 e2e 脚本，而**这次审计的全部风险恰好集中在
那部分**。搬家部分 `git show --numstat` 全是 `N 0`（纯新增零删除）这条性质本可以让 reviewer
一条命令确认"B01 是纯搬"，被混入的守卫稀释了。**下次这种"搬家时顺手发现的缺陷修复"应单独
commit**——本次的审计修复即按此办理，独立成 commit。

## 7. 签收
- [ ] 通过代码审计（无阻塞项）
- [ ] 通过工程审计
- [ ] 主计划已据此更新（含变更记录）
