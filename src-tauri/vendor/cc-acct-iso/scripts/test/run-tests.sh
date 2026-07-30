#!/usr/bin/env bash
# cc-acct-iso 沙盒集成测试:全部在 mktemp 造的假 $HOME 上跑,零真机风险。
#   bash ~/.claude/skills/cc-acct-iso/scripts/test/run-tests.sh
# 断言条件刻意用单引号(延迟到 chk/chkn 里 eval 时才展开):SC2016 是预期的。
# shellcheck disable=SC2016,SC2034,SC2329,SC2012
set -uo pipefail

ORIG_HOME="$HOME"
SCRIPTS_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
CLI="$SCRIPTS_DIR/cc-acct-iso"
export NO_COLOR=1
SANDBOXES=()
T=0; F=0; GROUP=""

cleanup() { local s; for s in ${SANDBOXES[@]+"${SANDBOXES[@]}"}; do [ -n "$s" ] && rm -rf -- "$s"; done; }
trap cleanup EXIT

group() { GROUP="$1"; printf '\n\033[1m== %s ==\033[0m\n' "$1"; }
pass()  { T=$((T+1)); printf '  ok   %s\n' "$1"; }
fail()  { T=$((T+1)); F=$((F+1)); printf '  FAIL %s\n     %s\n' "$1" "${2-}"; }
chk()   { if eval "$2" >/dev/null 2>&1; then pass "$1"; else fail "$1" "条件不成立: $2"; fi; }
chkn()  { if eval "$2" >/dev/null 2>&1; then fail "$1" "条件本不该成立: $2"; else pass "$1"; fi; }
eq()    { if [ "$2" = "$3" ]; then pass "$1"; else fail "$1" "期望 [$3] 实得 [$2]"; fi; }

cc() { "$CLI" "$@"; }                      # 正常跑(输出可见)
ccq() { "$CLI" "$@" >/dev/null 2>&1; }     # 静默跑,只看退出码
xok()   { local d="$1"; shift; if "$@" >/dev/null 2>&1; then pass "$d"; else fail "$d" "命令本该成功却失败:$*"; fi; }
xfail() { local d="$1"; shift; if "$@" >/dev/null 2>&1; then fail "$d" "命令本该失败却成功:$*"; else pass "$d"; fi; }

new_sandbox() {
  SB="$(mktemp -d "${TMPDIR:-/tmp}/ccai-test.XXXXXX")"
  SANDBOXES+=("$SB")
  export HOME="$SB"
  export CC_ACCT_ISO_CONFIG="$SB/no-such-config"   # 不存在 ⇒ 全默认值
  mkdir -p "$SB/.claude/skills" "$SB/.claude/projects/proj-a" "$SB/.claude/plugins" \
           "$SB/.claude/sessions" "$SB/.claude/backups" "$SB/.claude/accounts" "$SB/bin"
  printf 'shared skill\n'      >"$SB/.claude/skills/foo.md"
  printf '{"theme":"dark"}\n'  >"$SB/.claude/settings.json"
  printf '# global\n'          >"$SB/.claude/CLAUDE.md"
  printf 'h1\n'                >"$SB/.claude/history.jsonl"
  printf '{"usage":1}\n'       >"$SB/.claude/stats-cache.json"
  printf 'old backup\n'        >"$SB/.claude/settings.json.bak"
  printf 'old-bak2\n'          >"$SB/.claude/settings.json.bak-before-x"
  printf '{"tok":"AAA-default"}' >"$SB/.claude/.credentials.json"
  printf '{"tok":"BBB-second"}'  >"$SB/.claude/accounts/second.json"
  printf 'stale\n'             >"$SB/.claude/backups/old.json"
  cat >"$SB/.claude.json" <<'J'
{
  "oauthAccount": { "emailAddress": "default@example.com", "accountUuid": "uuid-1" },
  "projects": { "/tmp/x": { "hasTrustDialogAccepted": true } },
  "numStartups": 42
}
J
  # 假启动器:打印它看到的 CLAUDE_CONFIG_DIR 和参数
  cat >"$SB/bin/fakeclaude" <<'L'
#!/usr/bin/env bash
# Z01:必须用 `${X-…}` 而**不是** `${X:-…}` —— 后者把**空串**也报成 <unset>,
# 而「空值 ≠ 未设」正是账号 0 全部设计的支点,抹掉它这套断言就成了安慰剂。
printf 'CFG=%s ARGS=%s\n' "${CLAUDE_CONFIG_DIR-<unset>}" "$*"
L
  chmod +x "$SB/bin/fakeclaude"
  PATH="$SB/bin:$PATH"; export PATH
}

snapshot() { ( cd "$SB" && { find . -printf '%y|%p|%l\n' | sort; find . -type f -exec md5sum {} + 2>/dev/null | sort; } ); }

# ══════════════════════════════════════════════════════════════════
group "0. V2 迁移(init):dry-run 零落盘 → --apply 搬对位置"
new_sandbox
BEFORE="$(snapshot)"
xok  "dry-run init 退出码 0" "$CLI" init d
eq   "dry-run 后文件树完全没变" "$(snapshot)" "$BEFORE"
chkn "dry-run 没建 ACCTS_DIR" '[ -e "$SB/.claude-accts" ]'
chk  "dry-run 后凭据仍在原位" '[ -f "$SB/.claude/.credentials.json" ]'

xok  "init --apply 退出码 0" "$CLI" init d --apply
chk  "凭据已搬进账号目录"   '[ -f "$SB/.claude-accts/d/.credentials.json" ]'
chkn "共享库里已无凭据"     '[ -e "$SB/.claude/.credentials.json" ]'
chk  ".claude.json 已搬进账号目录" '[ -f "$SB/.claude-accts/d/.claude.json" ]'
chkn "\$HOME/.claude.json 已迁走"  '[ -e "$SB/.claude.json" ]'
chk  "隔离项 backups/ 跟着搬"      '[ -d "$SB/.claude-accts/d/backups" ] && [ ! -L "$SB/.claude-accts/d/backups" ]'
chk  "隔离项 stats-cache.json 跟着搬" '[ -f "$SB/.claude-accts/d/stats-cache.json" ]'
chk  "共享项 skills 是 symlink"    '[ -L "$SB/.claude-accts/d/skills" ]'
eq   "skills 链指向共享库" "$(readlink "$SB/.claude-accts/d/skills")" "$SB/.claude/skills"
chk  "共享项 projects/settings.json/CLAUDE.md 都在" \
     '[ -L "$SB/.claude-accts/d/projects" ] && [ -L "$SB/.claude-accts/d/settings.json" ] && [ -L "$SB/.claude-accts/d/CLAUDE.md" ]'
chkn "SHARE_EXCLUDE:accounts 没被链" '[ -e "$SB/.claude-accts/d/accounts" ]'
chkn "SHARE_EXCLUDE:*.bak 没被链"    '[ -e "$SB/.claude-accts/d/settings.json.bak" ]'
chkn "SHARE_EXCLUDE:*.bak-* 没被链"  '[ -e "$SB/.claude-accts/d/settings.json.bak-before-x" ]'
eq   "凭据内容原样(未损坏)" "$(cat "$SB/.claude-accts/d/.credentials.json")" '{"tok":"AAA-default"}'
eq   "凭据权限 600" "$(stat -c %a "$SB/.claude-accts/d/.credentials.json")" "600"
eq   "账号目录权限 700" "$(stat -c %a "$SB/.claude-accts/d")" "700"
eq   ".claude.json 保留原内容(numStartups)" "$(jq -r .numStartups "$SB/.claude-accts/d/.claude.json")" "42"

group "1. manifest(契约 schema v1)"
M="$SB/.claude-accts/accounts.json"
chk  "manifest 存在"        '[ -f "$M" ]'
chk  "manifest 是合法 JSON" 'jq -e . "$M"'
eq   "version"      "$(jq -r .version "$M")"      "1"
eq   "sharedStore"  "$(jq -r .sharedStore "$M")"  "$SB/.claude"
eq   "acctsDir"     "$(jq -r .acctsDir "$M")"     "$SB/.claude-accts"
eq   "账号数(1 个注册账号 + 恒在列的账号 0)" "$(jq -r '.accounts|length' "$M")" "2"
eq   "accounts[0].name"      "$(jq -r '.accounts[0].name' "$M")"      "d"
eq   "accounts[0].email(取自 oauthAccount)" "$(jq -r '.accounts[0].email' "$M")" "default@example.com"
eq   "accounts[0].configDir" "$(jq -r '.accounts[0].configDir' "$M")" "$SB/.claude-accts/d"
eq   "accounts[0].isDefault" "$(jq -r '.accounts[0].isDefault' "$M")" "true"
chkn "manifest 里没有任何 token 字样" 'grep -qi "tok" "$M"'

