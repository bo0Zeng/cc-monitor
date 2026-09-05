# K-W4 · D4 · 那份 2.3 MB 的动词 + `.build_id` 断裂实测

> 件：`K-W4`（DoD `KW4D4`，件文件 `§0c` / `§3`）
> 代码量于：`d305ffa`（改前）→ `adc10cf`（改后，`track/k-w4` 尖）
> 门禁：`PB_WS=backend-consolidation .claude/devbox/gate <树> k-w4`（沙箱，默认断网）。**宿主上没跑任何测试。**
> 打于：2026-09-04 夜

## 1 · 先答那一句：那份 2.3 MB 是「**被替换**」

选 **`§0c` 的乙**（改名 + 迁移），动词是**被替换**（先铺新名、再删旧名），**不是「留下」也不是「删除」**。

**论据，逐条：**

1. **甲（部署面不改名）自相矛盾。** 甲要的是「仓内叫 `cc-monitor-backend`、铺下去仍叫
   `cc-monitor-remote`」。而落点那个名字**不住在仓里** —— 它住在**用户可改并持久化的设置**
   `daemonPath` 里（`src/settings/machine-card.ts:164` 的常量 + `:172` 的
   `defaultDaemonPathFor()`，`:611` 从配置回填，`:247` 存回）。⇒ 甲实际上是「**默认值也不改**」，
   而 `KU2` 要修的那处「用户可见的不一致」恰恰就在这个默认值上。甲 = 不做，且要为「不做」再养一条
   仓内外名字不同的规矩。
2. **丙（只改默认值、不迁移）连改名都到不了部署面。** 老用户的 `daemonPath` 已经持久化成旧名
   （现打：`/home/zbl/.cc-monitor/bin/cc-monitor-remote` 就在那儿），新默认值只对**没填过**的机器生效
   （`:264` / `:371` 两处都是 `if (!value.trim())` 才填）⇒ 已配置的机器一台都不动。
   那份 2.3 MB 的动词会是「**永久留下**」，且是一份**名字在说假话**的残留。
3. **乙的迁移代价是可算的，而且今天只有一个落点。** 要动的是 `daemonPath` 的改写逻辑：
   「等于旧默认值 ⇒ 改写成新默认值」。判据是**字符串等于**，不是猜 —— 旧默认值是个具名常量
   （`machine-card.ts:164`），用户自己填过别的路径的机器**不该被改写**，而「等于旧默认」正好把这两群分开。
4. 🔴 **而乙有一条前置断裂，不治它乙就是那条断裂本身** —— 见下面第 2 节。**本波把那条断裂治了**；
   改名与迁移本体等 `K-W3` 的拆件表（PM 补充：本波不改名）。

⚠ **本波没做的**：`daemonPath` 的改写逻辑一行都没写（那是改名落地那一拍的事，`KW4D2`）。
本波交的是「那条断裂已经不在乙的路上了」。

## 2 · 那条断裂：**今天不改名也踩得到**（实测，不是推演）

### 2.1 机制，逐条读源码（住址带逐字内容）

| # | 事实 | 住址（`d305ffa`） |
|---|---|---|
| 1 | 标记是**目录级**的，路径里不带二进制名 | `sftp.rs` `fn marker_path` 逐字 `format!("{dir}/.build_id")` |
| 2 | 判定**只**吃两个入参，落点那个文件**不在入参里** | `sftp.rs` 逐字 `pub fn deploy_decision(remote_build_id: Option<&str>, expected: &str) -> DeployAction` |
| 3 | 自动部署那条路**从不 stat 二进制** | `ensure_daemon_deployed` 体内唯一的远端读是 `read_optional(sftp, &marker)`；`grep -nE 'metadata\|try_exists'` 在改前的 `sftp.rs` 上共 4 处，**没有一处**在这两条部署判定的前面（分别落在 `upload_atomic` / `read_profile_text` / `ensure_dir_all`） |
| 4 | 手动「安装 daemon」按钮**同一条病** | `deploy_remote_daemon` 的 `Skip` 支逐字回 `"远端已是最新 daemon（{}，{arch}）：{path}，无需重装。"` |
| 5 | 落点那个名字是**用户可改并持久化**的 | `machine-card.ts:164` / `:172` / `:247` / `:611` |
| 6 | 标记是**人取的标签**，不是内容哈希 | 现打用户机器上那份：`.build_id` 18 字节，内容逐字 **`p1r-event-liveness`** |

