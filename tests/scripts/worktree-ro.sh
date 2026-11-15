#!/usr/bin/env bash
# 判据：测试跑的时候不往仓工作树里写（夹具 · 探针落系统临时目录）。
#
# 仓里全仓判据共用的那份文件清单收没跟踪、也没被忽略的文件；测试往仓里写了又删，干净克隆里另一条判据就会列进它、
# 读时它已没了而转红。本脚本用 bwrap 把仓工作树只读挂载，在 strace 底下跑壳与后端两棵的 `cargo test --workspace --lib`
# 和整套 vitest：凡是往仓里建 · 写 · 改名的系统调用都撞只读（写了又删也算，测试自己吞了错误也算），
# strace 记下的那几个路径就是证据；因此而红的测试另行点名。只判这三样跑的测试（集成测试 · doctest · e2e 不在内）。
#
# 可写的只有两类，都从仓里派生、不手写：
#   - 跟踪着的 `.gitignore` 挡着的、此刻已在的路径（不认本机 `info/exclude`）：`.build/` · `node_modules/` 这类。
#     工作树里有没跟踪或改过没提交的 `.gitignore` ⇒ 判不了（它会把本该只读的地方放成可写）；
#     挡着、但此刻还不在的路径照旧只读（会误报红，不会误报绿）；
#   - `generated/` 目录下跟踪着的每一份文件，逐份可写（生成物测试原地重写它们）；那几个目录本身照旧只读，新建一份照样红。
# 判的是跑测试的时候：两棵树先在只读挂载外面编好（构建脚本照常写 `.gitignore` 挡着的地方），再只读着跑。
#
# 用法：在仓里任一处 `bash tests/scripts/worktree-ro.sh [shell] [backend] [vitest]`，判的是当前目录所在的那棵树；不带段 ⇒ 三段都跑。
# 退出：0 没有往仓里写的 · 1 有 · 2 判不了（没有 bwrap / strace / 起不来 / 只读没生效 / 自检没认出）
#       · 3 没有往仓里写的、却有别的红（那几条红在半路，后面写没写看不到）
set -uo pipefail

cant() { printf '判不了：%s\n' "$1"; exit 2; }

root=$(git rev-parse --show-toplevel 2>/dev/null) || cant "当前目录不在一个 git 仓里"
root=$(cd "$root" && pwd -P)
cd "$root" || cant "进不了 $root"
segs=("$@")
[ ${#segs[@]} -eq 0 ] && segs=(shell backend vitest)
for s in "${segs[@]}"; do
  case "$s" in shell|backend|vitest) ;; *) cant "段只有 shell · backend · vitest，收到「$s」" ;; esac
done
for t in bwrap strace python3; do
  command -v "$t" >/dev/null 2>&1 || cant "这台机器上没有 $t"
done
dirty=$(git status --porcelain -- ':(glob)**/.gitignore')
[ -z "$dirty" ] || cant "工作树里有没跟踪或改过没提交的 .gitignore：$dirty"

out=$root/.build/worktree-ro
mkdir -p "$out"
scrub() { bash "$root/tests/scripts/session-scrub.sh" "$@"; }
tree_of() { case "$1" in shell) echo src/frontend/shell ;; backend) echo src/backend ;; esac; }
for s in "${segs[@]}"; do
  case "$s" in
    vitest)
      if [ ! -x node_modules/.bin/vitest ]; then
        npm ci --prefer-offline --no-audit --no-fund >"$out/npm-ci.log" 2>&1 || cant "npm ci 失败（$out/npm-ci.log）"
      fi ;;
    *)
      (cd "$(tree_of "$s")" && CARGO_TARGET_DIR="$out/target-$s" scrub cargo test --workspace --lib --no-run) >"$out/$s-build.log" 2>&1 ||
        cant "$s 那棵编不过（$out/$s-build.log）" ;;
  esac
done

