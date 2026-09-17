# K-R27 读数簿 —— 14 条 `parked` 的时态审计（逐条待办）

> 量于工作树 `/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r27` @ `85cdf2d`
> （`git merge-base --is-ancestor 85cdf2d main` = YES ⇒ **它就是今天的主干尖**）。
> `parked` 文本取自 `.claude/planned-build/backend-consolidation/.dispatch.json`（**20 条**，`len(d['parked'])` 现打）。
> ⚠ 下面每个数都带「用什么量的」；带行号的都在旁边抄了逐字内容或钉了 sha（`brief` 13c）。

## §A 射程与分母

| 量 | 数 | 怎么数的 |
|---|---|---|
| 本区 `parked` 全体 | **20** | `python3 -c "import json;print(len(json.load(open('.dispatch.json'))['parked']))"` |
| 「明写等用户」**不核** | **6** | `K-P2` `K-P4` `K-R11` `K-R16` `K-R19` `K-R8` —— 按 `§0a` PM 给的划分照收，本件不重划 |
| **本件分母：非「等用户」** | **14** | 其余 14 条，闭集住 `evidence/K-R27-parked-staleness-probe.py` 的 `FOURTEEN` |
| 从这 14 条里**摘出的待办** | **53** | 摘法见 §B；逐条表在 §C |

**待办的摘法（先说切法再报数）** —— 一条 `待办` = `parked` 文本里**指名一件还没做的活**的句子。收：
`不 accept：… 未开 / 没做 / 没开 / 待摸底` 列表里的每一项 · `另立件` · `归下一拍` · `归 PM 落` ·
`PM 要改` · `归排期` · `未核` · `判不了`。
**不收**：纯记账句（`🔴 PM 账：pb dispatch 没吐单` 这类只陈述已发生的事、不指派活的）·
读数与论据 · 已完成事项的描述。⚠ 一条 `parked` 的 `不 accept` 列表用 `·` 分隔时，**按项拆**；
用括号嵌套罗列子项时（`K-W2D` 那条），**按子项拆**并在表里注明是拆出来的。
一句话里**两个分句各指一件不同的活**时也拆（`K-P3` 那条的 `P3-1a`/`P3-1b`：
「还没有生产调用点」与「账上恒空」在今天已经不是同一个答案）。

## §B 三档分布（🔴 三档各自有数，相加 == 分母）

| 档 | 条数 | 占 53 |
|---|---|---|
| **仍成立** | **38** | 71.7% |
| **已被别的件/别的提交做掉** | **14** | 26.4% |
| **说不准（判不了）** | **1** | 1.9% |
| 合计 | **53** | 100% |

⚠ **这张表与 `§C` 现打对拍过**（免得手数漂）：把 `§C` 里形如 `| <待办 id> |` 的数据行按判档分桶，
机器数出 **53 行 = 38 / 14 / 1** —— 与上表逐格相同。对拍口径：待办 id 形 `(P3|W1B|W1C|W2D|W2E|W4|R1|R2|R12|R22|R23|R24|R25|R9)-\d+[ab]?`，
判档按行内出现的 `**仍成立` / `**已被做掉**` / `**说不准**` 认，三者互斥、认不出就单列（本趟单列 0 行）。

**按 `parked` 条目计（另一把尺子，不与上面相加）**：14 条里**有 10 条**至少含一条**确证已被做掉**的待办
（`K-P3` `K-W1C` `K-W2D` `K-W2E` `K-W4` `K-R1` `K-R12` `K-R22` `K-R23` `K-R9`），
只有 **4 条**（`K-W1B` `K-R2` `K-R24` `K-R25`）整条**全部仍成立**。
⇒ 🔴 **PM 随手核的 2 条只是那 10 条里的 2 条**；「2 打 2」不是运气好，是**这一批里十条有账**。

## §C 逐条待办（53 行）

口径：**证据**栏给的是**今天现打**的命令与命中数；判「已被做掉」的点名提交/件，
判「仍成立」的给今天仍然存在的物证。「面 0 提交」= `git log --oneline 3ed7f1b..HEAD -- <路径> | wc -l` 为 0
（`3ed7f1b` = 09-04 23:57 第五波最后一条合入，即这批 `parked` 落笔的那一刻附近）。

### K-P3（3 条）

