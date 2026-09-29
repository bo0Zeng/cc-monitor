#!/bin/bash
# ccm CLI 的 shell 级测试（unify-launch F02）。
#
# 全部走 `--ccm-print`（V138 前叫 `--print`）断言命令串——不真起 agent、不碰 tmux（tmux 行为由
# tests/e2e/tmux-target-acceptance.sh 那套真机 harness 管）。
#
# 跑法：bash tests/e2e/ccm-cli.test.sh   （npm run test:ccm-cli）
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"

# ★★ `K-R48` 第二拍（09-11）：被测对象从 `shared/ccm`（bash）换成**后端二进制本体**。
#   〔用@09-11 `K33`〕逐字「后端**只有一个**，**不要有什么 bash 脚本**，**不要有什么单独的 ccm**」。
#   `intercept` 认的是 `argv[0]` 的 basename ⇒ 做一条叫 `ccm` 的软链指过去。
# 🔴 **fail-closed**：没 build 就响亮退出，不许静默回落到 PATH 上碰巧有的那一份
#   （那正是本文件全篇隔离纪律要治的那一族：测试结果不许随「是谁在跑测试」而漂移）。
CCM_NATIVE="${CARGO_TARGET_DIR:-$REPO/.build/backend}/debug/cc-monitor-backend"
[ -x "$CCM_NATIVE" ] || {
  echo "::error::找不到原生入口 $CCM_NATIVE —— 先 \`cd src/backend && cargo build --bin cc-monitor-backend\`" >&2
  exit 2; }
CCMDIR="$(mktemp -d)"; trap 'rm -rf "$CCMDIR"' EXIT
ln -s "$CCM_NATIVE" "$CCMDIR/ccm"
CCM="$CCMDIR/ccm"

# ⚠ 〔`K-R48` 第二拍 09-11〕**这里原来有两道 `jq` 的 fail-closed 硬依赖闸，本轮删了。**
#   它们守的是本文件那几份**假 backend**（`mk_mirror_backend` 用 `jq` 把夹具 manifest 翻成
#   `--list-accounts` 的帧形状、`KREC` 的记账 shim 转发给 `/usr/bin:/bin` 上那一个）——
#   而那几份假后端与它们服务的判据本轮一起删了：同一个进程之下没有「帧」这回事，
#   账号表由后端自己读那份 manifest。**本文件今天一处都不用 `jq`**（`grep -c jq` 自己看）。
#   〔FIX4 · J7〕原来这里还有一条「`npx` 那条纪律仍在」（会话名派生那一节拿 `npx tsx` 真跑前端那个函数对拍）：前端那份删了，那一节改手写期望，本文件不再要 `npx`。

PASS=0; FAIL=0
ck() { # ck <描述> <期望> <实得>
  if [ "$2" = "$3" ]; then printf 'PASS | %s\n' "$1"; PASS=$((PASS+1))
  else printf 'FAIL | %s\n      期望: %s\n      实得: %s\n' "$1" "$2" "$3"; FAIL=$((FAIL+1)); fi
}
# **必须同时隔离 CCM_ACCTS_MANIFEST**：不隔离的话本机 manifest 的 isDefault 账号会被注入
# 每条黄金串（"零修饰 = 今天的 ccm()" 是**基座**语义，不带账号）。默认号注入另有专测。
# **全文件恒隔离 CLAUDE_CONFIG_DIR**：本机开发者本人就可能正跑在某个隔离账号下（这里真的
# 踩过——CLAUDE_CONFIG_DIR=/home/zbl/.claude-accts/z 是本次开发时的真实环境）。account-reset
# 修复后 ccm 会真的读这个变量，不隔离会让测试结果随"是谁在跑测试"而漂移。
# ★★ 〔`K-R48` 第二拍 09-11〕**`CCM_BACKEND_BIN` 那一栏没了，`CCM_ACCTS_MANIFEST` 那一栏留着。**
#   从前本套件要**自带一份后端**（`FAKED`）：账号解析没有本地退路，不给后端的话每一条判据
#   都会死在 `exit 4` 上。今天敲的那个命令**就是**后端 ⇒ 那个变量在原生实现里现打 `grep -rn`
#   **零命中**，留着它等于在夹具里摆一个谁也不读的旋钮。
#   ⚠ `CCM_ACCTS_MANIFEST` 仍是 `<目录>/accounts.json` 这个形态（不是裸 `/nonexistent`）——
#     那是**生产上那个值的形状**，与「后端是谁」无关。
ccm() { env -u CLAUDE_CONFIG_DIR CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST=/nonexistent/accounts.json "$CCM" "$@" 2>&1; }

