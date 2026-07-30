# STATUS — zero-poll-liveness

> **恢复入口。每轮先读本文件，再读 `MASTERPLAN.md` 与当前 feature 文件。**

## 当前阶段

**主计划 2026-07-30 用户已批准 + P4 hook 授权已给。P0/P1 已交付签收。下一个：P2。**

## 自动模式

**全自动**（用户 2026-07-30 原话「批准，开始全自动跑」）：loop 连续 B→G，
只在阻塞 / 需新决策 / 全部完成时停。功能计划（Phase B）不再逐个呈交。

## 功能进度

| # | 功能 | 状态 |
|---|---|---|
| P0 | 五项机制实测 | **✅ 完成签收**（`features/P0-machine-facts.md`）。五项里三项坏答案，定死三条设计 |
| P1 | `ZeroSessions` 观测分类（销 `INVARIANTS:408` 残留） | **✅ 完成签收**（`features/P1-zero-sessions-sentinel.md`）。三条变异双向成立；延迟「永不」→ **~16s**（有界化不是即时化） |
| P2 | pidfd 替判活轮询 + 建统一事件 channel | 未开工（**下一个**）。`libc 0.2.186` 已在 lock；**必须自带 pidfd 双向变异验收** |
| P3 | tmux server 生/死/复活（**不删 8s 轮询**） | 未开工 |
| P4 | daemon 装 tmux hook | 未开工，**需授权** |
| P5 | wire 正向死亡帧 + 免 debounce retire + **删 `TMUX_EMIT_INTERVAL`** | 未开工（承 P4） |
| P6 | 零定时器守卫 + 延迟 e2e | 未开工 |
| P7 | 文档收口 + E34 结案 | 未开工 |

## 阻塞项 / 待用户表态

| # | 事项 | 阻塞谁 | 状态 |
|---|---|---|---|
| 1 | ~~主计划审批~~ | — | **✅ 2026-07-30 用户已批准** |
| 2 | ~~授权装 hook~~ | — | **✅ 2026-07-30 用户已授权**（P0-② 实测证明 per-session 路线不存在 ⇒ 这条授权正是必需的） |

**当前无阻塞项。**

## 必须做但刻意延后的（不许忘）

| # | 事项 | 排在哪 |
|---|---|---|
| 1 | **bump `BUILD_ID`**（现 `p1q-accounts`）+ 重部署 | **P5**。不 bump ⇒ 已部署的旧 daemon 不被判 stale ⇒ 不自动重装 ⇒ **P1 的修复在远端休眠**。推到 P5 是为了让整个工作区只强制重装一次（P5 要加新帧 kind）。`release.yml` 每次发版现场交叉编译，不需本机 zigbuild |
| 2 | **给 6 套非 CI e2e 做 socket 隔离**（`graylight-*` / `restart-*` / `resume-*` + `gen-idle-tmux.sh`）—— 它们一处 `-L` 都没有，会动默认 socket | **P6**（原计划要拿 graylight 当延迟 e2e 的载体，实测行不通）。已登记 BACKLOG **E41** |

## 本轮 loop 目标

**P2 — pidfd 替掉判活轮询 A + 建统一事件 channel**（账本第 1 行的最终形态由 P2 建立：
一个阻塞在无超时 `recv()` 上的循环、单一 `mpsc<WatchEvent>`；**P3/P4 只往里加事件源，
禁止各挂一条独立线程+定时器**）。`libc 0.2.186` 已在 `src-tauri/Cargo.lock` ⇒ 可离线。
**pidfd 必须自带双向变异验收**（杀 → 事件到；不杀 → 事件不到），不许只引用用户调研。

## loop 停止条件

- P0 拿到会推翻设计的答案 → 停，交回用户重定方向
- 跑到 P4 而授权未给 → 跳过 P4/P5，继续 P6/P7，收尾如实列出
- 同一步 ≥2 次失败 → 停
- 门禁红且非在途变异 → 停
- 全部完成 → Phase G → 停

## 与其他工作区的关系

- **执行顺序表第 21 行就是本区**（E34）。原表写「（新，未分配工作区）」+「需改
  `shared/ccm` 本体」——**两条都要订正**：本区已建，且 hook 由 daemon 装 ⇒ 不碰 ccm
- `gate-integrity` 也会碰 `ci.yml` ⇒ 双方都只追加
- `local-as-remote` L1 会碰同一个 daemon ⇒ 本区先落地，L1 继承事件模型

## 时间线

- 2026-07-30 Phase A 落盘（本文件 + `MASTERPLAN.md`）
