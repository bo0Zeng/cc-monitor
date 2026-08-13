# shellcheck shell=bash
# `C7i` 的**唯一**隔离原语：把 `$BIN/tmux` shim 放进 PATH 最前，它 `exec` 真 tmux 并强插 `-L <私有名>`。
#
# ## 用法
#
#   TMUX_SHIM_SOCK=e2eResume . "$(dirname "$0")/tmux-shim.sh"
#   trap 'tmux_shim_cleanup' EXIT     # 或并进你自己的 trap
#
# ## 为什么是 shim 而不是环境变量
#
# 2026-08-11 实测事故：一条探针写了 `TMUX_TMPDIR=… tmux kill-server` 却漏了 `unset TMUX`，
# `$TMUX` 有值时 tmux **按它给的 socket 走、`TMUX_TMPDIR` 完全不起作用**
# ⇒ 那条命令打到用户真实 server 上，**9 个真实 tmux 会话没了**。
#
# ⇒ `C7i` 立为红线：**tmux 命令一律带 socket 选择器（`-S <绝对路径>` 或 `-L <名>`），
# 禁止靠 `TMUX_TMPDIR`/`unset TMUX` 做隔离。**
#
# shim 的两条好处：
# · **漏什么环境变量都打不偏** —— 选择器写死在 shim 里，不依赖「记得清某个变量」；
# · **零调用点改动** —— 套件里的裸 `tmux` 一个不用改，连它 shell out 出去的东西
#   （`ccm` / `cc-spawn` 内部也裸调 tmux）也一并覆盖。
#
# `unset TMUX` **仍然保留**，但它现在只是「让被测行为发生」（tmux 内会退化成就地起），
# **不再是隔离手段** —— 隔离由 shim 独自负责。
#
# ## 为什么抽成共享文件〔`P0e` 08-12〕
#
# 这段原来在 `graylight-suite` / `graylight-daemon-frames` / `p3t-local-tmux` 里**各抄了一份**。
# 而红线的落地**不该有三份实现**：改一处漏两处，正是本仓一路在收的那一族。

: "${TMUX_SHIM_SOCK:?tmux-shim.sh 需要 TMUX_SHIM_SOCK=<私有 socket 名>}"

unset TMUX TMUX_PANE
TMUX_SHIM_REAL="$(command -v tmux)" || { echo "需要 tmux"; exit 1; }
TMUX_SHIM_BIN="$(mktemp -d /tmp/e2e-tmuxshim.XXXXXX)"
printf '#!/bin/sh\nexec %s -L %s "$@"\n' "$TMUX_SHIM_REAL" "$TMUX_SHIM_SOCK" > "$TMUX_SHIM_BIN/tmux"
chmod +x "$TMUX_SHIM_BIN/tmux"
export PATH="$TMUX_SHIM_BIN:$PATH"

# ★ 前置断言**经登录 shell 问** —— 08-12 实测教训：在外层 shell 量 `command -v tmux` 会报 PASS，
#   而命令真正跑在 `bash -lic` 里（PATH 被 profile 重排过）⇒「隔离没生效」以 PASS 的形式呈现。
#   ⚠ `E2E_SKIP_SHIM_PROBE=1` 只给**不经登录 shell** 的调用方用（它们的 PATH 不会被重排）；
#     绝不是「探针麻烦就关掉」的开关 —— 关了它，隔离没生效时你会看到一片 PASS。
if [ "${E2E_SKIP_SHIM_PROBE:-0}" != "1" ]; then
  _tmux_shim_probe="$(bash -lic 'command -v tmux' 2>/dev/null | tail -1)"
  if [ "$_tmux_shim_probe" != "$TMUX_SHIM_BIN/tmux" ]; then
    echo "  ABORT 隔离没生效：登录 shell 里的 tmux 是 '$_tmux_shim_probe'，不是 shim $TMUX_SHIM_BIN/tmux"
    echo "        绝不降级裸跑 —— 那会打到用户真实 tmux server 上（C7i 红线）。"
    rm -rf -- "$TMUX_SHIM_BIN"
    exit 2
  fi
fi

# 收尾：只收自己那台（`-L` 选择器在，**绝不裸 `kill-server`**）。
tmux_shim_cleanup() {
  set +e
  [ -n "${TMUX_SHIM_REAL:-}" ] && "$TMUX_SHIM_REAL" -L "$TMUX_SHIM_SOCK" kill-server 2>/dev/null
  [ -n "${TMUX_SHIM_BIN:-}" ] && rm -rf -- "$TMUX_SHIM_BIN"
}