UNSET="unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID CLAUDE_CODE_CHILD_SESSION"

echo "===== 契约：壳层选项 × 交给 claude 的参数（V138）====="
# 〔AL3 · V138〕ccm 是 claude 的壳：位置动作取消、`--resume` / `--model` / 未知旗标原样交 claude、诊断口改 `--ccm-*`；
#   从前 resume 那条 `set -f; exec <后端答案>` 的配方随进程内 `resolve` 一起删了（留着会与透传的 `--resume` 叠两份）。

# 〔用户 09-26〕相对 `--cwd` 按当前目录补成绝对、按字面折掉 `.` / `..`，再过 §47 形式判定 ⇒ 零修饰回到 `--cwd .`，期望是绝对路径。
HERE_P="$(pwd -P)"
ck "零修饰（--cwd .）：补成绝对的当前目录" \
   "$UNSET; cd '$HERE_P' && exec claude" \
   "$(ccm -- --cwd . --ccm-print)"
ck "相对 --cwd ../x：按当前目录补全、折掉 .." \
   "$UNSET; cd '$(dirname "$HERE_P")/x' && exec claude" \
   "$(ccm -- --cwd ../x --ccm-print)"
ck "--resume <sid> 原样交给 claude（ccm 只看不吃）" \
   "$UNSET; cd '/p' && exec claude --resume abc-123" \
   "$(ccm --resume abc-123 -- --cwd /p --ccm-print)"
ck "-r <sid> 同样原样交出去（不翻译成长形）" \
   "$UNSET; cd '/p' && exec claude -r abc-123" \
   "$(ccm -r abc-123 -- --cwd /p --ccm-print)"
ck "--resume=<sid> 等号形式原样交出去" \
   "$UNSET; cd '/p' && exec claude --resume=abc-123" \
   "$(ccm --resume=abc-123 -- --cwd /p --ccm-print)"
# U9a 2026-08-02：codex 的黄金串多了 cc-bus 身份注入那一段。
# **它一直都在真 exec 那条路上**（`shared/ccm::derive_bus_id`，codex 沙箱够不着 tmux socket ⇒
# 会话名必须经 env 透进去），只是 `--print` 从来没说 —— 而整个仓拿 `--print` 当离线预言机。
# 打印的是**配方不是值**（`TMUX` 判断留在串里、执行时才求值），所以这条串对宿主 `TMUX`
# 仍然逐字节稳定，不需要给这个 helper 加 `env -u TMUX`（那就成了为实现让路改判据）。
# 〔SH1 · §49〕配方里读会话名那一发带 `-u`（主线 SH1 改了配方、这一行没跟上，本路合并时补）。
ck "--ccm-agent codex：换启动器 + 无嵌套 env + cc-bus 身份配方" \
   "if [ -n \"\${TMUX:-}\" ]; then _ccm_bus=\"\$(tmux -u display-message -p \"#S\" 2>/dev/null)\"; [ -n \"\$_ccm_bus\" ] && export CC_BUS_ID=\"\$_ccm_bus\"; unset _ccm_bus; fi; cd '/p' && exec codex" \
   "$(ccm -- --ccm-agent codex --cwd /p --ccm-print)"
ck "--ccm-agent codex：resume <sid> 原样交给 codex（它自己的子命令形；从前 ccm 报「不支持 resume」）" \
   "yes" \
   "$(ccm resume x -- --ccm-agent codex --cwd /p --ccm-print | grep -q "cd '/p' && exec codex resume x\$" && echo yes || echo no)"
ck "--launcher 覆盖默认启动器" \
   "$UNSET; cd '/p' && exec mycc --resume s1" \
   "$(ccm --resume s1 -- --cwd /p --launcher mycc --ccm-print)"
ck "--base：显式 unset CLAUDE_CONFIG_DIR（#75 逃生口）" \
   "unset CLAUDE_CONFIG_DIR; $UNSET; cd '/p' && exec claude" \
   "$(ccm -- --cwd /p --base --ccm-print)"
