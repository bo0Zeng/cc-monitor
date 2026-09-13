# K-R97 死值验留档

〔实现方 C 阶段填。**全部在沙箱里跑**（`K31`/`R15`）：
`PB_WS=backend-consolidation .claude/devbox/gate <工作树> k-r97`，一趟一刀，跑完原样还原。〕

## 0 量具与口径

- **变异台架**：`run.sh <标签> <变异脚本>` —— `cp -a` 备份整棵 `src-tauri/src` → 落变异 →
  跑**整趟门禁** → 抓 `failures:` 名单 → **用落刀前的原件覆盖回去**（`cp -a`，
  **不走** `git checkout <base> -- <file>`）→ `git status --porcelain` 自证还原干净。
  每一刀的门禁原文落在 scratchpad 的 `gate-<标签>.txt`。
- **判红判绿的口径**：整趟门禁的 `GATE:` 那一行 ＋ `failures:` 段里**被点名的判据**。
  ⚠ **只看「门禁红了」不算数** —— 要看**红的是哪一条**。下面每一刀都点名。
- **基线（`M0`）**：`GATE: OK —— 13 格全绿`。cargo **1578** · daemon **713** · npm **1709** ·
  hooks 11 · e2e 12/8/46/45 · fmt / fmt-daemon / winchk / generated 绿 · pb check FAIL=0 BROKEN=0。
  **量于分支 `track/k-r97`**（基点 `0527de6`），落刀前后各跑一趟，两趟同。
- ⚠ **每一刀还原后 `git status --porcelain` 都是同一份 8 行清单**（本件真正改过的那 8 个文件
  ＋ 本文件），逐刀核过 —— 变异一个字节都没留在盘上。

## 1 `KR97D1` 的三刀 ——「本机那条路的数据来自后端」

判的是**性质**：改后端那条的产出，本机 `list_history_projects` 的返回跟不跟。
🔴 逐字**不判**「代码里还有没有 `read_dir`」（那判的是写法）。

### 刀 ① 后端产出变了而本机不跟 ⇒ **必须红** —— 实测**红**

变异（`M1`）：在 `history::local_projects_via` 末尾把后端给的那一格**钉成常量**
（＝「不跟」的最小形态）：

```
（在 log_unknown_reasons 之前插）
for (p, _) in rows.iter_mut() { p.last_activity = 111; }
```

读数：`GATE: FAIL —— cargo（退出码 101）`；`1457 passed; 1 failed`。
点名：**`history::tests::the_local_project_list_is_whatever_the_backend_said`**
（逐字 `left: ("beta", "/w/beta", 3, 111)` / `right: (…, 222)`）。
⇒ 喂进第二份后端产出时，本机那一格没跟着变，判据当场红。**只红这一条。**

### 刀 ② 跟了 ⇒ **必须绿** —— 实测**绿**

即基线 `M0`：同一条路喂两份**不同**的后端产出，`projectName` / `projectPath` /
`sessionCount` / `lastActivity` 四格逐格跟着变（`alpha,/w/alpha,2,111` → `beta,/w/beta,3,222`），
且**后端没说的项目一个都没冒出来**（空 stdout ⇒ 空列表）。整趟 `GATE: OK`。

### 刀 ③ 本机退回自己遍历 records 根 ⇒ **必须红** —— 实测**红**（三条判据一起点名）

变异（`M2`）：把那两行原样加回 `list_history_projects`：

```
let claude_dir = paths::resolve_claude_dir().ok_or("claude dir not found")?;
let projects_dir = crate::adapter::records_dir(&claude_dir);
```

读数：`GATE: FAIL —— cargo（退出码 101）`；`1455 passed; 3 failed`。点名三条：
- **`local_read_surface_registry::tests::the_local_read_surface_matches_the_registry_line_for_line`**
  （`src/history.rs` 盘上 15 处、登记表写 13 处 —— **本机读面的递减棘轮**）；
