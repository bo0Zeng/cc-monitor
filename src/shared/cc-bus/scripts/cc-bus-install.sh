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

cat <<EOF

—— 剩下需手动做的激活步骤(本脚本刻意不碰)——

1) 【收信钩子】Claude Code:合并进 ~/.claude/settings.json 的顶层(保留现有字段,只加 hooks):
   "hooks": {
     "Stop": [ { "hooks": [ { "type": "command", "command": "cc-bus stop-hook" } ] } ]
   }
   校验:jq . ~/.claude/settings.json
   Codex:同一条(Stop → cc-bus stop-hook)写进 ~/.codex/hooks.json,再在 Codex 里 /hooks 信任它。

2) 【会话】用 cc-monitor 开会话 —— 身份 = 会话 id,由 cc-monitor 认(cc-bus whoami 问 \`ccm -- --session-self\`)。
   收件箱住 cc-monitor 后端;没装 cc-monitor ⇒ cc-bus 不工作。

停用:删 settings.json 里那条钩子;删 $BINDIR/cc-bus。脚本本体无害留存。
EOF