ck "--model 原样交给 claude（V138：不再 export ANTHROPIC_MODEL）" \
   "$UNSET; cd '/p' && exec claude --resume s1 --model opus" \
   "$(ccm --resume s1 --model opus -- --cwd /p --ccm-print)"
ck "--model=<名> 等号形式原样交出去" \
   "$UNSET; cd '/p' && exec claude --model=opus" \
   "$(ccm --model=opus -- --cwd /p --ccm-print)"
ck "〔V151〕-- 左边透传给 agent，含特殊字符正确 quote" \
   "$UNSET; cd '/p' && exec claude 'a b' 'x'\''y'" \
   "$(ccm "a b" "x'y" -- --cwd /p --ccm-print)"
ck "〔V151〕-- 左边原样交给 claude，按原顺序" \
   "$UNSET; cd '/p' && exec claude -p 'a b' --verbose" \
   "$(ccm -p "a b" --verbose -- --cwd /p --ccm-print)"
ck "claude 自己的 --tmux / --agent 原样交出去（用户 09-26：ccm 的改名 --ccm-tmux / --ccm-agent）" \
   "$UNSET; cd '/p' && exec claude --tmux --agent x" \
   "$(ccm --tmux --agent x -- --cwd /p --ccm-print)"
ck "〔V153〕ccm new（-- 左边）照 V151 整个交给 claude，不开例外" \
   "$UNSET; cd '/p' && exec claude new" \
   "$(ccm new -- --cwd /p --ccm-print)"
ck "〔V153〕new 是 ccm 自己的词：写在 -- 右边第一个 = 起新会话（与不写同）" \
   "$UNSET; cd '/p' && exec claude -p x" \
   "$(ccm -p x -- new --cwd /p --ccm-print)"
ck "〔V153〕new 不在 -- 右边第一个 ⇒ 报错（不猜）" \
   "ccm: new 只能是 -- 右边第一个词（ccm [交给 claude 的…] -- new [ccm 的选项…]）" \
   "$(ccm -- --cwd /p new --ccm-print 2>&1)"
ck "--launcher 'ccr code' 拆成词（用户 09-26）" \
   "$UNSET; cd '/p' && exec ccr code -p x" \
   "$(ccm -p x -- --launcher 'ccr code' --cwd /p --ccm-print)"
ck "--account 与 --base 互斥" \
   "ccm: --account 与 --base 互斥" \
   "$(ccm -- --cwd /p --account z --base --ccm-print)"
ck "未知 agent 报错" \
   "ccm: 未知 agent: gpt（支持 claude|codex）" \
   "$(ccm -- --ccm-agent gpt --cwd /p --ccm-print)"
ck "未知选项原样交给 claude（V138：从前报「未知选项」）" \
   "$UNSET; cd '/p' && exec claude --nope" \
   "$(ccm --nope -- --cwd /p --ccm-print)"
ck "--help 交给 claude（ccm 自己的帮助是 --ccm-help）" \
   "$UNSET; cd '/p' && exec claude --help" \
   "$(ccm --help -- --cwd /p --ccm-print)"
ck "--ccm-version 是 ccm 自己的版本" \
   "ccm 6" \
   "$(ccm -- --ccm-version)"
ck "--attach 接回（V138：位置动作 attach 取消）" \
   "tmux attach -t '=cc-foo:'" \
   "$(ccm -- --attach cc-foo --ccm-print)"

echo
echo "===== 账号三态（D 审计 B1/B2 回归）====="
# ★★ `K-C1`（08-24）**本组的夹具改了两处，都是为了让它测的是生产形状**：
#   ① `m.json` → `accounts.json`。backend 的 `--list-accounts` **只收目录**（`--accts-dir`），
#      manifest 的文件名由它自己拼（`acct-core::MANIFEST_NAME`）⇒ 叫别的名字时 ccm 判得出
#      「这个问题后端答不了」、降级读文件并**说一句**，于是这几条黄金串会多出一行 stderr。
#      生产路径本来就是 `<目录>/accounts.json`（默认值与 cc-acct-iso 的 `ACCTS_DIR` 都是），
#      夹具跟上去 = 测的是真形状，不是「顺手把判据改绿」。
#   ② 〔原第②条：`CCM_BACKEND_BIN` 钉到一份假后端，免得查找次序摸到开发机上用户的真二进制〕
#      **`K-R48` 第二拍 09-11 作废** —— 没有查找次序了，敲的那个命令就是后端。
#      它治的那条病（同一条判据在开发机与 CI 上走两条不同的路）今天由文件顶上那道
#      `[ -x "$CCM_NATIVE" ]` fail-closed 顶着：被测对象**只可能**是本工作树刚 build 出来的那一份。
ACCTMP="$(mktemp -d)"; mkdir -p "$ACCTMP/z" "$ACCTMP/b" "$ACCTMP/bin"
cat > "$ACCTMP/accounts.json" <<JSON
{ "version": 1, "accounts": [
  { "name": "z", "configDir": "$ACCTMP/z", "isDefault": true },
  { "name": "b", "configDir": "$ACCTMP/b", "isDefault": false } ] }
