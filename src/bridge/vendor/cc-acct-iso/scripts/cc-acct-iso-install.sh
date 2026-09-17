#!/usr/bin/env bash
# cc-acct-iso 安装器:只做软链 + 建配置目录 + 放示例配置,然后把「需要你手动做的」打印出来。
# 刻意不改你的 rc、不动任何账号/凭据、不触发登录。可逆:--uninstall。
set -euo pipefail

SKILL_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_SRC="$SKILL_DIR/scripts/cc-acct-iso"
BIN_DIR="${CC_ACCT_ISO_BIN_DIR:-$HOME/.local/bin}"
CFG_DIR="${CC_ACCT_ISO_HOME:-$HOME/.cc-acct-iso}"
LINK="$BIN_DIR/cc-acct-iso"

b=''; r=''; d=''
if [ -t 1 ]; then b=$'\033[1m'; r=$'\033[0m'; d=$'\033[2m'; fi

if [ "${1-}" = "--uninstall" ]; then
  if [ -L "$LINK" ]; then rm -f -- "$LINK"; echo "已删软链:$LINK"; else echo "(无软链 $LINK)"; fi
  echo ""
  echo "${b}剩下的请自己决定(装的时候没动、卸的时候也不替你动):${r}"
  echo "  · 配置目录 $CFG_DIR —— 想清就 rm -rf"
  echo "  · ~/.bashrc 里 cc-acct-iso 那段(shellinit 生成的 export/函数)"
  echo "  · 账号库本身(\$ACCTS_DIR) —— 要回到迁移前,请**从新到旧逐个**回退:"
  echo "      cc-acct-iso rollback latest --apply   (重复,直到最早那次)"
  echo "    跳着回退只会得到半新半旧的混合态。"
  echo "  · 备份目录 \$ACCTS_DIR/.backup-*/ 里有凭据明文副本,确认不需要了再删"
  exit 0
fi

[ -f "$BIN_SRC" ] || { echo "找不到主脚本:$BIN_SRC" >&2; exit 1; }
chmod +x "$BIN_SRC" "$SKILL_DIR/scripts/test/run-tests.sh" 2>/dev/null || true

mkdir -p -- "$BIN_DIR" "$CFG_DIR"
chmod 700 -- "$CFG_DIR"
ln -sfn -- "$BIN_SRC" "$LINK"
echo "✔ 软链:$LINK → $BIN_SRC"

if [ -f "$CFG_DIR/config" ]; then
  echo "· 已有配置,未覆盖:$CFG_DIR/config"
else
  cp -- "$SKILL_DIR/examples/config" "$CFG_DIR/config"
  echo "✔ 示例配置:$CFG_DIR/config ${d}(全是注释掉的默认值,按需改)${r}"
fi

echo ""
echo "${b}接下来需要你自己做的:${r}"
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) echo "  0) $BIN_DIR 不在 \$PATH 里 → 往 ~/.bashrc 加:export PATH=\"\$HOME/.local/bin:\$PATH\"" ;;
esac
cat <<EOF
  1) 先自测(全在沙盒里跑,不碰你的 ~/.claude):
       bash $SKILL_DIR/scripts/test/run-tests.sh
  2) 看看要做什么(dry-run,什么都不动):
       cc-acct-iso init <默认账号名>
  3) 确认后落盘 → 自检:
       cc-acct-iso init <默认账号名> --apply && cc-acct-iso verify
  4) 装 rc 片段(迁移后裸 claude 需要它才找得到凭据;工具只打印,不改你的 rc):
       cc-acct-iso shellinit        # 把输出贴进 ~/.bashrc
  5) 如果你原来有「swap .credentials.json」式的旧切号方案(如 ~/.bashrc 里的 cc-acct 块),
     迁移后请删掉它 —— 两套并存会互相打架。
  6) 加第二个账号(有旧快照就导入,没有就 add 完 run 进去 /login):
       cc-acct-iso add <名> --from-credentials <旧快照.json> --apply

后悔药:cc-acct-iso rollback latest --apply(每次 --apply 都自动备份到 \$ACCTS_DIR/.backup-<时间戳>/;
        要退回更早的状态就从新到旧逐个 rollback。备份里含凭据明文副本,不需要时请自行删除。)
卸载:  bash $SKILL_DIR/scripts/cc-acct-iso-install.sh --uninstall
EOF