- **`agent_dispatch_registry::tests::agent_coupling_sites_are_enumerated_one_by_one`**
  （`history.rs` 的门面那张脸 9 vs 8）；
- **`agent_dispatch_registry::tests::the_coupling_total_only_ever_goes_down`**（38 > 37）。

⚠ **三条都是该红的**：它们钉的都是「桌面侧又自己去碰数据了」这同一件事的不同侧面。
⚠ **本刀刻意不靠「代码里有没有 `read_dir`」**：上面三条一个字都没扫 `read_dir`，
它们数的是**读面命中行数**与**适配层门面处数**这两个可数的事实。

## 2 `KR97D2` 的两刀 ——「不知道」不被压平

### 刀 ① 压平 ⇒ **必须红** —— 实测**红**

变异（`M3`）：在 `local_projects_via` 的行循环里把三格压回去（`K-R92` 那一形换个位置再长一次）：

```
hp.starred_count = Some(hp.starred_count.unwrap_or(0));
hp.hidden_count  = Some(hp.hidden_count.unwrap_or(0));
hp.has_live      = Some(hp.has_live.unwrap_or(false));
```

读数：`GATE: FAIL —— cargo（退出码 101）`；`1457 passed; 1 failed`。
点名：**`history::tests::an_unknown_from_the_backend_row_is_not_flattened_on_the_local_path`**
（逐字 `left: Some(0)` / `right: None`）。**只红这一条。**

### 刀 ② 带得过去 ⇒ **必须绿** —— 实测**绿**

基线 `M0`：喂一行**没有 `sessionIds`** 的旧版后端产出 ⇒ 三格全 `None`，
且逐格 `assert_ne!` 钉住它**不是** `Some(0)` / `Some(false)`；
反面喂全字段的一行 ⇒ `starred=Some(1)` · `hidden=Some(0)` · `has_live=Some(true)`
（否则上面三条会退化成「反正都是 None」的空转）。
另有 `history::tests::the_local_liveness_oracle_answers_known_not_unknown`：
本机那个判活绑定**答得出真值**（`Counted::Known(false)`），不跟着远端一起「不知道」。

## 3 `KR97D3` 的两刀 —— spawn 次数不随项目数增长

同 `KR83D3` 的口径：判**「一次调用里问了后端几次」这个可数的事实**，不判有没有 for 循环。

### 刀 ① 逐项目再问一次 ⇒ **必须红** —— 实测**红**

变异（`M4`）：在行循环里补一句 `let _again = query(&["--list-projects"]);`

读数：`GATE: FAIL —— cargo（退出码 101）`；`1457 passed; 1 failed`。
点名：**`history::tests::one_call_asks_the_backend_exactly_once_no_matter_how_many_projects`**
（逐字 `left: 4` / `right: 1` —— 3 个项目问了 4 次）。**只红这一条。**

### 刀 ② 一次拿全 ⇒ **必须绿** —— 实测**绿**

基线 `M0`：喂 3 个项目（会话数 1/2/3），会计数的假查询被调 **1** 次，
且它自己断言问的就是 `--list-projects` 这一条子命令。

## 4 这一趟顺带逮到的一处**既有**缺陷（不是本件改坏的）

`backend/observe/local_query.rs` 那条
`every_caller_of_this_transport_is_already_off_the_read_surface_ledger`
按**字节**切 120 的窗口：`&ledger[at..(at + 120).min(ledger.len())]`。
登记表里全是中文说明 ⇒ 那个下标十有八九落在一个汉字中间。
本件动了 `local_read_surface_registry.rs` 之后偏移一变，它当场 panic：
逐字 `end byte index 9996 is not a char boundary; it is inside '栏' (bytes 9994..9997 of string)`。

⇒ 这不是「本条坏了」，是它**一直**踩在一颗只由字节偏移决定的雷上 ——
**上一处 `"src/…"` 的位置一变，雷就换个地方埋**，而它此前从没被踩到过。
订正：切之前先 `is_char_boundary` 往回退到边界（口径不变，仍是 120 字节窗口）。
