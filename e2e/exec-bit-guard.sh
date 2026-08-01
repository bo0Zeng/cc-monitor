#!/bin/bash
# 结构性守卫：带 shebang 的脚本在 **git 里**必须记成 100755。
#
# 为什么需要这条：本仓 `core.fileMode=false`（`git config core.fileMode`），
# git **忽略文件系统的可执行位**——所以文件在盘上是 755、`git add` 之后仍被记成 100644，
# 而且本地怎么跑都正常（本地用的是盘上那份），**只有干净 checkout 才会暴露**。
# 已经踩过两次：
#   ① R00：`shared/ccm` 记成 644 → CI 上 `-bash: …/shared/ccm: Permission denied`，
#      e2e 连红三轮才靠 pane dump 定位到；
#   ② B01：13 个 cc-bus 脚本 `git add` 后全部是 100644。
# 靠"记得 git update-index --chmod=+x"是纪律，这条脚本把它变成门禁。
#
# **本脚本自身的失效模式**（B01 审计实测复现，逐条修在下面）：沉默即通过。
# 早期版本既不校验 `git ls-files` 的退出码、也不校验受检文件数，于是
# `shared/` 一旦改名/移动，或 git 在 CI 容器里因任何原因失败（如 dubious ownership），
# 它就**恒绿并且理直气壮地打印"全部 100755"**——那才是"守卫变摆设"的真实入口，
# 而不是当初担心的 `core.fileMode`（那条已实测排除：`ls-files -s` 报的是 index 里
# 已记录的模式，不受 fileMode 影响；CI 全新 clone 的 index 由 tree 生成，正是要断言的东西）。
set -o pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
cd "$REPO" || exit 1

# `e2e/` 下**约 20 个**带 shebang 的 .sh 是 100644 且**故意的**——它们一律经 `bash "$X"`
# 调用（见 e2e/resume-suite.sh:52、e2e/graylight-suite.sh:63），不依赖自身可执行位。
# 所以这里**不能**把 glob 放宽到 `e2e/`（会误伤那 20 个），而是显式白名单：
# 下面两个是**被直接执行**的，与 shared/** 属同一失效类（退回 644 → 本地看不出、CI 才炸）。
ALLOWLIST=(
  e2e/fake-claude          # 作为 launcher 路径喂进 tmux 命令串与 daemonPath，被直接 exec
  e2e/daemon-wrapper.sh    # 作为 daemonPath 由 app 直接执行
)

# E67②（2026-07-31）：**vendored `cc-acct-iso` 是第三个作用域**，而且是被这条守卫漏掉、
# 让 CI 连红两个版本的那一个。病灶一模一样：vendor 进来时丢了可执行位（三个文件全是
# 100644），而它自带的测试脚本用 `"$CLI" init d` **直接执行**副本 ⇒ rc=126「权限不够」，
# 294 条断言里 184 条连锁失败。**盘上/远端都看不出来**：`core.fileMode=false` 让本地照跑，
# 而 `acct_iso_deploy.rs` 上传时显式给 0o755（见该文件 :198/:207/:214）——所以真正被咬的
# 只有「从干净 checkout 直接执行副本」这一条路，也就是 CI。
#
# **为什么这里必须是显式白名单，不能沿用 shared/ 那套「有 shebang 就该 755」**：
# 这是 **vendored 副本**，`VENDOR.md` 的铁律是「副本是上游的镜子，不是分身」。
# 上游 `~/.claude/skills/cc-acct-iso/scripts/` 实测：`cc-acct-iso` 755 ·
# `cc-acct-iso-install.sh` 755 · `test/run-tests.sh` 755 · **`lib.sh` 644**（它是被
# `source` 的库，虽然带 shebang）。照 shebang 一刀切会把 `lib.sh` 也逼成 755 ——
# 那就是在副本里改出自己的版本了。所以只列上游确实是 755 的那三个。
#
# 顺带说明**为什么改模式不算违反镜子铁律**：`.vendor_id` 是**内容** sha256（VENDOR.md §8），
# 恢复可执行位不动内容、指纹不变；丢可执行位本身是 vendor 那一步的失手，补回去是**向上游看齐**。
VENDOR_ALLOWLIST=(
  src-tauri/vendor/cc-acct-iso/scripts/cc-acct-iso
  src-tauri/vendor/cc-acct-iso/scripts/cc-acct-iso-install.sh
  src-tauri/vendor/cc-acct-iso/scripts/test/run-tests.sh
)

