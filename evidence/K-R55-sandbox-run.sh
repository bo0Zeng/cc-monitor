#!/usr/bin/env bash
# K-R55 实现方自用量具（住址唯一：本文件）。被测对象 = /home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r55
# 用法: kr55-box.sh <要在容器里跑的 bash 命令串>
set -o pipefail
PROJ=/home/zbl/文档/claudecode-frontend
SKILL=/home/zbl/.claude-accts/z/skills/planned-build
TARGETS="$PROJ/.claude/pm-targets"
WT="$PROJ/.claude/worktrees/k-r55"
exec docker run --rm \
  --network "${DEVBOX_NET:-none}" \
  -v "$PROJ:$PROJ" \
  -v "$SKILL:$SKILL:ro" \
  -v "ccmon-cargo-registry:/opt/rust/cargo/registry" \
  -e "CARGO_TARGET_DIR=$TARGETS/k-r55" \
  -e HOME=/home/zbl \
  -w "$WT" \
  ccmon-devbox:latest \
  bash -o pipefail -c 'mkdir -p "$HOME/.claude/projects"; '"$1"