| # | 待办（逐字要点） | 判 | 证据（今天现打） |
|---|---|---|---|
| P3-1a | `record_death` 还没有生产调用点 | **已被做掉** | `K-P3b`（`state: 已签收`；merge `3e98353`，实现 `40cf161`）。`git grep -n record_death -- src-tauri/src` → 生产调用点 **3 处**：`local_daemon.rs:776` / `:1278` / `:1301`（三处逐字都是 `shout_if_the_ledger_refused(crate::daemon_policy::record_death(`），并由 `daemon_policy.rs::the_death_ledger_is_wired_at_exactly_these_sites` 逐处点名钉住 |
| P3-1b | 账上恒空（= `exit: 待摸底` 的第一问：跑一段时间之后那本账上有没有出现过一次真崩溃） | 🔴 **说不准** | **时态真的变了，但变成了「判不了」而不是「做掉了」**：09-04 它恒空是**结构性的**（零调用点，P3-1a）；今天接线在了，恒不恒空**取决于真跑**。<br>**缺什么才判得了**：一台**真的跑过一段时间**的 monitor 的死亡账读数。而账**不跨 monitor 进程** —— `K-P3` 件文件 `:503` 逐字「账**不跨 monitor 进程**（`LEDGER_IS_PROCESS_LOCAL`）⇒ 这一问今天**答不出来**」⇒ **从盘上读不出来**，静态尺子一把都够不着。要判得了：① 真机跑一段并把那本账落盘，或 ② 先做跨进程持久化（而那本身是第二档的题目，见 P3-3） |
| P3-2 | 三处接线（`local_daemon reap_detached` · `local_backend supervise_with_stdio` · `daemon-section.ts` 另起一行）归下一拍单独一件 | **已被做掉** | 同 `K-P3b`。⚠ **形状变了**：`supervise_with_stdio` 那一处 K-P3b **反过来禁掉**了 —— `daemon_policy.rs::the_supervisor_itself_never_records_a_death` 断言该函数体内 `record_death(` **恰好 0 处**、整棵 `backend/` 也 0 处（理由逐字：「它同时监护 daemon 与中转 ⇒ 中转的死会被记进『这台机的 daemon』那本账」）；界面那一处落在 `src/settings/daemon-section.ts:47`（逐字「从 `daemon_status` 那份 JSON 里取死亡账读数」） |
| P3-3 | 第二档未开（`KP3E` 放宽相等断言 · `KP3F` 崩溃循环天花板搬进 daemon） | **仍成立** | `remote-daemon-proto/src/readonly_guard.rs:331` 逐字 `whitelisted, 1,`；`:816` 逐字 `const SPAWN_SITES_TODAY: usize = 9;`、`:819` 仍是 `assert_eq!` 那一形；`ratchet_guard.rs:68`/`:90` 反过来钉着「这里用的是 `assert_eq!` 而不是地板」。`readonly_guard.rs` 面 **0 提交**。daemon 侧 `git grep 'crash_loop'` → **0 命中** |

### K-W1B（3 条）

| # | 待办 | 判 | 证据 |
|---|---|---|---|
| W1B-1 | `D3`（27 处收进接口，写区 `observe/**`）未开 | **仍成立** | 现打 `python3 evidence/K-W1B-D1-agent-coupling-census.py` 尺子① 合计 **27**，逐文件 `control/fork_write.rs 3 · control/resolve_query.rs 6 · main.rs 1 · observe/accounts_query.rs 3 · observe/history_query.rs 5 · observe/search_query.rs 2 · observe/usage_query.rs 3 · observe/watcher.rs 4`，与 09-04 逐字相同 |
| W1B-2 | `D4` 未开（最小假 agent 实测「加一家、通用层零改动」） | **仍成立** | `AgentKind` 今天仍是住通用层的**两值闭合枚举**（`src-tauri/src/adapter.rs:19-23`，`pub enum AgentKind { ClaudeCode, Codex }`）；`git grep FIXTURE_HOMES -- src-tauri/src` → **0 命中**（桌面侧仍没有夹具家）；daemon 侧 `agents/fake/mod.rs` 在。`adapter.rs` 面 **0 提交** |
| W1B-3 | `D5` 未开（交 `K-W3` 的证据面） | **仍成立** | 件文件 352 行里 `§3` 逐字「刀 `D5` · 无（`D5` 是人评，交的是论据面）」，无交回正文。⚠ **前提变了**：消费者 `K-W3` 今天 `state: 已签收`、`KW3D5` 拆件表已出（`§0b-5`，W3-1…W3-7），表里逐字把它记成「归别人 / 待 `K-W1B`」⇒ **收货方已经收口了**，派它之前先决定它还要不要 |

### K-W1C（5 条）

| # | 待办 | 判 | 证据 |
|---|---|---|---|
| W1C-1 | `D4` 甲留未决（Windows 真机可达性） | **仍成立** | `evidence/K-W1C-D4-reachability.md` 仍是唯一读数（判不了 + 可照做的步骤）；仓里没有任何 Windows 真机读数落盘；`src-tauri/src/bind.rs` 面 **0 提交** |
| W1C-2 | `D2` **跨区那一行**由 PM 落（往 `unified-backend/ROADMAP.md` 的 `U8e` 下加订正） | **仍成立** | `grep -c 'K-W1C' unified-backend/ROADMAP.md` → **0**；`U8e` 那一行今天仍逐字写着「杀会话/换会话时**没有任何机制**去 `forget` 那条缓存」；`stat` 该文件 mtime = **2026-09-01 18:30**（早于这条待办诞生）⇒ 09-04 之后没人碰过它 |
| W1C-3 | `D5`/`D6` 只有论据 | **仍成立** | 件文件 `§5-5` 标题逐字仍是「`D5` / `D6` —— **只写论据，不改**」，正文逐字「我按前者给论据，**不动代码、不自批结论**」；今天盘上对上：边表里 verify-fail 那条边（`src-tauri/src/lib.rs:2030` 逐字 `cache.forget(&session_id);`）判据栏仍 **0**，`git grep verify_binding -- src/` → **0 命中**（`D6` 那一侧一个字没接） |
| W1C-4 | 建边 3 条 + verify-fail 那条 + emitter→入口那一跳仍 0 判据 | **仍成立** | 现打 `python3 evidence/K-W1C-D1-edge-ruler.py`：**7 条边 · 有判据 2 条 · 判据栏 0 的 5 条**（机检口径）＝ 登记 6 行口径下「建边 3 + verify-fail 1 = 4」。⚠ **两个口径都要写**：机检 7 条 ↔ 登记 6 行（一行盖住两个逐字同形的调用点），单报一个会被读成漂移。与 09-04 的「七条边（判据栏 2/7 有名）」逐字相同 |
| W1C-5 | `lib.rs` 改点①上方残注释由 PM 落 | **已被做掉** | 提交 `261fcc1`（merge `a4e0acb`），`src-tauri/src/lib.rs:693-698` 那段注释换成「「两种 cause 一视同仁」这句话的正主住 `SidHwndCache::apply_local_removal` 的头注」 |

