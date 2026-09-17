# K-W2E 读数簿 —— 最小假插件走通全流程

> 每个数带「量于哪一刻 · 在哪棵树 · 用什么量的」。**引用前重打**，别当常量。
> 量具住址（唯一，只属于本件）：`evidence/K-W2E-hop-census.py` · 输出快照
> `evidence/K-W2E-hop-census.out`。它的**被测对象由 `--root` 指定，默认 = 它所在的那棵树**
> —— 也就是 `.claude/worktrees/k-w2e`，**不是主干**。

## 0 · 量于哪儿

| 项 | 值 |
|---|---|
| 树 | `/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-w2e`（分支 `track/k-w2e`） |
| 基点 | 主干 `d305ffa` |
| 门禁 | `PB_WS=backend-consolidation .claude/devbox/gate <树> k-w2e`（沙箱，默认 `--network none`） |
| 时刻 | 09-04 夜 |

⚠ **PM 那份计划里的数量于 `4eb271f`，本件量于 `d305ffa`** —— 两个提交不同，
凡两边的数对不上，下面都逐条点名，别按「谁记错了」处理。

## 1 · 门禁九格（入场 / 交回）

| 格 | 入场（改前，本树 @ `d305ffa`） | 交回（改后） | 差 |
|---|---|---|---|
| cargo | 1390 | 1390 | 0（本件全部落点在 `remote-daemon-proto/`，那是**另一格**） |
| generated | ok | ok | — |
| **daemon** | **542** | **554** | **+12**（本件新增 12 条判据） |
| npm | 1541 | 1541 | 0 |
| ccm e2e ×4 | 12 / 8 / 242 / 68 | 12 / 8 / 242 / 68 | 0 |
| pb check | FAIL=0 | FAIL=1~2 | —— **不是本件的账**，见 §6④ |

入场那一趟的日志：`kw2e-gate-00-entry.log`（在本会话的 scratchpad 里，不进仓）。

⚠ **本件跑过 7 趟门禁，其中一趟是 CRASH，如实记**（`brief` 第 8 条：判定行掉了按 CRASH 记，
不许报「新红 0」）：第 4 趟 `cargo test` **编译不过**（`E0277: Walk doesn't implement Debug`，
落点 `plugin_walk_fixture.rs` 那条新加的刀 C 判据的 `{other:?}`）⇒ **那一趟一行判定行都没有**，
不是「0 红」。给 `Walk` 加 `#[derive(Debug)]` 之后第 5 趟恢复。

## 2 · 尺子甲：九跳 × 三维（`KW2E1`，PM 写于 09-04，本件重打）

㈠ 生产代码在不在 · ㈡ 生产路上有没有人走 · ㈢ 有没有**执行读数**（判据真驱动过）。

| 跳 | ㈠ | ㈡ | ㈢ 改前 | ㈢ 改后（本件） |
|---|---|---|---|---|
| ① 宣告接得下 | 在 | 走（真 daemon） | 有，但驱动它的是 `wire`/`inbound` 自己的判据，**不是被一个插件驱动的** | **仍然没有** —— 要一条真命令进 `REGISTRY`，那是 PM 持有的文件（走上报） |
| ② 宣告做不到 | 在 | 🔴 **不走**（`main.rs` 生产段硬写 `unavailable: Vec::new(),`） | 有（**tmux 轴**：`unavailable_from(Some(false))`）；**插件轴 0** | **插件轴的判定那一半有了**（`not_installed` 三条命令，见 §4）；**进真 `Hello` 那一半仍然没有** |
| ③ 收命令分派 | 在 | 走 | 有 | 仍然没有（同 ①） |
| ④ 找它 | 在 | 走（`control/cc_bus.rs` 生产段那一处） | 有 —— `discover.rs` **5 条** | **多 5 条**，其中 3 条是**真的在盘上找一个真可执行文件** |
| ⑤ 问它会什么 | 在 | 🔴 **零生产调用方**（`plugin/mod.rs` 头注自陈 + `#[allow(dead_code)]`） | 有 —— `probe.rs` **8 条**，喂的全是**合成 probe 文本** | **多 3 条，而且喂的是一个真进程刚打出来的字节**（这是本件在这一跳上真正的增量） |
| ⑥ 传 argv 起它 | 在 | 走（`cc_bus.rs` 生产段，全 crate 唯一生产调用） | 🔴 **零**（`invoke.rs` 6 条是 `argv_for` 纯函数断言 + 手搓 `Done{…}`；`cc_bus.rs` 7 条全是纯函数） | **4 条真起进程**（本 crate 第一次） |
| ⑦ 拿码摘诊断 | 在 | 走 | 有 —— 手搓 `Done{…}` 驱动 | **码与两条流来自真进程**：`code=Some(6)` / `diag="no realm was handed over"` / `code=Some(0)` / `stdout="sealed in alpha\n"` |
| ⑧ 翻语义 | 在 | 走 | 有 —— `cc_bus.rs::the_three_exit_codes_map_to_three_different_meanings` | **多一份第二个插件的码表**，且与那一族**逐码对拍不同形**（表见 §5） |
| ⑨ 回帧 | 在 | 走 | 有（`inbound`/`wire` 自己的） | 仍然没有（同 ①） |

