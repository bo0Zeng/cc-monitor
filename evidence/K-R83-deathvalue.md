# K-R83 死值验留档

〔PM 09-12 派工时建的空壳，内容由实现方 C 阶段填〕

## 量法（每一刀都同一把尺子）

- **沙箱**：`ccmon-devbox:latest`，`--network none`，`CARGO_TARGET_DIR=.claude/pm-targets/k-r83`。
  宿主上**一条测试都没跑**（`K31`）。
- **daemon 那一格**：`cd remote-daemon-proto && cargo test kr83`
- **monitor 那一格**：`cd src-tauri && cargo test -p monitor --lib kr83`
- 两个数都读 `test result:` 那一行；**每刀跑完就把文件从备份原样拷回**，下一刀从干净盘面起。
- ⚠ 下面每一行的 `passed/failed` 是**本件那几条判据**的读数，**不是**整趟门禁的合计
  （门禁合计另见件文件 `§8`）。

## 基线（无变异，量于工作树当前状态）

| 那一格 | 读数 |
|---|---|
| daemon `kr83` | **4 passed / 0 failed** |
| monitor `kr83` | **11 passed / 0 failed** |

## `KR83D1` —— daemon 那一行带得出算这三个数所需的东西

| 刀 | 怎么变的（住址） | 该怎样 | **现打** |
|---|---|---|---|
| ① 把新加的那一样从输出里摘掉 | `history_query.rs::project_row` 的 `json!` 里删掉 `"sessionIds": session_ids,` 那一项 | 红 | 🔴 daemon **1 passed / 3 failed** |
| ② 填成空清单（`[]`）而项目下确实有会话 | 同处换成 `"sessionIds": Vec::<String>::new(),` | 红 | 🔴 daemon **1 passed / 3 failed** |
| ②b 同一刀的 **monitor 半边** | `remote_history.rs::project_counts` 里把「清单长度 vs `sessionCount`」那一段整个删掉 | 红 | 🔴 monitor **10 passed / 1 failed**（`an_empty_list_on_a_project_that_has_sessions_is_a_broken_row_not_a_zero`） |
| ③ 只改字段名、内容等价 | **只动生产段两处**：daemon 的 json key `sessionIds` → `sessionUuidList`；monitor 的 `REMOTE_SESSION_IDS_FIELD` 常量同改。**测试一个字没动** | 绿 | 🟢 daemon **4 passed / 0 failed** ＋ monitor **11 passed / 0 failed** |

★ 第 ③ 刀是本条的要害，**它第一趟没绿** —— 如实记：
初版 daemon 侧有两条判据是 `row["sessionIds"]` 按 key 取的（`the_two_query_paths_spell_a_session_id_the_same_way`
与 `the_session_id_list_and_the_count_are_produced_by_the_same_guard`），改名当场红 ⇒
**那就是「判了名字」**，正是 `KR83D1` 逐字点名的失效方向。
处置：daemon 侧判据改成按**值**取（`string_leaves` 递归收全部字符串叶子，不看 key），
monitor 侧字段名收进**一个常量**、生产与夹具都从那里取。改完重跑第 ③ 刀 ⇒ 上表那一格。

## `KR83D2` —— 那三个数不许再有「不知道」与「真的是 0」同形

| 刀 | 怎么变的（住址） | 该怎样 | **现打** |
|---|---|---|---|
| ① 恢复成今天的 `0 / 0 / false` 写死 | `fanout_list_projects` 里三个字段换回字面量 `0` / `0` / `false` | 红 | 🔴 monitor **9 passed / 2 failed**（`the_wire_row_carries_the_real_star_and_hide_counts_now` ＋ `there_is_exactly_one_place_that_flattens_unknown_into_a_wire_value`） |
| ② 算得出真值时按真值走 | 不变异 | 绿 | 🟢 monitor **11 passed / 0 failed** |
| ③ **算不出的那一档**退化成「真的是 0」 | `project_counts` 里「没有清单」那一支从 `all_unknown(NoSessionIdList)` 换成 `Known(0)/Known(0)/Known(false)` | 红 | 🔴 monitor **10 passed / 1 failed**（`a_row_without_the_list_yields_unknown_and_unknown_is_not_zero`） |
| ③b 只把 `has_live` 那一格退化 | `has_live: Counted::Unknown(NoRemoteLivenessOracle)` → `Counted::Known(false)` | 红 | 🔴 monitor **10 passed / 1 failed**（`liveness_has_no_oracle_here_so_it_says_so_instead_of_saying_false`） |

⚠ **③b 是自己加的一刀**：件文件的 ③ 说的是「daemon 版本旧、字段没有」那一档，
而本件落地后 `has_live` 是**第二个**算不出的档（本机没有远端判活的真相源）。
两档各挨一刀，免得「有一档有牙、另一档没有」。

## `KR83D3` —— 不许用「每个项目再来一次 `--list-sessions`」换到它

| 刀 | 怎么变的（住址） | 该怎样 | **现打** |
|---|---|---|---|
| ① 写一版按项目循环 spawn 的实现 | `fanout_list_projects` 的逐行循环里插一句 `let _ = query(cfg.clone()).await;` | 红 | 🔴 monitor **9 passed / 2 failed**（`the_number_of_remote_execs_does_not_grow_with_the_number_of_projects` 现打 **201**，`..._equals_the_number_of_hosts` 现打 **153**） |
| ② 一次拿全 | 不变异 | 绿 | 🟢 1 台 × 3 个项目 ⇒ **1** 次；1 台 × 200 个项目 ⇒ **1** 次；3 台 × 50 个项目 ⇒ **3** 次 |

★ 判的是**「一次调用里 spawn 了几次」这个可数的事实** —— 逐字**不判**「代码里有没有 for 循环」。
落法：`fanout_list_projects` 把「去问远端」做成**入参**，判据喂一个会计数的假查询进去，
直接读那个计数。真 SSH 在红线内跑不了，而这个数与 SSH 在不在无关。