⇒ **①+②+③ 合起来**：那一个 `.build_id` 读数今天同时被当成「版本对不对」**和**「那个文件在不在」。
⑥ 又说明它连「字节相同」都不蕴含。

### 2.2 「今天不改名也踩得到」的三形，与各自的必要条件

| 形 | 必要条件 | 今天成立吗 |
|---|---|---|
| **㈠ 二进制被删/被移走，`.build_id` 留下** | 有人删了那个文件而没删同目录的标记（手工清理 · 磁盘工具 · 半途失败的拷贝）。**卸载按钮不算** —— 它两个都删（`uninstall_remote_daemon` 逐字删 `path` 与同目录 `.build_id`） | **成立**（结构上无人防；标记与二进制是两个独立文件） |
| **㈡ 二进制被截成 0 字节** | 不是假想：`upload_atomic` 里那条「**绝不 set_metadata**」注释逐字记着真机 e2e 实测 —— OpenSSH sftp-server 上 setstat 会把刚 rename 好的文件截成 0 字节，「daemon 因此变 0 字节不可 exec」 | **成立**，且 `try_exists` 会把它算成「在」⇒ 只问存在性的修法在这一形上仍然静默 |
| **㈢ `daemonPath` 被改到同目录另一个文件名** | 用户在设置里把路径从 `…/bin/cc-monitor-remote` 改成 `…/bin/别的名`。标记是**目录级**的 ⇒ 照旧匹配 | **成立**，且**这一形与改名是同一个形状** —— 改名只是让全体用户一次性都走进 ㈢ |

⇒ **答 PM 的问句：踩得到。** 三形都不需要改名。㈢ 正是改名会放大的那一形。

### 2.3 此前那条防御**只断了链的上半段** —— 有逐字证据

`upload_atomic_verified` 的头注逐字写着这条链与它的修法：

> 「传输损坏的 daemon 二进制照样被写上正确的 `.build_id` 标记 → 下次 `deploy_decision` 判
> 「已是最新，跳过」→ **坏二进制永久驻留**，而用户看到的是部署成功。标记写在校验之后，就断了这条链。」

★ 那条修法管的是「**我们自己这一次传坏了**」（先读回逐字节比对，过了才写标记）。
它**管不到部署成功之后那个文件再出事** —— ㈠㈡㈢ 三形全在它的射程之外。
**本波补的是这条链的下半段。**

### 2.4 改前读数（机器打的，不是我说的）

改前语义 = 「Missing 也只看版本」。用变异刀1 把这一格退回改前语义，判据当场红，报文逐字：

```
thread 'sftp::tests::a_matching_marker_no_longer_speaks_for_a_binary_that_is_not_there'
panicked at src/sftp.rs:1699:13:
标记相符但落点没有二进制，判定仍是 Skip —— 一个 `.build_id` 又同时替「版本对不对」和「那个文件在不在」两件事说了话
```

⇒ **改前 = `Skip`**（静默）。改后 = `Deploy("落点没有 daemon 二进制（而版本标记说它已是 …）")`。

## 3 · 修法（改后）

照本模块**既有的形状**：纯判定 + 异步取样（先例：`interpret_profile_read` + `read_profile_text`
——那一对就是把 `read_optional` 的 `Option<Vec<u8>>` 拆成三态，治的是同一族病「一个值装了两件事」）。