**⇒ 覆盖读数（带分母）**：全流程 **9 跳**。本件的假插件真走 **6 跳**（②半 · ④⑤⑥⑦⑧），
其中**真过通用层 4 跳半**（④⑤⑥⑦ ＋ ② 的判定半）· **自问自答 1 跳**（⑧ —— 而这一跳按 `E6`
本来就该每插件一份，「没过通用层」是设计不是缺口）· **走不到 3 跳**（①③⑨，见 §7）。

对照 `S6` 的比例（7 段里 2 段真过通用层 = 2/7）：本件是 **4.5/6 真过**，
而**分母不同** —— 它数的是 agent 轴的 7 段，这里数的是插件轴本件走得到的 6 跳。
两个比例不许直接比大小。

### 2a · 「没有一条判据同时驱动 ≥2 跳」这句话的读数（`KW2E1` 边界②）

量具：`evidence/K-W2E-hop-census.py`（针表按文件给，逐条局限写在它的模块头注里）。

| 语料 | 判据条数 | 同时驱动 ≥2 跳 | 真起进程 |
|---|---|---|---|
| `plugin/discover.rs` | 5 | 0 | 0 |
| `plugin/probe.rs` | 8 | 0 | 0 |
| `plugin/invoke.rs` | 6 | 0 | 0 |
| `plugin/mod.rs`（层边界判据，不驱动任何一跳） | 6 | 0 | 0 |
| `control/cc_bus.rs` | **7**（PM 那份计划两处写「6 条」，见 §6①） | 0 | 0 |
| **改前小计** | **32** | **0** | **0** |
| `plugin_walk_fixture.rs`（本件） | 12 | 8 | 6 |
| **改后合计** | **44** | **8** | **6** |

⚠ 「改前」这一栏的分母是**同一趟**的子人群（前五个文件），不是另跑一趟 ——
逐条明细在 `K-W2E-hop-census.out` 里，一条都没有 `★真起进程` 标记。

⚠ 量具第一版把走全流程那条判据的跳数数**偏小**（六跳全在 driver 里，而它只看
`#[test]` 之后那一段）⇒ 已加**传递归因**并在输出里把「经 driver 传递」标出来。
**这就是「量具的作用域对不上事实」在本件里的活体**，写在这里免得下一个人照第一版复打。

## 3 · 尺子乙：「加一个插件，宿主零改动」的差距读数（`KW2E5`）

见 `plugin_walk_fixture.rs::adding_a_plugin_still_costs_the_host_something` 的头注
（那张八行表 + 两个带条件的数）。这里只记**本件自己付的那一处**：

- **1 处** —— `remote-daemon-proto/src/main.rs` 的一行 `mod plugin_walk_fixture;`（PM 预批）。
- 而这 1 处**买不到**跳①③⑨：假插件进不了真 `Hello.commands`、进不了 `dispatch`。

