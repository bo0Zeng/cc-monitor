# Phase A 摸底 C — `shared/ccm` 能力盘点（2026-08-01）

> 四份摸底之一。**这份是「要搬的东西到底有多少、哪些搬不走」的事实底账。**
> 结论进主计划，原始细节留在这里，别在主计划里复述一遍。

## 0. 两处先订正

- **`shared/ccm` 是 662 行，不是 1300 行**（`wc -l` 实测）。我先前口头说的 1300 是错的
  —— 那个数是 `ci.yml` 注释里 **vendored `cc-acct-iso`** 的 1348 行，两回事。
- **「必须在交互 shell 里」这个分类要重新读**：它排除的是「让 daemon 代劳」，
  **不排除「用 Rust 二进制代替 bash」**（Rust 一样能 `setenv` + `chdir` + `execvp`）。
  **真正非 shell 不可的只有一条**：`eval "$CCM_ENV"`（用户配置里的任意 shell 片段）。

## 1. 能搬的（Rust 一份实现）—— 大头

参数解析与全部校验（`:94-224`）· `sq`/`qarg`（`:102-112`）· agent 查表（`:114-133`）·
`resolve_cwd` 的纯计算部分（`:228-248`）· **`derive_tmux_name`（`:254-262`，搬走即消掉
与 `src/remote-launch.ts::deriveTmuxName` 的跨语言双写点 = E49 的正题）** ·
manifest 三个解析函数 + 账号三态（`:286-345`）· 内层载荷构造 + R08 显式化（`:488-526`）·
tmux 序列构造（`:528-569`）· `--print` 等价行拼装（`:579-591`）。

## 2. 搬不走的 —— 薄 ccm 的最小职责（评估，7 件）

1. **`exec` 顶替自身**（三条出口 `:367` / `:572` / `:662`）。不能改成 fork+wait：
   PID 要保持不变（**它就是 pidfile 的文件名**）、tty / 作业控制 / Ctrl-C 要直接落在 agent 上、
   `tmux attach` 必须在用户终端上跑。
2. **在自己进程里设 env 再 exec**（`:594-613`）。`export CLAUDE_CONFIG_DIR` 之所以有效
   全靠它发生在最终 exec 的那个进程里 —— **这正是旧 `cct` 死掉的地方**。
3. **`eval "$CCM_ENV"`**（`:594`）—— 唯一非 shell 不可的一条。Rust 版要么 `sh -c '…; exec …'`，
   要么明确弃用这个配置项。**这是一个决策点。**
4. **只有它能看见的上下文**，必须由它采集并上报：`$TMUX` / `TMUX_PANE`（决定「就地起 vs 建容器」，
   也是「我在哪个 tmux 会话里」的唯一来源）· `$PWD`（`--cwd auto` 的输入）· 继承的
   `CLAUDE_CONFIG_DIR`（R11/R08 两条真机 bug 的核心输入）· **自己的 `$$`** · `$0` · PATH 里有没有 tmux/jq。
5. **`--print` / `--ccm-probe` / `--version` 三个自省出口**。`--print` 必须仍然**纯**
   （不查 tmux、不写文件）且**逐字节稳定**（44 条断言压着）；`--ccm-probe` 首行逐字 `name=ccm`
   是 app 与 `cc-spawn` 的分流开关。
6. **daemon 不在时终端用户看到什么** —— 用户已拍板「用 ccm 就必须装 daemon」，
   但**报错形态要设计**，不能默认继承。
7. **`--help`**（今天靠 `sed` 读自身源文件，`:199`）。

### ★ 由此白捡一个 join key

薄 ccm 主动上报自己的 `$$`，daemon 就**直接拿到 `PID ↔ tmux 会话` 的映射** ——
那正是 E76 缺的那一块（daemon 只知道 sid↔pid↔jsonl，不知道 pid 在哪个 tmux 会话里）。

## 3. 通道 A/B：形状可以原封不动，只是「确认者换人」