### K-W2D（4 条 —— 由 `不 accept` 那个括号列表拆出）

| # | 待办 | 判 | 证据 |
|---|---|---|---|
| W2D-1 | 真做那拍的「拉取协议 · 哈希 · 唯一失败面」还没开 | **仍成立** | `git ls-files -z \| grep sidecars/` → **0**；`remote-daemon-proto/src/panorama_locus_guard.rs:29` 逐字「**第三棵树今天不存在** —— S2（本仓自己出一个薄 sidecar crate）落地那天会出现一棵」；该文件面 **0 提交**，仍 **5** 个 `#[test]` |
| W2D-2 | `KW2D6` 五根针落地〔已交 `K-W2E`〕 | **已被做掉** | `K-W2E`（merge `d306542`，实现 `466618f`「K-W2E：通用口的词汇针补上第二族（代码全景那个插件），并把「6 根针」那几处散文收成一个住址」）；K-W2E 自陈「`KW2D6` 五根针已落（M8a 0 红 / M8b 1 红）」；今天词汇针在 `remote-daemon-proto/src/plugin/mod.rs:107` / `:662` |
| W2D-3 | `KW2D5`（`readonly_guard` 从「判据看不见」变成「有人签过字」）〔`K-R2` 在做〕 | **已被做掉** | `K-R2`（merge `bb7aeac`，实现 `b8d1cbe`「依赖 crate 的写面：从『判据看不见』变成『有人签过字』（不补人群）」）；今天 `readonly_guard.rs:1582` 起有签字表，判档闭集三值（`:1614-1616` `MEASURED_WRITES` / `MEASURED_CLEAN` / `UNMEASURED`） |
| W2D-4 | `KW2D4`（引擎取用口在跨进程尺度上恰好一处）〔`K-R2` 在做〕 | **仍成立（剩下那半）** | `K-R2` 兑现的是 monitor + daemon + 共享 crate **三棵**；而件文件 `:416` 逐字「`KW2D4` 的形状取决于 `§4a` 的答案 —— 选了 `S2` ⇒ 要钉的是「**三棵树**合起来恰好一处」」，`§4a` 选的正是 `S2`（`:316` 逐字「**① 谁出那个二进制：选 `S2`**」），而 S2 的第三棵树今天不存在（同 W2D-1）⇒ 剩下那半**没有落点**。今天生产段 `Engine::open` 仍恰好 1 处（`src-tauri/src/panorama.rs:87`） |

### K-W2E（4 条）

| # | 待办 | 判 | 证据 |
|---|---|---|---|
| W2E-1 | 🔴 `R4` 暗路：`invoke::run` 不 `env_clear` ⇒ 插件继承 `ENV_PORT`/`ENV_TOKEN` 可回连宿主 —— PM 裁立件，先关暗路 | **已被做掉** | `K-R26`（`state: 已签收`；merge `45e9ad3`，实现 `3a7dca0`「插件不再继承 daemon 的整份环境 —— `env_clear` + 一张具名白名单」）。今天 `remote-daemon-proto/src/plugin/invoke.rs:196` 逐字 `cmd.env_clear();`；`plugin_walk_fixture.rs:1652` 钉「生产段 `env_clear` 恰好一处」、`:1859` 钉行为那一半 |
| W2E-2 | 跳①③⑨ 要真 daemon | **仍成立** | 现打 `python3 evidence/K-W2E-hop-census.py`：47 条判据（分母 = 6 个语料文件里 `#[test]` 的处数）覆盖跳 **2/4/5/6/7/8**，跳 **①③⑨ 仍 0 条直接命中** |
| W2E-3 | `timeout(1)` 缺席那趟只买到说得出口 | **仍成立** | `plugin/invoke.rs` 相关两条判据仍喂**合成路径**（`:231` / `:270` 逐字 `let t = PathBuf::from("/usr/bin/timeout");`），没有一趟真的在缺 `timeout` 的环境里跑；该文件 09-05 的 2 个提交都是 `K-R26` 那一刀 |
| W2E-4 | `layering_guard` 够不着层外调用方（归 `K-W2D` 真做拍） | **仍成立** | `remote-daemon-proto/src/layering_guard.rs` 头注射程逐字只讲 `observe/`↔`control/` 两层与「允许跨层的边今天**恰好两个符号**」，没有层外调用方那一维；而 `K-W2D` 真做拍未开（同 W2D-1） |

### K-W4（3 条）