| 新增 | 是什么 |
|---|---|
| `enum TargetBinary` | 四态：`Present` / `Missing` / `Empty` / `Unknown`。**不是 `bool`** —— 「问不出来」不许读成「不在」（那等于每次 stat 失败就重传 2.3 MB），也不许读成「在」（那退回本枚举要治的静默） |
| `fn marker_phrase` | 让版本那一侧的事实在同一句话里单独说出来（相符 / 不符 / 无标记三种都说得出） |
| `fn deploy_decision_at` | 合并判定。**只有** `Missing` / `Empty` 越过版本门控；`Present` 与 `Unknown` 一律交回 `deploy_decision` |
| `async fn probe_target_binary` | 取样。`metadata` 一次往返；失败才补问 `try_exists`（要区分「明确不在」与「问不出来」，这两者在 `metadata` 的 `Err` 里长得一模一样） |

**接线**：两条 daemon 部署路都接上 —— `ensure_daemon_deployed`（自动）与 `deploy_remote_daemon`（手动按钮）。

**`deploy_decision` 的签名一个字没动** ⇒ 第三个调用点 `acct_iso_deploy.rs:188`（写区外）零改动。

**没做成「每次都重传」**：`Present`/`Unknown` 不越门控；`Missing`/`Empty` 越门控的那两形，
重传恰恰是唯一正确的动作（落点没有可跑的字节）。

**代价，如实写**：自动部署那条路每次多一次 SFTP `metadata` 往返（此前是 1 次 `read` 标记，现在 1 次
`read` + 1 次 `metadata`）。它只发生在已经开了 SFTP 会话的那条路上，不新增连接。

## 4 · 死值验（变异表）

每一刀都在沙箱门禁里整趟跑，日志逐趟落盘（住址见末节）。
**判定行口径**：`monitor` 包那一行 `test result:`（cargo 格是 8 个包合计，绿趟 1394 = `monitor` 1284 + 其余 7 包 110）。

| 刀 | 锚点（切在哪 · 切前命中数） | 变异内容 | 预期 | **实测红了哪几格** | `monitor` 判定行 | 分母 |
|---|---|---|---|---|---|---|
| — | — | 无（交付态 `adc10cf`） | 全绿 | **0 红**，`GATE: OK` | `ok. 1284`（由 1394 − 110 推得；绿趟门禁只印摘要） | 4 条新判据 / 全仓 1394 |
| **1** | `deploy_decision_at` 的 `TargetBinary::Missing` 支 · **1 处** | 退回改前语义：`=> deploy_decision(remote_build_id, expected)` | 只红「拆开」那一格 | **1 红**：`sftp::tests::a_matching_marker_no_longer_speaks_for_a_binary_that_is_not_there` | `FAILED. 1283 passed; 1 failed; 10 ignored` | 同上 |
| **2** | `deploy_decision_at` 的 `Present \| Unknown` 支 · **1 处** | 把版本门控弄坏：`=> DeployAction::Skip`（`.build_id` 不一致也不重传） | 只红「防御没拆」那一格 | **1 红**：`sftp::tests::splitting_the_two_facts_did_not_dismantle_the_version_gate`（报文逐字「版本不符 + 文件在 ⇒ 竟然跳过，stale 防御被拆了」）· ⚠ 另有 1 红是**别处的抖动**，见第 5 节 | `FAILED. 1283 passed; 1 failed` | 同上 |
| **3** | `deploy_decision_at` 的 `TargetBinary::Empty` 支 · **1 处** | 退回版本门控（= 只问存在性的那种修法） | 只红「0 字节」那一格 | **1 红**：`sftp::tests::a_zero_byte_daemon_is_not_a_deployed_daemon` | `FAILED. 1283 passed; 1 failed` | 同上 |
| **4**（`7u`） | 两个调用点的 `deploy_decision_at(remote_id.as_deref(), bin.build_id, target)` · **2 处** | 接线整个退掉：两处都退成裸 `deploy_decision(...)`，取样仍跑但结果不进判定 | 只红「防空转」那一格 | **1 红**：`sftp::tests::both_daemon_deploy_paths_ask_the_file_itself_not_only_the_marker`（报文逐字「`pub async fn ensure_daemon_deployed(`: 还留着裸 `deploy_decision(` 调用 —— 两条判定并存迟早分叉」）· ⚠ 另有 1 红是抖动，见第 5 节 | `FAILED. 1283 passed; 1 failed` | 同上 |