- **通道 A（意图）**`@ccm_sid_expect`：ccm 一拿到 sid 就写。两个写点：`:541`（外层、带 `-t $t`）
  与 `:620-622`（pane 内、不带 `-t`、靠 `$TMUX`）。
- **通道 B（事实）**`@ccm_sid`：`:646-659` 的后台 poller，**独立读 Claude 自己写的 pidfile
  确认之后才写**。它是 `@ccm_sid` 的唯一写者。
- **防的是**（INVARIANTS `:621-626` 原文）：「一个从未真正跑起来过的声明，会永久冒充事实，
  被后续的身份核验采信。」破坏性动作（kill / send-keys）**只认 `@ccm_sid`**。

搬进 daemon 之后的变化：

| 前提 | 今天 | 之后 |
|---|---|---|
| 1 Hz 轮询 | ccm 自己 `sleep 1` | **不能照搬** —— 撞 `no_timer_guard`（P6 刚把 daemon 里两条轮询全删掉）。必须接已有的 pidfile inotify + pidfd |
| pidfile 观测 | 每秒 grep 一次 | **已经有了而且更好**：daemon 本来就 inotify 盯 `sessions/<PID>.json`，延迟从「最多 1s」变内核事件级 |
| PID→tmux 会话 | 白送（poller 就住在那个 pane 里） | **真正的缺口**。两个可选 join key：`@ccm_sid_expect`（ccm 用 `-t =名:` 精确写下）或薄 ccm 上报 `$$` |
| daemon 能不能写 tmux | — | **能，有先例**：`tmux_hook.rs` 已经在跑 `tmux set-hook -g`。`readonly_guard` 禁的是**文件系统写** |
| OSC 标题写 | poller 写自己的 stdout | **搬不走**（daemon 没那个 tty）。但 tmux 内已被 `set-titles-string '#{?@ccm_sid,…}'` 取代 ⇒ daemon 写对 `@ccm_sid` 标题自动合成。**只有「不在 tmux 里跑」那条路会失去 marker** |

## 4. ★ 唯一正面撞红线的能力：预信任

`:421-486`：`jq` **改写用户既有的 `~/.claude.json`** + 追加写 `~/.codex/config.toml`。
而 `readonly_guard` 在 G2 之后的判据是「**daemon 不许改动用户既有数据**」（`fork_write.rs`
是唯一白名单且只准 `O_EXCL` 新建）。**这不是新建文件，是原地改用户配置。**

⇒ 预信任**搬不进 daemon 进程**（除非用户显式再松一次红线）。可选落点：留在薄 ccm 里 /
走 monitor 侧既有的 SSH exec + SFTP 通道（E50 已确认那条路不经 daemon）。
**主计划要把它单列成一个决策点。**

反过来 **tmux 那些副作用不撞红线** —— E49 里有用户当场做的更正：
「`tmux new-session` 是起进程、不是 fs 写，护栏拦不到，真正的问题只是语义（观察者 vs 执行者）」。

## 5. 动 ccm 之前必须先做的一件事（否则整轮重构在假绿里跑）

**E11：这几套 e2e 没有最小断言数地板**，只有 `[ "$FAIL" -eq 0 ]` + 3 处静默 SKIP
⇒ 「少跑若干条 + 绿」。实测地板值（BACKLOG 已给 / 本次复测）：

| 套件 | 断言数 |
|---|---|
| `ccm-cli` | 44 |
| `ccm-acceptance` | 19（BACKLOG 记的 15 已过期） |
| `ccm-pretrust` | 13 |
| `ccm-print-parity` | 12 |
| `ccm-rbind-title` | 6（CI 地板写的 8，**对不上，要核**） |

⇒ **主计划第一个功能应该是「给这几套加地板」**，在动任何 ccm 代码之前。

## 6. 改 `shared/ccm` 会直接弄红 `cargo test`

