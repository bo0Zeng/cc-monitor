# shellcheck shell=bash
# e2e 私有名字的**唯一来源** ＋ `C7i` 的**唯一**隔离原语（把 `$BIN/tmux` shim 放进 PATH 最前，
# 它 `exec` 真 tmux 并强插 `-L <私有名>`）。
#
# ## 用法
#
#   # shellcheck source=tests/e2e/tmux-shim.sh
#   . "$(cd "$(dirname "$0")" && pwd)/tmux-shim.sh" e2eResume   # 装 shim；私有 socket 名在 $TMUX_SHIM_SOCK
#   trap 'tmux_shim_cleanup' EXIT     # 或并进你自己的 trap
#
#   . "$HERE/../tmux-shim.sh" --names-only                      # 只要名字（docker 台架、自带 shim 的套件）
#   NET="$(e2e_run_name ccmon-weaknet-net)"
#
# ## 名字：一趟一个
#
# `e2e_run_name <前缀>` = `<前缀>-<工作树路径短哈希>-<本趟 pid>`。套件只给前缀，名字本身由这里拼：
# 两棵工作树同时跑门禁、同一棵树里同时起两趟，私有 tmux socket / docker 网络 / 容器名都不撞。
# 写死的名字下，一边收尾的 `kill-server` / `docker rm` 会打掉另一边正在跑的那趟（PASS 数随并发漂）。
# 收尾只按自己的名字收；`tests/e2e/` 下写死的私有名字由 `e2e_gate_registry` 的扫描判据拦。
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

_e2e_tree_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)" || exit 2
_E2E_TREE_TAG="$(printf '%s' "$_e2e_tree_root" | sha256sum | cut -c1-8)"
case "$_E2E_TREE_TAG" in
  [0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]) ;;
  *) echo "tmux-shim.sh：算不出工作树短哈希（sha256sum 不在？）—— 拼不出本趟的私有名字，不跑" >&2; exit 2 ;;
esac

# 同一棵树各趟共用的那一段：`<前缀>-<树哈希>-`。前缀只许字母、数字、连字符（tmux 与 docker 都收）。
e2e_run_family() {
  case "${1:-}" in
    ''|-*|*[!A-Za-z0-9-]*) echo "e2e_run_name：前缀只许字母、数字、连字符，实得 '${1:-}'" >&2; return 2 ;;
  esac
  printf '%s-%s-' "$1" "$_E2E_TREE_TAG"
}

# 本趟的私有名字（`$$` 在 `$(…)` 里仍是调用方那条 shell 的 pid）。
e2e_run_name() {
  local fam
  fam="$(e2e_run_family "${1:-}")" || return 2
  printf '%s%s' "$fam" "$$"
}

# 从 stdin 读一列名字，印出**本棵树**、给定前缀下、主人那条进程已经不在的那些（崩掉的上一趟留下的）。
# 活着的那趟（同树并发）与别棵树的一概不印 —— 给台架的 `--clean` 用。
e2e_dead_runs() {
  local name pre fam pid
  while IFS= read -r name; do
    for pre in "$@"; do
      fam="$(e2e_run_family "$pre")" || return 2
      case "$name" in "$fam"*) ;; *) continue ;; esac
      pid="${name#"$fam"}"
      case "$pid" in ''|*[!0-9]*) continue ;; esac
      [ -e "/proc/$pid" ] || printf '%s\n' "$name"   # 不用 kill -0：别的用户的进程回 EPERM，会被误认成不在
    done
  done
}

# 宿主快照（docker 台架跑前跑后逐字比的那两份）：`docker network ls` ＋ `ip link`，
# 剔掉同族**别的趟**（别棵树、同树并发）建的网络、它们的桥 `br-<id>` 与挂在那座桥上的网卡。
# 本趟自己的网络不剔：它跑前不该在、跑后不该留，留了就是 diff。
# 网络 id 在快照前后各取一遍：快照那一刻还在的别趟网络，至少有一遍取得到。
# $1 = 网络名前缀  $2 = 本趟的网络名  $3 = 落盘前缀（写 $3.net 与 $3.link）
e2e_docker_host_snapshot() {
  local pre="$1" own="$2" out="$3" pat ids
  pat="^${pre}-[0-9a-f]+-[0-9]+\$"   # 不写 {8}：老 mawk 不认区间
  ids="$(_e2e_foreign_net_ids "$pat" "$own")"
  # 固定列格式：默认表格按最长的名字补空格，别趟的长名字一进一出就会把整张表的列宽改掉。
  docker network ls --format '{{.ID}} {{.Name}} {{.Driver}} {{.Scope}}' > "$out.net.raw" 2>&1
  ip link > "$out.link.raw" 2>&1
  ids="$ids $(_e2e_foreign_net_ids "$pat" "$own")"
  awk -v pat="$pat" -v own="$own" '!($2 ~ pat && $2 != own)' "$out.net.raw" > "$out.net"
  awk -v ids="$ids" '
    BEGIN { n = split(ids, a, " "); for (i = 1; i <= n; i++) if (a[i] != "") br["br-" a[i]] = 1 }
    /^[0-9]+: / {
      drop = 0; nm = $2; sub(/:$/, "", nm); sub(/@.*/, "", nm)
      if (nm in br) drop = 1
      for (i = 3; i < NF; i++) if ($i == "master" && ($(i + 1) in br)) drop = 1
    }
    !drop' "$out.link.raw" > "$out.link"
  rm -f -- "$out.net.raw" "$out.link.raw"
}
_e2e_foreign_net_ids() { # $1 = 同族名字的正则  $2 = 本趟的网络名；印别趟网络 id 的前 12 位（= 桥名后缀）
  docker network ls --no-trunc --format '{{.ID}} {{.Name}}' 2>/dev/null \
    | awk -v pat="$1" -v own="$2" '$2 ~ pat && $2 != own { print substr($1, 1, 12) }'
}

case "${1:-}" in
  --names-only) return 0 ;;
  ''|-*|*[!A-Za-z0-9-]*)
    echo "tmux-shim.sh 要一个私有名前缀：. tmux-shim.sh <前缀>（只要名字：--names-only），实得 '${1:-}'" >&2
    exit 2 ;;
esac
TMUX_SHIM_SOCK="$(e2e_run_name "$1")" || exit 2

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
