# F-E1 灰灯边界补（gray-light boundary extension）

> F-E1 核心已交付（daemon-frame 5/0 + 全链 3/0）。本轮只**补边界**，复用现成 `e2e/graylight-suite.sh` / `graylight-daemon-frames.sh` fixture。
> 边界矩阵见 MASTERPLAN「★功能边界矩阵 › F-E1 灰灯边界补」。分支 account-ux。红线：daemon 零改·不 push/bump·无 emoji。

## 目标
补灰灯三态的**边界**回归，尤其把已知残留与"只清目标 tab"钉成断言。

## DoD（边界，逐条实跑，如实标层级）
- [ ] **多会话各自独立变灰**：造 2 个 idle-tmux（cc-A、cc-B），杀 A 的 claude → 只 A 灰（`[e2e] tab-state sid=A tmuxIdle=1`），B 不受影响；再杀 B → B 也灰。daemon-frame 或全链级。
- [ ] **空 backend 最后会话卡灰（已知残留）**：只剩一个会话时杀其 claude、tmux 仍在 → 断言**当前行为**（daemon 发 NO_SESSIONS 哨兵被红线挡→last-session 卡灰）。**固化已知残留**（INVARIANTS §24bis 已记；修属 daemon，红线外，不动）。如实标注这是"已知 stuck-gray 残留"。
- [ ] **复活只清目标 tab 的灰**：A、B 都灰，只 resume/复活 A → 只 A 清灰变 live（`[e2e] tab-state sid=A ... tmuxIdle=0`），B 仍灰。防误清邻居。
- [ ] 门禁：tsc 0 + vitest 不回归；**不破** graylight-suite / f40-suite 既有断言；零 src/daemon 改动。

## Fixture（复用，别重造）
`e2e/gen-idle-tmux.sh`（造多个 `cc-<x8>` + @ccm_sid）、`fake-claude`、`daemon-wrapper.sh`、daemon 二进制。多会话=多次 gen + 多份 fake-claude pidfile。仿 `graylight-daemon-frames.sh`/`graylight-suite.sh` 加边界断言，或新增 `e2e/graylight-boundary.sh`。

## 诚实层级
灰灯是后端+daemon 驱动 → daemon-frame 级最稳；多会话/复活清灰可上全链（Xvfb+tauri dev+loopback，同 F-E1 核心那样真出 `[e2e] tab-state`）。空 backend 残留走 daemon-frame（断言哨兵被挡的现状）。逐条标到哪层。

## 步骤
1. sync worktree 到 account-ux；读本文件 + MASTERPLAN F-E1 矩阵 + INVARIANTS §24bis（空 backend 残留）。
2. 复用 fixture 造多会话；建/扩 boundary 套件。
3. 逐边界实跑，回盘核实真结果；tsc+vitest 绿、不破既有套件。

## 审计结果 / 签收
（D / E / F —— 待填）
