#!/usr/bin/env bash
# `P0b-Y2`〔08-13〕：daemon 盯着的 `<claude_dir>/sessions/` **被换掉 / 还没出现**时仍要宣告会话。
#
# ## 为什么这条必须是真跑，不能是单测
#
# 病在 **inotify 的 watch 绑在 inode 上**这个运行期事实上：`rm -rf sessions && mkdir sessions`
# 之后是另一个 inode，旧 watch 还挂在已删的那个上 ⇒ 新目录里发生什么都听不见，
# **而且没有任何错误**。单测调 `process_session_added` 是**绕过 watch 直接喂路径**，
# 天然看不见这一族 —— 本仓这一轮反复撞的那句话：判据钉得了「代码在哪」，钉不了「它真的работает」。
#
# ## 三组对照（缺任何一组都读不出结论）
#
# | 组 | 干什么 | 期望 |
# |---|---|---|
# | control | 不动 `sessions/` | 有 `session_added` |
# | recreate | daemon 起来后 `rm -rf` 再 `mkdir` | 有 `session_added` |
# | late | `sessions/` 起初**不存在**，daemon 起来后才建 | 有 `session_added` |
#
# ★ `control` 不只是陪跑：它是**量具自检**。它绿不了，另外两组的读数一个字都不能信
#（08-13 实测吃过一次：pidfile 文件名不是 `<PID>.json`，三组全空，差点把「修好了」读成「没修」）。
#
# ## 本机安全
#
# 全程**不碰 tmux**（本套件不建任何会话），`claude_dir` 一律在 `/tmp` 下的临时目录，
# 假会话的 pid 用一个自己起的 `sleep`（不是真 claude，`C7`）。
set -o pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
D="${CCM_E2E_DAEMON_BIN:-$REPO/remote-daemon-proto/target/debug/cc-monitor-remote}"
[ -x "$D" ] || { echo "需要 daemon 二进制：$D（先 cargo build -p cc-monitor-remote）"; exit 1; }

W="$(mktemp -d /tmp/e2e-rewatch.XXXXXX)"
cleanup() { rm -rf -- "$W"; }
trap cleanup EXIT

fail=0
pass=0
chk() { if [ "$2" = "$3" ]; then echo "  PASS $1"; pass=$((pass+1)); else echo "  FAIL $1: 期望[$3] 实得[$2]"; fail=$((fail+1)); fi; }

# $1=标签 $2=recreate|late|none
probe() {
  local label="$1" mode="$2"
  local F="$W/$label" out="$W/$label.frames"
  rm -rf -- "$F"; mkdir -p "$F/projects/p"
  [ "$mode" = late ] || mkdir -p "$F/sessions"

  sleep 60 & local vpid=$!
  local ticks; ticks=$(awk '{print $22}' "/proc/$vpid/stat")

  CLAUDE_CONFIG_DIR="$F" timeout 20 "$D" --with-bg < /dev/null > "$out" 2> "$W/$label.err" &
  local dpid=$!
  sleep 2.5
  case "$mode" in
    recreate) rm -rf -- "$F/sessions"; mkdir -p "$F/sessions"; sleep 0.5 ;;
    late)     mkdir -p "$F/sessions"; sleep 0.5 ;;
    # ★ 窗口期那一格：重建之后**立刻**写 pidfile，不给 daemon 留重挂的时间
    #   ⇒ 那次写的事件永远不会来，只有「重挂时顺带重扫」才捞得回来。
    racy)     rm -rf -- "$F/sessions"; mkdir -p "$F/sessions"
              printf '{"pid":%d,"sessionId":"sid-%s","cwd":"/tmp","kind":"interactive","procStart":"%s"}\n' \
                "$vpid" "$label" "$ticks" > "$F/sessions/$vpid.json" ;;
  esac
  printf '{"pid":%d,"sessionId":"sid-%s","cwd":"/tmp","kind":"interactive","procStart":"%s"}\n' \
    "$vpid" "$label" "$ticks" > "$F/sessions/$vpid.json"
  printf '{"type":"user","sessionId":"sid-%s"}\n' "$label" > "$F/projects/p/sid-$label.jsonl"
  sleep 4
  kill "$dpid" "$vpid" 2>/dev/null
  wait "$dpid" 2>/dev/null
  grep -c "\"kind\":\"session_added\"" "$out" 2>/dev/null || true
}

echo "[1] 量具自检：不动 sessions/ 时必须有 session_added"
chk "control 有 session_added" "$(probe control none)" "1"

echo "[2] daemon 起来后 sessions/ 被 rm -rf 再 mkdir（inode 换了）"
chk "recreate 仍有 session_added" "$(probe recreate recreate)" "1"

echo "[3] sessions/ 起初不存在，daemon 起来后才被创建"
# ★ 这一格是**生产里的洞**：`<claude_dir>/sessions/` 是用户第一次跑 claude 时才建的，
#   daemon 起得比它早，修之前就永远不宣告会话（watcher.rs 里那条 warn 逐字记着）。
chk "late 仍有 session_added" "$(probe late late)" "1"

echo "[4] 重建之后**立刻**写 pidfile（重挂前的窗口期）"
# ★ 这一格是 D 阶段变异 M23 逼出来的：把「重挂时顺带重扫」删掉，前三格**全绿** ——
#   因为它们都在重建与写文件之间留了 0.5s，事件驱动那条路够用。
#   而真实台架/真实用户不会替我们留这半秒。
chk "racy 仍有 session_added" "$(probe racy racy)" "1"

echo
echo "===== 合计 PASS=$pass FAIL=$fail ====="
if [ "$fail" -eq 0 ]; then echo "===== sessions 重挂验收全部通过 ====="; fi
exit "$fail"