| # | 待办 | 判 | 证据 |
|---|---|---|---|
| W4-1 | 正题 `D2`/`D3`/`D5`/`D6`（改名落地 · lockstep · 文档 · 全局 Phase G）本波没做，**等 `K-W3` 拆件表** | **仍成立（而挡路的换了）** | 活确实没做（`K-W4 state: 未开`；`evidence/K-W4-D1-rename-surface.md` 之后无续作）。⚠ **「等 K-W3 拆件表」今天不成立**：`K-W3 state: 已签收`，`KW3D5` 拆件表已在盘上（`§0b-5`，W3-1…W3-7），表里逐字写着 **`K-W4` 的唯一剩余前置是 `W3-7`**（两个 cargo 构建单元 → 一个），而 W3-7 自己卡在「体积预算那个数 **或** 待 `K-W2D`」⇒ 挡路的**换了名字，不是消失了** |
| W4-2 | 🔴 取样层零判据 ⇒ 下一拍先立**可注入假 SFTP** | **仍成立** | `src-tauri/src/sftp.rs:479` 逐字 `async fn probe_target_binary(sftp: &SftpSession, path: &str) -> TargetBinary` —— 仍吃具体类型，没有 trait/注入点；`git grep 'FakeSftp\|fake_sftp'` 全树 **0 命中**；`sftp.rs` 面 **0 提交** |
| W4-3 | 另归 PM 落：daemon 侧 `build_id_guard.rs` / `main.rs:141` 两句「唯一判据就是 build_id」馊了半句 | **已被做掉** | 提交 `261fcc1`（merge `a4e0acb`），两处都加了「**版本那一维**」限定（`build_id_guard.rs` +2/-1 · `main.rs` +2/-1，diff 逐字可见）。⚠ 原文点的 `main.rs:141` 今天落在 `:143`（+2 行）—— 行号漂了，内容对上了 |

### K-R1（5 条）

| # | 待办 | 判 | 证据 |
|---|---|---|---|
| R1-1 | 转换层未做 | **仍成立** | `git grep '转换层' -- src-tauri/src remote-daemon-proto/src doc` → **0 命中**；`src-tauri/src/creds_store.rs` 与 `remote-daemon-proto/src/relay/table.rs` 面**各 0 提交** |
| R1-2 | `message_start` usage 结构性填不出真值那格压着 | **仍成立** | `git grep -c message_start` 全树 → **1 处**，在 `remote-daemon-proto/src/relay/tee.rs`，是一条**测试用 wire 字面量**（`b"event: message_start\ndata: …"`）⇒ 生产段没有任何 usage 填充 |
| R1-3 | 界面入口没接 | **仍成立** | `git grep -c 'auth_style\|authStyle' -- src/` → **0 命中**；命中全在 daemon relay（`relay/table.rs` 39）与 monitor/crates 的 store（`creds_store.rs` 7 · `crates/creds-core/src/store.rs` 35） |
| R1-4 | 前缀末段 == 请求首段的专门出声归下一拍 | **仍成立** | 两个可能落点 `creds_store.rs` / `relay/table.rs` 面**各 0 提交**；`doc/IPC-PROTOCOL.md` 的三行订正只说「前缀接在原样路径前且**不合并重复段**」——**记的是现状，不是出声** |
| R1-5 | `IPC-PROTOCOL` 三行 PM 在 `K-P2` 合完后落 | **已被做掉** | 提交 `a0aa270`（merge `9acbdd8`），标题逐字「`doc/IPC-PROTOCOL.md`：K-R1 交回的三行订正（基址可带路径前缀 · 前缀接在原样路径前且不合并重复段 · 鉴权头两个例外）」，`+16/-1`。**第二把尺子**：`git log -S '不合并重复段' -- doc/IPC-PROTOCOL.md` 只回这一条 |

### K-R2（5 条）

| # | 待办 | 判 | 证据 |
|---|---|---|---|
| R2-1 | 真搬家（引擎按需拉取 · 第三棵树）没开 | **仍成立** | `grep -n 'code-picture' remote-daemon-proto/Cargo.toml` → **0 命中**（只有 `src-tauri/Cargo.toml:61` 那一份）；`ls src-tauri/crates/` → 7 个（`acct-core branch-core creds-core gate-core guard-core shell-quote-core usage-core`），没有 sidecar 那棵 |
| R2-2 | 第三棵树那半（`KR23` 作用域） | **仍成立** | 同 R2-1 + W2D-4：那半的落点就是不存在的第三棵树 |
| R2-3 | 「已量未见写面」那档**常驻判据** | **仍成立** | `remote-daemon-proto/src/readonly_guard.rs:1631-1636` 那一档的解锁条件逐字：「⚠ 如实写明：**没有任何判据会在那一刻自动红** —— 这一档的尺子是签字那一刻现打的，不是常驻的。要常驻就得补人群，那是 `K-G6` 的射程」；该文件面 **0 提交** |
| R2-4 | `K-G6` 人群都空着 | **仍成立** | `K-G6 state: 已签收 \| exit: 改件`（08-28 就收了），而它的 `§2` 逐字把这一格**明确排除**：「**不清判据面的存量问题**（恒绿 / 假阳 / **人群不符**那一族有 20 余条，归各自的件）」（`features/K-G6-….md:103`）⇒ 它签收了，人群那一格**它从没做**。R2-3 那句解锁条件今天仍把落点指回它；`readonly_guard.rs` 面 **0 提交**（要补人群必然要动它） |
| R2-5 | 与 `K-W2D` 的 `panorama_locus_guard` 重叠格，落法归 `K-W2D` 真做拍 | **仍成立** | `remote-daemon-proto/src/panorama_locus_guard.rs` 面 **0 提交**，仍 **5** 个 `#[test]`；`K-W2D` 真做拍未开（W2D-1） |

