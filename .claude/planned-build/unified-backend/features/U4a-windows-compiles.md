# U4a · backend 在 Windows **编得过** + 进 CI

- 工作区：unified-backend · 主计划 §3 第三梯队 · 任务 #92
- 风险档：**高**（动 `platform/` 的形状 + 处置一个两轮都推迟的语义地雷）

## Phase B：**U4 必须拆成 a/b —— 计划与现实冲突（铁律 4）**

主计划 U4 行的 DoD 有两半，**它们的可验证性完全不同**：

| 半 | 内容 | 今天能不能验 |
|---|---|---|
| **U4a** | `cargo check --all-targets --target x86_64-pc-windows-msvc` 12 个错清零 + 进 CI | **能**。`check` 不链接，ubuntu 上就能跑，判据是编译器给的、不掺主观 |
| **U4b** | `WaitForSingleObject` 换 pidfd · 判活/procStart 双格式 | **不能**。DoD 自己写着「**等价性仓里无实测，第一步先验**」—— 而那个「验」必须在 Windows 真机上做 |

⇒ 拆。**U4a 今天做完并验收；U4b 单独立项，需要真机**（对应 STATUS 停止条件第 4 条）。

**为什么不合在一起硬做完**：把一份**无法验证**的 Win32 实现写进去、然后宣布 U4 完成，
就是「把没做的标成做完」。而且这个仓刚在 U3 栽过一次同型的
（我宣称 `layering_guard`「恰好一个符号」而它能被三种拼法绕过）——**宣称强度前先验证**。

## 这一轮要处置的语义地雷（U2/U3 两轮都推迟到这里）

| # | 地雷 | 今天的行为 | 处置 |
|---|---|---|---|
| ① | `platform::proc::pid_alive` 非 Linux | **恒返回 `true`** | 见下 |
| ② | `platform::signal::send_sigusr1` 非 Unix | 恒返回 `false` | **保留**——方向保守（发不出去当没发，调用方本来就容忍失败），且它是 U3 新写的、已在头注写明。U4b 给 Windows 决定等价物 |
| ③ | `is_same_live_process` 埋在按 `/proc` 命名的模块里 | — | 上提 `platform/liveness.rs`（U3 交接项 1） |

### ① `pid_alive` 恒 `true`：**从「静默说谎」改成「大声未实现」**

它是判活的加表门。恒 `true` 的后果是**会话永远不被归档**，而且**没有任何信号**说
「这个平台上我根本不知道」。两轮推迟的理由都是「改它 = 决定 Windows 语义，那是 U4 的正题」。

现在到 U4 了，但**真语义（`OpenProcess` + 退出码）属于 U4b**。那 U4a 该怎么办？

⇒ **`unimplemented!()`**。理由：
- 它把**静默的错误答案**换成**大声的未实现**。这是严格的改进 —— 今天那个 `true` 是一个
  没人会发现的谎；panic 是一个没人能忽略的事实。
- **不可能回归 Linux**：那条分支在 Linux 上编译期就不存在。
- Windows daemon 今天根本跑不起来（U4b 才让它能跑），所以「panic」不影响任何现存路径。
- 它给 U4b 留了一个**编译器帮你找**的落点，而不是一个「看起来能用」的假实现。

**不做**：不加 `windows` crate 依赖 · 不写任何 Win32 调用 · 不碰 `session_map.rs`（那是 monitor 侧）·
不改 Linux 侧任何行为。

## DoD

| # | 项 | 验收 |
|---|---|---|
| ① | Windows 跨 target **12 个错清零** | `cargo check --offline --all-targets --target x86_64-pc-windows-msvc` **RC=0** |
| ② | **进 CI** | daemon job 加一步；在 ubuntu 上跑 |
| ③ | Linux 侧**行为逐字不变** | daemon `cargo test` 199 不减 · wire 逐字节对拍 · 四套 daemon e2e 过地板 |
| ④ | 地雷①处置且**有机检** | 非 Linux 分支不许再返回一个凭空的 `true`；加一条源码扫描钉住「`platform/` 里的 fallback 分支不许凭空返回成功值」 |
| ⑤ | `is_same_live_process` 上提 `platform/liveness.rs` | U3 交接项 1 |
| ⑥ | 分层护栏的登记表**有 cfg 概念的措辞**（U3 交接项 2） | 头注写明：Windows 若需要 `install_hooks` 等价物，登记表会有一条在本平台不编译的项 |
| ⑦ | 文档面 | daemon README 的「3 处平台 cfg 在 platform 之外」表 · 新增 U4b 的待办说明 |

## 逐条实现步骤

1. `platform/pidwatch.rs` → `pidwatch/{mod,linux,fallback}.rs`，Linux 实现**逐字搬**进 `linux.rs`。
   *验证*：Linux 侧 `cargo test` 199 不减。
2. `fallback.rs`：`watch_pid_until_exit` 的非 Linux 形态。**不调 `on_dead`**（与「poll 真错误不报死」
   同一条纪律：宁可让会话留在 live，也不因平台未实现就误归档）+ `tracing::error!` 大声说未实现。
3. `pid_alive` / `proc_starttime` 等 `/proc` 一族的非 Linux 分支按上表处置。
4. `is_same_live_process` → `platform/liveness.rs`。
5. `watcher.rs:2182` 的 `libc::getuid`（测试段）加 cfg 门。
6. 跨 target check 清零 → 进 CI。
7. 机检④ + 文档面 + 全量门禁 + wire 对拍。