🔴 **本件没有把这个数改小**：无条件要动的那两处（`REGISTRY` ＋ 协议文档锚点）一处没少，
「每插件一组专有针」那一格仍然**没人管**（本件给自己带了一组，但没有任何机检能逼下一个人带）。

## 4 · 跳② 出路丙的实得（`KW2E4`）

现打（沙箱内，真跑）：

```
跳② 事前那张表 = [Unavailable { command: "bus-list", code: "not_installed" },
                  Unavailable { command: "bus-kill", code: "not_installed" },
                  Unavailable { command: "bus-send", code: "not_installed" }]
```

- **3 条**，全部从 `inbound::REGISTRY` 的 `codes` 派生（不手写第二张表）；
- 三态实测：`Some(true)` ⇒ 空 · `None` ⇒ 空（「判不出来」不倒向「做不到」）· `Some(false)` ⇒ 上面那 3 条；
- 事前那个词与**事后真调用**（一次真的盘上找不到）回的词逐字相同：`not_installed`；
- 🔴 **缺口如实记**：假插件自己的名字 `hedron` **不在** `inbound::COMMANDS` 里
  ⇒ 它「说得出」但**说不出口**。买另一半要走出路乙（`REGISTRY` 8 → 9），
  那要动 PM 持有的 `inbound.rs` ⇒ 上报，不自批。
- 🔴 三条红线一条没碰：`main.rs` 那行 `unavailable: Vec::new(),` 没动 ·
  `hello_unavailable_is_additive_present_and_absent` 的两串期望字节没动
  （本件**没有构造任何 `Frame::Hello`**）· 宣称的 code 就是 `REGISTRY` 自己登记的那个词。

## 5 · 跳⑧ 逐码对拍：本夹具 vs 那个真插件族（活体，不是手抄的表）

现打（`code_word` vs `control::cc_bus::classify_send`）：

| 退出码 | 本夹具 | 那一族 | 判 |
|---|---|---|---|
| 0 | `sealed` | `<成事>` | 刻意重叠 —— 「成功」这件事本身，不是照抄 |
| 2 | `broke` | `invalid_args` | 不同 |
| 3 | `broke` | `rejected` | 不同 |
| 5 | `stale_ledger` | `failed` | 不同 |
| 6 | `realm_locked` | `failed` | 不同 |
| 7 | `broke` | `failed` | 不同 |
| 124 | `deadline_hit` | `timed_out` | 不同 |
| `None`（被信号打断） | `signalled` | `failed` | 不同 |

## 5b · 死值验（`§3` 三刀 + `7u`）—— 逐行真实输出

分母基线：门③ daemon 格，**本树 @ `d305ffa` 实测 542**（PM 计划里写 538，那是量于 `4eb271f`）。
出货版 **554**（542 + 12）。两刀**同一趟**落地（它们的红互不覆盖，见「怎么归因」一栏）。

| 刀 | 锚点 · 命中数 | 改法 | 判定行 | 新红 | 红的那句话（逐字） |
|---|---|---|---|---|---|
| **A**（挖一跳的实现） | `Caps::without` 里 `"候选路径与那句尾巴" => c.discovery = None,`，**命中 1** | 换成 `=> {}`（形状对、恒答原样那张脸） | `548 passed; 6 failed; 1 ignored` | 2 | ① `挖掉「候选路径与那句尾巴」之后流程仍然走完了 ["跳② …", "跳④ 找它", …] —— 少一样知识却一路绿灯，那正是 KW2E3 禁掉的那种「笼统地成功」` ② `挖「候选路径与那句尾巴」这个洞没有挖到任何东西` |
| **C**（把脚本去掉执行位） | —— **做成常驻**，不是一次性变异 | 见 `an_unarmed_plugin_stops_at_hop_four_saying_where_it_looked` | 见 §1 | （常驻，恒验） | `NotFound { hop: "跳④ 找它", why: "找不到 \`hedron\`：查过 …/libexec/hedron · …/hedron，以及 PATH 上的 0 个目录。（夹具口径：只查上面这两处，刻意不兜 PATH）" }` |
| **`7u`**（把实现整个退掉） | 起进程口的全路径调用形，**命中 6 处代码**（＋1 处注释，⚠ 第一次数成 7 就是量到了自己） | 6 处全换成手搓 `Done{…}`（今天 `invoke.rs` 那 6 条判据的形状） | 同上一行 | 4 | ③ `探测输出里没有那个真进程报回的**父进程名** —— 那一趟多半根本没 fork/exec` ④ `问不出子进程的父进程是谁（/proc 读不到？）—— 本格此刻分不出两条路，按「判不了」记` ⑤ `两边有一边的 PATH 是空的 ⇒ 本段是空真：host="/opt/rust/cargo/bin:…" child=""` ⑥ `本文件的测试段里找不到 \`invoke::run(\` —— 抽取坏了，下面那条天花板在空转` |

