#!/usr/bin/env bash
# `K-R82` `KR82D1`：**`hooks/` 下会被执行的脚本，跑不跑得起来这件事有人守。**
#
# ## 它治的是什么
#
# `K-R80` 的转置读数逐字：**`hooks/` · `evidence/` · 仓根文件三棵树，12 格里 0 格覆盖**。
# 而 `hooks/` 与另两棵**性质不同** —— `hooks/pre-commit` 是**会被 git 执行**的东西，
# 跑在**每一次提交**上，**能改仓**。它坏了不是读数错，是**提交路坏了**。
#
# ★ **两种坏法的形状不一样，现打过，别凭印象写**（`git version 2.43.0`，沙箱
#   `ccmon-devbox:latest`，合成仓 + `core.hooksPath hooks`，读数落在
#   `evidence/K-R82-hooks-gate.md` `§1`）：
#     · **没有可执行位** ⇒ git **忽略这个 hook 并照常提交**：`rc=0`，提交真的进去了，
#       只在 stderr 上留一句 `hint: The 'hooks/pre-commit' hook was ignored because
#       it's not set as executable.` —— 而那句 hint **`git config advice.ignoredHook false`
#       就能关掉**，且它是提示不是失败。⇒ **闸门整个不在了，而退出码看起来一切正常。**
#       ⚠ 上一版本文件的头注这里写的是「一个字都不印」—— **那句是错的**，现打推翻，已订正。
#     · **有可执行位但语法坏** ⇒ 解释器报错、`rc=1`，提交**被挡住**。这一形是 fail-closed 的，
#       但它把**每一次提交**都变成一次报错 ⇒ 提交路坏了，同样得有东西红。
# ⇒ 「hook 在盘上」与「hook 真的会跑」是两句话。
#
# ## 🔴 两句话分开判 —— 这是本格的承重设计，不是啰嗦
#
# 本仓 `core.filemode=false`（现打：`git config core.filemode` ⇒ `false`）
# ⇒ **`chmod +x` 进不了 git**（记忆条 `filemode-false-chmod-invisible`）。
# 于是同一份 hook 会长出**两个互不相干的事实**：
#   · **盘上跑不跑得起来** —— `test -x`。它跟着**这一棵工作树的 checkout** 走。
#   · **库里记没记** —— index 里的 mode（`100755` / `100644`）。它跟着**提交**走。
# 两者能长期不一致而没人发现：`K-R82` 落地前盘上现打就是这一形 ——
# 主树 `cc-monitor` 里 `hooks/pre-commit` 是 `-rwxrwxr-x`（能跑），
# 而 `git ls-files -s hooks/` 是 **`100644`**（没记）⇒ **任何一棵新开的工作树 checkout 出来都是 644**，
# 那份 hook 在那些树里**静默地不跑**。本文件把这两句各判一条，红的时候也分开说。
#
# ## 🔴 失效方向（件计划 `KR82D1` 逐字点名）：**只判「文件在不在」**
#
# 那和「它跑得起来」是两件事。⇒ 本文件一条 `test -e` 都没有：三条判据分别是
# **盘上可执行** · **库里记着可执行位** · **语法过得了它自己声明的解释器**。
#
# ## 口径（fail-closed，别静默放行）
#
# **`hooks/` 下 git 跟踪的每一个文件，都当成「会被 git 执行的东西」。**
# 往里放一份不该被执行的文件（`README.md` 之类）⇒ 本格会红 ——
# 那时**回来改这条口径**，不许在这里加一条「跳过非脚本」的静默豁免：
# 「跳过」与「查过了」在输出上一模一样，那正是本区最贵的那族病。
# 同理：`hooks/` 不存在、或一个跟踪文件都没有 ⇒ **红**（空真挡在门外）。
#
# ## 阳性对照（`K-R79` / `K-R81`：自检别写成地板）
#
# 三把尺子**各自带正反两条对照**（`S1`–`S8`，跑在 `mktemp -d` 里的合成夹具上，
# 不碰工作树）：坏的必须被逮到（挡「尺子瞎了」）、好的必须放行（挡「尺子恒红」）。
# 对照与真判据**走同一个函数**，不是另写一份 —— 两份必漂。
#
# ## 跑法
#
#     bash scripts/hooks-are-runnable.sh [<仓根>]
#
# 不给参数就用本文件的上一级目录。给参数是为了**对着变异过的副本跑**（死值验）：
# 那个副本得是个真 git 仓（`index` 那条判据要 `git ls-files -s`），造法见
# `evidence/K-R82-hooks-gate.md`。

