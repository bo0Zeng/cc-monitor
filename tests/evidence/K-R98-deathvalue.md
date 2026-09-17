# K-R98 死值验留档

〔实现方 C 阶段填。**全部在沙箱里跑**（`K31`/`R15`）：
`PB_WS=backend-consolidation .claude/devbox/gate <工作树> k-r98`，一趟一刀，跑完原样还原。〕

## 0 量具与口径

- **变异台架**：`run.sh <标签> <变异脚本>` —— `cp -a` 备份整棵 `src-tauri/src` → 落变异 →
  跑**整趟门禁** → 抓 `failures:` 名单 → **用落刀前的原件覆盖回去**（`cp -a`，
  **不走** `git checkout <base> -- <file>`）→ `git status --porcelain` 自证还原干净。
  每一刀的门禁原文落在 scratchpad 的 `gate-<标签>.txt`。
- **判红判绿的口径**：整趟门禁的 `GATE:` 那一行 ＋ `failures:` 段里**被点名的判据**。
  ⚠ **只看「门禁红了」不算数** —— 要看**红的是哪一条**。下面每一刀都点名。
- **基线（`M0`）**：`GATE: OK —— 13 格全绿`。cargo **1581** · daemon **713** · npm **1709** ·
  hooks 11 · e2e 12/8/46/45 · fmt / fmt-daemon / winchk / generated 绿 · pb check FAIL=0 BROKEN=0。
  **量于分支 `track/k-r98`**（基点 `4ee7851`，主干实测 cargo 1578 ⇒ 本件 **+3**：三条新判据）。
- ⚠ **每一刀还原后 `git status --porcelain` 都是同一份 4 行清单**
  （`src-tauri/src/cc_bus.rs` · `src-tauri/src/parity_ledger.rs` ·
  `src-tauri/src/exec_site_registry.rs` ＋ 未跟踪的本文件），逐刀核过 ——
  变异一个字节都没留在盘上。

## 1 `KR98D1` 的三刀 ——「走的是 daemon 原语，不是拼出来的 shell 串」

判的是**性质**：发一条给远端时，这条路（`cc_bus_send` → `send_via_daemon`）上有没有命令串。
🔴 逐字**不判**「代码里还有没有 `exec_read`」—— 文件里当然还有（inbox / 广播 / 收掉 / spawn
四条仍旧走它，那正是 `KR98D3` 钉着的东西）。

### 刀 ① 退回拼 shell ⇒ **必须红** —— 实测**红**

变异（`M1`）：在 `send_via_daemon` 里内联一份老命令串（**照抄 08-07 那次实测的形态**：
当年在生产段内联一份 `format!("cc-send {id} {text} 2>&1")`，全仓 973 条判据一条不红）：

```
let _legacy = format!("cc-send {id} {} 2>&1", crate::ssh_source::shell_quote(text));
```

读数：`GATE: FAIL —— cargo（退出码 101）`；`1460 passed; 1 failed`。
点名：**`cc_bus::tests::the_send_path_asks_the_backend_instead_of_composing_a_shell_line`**
（逐字「`send_via_daemon` 这条路上又出现了命令串的痕迹
`["shell_quote(", "2>&1", "cc-send "]`」）。⇒ **只红这一条。**

### 刀 ② 走原语 ⇒ **必须绿** —— 实测**绿**

即基线 `M0`：`cc_bus_send` 的整个函数体就是一句
`send_via_daemon(&origin, &id, &text).await`，`send_via_daemon` 里 `.call(BUS_SEND, …)`。
整趟 `GATE: OK —— 13 格全绿`。

### 刀 ③ 🔴 本机与远端两条路的可观测行为**不等价** ⇒ **必须红** —— 实测**红**

变异（`M2`）：给本机单写一句话（**只动措辞，不动结构** —— 刻意挑一个
「禁 `LOCAL_ORIGIN` 出现在那两个函数体里」那道结构闸**够不着**的形态）：

```
pub(crate) fn describe_no_channel(origin: &str, id: &str) -> String {
    if origin == crate::inbound_client::LOCAL_ORIGIN {
        return format!("本机 daemon 没起来（{id}）");
    }
    …
```