**怎么归因**：两刀的 6 条红按**报文**分得开 —— A 的两条逐字点着被挖的那一样知识的名字；
`7u` 的四条逐字点着「只有真进程才产得出的那样事实不见了」或「起进程口的针不见了」。
两族不重叠。⚠ 而 `digging_out…` 那条同时用着 driver（A 的靶）与真进程（`7u` 的靶）——
它红在 A 那一句上，说明 `7u` 没有把它先红掉（手搓的 `Done` 让六个洞照样各停各的）。

### `7u` 的正题：**退掉之后还有多少条新断言仍绿 —— 8 / 12**

⚠ 这是本件**最难看也最要紧**的一个读数，不许含混：

| 仍绿的 | 为什么它仍绿（逐条给理由，不写「它测的是别的」了事） |
|---|---|
| `one_fake_plugin_walks_six_hops_as_one_path` | 🔴 **本件的旗舰判据，而它对「真起进程」没有牙** —— 它断的全是**契约**（码是几 · 诊断里有哪个词 · stdout 里有没有那一项 env），而契约是手搓得出来的。这一条**在规划这一刀时就先纸上推了出来**，所以同轮补了第 12 条判据（`the_walk_really_started_a_process_not_a_hand_built_done`）专治它 —— 而那一条在同一刀下**当场红了**（上表 ③）。⇒ 牙在别的刀上，且那把刀是本轮新配的 |
| `an_unarmed_plugin_stops_at_hop_four_saying_where_it_looked` | 它的靶是**刀 C**（执行位），走的是 `discover::find` 这条**没被退掉**的路 ⇒ 本刀对它天然无效 |
| `a_missing_or_unarmed_executable_says_where_it_looked` | 同上（纯 `discover`，一处进程都不起） |
| `the_pre_declaration_reuses_the_word_the_registry_already_owns` | 跳② 从 `inbound::REGISTRY` 派生，与起不起进程无关；它的牙在「那个词被改名」那一刀上 |
| `the_generic_port_never_learns_this_fixtures_vocabulary` | 源码扫描判据，靶是 `plugin/` 的生产段；它的牙在它自己的**阴性对照**里（合成违规样本必须命中） |
| `adding_a_plugin_still_costs_the_host_something` | 读的是注册表与源码面的差距读数，与起不起进程无关 |
| `this_fake_plugin_is_deliberately_unlike_the_real_one` | （本趟它红了，但红在**刀 A** 上；对 `7u` 它是仍绿的 —— 逐维对拍全是纯函数） |
| `digging_out_any_one_hop_stops_where_it_can_name_it` | （同上：红在刀 A 上；对 `7u` 仍绿 —— 六个洞的停车位置不依赖进程真不真） |

⇒ **一句话**：12 条新判据里，真正为「**这一趟是真的 fork/exec 了**」买单的只有 **4 条**
（新加的第 12 条 · 期限那一格 · 环境继承那一格 · 天花板的抽取自检）。
另外 8 条买的是别的性质，它们仍绿**不是仪式**，但**也不能拿来给「真起进程」作证**。

## 5c · PM 追加那一格：通用口的词汇针补第二族（`M8a` / `M8b` 复打）

**先答冲突**：本件那个假插件的词是**第三族**（`hedron` · `facets` · `seal` · `nap` ·
`HEDRON_REALM` · `alpha` · `emit-ledger` · `dry-run` · `sealed` · `stale_ledger` ·
`realm_locked` · `broke` · `signalled`），与 PM 给的 5 个词**零重叠** ⇒ **不冲突**，照加。