JSON
# ⚠ 〔`K-R48` 第二拍 09-11〕**这里原来有一份镜像式假后端（`mk_mirror_backend`），本轮删了。**
#   它的活是「把夹具 manifest 原样翻成 `--list-accounts` 帧形状」，好让 bash 那侧跨进程问到账号表。
#   今天后端**自己读**那份 manifest（`CCM_ACCTS_MANIFEST` 仍是唯一事实源），中间那一跳没有了。
#   下面那几个 helper 里留着的 `CCM_BACKEND_BIN=` 也一并去掉：原生实现现打 `grep -rn` **零命中**，
#   留着它等于在夹具里摆一个谁也不读的旋钮。
acct() { env -u CLAUDE_CONFIG_DIR CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST="$ACCTMP/accounts.json" "$CCM" "$@" 2>&1; }
ck "显式 --account 注入其 configDir" \
   "export CLAUDE_CONFIG_DIR='$ACCTMP/b'; $UNSET; cd '/p' && exec claude" \
   "$(acct -- --cwd /p --account b --ccm-print)"
# B1：die 在 \$(...) 里只杀子 shell —— 曾"报错后照跑"，落到继承来的账号上且 rc=0
ck "账号不存在 → 中止（rc≠0，且不得吐出 exec）" \
   "ccm: 账号 'nope' 不可用（不在 $ACCTMP/accounts.json，或其目录不存在）。可用: z b" \
   "$(acct -- --cwd /p --account nope --ccm-print)"
ck "账号不存在 → rc=2" "2" \
   "$(acct -- --cwd /p --account nope --ccm-print >/dev/null 2>&1; echo $?)"
# B2：cc-acct-iso 搬走凭据后基座常已无 .credentials.json —— 不落默认号则 cc/cct 掉进未登录目录
ck "不传 --account → 落 manifest 的 isDefault（复刻旧 _cc_acct_last 粘滞）" \
   "export CLAUDE_CONFIG_DIR='$ACCTMP/z'; $UNSET; cd '/p' && exec claude" \
   "$(acct -- --cwd /p --ccm-print)"
ck "--base → 显式不注入（#75 逃生口，压过默认号）" \
   "unset CLAUDE_CONFIG_DIR; $UNSET; cd '/p' && exec claude" \
   "$(acct -- --cwd /p --base --ccm-print)"
ck "无账号库 → 退化为基座（不报错）" \
   "$UNSET; cd '/p' && exec claude" \
   "$(ccm -- --cwd /p --ccm-print)"
ck "--account 与 --model 组合：账号目录照注入，--model 原样交给 claude（V138）" \
   "export CLAUDE_CONFIG_DIR='$ACCTMP/b'; $UNSET; cd '/p' && exec claude --model sonnet" \
   "$(acct --model sonnet -- --cwd /p --account b --ccm-print)"
rm -rf "$ACCTMP"

echo
echo "===== 动作/目录语义（D 审计：auto 只对 new 生效）====="
ck "resume 不做 auto 解析（cc-monitor 已 cd 到会话目录，再解析会跑到 git 仓父目录）" \
   "$UNSET; cd '$PWD' && exec claude --resume s1" \
   "$(ccm --resume s1 -- --ccm-print)"
ck "--attach 后跟 flag → 报错（别把 --ccm-tmux 当会话名）" \
   "ccm: --attach 需要一个值，但拿到的是 '--ccm-tmux'（像是漏了参数）" \
   "$(ccm -- --attach --ccm-tmux --ccm-print)"
ck "--attach 缺值 → 报错" \
   "ccm: --attach 需要一个值" \
   "$(ccm -- --attach)"

