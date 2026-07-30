# Z03 — 账号 0 接上既有能力（**(a) 用量探针已交付 · (b) 按会话切号卡红线**）

> 主计划：`../MASTERPLAN.md` §1 Z03（第 210 行）
> 前置：**Z01**（`7ad1ae3`）· **Z02 部分**（`9cfdd38`，`--base` 契约守卫 + 订正 Z01 的错记录）

## 1. 开工实测（上一轮已做，本轮直接用）

Z03 干净拆成两半，一半不碰红线：

| 半 | 面 | 判定 |
|---|---|---|
| **(a) 用量探针支持账号 0** | `remote-launch.ts::buildUsageProbePayload` · `account-usage.ts::fetchAccountUsage` 签名 · Z01 放的两处「暂不支持」占位 | **可做**，全在 TS 数据层与两个渲染点，**不碰 `tabs.ts`** |
| **(b) 按会话切号切到账号 0** | tab 右键菜单 | **卡 `tabs.ts` 红线**，且承 Z02 的三态化 |

计划原文说「今天 `remote-launch.ts:74` 明确拒绝无 configDir」—— **行号实测是 `:70`**（`:74` 是那段注释）。

## 2. (a) 做了什么

### 2.1 载荷有且只有两种形态，**没有第三种**

```
configDir 是路径  → 具名账号 → export CLAUDE_CONFIG_DIR='…'; unset <嵌套env>; claude
configDir === null → 账号 0   → unset CLAUDE_CONFIG_DIR;      unset <嵌套env>; claude
configDir === ""   → **仍然 throw**
```

**空串仍然 throw** 是刻意的：它不是账号 0，是坏数据（空值 ≠ 未设 —— Z01 起整套设计的支点）。
既有那条「`configDir` 非法（如空串）→ probe-failed」的断言**原样保留并仍然绿**。

### 2.2 ★ fail-closed：账号 0 这条路**绝不**退化成「什么前缀都不加」

裸载荷会继承远端 shell rc 里那句 `export CLAUDE_CONFIG_DIR=<默认账号>`
（`cc-acct-iso shellinit` 生成的就是它）⇒ **探针探到的是默认账号，而 UI 会把结果标成账号 0 的用量**。
这是 BACKLOG E37 那类「静默共用身份」的最坏形态：数字看着正常，指的是别人。

⇒ 有一条 `★` 断言直接钉住「载荷以 `unset CLAUDE_CONFIG_DIR; ` 打头且不含 `export CLAUDE_CONFIG_DIR`」，
设置面那条也独立钉一遍（两条不同层：纯函数 + 真实点击到 IPC 的 payload）。

### 2.3 顺手消掉一个正在长出来的双写点

`unset CLAUDE_CONFIG_DIR; ` 这个逐字节串，此前是 `launch-render-fallback.ts` 里的一个字面量。
Z03 要在探针里也用它 ⇒ **提成 `shell-quote.ts::UNSET_CONFIG_DIR_PREFIX`**，两处共用。

不提的话它会变成第三处独立字面量（CLI 那处的同语义由 `shared/ccm` 的 `--base` 承担，
已由 `base-flag-contract-guard.vitest.ts` 钉住）。**e2e 探针用
`grep -q "unset CLAUDE_CONFIG_DIR;"` 断言这个精确子串，提常量后逐字节不变。**

### 2.4 换掉 Z01 留的两处占位

`account-chip.ts` 与 `settings/accounts-section.ts` 的「账号 0 暂不支持用量查询」删掉，
`configDir` **原样传 `null`**。两处都写了注释：**别 `?? ""`** —— 空串会被 fail-closed 拒掉。

### 2.5 Rust 侧两处过时文档注释

`account_usage.rs` 的模块头与 `account_usage` 命令头都把载荷描述成
`export CLAUDE_CONFIG_DIR=...; …`。**Rust 只透传不校验**（已核：只 `shell_quote` 后执行），
所以不是 bug，但注释描述的契约变了 ⇒ 一并订正成「两种形态」。

## 3. 变异验收（Phase D）

| 变异 | 做了什么 | 结果 |
|---|---|---|
| **A** | 账号 0 的载荷退化成**裸载荷**（去掉 unset 前缀）—— 即 §2.2 那个最危险的回归 | **成立**：红 2 条（纯函数层 + 设置面真实 payload 层） |
| **B** | `if (configDir === null)` 改成 `if (!configDir)`（空串也当账号 0，支点被拆） | **成立**：红 2 条，其中**一条是既有断言**（「空串 → probe-failed」）⇒ 老套件本来就在守这条 |

两条都先 `tsc` 确认编译过再判色。逐条回盘后 860 全绿。

**如实标注**：铁律 8 要求并行多 agent 审计；本会话常驻指令「除非用户要求不开 agent」
⇒ 主线程变异 + 全门禁代替。**这是欠账，不是强度裁剪。**

## 4. (b) 按会话切号：卡红线，未做

「把此会话切到账号 X」是 tab 右键菜单（`tabs.ts`），且要先有 Z02 的三态化才能表达
「切到账号 0」这个意图。⇒ **红线松开后按 `Z02-PARTIAL.md` §6 的七步做完，(b) 接在第 7 步**。

**注意**：`account-chip.ts:259` 与 `settings/accounts-section.ts:559` 那两句
「in-place 模式：不支持按会话切号」说的是 **in-place 逃生口**，不是账号 0 —— 两者别混。

## 5. 门禁

| 门禁 | 前 | 后 |
|---|---|---|
| vitest | 855 / 57 files | **860 / 57 files**（+5；另有 1 条既有断言按契约变化改写） |
| tsc · eslint · fmt | 0 · 7 基线 · clean | 同左 |
| cargo test --all / daemon / vendored 自测 | 618 · 149 · 294 | **不变** |

## 6. 签收（部分）

- [x] (a) 用量探针支持账号 0，**fail-closed 显式 unset**（两条变异成立）
- [x] 消掉一个正在长出来的双写点（`UNSET_CONFIG_DIR_PREFIX`）
- [x] 换掉 Z01 的两处占位；订正 Rust 侧两处过时注释；订正计划里的行号
- [ ] **(b) 按会话切号切到账号 0：卡 `tabs.ts` 红线，未做**（恢复步骤见 `Z02-PARTIAL.md` §6 第 7 步）
