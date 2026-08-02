# U3 · 拆 `observe/` + `control/`

- 工作区：unified-backend · 主计划 §3 第二梯队 · 任务 #91
- 风险档：**高**（动全部业务模块的位置 + 动 `readonly_guard` 的钉法）
- 性质：**纯重构，行为逐字不变**。

## Phase B 摸底：两条跨层边，一条正向一条**反向**

实测依赖矩阵（`grep -oE 'crate::[a-z_]+'` 逐文件）：

| 模块 | 依赖 |
|---|---|
| `watcher` | common · platform · **tmux_hook** · turn_detect · wire |
| `fork_write` | common · **accounts_query** |
| 其余 | 只依赖 common / platform / 彼此无跨层 |

- **正向（allowed）**：`watcher`（observe）→ `tmux_hook`（control）。**恰好一个符号、一个调用点**：
  `watcher.rs:212` 的 `crate::tmux_hook::install_hooks(&exe, me, start)`。
  这正是 §0.5-7 预言过的那条 —— hook 活在 tmux server 内存里、每次 server 重起要重装，
  而「server 起来了」只有 observe 知道。**不是设计失误，是真实的信息流方向。**
- **反向（§1.1-2 明写不许）**：`fork_write`（control）→ `accounts_query::read_regular_capped`（observe）。

### 反向边有优雅解，不需要例外

`read_regular_capped` 根本不是 observe 的域逻辑，它是**通用安全读文件**（先确认常规文件挡掉
FIFO/字符设备，再 `take(cap)` 限量读，一步消掉 metadata↔read 的 TOCTOU —— 头注记着
「审计实测 symlink→/dev/zero 6 秒涨 11GB」）。

它满足 `common/` 的三条门槛：**≥2 层用**（observe 3 个生产调用点 + control 1 个）·
**平台无关**（纯 `std::fs`）· **无域知识**。⇒ **搬进 `common/fs.rs`，反向边自然消失。**
这是铁律 6 的正例：不给例外、不加豁免，改结构让问题不存在。

## 分层归属

| 层 | 模块 | 判据 |
|---|---|---|
| `observe/` | `watcher` · `history_query` · `search_query` · `usage_query` · `accounts_query` · `turn_detect` · `codex` | 只读、不改变世界 |
| `control/` | `fork_write`（唯一写盘白名单）· `tmux_hook`（装 hook + 发 SIGUSR1）· `resolve_query`（产 CommandPlan，账本 S14 的「计划面」） | 会改变世界（写文件 / 改 tmux server 状态 / 发信号），或产出「要怎么改变世界」的计划 |
| 顶层不动 | `main`（组装根）· `wire`（协议类型，两边都用）· 四个 guard · `platform/` · `common/` | — |

> **`resolve_query` 归 control 的理由**：它名字里有 `query`，但产出的是 `CommandPlan`
> ——「怎么起这个会话」。账本 S14 明写它要「吸收进 backend 的**计划面**」。
> 按「读 / 改变世界」分，产计划属于控制的前半，不属于观测。

## DoD

| # | 项 | 验收 |
|---|---|---|
| ① | 两层建立，模块按上表归位 | `src/observe/` 7 个 · `src/control/` 3 个 |
| ② | **反向边消失** | `read_regular_capped` 进 `common/fs.rs`；机检：`control/` 下不许出现 `crate::observe` |
| ③ | **正向窄接口显式列举且钉住条数** | 一条机检：`observe/` 引用 `crate::control::` 的符号**恰好 1 个**（`tmux_hook::install_hooks`）。变异：多加一个 ⇒ 红 |
| ④ | `readonly_guard` 改钉 `observe/` + `control/` 单立窄写护栏 | 三条白名单子判据**原样迁**（`create_new` 判据 · 禁截断/追加 · 禁删/改名）；**白名单匹配从裸文件名改成路径**（U2 交接项 2） |
| ⑤ | **行为逐字不变** | daemon `cargo test` 196 不减 · wire 逐字节对拍 · 四套 daemon e2e 过地板 |
| ⑥ | U2 交接的四条清单逐条处置 | `tmux_hook` 的 `libc::kill` · `readonly_guard` 白名单路径化 · `mtime_ms` 的「≥2 层」重判 · `matches_registered` 的 `ends_with` 分支**第一次真生效** |
| ⑦ | 文档面 | daemon README 的分层图 · `ARCHITECTURE.md` · `INVARIANTS` 里 §41.6 的白名单表述 |

**不做**：不碰 Windows 编译（U4）· 不改任何判据强度 · 不动 wire 协议 · 不改 `pid_alive` 语义。