加针前现打（`grep` 于本树 `d305ffa`，`plugin/` 三个被扫文件）：
`code-picture` · `codepicture` · `panorama` · `code_picture_core` · `index.db`
**逐个命中均为 0** ——与 `K-W2D` 的读数一致。

| 刀 | 状态 | 判定行 | 新红 | 红的那句话（逐字） |
|---|---|---|---|---|
| **M8a** | 针 = 旧 6 根 ＋ 往 `discover::find` 生产段头上塞 `let _ = "codepicture";` | `554 passed; 0 failed; 1 ignored` | **0** | ——（**一条都不红**：那条判据此前对第二个插件是**空的**） |
| **M8b** | 针 = 11 根（旧 6 ＋ 新 5）＋ **同一刀** | `553 passed; 1 failed; 1 ignored` | **1** | ``通用插件调用口里出现了**某一个具体插件**的词汇：``  ``  plugin/discover.rs:53 [另一个插件的数据目录名] let _ = "codepicture";`` |

⇒ 新红**恰好 1 条**、**正是那一格**（`plugin::layer_guard::the_generic_port_names_no_concrete_plugin`）、
**最小面**（逐字点名那一行，不牵连别的判据）。刀撤干净：
`git diff -- plugin/discover.rs` **逐字节相同**。

**同轮做的两件（不做就会立刻长出新债）**：
- **散文收成一个住址**：模块头注那张表 · `concrete_plugin_words` 的头注 · 正题那条的射程段
  此前**各抄了一份**「**6 根针** ＋ 逐个列名」。加第二族之后三处一起变假 ⇒ 一律改成**只给住址**
  （报错文案里那个数本来就是 `words.len()` 现算的）。⚠ 本件自己那份文档里也抄过一处，同轮改掉。
- **阴性对照扩成「一族一个样本」**：只喂一族的话，另一族的针**整族失效**会被这一族盖住
  （本仓「N 个独立源只断一次」踩过）。新加一族词就在那里加一行样本。

⚠ **射程如实写**：加完之后它仍然**只认表里那两族**。本件的假插件换的是第三族，
这张表对它**一根都对不上** —— 那一半由 `plugin_walk_fixture` 自带的一组专有针管（`E6`）。

⚠ **顺带一条给 PM / `K-W2D` 的读数（我没改它）**：monitor 侧
`plugin_class_registry` 那条「daemon 整棵源码树零全景引擎」用的针是
`format!("code{}picture", '_')` ＝ `code_picture`，且走 `contains_word`（`-` 与 `_` 都算词内字符）
⇒ 它**匹配不上** `codepicture`、`code-picture`，连 `code_picture_core` 也因尾随 `_` 而不算词命中。
本轮这 5 根针与它**几乎不重叠**（是互补，不是重复）；那条针自己的射程要不要收，归 `K-W2D` 判。

## 6 · 顶回 PM 题面的地方（逐条带现打读数）

① **`control/cc_bus.rs` 是 7 条判据，不是 6 条。**
   件文件 `§0c` 与 `KW2E1` 边界② 两处都写「`cc_bus.rs` 6 条」。
   现打 `grep -c '^\s*#\[test\]' src/control/cc_bus.rs` ⇒ **7**；
   而且 `git show 4eb271f:remote-daemon-proto/src/control/cc_bus.rs | grep -c` 也是 **7**
   ⇒ 那个数**在 PM 自己的口径提交上就已经是 7**，不是本树多长了一条。
   第 7 条逐字叫 `the_daemon_does_not_re_implement_the_recipient_charset_rule`。
   ⇒ 「插件口 25 条 + cc_bus 6 条 = 31」这个分母应为 **32**。
   〔结论方向不变：32 条里同时驱动 ≥2 跳的仍然是 **0** 条。〕

