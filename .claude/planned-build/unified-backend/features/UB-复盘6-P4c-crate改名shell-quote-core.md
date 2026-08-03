# UB-复盘6 / P4c — `launch-core` → `shell-quote-core`（§1.4b 收尾）

- 工作区：unified-backend · 前置 P4a（`c86e443`）· P4b（`bb6f6d2`）
- 本件性质：**改名 + 清 P4b 留下的指针债**。零行为变化。

## 一、为什么叫 `shell-quote-core`，以及为什么不并进别的 crate

P4b 之后这个 crate 只剩 `posix_quote`，名字就成了说谎（`launch-core` 暗示它管「起会话」）。

**并进既有共享 crate 都不成立**（摸底逐个看过，不是拍脑袋）：

| 候选 | 为什么不 |
|---|---|
| `acct-core`（147 行） | 域是**账号契约 + 名字安全判据**。shell quoting 与账号无关 —— 并进去等于把「哪些字符能进命令」和「哪些字符能当账号名」混成一个域 |
| `branch-core` / `usage-core` | 域是会话分叉 / 用量口径，毫不相关 |
| `guard-core` | **dev-only**（两侧都在 `[dev-dependencies]`），而 quote 是生产代码 |

⇒ 留独立 crate，改名 **`shell-quote-core`**：与 TS `src/shell-quote.ts`、`shared/ccm::sq` 同族，
**一眼看出这三份是同一件事**（跨语言那两份由黄金串夹具对拍）。`-core` 后缀随既有四个。

## 二、改了哪些面

| 面 | 处数 |
|---|---|
| 目录名 `crates/launch-core/` → `crates/shell-quote-core/` + 它的 `Cargo.toml`（`name` + description） | 2 |
| `src-tauri/Cargo.toml` 依赖行（⚠ **blob-replay**，见下）· `remote-daemon-proto/Cargo.toml` | 2 |
| `ci.yml` **六处**（三个步骤名 + 三条命令） | 6 |
| `launch_core::posix_quote` → `shell_quote_core::posix_quote`（7 个 `.rs`，含 daemon 与守卫本体） | 7 文件 |
| `quote_singleton_guard` 的 `SOLE_HOME` 常量 + 五处文案 + `expect` 文案 | 8 |
| 其余 `.rs` 散文提及（现在时改新名；历史性提及标注「P4c 前叫」/「当时叫」） | 13 文件 |
| `Cargo.lock` ×2 | 自动 |

### ★ 顺带清掉 P4b 留下的**指针债**（10 个 TS 文件 + `doc/INVARIANTS.md` 四处）

P4b 把载荷编译器与决策内核搬进了 `backend/control/`，但**十个 TS 文件的注释还指着
`launch_core::usage_probe_payload` / `launch_core::cli::render_ccm_invocation` /
`launch_core::render_payload`** —— 那些符号在搬家后已经不在那儿了。
`doc/INVARIANTS.md` 的 §35 / §39 / U8c-1 / U8c-2c-1 四行同病。

这类「指针指向不存在的位置」正是 P2 治过的那个病（`cli.rs` 那句「下面那条测试」指向一条
不存在的测试）。⇒ 本轮一并改到真位置，并在 U8c-1 那行写明「**P4b 起内核不在共享 crate 里了**」。

## 三、`src-tauri/Cargo.toml` 的 blob-replay

这个文件带着**用户自己的 `[profile.dev]` 改动**（dev 构建瘦身，19 行），
十七轮都刻意没提交。而本轮它**必须**带一个我的改动（依赖行改名）。做法：

1. `git show HEAD:src-tauri/Cargo.toml` 拿到不含用户改动的版本；
2. 在那份上只施加我那一行改名 → 写成临时文件；
3. `git hash-object -w` 造 blob，`git update-index --cacheinfo` 让**索引**指向它；
4. 工作区照旧是「用户的 profile.dev + 我的改名」。

⇒ commit 里只有我那一行；工作区仍留着用户那段（`git status` 里它继续显示为 modified）。
两个方向都逐条核过（见下）。

## 四、锚点三条复验（改名会动 `SOLE_HOME`，必须重证）

| 复验 | 结果 |
|---|---|
| ① 行为等价改写、源码不留那个字面量 | **只有 `the_sole_home_really_holds_the_implementation` 红**（776/1） |
| ② 往别处（`launch.rs`）复制一份 quote | `posix_single_quote_escaping_has_exactly_one_home` **红**，逐字点名 `src-tauri/src/launch.rs` |
| ③ 新名 crate 的 CI clippy 步骤注释掉 | **红 2 条**，文案逐字报出「`shell-quote-core`: 缺 cargo clippy」—— 说明注册表**自动跟上了新名**（它遍历 `crates/*/Cargo.toml` 取名，不是手写清单） |

⚠ **② 第一次注入失败**：我按 `pub fn posix_quote(` 找锚点，而真实签名是 `pub(crate) fn`
⇒ `assert count==1` 当场拦下，脚本没写盘。**那个 assert 值** —— 否则「4 passed」会被读成
「变异存活」或「判据没用」，而实际是**变异根本没注入**。这与本会话连栽四次的
「没输出不是绿」是同一类：**先确认这个数在数什么。**

## 五、逐套对数（零行为变化）

monitor **777** · `shell-quote-core` **1** · daemon **237**（跨 target Windows check **0 error**）·
vitest 84 文件 **1179** · tsc 0 · fmt clean（含新名 crate）· e2e **17 套全绿** ·
clippy 去行号 **46 == 46 零新增**。

## 六、§1.4b 至此收尾

| 件 | 状态 |
|---|---|
| P4a | monitor 划出 `backend/control/` + 四条边界机检 |
| P4b | 决策内核与载荷编译器归位；crate 缩到 58 行、零依赖 |
| **P4c（本件）** | crate 名副其实；P4b 的指针债清完 |

⇒ 复盘的 **P0–P4 全部闭环**。下一轮回 U 序列主线：**U8a-2c**（daemon `launch` 的第一条
生产调用 —— 那是「控制真正搬进 daemon」的分水岭，也是三视角复盘指出的「方向偏移」要正过来的地方）。

## 签收

- [x] 过代码审计（D）—— 三条锚点逐条复验（含一次注入失败被 assert 拦下）；零行为变化逐项对数
- [x] 过工程审计（E）—— 并进既有 crate 的三个候选逐个否掉并写明理由；
      P4b 的指针债（10 个 TS 文件 + INVARIANTS 四处）一并清掉；blob-replay 两个方向都核过
- [x] 主计划已更新（F）