**四刀合起来 = 四条新判据一对一**，没有一格是仪式：

| 新判据 | 被哪一刀逮到 | 射程（它**不**管什么） |
|---|---|---|
| `a_matching_marker_no_longer_speaks_for_a_binary_that_is_not_there` | 刀1（最小面：只切 `Missing` 一支） | 不管 0 字节、不管接线 |
| `splitting_the_two_facts_did_not_dismantle_the_version_gate` | 刀2（最小面：只切 `Present\|Unknown` 一支） | 它**不是**新功能的判据，是「别用错的修法」的守卫 ⇒ 全退实现它仍绿，这是设计如此 |
| `a_zero_byte_daemon_is_not_a_deployed_daemon` | 刀3（最小面：只切 `Empty` 一支） | 不管「在但内容是别的东西」（那要内容哈希，本波没做） |
| `both_daemon_deploy_paths_ask_the_file_itself_not_only_the_marker` | 刀4 | 射程只到 daemon 那两条路；`acct_iso_deploy` **刻意不在分母里** |

### `7u` 逐条：把实现退掉，还有多少条新断言仍绿

刀4 退的是**接线**（4 条里红 1、仍绿 3）；把**纯判定**退掉的是刀1/刀3（各红 1）。合起来：

| 新判据 | 全退接线（刀4） | 退掉对应的纯判定支 | 仍绿的理由（如实写） |
|---|---|---|---|
| 「拆开」 | 仍绿 | 刀1 红 | 它的牙在纯判定上，**逮不到死代码** —— 这正是「防空转」那一格存在的理由 |
| 「防御没拆」 | 仍绿 | 刀2 红 | **它本来就该在全退时绿**：它断的是「版本门控还在」，而全退回去门控当然还在。它是反向守卫，不是正向判据 |
| 「0 字节」 | 仍绿 | 刀3 红 | 同「拆开」，牙在纯判定上 |
| 「防空转」 | **红** | 不适用 | 它是唯一一条盯着调用点的 |

⇒ **真空真 = 0 条**：四条各有各的刀，没有一条是「退了实现照样绿且别处也没牙」。

## 5 · ⚠ 两条**与本件无关的抖动**（逐字登记，别记在本件账上，也别当没有）

两条都不在本件写区、不在本件改动面里，且**同一份字节在别趟是绿的**。

| # | 格 | 报文逐字 | 住址 | 为什么判它是抖动 |
|---|---|---|---|---|
| 甲 | `daemon` | `assertion `left == right` failed: 起来的中转必须**只有一个**（`listening on` 恰好一行）：""` / `left: 0` / `right: 1` | `remote-daemon-proto/src/relay/server.rs:2633`（测试 `relay::server::tests::one_relay_process_serves_both_keys_and_shares_its_tee_sequence`） | 只在刀2 那一趟红。`remote-daemon-proto/` 这个包在绿趟与刀2 趟**逐字节相同**（本件一个字节都没改它）⇒ 同一份字节两趟不同答。判定行 `541 passed; 1 failed`（绿趟 `542 passed`） |
| 乙 | `npm` | `Error: Test timed out in 120000ms.` —— 格 `src/eslint-baseline.vitest.ts > V7-3：eslint 基线与作用面 > ① 全仓错误数就是基线那个数（散文声称的那个）` | `src/eslint-baseline.vitest.ts:96` | 只在刀4 那一趟红，且**是超时不是断言失败**（它整仓跑一遍 eslint）。那一趟 vitest 总数仍是 `1540 passed \| 1 failed (1541)`，与绿趟 `1541` 同分母。当时机器上有 13 路并发 + PM 预热 |