## 逐条实现步骤

1. `read_regular_capped` → `common/fs.rs`（**先做**，反向边消失之后再分层，避免中间态编不过）。
2. 建 `observe/` + `control/`，`git mv` 式搬文件，改 `main.rs` 的 mod 声明与所有 `crate::` 路径。
   *验证*：`cargo test` 196 不减。
3. 加两条**分层机检**（③ 的正向条数 + ② 的反向禁止）。
   *验证*：各自变异一次。
4. `readonly_guard` 重钉：默认层扫 `observe/`（+ 顶层），`control/` 走窄写护栏。
   *验证*：**先跑一次看它是不是真的红了**（计划预言 `whitelisted==1` 会当场红），再改。
5. U2 交接四条逐条处置 + 确认 `matches_registered` 的 `ends_with` 分支真生效。
6. wire 逐字节对拍 + 四套 e2e + 文档面 + 全量门禁。

## 测试策略

变异一律退出码判定；`cp -a` 还原后 `touch`；**新文件不能用 `git checkout` 还原**。
纯重构的特有风险是「搬丢了」⇒ 196 这个数 + wire 逐字节对拍两头卡。

## 实现期与计划的偏离

### 偏离①：**计划预言「`readonly_guard` 的 `whitelisted==1` 会当场红」——它没红，而没红本身就是缺陷**

计划（照主计划抄的）写：「`readonly_guard` 改钉 `observe/`（**会当场红 `whitelisted==1`**，逼出 control 护栏）」。

实测：`fork_write.rs` 从 `src/` 搬进 `src/control/` 之后，`readonly_guard` **一声不吭**。
原因是它按 `path.file_name()` 匹配白名单 —— 文件名没变，护栏对**整个分层重组毫无察觉**。

**「没红」在这里不是好消息。** 同样的逻辑意味着**将来任何目录下的 `fork_write.rs` 都会被当白名单
放行**，而白名单层比默认层松（它允许 `O_EXCL` 新建）⇒ 给写盘能力开一个没人知道的第二个洞。
U2 的 Phase D 审计已经点名过这条，我登记成了「U3 交接项 2」，这一刻兑现。

⇒ 改成**按仓库相对路径**匹配（`control/fork_write.rs`）。两个变异都咬：
① 另一个目录下放同名 `fork_write.rs` ⇒ 被**默认层**（更严那层）抓住；
② 常量还指旧路径（= 文件搬家护栏没跟）⇒ 真 `fork_write` 被默认层抓住。

### 偏离②：`mtime_ms` 从 `common/` 搬回 `observe/` —— **我自己定的规则第一次要求我改自己的代码**

`common/mod.rs` 的门槛第一条是「≥2 个上层用（按**层**数）」。U2 写它时 `observe/`/`control/`
还不存在，判不了，我**如实登记了「U3 必须复查」**。U3 一查：两个调用点同属 observe。

⇒ 按规则办，搬进 `observe/fs.rs`。留在 `common/` 并说「反正 control 将来可能用」，
正是那份门槛里**被逐字禁掉**的那句话。门槛只有在写规则的人也照它办时才有约束力。

### 偏离③：`libc::kill` 下沉 `platform/signal.rs`（U2 交接项 1）

`tmux_hook` 归了 `control/`，但它带着一个裸 `#[cfg(unix)] + libc::kill`，违反 §1.1-1。
下沉时**身份校验刻意留在调用方** —— 那是域判断（「这个 pid 是不是我那个 daemon」，靠 starttime 比对），
不是平台能力。平台层只负责「把信号发出去」。

顺带：写完注释后 grep 又命中了它（我在注释里写了那个 libc 函数名）⇒ 改措辞。
「本层还有没有平台原语」是靠 grep 查的，注释里留一个会让下一个人白查一趟。

### 偏离④：monitor 侧断了两处 —— **U2 审计预言过、我登记了，这一刻兑现**

U2 的 Phase D 审计写着：「monitor 侧还有 4 处硬编码 daemon 源路径，U2/U3 搬家会一起断」。
U3 搬 `watcher.rs`/`accounts_query.rs` 进子目录后：

- `tmux.rs` 两处 `include_str!("../../remote-daemon-proto/src/watcher.rs")` ⇒ **编译错**
- `local_accounts.rs:452` 运行期 `join("accounts_query.rs")` ⇒ **测试红**

两处都**响**（不是静默假绿），这是好的。但 `tmux.rs` 那两处散着 ⇒ 按审计建议收进一个单一落点。