echo
echo "===== 账号继承（F03 综合设计时发现的 bug 回归）====="
# 真机复现过：cc-monitor 把「远端 resume 命令」配成 ccm 时，实际调用形态是
#「外层已 export 好账号 X 的 CLAUDE_CONFIG_DIR，再 exec ccm --resume <sid>（不带任何账号 flag）」。
# 若 ccm 无脑落 manifest 默认号，会把 cc-monitor 精心选中的账号**静默覆盖**——账号选择完全失效，
# 且正是这轮建议用户使用的配置会踩中的场景。
# **必须 `-u TMUX -u TMUX_PANE`**：下面 R08 那几条测的是**容器路径**，而 ccm 有一条
# 「已在 tmux 内且未给会话名 → 就地起，不建嵌套」的分支。开发者在 tmux 里跑这套件时，
# `--tmux` 会落进那条分支、根本不走容器路径，于是 4 条 R08 断言假红（CI 上无 TMUX 所以
# 一直看不出来）。实测：同一份 HEAD，`TMUX` 有无决定 44/0 还是 40/4。
# 测什么就要固定什么，不能让环境替测试选路径。
# `K-C1`：夹具改名 + 钉假后端，理由同上一组（那段头注逐条写了，别在这儿重抄）。
inherit_acct() { CLAUDE_CONFIG_DIR="$ACCTMP/b" env -u TMUX -u TMUX_PANE CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST="$ACCTMP/accounts.json" "$CCM" "$@" 2>&1; }
ACCTMP="$(mktemp -d)"; mkdir -p "$ACCTMP/z" "$ACCTMP/b" "$ACCTMP/bin"
cat > "$ACCTMP/accounts.json" <<JSON
{ "version": 1, "accounts": [
  { "name": "z", "configDir": "$ACCTMP/z", "isDefault": true },
  { "name": "b", "configDir": "$ACCTMP/b", "isDefault": false } ] }
JSON
ck "外层已继承账号 b（无 --account/--base）→ 保留 b，不被默认号 z 静默覆盖"    "$UNSET; cd '/p' && exec claude"    "$(inherit_acct -- --cwd /p --ccm-print)"
ck "裸终端（无继承）仍落 manifest 默认号 z"    "export CLAUDE_CONFIG_DIR='$ACCTMP/z'; $UNSET; cd '/p' && exec claude"    "$(env -u CLAUDE_CONFIG_DIR CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST="$ACCTMP/accounts.json" "$CCM" -- --cwd /p --ccm-print 2>&1)"
ck "--base 显式清空，不受继承影响"    "unset CLAUDE_CONFIG_DIR; $UNSET; cd '/p' && exec claude"    "$(inherit_acct -- --cwd /p --base --ccm-print)"
ck "--account 显式指定，优先级最高（覆盖继承的 b）"    "export CLAUDE_CONFIG_DIR='$ACCTMP/z'; $UNSET; cd '/p' && exec claude"    "$(inherit_acct -- --cwd /p --account z --ccm-print)"

# R08（2026-07-28 实测复现）：R11 的修法在**容器路径上留了个洞**。
# 上面那条注释说"两个场景用同一条 if 天然区分"，**只对非容器路径成立**——
# 非容器路径下 `exec` 保留进程环境，"尊重继承值"= 什么都不做就已经对了；
# 但容器路径下 send-keys 打进的是 **tmux server fork 的新 shell**，
# `update-environment` 默认列表不含 CLAUDE_CONFIG_DIR（这正是本项目要治的那个病），
# 于是继承值在 L1 边界被吃掉，内层 ccm 看到空值 + 无账号 flag → 落 manifest 默认号 z。
# 症状与 R11 同型且同样隐蔽：**看起来生效了，只是换成了错的号。**
# 命中条件：启动器自己建容器（cct = ccm --tmux）+ 账号只靠继承环境变量传递。
# 载荷在 --print 里是 `'\''` 转义形态（外层再被单引号包一层），直接 grep 裸引号模式会全部落空
# ——我第一版就写错成这样，三条里两条假红。先反转义 `'\''` → `'` 再匹配。
# （R02 记的第三个失效模式的同型：断言模式与被测输出的真实形态不符 = 假信号。）
unesc() { sed "s/'\\\\''/'/g"; }
ck "R08：容器路径 + 继承账号 b → 内层载荷必须显式带上 b（不能靠继承穿 tmux 边界）" \
   "yes" \
   "$(inherit_acct -- --ccm-tmux --cwd /p --ccm-print | unesc | grep -qF "$ACCTMP/b" && echo yes || echo no)"
