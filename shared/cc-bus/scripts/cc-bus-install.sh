#!/usr/bin/env bash
# cc-bus 安装脚本 —— 把脚本软链到 PATH、建运行时目录、放示例配置。
# 【只做可逆的本地安装】:不改全局 settings.json、不 systemctl。最后打印需你手动做的激活步骤。
# 可用环境变量覆盖(供测试):CCBUS_BINDIR(默 ~/.local/bin)、CC_BUS_HOME(默 ~/.cc-bus)。
set -euo pipefail
SRC=$(cd "$(dirname "$(readlink -f "$0")")" && pwd)     # 脚本规范源目录
BINDIR="${CCBUS_BINDIR:-$HOME/.local/bin}"
BUSHOME="${CC_BUS_HOME:-$HOME/.cc-bus}"
SKILLDIR=$(dirname "$SRC")                                # skills/cc-bus

mkdir -p "$BINDIR" "$BUSHOME"/{inbox,state,log,queue}

# 前置检查:cc-bus 运行时需要 jq / tmux / flock。缺了只告警(不失败——安装本身不需要它们)。
miss=""; for c in jq tmux flock; do command -v "$c" >/dev/null 2>&1 || miss="$miss $c"; done
[ -n "$miss" ] && echo "⚠ 运行时依赖缺失(cc-bus 需要):$miss —— 请先安装"

# 软链可执行脚本(cc-bus-lib.sh 不链:各脚本用 readlink -f 定位真身目录再 source 它)
# 不静默覆盖用户已有同名命令:仅刷新指向本仓库 scripts/ 的旧软链,其余跳过并告警。
skipped=""
for s in cc-whoami cc-register cc-send cc-recv cc-list cc-broadcast cc-bus-stop-hook cc-busd cc-spawn cc-kill cc-agents; do
  chmod +x "$SRC/$s"
  dst="$BINDIR/$s"
  if [ -e "$dst" ] || [ -L "$dst" ]; then
    cur=$(readlink -f "$dst" 2>/dev/null || true)
    case "$cur" in
      */scripts/"$s") ;;                    # 本仓库(live 或分发仓)旧软链 → 刷新
      *) skipped="$skipped $s"; continue;;  # 陌生的同名命令 → 不覆盖
    esac
  fi
  ln -sf "$SRC/$s" "$dst"
done
chmod +x "$SRC/cc-bus-lib.sh"
[ -n "$skipped" ] && echo "⚠ 跳过(PATH 已有同名命令,未覆盖):$skipped —— 确认要用本版本请先手动移除再重装"

# 放示例配置(不覆盖已有;默认不激活任何阀门/ACL)
[ -f "$BUSHOME/config.example" ]     || cp "$SKILLDIR/examples/config"     "$BUSHOME/config.example"
[ -f "$BUSHOME/policy.tsv.example" ] || cp "$SKILLDIR/examples/policy.tsv" "$BUSHOME/policy.tsv.example"

echo "✅ 已安装:11 个命令 → $BINDIR;运行时根 → $BUSHOME"
case ":$PATH:" in *":$BINDIR:"*) :;; *) echo "⚠ $BINDIR 不在 PATH,请加入";; esac

cat <<EOF

—— 剩下需你手动做的激活步骤(本脚本刻意不碰)——

1) 【收信钩子】把下面合并进 ~/.claude/settings.json 的顶层(保留你现有字段,只加 hooks):
   "hooks": {
     "SessionStart": [ { "hooks": [ { "type": "command",
        "command": "cc-register >/dev/null 2>&1 || true" } ] } ],
     "Stop": [ { "hooks": [ { "type": "command", "command": "cc-bus-stop-hook" } ] } ]
   }
   校验:jq . ~/.claude/settings.json

