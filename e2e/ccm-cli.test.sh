#!/bin/bash
# ccm CLI 的 shell 级测试（unify-launch F02）。
#
# 全部走 `--print` 断言命令串——不真起 agent、不碰 tmux（tmux 行为由
# e2e/tmux-target-acceptance.sh 那套真机 harness 管）。
#
# 跑法：bash e2e/ccm-cli.test.sh   （npm run test:ccm-cli）
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
CCM="$REPO/shared/ccm"

PASS=0; FAIL=0
ck() { # ck <描述> <期望> <实得>
  if [ "$2" = "$3" ]; then printf 'PASS | %s\n' "$1"; PASS=$((PASS+1))
  else printf 'FAIL | %s\n      期望: %s\n      实得: %s\n' "$1" "$2" "$3"; FAIL=$((FAIL+1)); fi
}
# **必须同时隔离 CCM_ACCTS_MANIFEST**：不隔离的话本机 manifest 的 isDefault 账号会被注入
# 每条黄金串（"零修饰 = 今天的 ccm()" 是**基座**语义，不带账号）。默认号注入另有专测。
ccm() { CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST=/nonexistent bash "$CCM" "$@" 2>&1; }

UNSET="unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID CLAUDE_CODE_CHILD_SESSION"

echo "===== 契约：动作 × 修饰 ====="
ck "零修饰（--cwd .）：最终 exec 与今天 ccm() 逐字节一致" \
   "$UNSET; cd '.' && exec claude" \
   "$(ccm --cwd . --print)"
ck "resume <sid>" \
   "$UNSET; cd '/p' && exec claude --resume abc-123" \
   "$(ccm resume abc-123 --cwd /p --print)"
ck "--resume <sid> 与 resume <sid> **等价**（cc-monitor 今天的拼法，零改动即正确）" \
   "$(ccm resume abc-123 --cwd /p --print)" \
   "$(ccm --resume abc-123 --cwd /p --print)"
ck "--resume=<sid> 等号形式" \
   "$(ccm resume abc-123 --cwd /p --print)" \
   "$(ccm --resume=abc-123 --cwd /p --print)"
ck "--agent codex：换启动器 + 无嵌套 env" \
   "cd '/p' && exec codex" \
   "$(ccm --agent codex --cwd /p --print)"
ck "--agent codex 不支持 resume → 报错" \
   "ccm: agent=codex 不支持 resume（无 resume flag）" \
   "$(ccm resume x --agent codex --cwd /p --print)"
ck "--launcher 覆盖默认启动器" \
   "$UNSET; cd '/p' && exec mycc --resume s1" \
   "$(ccm resume s1 --cwd /p --launcher mycc --print)"
ck "--base：显式 unset CLAUDE_CONFIG_DIR（#75 逃生口）" \
   "unset CLAUDE_CONFIG_DIR; $UNSET; cd '/p' && exec claude" \
   "$(ccm --cwd /p --base --print)"
ck "-- 之后透传给 agent，含特殊字符正确 quote" \
   "$UNSET; cd '/p' && exec claude 'a b' 'x'\''y'" \
   "$(ccm --cwd /p --print -- "a b" "x'y")"
ck "--account 与 --base 互斥" \
   "ccm: --account 与 --base 互斥" \
   "$(ccm --cwd /p --account z --base --print)"
ck "未知 agent 报错" \
   "ccm: 未知 agent: gpt（支持 claude|codex）" \
   "$(ccm --agent gpt --cwd /p --print)"
ck "未知选项报错" \
   "ccm: 未知选项: --nope（用 --help 看用法）" \
   "$(ccm --nope --print)"
ck "attach 动作" \
   "tmux attach -t '=cc-foo:'" \
   "$(ccm attach cc-foo --print)"

echo
echo "===== 账号三态（D 审计 B1/B2 回归）====="
ACCTMP="$(mktemp -d)"; mkdir -p "$ACCTMP/z" "$ACCTMP/b"
cat > "$ACCTMP/m.json" <<JSON
{ "version": 1, "accounts": [
  { "name": "z", "configDir": "$ACCTMP/z", "isDefault": true },
  { "name": "b", "configDir": "$ACCTMP/b", "isDefault": false } ] }
JSON
acct() { CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST="$ACCTMP/m.json" bash "$CCM" "$@" 2>&1; }
ck "显式 --account 注入其 configDir" \
   "export CLAUDE_CONFIG_DIR='$ACCTMP/b'; $UNSET; cd '/p' && exec claude" \
   "$(acct --cwd /p --account b --print)"
# B1：die 在 \$(...) 里只杀子 shell —— 曾"报错后照跑"，落到继承来的账号上且 rc=0
ck "账号不存在 → 中止（rc≠0，且不得吐出 exec）" \
   "ccm: 账号 'nope' 不可用（不在 $ACCTMP/m.json，或其目录不存在）。可用: z b" \
   "$(acct --cwd /p --account nope --print)"
ck "账号不存在 → rc=2" "2" \
   "$(acct --cwd /p --account nope --print >/dev/null 2>&1; echo $?)"
# B2：cc-acct-iso 搬走凭据后基座常已无 .credentials.json —— 不落默认号则 cc/cct 掉进未登录目录
ck "不传 --account → 落 manifest 的 isDefault（复刻旧 _cc_acct_last 粘滞）" \
   "export CLAUDE_CONFIG_DIR='$ACCTMP/z'; $UNSET; cd '/p' && exec claude" \
   "$(acct --cwd /p --print)"