set -uo pipefail

ROOT="${1:-$(cd "$(dirname "$0")/.." && pwd)}"

fails=()
passed=0
ok()   { passed=$((passed + 1)); }
bad()  { fails+=("$1"); }

# ── 三把尺子。真判据与阳性对照**都只走这三个函数**。────────────────────────────

# 尺子① 盘上跑不跑得起来。⚠ 不是 `test -e` —— 那正是件计划点名的失效方向。
chk_disk_exec() { [ -x "$1" ]; }

# 尺子② 库里记没记。入参是 `git ls-files -s` 那一列 mode 字符串。
# ⚠ 只认 `100755`：`100644` 是「没记」，`120000`（软链）/ `160000`（子模块）在这里同样判不了 ⇒ 红。
chk_index_exec() { [ "$1" = "100755" ]; }

# 尺子③ 语法过不过。**解释器从 shebang 里读，闭集之外一律红**（不许静默跳过）。
#   回声：把认出来的解释器打到标准输出，调用方拿去印进活体信号。
chk_syntax() {
  local f="$1" line1 interp
  # ⚠ 先 `-r` 再读：不先判可读，`<"$f"` 失败时**是 shell 自己**把一行 `No such file`
  #   吐到 stderr（`2>/dev/null` 挡的是 `read`，挡不住重定向本身）—— 那行会混进本格的输出面。
  [ -r "$f" ] || { echo "<盘上读不到>"; return 1; }
  IFS= read -r line1 <"$f" 2>/dev/null || { echo "<空文件>"; return 1; }
  case "$line1" in
    '#!'*) ;;
    *) echo "<没有 shebang>"; return 1 ;;
  esac
  # `#!/usr/bin/env sh` 与 `#!/bin/bash -e` 两形都要认：取 env 之后那个词，或路径的 basename。
  interp="$(printf '%s' "${line1#\#!}" | awk '{ for (i = 1; i <= NF; i++) {
      n = $i; sub(/^.*\//, "", n);
      if (n == "env") continue;
      if (n ~ /^-/) continue;
      print n; exit } }')"
  case "$interp" in
    sh|bash|dash|zsh|ksh)
      command -v "$interp" >/dev/null 2>&1 || { echo "$interp<不在 PATH 上>"; return 1; }
      "$interp" -n "$f" >/dev/null 2>&1 || { echo "$interp"; return 1; } ;;
    python|python3)
      command -v python3 >/dev/null 2>&1 || { echo "$interp<不在 PATH 上>"; return 1; }
      python3 -c 'import ast,sys; ast.parse(open(sys.argv[1], encoding="utf-8").read())' "$f" \
        >/dev/null 2>&1 || { echo "$interp"; return 1; } ;;
    *)
      echo "${interp:-<空>}<不在闭集里>"; return 1 ;;
  esac
  echo "$interp"
}