group "2. add:导入旧快照凭据 + 种 .claude.json(剥 oauthAccount)"
BEFORE="$(snapshot)"
ccq add x --from-credentials "$SB/.claude/accounts/second.json" --seed-claude-json "$SB/.claude-accts/d"
eq   "add dry-run 零落盘" "$(snapshot)" "$BEFORE"
xok  "add --apply 退出码 0" "$CLI" add x --from-credentials "$SB/.claude/accounts/second.json" --seed-claude-json "$SB/.claude-accts/d" --apply
chk  "x 的 config-dir 建好"     '[ -d "$SB/.claude-accts/x" ]'
eq   "x 的凭据来自旧快照" "$(cat "$SB/.claude-accts/x/.credentials.json")" '{"tok":"BBB-second"}'
eq   "x 凭据权限 600" "$(stat -c %a "$SB/.claude-accts/x/.credentials.json")" "600"
eq   "种下的 .claude.json 已剥掉 oauthAccount" "$(jq -r '.oauthAccount // "null"' "$SB/.claude-accts/x/.claude.json")" "null"
eq   "种下的 .claude.json 保留项目信任" "$(jq -r '.projects["/tmp/x"].hasTrustDialogAccepted' "$SB/.claude-accts/x/.claude.json")" "true"
chk  "x 的共享项是 symlink"     '[ -L "$SB/.claude-accts/x/skills" ]'
eq   "manifest 现在 2 个注册账号 + 账号 0" "$(jq -r '.accounts|length' "$M")" "3"
eq   "x 不是默认"            "$(jq -r '.accounts[1].isDefault' "$M")" "false"
xfail "重复 add 同名被拒" "$CLI" add x --apply

group "3. 隔离:两账号凭据互不覆盖(模拟并发各写各的)"
printf '{"tok":"AAA-refreshed"}' >"$SB/.claude-accts/d/.credentials.json"
printf '{"tok":"BBB-refreshed"}' >"$SB/.claude-accts/x/.credentials.json"
eq   "d 的凭据没被 x 覆盖" "$(cat "$SB/.claude-accts/d/.credentials.json")" '{"tok":"AAA-refreshed"}'
eq   "x 的凭据没被 d 覆盖" "$(cat "$SB/.claude-accts/x/.credentials.json")" '{"tok":"BBB-refreshed"}'
jq '.oauthAccount={"emailAddress":"second@example.com"}' "$SB/.claude-accts/x/.claude.json" >"$SB/t.json" && mv "$SB/t.json" "$SB/.claude-accts/x/.claude.json"
eq   "d 的身份仍是 default@" "$(jq -r .oauthAccount.emailAddress "$SB/.claude-accts/d/.claude.json")" "default@example.com"
eq   "x 的身份是 second@"    "$(jq -r .oauthAccount.emailAddress "$SB/.claude-accts/x/.claude.json")" "second@example.com"

group "4. 共享:一处改动,两个账号都看得见(双向)"
printf 'brand new\n' >"$SB/.claude/skills/new.md"
chk  "d 经 symlink 看到共享库新文件" '[ -f "$SB/.claude-accts/d/skills/new.md" ]'
chk  "x 经 symlink 看到共享库新文件" '[ -f "$SB/.claude-accts/x/skills/new.md" ]'
printf 'written by x\n' >"$SB/.claude-accts/x/skills/from-x.md"
chk  "x 写的落进共享库实体"   '[ -f "$SB/.claude/skills/from-x.md" ] && [ ! -L "$SB/.claude/skills/from-x.md" ]'
chk  "d 立刻看到 x 写的"      '[ -f "$SB/.claude-accts/d/skills/from-x.md" ]'
printf 'memory line\n' >>"$SB/.claude-accts/d/projects/proj-a/MEMORY.md"
chk  "projects(memory/history)也是活共享" '[ -f "$SB/.claude/projects/proj-a/MEMORY.md" ] && [ -f "$SB/.claude-accts/x/projects/proj-a/MEMORY.md" ]'

group "5. sync:补新增顶层项 / 修断链 / 清退休链 / 刷新邮箱 / 幂等"
mkdir -p "$SB/.claude/newdir"; printf 'x\n' >"$SB/.claude/newdir/a"
BEFORE="$(snapshot)"
ccq sync
eq   "sync dry-run 零落盘" "$(snapshot)" "$BEFORE"
ccq sync --apply
chk  "CC 升级新增的顶层项已补链到两个账号" \
     '[ -L "$SB/.claude-accts/d/newdir" ] && [ -L "$SB/.claude-accts/x/newdir" ]'
eq   "sync 顺手把 x 的邮箱刷进 manifest" "$(jq -r '.accounts[1].email' "$M")" "second@example.com"
rm -f "$SB/.claude-accts/x/skills"; ln -s "/nonexistent/skills" "$SB/.claude-accts/x/skills"
ccq sync --apply
eq   "断链/错链已修回共享库" "$(readlink "$SB/.claude-accts/x/skills")" "$SB/.claude/skills"
rm -rf "$SB/.claude/newdir"
ccq sync --apply
chkn "共享库里删掉的项,账号里的残链被清掉" '[ -L "$SB/.claude-accts/d/newdir" ]'
AFTER="$(snapshot)"
ccq sync --apply
eq   "sync 幂等(再跑一次无任何变化)" "$(snapshot)" "$AFTER"

group "6. verify:结构 + 隔离 + 共享(只读,不起 claude)"
cc verify >"$SB/verify.out" 2>&1
eq   "verify 退出码 0(PASS)" "$?" "0"
chk  "输出含 PASS"            'grep -q "结果:PASS" "$SB/verify.out"'
chk  "报告了账号 0 未登录"     'grep -q "账号 0 未登录" "$SB/verify.out"'
chk  "报告了各账号邮箱互不相同" 'grep -q "邮箱互不相同" "$SB/verify.out"'
chk  "活体探测证明共享生效"    'grep -q "个账号都能经各自 config-dir 看到" "$SB/verify.out"'
chkn "探测文件已清理"          'ls "$SB"/.claude/*/.cc-acct-iso-probe.* 2>/dev/null'
# 制造故障:把隔离项变成 symlink(串号) → 必须 FAIL
rm -f "$SB/.claude-accts/x/.credentials.json"
ln -s "$SB/.claude-accts/d/.credentials.json" "$SB/.claude-accts/x/.credentials.json"
cc verify >"$SB/verify2.out" 2>&1
eq   "隔离项被换成 symlink 时 verify 退出码 1" "$?" "1"
chk  "指出了串号风险" 'grep -q "竟是 symlink" "$SB/verify2.out"'
rm -f "$SB/.claude-accts/x/.credentials.json"; printf '{"tok":"BBB"}' >"$SB/.claude-accts/x/.credentials.json"
# 制造故障:共享项缺失 → 必须 FAIL
rm -f "$SB/.claude-accts/x/skills"
cc verify >"$SB/verify3.out" 2>&1
eq   "共享项缺失时 verify 退出码 1" "$?" "1"
chk  "提示跑 sync 补" 'grep -q "sync --apply" "$SB/verify3.out"'
ccq sync --apply

group "7. list / which / run / shellinit"
cc list >"$SB/list.out" 2>&1
chk  "list 列出两个账号"     'grep -qE "^\*?d +" "$SB/list.out" && grep -qE "^ x +" "$SB/list.out"'
chk  "list 现读邮箱"         'grep -q "second@example.com" "$SB/list.out"'
chk  "list 标出默认账号 *"   'grep -q "^\*d" "$SB/list.out"'
chkn "list 不泄露凭据"       'grep -qi "tok" "$SB/list.out"'
eq   "which <dir> 认出账号" "$(cd "$SB" && CLAUDE_CONFIG_DIR="$SB/.claude-accts/x" "$CLI" which)" "x"
eq   "which 默认账号带标注" "$("$CLI" which "$SB/.claude-accts/d")" "d (默认)"
xfail "which 未登记目录返回非 0" "$CLI" which "$SB/nope"
OUT="$("$CLI" --launcher fakeclaude run x -- --resume abc123 2>&1)"
eq   "run 注入了正确的 CLAUDE_CONFIG_DIR 并透传参数" "$OUT" "CFG=$SB/.claude-accts/x ARGS=--resume abc123"
cc shellinit >"$SB/sh.out" 2>&1
chk  "shellinit 导出默认账号"  'grep -q "export CLAUDE_CONFIG_DIR=" "$SB/sh.out"'
chk  "shellinit 生成 dcc/xcc" 'grep -q "^dcc()" "$SB/sh.out" && grep -q "^xcc()" "$SB/sh.out"'
chk  "shellinit 片段本身是合法 shell" 'bash -n "$SB/sh.out"'