ck "R08：容器路径 + 继承账号 b → 内层绝不能落到默认号 z" \
   "yes" \
   "$(inherit_acct -- --ccm-tmux --cwd /p --ccm-print | unesc | grep -qF "$ACCTMP/z" && echo no || echo yes)"
ck "R08：容器路径 + --base → 内层显式 --base（不受继承影响，issue #75 逃生口不被削弱）" \
   "yes" \
   "$(inherit_acct -- --ccm-tmux --cwd /p --base --ccm-print | unesc | grep -qF -- '--base' && echo yes || echo no)"
ck "R08：容器路径 + 显式 --account z → 内层带 --account z（优先级不变）" \
   "yes" \
   "$(inherit_acct -- --ccm-tmux --cwd /p --account z --ccm-print | unesc | grep -qF -- "'--account' 'z'" && echo yes || echo no)"
ck "R08：容器路径 + 裸终端（无继承）→ 内层仍落默认号 z（粘滞体验不回退）" \
   "yes" \
   "$(env -u CLAUDE_CONFIG_DIR -u TMUX -u TMUX_PANE CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST="$ACCTMP/accounts.json" "$CCM" -- --ccm-tmux --cwd /p --ccm-print 2>&1 | unesc | grep -qF -- "'--account' 'z'" && echo yes || echo no)"
rm -rf "$ACCTMP"

echo
echo "===== 不给 --cwd = 站在哪儿起在哪儿（5 种布局，一格都不许跳）====="
TMPROOT="$(mktemp -d)"
trap 'rm -rf "$TMPROOT"' EXIT
export CC_WORKSPACE="$TMPROOT/workspace"; mkdir -p "$CC_WORKSPACE"
mkdir -p "$TMPROOT/plain" "$TMPROOT/repo/sub/deep"
( cd "$TMPROOT/repo" && git init -q . 2>/dev/null )
FAKEHOME="$TMPROOT/home"; mkdir -p "$FAKEHOME"

# 🔴 〔`K-R58` 09-11 · `KR58D3` · `K37` 第三条〕**这一组被翻过来了，不是被删掉。**
#
#   上一版这里逐字写着「`--cwd auto` 与旧 `_cc_resolve_target` 对拍」，`want` 那一半是
#   那段旧 bash 的逐字复刻：**在 $HOME 跳工作区 · 在 git 仓跳仓的父目录**。
#   〔用@09-11 逐字〕「`cc` 默认就起会话就行，**跳目录是我自己的设置，不要搞进 app**。」
#   ⇒ 那两档按 `K37`（「把这个行为去掉，用户还做不做得到同一件事」= 做得到 ⇒ 偏好，出去）
#   删了，于是**把旧语义钉死的正是这 5 条判据本身** —— 本件不删它们，改成钉新语义：
#   同样这 5 种布局，今天一律该停在原地。布局 1/2/3 就是那两档旧分支的现场，
#   它们从「证明会跳」变成「证明不跳」，**射程一格没少**。
#
# 🔴 **`CCM_WORKSPACE` 这里是故意还在导出的**：`KR58D3` 的失效方向逐字是
#   「把猜挪进别处（比如挪成一个默认开着的开关）」。设着它、站在 $HOME、答案仍必须是
#   $HOME —— 那一档要是哪天悄悄回来，这一条当场红。（后端今天**根本不读**这个变量了：
#   `Env` 里那个 `workspace` 字段跟着删了。）
#
# ⚠ `want` 取 `pwd -P`（物理路径）而不是 `$PWD`：`mktemp -d` 在有符号链接的 `/tmp` 上
#   两者不同值，而 Rust 的 `current_dir()` 给的是物理路径 —— 那会是一次跟本题无关的假红。
cmp_cwd() {
  local desc="$1" dir="$2" home="${3:-$HOME}" got want
  want="$( cd "$dir" && pwd -P )"
  got="$( cd "$dir" && HOME="$home" CCM_CONFIG=/nonexistent \
      CCM_WORKSPACE="$CC_WORKSPACE" \
      CCM_ACCTS_MANIFEST=/nonexistent/accounts.json "$CCM" -- --ccm-print 2>&1 | sed -n "s/.*cd '\\([^']*\\)' && .*/\\1/p" )"
  ck "$desc" "$want" "$got"
}
cmp_cwd "布局1：在 \$HOME（设着 CCM_WORKSPACE）→ 仍是 \$HOME，不跳工作区" "$FAKEHOME" "$FAKEHOME"
cmp_cwd "布局2：git 仓根 → 仍是仓根，不跳仓的父目录"     "$TMPROOT/repo"
cmp_cwd "布局3：git 仓子目录 → 仍是那个子目录"          "$TMPROOT/repo/sub/deep"
cmp_cwd "布局4：非 git 目录 → 目录自己"                 "$TMPROOT/plain"
cmp_cwd "布局5：工作区自身（非 git）→ 自己"             "$CC_WORKSPACE"