② **乙档的代价，件文件 `§0b` 那一行是估的，不是量的。**
   逐字：「`layering_guard` 里进 `plugin` 的边要多登记几条，而那张表**条数钉死**（今天 5）」。
   现打：`the_interface_into_plugin_is_exactly_the_registered_set` 的人群逐字是
   `for layer in ["control", "observe"]`，而 `layer_sources` 按 `src/<层名>` 拼路径
   ⇒ **顶层文件不在它的采集面里**；它扫的又是 `production_code` 的产物，
   而本件每一条 `plugin` 边都在 `#[cfg(test)]` 里 ⇒ **两道构造性摘除各自独立**。
   ⇒ 乙档对那张表的真实代价 = **0 条**，`ALLOWED_INTO_PLUGIN` 本件一个字节没改。
   ⚠ 而这同时是一条**射程读数**（归 `KW2E5`，也直接影响 `K-W2D`）：
   那张表自称「进 `plugin/` 的边逐条登记」，真实射程是「**`control/` 与 `observe/`
   两层生产段里**的边」。⇒ **`sidecars/` 那一层将来调 `plugin::invoke::run`，
   这张钉死 5 条的表一个字都不会说。**

③ **甲档在本轮写区里做不了**（不是「不想做」）：`plugin/mod.rs` 那条
   `the_plugin_layer_collection_is_complete` 的诊断文案逐字要求「**同轮**加进
   `agent_boundary_guard::CORE_FILES` 与它的棘轮表」，而 `agent_boundary_guard.rs`
   不在本轮写区。另有一条**技术性**理由（更硬）：假插件必带它自己那份码表，
   而那张表的形状正是 `plugin::layer_guard` 形状那一族认的两根针
   ⇒ 甲档会被通用层自己的判据当场打红，**而那不是误伤**。

④ **`pb check` 那一格的 FAIL=1 不是本件的账。**
   报文逐字 `FAIL [J3 陈账] INDEX.md 比源文件旧 —— 重跑 pb index 落盘`。
   现打计划仓 `git status --short` ⇒ 4 份**别人**的件文件有改动
   （`K-R2` · `K-R24` · `K-W1C` · `K-W3`），mtime 22:08–22:17，而 `INDEX.md` 是 21:14。
   ⇒ 别的道在写自己的件文件，INDEX 就旧了。
   🔴 本件**不跑 `pb index`**（那是生成命令，PM 的动作序：窗口开着期间一概不跑）。
   入场那一趟（同一棵树、同一个提交、22:00 前）这一格是 **FAIL=0**。

⑤ **npm 那一格红过一次，是 flaky，不是本件。**
   第一趟：`Tests 4 failed | 1537 passed (1541)`，四条全在 `src/ipc/commands.vitest.ts`，
   报文全是 `Test timed out in 5000ms`（`Duration 227.49s`，`environment 2276.87s`
   —— 本波 13 道并发在抢 CPU）。
   凭什么说不是本件：那个套件的 Rust 语料面逐字是
   `walk(resolve(REPO_ROOT, "src-tauri/src"), ".rs")` ⇒ **它根本不扫
   `remote-daemon-proto/`**，而本件的改动全在那棵子树里（＋`main.rs` 一行 `mod`）。
   第二趟同一棵树同一份改动：`ok npm 1541 passed`。

## 7 · 边界：哪几跳本地验不了（`KW2E6`）

- **跳①③⑨ 归真机**：它们要一个真 daemon 收发帧，而派工令硬边界是「不起真 claude、不起真 daemon」。
- 今天真跑过那条路的是 `e2e/daemon-cc-bus.sh`，CI 地板 **50** ——
  住址 `.github/workflows/ci.yml:646`，那一行逐字
  `run: bash e2e/assert-pass-floor.sh daemon-cc-bus 50`（09-04 现打）。
- ⚠ **它不在门禁九格里**：`grep -c daemon-cc-bus scripts/gate.sh` ⇒ **0**（09-04 现打）。
  门禁只跑四套 `ccm` e2e。⇒ **出货那一刀看不见它。**
- ⚠ 而且它测的是**那个既有插件**，不是「加一个新插件」。
- 🔴 **不许把「归真机」当挡箭牌**：跳④⑤⑥⑦⑧ 本地验得了，本件一条都没往真机推。