### K-R12（4 条）

| # | 待办 | 判 | 证据 |
|---|---|---|---|
| R12-1 | 「旗真的被走到了」仍只由上一拍 evidence 背书 | **仍成立** | `remote-daemon-proto/src/common/` 面 **0 提交**；`common/tmux_utf8.rs` 里的判据都是结构形（家唯一 `:246` · 消费者只引用 `:286` · 反向自检 `:318`），没有一条证「运行时真被走到」 |
| R12-2 | SSH 送 env 那行仍未实测 | **仍成立** | 09-05 唯一碰真 SSH 的是 `W-F1b`（`7265f7d`，5 份量具 + 5 份 `.out`）：`grep -c 'LC_ALL\|send_env\|SendEnv' evidence/W-F1b-*` → **10 个文件全 0**；`e2e/weak-net/` 5 个脚本里也没有 |
| R12-3 | `K-R23 D4` 另立 | **仍成立** | `features/` 61 件里没有承接 `K-R23 D4` 的新件（`ls features/ \| grep -c 'K-R23'` → 1，就是 K-R23 自己）；`K-R23 state: 未开` |
| R12-4 | 🔴 PM 补定框 | **已被做掉** | 计划仓提交 **`17ac1c5`**（09-04 23:43）标题逐字「backend-consolidation: 第五波回收中段 —— 六道审计 + 合并记录 + R2 落定 + **K-R12/K-P3 补定框链接**」。<br>**尺子**：`git log -L3,3:'backend-consolidation/features/K-R12-tmux34上那条静默失效.md'` 在计划仓里跑，那一行的历史只有两笔 —— `b124d9a` 建件时 `links: K-R8, K-R7`，`17ac1c5` 改成 `links: K-R8, K-R7, K7`。`K7` 是 `MASTERPLAN.md` 里的 charter（本区 charter 闭集现打 **20** 条）。<br>🔴 **这一行是我自己纠回来的**：初判写的是「说不准 —— 计划仓不在 git 里」，那是**量具的作用域对错了对象** —— 我量的是 `.claude/`（确实不是 git 仓），而真正的仓根是 `.claude/planned-build/`（`git rev-parse --show-toplevel` 现打就是它，件文件也真的被跟踪）。**见 §E-3** |

### K-R22（2 条 —— 🔴 PM 已核 1 条，本件复打）

| # | 待办 | 判 | 证据 |
|---|---|---|---|
| R22-1 | `D3` 动态射程只覆盖 8 个调用点里的 2 个，「只删一个调用点」今天逮不住（登记为已知边界，等真有人删了调用点再谈） | **仍成立** | `scripts/gate.sh` 今天 **891 行**；`gate_diag` 剔掉注释行后 **9 行 = 1 处定义（`:285` `gate_diag() {`）+ 8 个调用点**（`:353 :356 :410 :416 :419 :747 :753 :871`）⇒ 比例仍 **2/8**。<br>⚠ 09-05 `N-G1`/`N-G2`（**别的工作区**）确实往这段加了探针③④与⑤–⑩，但它们钉的是**另一维**：`gate.sh:479-486` 逐字引 K-R22 的射程「`gate_diag` 本体 ＋ `run_gate` / `run_gate_sum` 两个失败支，8 个调用点里的 2 个」并写明「**「哪一条判定在判」从来不在谁的射程里**」⇒ 覆盖面**没有**扩到 gate_diag 调用点。<br>⚠ 附带：parked 里「`gate.sh` 478→627 行」这个**读数今天已馊**（891） |
| R22-2 | 🔴 另立件待办：`race_watchdog_times_out_on_blackhole` 是判据的问题 | **已被做掉** | `K-R24 D2`（`ab48245`，`git merge-base --is-ancestor` = 主干祖先；标题逐字「看门狗那条判据的前提，改成它自己建立、自己断言」）。今天那条判据叫 `race_watchdog_times_out_on_a_silent_peer`（`src-tauri/src/ssh_source.rs:7346`），第一行 `:7347` 逐字 `let (silent, trace) = spawn_silent_peer().await;`；`spawn_silent_peer` 定义在 `:7230`。<br>🔴 **PM 那句「旧名全仓 0 命中」复打对不上**：旧名今天全仓 **4 处命中 / 2 份文件**（`evidence/K-H2b-C10-sbx-fastlane.py:27` · `evidence/K-R24-D1-premise-census.md:53 :80 :115`）。**生产段 0 命中**是对的、结论方向不变，**但「全仓」这个分母写错了**（`brief` 12 那一族） |

### K-R23（3 条）

