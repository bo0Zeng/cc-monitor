# UB-复盘4 / P4a — monitor 侧划出 `backend/` 边界（§1.4b 的第一刀）

- 工作区：unified-backend · 复盘产物 P4 的前半
- 前置：P0（`75faef8`）· P1（`f4452c8`）· P2+P3（`2e87a51`）
- 本件性质：**结构搬家 + 建边界**。零行为变化。

## 一、摸底改了计划两处（铁律 4）

### ① 「三个原语都是两侧共用」这句是错的 —— 只有一个是

逐项数 daemon 侧对 `launch_core::` 的引用：**全仓一处** ——
`remote-daemon-proto/src/control/tmux_hook.rs::sq` 里的 `posix_quote`。

| 原计划说要留下的 | 实测 daemon 用量 | monitor 用量 |
|---|---|---|
| `posix_quote` | **1 处** | 5 处 |
| `config_dir_command_safe` | **0** | 3 处 |
| `UNSET_CONFIG_DIR_PREFIX` | **0** | 1 处 |

而 daemon 的 `control/` 里**一处 config-dir 都没有**（它那边 `config_dir` 全在 `observe/`
与 `platform/` —— 那是**读**账号归属，不是**写**环境）。
⇒ 缩回去的只有 `posix_quote`。**这句是照审计的措辞写下来的，没有核过数。**

### ② 顺带把「为什么家在 monitor」的理由换硬了

原理由是「monitor 侧一个边界都没有，所以东西被推进 crate」—— 那只说明**需要一个家**，
没说明**为什么这个家在 monitor**。真正的理由是结构性的：

- §1.3 把最终 exec 钉在**用户自己的终端进程**里（pid 必须等于 pidfile 名、tty/Ctrl-C 落在 agent 上）；
- U8a-2b 把 daemon 的执行面定成 **argv 直传、不过 shell**
  （`control/launch.rs` 头注逐字：「这条路根本不过 shell」，本轮复核过）。

⇒ **「渲染一条 shell 命令串」永远属于开终端的那一侧，daemon 不会需要它。**
所以搬进 monitor 不是权宜、不是「将来还要搬回 crate」—— **是它本来的归属地。**

### ③ P4 拆成 P4a / P4b

| 件 | 内容 | 为什么这么切 |
|---|---|---|
| **P4a（本件）** | 建 `backend/` + `backend/control/`；`launch_cli_cmd.rs` → `backend/control/launch_wire.rs`；两条 parity 判据一起迁；加边界机检。**`launch-core` 一个字节不动** | 夹具路径**零改动**（14 处引用全在 `crates/launch-core/fixtures/` 下）。P4b 要的落点由本件先造出来 |
| **P4b** | `launch-core` 缩回 `posix_quote`：`cli.rs` + 载荷渲染器迁进 `backend/control/`；夹具搬家（14 处）；`quote_singleton_guard` 的 `SOLE_HOME` 改指；**crate 改名**（只剩一个 quote 函数还叫 `launch-core` 是说谎） | 夹具搬家 + 改名牵动 Cargo.toml×2 / ci.yml×3 / 全部 `launch_core::` 引用 —— 与搬家混在一轮，出错时分不清是谁的锅 |

### ④ `observe/` 本轮刻意不建

monitor 侧的读面（`local_accounts.rs` 30KB + `history_query.rs` 一族）正是 **U7 要退役的那批**
—— 现在搬进来再由 U7 删掉是纯搬运；只建一个空目录则是装饰。
⇒ 先建 `control/`。判据认 `observe/`，它一建出来自动纳入。**这条不是遗漏，是登记在案的取舍。**

## 二、做了什么

```
src-tauri/src/backend/
  mod.rs                          边界的说明 + 四条机检 + 归属登记表
  control/mod.rs                  写/控制面的说明（含「为什么渲染 shell 串是这一侧的事」）
  control/launch_wire.rs          ← src/launch_cli_cmd.rs（wire 类型 + 两个 tauri 命令）
  control/launch_cli_parity.rs    ← src/launch_cli_parity.rs
  control/launch_payload_parity.rs ← src/launch_payload_parity.rs
```