`src-tauri/src/sftp.rs::ccm_cli_has_required_elements` 钉着：
11 个 needle（`--ccm-probe` / `--print` / `--tmux` / `--account` / `--agent` / `--ccm-sid` /
`CLAUDE_CONFIG_DIR` / `@ccm_sid` / `@ccm_sid_expect` / `@ccm_agent` / `exec`）·
通道 A 两处**逐字片段** · `pin_definition`（`t="$(sq "=$tmux_name:")"` 逐字且只赋值一次）·
`structural_scan`（脚本里每个 `-t` 后必须是 `=…:` 或字面 `$t`）。

## 7. 第二个消费者：`cc-spawn`（已收编，不是计划）

`shared/cc-bus/scripts/cc-spawn:124-132` 今天**全部经 ccm** 建会话，并在 `:58-63` 用
`--ccm-probe` 的 `capabilities=` 做能力协商。⇒ **薄 ccm 必须保留 `--detach` / `--tmux-size` /
`--ccm-probe`**，且 `--detach` 必须真的返回（不 attach）。
（顺带：`shared/ccm:421-431` 那段「与 cc-spawn 重复实现」的维护提示**已经过期**，B02 已消除重复。）

## 8. 会被这次重构推翻的既有断言（要同步重写，且强度不许降）

今天 `--tmux` 的实现是**外层 ccm 把「同一条 ccm 命令去掉 `--tmux`」送进 pane，由内层 ccm 设 env 后 exec**。
若 daemon 直接构造 `export …; exec claude …` 送进去，这层递归就没了 ——
**而 R08 那 5 条 e2e 断言的正是「内层载荷里必须出现 `'--account' 'z'` / configDir 字面量」**，
断的是**载荷的语法形态**。换实现必须同步重写，原注释还记着「我第一版就写错成这样，三条里两条假红」。

## 9. 顺手闭合 / 至少别复刻的既有缺陷

- **E19（重要）**：相对 `--cwd` 被应用两次 + 产孤儿会话。Phase D 为预信任造过 `cwd_abs`（`:444`），
  **没推广到容器路径**。含符号链接的绝对路径同型。
- **E15（重要）**：两个渲染器对「未选账号」不等价（CLI → `--base` → 强制基座；兜底 → 零 env op → 继承）。
  **待用户拍板是「不注入」还是「强制基座」。**
- **E3**：装 ccm 时读回不符**不回滚**，坏 CLI 留在远端。薄客户端一旦「必须与 daemon 版本匹配」，杀伤力变大。
- **E28**：已删的 `shared/ccm-wrapper.sh` 仍被 4 处当现存文件引用。

## 10. `--resolve` 是现成模板，但它的契约冻结在两仓之间

`resolve_query.rs` 的形状（stdin `ResumeSpec` → stdout `CommandPlan`，exit 0 / 错误 exit 2 +
结构化 stderr）**就是「薄客户端 + Rust 决策」的模板**。但：

1. 它自称 **「advisory not owning：只返命令串、daemon 零 handle、绝不执行后端」**
   —— 让 daemon 真去建会话是**角色变更**，不是加个子命令。
2. 字段名与 aterm 严格对齐、**契约已冻结**（2026-07-18）⇒ 复用/扩展要走两仓 lockstep。
3. `IPC-PROTOCOL §10.1` 警告过：它回的 `sessionName` 是**纯派生值**。薄 ccm 若消费 daemon 返回的
   会话名，**必须区分「派生」与「探测」** —— 这正是通道 A/B 那条分界线在另一个面上的重演。

## 11. 一个容易漏的小事实

`--print` 在容器路径下展示的是**基名**（不含 `-2/-3` 避让），因为避让查询被
`[ "$do_print" != 1 ]` 门掉了（`:412`）—— 注释写明「`--print` 必须是纯的」。
薄客户端若把 `--print` 变成一次 RPC，**这条「纯」靠什么保住，要写清楚**。
