#!/usr/bin/env bash
# 拉前绑定的 marker **必须常驻**在窗口标题里 —— 不能被 claude 的状态标题冲掉。
#
# # 这条套件在守什么
#
# monitor 靠扫 Windows 窗口标题里的 `ccm-rbind-<sid>` 绑定终端窗口（`bind.rs`）。
# `ccm` 此前把 tmux 的 `set-titles-string` 设成 `#T`（= 窗口标题就是 pane 标题），
# 而 **claude 也在往 pane 标题写自己的状态**（转圈 + 当前在干什么）⇒ 两者抢同一个位置。
#
# 真机实测（2026-07-31）：四个空闲会话 marker 都在、唯独忙碌那个被冲成「⠐ 理解…」，
# 点 ↗ 必弹「未绑定窗口」。ccm 每 20 秒补一次、而点击时的现扫窗口是
# `40×100ms = 4s`（`bind.rs::ON_DEMAND_BIND_ATTEMPTS`）⇒ **约 1/5 命中**。
#
# 修法是让 tmux 从 `@ccm_sid` **自己合成**窗口标题，与 pane 标题彻底分开。
# 本套件钉的就是这条性质：**pane 标题被写成任意内容，窗口标题里的 marker 照样在。**
#
# 红线：**私有 socket**（`unset TMUX` + `-L`），绝不碰默认 socket 上的真实会话。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); printf '  ok   %s\n' "$1"; }
bad() { FAIL=$((FAIL+1)); printf '  FAIL %s\n     %s\n' "$1" "${2:-}"; }

# ① unset TMUX：从 tmux 里跑时 $TMUX 会让客户端连**外层那台 server** 并忽略 TMUX_TMPDIR。
# ② TMUX_TMPDIR 必须短：unix socket 路径上限 108 字节。
unset TMUX TMUX_PANE
TMUX_TMPDIR="$(mktemp -d /tmp/e2e-rbind.XXXXXX)"; export TMUX_TMPDIR
SOCK=(-L e2e-rbind)
cleanup() { tmux "${SOCK[@]}" kill-server 2>/dev/null || true; rm -rf "$TMUX_TMPDIR"; }
trap cleanup EXIT

SID="9d66c46d-bf88-4f99-877e-455555555555"

echo "== 取 ccm 里那行 set-titles-string 的真实值（不重抄一份，避免双写点） =="
FMT="$(sed -n "s/^ *tmux set-option set-titles-string '\(.*\)'.*/\1/p" "$ROOT/shared/ccm" | head -1)"
[ -n "$FMT" ] && ok "从 shared/ccm 读到 format：$FMT" \
  || { bad "读不到 set-titles-string" "ccm 里那行改形状了？"; echo "FAIL=$FAIL"; exit 1; }

tmux "${SOCK[@]}" new-session -d -s probe -x 80 -y 24
tmux "${SOCK[@]}" set-option -t probe set-titles on
tmux "${SOCK[@]}" set-option -t probe set-titles-string "$FMT"
title() { tmux "${SOCK[@]}" display-message -p -t probe "$FMT"; }

echo "== 1. @ccm_sid 还没回填时回退 pane 标题（不产出空的 ccm-rbind-） =="
tmux "${SOCK[@]}" select-pane -t probe -T "plain-shell"
[ "$(title)" = "plain-shell" ] && ok "回退到 #T" || bad "回退" "got: $(title)"
case "$(title)" in ccm-rbind-) bad "产出了空 marker";; *) ok "没有产出空 marker";; esac

echo "== 2. ★ 回填 sid 后，pane 标题被 claude 那种状态串冲掉也不影响 =="
tmux "${SOCK[@]}" set-option -t probe @ccm_sid "$SID"
for clobber in "⠐ 理解re-vendor的含义" "✳ Thinking…" "bash" ""; do
  tmux "${SOCK[@]}" select-pane -t probe -T "$clobber"
  got="$(title)"
  [ "$got" = "ccm-rbind-$SID" ] \
    && ok "pane 标题=[${clobber:-<空>}] 时窗口标题仍是 marker" \
    || bad "marker 被冲掉" "pane=[$clobber] window=[$got]"
done

echo "== 3. 反向自检：这条套件真的能红（把 format 换回 #T 就该失败） =="
tmux "${SOCK[@]}" set-option -t probe set-titles-string "#T"
tmux "${SOCK[@]}" select-pane -t probe -T "⠐ busy"
[ "$(tmux "${SOCK[@]}" display-message -p -t probe '#T')" != "ccm-rbind-$SID" ] \
  && ok "用 #T 时确实拿不到 marker（判据有牙）" \
  || bad "反向自检失效" "用 #T 竟也拿到了 marker"

echo
echo "===== 合计 PASS=$PASS FAIL=$FAIL ====="
[ "$FAIL" -eq 0 ]