2) 【启动 CC】:直接在 tmux pane 里普通启动 \`claude\` 即可——身份自动 = tmux 会话名(如 shengwu_cc),
   无需 CC_BUS_ID。想给同一会话里多个 CC 细分,给各自 pane 打标签:  tmux set -p @cc_id <名字>
   (查自己是谁:cc-whoami;看谁在线:cc-list)

3) 【可选:broker 守护进程】(不装也能用——不在时 cc-send 自动兜底跑同一套管线)
   手动前台/后台:  cc-busd start   (停:cc-busd stop;状态:cc-busd status)
   或装成 systemd --user 服务(开机自启):
     mkdir -p ~/.config/systemd/user && cp "$SKILLDIR/examples/cc-busd.service" ~/.config/systemd/user/
     systemctl --user daemon-reload && systemctl --user enable --now cc-busd

4) 【可选:开路由策略】cp $BUSHOME/config.example $BUSHOME/config,按需开限流/ACL(见文件注释)。

—— Codex CLI 激活(cc-bus 跨工具:Claude Code 与 Codex 共用同一条 ~/.cc-bus 总线)——

C1) 【收信钩子 · Codex】同样两个钩子(SessionStart→cc-register、Stop→cc-bus-stop-hook),两种接法:
    (a) 插件形态(进阶):Codex 从 *marketplace*(一个 marketplace.json 目录清单)装插件,而非直接给仓库路径。
        本仓库已具备插件形态(.codex-plugin/plugin.json + hooks/hooks.json;钩子命令用 \$CLAUDE_PLUGIN_ROOT,Codex
        为兼容会设它),但要装它得先加一个指向 en/ 或 zh/ 插件目录的 marketplace.json 条目,再
        \`codex plugin marketplace add <那个 marketplace>\` + \`codex plugin add cc-bus\`,然后 /hooks 信任。见 Codex
        "Build plugins"。除非你本就在跑 marketplace,否则用下面的 (b)——它这些都不需要。
    (b) 非插件形态(推荐):把 $SKILLDIR/examples/codex-hooks.json 合并进 ~/.codex/hooks.json
        (或在 ~/.codex/config.toml 里用内联 [[hooks.SessionStart]] / [[hooks.Stop]] 表)。
        校验:jq . ~/.codex/hooks.json
    然后【信任钩子】(Claude 不需要、Codex 独有的一步):在 Codex 里运行 /hooks 审查并信任这两个钩子——
    Codex 对未信任的非管理命令钩子会跳过不跑——或对你已自行审核过来源的无人值守自动化,用
    --dangerously-bypass-hook-trust 启动 Codex(别无脑开 bypass)。

C2) 【启动 Codex】直接在 tmux pane 里普通启动 \`codex\` 即可——身份 = tmux 会话名,和 CC 完全一样。
    Codex 还需要:宽松的 approval_policy + 能写 ~/.cc-bus/ 的沙箱(否则注入的 cc-* 命令会卡在审批弹窗),
    且 shell env 要保留 PATH/tmux/\$TMUX。见 Codex 运行配置示例(examples/codex-config.toml)与下方 Codex 派生说明。
    注:Codex 在【第一个回合】才触发 SessionStart,而非 TUI 一打开就触发——所以手动启动、空闲着的 Codex 会在它的
    第一个回合注册(随便发个 prompt 它就注册)。cc-spawn 会自注册,所以派生出来的 Codex 会话立即加入总线。

C3) 【Codex 运行配置】没有宽松的运行配置,Codex 不会无人值守地跑 cc-bus。把 $SKILLDIR/examples/codex-config.toml
    里的键拷进 ~/.codex/config.toml(用户级)。它设:approval_policy="never"(注入的 cc-* 不卡审批)、
    sandbox_mode="workspace-write" + writable_roots=[".../.cc-bus"] 让脚本能写总线目录(或 "danger-full-access"
    直接关沙箱)、/tmp 保持可写(tmux 服务端 socket 在那)、[shell_environment_policy] inherit="all" 让
    PATH/tmux/\$TMUX 传到钩子与注入命令。把 /home/YOU 换成你的 \$HOME,改完先校验 TOML 语法再依赖它。
    ⚠ approval_policy="never" + 开放沙箱是全局大锤——收敛到项目层、或只在你信任其做无人值守 agent 的机器上用,别无脑开。

C4) 【派生一个 Codex 协作者】cc-spawn 不只能启 Claude,也能启 Codex 会话:
    \`cc-spawn --tool codex <目录> "<任务>"\`。它会在 ~/.codex/config.toml 里预先信任 <目录>、启动 codex(可用
    CCSPAWN_LAUNCH 覆盖启动器,例如加 --dangerously-bypass-hook-trust 做无人值守的钩子信任),并立即把会话自注册
    到总线上(所以它在第一个回合之前就已加入)。默认(不带 --tool)仍派生 claude,不变。

停用:从 ~/.claude/settings.json 删 hooks 段、并从 ~/.codex/hooks.json(或 config.toml)删 cc-bus 钩子;Codex 上若你只为 cc-bus 拷过 C3 运行配置键(approval_policy / sandbox_mode / [sandbox_workspace_write] / [shell_environment_policy]),也一并从 ~/.codex/config.toml 删掉;cc-busd stop;删 $BINDIR 里的软链。脚本本体无害留存。
EOF