⚠ 甲**不是** `COMMON-PM` 登记的那条已知 flaky（那条是 `launch::tests::the_terminal_we_hand_the_command_to_really_gets_the_relay_prefix`，报文 `Text file busy`）。**这是第二条，报文与住址都不同。**

## 6 · 改动面（`d305ffa` → `adc10cf`）

`git diff --stat`：`src-tauri/src/sftp.rs` +270/-3（1969 → 2235 行）· `src/settings/machine-card.ts` +4/-1（1197 → 1200 行）
· `evidence/K-W4-D1-rename-surface.md` 新增 187 行。

**逐条目 md5**（量具：按「顶层 `fn`/`enum`/`struct` 签名行 → 下一个独占一行的 `}`」切，与
`sftp.rs` 里 `deploy_paths_use_verified_upload_for_content` 的 `body()` 同法；「去注释」= 剥掉全部
`//` 打头的行再 md5，用来分开「只改措辞」与「动了代码」）：

| 条目 | 全文 md5 前 → 后 | 去注释 md5 前 → 后 | 判读 |
|---|---|---|---|
| `TargetBinary` | — → `05c49feb` | — → `d90509e9` | 新增 |
| `marker_phrase` | — → `9ef9b48a` | — → `9ef9b48a` | 新增 |
| `deploy_decision_at` | — → `1f76c24f` | — → `2dd73f87` | 新增 |
| `probe_target_binary` | — → `3e46c73e` | — → `1afbacb4` | 新增 |
| `ensure_daemon_deployed` | `4e57524f` → `bc629303` | `5fb38668` → `d69eedf3` | 动了代码（+1 `let target`、判定换成 `deploy_decision_at`） |
| `deploy_remote_daemon` | `7a4e3607` → `92c62231` | `7a4e3607` → `075daa0a` | 动了代码（同上） |

整份 `sftp.rs`：`fd62002a` → `9cd46150`（去注释 `3e030e9d` → `f3249cd3`）。
`machine-card.ts` 整份：`7c3c601d` → `fe35a14b`。

⚠ **这把尺子的作用域，写出来别当通则**：它从**签名行**起切 ⇒ 签名**上方的 doc 注释**不在里面，
`mod tests` 也不是它认得的条目。所以上表**漏掉两处真改动**，另行核实：

- `deploy_decision` 与 `marker_path` 的**函数体逐字节相同**（`body()` 口径现打：
  `db9a6150` / `ffcd2394`，前后同值）—— 它们变的只有签名上方的 doc 注释（各 +6 / +3 行）。
- `mod tests` 里 **+156 行 = 4 条新判据**。

九个 hunk 全部对上（`git diff -U0` 的 `@@` 头）：
`+350,6`（`deploy_decision` docstring）· `+366,73`（三个新条目）· `+449,3`（`remote_parent` 后）·
`+461,23`（`probe_target_binary`）· `+555,2` 与 `+558,1`（`ensure_daemon_deployed`）·
`+689,4`（`deploy_remote_daemon`）· `+1684,156`（四条判据）· machine-card `+557,4`。

## 7 · 日志住址（每趟一份独立文件名，没有与别人共用的住址）

`/tmp/claude-1000/-home-zbl----claudecode-frontend/abfa3e79-ba75-4cc8-8b57-3abf56ea2d80/scratchpad/`
下 `K-W4-gate-green.log` · `K-W4-cut1.log` · `K-W4-cut2.log` · `K-W4-cut3.log` · `K-W4-cut4.log`
（被测对象一律指向 `/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-w4`，target 目录名 `k-w4`）。
⚠ 那是会话私有的临时目录，**会过期** —— 变异表的读数已逐条抄进本文件，不指望日志还在。