group "8. rm:只删账号目录,不动共享库"
ccq rm x --apply
chkn "x 的 config-dir 已删"     '[ -e "$SB/.claude-accts/x" ]'
eq   "manifest 只剩 1 个注册账号 + 账号 0" "$(jq -r '.accounts|length' "$M")" "2"
chk  "共享库文件毫发无损"        '[ -f "$SB/.claude/skills/foo.md" ] && [ -f "$SB/.claude/skills/from-x.md" ] && [ -f "$SB/.claude/settings.json" ]'
xfail "默认账号不加 --force 删不掉" "$CLI" rm d --apply
chk  "默认账号仍在"              '[ -d "$SB/.claude-accts/d" ]'

group "9. rollback:一键回到迁移前"
new_sandbox
BEFORE="$(snapshot)"
ccq init d --apply
ccq add x --from-credentials "$SB/.claude/accounts/second.json" --apply
chk  "迁移+加号已生效" '[ -d "$SB/.claude-accts/x" ] && [ ! -e "$SB/.claude/.credentials.json" ]'
ccq rollback latest --apply           # 回退 add x
chkn "rollback 撤掉了 add x"   '[ -e "$SB/.claude-accts/x" ]'
chk  "d 仍在(只回退了最后一步)" '[ -d "$SB/.claude-accts/d" ]'
BK="$(ls -d "$SB"/.claude-accts/.backup-* | head -1)"
ccq rollback "${BK##*/.backup-}" --apply   # 回退 init
chk  "凭据已还原回共享库"      '[ -f "$SB/.claude/.credentials.json" ]'
eq   "还原的凭据内容正确" "$(cat "$SB/.claude/.credentials.json")" '{"tok":"AAA-default"}'
chk  "\$HOME/.claude.json 已还原" '[ -f "$SB/.claude.json" ]'
eq   "还原的 .claude.json 内容正确" "$(jq -r .numStartups "$SB/.claude.json")" "42"
chkn "账号目录 d 已删除"       '[ -e "$SB/.claude-accts/d" ]'
chkn "manifest 已删除"         '[ -e "$SB/.claude-accts/accounts.json" ]'
rm -rf "$SB/.claude-accts"
eq   "回退后文件树与迁移前一致" "$(snapshot)" "$BEFORE"

group "10. 安全 / 参数化 / 边界"
new_sandbox
xfail "拒绝含空格的账号名"            "$CLI" init 'bad name' --apply
xfail "拒绝路径穿越式账号名"          "$CLI" init '../evil' --apply
xfail "拒绝含斜杠的账号名"            "$CLI" init 'x/y' --apply
xfail "拒绝 ACCTS_DIR 位于 SHARED_STORE 内" "$CLI" --accts-dir "$SB/.claude/inside" init d --apply
xfail "拒绝不存在的共享库"            "$CLI" --shared-store "$SB/nonexistent" init d --apply
xfail "拒绝相对路径"                  "$CLI" --shared-store 'relative/path' init d --apply
xfail "未 init 时 add 被拒"           "$CLI" add x --apply
chkn  "以上失败都没落盘"              '[ -e "$SB/.claude-accts" ]'
xok   "正常 init"                     "$CLI" init d --apply
xfail "凭据源文件不存在时被拒"        "$CLI" add x --from-credentials "$SB/nope.json" --apply
chkn  "被拒后没留下半成品目录"        '[ -e "$SB/.claude-accts/x" ]'
# 参数化:换一套完全不同的路径也能跑
ALT="$SB/alt-home"; mkdir -p "$ALT/store/skills"; printf 'alt\n' >"$ALT/store/skills/a.md"
printf '{"tok":"ALT"}' >"$ALT/store/.credentials.json"
cat >"$SB/altconfig" <<EOF
SHARED_STORE="$ALT/store"
ACCTS_DIR="$ALT/accts"
LEGACY_HOME_DIR="$ALT"
LAUNCHER="fakeclaude"
EOF
xok  "自定义路径也能 init" env CC_ACCT_ISO_CONFIG="$SB/altconfig" "$CLI" init main --apply
chk  "自定义路径:凭据搬进自定义账号库" '[ -f "$ALT/accts/main/.credentials.json" ]'
chk  "自定义路径:共享项链回自定义共享库" '[ -L "$ALT/accts/main/skills" ]'
eq   "自定义路径:manifest 写的是自定义路径" "$(jq -r .sharedStore "$ALT/accts/accounts.json")" "$ALT/store"
chk  "默认路径那套不受影响"            '[ -d "$SB/.claude-accts/d" ]'
# 视觉欺骗类 Unicode(与 daemon is_deceptive_char 对齐):普通路径放行,欺骗字符拒绝
xok  "path_shell_safe 放行普通路径"    bash -c '. "'"$SCRIPTS_DIR"'/lib.sh"; path_shell_safe "/home/u/z"'
xok  "path_shell_safe 放行中文路径"    bash -c '. "'"$SCRIPTS_DIR"'/lib.sh"; path_shell_safe "/home/用户/z"'
xfail "path_shell_safe 拒绝零宽空格"   bash -c '. "'"$SCRIPTS_DIR"'/lib.sh"; path_shell_safe "/home/u/z$(printf '"'"'​'"'"')b"'
xfail "path_shell_safe 拒绝双向覆盖RLO" bash -c '. "'"$SCRIPTS_DIR"'/lib.sh"; path_shell_safe "/home/u/$(printf '"'"'‮'"'"')x"'
xfail "path_shell_safe 拒绝 NBSP"      bash -c '. "'"$SCRIPTS_DIR"'/lib.sh"; path_shell_safe "/home/u/z$(printf '"'"' '"'"')b"'

group "11. 无 jq 降级(纯 bash 读 manifest / 读邮箱)"
OUT="$(
  set +u
  # shellcheck source-path=SCRIPTDIR
  # shellcheck source=../lib.sh
  . "$SCRIPTS_DIR/lib.sh"
  have() { [ "$1" != jq ]; }          # 假装没有 jq
  SHARED_STORE="$SB/.claude"; ACCTS_DIR="$SB/.claude-accts"; MANIFEST="$ACCTS_DIR/accounts.json"
  ISOLATE_SET=".credentials.json .claude.json"
  manifest_load
  printf '%s|' "${#MF[@]}"
  IFS="$MF_SEP" read -r n e c d <<<"${MF[0]}"
  printf '%s|%s|%s|%s|' "$n" "$e" "$c" "$d"
  printf '%s' "$(claude_json_email "$SB/.claude-accts/d")"
)"
eq "无 jq 也能解析 manifest + 读邮箱" "$OUT" "1|d|default@example.com|$SB/.claude-accts/d|true|default@example.com"

group "12. --default-in-place(旧 V1 逃生口)"
new_sandbox
ccq init d --default-in-place --apply
eq   "in-place:configDir == 共享库" "$(jq -r '.accounts[0].configDir' "$SB/.claude-accts/accounts.json")" "$SB/.claude"
chk  "in-place:凭据原地不动"        '[ -f "$SB/.claude/.credentials.json" ]'
chk  "in-place:\$HOME/.claude.json 原地不动" '[ -f "$SB/.claude.json" ]'
chkn "in-place:没在共享库里乱建 symlink" '[ -L "$SB/.claude/skills" ]'
cc verify >"$SB/v.out" 2>&1
chk  "in-place:verify 跳过全迁检查而非误报" 'grep -q "跳过「共享库应无账号态」检查" "$SB/v.out"'

