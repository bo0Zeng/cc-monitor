#!/usr/bin/env bash
# G2（branch-anywhere）：`--fork-session` 的**跨进程**验收 —— 真跑 daemon 二进制。
#
# **为什么单测不够**：`accounts_query` 的单测当年直接调 `run()`、**绕过了 main 的 dispatch**，
# 于是 `--account-trust-zero` 在模块里实现完整、却在 dispatch 里漏了一臂 —— 随 v3.4.0
# 发出去成了真 bug（见 `main.rs` 那段注释）。本套件从 **argv 进、stdout/exit code 出**，
# 盯的正是那一层。
#
# 不需要 tmux、不需要 ssh：`--fork-session` 是一次性查询模式，指向隔离的 fixture 目录即可。
# 红线：**不碰真 ~/.claude** —— 全程用 `CLAUDE_CONFIG_DIR` 指向临时目录
#（daemon 的 `resolve_claude_dir` 优先读它，没有 `--claude-dir` 这种参数）；不改 daemon 行为（只跑它）。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PASS=0; FAIL=0
ok()   { PASS=$((PASS+1)); printf '  ok   %s\n' "$1"; }
bad()  { FAIL=$((FAIL+1)); printf '  FAIL %s\n     %s\n' "$1" "${2:-}"; }

WORK="$(mktemp -d /tmp/e2e-fork.XXXXXX)"
cleanup() { rm -rf "$WORK"; }
trap cleanup EXIT

echo "== 构建 daemon =="
( cd "$ROOT/remote-daemon-proto" && cargo build --quiet )
BIN="$ROOT/remote-daemon-proto/target/debug/cc-monitor-remote"
[ -x "$BIN" ] || { echo "daemon 二进制没构建出来: $BIN"; exit 1; }

CLAUDE_DIR="$WORK/claude"
PROJ="$CLAUDE_DIR/projects/proj-x"
mkdir -p "$PROJ"
SRC_SID="11111111-2222-3333-4444-555555555555"
{
  printf '{"type":"user","uuid":"u1","parentUuid":null,"timestamp":"t1","sessionId":"%s"}\n' "$SRC_SID"
  printf '{"type":"assistant","uuid":"u2","parentUuid":"u1","timestamp":"t2","sessionId":"%s"}\n' "$SRC_SID"
  # 被 ESC 回退的旁支，**夹在链中间** —— 祖先回溯必须跳过它
  printf '{"type":"user","uuid":"x1","parentUuid":"u1","timestamp":"t9","sessionId":"%s"}\n' "$SRC_SID"
  printf '{"type":"user","uuid":"u3","parentUuid":"u2","timestamp":"t3","sessionId":"%s"}\n' "$SRC_SID"
} > "$PROJ/$SRC_SID.jsonl"
SRC_BEFORE="$(sha256sum "$PROJ/$SRC_SID.jsonl" | cut -d' ' -f1)"

run_fork() { CLAUDE_CONFIG_DIR="$CLAUDE_DIR" "$BIN" --fork-session "$@" 2>"$WORK/err" ; }

echo "== 1. 正常分叉：exit 0 + stdout 一行 JSON =="
if OUT="$(run_fork "$SRC_SID" u3)"; then
  ok "exit 0"
else
  bad "exit 0" "exit=$? stderr=$(cat "$WORK/err")"
fi
NEW_SID="$(printf '%s' "${OUT:-}" | python3 -c 'import json,sys; print(json.load(sys.stdin)["sessionId"])' 2>/dev/null || true)"
NEW_PATH="$(printf '%s' "${OUT:-}" | python3 -c 'import json,sys; print(json.load(sys.stdin)["jsonlPath"])' 2>/dev/null || true)"
[ -n "$NEW_SID" ] && ok "stdout 是可解析的 JSON，带 sessionId" || bad "stdout JSON" "got: ${OUT:-<空>}"
[ -n "$NEW_PATH" ] && [ -f "$NEW_PATH" ] && ok "新会话文件已落盘" || bad "新文件落盘" "path=${NEW_PATH:-<空>}"

echo "== 2. 源文件一字节没动 =="
SRC_AFTER="$(sha256sum "$PROJ/$SRC_SID.jsonl" | cut -d' ' -f1)"
[ "$SRC_BEFORE" = "$SRC_AFTER" ] && ok "源文件 sha256 不变" || bad "源文件被改动" "$SRC_BEFORE -> $SRC_AFTER"

echo "== 3. 走的是祖先回溯：夹在中间的旁支 x1 不许出现 =="
if [ -n "${NEW_PATH:-}" ] && [ -f "$NEW_PATH" ]; then
  UUIDS="$(python3 -c '
import json,sys
print(",".join(json.loads(l)["uuid"] for l in open(sys.argv[1]) if l.strip()))' "$NEW_PATH")"
  [ "$UUIDS" = "u1,u2,u3" ] && ok "uuid 序列 = u1,u2,u3（旁支 x1 已跳过）" \
    || bad "祖先回溯" "got: $UUIDS（含 x1 = 退回线性切片）"
  SIDS="$(python3 -c '
import json,sys
print(",".join(sorted({json.loads(l)["sessionId"] for l in open(sys.argv[1]) if l.strip()})))' "$NEW_PATH")"
  [ "$SIDS" = "$NEW_SID" ] && ok "sessionId 已换成新 sid" || bad "sessionId" "got: $SIDS"
else
  bad "祖先回溯" "没有新文件可检"
fi

echo "== 4. 不存在的 sid：exit 2 + stderr 是结构化 JSON =="
if run_fork "99999999-0000-0000-0000-000000000000" u1 >/dev/null; then
  bad "未知 sid 应失败" "却 exit 0"
else
  rc=$?
  [ "$rc" = 2 ] && ok "exit 2" || bad "exit code" "got $rc"
  python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); assert d["code"] and d["message"]' "$WORK/err" 2>/dev/null \
    && ok "stderr 是 {code,message} JSON" || bad "stderr 信封" "$(cat "$WORK/err")"
fi

echo "== 5. 路径穿越的 sid 必须被拒 =="
if run_fork "../../../etc/passwd" u1 >/dev/null 2>&1; then
  bad "路径穿越应被拒" "却 exit 0"
else
  ok "路径穿越被拒"
fi

echo "== 6. O_EXCL：目标已存在时不覆盖 =="
# 直接验语义：把刚产出的新文件当作「已存在的目标」，再对同一源分叉一次不会动它。
if [ -n "${NEW_PATH:-}" ]; then
  KEEP="$(sha256sum "$NEW_PATH" | cut -d' ' -f1)"
  run_fork "$SRC_SID" u2 >/dev/null 2>&1 || true
  [ "$(sha256sum "$NEW_PATH" | cut -d' ' -f1)" = "$KEEP" ] \
    && ok "既有的分支文件没被后一次分叉动过" || bad "既有文件被改动"
fi

echo
echo "===== 合计 PASS=$PASS FAIL=$FAIL ====="
[ "$FAIL" -eq 0 ]