# bwrap 的参数从 3 号描述符读（NUL 分隔）：生成物逐份挂成可写，份数上千。
args=$out/bwrap-args
{
  printf '%s\0' --dev-bind / / --ro-bind "$root" "$root"
  [ -d "$root/.git" ] && printf '%s\0' --bind "$root/.git" "$root/.git"
  git ls-files -z --others --ignored --exclude-per-directory=.gitignore --directory |
    while IFS= read -r -d '' p; do
      printf '%s\0' --bind "$root/${p%/}" "$root/${p%/}"
    done
  git ls-files -z | while IFS= read -r -d '' p; do
    case "/$p" in */generated/*) [ -f "$p" ] && printf '%s\0' --bind "$root/$p" "$root/$p" ;; esac
  done
  printf '%s\0' --chdir "$root"
} >"$args"

# 只读挂载里、strace 底下起一条命令（先摘掉会话继承来的变量，同 quick-check 每一步）。
# strace 只记失败的文件类系统调用；第一个参数是记到哪一份。
ro() {
  local trace=$1; shift
  scrub bwrap --args 3 strace -f -qq --seccomp-bpf -e trace=%file -e status=failed -o "$trace" "$@" 3<"$args" </dev/null
}

# 一份 strace 记录里撞了只读的仓内路径（相对路径照写出来、标上）。
denied() {
  python3 -I - "$root" "$1" <<'PY'
import re, sys
root, path = sys.argv[1], sys.argv[2]
seen = []
for line in open(path, encoding="utf-8", errors="replace"):
    if not re.search(r"= -1 (EROFS|EXDEV|EBUSY)\b", line):
        continue
    for p in re.findall(r'"((?:[^"\\]|\\.)*)"', line):
        if p.startswith(root + "/"):
            p = p[len(root) + 1:]
        elif p.startswith("/"):
            continue
        else:
            p += "（相对路径）"
        if p not in seen:
            seen.append(p)
for p in seen:
    print(p)
PY
}

# 撞了只读的那几个路径分两类：跟踪着的 `.gitignore` 挡着的（共用清单不收它们，不算写仓）· 别的（写仓）。
# 拿一个空的 git 目录配本树当工作树去问，只读工作树里的 `.gitignore`，不读本机 `info/exclude` 与全局忽略。
bare=$out/ignore-git
[ -d "$bare" ] || git init -q --bare "$bare" || cant "建不了问忽略用的空 git 目录"
split_denied() {
  local p ign
  while IFS= read -r p; do
    [ -n "$p" ] || continue
    case "$p" in *"（相对路径）") printf '写仓\t%s\n' "$p"; continue ;; esac
    # 带一个尾斜杠再问一次：`__pycache__/` 这类只挡目录的规则，问一个此刻不在的路径时只认带斜杠的那一形。
    ign=$(git --git-dir="$bare" --work-tree="$root" -c core.excludesFile=/dev/null check-ignore --no-index -- "$p" "$p/" >/dev/null && echo y)
    if [ "$ign" = y ]; then printf '挡着\t%s\n' "$p"; else printf '写仓\t%s\n' "$p"; fi
  done
}

# 一份测试输出里因只读而红的测试（cargo 的失败块 · vitest 的失败块都认），另列别的红。
classify() {
  python3 -I - "$1" <<'PY'
import re, sys
text = re.sub(r"\x1b\[[0-9;]*m", "", open(sys.argv[1], encoding="utf-8", errors="replace").read())
ro = re.compile(r"Read-only file system|ReadOnlyFilesystem|EROFS|EXDEV|CrossesDevices|cross-device|Device or resource busy|EBUSY")
for m in re.finditer(r"^---- (\S+) stdout ----\n(.*?)(?=^---- \S+ stdout ----|^failures:|\Z)", text, re.S | re.M):
    print(("写仓" if ro.search(m.group(2)) else "别的红") + "\t" + m.group(1))
# vitest：` FAIL  [项目] 文件 > 套件 > 测试`（项目名无色时写成 `|node|`，有色时是一块底色字），到下一块或分隔线为止。
for m in re.finditer(r"^ FAIL +(?:\S+ +)?(\S+\.[cm]?[jt]sx? > .+?)\n(.*?)(?=^ FAIL +|^⎯{10,}|\Z)", text, re.S | re.M):
    print(("写仓" if ro.search(m.group(2)) else "别的红") + "\t" + m.group(1).strip())
PY
}

# 自检一：只读生效、`.build/` 照样可写。
canary=$root/.worktree-ro-canary
rm -f -- "$canary"
probe=$(ro "$out/self-1.strace" bash -c 'touch "$1" 2>&1; echo "rc=$?"; : >"$2/canary" && echo build-ok' _ "$canary" "$out" 2>&1) ||
  cant "bwrap 或 strace 起不来（多半是这台机器不许用户命名空间或 ptrace）：$probe"
case "$probe" in *"Read-only file system"*"rc=1"*build-ok*) ;; *) cant "只读没生效或 .build 不可写：$probe" ;; esac
[ -e "$canary" ] && cant "金丝雀落进了仓里（$canary）—— 只读没生效"

# 自检二：一条吞了错误的「写了又删」（不红），strace 那一路要认出它写的路径。
ro "$out/self-2.strace" python3 -I -c '
import os, sys
try:
    os.mkdir(sys.argv[1]); os.rmdir(sys.argv[1])
except OSError:
    pass
' "$root/.worktree-ro-sample" >/dev/null 2>&1
denied "$out/self-2.strace" | grep -qx '.worktree-ro-sample' || cant "自检没从 strace 记录里认出那条写了又删（$out/self-2.strace）"

# 自检三：两种测试输出的失败块，认法都认得出因只读而红的那一条。
sample=$out/self-3.txt
printf '%s\n' '---- a::cargo_shape stdout ----' '建探针目录: Os { code: 30, kind: ReadOnlyFilesystem }' '' 'failures:' \
  ' FAIL  |node| tests/x.vitest.ts > 套件 > vitest 那一形' "Error: EROFS: read-only file system, mkdir '/x'" '⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯' >"$sample"
got=$(classify "$sample")
case "$got" in *"写仓	a::cargo_shape"*"写仓	tests/x.vitest.ts > 套件 > vitest 那一形"*) ;; *) cant "自检没认出两种失败块（$sample）：$got" ;; esac

write=0 otherred=0
for s in "${segs[@]}"; do
  log=$out/$s.log trace=$out/$s.strace
  case "$s" in
    shell | backend) ro "$trace" bash -c 'cd "$1" && CARGO_TARGET_DIR="$2" cargo test --workspace --lib --no-fail-fast' _ "$(tree_of "$s")" "$out/target-$s" >"$log" 2>&1 ;;
    vitest) ro "$trace" node_modules/.bin/vitest run >"$log" 2>&1 ;;
  esac
  seg_rc=$?
  kinds=$(denied "$trace" | split_denied)
  paths=$(printf '%s\n' "$kinds" | awk -F '\t' '$1=="写仓"{print $2}')
  shielded=$(printf '%s\n' "$kinds" | awk -F '\t' '$1=="挡着"{print $2}')
  found=$(classify "$log")
  red=$(printf '%s\n' "$found" | awk -F '\t' '$1=="写仓"{print $2}')
  other=$(printf '%s\n' "$found" | awk -F '\t' '$1=="别的红"{print $2}')
  if [ -n "$paths" ]; then
    printf '  红   %-8s 往仓里写了（strace 记录 %s）：\n' "$s" "$trace"
    printf '%s\n' "$paths" | sed 's/^/         /'
    if [ -n "$red" ]; then
      printf '         因只读而红的测试：\n'
      printf '%s\n' "$red" | sed 's/^/           /'
    else
      printf '         没有测试因此而红：写的那一条把错误吞了，按上面的路径找\n'
    fi
    write=1
  elif [ -n "$red" ] && [ -z "$shielded" ]; then
    printf '  红   %-8s 有测试报只读而红，strace 却没记到仓内路径（整份输出 %s）：\n' "$s" "$log"
    printf '%s\n' "$red" | sed 's/^/         /'
    write=1
  elif [ -n "$red" ]; then
    printf '  ??   %-8s 有测试报只读而红，撞的只是 .gitignore 挡着、此刻还不在的路径（不算写仓，但挡住了判断）：\n' "$s"
    printf '%s\n' "$shielded" | sed 's/^/         /'
    printf '%s\n' "$red" | sed 's/^/           /'
    otherred=1
  fi
  if [ -z "$paths" ] && [ -z "$red" ] && [ "$seg_rc" -ne 0 ]; then
    printf '  ??   %-8s 有别的红（不是只读引起的；整份输出 %s）：\n' "$s" "$log"
    if [ -n "$other" ]; then printf '%s\n' "$other" | sed 's/^/         /'; else printf '         （没认出是哪一条，看整份输出）\n'; fi
    otherred=1
  elif [ "$seg_rc" -eq 0 ] && [ -z "$paths" ]; then
    printf '  ok   %-8s 没有往仓里写的\n' "$s"
  fi
done

[ $write -eq 1 ] && exit 1
[ $otherred -eq 1 ] && exit 3
exit 0