group "13. 审计回归:rollback 安全 + 韧性"
new_sandbox
ccq init d --apply
BKTS="$(cd "$SB/.claude-accts" && ls -d .backup-* | head -1)"; BKTS="${BKTS#.backup-}"
# 阻塞-1:时间戳参数不得能穿越出 ACCTS_DIR
mkdir -p "$SB/evil/root$SB/VICTIM"; printf 'keep\n' >"$SB/evil/root$SB/VICTIM/decoy"
printf 'RESTORE\t%s/VICTIM\n' "$SB" >"$SB/evil/undo.tsv"
mkdir -p "$SB/VICTIM"; printf 'precious\n' >"$SB/VICTIM/precious.txt"
xfail "rollback 拒绝穿越式时间戳" "$CLI" rollback "$BKTS/../../../../$(basename "$SB")/evil" --apply
chk  "受害目录毫发无损"        '[ -f "$SB/VICTIM/precious.txt" ]'
xfail "rollback 拒绝带斜杠的时间戳" "$CLI" rollback "../evil" --apply
xfail "rollback 拒绝含 .. 的时间戳" "$CLI" rollback "20260101-000000..x" --apply
# RESTORE 目标越界要被跳过而不是照做
printf 'RESTORE\t%s/VICTIM\n' "$SB" >>"$SB/.claude-accts/.backup-$BKTS/undo.tsv"
ccq rollback "$BKTS" --apply || true
chk  "越界的 RESTORE 条目被跳过,受害目录仍在" '[ -f "$SB/VICTIM/precious.txt" ]'
chk  "同一次 rollback 里合法条目仍然执行了(凭据已还原)" '[ -f "$SB/.claude/.credentials.json" ]'
# 韧性:一条失败不能挡住其余(尤其挡住"救数据"那条)
new_sandbox
ccq init d --apply
chmod 500 "$SB"                      # 让 $HOME/.claude.json 还原失败
ccq rollback latest --apply; RB_RC=$?
chmod 700 "$SB"
eq   "有条目失败时 rollback 返回非 0" "$([ "$RB_RC" != 0 ] && echo yes || echo no)" "yes"
chk  "但关键数据(凭据)仍被还原回共享库" '[ -f "$SB/.claude/.credentials.json" ]'
chkn "且当初新建的账号目录已清掉"      '[ -e "$SB/.claude-accts/d" ]'

group "14. 审计回归:断链不再让 sync 来回翻 / verify 恒 FAIL"
new_sandbox
ln -s "$SB/gone-forever" "$SB/.claude/local"     # 共享库里一条断链(CC 装过 local 后目标被清掉)
ccq init d --apply
chk  "断链项也被建了链"   '[ -L "$SB/.claude-accts/d/local" ]'
ccq sync --apply; ccq sync --apply; ccq sync --apply
N_BK="$(cd "$SB/.claude-accts" && ls -d .backup-* 2>/dev/null | wc -l)"
eq   "3 次 sync 后备份目录数没增长(=收敛,不再一删一建)" "$N_BK" "1"
chk  "链还在"            '[ -L "$SB/.claude-accts/d/local" ]'
cc verify >"$SB/v14.out" 2>&1; V14=$?
eq   "verify 不因源头断链而 FAIL" "$V14" "0"
chk  "但点名了是共享库源头断的" 'grep -q "源头就断了" "$SB/v14.out"'

group "15. 审计回归:verify 不再放绿灯"
new_sandbox
ccq init d --apply
xfail "verify 打错账号名要报错而不是 PASS" "$CLI" verify no-such-account
chmod 666 "$SB/.claude-accts/d/.credentials.json"
chmod 777 "$SB/.claude-accts/d"
cc verify --no-probe >"$SB/v15.out" 2>&1; V15=$?
eq   "凭据 666 / 目录 777 时 verify FAIL" "$V15" "1"
chk  "指出了凭据权限"     'grep -q "凭据权限 666" "$SB/v15.out"'
ccq sync --apply
eq   "sync 把凭据权限修回 600" "$(stat -c %a "$SB/.claude-accts/d/.credentials.json")" "600"
eq   "sync 把目录权限修回 700" "$(stat -c %a "$SB/.claude-accts/d")" "700"
xok  "修完 verify 恢复 PASS" "$CLI" verify --no-probe
# 空共享库不能判 PASS
new_sandbox
rm -rf "$SB/.claude/skills" "$SB/.claude/projects" "$SB/.claude/plugins" "$SB/.claude/sessions" \
       "$SB/.claude/settings.json" "$SB/.claude/CLAUDE.md" "$SB/.claude/history.jsonl" \
       "$SB/.claude/settings.json.bak" "$SB/.claude/settings.json.bak-before-x" "$SB/.claude/accounts"
ccq init d --apply
cc verify >"$SB/v15b.out" 2>&1; V15B=$?
eq   "共享项 0 个时 verify FAIL(而不是'0 个全部就位')" "$V15B" "1"
chk  "说清了根本没有共享" 'grep -q "共享项 0 个" "$SB/v15b.out"'
# 混合 in-place + V2 时,全局检查不能整体跳过
new_sandbox
ccq init ip --default-in-place --apply
ccq add v2 --apply
printf '{"tok":"leak"}' >"$SB/.claude/.credentials.json"
cc verify --no-probe >"$SB/v15c.out" 2>&1; V15C=$?
eq   "混合模式下共享库出现凭据仍要 FAIL" "$V15C" "1"

group "16. 审计回归:配置文件不能废掉 dry-run / 危险字符 / symlink 凭据"
new_sandbox
printf 'APPLY=1\n' >"$SB/badcfg"
CC_ACCT_ISO_CONFIG="$SB/badcfg" ccq init d || true
chkn "配置文件里写 APPLY=1 也不能绕过 dry-run" '[ -e "$SB/.claude-accts" ]'
xfail "ACCTS_DIR 含单引号被拒(会闭合消费端的 shell 引号)" "$CLI" --accts-dir "$SB/ac'ts" init d --apply
xfail "ACCTS_DIR 含 \$( ) 被拒" "$CLI" --accts-dir "$SB/a\$(id)b" init d --apply
xfail "--email 缺值时报错(而不是静默退出)" "$CLI" add x --email
ccq init d --apply
xfail "--email 含控制字符被拒" "$CLI" add c --email "$(printf 'a\001b')" --apply
chk  "manifest 仍是合法 JSON"  'jq -e . "$SB/.claude-accts/accounts.json"'
# symlink 形态的凭据 = 旧软链切号方案 → 迁移会串号,必须拒绝
new_sandbox
mv "$SB/.claude/.credentials.json" "$SB/.claude/accounts/real.json"
ln -s "$SB/.claude/accounts/real.json" "$SB/.claude/.credentials.json"
xfail "隔离项是 symlink 时 init 拒绝迁移" "$CLI" init d --apply
chk  "拒绝后什么都没建"        '[ ! -e "$SB/.claude-accts" ]'

group "17. 审计回归:权限 / 并发 / 契约字段 / --apply 位置"
new_sandbox
ccq init d --apply
eq   "备份目录 700(里面是凭据明文副本)" "$(stat -c %a "$(find "$SB/.claude-accts" -maxdepth 1 -name '.backup-*' | head -1)")" "700"
eq   "ACCTS_DIR 700" "$(stat -c %a "$SB/.claude-accts")" "700"
eq   "manifest 600" "$(stat -c %a "$SB/.claude-accts/accounts.json")" "600"
chkn "没有残留的 .tmp 中间文件" 'ls "$SB"/.claude-accts/*.tmp.* 2>/dev/null'
xok  "--apply 放在命令之前也认" "$CLI" --apply add pre
chk  "确实落盘了" '[ -d "$SB/.claude-accts/pre" ]'
# 并发 add:两个进程同时写 manifest,一个都不能丢
"$CLI" add p1 --from-credentials "$SB/.claude/accounts/second.json" --apply >/dev/null 2>&1 &
"$CLI" add p2 --from-credentials "$SB/.claude/accounts/second.json" --apply >/dev/null 2>&1 &
wait
eq   "并发 add 两个账号都在 manifest 里" "$(jq -r '[.accounts[].name] | sort | join(",")' "$SB/.claude-accts/accounts.json")" "0,d,p1,p2,pre"
# 契约字段
eq   "manifest 有 updatedAt" "$(jq -r 'has("updatedAt")' "$SB/.claude-accts/accounts.json")" "true"
eq   "每个账号有 mode 字段(账号 0 = bare)" "$(jq -r '[.accounts[].mode] | unique | join(",")' "$SB/.claude-accts/accounts.json")" "bare,isolated"
chk  "list --json 是合法 JSON" '"$CLI" list --json | jq -e . >/dev/null'
eq   "list --json 带登录态"    "$("$CLI" list --json | jq -r '.accounts[] | select(.name=="p1") | .loggedIn')" "true"
eq   "list --json 未登录的标 false" "$("$CLI" list --json | jq -r '.accounts[] | select(.name=="pre") | .loggedIn')" "false"
# 删掉默认号要自动补选,否则 shellinit 不再产出 export
ccq rm d --force --apply
eq   "删默认号后仍恰好有一个默认" "$(jq -r '[.accounts[] | select(.isDefault)] | length' "$SB/.claude-accts/accounts.json")" "1"
"$CLI" shellinit >"$SB/si17.out" 2>&1      # 不用管道:grep -q 提前退出会给上游 SIGPIPE,配 pipefail 会假失败
chk  "shellinit 仍产出 export" 'grep -q "^export CLAUDE_CONFIG_DIR=" "$SB/si17.out"'

