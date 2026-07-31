# G1 — 记录变换提成 monitor 与 daemon 共用的纯函数

分叉这件事两边都要做：本地由 monitor 直接算，远端由 daemon 在**远端本地**算
（几十 MB 的 jsonl 不该为了分叉拉过 ssh）。两边跑的必须是**同一段逻辑**。

## §0 硬约束：不能把 daemon 拖进 monitor 的 workspace

`remote-daemon-proto/Cargo.toml` 抬头逐字写着：

> Standalone crate, intentionally NOT part of a workspace. There is no root
> Cargo.toml: a workspace would pull this Linux-only daemon into the Windows
> CI `cargo test --all` and break the build.

⇒ 任何选型**先看它会不会破坏这一点**。

## §1 三条路，拿实测说话

| | 做法 | 实测/判断 |
|---|---|---|
| ① | **共享 crate + 单向 path 依赖** | **选它**。实测见 §2 |
| ② | 复制一份 + 漂移守卫 | 仓里**有三个先例**（`TMUX_LS_FMT` / `observation` 取值集 / `RemovalCause` 字面量），但那三个都是**常量**。把一个 80 行算法复制一份，守卫要么**脆**（改个变量名就假红），要么退化成**整体字节比对**（等于禁止重构）。**对算法不合适** |
| ③ | 提第三个顶层 crate | 与 ① 等价，只是位置不同；放 `src-tauri/crates/` 能沿用仓里既有的 path-dep 形状（`vendor/code-picture-core` 就是），不必新造一层顶层目录 |

## §2 ① 的两条实测

**a. daemon 单向 path 依赖会不会被 workspace 挡住** —— 不会：

```
Adding branch-core v0.1.0 (…/src-tauri/crates/branch-core)
Compiling branch-core v0.1.0
Compiling cc-monitor-remote v0.0.0
Finished `dev` profile
```

依赖是单向的，**不会反向制造 workspace 成员关系** ⇒ daemon 仍是独立 crate，
Windows CI 的 `cargo test --all` 照样碰不到它。

**b. `cargo test -p branch-core` 能不能跑到** —— 能，7 条测试。

## §3 ★ 一个差点漏掉的坑：`--all` 测不到它

拆完第一次跑 `cargo test --all`：**639**（原 646），而 branch-core 的 7 条**一条没跑**。

原因：**path 依赖不自动成为 workspace 成员**。`vendor/code-picture-core` 也是这样 ——
仓里对它的处理正是在 CI 里单开一步 `cargo test -p code-picture-core`（`ci.yml:73`
那段注释逐字写着「是 path 依赖、**非** workspace 成员 → `--all` 测不到它」）。

⇒ 照同一先例，CI 的 rust job 加一步 `cargo test -p branch-core`，
`doc/RELEASING.md` 的 checklist 同步加。

**不加会怎样**：G0 那条「落盘格式 == 官方 `/branch`」的机检**在 CI 里等于不存在** ——
本地绿、CI 从不跑，那正是本仓反复在治的「门禁静默失效」。

## §4 拆完必须重验守卫（拆分最容易出事的地方）

搬家后重跑 G0 的两条变异，确认守卫没随之失效：

| 变异 | 结果 |
|---|---|
| M1 线性切片取代祖先回溯 | **红**（`branch_matches_native_fork_shape` 等 3 条） |
| M2 remap 全部 uuid | **红**（6 条） |

## §5 边界：搬什么、不搬什么

**搬**：`build_branch_records`（纯变换）+ 它的 7 条纯测试 + 2 个夹具。

**不搬**：`read_jsonl_values`（IO）、`validate_branch_source`（路径守卫）、
`write_branch_file`、`branch_impl`、`BranchResult`（ts-rs 生成物）——
这些是 **monitor 侧特有**的；daemon 的 projects 目录语义与路径守卫不同，G2 自己写。

monitor 侧那条 IO 壳测试还需要一段「能分叉的会话」，给它留了一份**最小**本地夹具
`io_sample_session`——不为一个 IO 测试把测试夹具做成跨 crate 的公开面。

## §6 门禁

`cargo test --all` **639** + `-p branch-core` **7** + `-p code-picture-core` **25** = **671**
（G1 之前 646 + 25 = 671，**总数不变** ⇒ 确认是纯搬运）。
两侧 `cargo fmt --check` 干净、clippy 62 警告未变基线、daemon **176**、
daemon 独立 `cargo build` 通过、vitest 1048（前端未动）。

## §7 签收

- [x] 过代码审计（选型有实测；搬家后重验 M1/M2）
- [x] 过工程审计（CI 与发版 checklist 同步补上 `-p branch-core`）
- [x] 主计划已更新