echo
echo "===== 会话名派生：真跑那条路铸出来的名字（〔FIX4 · J7〕前端那份删了，规则只剩后端一份）====="
# 同规则 = 终端里敲 `ccm` 与 app「开新 Claude」（问后端 `tmux-name-mint`，同一个 `plan::mint_tmux_name`）在同一目录铸同一个名字。
# 〔FIX4 · `设计/90 §3` J7〕原先这 5 条拿 `npx tsx` 真跑前端 `deriveTmuxName` 对拍（跨语言双写点的漂移守卫，E49）。
#   前端那份删了 ⇒ 没有第二份可拍；期望改手写（与 `plan_tests.rs::the_session_name_derivation_rule` 同一组样本），
#   钉的是「`--ccm-print` 那一行里抽得出这个名字」—— 抽取器失灵（下面 `^[{ ]*` 那段注脚）照样当场红。
# **必须 env -u TMUX**：CLI 在 tmux 内会退化成"就地起"（不建嵌套会话），
# 那时 --print 没有 tmux 命令序列可抓。生产路径是 `ssh -t … bash -lic`，$TMUX 本就不存在。
# ⚠ **`^[{ ]*` 不能省**〔`U-NP④` 08-14 顺手修的既有腐坏〕：`P3sc`（08-13）把撞名改成
# 「响亮失败」时，把 `tmux new-session` 包进了 `{ … || { …; exit 3; }; }` ——
# 于是这条 `sed` 的 `^tmux` 锚点**零命中**，下面 5 条**全部拿到空串、静默常红**。
# 这正是「判据的匹配单位跟不上事实的形状」那一族：报的是「不一致」，真因是抽取器失灵。
name_of() { env -u TMUX CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST=/nonexistent/accounts.json "$CCM" -- --ccm-tmux --cwd "$1" --ccm-print 2>&1 \
            | sed -n "s/^[{ ]*tmux new-session -d -s \\('[^']*'\\|[^ ]*\\) .*/\\1/p" | tr -d "'"; }
# tmux 名撞名时 CLI 会加 -2/-3；此处只比基名（测试环境不建会话，故恒等基名）
for pair in "/home/pi/proj|proj-cc" "/home/pi/a  b|a-b-cc" "/home/pi/proj///|proj-cc" "/|session-cc" "/home/pi/.hidden.dir|hidden-dir-cc"; do
  d="${pair%%|*}"; want="${pair##*|}"
  ck "会话名派生: $d" "$want" "$(name_of "$d")"
done