group "18. 审计回归:SHARE_SET 白名单 + 探测双向"
new_sandbox
cat >"$SB/wl.cfg" <<EOF
SHARE_SET="skills settings.json"
EOF
CC_ACCT_ISO_CONFIG="$SB/wl.cfg" ccq init d --apply
chk  "白名单里的项建了链" '[ -L "$SB/.claude-accts/d/skills" ] && [ -L "$SB/.claude-accts/d/settings.json" ]'
chkn "白名单外的项没建链" '[ -e "$SB/.claude-accts/d/projects" ]'
new_sandbox
ccq init d --apply
ccq add x --apply
cc verify >"$SB/v18.out" 2>&1
chk  "探测验证了账号→共享库方向" 'grep -q "写的文件出现在共享库实体里" "$SB/v18.out"'
chk  "探测验证了共享库→各账号方向" 'grep -q "个账号都能经各自 config-dir 看到" "$SB/v18.out"'
chkn "探测文件已清理(共享库侧)" 'ls "$SB"/.claude/*/.cc-acct-iso-probe.* 2>/dev/null'


# ══════════════════════════════════════════════════════════════════
group "15. Z08:隔离项私有化（isolate / sync 不再删文件 / add 认隔离集）"
# 这一组钉的是一个**真实的数据丢失回归**:把某项加进 ISOLATE_SET 再 sync --apply,
# 此前 sync 会把每个账号的软链**直接删掉**(实测两个账号的 settings.json 都不存在了、
# 设置静默回落 Claude Code 默认值)。正确处置是私有化。
new_sandbox
ccq init z --apply
ccq add b --apply
ISO_PLUS=".credentials.json .claude.json backups policy-limits.json stats-cache.json settings.json"

chk  "前置:settings.json 初始是共享软链(z)" '[ -L "$SB/.claude-accts/z/settings.json" ]'
chk  "前置:settings.json 初始是共享软链(b)" '[ -L "$SB/.claude-accts/b/settings.json" ]'

# —— sync:隔离项若还是软链 ⇒ 私有化,**不是删** ——
ISOLATE_SET="$ISO_PLUS" ccq sync --apply
chk  "sync 后 z/settings.json 存在(不再被删掉)"  '[ -e "$SB/.claude-accts/z/settings.json" ]'
chk  "sync 后 b/settings.json 存在(不再被删掉)"  '[ -e "$SB/.claude-accts/b/settings.json" ]'
chkn "sync 后 z/settings.json 不再是软链"        '[ -L "$SB/.claude-accts/z/settings.json" ]'
chkn "sync 后 b/settings.json 不再是软链"        '[ -L "$SB/.claude-accts/b/settings.json" ]'
chk  "私有化后内容与共享库逐字节相同(z)" 'cmp -s "$SB/.claude/settings.json" "$SB/.claude-accts/z/settings.json"'
chk  "共享库那份**保留**作新账号模板(绝不 MOVE)" '[ -f "$SB/.claude/settings.json" ]'
chk  "两个账号各自独立(改 z 不影响 b)" 'printf "{\"theme\":\"zzz\"}\n" >"$SB/.claude-accts/z/settings.json"; ! cmp -s "$SB/.claude-accts/z/settings.json" "$SB/.claude-accts/b/settings.json"'
chkn "改 z 也不回写共享库(软链已摘掉,不会反向覆盖)" 'grep -q zzz "$SB/.claude/settings.json"'

# —— sync 幂等:再跑一次不该有任何改动 ——
OUT15="$(ISOLATE_SET="$ISO_PLUS" cc sync 2>&1)"
chk  "sync 私有化后幂等(第二次无改动)" 'printf "%s" "$OUT15" | grep -q "无需改动"'

# —— add 认隔离集:新账号也要拿到该私有项 ——
ISOLATE_SET="$ISO_PLUS" ccq add c --apply
chk  "add 后新账号 c 拿到 settings.json"      '[ -e "$SB/.claude-accts/c/settings.json" ]'
chkn "add 后 c/settings.json 不是软链"        '[ -L "$SB/.claude-accts/c/settings.json" ]'
chk  "c 的内容取自共享库模板"                  'cmp -s "$SB/.claude/settings.json" "$SB/.claude-accts/c/settings.json"'
chkn "add **不**从别处复制身份(.credentials.json 不种)" '[ -e "$SB/.claude-accts/c/.credentials.json" ]'

# —— 定向 isolate 命令 ——
new_sandbox
ccq init z --apply
ccq add b --apply
xfail "isolate 缺项名要报错"                     "$CLI" isolate --apply
xfail "isolate 拒绝含 / 的项名"                  "$CLI" isolate a/b --apply
xfail "isolate 拒绝共享库里没有的项"             "$CLI" isolate no-such-thing --apply
OUT16="$(cc isolate settings.json 2>&1)"
chk  "isolate 默认是 dry-run(不落盘)"            '[ -L "$SB/.claude-accts/z/settings.json" ]'
chk  "isolate dry-run 报出私有化计划"            'printf "%s" "$OUT16" | grep -q "私有化"'
chk  "isolate 对不在 ISOLATE_SET 里的项要 warn"  'printf "%s" "$OUT16" | grep -q "下次 sync"'
ccq isolate settings.json --apply
chkn "isolate --apply 后 z 不再是软链"           '[ -L "$SB/.claude-accts/z/settings.json" ]'
chkn "isolate --apply 后 b 不再是软链"           '[ -L "$SB/.claude-accts/b/settings.json" ]'
chk  "isolate 幂等(已私有的再跑不报错)"          "$CLI" isolate settings.json --apply

# —— 回滚:私有化是可撤的 ——
new_sandbox
ccq init z --apply
ccq add b --apply
ISOLATE_SET="$ISO_PLUS" ccq sync --apply
chkn "回滚前:z 是私有实体" '[ -L "$SB/.claude-accts/z/settings.json" ]'
ccq rollback latest --apply
chk  "rollback 后 z/settings.json 回到软链" '[ -L "$SB/.claude-accts/z/settings.json" ]'


# ══════════════════════════════════════════════════════════════════
group "16. Z06:原生身份组成的单点声明（NATIVE_IDENTITY 派生）"
# 这一组钉的是「三个集合从声明派生、且派生结果与历史字面量逐字相同」。
# 派生对了但**行为变了**才是真危险,所以既比字面量、也比行为。
# shellcheck source=/dev/null
. "$SCRIPTS_DIR/lib.sh"

eq   "ni_isolate_default 逐字等于历史 ISOLATE_SET" \
     "$(ni_isolate_default)" ".credentials.json .claude.json backups policy-limits.json stats-cache.json"
eq   "ni_home_rooted 逐字等于历史 LEGACY_HOME_ITEMS" "$(ni_home_rooted)" ".claude.json"
eq   "ni_secrets = 两个身份本体" "$(ni_secrets)" ".credentials.json .claude.json"
chk  "ni_is_secret 认 .credentials.json"      'ni_is_secret .credentials.json'
chk  "ni_is_secret 认 .claude.json"           'ni_is_secret .claude.json'
chkn "ni_is_secret 不认 backups(derived)"     'ni_is_secret backups'
chkn "ni_is_secret 不认 stats-cache.json(state)" 'ni_is_secret stats-cache.json'
chkn "ni_is_secret 不认表外的项"               'ni_is_secret settings.json'
eq   "声明有 5 项" "$(ni_items | grep -c .)" "5"

# ★ set -euo pipefail 下三个投影都必须 rc=0。
# 栽过一次:while 里用 `cond && printf`,最后一行条件为假 ⇒ while 以 1 退出 ⇒ pipefail
# 判整条管线失败 ⇒ set -e 就地退出(派生值全对,却在赋值之后死掉,197 条红了 115 条)。
chk  "ni_isolate_default 在 pipefail 下 rc=0" 'bash -euo pipefail -c ". \"$SCRIPTS_DIR/lib.sh\"; ni_isolate_default >/dev/null"'
chk  "ni_home_rooted 在 pipefail 下 rc=0"     'bash -euo pipefail -c ". \"$SCRIPTS_DIR/lib.sh\"; ni_home_rooted >/dev/null"'
chk  "ni_secrets 在 pipefail 下 rc=0"         'bash -euo pipefail -c ". \"$SCRIPTS_DIR/lib.sh\"; ni_secrets >/dev/null"'

# —— 覆盖仍然生效(派生只改**默认值**的来源,不改覆盖语义)——
new_sandbox
OUT16A="$(ISOLATE_SET="a.json b.json" cc config 2>&1)"
chk  "ISOLATE_SET 环境变量仍能覆盖派生默认值" 'printf "%s" "$OUT16A" | grep -q "ISOLATE_SET       a.json b.json"'
OUT16B="$(cc config 2>&1)"
chk  "不覆盖时 config 打印的就是派生值" 'printf "%s" "$OUT16B" | grep -q "ISOLATE_SET       .credentials.json .claude.json backups policy-limits.json stats-cache.json"'

