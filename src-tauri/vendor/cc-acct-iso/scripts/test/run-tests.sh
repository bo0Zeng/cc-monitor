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
printf 'CFG=%s ARGS=%s\n' "${CLAUDE_CONFIG_DIR:-<unset>}" "$*"
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
eq   "账号数"        "$(jq -r '.accounts|length' "$M")" "1"
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
eq   "manifest 现在 2 个账号" "$(jq -r '.accounts|length' "$M")" "2"
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
chk  "报告了共享库无凭据"      'grep -q "纯共享库" "$SB/verify.out"'
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
eq   "manifest 只剩 1 个账号"    "$(jq -r '.accounts|length' "$M")" "1"
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
eq   "并发 add 两个账号都在 manifest 里" "$(jq -r '[.accounts[].name] | sort | join(",")' "$SB/.claude-accts/accounts.json")" "d,p1,p2,pre"
# 契约字段
eq   "manifest 有 updatedAt" "$(jq -r 'has("updatedAt")' "$SB/.claude-accts/accounts.json")" "true"
eq   "每个账号有 mode 字段"   "$(jq -r '[.accounts[].mode] | unique | join(",")' "$SB/.claude-accts/accounts.json")" "isolated"
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
export HOME="$ORIG_HOME"
printf '\n\033[1m────────────────────────────\033[0m\n'
if [ "$F" -eq 0 ]; then
  printf '\033[32m全绿:%d/%d 断言通过\033[0m\n' "$T" "$T"; exit 0
else
  printf '\033[31m失败:%d/%d 断言未通过\033[0m\n' "$F" "$T"; exit 1
fi