# ── 阳性对照：三把尺子各正反一条（`S1`–`S8`）──────────────────────────────────
selftest() {
  local d out
  d="$(mktemp -d)" || { bad "S0 造不出临时目录 —— 阳性对照跑不了，本格判不了（不许当成绿）"; return; }
  # S1/S2 语法尺子
  printf '#!/usr/bin/env sh\nif [ 1 = 1 ]; then echo hi\n' >"$d/broken"   # 缺 fi
  printf '#!/usr/bin/env sh\necho hi\n'                    >"$d/good"
  printf '#!/usr/bin/env nosuchinterp7\necho hi\n'         >"$d/alien"
  printf 'echo hi\n'                                       >"$d/bare"
  chmod +x "$d/broken" "$d/good" "$d/alien" "$d/bare"
  out="$(chk_syntax "$d/broken")" \
    && bad "S1 语法尺子对**坏语法**放行了（回声 $out）—— 这把尺子瞎了，本格的绿不算数" || ok
  out="$(chk_syntax "$d/good")" \
    && ok || bad "S2 语法尺子对**好语法**也红（回声 $out）—— 恒红的尺子和没有尺子一样，会被人关掉"
  out="$(chk_syntax "$d/alien")" \
    && bad "S7 语法尺子对**闭集外的解释器**放行了（回声 $out）—— 静默跳过 = 没查" || ok
  out="$(chk_syntax "$d/bare")" \
    && bad "S8 语法尺子对**没有 shebang** 的文件放行了（回声 $out）—— 那种文件 git 跑它靠猜" || ok
  # S3/S4 盘上尺子
  chmod -x "$d/good"
  chk_disk_exec "$d/good" && bad "S3 盘上那把尺子对**没有 +x** 的文件放行了 —— git 会当它不存在，而本格看不见" || ok
  chmod +x "$d/good"
  chk_disk_exec "$d/good" && ok || bad "S4 盘上那把尺子对**有 +x** 的文件也红 —— 恒红"
  # S5/S6 库里那把尺子（纯字符串判定，不需要真 git）
  chk_index_exec 100644 && bad "S5 库里那把尺子把 100644 当成「记着了」—— 那正是 core.filemode=false 下的病灶形状" || ok
  chk_index_exec 100755 && ok || bad "S6 库里那把尺子对 100755 也红 —— 恒红"
  rm -rf -- "$d"
}
selftest

# ── 真判据 ──────────────────────────────────────────────────────────────────
if [ ! -d "$ROOT/hooks" ]; then
  bad "hooks/ 在 $ROOT 下不存在 —— 本格的分母是空的，判不了（不许当成绿）"
else
  entries="$(git -C "$ROOT" ls-files -s -z -- hooks/ 2>/dev/null | tr '\0' '\n')"
  n_files=0
  while IFS= read -r ent; do
    [ -n "$ent" ] || continue
    mode="${ent%% *}"
    path="${ent#*$'\t'}"
    n_files=$((n_files + 1))
    f="$ROOT/$path"
    # ① 盘上
    if chk_disk_exec "$f"; then ok; disk="可执行"
    else bad "$path **盘上没有可执行位** —— 现打（git 2.43.0）：git **忽略这个 hook 并照常提交**（rc=0，只在 stderr 留一句 advice hint，而那句 hint 用 advice.ignoredHook 就能关掉）⇒ 这一棵工作树里那道挡等于不在。修：chmod +x $path"; disk="不可执行"; fi
    # ② 库里
    if chk_index_exec "$mode"; then ok
    else bad "$path **库里记的是 $mode，不是 100755** —— 本仓 core.filemode=false，chmod 进不了 git ⇒ 从这次提交 checkout 出来的每一棵新工作树，这个 hook 都是 644、都不跑。修：git update-index --chmod=+x $path 之后只提交暂存区"; fi
    # ③ 语法
    if interp="$(chk_syntax "$f")"; then ok
    else bad "$path **语法过不了它自己声明的解释器**（$interp）—— 会被执行的东西，语法坏了必须红"; fi
    printf '  · %-24s 盘上 %s · 库里 %s · 解释器 %s · md5 %s\n' \
      "$path" "$disk" "$mode" "$interp" "$(md5sum "$f" 2>/dev/null | cut -c1-8)"
  done <<<"$entries"
  if [ "$n_files" -eq 0 ]; then
    bad "hooks/ 下一个被跟踪的文件都没有 —— 分母是空的，三条判据全成空真（不许当成绿）"
  fi
fi

if [ "${#fails[@]}" -ne 0 ]; then
  for f in "${fails[@]}"; do printf '✗ %s\n' "$f"; done
  printf 'hooks: FAIL=%s（通过 %s 条）\n' "${#fails[@]}" "$passed"
  exit 1
fi
printf 'hooks: %s passed\n' "$passed"
