#!/usr/bin/env bash
# K-R69 专用：在**同一个沙箱镜像 / 同一套挂载**里跑一条命令（迭代用）。
# 与 `.claude/devbox/gate` 逐字同一段 docker run，只把最后那条命令换成参数
# ⇒ 判别住 `DECISIONS.md#R21`（测试一律进沙箱），不在宿主跑。
set -o pipefail
PROJ=/home/zbl/文档/claudecode-frontend
SKILL=/home/zbl/.claude-accts/z/skills/planned-build
TARGETS="$PROJ/.claude/pm-targets"
WT="$PROJ/.claude/worktrees/k-r69"
exec docker run --rm --network none \
  -v "$PROJ:$PROJ" -v "$SKILL:$SKILL:ro" \
  -v "ccmon-cargo-registry:/opt/rust/cargo/registry" \
  -e "CARGO_TARGET_DIR=$TARGETS/k-r69" -e HOME=/home/zbl -e PB_WS=backend-consolidation \
  -w "$WT" ccmon-devbox:latest \
  bash -o pipefail -c "mkdir -p \"\$HOME/.claude/projects\" && $*"
