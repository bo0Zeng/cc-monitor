# U7-4 · 补 `local_accounts.rs` 的测试覆盖

- 工作区：unified-backend · 第五梯队 · 任务 #95
- 风险档：中（只加测试，不动生产代码；但**顺带订正了我在 U7-3 报错的一条安全发现**）

## Phase B：不是「没测到」，是**测不到**

U7-3 变异共享内核时 monitor 两次全绿。查清了确切原因，两条都不是「漏写了一个用例」：

### ① 沙盒用同一个常量写文件 ⇒ 结构上不可能因常量漂移而失败

```rust
fn write_manifest(&self, json: &str) {
    std::fs::write(self.0.join(MANIFEST_NAME), json)   // ← 就是生产读的那个常量
}
```

测试的**写侧**与生产的**读侧**用同一个 `const`：常量一起变，文件名一起变，测试恒绿。
`CREDENTIALS_NAME` 同样（`a.join(CREDENTIALS_NAME)`）。

⇒ 修法：**沙盒写字面量**。常量是**实现**，文件名是**契约**（bash 写侧 / daemon / 本机三方共用），
测试该钉契约。

### ② 欺骗字符只测过 `U+202E` 一个 —— 而那是两侧本来都有的

⇒ 修法：按**来源分组**取代表（13 组），任何一组从内核掉出去立刻红。

### ③ `ACCTS_DIR_NAME` 此前**完全没有测试碰过**

`local_accts_dir()` 拼 `$HOME/<ACCTS_DIR_NAME>`，改了它本机就去别处找账号库，
UI 上只表现为「一个账号都没有」。

## ★ 顺带订正：我在 U7-3 报错了一条安全发现

U7-3 我写「本机缺 `U+0085`（NEL）⇒ 安全洞」。**那是错的。**

写完覆盖测试后做验收变异 —— 删掉内核里的 NEL，**三侧全绿** —— 才发现不对。
单独跑了一遍 `char::is_control()`：

```
U+0085 NEL     is_control=true      ← 属 Cc 类
U+00A0 NBSP    is_control=false
U+2060 WJ      is_control=false
U+3000 IDEO    is_control=false
```

`is_safe_config_dir` 第一条就是 `c.is_control()` ⇒ **monitor 本来就拒了 NEL**。
集合确实差了一项，**可观察行为没差**。

根因：daemon 源码里那句注释「NEL（C1 换行，**不在 `char::is_control` 里**）」是**事实错误**，
而我**照抄了它、没验**。daemon 那半（word joiner / 各类空白，`is_control()` 全 false）**是真的**。

⇒ 已在 `acct-core` 文档、两处测试注记、`local_accounts.rs`、U7-3 的 feature 文件、
主计划变更记录**逐处订正**，并补了一条横切约定：
**从别处源码抄来的事实性断言，要么自己验一遍，要么标成「转述」。**
抄注释比抄代码更危险 —— 代码错了会红，注释错了会被当成依据继续传播。

## DoD 验收（用 U7-3 遗留的那两个变异当判据）

| 变异 | U7-3 时 | 现在 |
|---|---|---|
| 内核删 `U+2060`（word joiner，`is_control=false`） | monitor **绿** | monitor **红** ✅ |
| 内核改 `MANIFEST_NAME` | monitor **绿** | monitor **红（4 条）** ✅ |
| 内核改 `ACCTS_DIR_NAME` | 无覆盖 | monitor **红** ✅ |
| 内核改 `CREDENTIALS_NAME` | monitor 绿 | monitor **红** ✅ |
| 内核删 `U+0085`（NEL） | monitor 绿 | monitor **仍绿** —— **这是对的**，见上方订正 |
| 全部还原 | | 全绿 |

`local_accounts.rs` 测试数 5 → 7；monitor 总数 669 → 671。

## 实现期与计划的偏离

计划说「U7-4 做完，那两个变异应当让 monitor 也红」。实际**只有一个该红**：
NEL 那个不该红，因为它本来就不是洞。**把验收标准照搬会逼我写一条假测试** ——
要么去掉 `is_control()` 好让 NEL 测试有意义，要么写一个绕过 `is_safe_config_dir`
直接调 `is_deceptive_char` 的测试假装覆盖了。两条都是为了让红灯好看而改代码。

## 代码审计结果（D）

本轮自审 + 上表六条变异。

## 工程审计结果（E）

- **账本 S18 的验收补齐**：`acct-core` 的四条常量与欺骗字符集现在**三侧都有判据**。
- **给后续的移交**：`usage-core` 那侧同样值得查一遍「测试是不是用同一个常量写又读」——
  本轮这个失效模式（自洽夹具）不限于账号。

## 签收

- [x] 过代码审计（D）
- [x] 过工程审计（E）
- [x] 主计划已更新（F）