读数：`GATE: FAIL —— cargo（退出码 101）`；`1460 passed; 1 failed`。
点名：**`cc_bus::tests::the_send_path_asks_the_backend_instead_of_composing_a_shell_line`**
（逐字 `assertion left == right failed: 本机与远端不只差一个称呼 —— 那就是同一件事的第二种说法`）。
⇒ 等价那一刀**不是靠结构闸空转过关的**：一处纯措辞的分岔照样逮得住。

## 2 `KR98D2` 的两刀 ——「老 daemon 那一档说得出话」

### 刀 ① 压成通用失败 ⇒ **必须红** —— 实测**红**

变异（`M3`）：把「太旧」那句换成分流器给超时/断连的那句（**同形**）：

```
format!("{} 发消息失败 —— ⚠ 无法确认远端是否已经执行过这条命令（{id}）。", machine_label(origin))
```

读数：`GATE: FAIL —— cargo（退出码 101）`；`1460 passed; 1 failed`。
点名：**`cc_bus::tests::an_old_daemon_on_the_send_path_is_told_apart_from_a_timeout`**
（panic 打的正是那句被压平的话：`h1 发消息失败 —— ⚠ 无法确认远端是否已经执行过这条命令（proj_cc）。`）。

### 刀 ② 分得开 ⇒ **必须绿** —— 实测**绿**

即基线 `M0`：`accepts(BUS_SEND)` 在 `.call(` **之前**问一句 ⇒
「这台的 daemon 太旧」；超时 / 断连仍旧经 `route_call_error` 出来，带着
「无法确认远端是否已经执行过这条命令」。四句话两两不同，且
「太旧」不含「无法确认」、「无法确认」那两句不含「太旧」。

⚠ 这一档**不能**靠在 `cc_bus.rs` 里 match `CallError::Unsupported` 来做 ——
`daemon_route::every_daemon_sender_is_registered_and_uses_the_one_router` 逐字禁掉
（登记为 `UsesRouter` 的文件生产段不许自己 match `CallError`，那是分流规则的第二份实现）。
⇒ 改用**发之前的能力协商**（`accepts`），分流仍然只有那一份。

## 3 `KR98D3` 的两刀 ——「另外三条仍旧逐条拒」

### 刀 ① 三条里任一条被放行 ⇒ **必须红** —— 实测**红**

变异（`M4`）：把 `cc_bus_kill` 搭上发消息那条路：

```
if !id.is_empty() {
    return send_via_daemon(&origin, &id, "cc-kill").await;
}
```

读数：`GATE: FAIL —— cargo（退出码 101）`；`1460 passed; 1 failed`。
点名：**`cc_bus::tests::letting_send_through_did_not_let_broadcast_kill_or_spawn_through`**
（逐字「`cc_bus_kill` 搭上了发消息那条路 —— 那是把三条一起放行」）。

### 刀 ② 拒绝理由被合并成一句 ⇒ **必须红** —— 实测**红**（两条判据一起点名）

变异（`M5`）：把 spawn 那句换成 kill 那句（＝「合并成一句通用话」的最小形态）：

```
if let Some(why) = refuse_local_write(&origin, "收掉 agent") {
```

读数：`GATE: FAIL —— cargo（退出码 101）`；`1459 passed; 2 failed`。点名两条：
- **`cc_bus::tests::letting_send_through_did_not_let_broadcast_kill_or_spawn_through`**
  （逐字「三条对本机说的话只剩 **2** 种 —— 它们被合并成一句通用话了：
  `["return Err(format!(\"{why}（本机没有第二条路可走）\"));", "收掉 agent", "收掉 agent"]`」）；
- **`cc_bus::tests::the_write_face_branches_on_local_before_it_asks_for_a_remote_config`**
  （既有判据，逐字「`pub async fn cc_bus_spawn(` 的本机分支没说清它在做什么」）。

🔴 **这一刀逼出过一次真修**：第一版的「三条互不相同」比的是**判据自己表里那三个常量**
（恒不同）⇒ 它是一条**不会响的闹钟**。改成**从源码里抠出**每条命令实际说的那句话之后，
`M5` 才打得中。留档在这里，因为这正是本件要防的那一形（判据看着有，其实在空转）。

## 4 还原自证

五刀跑完，`git diff --stat` 与落刀前逐字相同；`git status --porcelain` 恒为那 4 行。