## 测试策略

变异一律退出码判定；`cp -a` 还原后 `touch`；新文件不能用 `git checkout` 还原。
**Linux 侧「行为逐字不变」靠 199 + wire 逐字节对拍两头卡**（同 U2/U3）。

## 实现期与计划的偏离

### 偏离①：**U4 拆成 a/b**（已写在 Phase B，此处登记为对主计划的修改）

主计划 U4 行把「编得过」与「`WaitForSingleObject` 换 pidfd」放在同一个 DoD 里，
而后者的验证**必须在 Windows 真机上做**——DoD 自己就写着「等价性仓里无实测，第一步先验」。
⇒ 拆。**不合在一起硬做完**的理由：把一份无法验证的 Win32 实现写进去再宣布 U4 完成，
就是「把没做的标成做完」。这个仓刚在 U3 栽过同型的（我宣称 `layering_guard`「恰好一个符号」
而它能被三种拼法绕过）。

### 偏离②：12 个错里**只有 2 个需要动结构**，其余 10 个一个 cfg 解决

拆 `pidwatch/{linux,fallback}.rs` 之后 11 → 2，剩下两个都在 `watcher.rs` 的**测试段**
（`pidfd_open` 的 test-only import + 一个清理 `/tmp/tmux-<uid>/` 的 Linux-only 夹具）。
加 `#[cfg(all(test, target_os = "linux"))]` / `#[cfg(target_os = "linux")]` 即清零。

**这说明 U2 把 11/12 个错集中到一个文件是对的判断** —— 那一步之后，U4a 的主体就只是拆文件。

### 偏离③：新增 `platform/fallback_guard.rs`（计划里没有）

DoD ④ 只说「加一条源码扫描钉住」。实做成了一个独立护栏模块，理由是它要跨整个 `platform/` 扫、
且要有自己的采集面自检与「它挡不住什么」的诚实段——塞进别的模块会看不见。

**它第一次跑就咬到我自己**：`pid_alive` 的 `unimplemented!()` 文案里我写了「此前这里恒返回 true」，
那个词被扫到了。按 §41.4 第 1 条纪律**改措辞不改护栏**。

## 变异验证

| # | 变异 | 判据 | 实测 |
|---|---|---|---|
| 1 | 把 `pid_alive` 的地雷（`let _ = pid; true`）原样放回去 | `fallback_guard` | **RC=101**：`proc.rs 的 #[cfg(not(target_os = "linux"))] 块体里出现裸 true` |
| 2 | 跨 target check 本身 | DoD ① | 改前 **RC=101 / 12 错** → 改后 **RC=0** |
| 3 | wire 逐字节对拍 | Linux 行为逐字不变 | 与 **U2 之前**的基线仍 `diff` 无输出 |

## 门禁结果

| 项 | 值 |
|---|---|
| **`cargo check --all-targets --target x86_64-pc-windows-msvc`** | **RC=0**（此前 12 错），**并已进 CI** |
| daemon `cargo test` | **200 passed**（U3 后 199，+1 = `fallback_guard`），RC=0 |
| daemon `cargo build` / `clippy` | 各 **0 告警** |
| monitor `--lib` · `tsc` · `npm test` | 663 · RC=0 · 1154 例 |
| **wire 逐字节对拍** | 与 U2 前基线相同 |
| e2e（真跑 daemon 二进制） | 12 / 5 / 7 / 10 全过地板 |

## 代码审计结果（D）

（待填）

## 工程审计结果（E，主线程对账）

- **§1.1 第一条解耦线（平台线）的编译判据收口**，且**进了 CI** —— 此前 daemon 在 Windows 上
  12 个错，而 CI 从来不知道。语义判据（Windows 上真的跑得对）= U4b。
- **账本 S3 可以标交付一半**：平台原语归位 + 跨 target 编译绿。剩下的是 U4b 的真实现。
- **U3 交接四条**：① `is_same_live_process` 上提 `platform/liveness.rs` ✓
  ② `pid_alive` 地雷 ✓（静默说谎 → 大声未实现 + 机检钉住这一族）
  ③ `send_sigusr1` 非 Unix 恒 `false` —— **保留**，方向保守且已写明，U4b 定等价物
  ④ 登记表的 cfg 措辞 —— **U4a 用不上**（Windows 还没有 `install_hooks` 等价物），留 U4b
- **给 U4b 的清单**（现在写死）：
  1. `pidwatch/fallback.rs` 的空壳换成 `OpenProcess` + `WaitForSingleObject` —— 要加
     `[target.'cfg(windows)'.dependencies] windows = "0.56"`（本地 cargo 缓存有，monitor 侧在用）。
  2. `proc::pid_alive` 的 `unimplemented!()` 换成 `OpenProcess` + 退出码。
  3. **`WaitForSingleObject ≡ poll(pidfd)` 的等价性必须在真机上验**（主计划开放-1）。
  4. `send_sigusr1` 的 Windows 等价物（daemon 那条 SIGUSR1 通路在 Windows 上是什么）。
  5. `layering_guard` 的登记表届时会有一条**在 Linux 上不编译**的项，措辞要跟。

## 签收

- [ ] 过代码审计（D）—— **待跑**：代码已提交作检查点，审计随后
- [x] 过工程审计（E，主线程对账）
- [x] 主计划已更新（F）