| # | 待办 | 判 | 证据 |
|---|---|---|---|
| R23-1 | `D4` 答了「`kill` 那条的安全该显式化」但明确没做，标为可能的独立一件；PM 认但**不现在立** | **仍成立** | `features/` 里没有这一件；`remote-daemon-proto/src/control/gate.rs` 的 Gate 3 语义一字未动（`:271` 逐字仍是「⚠ **Gate 3 只给破坏性动作**：`send-keys` 不删除任何东西，窗口数与它无关」） |
| R23-2 | 🔴 PM 要改自己两处：件文件 `KR23D1` 那两个行号要标为错 | **已被做掉** | 计划仓提交 **`110c15d`**（09-04 18:07）「PM 收尾两笔：**订正题面里抄来的错行号** + 补上那条 flaky 的重复趟」（尺子：`git log -S'~~⚠ PM 现打知道两个' -- <该件文件>`，**只回这一条**）。今天盘上：`features/K-R23-那道门在通道脏时放行.md:55` 带删除线，`:57-61` 是「🔴🔴 〔09-04 PM 订正，实现方现打推翻〕」段，并给出真调用点 `:495`/`:587` |
| R23-3 | 纪律从「不许抄交回数」扩写成「不许抄交回数、行号、或任何位置指称」 | **已被做掉** | 同一条计划仓提交 **`110c15d`**（尺子：`git log -S'交回数、行号、或任何位置指称'` 回三条，最早那条即 `110c15d`；另两条 `153feb7`/`10c528f` 是同一天的 STATUS / 审计落点）。今天三处在盘上，承重句逐字相同：`STATUS.md:245` · `STATUS.md:353` · `features/K-R23-….md:66`；来历在 `audits/K-R23-PM.md:110` |

### K-R24（4 条）

| # | 待办 | 判 | 证据 |
|---|---|---|---|
| R24-1 | 外部二进制 · 非回环地址两条轴没量 | **仍成立** | `evidence/K-R24-D1-premise-census.md:25-31` 逐字仍是「本件实打的 A/B **只换了一条轴：网络**…其余的轴（宿主家目录内容 · 外部二进制在不在 · 有没有非回环地址 · 时区 / locale …）本件**没有**做 A/B，**一条都没量**」。09-05 唯一动网络的 `W-F1`/`W-F1b` 建的是**弱网台架**：`grep -n 'cargo\|gate.sh\|npm run' e2e/weak-net/rig.sh` → **0 命中** ⇒ 它不跑整趟门禁，不构成这条轴的 A/B |
| R24-2 | 写区外 4 处 ETXTBSY 人群（含生产段 `extract_embedded_to` → exec）没治 | **仍成立** | 现打 `python3 evidence/K-R24-D7-exec-after-write-census.py`：分母① **187** 份 `.rs`（三个根、去 vendor/target）· 分母② 候选面 **10 处**。`git grep 'os error 26'` 全树命中**只在** `src-tauri/src/launch.rs`（`:1411` 判别 + `:1443` 起的上限重试）⇒ 生产段那条路 `local_backend.rs:673 extract_embedded_to` → `platform_fs.rs:51 make_executable` 一个字没动 |
| R24-3 | `accounts_query.rs:2236` 负载 flaky 无主 | **仍成立** | 那条判据今天叫 `an_inherited_launch_id_is_never_reported_as_the_childs_own_identity`，住 `remote-daemon-proto/src/observe/accounts_query.rs:2351`；仍无人认领（该文件 09-05 的两个提交是 `cb67bcc` N-F1c 与 `59215aa` PM 注释家务，都没碰它）。<br>⚠ **活体**：`cb67bcc` 在它前面插了净 +189 行 ⇒ **行号从 `:2236` 漂到 `:2351`**，照 parked 的行号去指今天指的是别的东西（正是 `brief` 13c 那一族） |
| R24-4 | 负载轴人群大小判不了 | **仍成立** | `evidence/K-R24-D8-load-axis.md:305-340` 逐字给的两个必要条件 ——「一个**能重复跑整趟门禁 N 次**的台子」＋「一台**不与别人共用**的机器」—— 今天盘上都没有（`e2e/weak-net` 那套不跑门禁，同 R24-1）⇒ 那句「人群大小判不了」今天仍然成立 |

### K-R25（5 条）