ck "--base → 显式不注入（#75 逃生口，压过默认号）" \
   "unset CLAUDE_CONFIG_DIR; $UNSET; cd '/p' && exec claude" \
   "$(acct --cwd /p --base --print)"
ck "无账号库 → 退化为基座（不报错）" \
   "$UNSET; cd '/p' && exec claude" \
   "$(ccm --cwd /p --print)"
rm -rf "$ACCTMP"

echo
echo "===== 动作/目录语义（D 审计：auto 只对 new 生效）====="
ck "resume 不做 auto 解析（cc-monitor 已 cd 到会话目录，再解析会跑到 git 仓父目录）" \
   "$UNSET; cd '$PWD' && exec claude --resume s1" \
   "$(ccm --resume s1 --print)"
ck "resume 后跟 flag → 报错（别把 --tmux 当 sid）" \
   "ccm: resume 需要 <sid>" \
   "$(ccm resume --tmux --print)"
ck "attach 后跟 flag → 报错" \
   "ccm: attach 需要 <会话名>" \
   "$(ccm attach --print)"

echo
echo "===== --cwd auto 与旧 _cc_resolve_target 对拍（5 种布局）====="
TMPROOT="$(mktemp -d)"
trap 'rm -rf "$TMPROOT"' EXIT
export CC_WORKSPACE="$TMPROOT/workspace"; mkdir -p "$CC_WORKSPACE"
mkdir -p "$TMPROOT/plain" "$TMPROOT/repo/sub/deep"
( cd "$TMPROOT/repo" && git init -q . 2>/dev/null )
FAKEHOME="$TMPROOT/home"; mkdir -p "$FAKEHOME"

# CCM_CONFIG 指到一个临时 config，把 workspace 对齐到对照值
CFG="$TMPROOT/ccm-config"; printf 'CCM_WORKSPACE=%s\n' "$CC_WORKSPACE" > "$CFG"
cmp_cwd() {
  local desc="$1" dir="$2" home="${3:-$HOME}" got want
  want="$( cd "$dir" && HOME="$home" CC_WORKSPACE="$CC_WORKSPACE" bash -c '
      if [ "$PWD" = "$HOME" ]; then REPLY="$CC_WORKSPACE"
      else g="$(git rev-parse --show-toplevel 2>/dev/null)"; [ -n "$g" ] && REPLY="$(dirname "$g")" || REPLY="$PWD"; fi
      printf "%s" "$REPLY"' )"
  got="$( cd "$dir" && HOME="$home" CCM_SELF=/usr/local/bin/ccm CCM_CONFIG="$CFG" \
      CCM_ACCTS_MANIFEST=/nonexistent bash "$CCM" --print 2>&1 | sed -n "s/.*cd '\\([^']*\\)' && .*/\\1/p" )"
  ck "$desc" "$want" "$got"
}
cmp_cwd "布局1：在 \$HOME → 工作区"        "$FAKEHOME" "$FAKEHOME"
cmp_cwd "布局2：git 仓根 → 仓的父目录"      "$TMPROOT/repo"
cmp_cwd "布局3：git 仓子目录 → 仓的父目录"  "$TMPROOT/repo/sub/deep"
cmp_cwd "布局4：非 git 目录 → 目录自己"     "$TMPROOT/plain"
cmp_cwd "布局5：工作区自身（非 git）→ 自己" "$CC_WORKSPACE"

echo
echo "===== 会话名派生：与前端 deriveTmuxName **真值对拍**（跨语言漂移守卫）====="
# 同规则 = 终端 cct 与 app「开新 Claude」在同一目录造出同一个名字 → 幂等接回同一会话。
# 不与手写期望比，与 src/remote-launch.ts 的真实实现比。
# **必须 env -u TMUX**：CLI 在 tmux 内会退化成"就地起"（不建嵌套会话），
# 那时 --print 没有 tmux 命令序列可抓。生产路径是 `ssh -t … bash -lic`，$TMUX 本就不存在。
name_of() { env -u TMUX CCM_SELF=/usr/local/bin/ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST=/nonexistent bash "$CCM" --tmux --cwd "$1" --print 2>&1 \
            | sed -n "s/^tmux new-session -d -s \\('[^']*'\\|[^ ]*\\) .*/\\1/p" | tr -d "'"; }
if command -v npx >/dev/null 2>&1; then
  for d in /home/pi/proj "/home/pi/a  b" /home/pi/proj/// / /home/pi/.hidden.dir; do
    want="$(cd "$REPO" && npx --no-install tsx -e "
      import {deriveTmuxName} from './src/remote-launch.ts';
      process.stdout.write(deriveTmuxName(process.argv[1]));
    " "$d" 2>/dev/null)"
    got="$(name_of "$d")"
    # tmux 名撞名时 CLI 会加 -2/-3；此处只比基名（测试环境不建会话，故恒等基名）
    ck "deriveTmuxName 对拍: $d" "$want" "$got"
  done
else
  echo "SKIP | 无 npx，跳过跨语言对拍"
fi

echo
echo "===== 合计 PASS=$PASS FAIL=$FAIL ====="
[ "$FAIL" -eq 0 ]