fail=0
checked=0
# **按作用域分开计数**：只统计总数不够——白名单那 2 个恒在，会把"shared/ 整个消失"
# 这一变异盖成绿的（实测：`git rm -r --cached shared` 后总数仍为 2 → 旧自检不触发）。
# 每个作用域必须各自证明"我确实查到了东西"。
checked_shared=0
checked_vendor=0

# 取 git **index** 里记录的模式（不是盘上的）。
git_mode() { git ls-files -s -- "$1" | awk '{print $1}'; }

check_one() {
  local f="$1" mode
  mode="$(git_mode "$f")"
  # symlink（120000）：`git update-index --chmod=+x` 对它无意义，报也没有可行修法 → 跳过并说明。
  # （审计 S1：目前 shared/ 无 symlink，属潜伏；宁可显式跳过，也不给出一条无效的修复建议。）
  if [ "$mode" = "120000" ]; then
    printf 'SKIP | %s 是符号链接（git 120000），可执行位由目标决定\n' "$f"
    return 0
  fi
  checked=$((checked+1))
  if [ "$mode" != "100755" ]; then
    printf 'FAIL | %s 带 shebang 但 git 里是 %s（应为 100755）\n' "$f" "$mode"
    printf '       修法: git update-index --chmod=+x %s\n' "$f"
    fail=$((fail+1))
  else
    printf 'PASS | %s = 100755\n' "$f"
  fi
}

# 用 -z：`git ls-files` 默认 `core.quotePath=true`，**非 ASCII 文件名会被输出成 C 转义**
# （如 "shared/probe/\350\204\232..."），于是 `head -c 2 "$f"` 找不到文件而被静默跳过。
# 本仓工作目录本身就叫「文档/」、文档全中文——这不是理论风险（审计 I2 实测复现）。
# 同时把 git 的退出码落地：早期版本用进程替换 `< <(git ls-files …)`，git 失败也拿不到 rc。
list="$(mktemp)" || exit 1
trap 'rm -f "$list"' EXIT
if ! git ls-files -z -- shared/ > "$list"; then
  echo "守卫自检失败：git ls-files 执行失败（rc≠0）——不能把它当成'没有文件要查'"
  exit 1
fi

while IFS= read -r -d '' f; do
  # 判据取自 **index**（`:path`）而非工作树：两者不一致时（sparse-checkout、
  # 文件被删但仍 tracked）用工作树判会静默失效，而断言本来就是对 index 下的（审计 S3）。
  # 只看有 shebang 的：examples/*.service、SKILL.md、*.md 这些不该可执行。
  # 判 shebang 走**内容比较**而非管道退出码：`head -c 2` 读够就退出，会给
  # `git cat-file` 一个 SIGPIPE（rc=141），叠加上面的 `set -o pipefail` 就会被
  # `|| continue` 当成"没有 shebang"**静默跳过**——正是本脚本要消灭的那类沉默失效。
  # 今天 shared/ 下文件都小于管道缓冲区所以碰不到，但不能把正确性寄托在文件大小上。
  first2="$(git cat-file blob ":$f" 2>/dev/null | head -c 2)" || true
  [ "$first2" = '#!' ] || continue
  check_one "$f"
  checked_shared=$((checked_shared+1))
done < "$list"