| # | 待办 | 判 | 证据 |
|---|---|---|---|
| R25-1 | 「窗口」第三个单位（`tmux_daemon_gate_guard` ×4 · `parity_ledger:468` · `structural_scan:746`）在写区外 | **仍成立** | 现打 `python3 evidence/K-R25-D1-strip-input-unit-census.py`（它自报「盘上与 `HEAD` 一致」@ `85cdf2d`）：命中总数 **315**，`SUBUNIT` 仍 **6 处**，逐处住址与行号与 09-04 逐字相同（`parity_ledger.rs:468` · `structural_scan.rs:746` · `tmux_daemon_gate_guard.rs:442 :477 :512 :552`）；这三份文件面**各 0 提交** |
| R25-2 | 姊妹守卫 `local_backend.rs:2315-2379` 在写区外 | **仍成立** | 那一格今天是 `the_exit_path_really_stops_the_local_backend`（`src-tauri/src/backend/control/local_backend.rs`），仍靠**手写大括号配平**切退出臂那个**窗口**（`&prod[open..end]`）；该文件 09-05 的两个提交（`fb3bcea` K-P3b · `59215aa` PM 家务）都没碰这一格 |
| R25-3 | `tool_registry.rs:577` 私有剥法**进没进登记表未核** | **仍成立（而它今天零成本可关）** | 没有任何件 / 审计 / evidence 记过这个核（`grep -rn 'tool_registry' audits/ features/K-R25*` → 0）。⚠ **现打读数摆出来**：它**在表里** —— `src-tauri/src/structural_scan.rs:635` 逐字 `"tool_registry.rs::production_code",`，`:603` 还写着登记时读出的那件真事；`git log -S 'tool_registry.rs::production_code' -- src-tauri/src/structural_scan.rs` 只回 `62df1da`（老提交，不是 09-04 之后新加的）⇒ **待办本身没被谁做掉，但答案已经在盘上，关它要一条 grep** |
| R25-4 | `.ts` 轴无看门判据 | **仍成立** | `assert_block_comment_model_holds` 的两个 WATCH 调用点都只喂 `.rs` 树（`src-tauri/src/local_daemon.rs:4862` → `CARGO_MANIFEST_DIR/src`；`remote-daemon-proto/src/single_stream_guard.rs:393` → 同形），函数体 `guard-core/src/lib.rs:645-647` 逐字 `if path.extension()… != Some("rs") { continue; }` ⇒ `.ts` 一个字没进 |
| R25-5 | `D4` 拉进 `crates` + `build.rs` 只答没做 | **仍成立** | 两个 WATCH 调用点的根**逐字**都是 `Path::new(env!("CARGO_MANIFEST_DIR")).join("src")`（`local_daemon.rs:4863` · `single_stream_guard.rs:394`）⇒ `src-tauri/crates/**` 与 `src-tauri/build.rs` **不在看门人群里**；`git grep 'build.rs' -- src-tauri/crates/guard-core/src/lib.rs` → **0 命中**；`guard-core/src/lib.rs` 自 `3aab95a` 之后 **0 提交** |

### K-R9（2 条 —— 🔴 PM 已核 1 条，本件复打）

| # | 待办 | 判 | 证据 |
|---|---|---|---|
| R9-1 | 🔴 那个洞要另立件（治法要动判据抽取逻辑）—— PM 下一拍立 | **已被做掉** | `K-R25`（`3aab95a`；`git merge-base --is-ancestor 3aab95a HEAD` = **ANCESTOR**；标题逐字「K-R25 真改拍：剥法的输入单位抬回文件，看门判据把块也当一个单位量」）。**尺子①** 读函数：`guard_core::assert_block_comment_model_holds`（`src-tauri/crates/guard-core/src/lib.rs:625`）签名今天是 `(root, min_files, min_blocks)`，体内两个单位各一条断言（`bad` / `bad_blocks`），`:653-655` 头注逐字点着 K-R9 那一形（`/*` 落 A 块、`*/` 落 B 块）。**尺子②** `git log -S 'min_blocks' -- src-tauri/crates/guard-core/src/lib.rs` → **只回 `3aab95a` 一条**。**尺子③** 两个调用点都真传了块级地板：`local_daemon.rs:4862` → `(…, 100, 1200)`；`single_stream_guard.rs:393` → `(…, 70, 500)`。<br>✅ **PM 这条复打成立**，连他写的行号 `:625` 今天也仍然对 |
| R9-2 | ⚠ PM 要在 `audits/K-R9-PM.md §三` 追加订正：「把那一整类关掉」要限定成「块内配平那一类」 | **已被做掉** | `audits/K-R9-PM.md:62` 逐字：「> 🔴 **〔09-04 晚 PM 订正，K-R9 落定拍第三刀逮到〕上面那句「把那一整类关掉」要加限定：关掉的是「块内配平」那一类。**」，落在 `§三`（`:38`–`:74`）内、紧跟 `:60` 那句原文。<br>🔴 **PM 只核了 R9-1，这一条他没核** —— 而它也已经做掉了 |

## §D `KR27D3` 的活体台子（读数，不是感想）

量具：`evidence/K-R27-parked-staleness-probe.py`（住址唯一，被测对象 = 它所在那棵树）。
一趟跑完 14 条，输出见下（**这是现打，不是抄的**）：

```
甲（点名的符号死了）出声：1 / 14        → 命中 K-R22
乙（落笔后那片面被动过）出声：1 / 14    → 命中 K-R9
```

| 件 | 锚点 | 取法 | 符号面 | 甲·死名 | 路径面 | 乙·落笔后被动过 |
|---|---|---|---|---|---|---|
| K-P3 | 3e98353d1 | ②件号兜底 | 0 | — | 0 | — |
| K-R1 | a0aa27015 | ②件号兜底 | 0 | — | 0 | — |
| K-R12 | 42fe109ba | ②件号兜底 | 0 | — | 0 | — |
| K-R2 | 416d70ee1 | ②件号兜底 | 0 | — | 0 | — |
| **K-R22** | 56c1b42 | ①文内 sha | 3 | **`race_watchdog_times_out_on_blackhole`** | 0 | — |
| K-R23 | 140ce0b | ①文内 sha | 1 | — | 0 | — |
| K-R24 | 9be5bdf94 | ②件号兜底 | 0 | — | 0 | — |
| K-R25 | 3ed7f1ba5 | ②件号兜底 | 0 | — | 0 | — |
| **K-R9** | 4443877 | ①文内 sha | 1 | — | 1 | **guard-core/src/lib.rs(1)** |
| K-W1B | d354094 | ①文内 sha | 0 | — | 0 | — |
| K-W1C | 261fcc17d | ②件号兜底 | 0 | — | 0 | — |
| K-W2D | 3656870 | ①文内 sha | 0 | — | 0 | — |
| K-W2E | 3a7dca0a4 | ②件号兜底 | 0 | — | 0 | — |
| K-W4 | 261fcc17d | ②件号兜底 | 3 | — | 0 | — |