# —— secret 的权限:init 就该 600,sync 幂等不再产生一次性漂移 ——
new_sandbox
ccq init z --apply
eq   "init 后 .credentials.json 是 600" "$(stat -c %a "$SB/.claude-accts/z/.credentials.json" 2>/dev/null)" "600"
eq   "init 后 .claude.json 也是 600(Z06 从声明派生后补上的)" "$(stat -c %a "$SB/.claude-accts/z/.claude.json" 2>/dev/null)" "600"
chmod 644 "$SB/.claude-accts/z/.claude.json"
ccq sync --apply
eq   "sync 会修 .claude.json 的权限(此前只修 .credentials.json)" "$(stat -c %a "$SB/.claude-accts/z/.claude.json" 2>/dev/null)" "600"
OUT16C="$(cc sync 2>&1)"
chk  "权限已对时 sync 无改动(收敛)" 'printf "%s" "$OUT16C" | grep -q "无需改动"'


# ══════════════════════════════════════════════════════════════════
group "17. Z07:版本钉 + 声明漂移检测（销 E37）"
# E37 的实质:CLAUDE_CONFIG_DIR 零官方文档而整套隔离压在它上面,失效时**今天没有任何
# 东西会响**,最坏形态是**静默共用身份**。这一组钉住「会响」。
new_sandbox
ccq init z --apply
ccq add b --apply

# —— D1b(致命):secret 出现在共享库 ⇒ 会被自动 symlink 给每个账号 = 静默串号 ——
# 此前只查 .credentials.json;从声明派生后 .claude.json 也覆盖。**这条零误报**。
printf '{"oauthAccount":{"x":1}}' >"$SB/.claude/.claude.json"
cc verify --no-probe >"$SB/v17a.out" 2>&1 || true
chk  "共享库出现 .claude.json ⇒ verify FAIL" 'grep -q "结果:FAIL" "$SB/v17a.out"'
chk  "报的是 .claude.json 不在任何账号的原生位置" 'grep -q "共享库里有 .claude.json" "$SB/v17a.out" && grep -q "不是任何账号的原生位置" "$SB/v17a.out"'
rm -f "$SB/.claude/.claude.json"
xok  "移除后 verify 恢复 PASS" "$CLI" verify --no-probe

# —— D2(提示):共享库出现声明之外的 mode 600 文件 ⇒ 它会被自动 symlink 给每个账号 ——
printf 'sekrit' >"$SB/.claude/mystery-token.json"; chmod 600 "$SB/.claude/mystery-token.json"
# 必须先 sync:共享库多了一项而账号还没链上,会触发**既有**的「共享项缺失」检查而 FAIL,
# 那跟 Z07 无关。sync 之后才是「已正常共享、但它是个 600 的未声明项」这个真场景。
ccq sync --apply
cc verify --no-probe >"$SB/v17b.out" 2>&1 || true
chk  "600 的未声明共享项被点名"     'grep -q "mystery-token.json" "$SB/v17b.out"'
chk  "并给出处置建议(isolate)"       'grep -q "isolate mystery-token.json" "$SB/v17b.out"'
chk  "但只是提示、不判 FAIL"          'grep -q "结果:PASS" "$SB/v17b.out"'
chmod 644 "$SB/.claude/mystery-token.json"
cc verify --no-probe >"$SB/v17c.out" 2>&1 || true
chkn "同名文件改成 644 后不再点名(判据是 600 不是名字)" 'grep -q "mystery-token.json" "$SB/v17c.out"'
rm -f "$SB/.claude/mystery-token.json"
ccq sync --apply   # 清掉刚才那项留下的软链,免得影响后面

# —— D4(提示):声明项哪儿都找不到 ⇒ 只提示,**不判致命** ——
# 理由:policy-limits.json / stats-cache.json 是 Claude Code **懒创建**的,
# 「还没被创建」与「改了位置」分辨不出来。判致命会让几乎每台干净机器都红。
cc verify --no-probe >"$SB/v17d.out" 2>&1 || true
chk  "懒创建的声明项缺席只报提示"     'grep -q "可能只是\*\*还没被创建\*\*" "$SB/v17d.out" || grep -q "还没被创建" "$SB/v17d.out"'
chk  "缺席不影响 PASS"                'grep -q "结果:PASS" "$SB/v17d.out"'

# —— D3:版本钉 ——
# **沙盒里造一个确定性的版本来源**,否则会读到真机的 .last-update-result.json(测试泄漏)。
new_sandbox
# **必须在沙盒里遮蔽 `claude`**:ni_probe_version 的第一条来源是解析 launcher 可执行文件的
# 路径,而沙盒 PATH 不遮蔽的话会找到**真机**那个 `~/.local/bin/claude`(它软链到
# `.../versions/2.1.220`)⇒ 测试读到真机版本、结果不确定。本轮实测踩到这个泄漏。
printf '#!/bin/sh\nexit 0\n' >"$SB/bin/claude"; chmod +x "$SB/bin/claude"
printf '{"version_from":"9.9.1","version_to":null}\n' >"$SB/.claude/.last-update-result.json"
ccq init z --apply
chk  "sync/init 会把探测到的版本钉进 manifest" 'grep -q "\"claudeVersionPinned\": \"9.9.1\"" "$SB/.claude-accts/accounts.json"'
cc verify --no-probe >"$SB/v17e0.out" 2>&1 || true
chk  "版本一致时 verify 明说一致" 'grep -q "版本与声明所钉一致(9.9.1)" "$SB/v17e0.out"'
# 版本变了 ⇒ 必须要求人复核声明
printf '{"version_from":"9.9.1","version_to":"9.9.2"}\n' >"$SB/.claude/.last-update-result.json"
cc verify --no-probe >"$SB/v17e.out" 2>&1 || true
chk  "版本变化 ⇒ 要求复核 NATIVE_IDENTITY" 'grep -q "请复核 NATIVE_IDENTITY" "$SB/v17e.out"'
chk  "并打出新旧两个版本"                   'grep -q "从 9.9.1 变成了 9.9.2" "$SB/v17e.out"'
chk  "版本变化只是提示、不判 FAIL"          'grep -q "结果:PASS" "$SB/v17e.out"'
# 探不到版本时要明说跳过,**不能假装通过**
rm -f "$SB/.claude/.last-update-result.json"
cc verify --no-probe >"$SB/v17f.out" 2>&1 || true
chk  "探不到版本时明说跳过" 'grep -q "探测不到 Claude Code 版本" "$SB/v17f.out"'

# —— 版本探测本身:只读、绝不执行 claude ——
chk  "ni_probe_version 不执行 launcher(把 launcher 换成必失败的也不影响退出码)" \
     'LAUNCHER=definitely-no-such-binary bash -euo pipefail -c ". \"$SCRIPTS_DIR/lib.sh\"; SHARED_STORE=\"$SB/.claude\"; ni_probe_version >/dev/null"'


group "19. Z01:账号 0 —— 「不设 CLAUDE_CONFIG_DIR」这个状态本身"
new_sandbox
ccq init d --apply
ccq add x --apply
M="$SB/.claude-accts/accounts.json"
# init --apply 把凭据**搬进**了账号 d ⇒ 此刻共享库是空的。显式造一份,模拟
# 「全迁之后有人没设 CLAUDE_CONFIG_DIR 又起了一次 claude」—— 账号 0 就是这么诞生的。
printf '{"tok":"ZERO"}' >"$SB/.claude/.credentials.json"; chmod 600 "$SB/.claude/.credentials.json"

# —— 恒在列,且**在数组末尾**(放首位会让所有按下标取账号的地方整体错位) ——
eq   "manifest 里账号 0 恒在列"   "$(jq -r '[.accounts[].name] | index("0") != null' "$M")" "true"
eq   "账号 0 在数组**末尾**"      "$(jq -r '.accounts[-1].name' "$M")" "0"
eq   "既有账号下标未被扰动"       "$(jq -r '.accounts[0].name' "$M")" "d"
eq   "账号 0 的 mode 是 bare"     "$(jq -r '.accounts[-1].mode' "$M")" "bare"
eq   "账号 0 不是默认账号"        "$(jq -r '.accounts[-1].isDefault' "$M")" "false"