> **收落点时踩了一个坑**：第一版写成 `const DAEMON_WATCHER_SRC: &str = …`，
> `include_str!` 报 `argument must be a string literal` —— 它只接受**字面量 token**。
> 改用 `macro_rules!`（宏能展开成字面量），单一落点与 `include_str!` 的要求两头都满足。

## 变异 / 对拍验证

| # | 变异 | 判据 | 实测 |
|---|---|---|---|
| A | 别的目录下放同名 `fork_write.rs` | 白名单路径判定 | **RC=101**，被**默认层**抓住（旧版会当白名单放行） |
| B | 白名单常量还指旧路径 | 同上 | **RC=101**，真 `fork_write` 被默认层抓住 |
| C | `control/` 引用 `crate::observe::accounts_query::run` | 分层护栏·反向 | **RC=101**：`control/ 引用了 observe（§1.1-2 反向不许）：control/fork_write.rs → crate::observe::accounts_query::run` |
| D | `observe/` 多加一条未登记的跨层引用 | 分层护栏·正向条数 | **RC=101**：`left: [resolve_query::run, tmux_hook::install_hooks] / right: [tmux_hook::install_hooks]` |
| E | `matches_registered` 退回全等 | U-1 Phase E 那个前瞻修复 | **RC=101**，出的正是那条**误导性**诊断：「登记表里的 watcher.rs …… 已经不在生产代码里了，请清理登记」——表没腐烂，文件搬了家 |
| F | **wire 逐字节对拍** | 行为逐字不变 | 与 **U2 之前**的基线仍 `diff` 无输出 |

> **E 这条值得单记**：`ends_with` 那个分支 U-1 Phase E 加进去时**从未真跑过**（登记键 `watcher.rs`
> 还在顶层，命中永远由全等短路给出），U2 的审计指出这点、我给它补了真值表。
> U3 搬 `watcher.rs` 进 `observe/` 的这一刻它**第一次真生效** —— 前瞻修复兑现。

## 门禁结果

| 项 | 值 |
|---|---|
| daemon `cargo test` | **199 passed**（U2 后 196，+3 = 分层护栏），RC=0 |
| monitor `cargo test --lib` | 663 passed / 3 ignored，RC=0 |
| daemon `cargo build` / `clippy` | **各 0 告警** |
| `cargo fmt --check` 两侧 · `tsc` · `npm test` | OK · RC=0 · 1154 例 |
| **wire 逐字节对拍** | 与 U2 前基线相同 |
| e2e（真跑 daemon 二进制） | graylight-frames 12 · restart-frames 5 · resume-frames 7 · daemon-fork 10 |
| **两层生产段的平台原语** | **0**（逐行核过 `cfg(unix)`/`cfg(target_os`/`libc::`，剩余全在测试段） |

## 代码审计结果（D）

（待填）

## 工程审计结果（E，主线程对账）

- **§1.1 第二条解耦线交付**：两层建立、方向固定、接口面**恰好一个符号**且由 `layering_guard` 钉住。
  反向零容忍且**没有开任何例外** —— 摸底时那条真实的反向边靠「把放错地方的通用工具搬进 `common/`」消解。
- **§1.1 第一条线（平台线）向前一格**：两层的生产段现在**零平台原语**；剩余 3 处全在 `main.rs`
  （组装根，可辩护）。收口判据仍是 U4 的跨 target 编译。
- **U2 交接的四条全部处置**：`libc::kill` 下沉 ✓ · 白名单路径化 ✓ · `mtime_ms` 重判并搬回 observe ✓ ·
  `matches_registered` 的 `ends_with` 分支第一次真生效并有变异证明 ✓。
- **给 U4 的清单**（现在写死）：
  1. `is_same_live_process` 上提到 `platform/liveness.rs`（Windows 判活复用同一张判定表）。
  2. `pid_alive` 的非 Linux 恒 `true` —— U2/U3 两轮都明确推迟到这里，**U4 的 DoD 里必须有它**。
  3. `platform/signal.rs::send_sigusr1` 的非 Unix 分支恒 `false`。与 `pid_alive` 不同，
     **这个方向是保守的**（发不出去就当没发，调用方本来就容忍失败）；U4 要给 Windows 决定等价物。
  4. monitor 侧跨 crate 硬路径已收成两个落点（`tmux.rs` 的宏 + `local_accounts.rs` 的运行期 join），
     U4/U13 再动 daemon 目录时改这两处。

## 签收

- [ ] 过代码审计（D）—— **待跑**：代码已提交作检查点，审计随后，findings 走后续 commit
- [x] 过工程审计（E，主线程对账）
- [x] 主计划已更新（F）
