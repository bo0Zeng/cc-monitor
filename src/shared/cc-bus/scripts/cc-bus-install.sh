#!/usr/bin/env bash
# cc-bus 安装脚本 —— 把唯一的入口 `cc-bus` 软链到 PATH。收件箱与规矩都住 cc-monitor 后端,这里不建任何运行时目录。
# 【只做可逆的本地安装】:不改全局 settings.json、不 systemctl。最后打印需手动做的激活步骤。
# 可用环境变量覆盖(供测试):CCBUS_BINDIR(默 ~/.local/bin)。链接目录里别的东西一概不碰。
set -euo pipefail
SRC=$(cd "$(dirname "$(readlink -f "$0")")" && pwd)     # 脚本规范源目录
BINDIR="${CCBUS_BINDIR:-$HOME/.local/bin}"

mkdir -p "$BINDIR"

# 前置检查:cc-bus 运行时需要 jq。缺了只告警(不失败——安装本身不需要它)。
command -v jq >/dev/null 2>&1 || echo "⚠ 运行时依赖缺失(cc-bus 需要):jq"

# 入口:不静默覆盖已有的同名命令 —— 只刷新指向某个 `cc-bus/scripts/cc-bus` 的旧链接。
chmod +x "$SRC/cc-bus" "$SRC"/cc-bus-*.sh
dst="$BINDIR/cc-bus"
if [ -e "$dst" ] || [ -L "$dst" ]; then
  case "$(readlink "$dst" 2>/dev/null || true)" in
    */cc-bus/scripts/cc-bus) ;;
    *) echo "⚠ $dst 已经有别的东西,没覆盖 —— 确认要用这一版请先手动移除再装" >&2; exit 1 ;;
  esac
fi
ln -sfn "$SRC/cc-bus" "$dst"

echo "✅ 已安装:cc-bus → $BINDIR"
case ":$PATH:" in *":$BINDIR:"*) :;; *) echo "⚠ $BINDIR 不在 PATH,请加入";; esac

# 收信钩子要加的各段只有 cc-monitor 后端那一份(`hooks-diag` 每一家的 `.snippet`,指向扩展页装进 skills 的那个入口;声明了信任的那一家再带 `.trust.line`):这里照印,不自己写。
# shellcheck source=cc-bus-lib.sh
. "$SRC/cc-bus-lib.sh"
hooks=
if ccm=$(ccbus_ccm_bin); then
  hooks=$("$ccm" -- --hooks-diag --within-ms 10000 </dev/null 2>/dev/null | jq -r '(.data // .).families[]? | select(.snippet != null) | "【收信钩子】\(.name):把下面这一段合并进 \(.source)(保留现有字段):\n\(.snippet)" + (if .trust.line then "\n【钩子信任】\(.name):把下面这一段并进 \(.trust.file)(同一个键已在 ⇒ 只换它的值):\n\(.trust.line)" else "" end)' 2>/dev/null) || hooks=
fi

printf '\n—— 剩下需手动做的激活步骤(本脚本刻意不碰)——\n\n'
if [ -n "$hooks" ]; then
  echo "1) 下面各段由 cc-monitor 后端给(每家一份;合并后用 jq . <那份文件> 校验):"
  printf '%s\n' "$hooks"
else
  echo "1) 【收信钩子】这台的 cc-monitor 后端没给出要加的钩子(没找到后端,或 cc-bus 还没经 cc-monitor 装进 skills):先在 cc-monitor 扩展页装 cc-bus,再重跑本脚本。"
fi
cat <<EOF

2) 【会话】用 cc-monitor 开会话 —— 身份 = 会话 id,由 cc-monitor 认(cc-bus whoami 问 \`ccm -- --session-self\`)。
   收件箱住 cc-monitor 后端;没装 cc-monitor ⇒ cc-bus 不工作。

停用:删各家钩子文件里那条钩子;删 $BINDIR/cc-bus。脚本本体无害留存。
EOF