for f in "${ALLOWLIST[@]}"; do
  if [ -z "$(git_mode "$f")" ]; then
    printf 'FAIL | 白名单条目 %s 不在 git 里（改名了？请同步更新本脚本的 ALLOWLIST）\n' "$f"
    fail=$((fail+1))
    continue
  fi
  check_one "$f"
done

for f in "${VENDOR_ALLOWLIST[@]}"; do
  if [ -z "$(git_mode "$f")" ]; then
    printf 'FAIL | vendored 白名单条目 %s 不在 git 里（re-vendor 时改名/挪走了？）\n' "$f"
    fail=$((fail+1))
    continue
  fi
  check_one "$f"
  checked_vendor=$((checked_vendor+1))
done

echo
# **自检**：受检数为 0 说明这条守卫已经失去意义（shared/ 改名/移动、glob 写错、
# git 只是"成功地返回了 0 行"）。此时必须红，不能打印"全部 100755"（审计 I1）。
if [ "$checked" -eq 0 ]; then
  echo "===== 守卫自检失败：0 个受检文件 —— 这条守卫已形同虚设 ====="
  exit 1
fi
if [ "$checked_shared" -eq 0 ]; then
  echo "===== 守卫自检失败：shared/ 下 0 个带 shebang 的受检文件 —— 该目录改名/移动了？ ====="
  echo "      （白名单条目仍在，故总数非 0；但 shared/** 这个主作用域已失守）"
  exit 1
fi
if [ "$checked_vendor" -ne "${#VENDOR_ALLOWLIST[@]}" ]; then
  echo "===== 守卫自检失败：vendored 作用域只查到 $checked_vendor / ${#VENDOR_ALLOWLIST[@]} 个 ====="
  echo "      （每个作用域都要各自证明「我确实查到了东西」，否则整目录消失会被别的作用域盖成绿）"
  exit 1
fi
if [ "$fail" -gt 0 ]; then
  echo "===== 合计 FAIL=$fail（受检 $checked）—— 干净 checkout 上这些文件将不可执行 ====="
  exit 1
fi
echo "===== 合计 PASS（受检 $checked），带 shebang 的文件在 git 里均为 100755 ====="

# ---------------------------------------------------------------------------
# 漂移软警告（非阻断）。照抄 `src-tauri/build.rs::check_vendor_freshness` 的形状：
# 上游缺席 → no-op；有差异 → 警告但**不改退出码**。
#
# 为什么现在就要：仓内 `shared/cc-bus/` 今天是**死副本**（无 include_str!、无部署代码
# 引用它），漂移不会让任何东西变红——但盘上那份是**活的**（~/.local/bin 12 条软链指着它，
# 本机此刻就有 cc-bus 实例在跑），且这份代码的历史正是"一直手改、无版本管理"。
# 真正会被咬到的不是仓库，是 **B02**：其 §2.1/§2.4 是一张**按行号写死**的对拍表，
# 用来论证"删掉预信任那 46 行不是回归"。若两份之间发生了手改，B02 就是在对着一个
# **已经不是运行态**的基线做等价性证明。故这条警告保护的是那份论证，不是文件本身。
#
# 刻意**不**把盘上那份改成指向仓内工作树的 symlink：`git checkout` 到别的分支/旧 commit
# 会当场把用户正在运行的消息总线换掉——那比漂移糟得多。
UPSTREAM="$HOME/.claude/skills/cc-bus"
if [ -d "$UPSTREAM" ] && [ -d shared/cc-bus ]; then
  if ! diff -r -q --exclude='*.bak*' "$UPSTREAM" shared/cc-bus > /dev/null 2>&1; then
    echo
    echo "警告（不阻断）：shared/cc-bus 与 $UPSTREAM 已漂移。"
    echo "  差异： diff -r --exclude='*.bak*' \"$UPSTREAM\" shared/cc-bus"
    echo "  影响： B02 的等价性论证按行号锚定在仓内这份基线上，漂移会使其失效。"
  fi
fi