## 8 · 两条环境前提（件文件写死，各贴读数）

### `timeout(1)` 在不在 `PATH`

- **沙箱里：在。** 现打 `plugin::discover::on_path("timeout")` ⇒ `Some(..)`。
- **凭什么说期限真的落在子进程上**（不是「argv 里有个前缀」这种源码级说法）：
  那个真进程报出**它自己的父进程名** ⇒ `("parent", "timeout")`。
- **真超时那一趟**：`code=Some(124)` · `timed_out()=true` ⇒ 跳⑦ 的 `TIMED_OUT_CODE`
  第一次由一个真被收掉的子进程驱动。
- **找不到 `timeout` 那一趟**：本机（沙箱）走不到，**这一格说得出口** ——
  判据按 `on_path` 的实得分叉，两条路各有断言；`None` 那条断言的是
  「父进程不是期限命令 ＋ argv 里一个秒数都没有」，并**刻意不跑**那条会卡住的子命令
  （没有期限的裸跑会把门禁挂死）。⇒ 那一侧买到的是「说得出口」，**买不到**「真被收掉」，
  如实记在判据头注里。

### 沙箱默认断网

假插件是一个 `#!/bin/sh` 脚本，只用 `printf` / `[` / `case` / `cat /proc` / `sleep`，
**一处网络都不碰**。整趟门禁在 `--network none` 下跑绿。

## 9 · PM 点名要顺带答的那一格（`DECISIONS.md` 末节 `R4`）

问：**今天 `plugin/` 协议里有没有「插件 → 宿主基础命令」这一跳？**

**答：设计上没有（零处）；而事实上有一条没人设计过的暗路。** 逐条给住址与读数：

1. **设计上零处**（机检落点 `nothing_here_hands_the_plugin_a_designed_way_back_into_the_host_commands`）：
   `plugin/` 四个文件的**生产段**里 `inbound` / `COMMANDS` / `REGISTRY` **零命中**；
   `layering_guard::plugin_layer_must_not_reference_control_or_observe` 对
   `plugin → control` 是**零容忍** ⇒ 通用调用口在结构上**说不出**一条宿主命令的名字。
   交给子进程的只有三样：argv（`argv_for` 的形状钉着）· 额外 env · **关掉的 stdin**。
   没有任何回程端点。
2. 🔴 **暗路**：`plugin::invoke::run` **不 `env_clear()`**（生产段 `env_clear` 零命中），
   ⇒ 子进程**继承 daemon 的整份环境**。现打的行为读数：那个真进程报回的
   `("inherited-path", "/opt/rust/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin")`
   与宿主进程的 `PATH` **逐字相同**（而 `PATH` **不是** `run` 喂进去的那一项）。
   而常驻监听口那两个变量恰好是「接上宿主 ＋ 过鉴权」需要的两样 ——
   住址 `remote-daemon-proto/src/listen.rs` 的 `ENV_PORT` / `ENV_TOKEN`
   （只给住址，值不在这里复述）。
   ⇒ **只要 daemon 是带着它们起的，任何被它起的插件读一读自己的环境就能回连宿主、
   发全部基础命令**，而今天没有任何判据在看这件事。
   ⚠ 这**不是**「已经有回调口了」：它没有协议、没有权限模型、没有审计 ——
   它是**缺口的一种形状**，不是能力。
3. **最小形状建议（一段话，本波不做）**：真要给插件一条回调口，它该是
   `invoke::run` 的**第五个入参** —— 一份「这次调用允许回调哪几条基础命令」的显式清单；
   落地形态是把常驻口的地址与一枚**一次性、按调用发、只授这几条命令**的短票显式塞进
   子进程环境。同轮必须做两件事：ⓐ `invoke::run` 改成**先 `env_clear()` 再显式喂**
   （不关掉今天这条暗路，加了清单也是空的）；ⓑ 那份清单的取值空间**钉在
   `inbound::COMMANDS` 上**（同 `Hello.unavailable` 的口径 —— 事前事后同一套词，
   不许自造第二套）。⇒ 三件东西同轮，缺一件这条口就是「看起来有权限模型」。