# ★ 全局最要紧的一条:configDir 这个**键必须整个缺席**。
#   写成 "" 会让 run 那行 `env CLAUDE_CONFIG_DIR="$cfgdir"` 设出一个**空值**,
#   而空值 ≠ 未设 —— Claude Code 会拿空串当路径。写成 null 同理(消费侧 unwrap_or_default)。
eq   "账号 0 **没有** configDir 键" "$(jq -r '.accounts[-1] | has("configDir")' "$M")" "false"
chkn "manifest 里根本不出现 configDir 的空串" 'grep -q "\"configDir\": \"\"" "$M"'

# —— 读回:它是写时合成的,不许再被当成注册项载进 MF ——
cc list --json >"$SB/z01-l.json" 2>"$SB/z01-l.err"
eq   "读回 manifest 不产生 warn"  "$(grep -c "configDir 非法" "$SB/z01-l.err" || true)" "0"
eq   "list --json 里账号 0 只有一份" "$(jq -r '[.accounts[] | select(.name=="0")] | length' "$SB/z01-l.json")" "1"
eq   "list --json 账号 0 也在末尾"   "$(jq -r '.accounts[-1].name' "$SB/z01-l.json")" "0"
eq   "list --json 账号 0 也无 configDir" "$(jq -r '.accounts[-1] | has("configDir")' "$SB/z01-l.json")" "false"
# 幂等:再 sync 一次,账号 0 不会被复制成第二份、也不会消失
ccq sync --apply
eq   "sync 后账号 0 仍只有一份"   "$(jq -r '[.accounts[] | select(.name=="0")] | length' "$M")" "1"
eq   "sync 后注册账号数不变"      "$(jq -r '[.accounts[] | select(.name!="0")] | length' "$M")" "2"

# —— 登录态:共享库有 cfg 根的 secret = 账号 0 已登录 ——
eq   "共享库有凭据 ⇒ 账号 0 已登录" "$(jq -r '.accounts[-1].loggedIn' "$SB/z01-l.json")" "true"
mv "$SB/.claude/.credentials.json" "$SB/cred.bak"
cc list --json >"$SB/z01-l2.json" 2>&1
eq   "凭据搬走 ⇒ 账号 0 未登录"     "$(jq -r '.accounts[-1].loggedIn' "$SB/z01-l2.json")" "false"
eq   "未登录也仍在列(它是状态,不是记录)" "$(jq -r '.accounts[-1].name' "$SB/z01-l2.json")" "0"
mv "$SB/cred.bak" "$SB/.claude/.credentials.json"

# —— verify 改判:从「违规」变「状态」 ——
cc verify --no-probe >"$SB/z01-v.out" 2>&1 || true
chk  "verify 报账号 0 已登录"      'grep -q "账号 0 已登录" "$SB/z01-v.out"'
chk  "而且不判 FAIL"               'grep -q "结果:PASS" "$SB/z01-v.out"'
chkn "不再说共享库凭据是违规"      'grep -q "共享库里仍有 secret 项 .credentials.json" "$SB/z01-v.out"'
mv "$SB/.claude/.credentials.json" "$SB/cred.bak"
cc verify --no-probe >"$SB/z01-v2.out" 2>&1 || true
chk  "无凭据 ⇒ 报账号 0 未登录"    'grep -q "账号 0 未登录" "$SB/z01-v2.out"'
mv "$SB/cred.bak" "$SB/.claude/.credentials.json"

# ★ root 字段才是判据:.claude.json 的原生根是 $HOME ⇒ 共享库那份不是任何账号的原生位置。
#   (这条同时钉住:D1b 的改判**只放行 cfg 根的 secret**,不是把整条检查废掉。)
printf '{"oauthAccount":{"emailAddress":"ghost@example.com"}}' >"$SB/.claude/.claude.json"
cc verify --no-probe >"$SB/z01-v3.out" 2>&1 || true
chk  "home 根的 secret 在共享库 ⇒ 仍 FAIL" 'grep -q "结果:FAIL" "$SB/z01-v3.out"'
chk  "且措辞不再提「静默串号」(那是 Z07 的事实错误)" \
     'grep -q "不是任何账号的原生位置" "$SB/z01-v3.out" && ! grep -q "共享库里有 .claude.json.*静默串号" "$SB/z01-v3.out"'
rm -f "$SB/.claude/.claude.json"

# —— 保留名 / run / which / shellinit ——
xfail "add 0 被拒(保留名)"        "$CLI" add 0 --apply
xfail "rm 0 被拒"                 "$CLI" rm 0 --force --apply
# **别写成 `"$CLI" … | grep -q`**:套件开了 pipefail,左侧 die 非零 ⇒ 整条管线判失败,
# grep 命中了也报红(本轮实测栽过)。一律落文件再 grep。
"$CLI" add 0 --apply >"$SB/z01-add0.out" 2>&1 || true
chk   "拒绝理由说清了它是保留名"  'grep -q "保留名" "$SB/z01-add0.out"'
"$CLI" rm 0 --force --apply >"$SB/z01-rm0.out" 2>&1 || true
chk   "rm 0 的理由不是「没有这个账号」" 'grep -q "不是注册项" "$SB/z01-rm0.out"'

# ★ run 0 = **什么都不设**。用 env -u 而不是 =""(否则 fakeclaude 会打出 CFG= 而不是 CFG=<unset>)。
OUT="$(CLAUDE_CONFIG_DIR="$SB/.claude-accts/x" "$CLI" --launcher fakeclaude run 0 -- --foo 2>&1 | tail -1)"
eq   "run 0 把继承来的 CLAUDE_CONFIG_DIR **摘掉**" "$OUT" "CFG=<unset> ARGS=--foo"
eq   "which 在未设时报出账号 0" "$(cd "$SB" && env -u CLAUDE_CONFIG_DIR "$CLI" which 2>/dev/null | head -1)" "0"
xok  "which 未设时 rc=0(它是正常账号,不是错误态)" env -u CLAUDE_CONFIG_DIR "$CLI" which
cc shellinit >"$SB/z01-sh.out" 2>&1
chk  "shellinit 给出回到账号 0 的逃生口" 'grep -q "^0cc()" "$SB/z01-sh.out"'
chk  "逃生口用 env -u 而不是空串"        'grep -q "0cc() { env -u CLAUDE_CONFIG_DIR" "$SB/z01-sh.out"'
chkn "逃生口里绝不出现 CLAUDE_CONFIG_DIR=\"\"" 'grep -q "CLAUDE_CONFIG_DIR=\\x27\\x27\|CLAUDE_CONFIG_DIR=\"\"" "$SB/z01-sh.out"'
chk  "片段仍是合法 shell"                'bash -n "$SB/z01-sh.out"'
# 谓词必须**只给退出码**:初版 printf 'false' 把字符串漏进了 which/run 的 stdout。
eq   "acct_zero_logged 不往 stdout 吐东西" \
     "$(bash -euo pipefail -c ". \"$SCRIPTS_DIR/lib.sh\"; SHARED_STORE=\"$SB/.claude\"; acct_zero_logged && printf DONE")" "DONE"
# 三个 Z01 helper 在 set -euo pipefail 下都必须 rc=0(Z06 那个 while|pipe 的坑的同款防线)
xok  "acct_zero_json 在 pipefail 下 rc=0" \
     bash -euo pipefail -c ". \"$SCRIPTS_DIR/lib.sh\"; SHARED_STORE=\"$SB/.claude\"; LEGACY_HOME_DIR=\"$SB\"; acct_zero_json 1 >/dev/null"
xok  "ni_is_home_rooted 在 pipefail 下 rc=0" \
     bash -euo pipefail -c ". \"$SCRIPTS_DIR/lib.sh\"; ni_is_home_rooted .credentials.json || true"
# home 根的 secret 不算账号 0 登录(判据与 verify 同源:声明的 root 字段)
eq   "共享库只有 .claude.json 时账号 0 仍算未登录" \
     "$(bash -euo pipefail -c ". \"$SCRIPTS_DIR/lib.sh\"; SHARED_STORE=\"$SB/zz\"; mkdir -p \"$SB/zz\"; : >\"$SB/zz/.claude.json\"; acct_zero_logged_json")" "false"

group "20. Z04:守卫 —— 分裂实况可见 / 共享库删不掉 / 显式指共享库不是账号 0"
new_sandbox
ccq init d --default-in-place --apply
M="$SB/.claude-accts/accounts.json"

