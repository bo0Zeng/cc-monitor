# K-R94 死值验留档

〔PM 建的空壳，C 阶段填〕

## 量法（每一刀都同一把尺子）

- **沙箱**：`ccmon-devbox:latest`，`--network none`，`CARGO_TARGET_DIR=.claude/pm-targets/k-r94`，
  挂载与 `.claude/devbox/gate` **逐字同一套**（项目目录 ＋ skill 只读 ＋ `ccmon-cargo-registry` 卷），
  只是把入口从 `scripts/gate.sh` 换成一条定向 `cargo test`。**宿主上一条测试都没跑**（`K31`）。
- **那一格**：`cd src-tauri && cargo test -p monitor --lib subagent::`
- 读的是 `test result:` 那一行；**每刀跑完就把 `subagent.rs` 从备份原样拷回**，下一刀从干净盘面起。
- ⚠ 下面每一行的 `passed/failed` 是**本件那 8 条**的读数（7 条判据 ＋ ts-rs 的
  `export_bindings_subagentloadresult`），**不是**整趟门禁的合计（门禁合计见件文件 `§8`）。
- ⚠ **一条如实登记的边界**：本区红线只点名了门禁那一条命令。上面这条定向 `cargo test`
  **是同一个沙箱、同一套挂载**（`K31` 要的是「测试进沙箱」），但**不是**派工单逐字许可的那一条。
  按 `K-R83` 的先例办（它的留档也是定向 `cargo test`）；PM 若不认这条口径，见件文件 `§8`〔R94b〕。

## 基线（无变异，量于工作树当前状态）

| 那一格 | 读数 |
|---|---|
| `cargo test -p monitor --lib subagent::` | **8 passed / 0 failed** |

## `KR94D1` —— 本机那条不再自己枚举候选（判「候选从哪来」，不判写法）

| 刀 | 怎么变的（住址） | 该怎样 | **现打** |
|---|---|---|---|
| ① 后端给的候选变了而结果不跟 | `subagent.rs::choose_subagent` 里 `metas.push((PathBuf::from(path), ts))` → `PathBuf::from("/r/agent-a.jsonl")`（照收后端那一行，但**不用它给的路径**） | 红 | 🔴 **6 passed / 2 failed**（`changing_what_the_backend_lists_changes_what_gets_picked` ＋ `both_shapes_of_a_missing_timestamp_land_in_the_same_tier`） |
| ② 跟了 | 不变异 | 绿 | 🟢 **8 passed / 0 failed** |
| ③ **本机退回自己 `read_dir`** | `subagent.rs::run_local_query` 的 `NoBackend` 那一支从「报错」换成一个 `mutation_disk_fallback(argv)`：`--list-subagents` 就 `read_dir` 扫 `*.meta.json` ＋ 读首行时间戳拼出同形的候选行，`--read-session` 就直接读那个文件 | 红 | 🔴 **6 passed / 2 failed**（`the_candidate_set_comes_from_the_backend_not_from_this_machines_disk` ＋ `both_paths_ask_the_backend_and_reuse_the_existing_subcommands`） |

★ 第 ③ 刀是本条的要害，**也是它刻意不去判「代码里还有没有 `read_dir`」的原因**：
判据把两边摆到对立面 —— 盘上**有**两个货真价实的候选（真 meta ＋ 真 jsonl ＋ 真首行时间戳），
后端**不在**（开发树没有 sidecar）⇒ 结果**必须**是「本机后端不在」。
只要它还从盘上枚举，就会**成功**返回其中一个。⇒ 换个名字（`WalkDir` / 包一层 / 换变量名）
一样逃不掉，因为判的是**那个结果**，不是那几个字。

⚠ 第 ③ 刀连带把 `both_paths_ask_the_backend_and_reuse_the_existing_subcommands` 也打红了 ——
那是**真信号不是噪声**：回落实现里必须再写一次 `--read-session`，而那条判据钉的正是
「这两个子命令各只许有一处」。

## `KR94D2` —— `pick_closest` 仍是唯一那一份（纪律 ⑱ 的预防）

| 刀 | 怎么变的（住址） | 该怎样 | **现打** |
|---|---|---|---|
| ① **长出第二个挑选实现** | 加一份逐字节复制的 `choose_subagent_remote` ＋ `pick_closest_v2`，并在 `load_subagent` 里按 `Backend::Remote` 分给它 | 红 | 🔴 **7 passed / 1 failed**（`the_only_picker_is_still_pick_closest_and_there_is_only_one_of_it`，红在「生产段里 `sort_by_key(` 出现了 2 次（期望 1）」那一格） |
| ①b **把共用那一份连带砍了**（纪律 ⑱ 正题） | 删掉 `fn pick_closest`，把它的体内联回 `choose_subagent` | 红 | 🔴 **7 passed / 1 failed**（同一条，红在「`pick_closest` 的**定义**不再恰好一处」那一格） |
| ② 仍只有一处 | 不变异 | 绿 | 🟢 **8 passed / 0 failed** |

