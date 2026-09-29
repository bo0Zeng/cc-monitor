# 步 20 前置核账：条 51 / 条 66 今天成不成立（现打 2026-09-19）

分支 `w20/settings-four` · 基点 `b44e39c1`（= `wave0/delete-usage-and-fix-gate` 的 tip）。

> 排期表 `99 §4` 步 20 那一行写着「**要 11c 先答那个值的住址**」。
> 本文件先把那句话核一遍，再决定要不要往下做 —— 结论是**一半成立、一半不成立**，
> 所以本轮**没有**动条 51/66 的机制面，也**没有**照 `70 §2.3` 那张图把本机那行的控件撤掉。

---

## 一、条 66（「退出行为」那个值住哪）

### ✅ **设计面：住址已答**，那句阻塞解除

`01 §3.3b`（2026-09-18 拍板）逐字给了住址：**后端所在那台机器的 `~/.cc-monitor/backend.json`**，
只有后端写，前端改它走一条后端命令，最后一个客户断开那一刻现读，四条判据 `E1–E4` 也写好了。
`99 §4.5.8` 第 2 行也已改成 ✅ 已解。

⇒ 步 20 那一行「要 11c **先答那个住址**」——**这句话今天成立**（住址答了）。

### ❌ **机制面：11c 一个字节都没落地**（全部现打）

| 现打 | 读数 | 说明 |
|---|---|---|
| `grep -rn "backend\.json" src/ tests/` | **0 处** | 那个文件在仓里**根本不存在**（不是「写了一半」，是「还没开始」）|
| `grep -rn "lingerMs\|linger_ms" src/ tests/` | **1 处**，而且是我本轮在 `src/settings/skeleton.ts` 注释里**引用它当先例**的那一句 | 机制面 **0 处** |
| `set_backend_kill_on_exit` | **仍在**：`src/backend-policy.ts:189,235` → `src/ipc/commands.ts:904` → `src/bridge/src/backend_policy.rs:133` | `01 §3.3b ④` 说这条推送「随之退役」——**没退役** |
| 值住哪 | 仍住 **monitor 自己的 `config.json`**，键 `backendPolicy`，**按 origin 分键**（`src/backend-policy.ts` 的 `KEY` / `readPolicy` / `setKillOnExit`）| 正是 `§3.3b ①` 判为「真会犯一个错」的那个形状 |
| 连接计数 / 归零后等一下 | **不存在** | `§3.3b ⑥⑦` |

⇒ `99 §4` 里 **11c 那一行自己写的**「⚠ 仍在 20（`70` 的 UI）之前：判据 `E1–E4` 要与机制同拍进，
否则 UI 会在一个没有选择的档位上摆开关」—— **这句话今天不成立的那一半就是它**：11c 没做，
所以 `E1–E4` 一条都无从落地。

### 🔴 顺带逮到一条**设计文档里的假前提**

`01 §3.3b ⑤` 逐字：「⇒ `describeExitBehavior` 今天那**四句**要变**五句**」。
**今天不是四句，是三句**：

```
$ grep -c "^export const EXIT_" src/backend-policy.ts
3
```
`EXIT_KILLS` · `EXIT_UNATTENDED` · `EXIT_SELF_DIES`。Rust 那侧的 `EXIT_COPY`
（`backend_policy.rs:65`）也是**三条**，两侧一致。

第四句曾经存在（「已经脱离了 ⇒ 这个勾管不到它」），`backend-policy.ts::describeExitBehavior`
的头注逐字记着它「随缺口一起删掉了」。
⇒ 条 66 落地时该写的是「**三句变四句**」，不是「四句变五句」。
⚠ 这不改变条 66 的结论（第五档「读不出来」该有），**只改数**。

---

## 二、条 51（本机那行不该有 `[起][停]` 与「退出行为」）

### ❌ **前提不成立：代码里没有「折进前端」那个壳**

条 51 的后半句逐字是「**折进前端那个壳下**也没有『起/停』（monitor 就是后端）」，
而 `01 §3.3b ⑧` 给了这件事的前提：

> ⚠ 前提：后端要**答得出**「我这一趟是哪个壳」，否则界面只能猜。这一格进 `§7.2` 的能力表。

现打三条：

| # | 现打 | 住址 |
|---|---|---|
| 1 | `backend_status` 回的 JSON 里**没有「壳」这一格**（`origin`/`channel`/`pid`/`attempts`/`detached`/`killOnExit`/`health`，共 7 格）| `src/bridge/src/backend/control/backend_control.rs` 的 `backend_status` |
| 2 | 全仓搜不到任何「壳」这一轴的表示（`embedded_shell` / `in_process_backend` / `shell_kind` / `BackendShell` / 「哪个壳」）⇒ **0 处** | 现打 |
| 3 | 本机后端今天**是一个真的独立进程**：`local_backend_host::start_local_backend()` 走 `std::process::Command::new(bin)` 起它，有 pid、有 `is_detached()`、有 `stop_local_backend()` | `src/bridge/src/local_backend_host.rs`（`spawn_detached` 等） |

⇒ 今天本机那一行的 `[起]` `[停]` `☐ monitor 退出时结束它` **三个控件都是真能用的、也真有用** ——
把它们撤掉不是「去掉一个没有选择的档位上的开关」，而是**去掉一台机器上唯一能起停后端的入口**。
那正好撞上本仓那条硬纪律的反面。

### ⇒ 本轮的处置

**不做条 51 的 UI 撤除**，也**不动**任何机制面（`src/bridge/` · `src/backend/` 一个字节没改）。
`70 §2.3` 那张目标图里「本机 ● 已就绪（后端随 monitor 一起）/ 没有 [起][停]」那一格，
要等「壳」这一轴**先在后端存在、且答得出来**（`01 §3.3b ⑧` 的前提 ＋ `§7.2` 能力表那一行）。

---

## 三、一句话结论

| 问的是 | 答 |
|---|---|
| 步 20 那行「要 11c 先答那个住址」今天成不成立？ | **成立** —— 住址由条 66 于 09-18 答了（`01 §3.3b`）。|
| 那是不是就可以把条 51 那三个控件撤了？ | **不可以** —— 条 51 是条 66 ⑧ 的**推论**，而它的前提（后端答得出自己是哪个壳）今天**不成立**：那个壳、那一格读数、全仓都没有。|
| 11c 落地了吗？ | **没有**。`backend.json` 0 处、`lingerMs` 0 处、`set_backend_kill_on_exit` 仍在、值仍住 monitor 的 `config.json`。⇒ `E1–E4` 四条判据本轮**一条都没有落地条件**。|