**从这张表现打出来的三个数**（每个都写明分母）：

1. **准确率 2/2**：甲 ∪ 乙 出声 **2 条**（K-R22 · K-R9），**两条都是真阳** —— 恰好是 PM 手核逮到的那两条。**假阳 0 / 14**。
2. **召回 2/10**：而 §B 那把尺子算出来，14 条里**有 10 条**含至少一条**确证已被做掉**的待办 ⇒
   甲+乙 **漏掉 8 条**（`K-P3` `K-W1C` `K-W2D` `K-W2E` `K-W4` `K-R1` `K-R12` `K-R23`）。
   ⚠ 其中 `K-R12` / `K-R23` 那两条的落点在**计划仓**，而这两条候选只看**代码仓** ——
   **两条候选的射程里根本没有计划仓**，这是它们结构上够不着的一整类（8 条里占 2 条）。
3. **锚点那一级今天是坏的**：14 条里 **9 条**落到「②件号兜底」，而 ② 取的是
   `git log --grep=<件号>` 的**最新一条** —— 对一条**已经被修掉**的待办，那一条恰好就是**修它的那个提交**
   ⇒ `<锚点>..HEAD` 里当然什么都没有。**这是结构性的瞎，不是调参能救的**：
   `K-R1` 的锚点被取成 `a0aa270`（就是修掉 R1-5 的那一条）· `K-W2E` 被取成 `3a7dca0`（就是 `K-R26` 关暗路那一条）·
   `K-W1C`/`K-W4` 都被取成 `261fcc1`（就是修掉 W1C-5/W4-3 那一条）。
   ⇒ **乙要能用，锚点必须来自 `parked` 文本自己**，而今天 20 条里只有 **5 条**带文内 sha（K-R22 K-R23 K-R9 K-W1B K-W2D）。

## §E 诚实边界

1. **本件只核本区**（`backend-consolidation`）。别的工作区的 `parked` 有没有同样的病，**没量过**。
2. **那 6 条「等用户」的没核**（`K-P2` `K-P4` `K-R11` `K-R16` `K-R19` `K-R8`）——
   `§2.2` 明令，且它们等的是人不是代码。⚠ 顺带一句**没查实的观察**：`K-P2` 那条的 `不 accept` 列表
   （退出码 2/4 统一 · ccm-acceptance 挂门禁 · 真机往返零次 · exit 4 下游显示 · KP2D 题面住址馊了要改）
   读起来像**代码待办**而不是「等用户」—— 这条划分**我没核**，只登记，不改任何 `parked` 的处置（`§2.1`）。
3. 🔴 **我自己栽的一刀，如实登记（本仓最高频那一族：量具的作用域对不上事实）**：
   初稿里 R12-4 判的是「说不准 —— 计划仓不在版本控制里」，依据是
   `git -C /home/zbl/文档/claudecode-frontend/.claude rev-parse --show-toplevel` 报 not a git repository。
   **那把尺子量错了对象** —— 不是 git 仓的是 `.claude/`，而**真正的仓根是 `.claude/planned-build/`**
   （现打 `git -C .../planned-build rev-parse --show-toplevel` 就回它自己，件文件 `git ls-files --error-unmatch` 也认）。
   接住我的是一条无关的命令：交回前跑「三处 `git status`」时，计划仓那一处印出了「位于分支 main」。
   ⇒ 订正后 R12-4 变成**已被做掉**（`17ac1c5`），R23-2/R23-3 也从「点不出提交」变成**点得出**（`110c15d`）。
   **教训**：写「某某不在版本控制里」这种全称之前，要在**它自己那一层**上量一次，不能在祖先目录上量。
   ⇒ 三档里那唯一的「说不准」因此换了主 —— 今天是 `P3-1b`（要真跑才读得出的那一格），
   **不是**靠一条错读数凑出来的。
4. **「已被做掉」不等于「做成了 parked 预想的形状」**：P3-2 是活体 ——
   接线确实做了，但其中一处（`supervise_with_stdio`）被 K-P3b **反过来禁掉**并另找了落点。
   ⇒ 派工前只看「做没做」会漏掉「做成了别的形状」。
5. **W4-1 / W1B-3 是第三种时态**：**活没做（仍成立），而它写的那个「等谁」已经不成立**
   （`K-W3` 已签收 / 拆件表已出）。这一类**不会**被任何「做没做」的检查捕到，
   而它恰恰改变「下一件派什么」——`K-W4` 的挡路条件今天换成了 `W3-7`。
6. **§D 那两个数是这一批 14 条上的读数，不是这两条候选的固有性质**。换一批 `parked`、
   换一张噪声名单（`NOT_SYMBOLS` 现打 **20** 条，手维护 —— 量法：解析量具里那个具名常量，不手抄），
   数会变。引用前重跑。
7. **§C 里「面 0 提交」那把尺子的射程**：它答的是「这份文件从 `3ed7f1b` 到 `HEAD` 有没有提交碰过」，
   **不是**「这件事没人做过」—— 一件活也可能落在别的文件上。所以每一行都另配了一条**内容**读数
   （符号命中数 / 逐字内容 / 现打量具），两条一起才算数。