# ══════════════════════════════════════════════════════════════════════════════
# ⚠ 〔`K-R48` 第二拍 09-11〕**本文件原来有 264 条断言，本轮删到 46 条。**
#
# 〔用@09-11 `K33`〕逐字「后端**只有一个**，**不要有什么 bash 脚本**，**不要有什么单独的 ccm**」
# ⇒ `shared/ccm` 删了，本套件的被测对象换成后端二进制本体。
# 删掉的 218 条**逐条判词住 `tests/evidence/K-R48-356-verdicts.tsv`**（第 21–284 行是本套件那 264 条），
# 按族：
#   · 身份后端前置检查 15 条（第 67–81）—— 「在 tmux 里找不到 backend ⇒ 响亮失败」那一族。
#     **同一个二进制之下「找不到后端」这个概念不存在了**：敲的那个命令就是后端。
#   · 账号解析走后端一整节（第 84–152）—— 往返次数 / 帧形状 / 无 jq 纯 bash 解析路 /
#     「这段 bash 起了几个外部进程」的记账尺子。语义那一半已落成后端侧 Rust 判据
#     （`the_account_table_has_exactly_one_source` · `picking_an_account_never_falls_back_to_a_different_one`
#      · `needs_account_table`）。
#   · `JSONENC` 47 条（第 153–199）—— 那个**手写的 bash JSON 编码器**与 `jq -Rs .` 的逐字节对拍。
#     Rust 侧是 `serde_json`，**不是我们的实现，不必我们来测**。
#   · `WIRE` 85 条（第 200–284）—— 「发了 / 发对了 / 不可达 / 撞名 / 缺省尺寸 / 控制字符」。
#     同一个进程之下**没有「上线字节」这回事**；其中「那几件事一件都不许丢」已落成 Rust 判据
#     `the_container_launch_goes_through_the_one_door_with_every_field_intact`。
#
# 🔴 **两族是本拍实测推翻第一拍判词的，写在这里而不是藏进 TSV**（第一拍判 `M-repoint`，
#    本拍指过去之后发现它们没有指称对象 ⇒ 降级为 `N`）：
#   ① 第 82–83（`--print` 不受身份前置检查影响 ＋ 它的非空对照）——
#      **非空对照那一条断的正是 `rc=2`**，而那道检查随第 67–81 那族一起没了。
#      留下第 82 条单条 = 一条恒真（那正是第 83 条当初被加进来要防的东西）。
#   ② 第 281–284（`WIRE/launch/--print` 的纯性 4 条）—— 第 281 条数的是「假后端被调了几次」，
#      而原生实现根本不去调任何外部 backend ⇒ **恒 0，空真**。
#      余下三条的性质仍在别处守着：`--print` 吐本机 tmux 编排由上面「账号继承」那 5 条
#      容器路黄金串逐字钉住；「stderr 一个字都没有」由本文件每一条黄金串的 `2>&1` 口径钉住
#      （`ccm()` 把 stderr 并进被比的串 —— 吵一个字就当场不等）。
# ══════════════════════════════════════════════════════════════════════════════

echo
echo "===== 〔MC1〕CCM_SELF 删了：内层载荷只认「这个进程自己被怎么叫的」====="
# 〔MC1 · 2026-09-24〕`设计/01 §6.7b`：「`CCM_SELF` 这个环境变量随之删掉」。
# 两向：① 设了一个假值，容器路的内层载荷里**一处都不许出现它**（有人把那一格读回来 ⇒ 当场红）；
#       ② 正控：内层载荷真的以本进程的入口（`$CCM` 这条软链）开头 —— 否则 ① 可以靠「内层根本没打出来」零命中地绿。
SELF_OUT="$(env -u TMUX -u CLAUDE_CONFIG_DIR CCM_SELF=/bogus/old-ccm CCM_CONFIG=/nonexistent CCM_ACCTS_MANIFEST=/nonexistent/accounts.json "$CCM" -- --ccm-tmux --cwd /p --ccm-print 2>&1)"
ck "设了 CCM_SELF 也不被读（内层载荷里零命中）" "0" "$(printf '%s\n' "$SELF_OUT" | grep -c 'bogus/old-ccm')"
ck "正控：内层载荷以本进程被叫的那个入口开头" "yes" "$(printf '%s\n' "$SELF_OUT" | unesc | grep -qF "'$CCM' '--' '--cwd'" && echo yes || echo no)"

echo "===== 〔V151〕ccm [交给 claude 的…] -- [ccm 自己的…]：按最后一个 -- 切 ====="
ck "claude 自己的 -- 照写，按最后一个 -- 切（ccm 部分为空时写成 ccm -p -- -x --）" \
   "$UNSET; cd '$HERE_P' && exec claude -p -- -x" \
   "$(cd "$HERE_P" && ccm -p -- -x -- --ccm-print 2>&1 | head -1)"
ck "-- 右边认不得的词直接报错（不猜）" \
   "ccm: -- 右边只放 ccm 自己的选项，--model 不是；交给 claude 的参数写在 -- 左边（ccm -- --ccm-help 看选项）" \
   "$(ccm -- --model opus --ccm-print)"
ck "后端子命令只能紧跟打头的 --" \
   "ccm: --list-sessions 是后端的子命令，只能紧跟打头的 --（ccm -- --list-sessions …），前面不许有交给 claude 的参数" \
   "$(ccm -p -- --list-sessions)"
echo "===== 合计 PASS=$PASS FAIL=$FAIL ====="
[ "$FAIL" -eq 0 ]