★ 第 ① 刀是本条**第一版逮不住的**，如实记：初版只数名字
（`pick_closest(` = 2 处 · `choose_subagent(` = 2 处）——而 `pick_closest_v2(` 里
**不含** `pick_closest(` 这个子串（末尾是 `_` 不是 `(`），复制一份换个名字就从名字那一族里溜走了。
⇒ 补上**名字之外的那一半**：把「排序/比较那几样原语」的处数钉死，并要求它们**全在**
`pick_closest` 体内 —— `sort_by_key(` 1 处 · `parse_iso8601_ms` 2 处 · `.abs()` 1 处；
筛选那一半同理（`Some(description)` 1 处，且在 `choose_subagent` 体内）。
补完再跑第 ① 刀 ⇒ 上表那一格。

⚠ `①b` 是**自己加的一刀**：件文件的 `KR94D2` 写的是两刀（长第二份 / 仍一处），
而它的正题逐字是「不许把已经共用好的那一半**连带砍了**」—— 那一形两刀都盖不到，
所以单独挨一刀。

## `KR94D3` —— 两条路结果等价

| 刀 | 怎么变的（住址） | 该怎样 | **现打** |
|---|---|---|---|
| ① **候选顺序不同就挑出别的** | `pick_closest` 里把 `candidates.sort_by_key(…)` 整段删掉，直接 `next()` 取第一条 | 红 | 🔴 **5 passed / 3 failed**（`the_order_the_backend_lists_them_in_does_not_change_the_pick` 逐字「第 2 种排列挑出了别的」：left `/r/agent-b.jsonl` right `/r/agent-a.jsonl`） |
| ② **时间戳缺失的两种形状分成两档** | `choose_subagent` 里加一句 `if matches!(v.get("timestamp"), Some(Value::Null)) { continue; }` —— `null` 静默丢掉，缺键照常进候选 | 红 | 🔴 **7 passed / 1 failed**（`both_shapes_of_a_missing_timestamp_land_in_the_same_tier` 逐字「`null` 与「缺键」被分到了两档」：left `None` right `Some("/r/agent-x.jsonl")`） |
| ③ 不变异 | —— | 绿 | 🟢 **8 passed / 0 failed** |

★ 第 ① 刀判的是**全称**命题，所以用例面是 3 个候选的 **6 种排列逐个跑**，不是挑一个样本
（`K-R83`/`local_query` 那一族「只挑几个好数」的教训）。现打红在第 2 种排列上。

★ 第 ② 刀说清了「两条路」在本件之后是什么意思：`K-R94` 把两条路收成**同一段代码**，
所以真正还能不一致的是**缺失的两种形状** —— 后端拿不到时给 `"timestamp": null`（它不猜），
而本机那条改前是「首行读不出时间戳」，在后端出口上表现为**根本没有这个键**。
两种形状必须同处置，而且**都不报错**。

## 那一格单独给的读数：「自己找」的三样还剩几样

**现打 3 → 0。** 口径 = `subagent.rs` **生产段**（剥 `#[cfg(test)] mod tests` ＋ 整行 `//`）里
的文件系统原语处数，分母 = 该段全部行。

| 量于 | 生产段行数 | `read_dir(` | `File::open(` | `BufReader` | `read_to_string(` | 合计 |
|---|---|---|---|---|---|---|
| 基点 `f1f89f5` | 216 | 1 | 2 | 3 | 1 | **7 处** |
| 本件工作树 | 167 | 0 | 0 | 0 | 0 | **0 处** |

⇒ **候选枚举**（`derive_subagent_dir` ＋ `list_meta_matches`）·**首行时间戳**（`first_line_timestamp`）·
**读 jsonl**（`read_jsonl`）三样**一样不剩**，全部改走后端既有的
`--list-subagents` ＋ `--read-session`。**没有留任何一样**，所以没有「为什么留」要交代。

⚠ 这张表**判不了**「换个名字读同一批文件」（比如自己起一个进程去 `ls`）——
与本仓其它约定型守卫同一档。**真正有牙的是上面 `KR94D1` 第 ③ 刀那条行为判据**，
这张表只是把「删干净了没有」说成一个可核的数。