# —— ① rm 不得连带删掉共享库(is_under 守卫;此前零断言) ——
"$CLI" rm d --force --apply >"$SB/z04-rm.out" 2>&1 || true
chk  "rm in-place 账号被拒"          'grep -q "拒绝删除 ACCTS_DIR 之外的 config-dir" "$SB/z04-rm.out"'
chk  "★ 共享库整个还在"              '[ -d "$SB/.claude" ] && [ -f "$SB/.claude/.credentials.json" ] && [ -f "$SB/.claude/skills/foo.md" ]'
chk  "账号仍在 manifest 里(没被半删)" '[ "$(jq -r "[.accounts[] | select(.name==\"d\")] | length" "$M")" = 1 ]'
# ★ 共享库其实有**两道**独立守卫,变异验收时实测到的:把 cmd_rm 里那道 is_under 拿掉后,
#   计划执行器里 `RM)` 那道**照样**挡住了(共享库完好)。所以「共享库还在」这条断言在那次变异下
#   **没有变红,而那是正确的** —— 不是弱绿。两道都要钉住,而第二道从 CLI 走不到
#  (sync 只会 plan RM 账号目录内的东西)⇒ 用扫源码的方式钉它存在。
chk  "守卫层 1:cmd_rm 里有 is_under 前置" \
     'grep -q "is_under \"\$cfgdir\" \"\$ACCTS_DIR\" || die" "$SCRIPTS_DIR/cc-acct-iso"'
chk  "守卫层 2:计划执行器 RM 分支自己也查一遍(纵深防御)" \
     'grep -A 1 "^    RM)" "$SCRIPTS_DIR/lib.sh" | grep -q "is_under \"\$a\" \"\$ACCTS_DIR\" || die"'
chk  "反向自检:两条 grep 真读到了文件(不是空转)" \
     '[ "$(wc -c <"$SCRIPTS_DIR/cc-acct-iso")" -gt 1000 ] && [ "$(wc -c <"$SCRIPTS_DIR/lib.sh")" -gt 1000 ]'
# 账号 0 走的是另一条拒绝路径(Z01),这里补验它同样不动共享库
"$CLI" rm 0 --force --apply >"$SB/z04-rm0.out" 2>&1 || true
chk  "rm 0 之后共享库也毫发无损"      '[ -f "$SB/.claude/.credentials.json" ]'

# —— ② in-place:**没**分裂时说「还没分裂」,别喊狼来了 ——
# 沙盒的 .credentials.json 是默认 umask 造的(664)⇒ verify 有一条**与 Z04 无关**的既有致命
#(「凭据权限必须 600」)。不修它,下面「只提示不判 FAIL」验的就不是 Z04 这条判据。
chmod 600 "$SB/.claude/.credentials.json"
rm -f "$SB/.claude/.claude.json"
cc verify --no-probe >"$SB/z04-v1.out" 2>&1 || true
chk  "未分裂 ⇒ 措辞是「现在还没分裂」" 'grep -q "现在\*\*还没\*\*分裂" "$SB/z04-v1.out"'
chkn "未分裂 ⇒ 不喊「已经真的分裂」"   'grep -q "已经真的分裂" "$SB/z04-v1.out"'

# —— ③ ★ 真分裂:两份 .claude.json 同时存在。这条此前**完全不可见** ——
#    (上面「隔离项 .claude.json 是本账号私有实体」还会把共享库那份报成绿灯)
printf '{"oauthAccount":{"emailAddress":"split@example.com"},"numStartups":1}' >"$SB/.claude/.claude.json"
cc verify --no-probe >"$SB/z04-v2.out" 2>&1 || true
chk  "真分裂被点名"        'grep -q "已经真的分裂" "$SB/z04-v2.out"'
chk  "两个路径都打出来"    'grep -q "$SB/.claude/.claude.json" "$SB/z04-v2.out" && grep -q "$SB/.claude.json" "$SB/z04-v2.out"'
chk  "说清了哪条路读哪份"  'grep -q "设了 CLAUDE_CONFIG_DIR 起 claude 读前者" "$SB/z04-v2.out"'
chk  "只提示不判 FAIL(in-place 是逃生口,不给在野环境突然一个红)" 'grep -q "结果:PASS" "$SB/z04-v2.out"'
# 反向自检:那条「私有实体」绿灯仍在 ⇒ 证明新检查是**补了一条腿**,不是把旧的换掉了
chk  "旧的「私有实体」绿灯仍在(新检查是补腿不是替换)" 'grep -q "隔离项 .claude.json 是本账号私有实体" "$SB/z04-v2.out"'

# —— ④ which:显式把 CLAUDE_CONFIG_DIR 指到共享库 ——
CLAUDE_CONFIG_DIR="$SB/.claude" "$CLI" which >"$SB/z04-w1.out" 2>&1 || true
chk  "已登记 in-place:仍报出账号名"   'grep -q "^d (默认)" "$SB/z04-w1.out"'
chk  "但明说它不是账号 0"             'grep -q "不是账号 0" "$SB/z04-w1.out"'
# 全 isolated 的库里手工指共享库 = 最危险那种(谁都不是)
new_sandbox
ccq init d --apply
ccq add x --apply
CLAUDE_CONFIG_DIR="$SB/.claude" "$CLI" which >"$SB/z04-w2.out" 2>&1 || true
chk  "未登记地指向共享库 ⇒ 明说这不是账号 0"  'grep -q "这不是账号 0" "$SB/z04-w2.out"'
chk  "并给出正确起法"                        'grep -q "什么都不设" "$SB/z04-w2.out"'
xfail "未登记地指向共享库 ⇒ rc 非 0" env CLAUDE_CONFIG_DIR="$SB/.claude" "$CLI" which

# —— ⑤ run in-place 账号:代价说在前面,但**不禁**(逃生口要保持可用) ——
new_sandbox
ccq init d --default-in-place --apply
OUT="$("$CLI" --launcher fakeclaude run d 2>&1)"
chk  "run in-place 仍然起得来(不禁)" 'printf "%s" "$OUT" | grep -q "CFG=$SB/.claude"'
chk  "但先把两份状态的代价说了"      'printf "%s" "$OUT" | grep -q "是\*\*两份\*\*"'

# —— ⑥ ★ 反向自检:这三条守卫**不许误伤账号 0** ——
#    (Z07 对 D1b 犯的就是这类错:一条对合法状态恒红的检查)
OUT0="$(CLAUDE_CONFIG_DIR="$SB/.claude" "$CLI" --launcher fakeclaude run 0 2>&1 | tail -2)"
chk  "run 0 不冒出 in-place 警告"    'printf "%s" "$OUT0" | grep -qv "in-place(V1) 账号:" || true; ! printf "%s" "$OUT0" | grep -q "是\*\*两份\*\*"'
chk  "run 0 仍然把 CLAUDE_CONFIG_DIR 摘掉" 'printf "%s" "$OUT0" | grep -q "CFG=<unset>"'
env -u CLAUDE_CONFIG_DIR "$CLI" which >"$SB/z04-w0.out" 2>&1 || true
chk  "which(未设)报账号 0，且不冒出「这不是账号 0」" \
     'grep -q "账号 0" "$SB/z04-w0.out" && ! grep -q "这不是账号 0" "$SB/z04-w0.out"'
# ★ 这条断言初版写成「纯 in-place 库里 verify 仍单独报账号 0 已登录」——**写错了**:
#   in-place 账号与账号 0 读的是**同一份** .credentials.json(共享库那份),分开报会让人
#   以为是两个登录。正确的事实是「一个登录身份、两套状态文件」,由分裂那条说。
chmod 600 "$SB/.claude/.credentials.json"
printf '{"y":2}' >"$SB/.claude/.claude.json"
cc verify --no-probe >"$SB/z04-v0.out" 2>&1 || true
chk  "分裂告警说清了「凭据却是同一份」" 'grep -q "凭据却是同一份" "$SB/z04-v0.out"'
chk  "并点名它与账号 0 是同一个登录"     'grep -q "与账号 0 在这个库里是同一个登录" "$SB/z04-v0.out"'


# ══════════════════════════════════════════════════════════════════
export HOME="$ORIG_HOME"
printf '\n\033[1m────────────────────────────\033[0m\n'
# 断言条数地板（**同源**:地板写在套件自己这一处,CI 侧那条是双保险）。
# 为什么必须有:下面的退出码只看失败数 $F —— **$F=0 就 exit 0** ⇒ 一条不跑也会报绿。
# 改这个数的时机:真加了断言(只应涨)。删断言要说明理由。
MIN_ASSERTS=294
if [ "$T" -lt "$MIN_ASSERTS" ]; then
  printf '\033[31m断言条数缩水:%d < 地板 %d —— 有断言被删或整组没跑\033[0m\n' "$T" "$MIN_ASSERTS"
  exit 1
fi
if [ "$F" -eq 0 ]; then
  printf '\033[32m全绿:%d/%d 断言通过\033[0m\n' "$T" "$T"; exit 0
else
  printf '\033[31m失败:%d/%d 断言未通过\033[0m\n' "$F" "$T"; exit 1
fi