顺带改指的引用：`lib.rs` 三条 `mod` 声明 → 一条 `mod backend`、invoke_handler 两条命令路径、
两条 parity 判据的 `include_str!` 相对层级（`../` → `../../../`、`../../` → `../../../../`）、
`launch-cli-wire.vitest.ts` 里读 Rust 源码的那条硬路径、三处文档注释。

## 三、四条机检 —— 边界不能只是个文件夹

| 判据 | 钉的是什么 | 变异复验 |
|---|---|---|
| `every_file_under_backend_is_registered_with_a_reason` | 目录内容 == 登记表，**两个方向**（多的没写理由 ⇒ 红；表里写了不存在的 ⇒ 也红） | 塞一个未登记的文件 ⇒ **红** |
| `every_file_under_backend_lives_on_a_capability_line` | §1.1 那条能力线在 monitor 侧也成立：`backend/` 根下只允许 `mod.rs`，其余必须住 `control/` 或 `observe/` | 往根下塞一个 ⇒ **红** |
| `the_backend_layer_stays_host_agnostic` | **「一份代码两种宿主」今天唯一可机检的形态**：生产段里不许出现 `AppHandle`/`Window`/`State<`/`.emit(`/`Emitter`/`Manager` | 生产段引一个 `AppHandle` ⇒ **红**（逐字报出是哪个文件、哪个把手） |
| `the_backend_scan_actually_finds_files` | 抽取器自检 | 遍历器返回空 ⇒ **红 3 条** |

宿主无关那条的**诚实边界**（写在它头注里）：`#[tauri::command]` 是**登记在案的例外**
（它是 IPC 入口的标注，标注之下的函数体仍须宿主无关 —— 那才是这条查的东西）；
它是**约定型守卫**，挡得住「顺手 `app.emit` 一下」，挡不住「换个名字继续错」。

## 四、零行为变化的对数

monitor **742**（738 + 4 条新判据）· launch-core 36 · daemon 237 · vitest 84 文件 1179 ·
tsc 0 · fmt clean · e2e 17 套全绿 · clippy 去行号 46 == 46 **零新增、零消失**
（消失那栏空 = 没有哪条警告是靠改文件名躲掉的）。

⚠ **Windows 那条门禁本机跑不了**，如实记：
- 我第一次用了 `x86_64-pc-windows-gnu`，报 16 个 error —— 但**基线同样报**，
  根因是 `can't find crate for core`（本机没装那个 target 的 std）。**不是搬家造成的。**
- 本机装的是 `msvc`。monitor 用它 check 会挂在**第三方 build script 要 MSVC 工具链**
  （`ring` / `libsqlite3-sys` / 六个 tree-sitter）—— 环境限制。
  CI 里 monitor 是在 `windows-latest` 上**原生**编的，覆盖在那儿。
- daemon 的跨 target check（U4a 加的那条真判据）**0 error**，且本轮 daemon 零改动。

## 五、V3 那次「没输出」

`cargo test` 输出里一条 `test result:` 都没有，我一眼想记成绿 —— 实际是**编译失败**
（给 `#[tauri::command]` 加 `app: tauri::AppHandle` 形参需要 `Runtime` 泛型，
而我的 grep 没含 `^error`）。换成 `let _: Option<&tauri::AppHandle>` 后才真的跑起来、真的红。
**同一类第四次了**（`\|` 当正则 · `grep | head` 截断 · 两个位置过滤器 · 本次）。
共同点仍是那句：**先确认这个数在数什么，再读它。**

## 签收

- [x] 过代码审计（D）—— 四条新判据逐条变异转红；搬家零行为变化（742/36/237/1179/17 套逐项对数）
- [x] 过工程审计（E）—— 摸底订正 §1.4b 两处（只有一个原语是两侧共用；A 的理由换成结构性的那条）；
      P4 拆成 P4a/P4b 并落档；`observe/` 不建的取舍登记在案
- [x] 主计划已更新（F）
