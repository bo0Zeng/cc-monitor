# UB-复盘5 / P4b — `launch-core` 缩回 `posix_quote`（§1.4b 的第二刀）

- 工作区：unified-backend · 前置 P4a（`c86e443`）
- 本件性质：**结构搬家**。零行为变化（夹具字节不变、生成器幂等）。

## 一、搬了什么

| 从 | 到 | 规模 |
|---|---|---|
| `crates/launch-core/src/cli.rs`（决策内核 + 21 条自测） | `backend/control/ccm_invocation.rs` | 858 行 |
| `crates/launch-core/src/lib.rs` 的载荷一族（`SHELL_META_COMMON` · `is_command_unsafe_char` · `config_dir_command_safe` · `Account` · `config_dir_prefix_posix` · `UNSET_CONFIG_DIR_PREFIX` · `EnvOp` · `WrapSpec` · `arg_is_join_safe` · `PayloadSpec` · `render_env_ops` · `apply_wraps` · `render_payload` · `usage_probe_payload` + 13 条测试） | `backend/control/payload.rs` | 643 行 |
| `crates/launch-core/fixtures/*.json` | `backend/control/fixtures/` | 两份，**字节不变** |

`launch-core` 从 673+858 行**缩到 58 行**：只剩 `posix_quote` 与它那一条测试，
并且**卸掉了 `acct-core` 依赖**（那个依赖是 `is_command_unsafe_char` 带来的，已随载荷一族搬走）
⇒ 现在是**零依赖** crate。

判据是「daemon 真的在用」：`posix_quote` 有一处（`control/tmux_hook.rs::sq`），其余**全零**。

## 二、夹具搬家的两条证明

1. **字节不许变**：`git show HEAD:<旧路径> | diff - <新路径>` 两份都逐字节一致。
2. **生成器幂等**：把 TS 生成器指向新路径后重跑一次，`git diff` 对夹具目录**无输出**
   —— 说明「TS 现场渲染 == 入库」这条关系没有因搬家而漂。

`include_str!` 的层级顺带收紧了：两条 parity 判据与 `ccm_invocation.rs` 的自测现在都写
`include_str!("fixtures/…")`（同目录）。`ccm_invocation.rs` 的夹具读取从**运行时
`read_to_string`** 改成 `include_str!` —— 夹具被删/改名 ⇒ **编译失败**，而不是运行到才发现。

## 三、`quote_singleton_guard` 的锚点：`SOLE_HOME` 不用改，但必须复验

`posix_quote` 留在原地 ⇒ `SOLE_HOME`（`src-tauri/crates/launch-core/src/lib.rs`）**仍然有效**。
P3 刚实测证明那条反向自检是零命中守卫的**唯一锚点**，所以搬完两条都复验：

| 复验 | 结果 |
|---|---|
| 行为等价改写、源码不留那个字面量 | **只有 `the_sole_home_really_holds_the_implementation` 红**（776 passed / 1 failed）—— 锚点性质原样保住 |
| 往新家 `payload.rs` 复制一份 quote | `posix_single_quote_escaping_has_exactly_one_home` **红**，并逐字点名 `src-tauri/src/backend/control/payload.rs` |

## 四、★ 搬家暴露出一处真重复（crate 边界藏住的）

clippy 报 `variant Base is never constructed`。查下去不是噪音：

`history.rs::config_dir_prefix_posix` 自己又写了一遍**同一个三态 match**，
其中 `Base` 那臂与内核**逐字相同**（都返回 `UNSET_CONFIG_DIR_PREFIX`）——
也就是**这个三态里的 base 那一态，生产从来没走到内核里，本文件自己截住了**。
`pub` 跨 crate 时 clippy 看不见，搬成模块内项才暴露。

⇒ 最小的**真**修法：把 `Base` 那臂改成委托内核。字节完全一致（同一个常量），
由既有的 `posix_account_prefix_is_byte_identical_after_moving_to_launch_core` 兜。
clippy 回到 46 == 46 零新增。

⚠ **剩下的重复如实登记**：`None` 那臂与整个三态 match 仍是镜像。收它要连
`validate_config_dir_posix`（POSIX 侧多拒一个 `\`）与两处不同的错误文案一起重定，
**不在 P4b 范围**。

## 五、逐套对数（零行为变化）

| 门禁 | 数 |
|---|---|
| monitor `cargo test` | **777**（P4a 时 742；+35 = 搬进来的 35 条测试） |
| `launch-core` | **1**（原 36 —— 35 条随代码搬走） |
| daemon | 237（本轮零改动；跨 target Windows check 0 error） |
| vitest | 84 文件 1179 例 |
| tsc / fmt | 0 / clean（含 launch-core） |
| e2e | 17 套全绿（`history.rs` 改动后**重跑过一遍**） |
| clippy 去行号 | 46 == 46 **零新增** |

## 六、P4c 单列的理由（改名）

crate 里只剩一个 `posix_quote` 还叫 `launch-core` 是说谎。但改名牵动
`Cargo.toml`×2 · `ci.yml` 三步 · `shared_crate_registry` 的期望 · 全部 `launch_core::` 引用
（今天 6 处生产 + 守卫 2 处）—— 与搬家混在一轮，出错时分不清是谁的锅。
⇒ **P4c**。crate 头注与 `Cargo.toml` 的 description 里都已写明「名字是过渡态」。

## 签收

- [x] 过代码审计（D）—— 夹具字节不变 + 生成器幂等两条证明；两条锚点复验；
      clippy 暴露的重复用**最小真修法**收掉（委托，字节一致）而不是 `#[allow]`
- [x] 过工程审计（E）—— `launch-core` 缩到 58 行 + 零依赖；剩余镜像重复登记在案；P4c 单列
- [x] 主计划已更新（F）
