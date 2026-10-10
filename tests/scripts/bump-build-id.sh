#!/usr/bin/env bash
# 合并那一拍打版本号的机械那一半：改 `src/backend/lib.rs` 的 `BUILD_ID`；子命令集变了就改指纹历史表
#   （`tests/backend/build_id_guard.rs` 的 `SUBCOMMAND_HISTORY` 只留「上一版 ＋ 当前版」两行 ⇒ 删最老那行、追加新行）。
# 用法：在仓里任一处 `bash tests/scripts/bump-build-id.sh <新 id>`（新 id 照 `p<代号><小写字母>-<名>`）。
# 新行从哪来：指纹那条红时印一行 `NEW-ROW: ("<新 id>", "<指纹>")`，本脚本按它改表 —— 指纹只有那条测试会算，这里不另算一份。
# 不做的：`lib.rs` 版本谱系那段说明（人写）· re-embed（下一步，`tests/scripts/re-embed.sh`）。
# 测试在沙箱里跑：HOME 与 TMUX_TMPDIR 指到 `.build/` 下的一次性目录，清掉 TMUX / TMUX_PANE。
# 判据：`build_id_guard::tests::the_bump_script_walks_both_paths`（假 cargo，没变 / 变了两条路各走一次）。
# 末行 `BUMP: OK` / `BUMP: 红 —— <为什么>`。
set -uo pipefail
new=${1:-}
[ -n "$new" ] || { echo "BUMP: 红 —— 用法：bump-build-id.sh <新 id>"; exit 2; }
root=$(git rev-parse --show-toplevel) || exit 2
cd "$root" || exit 2
lib=src/backend/lib.rs
guard=tests/backend/build_id_guard.rs
[[ $new =~ ^p[0-9][a-z]+-[a-z0-9-]+$ ]] || { echo "BUMP: 红 —— $new 不是 p<代号><小写字母>-<名> 的形状"; exit 1; }
old=$(sed -n 's/^pub const BUILD_ID: &str = "\([^"]*\)";$/\1/p' "$lib")
[ -n "$old" ] || { echo "BUMP: 红 —— $lib 里抠不出 BUILD_ID"; exit 1; }
[ "$old" != "$new" ] || { echo "BUMP: 红 —— 新旧 id 一样（$old）"; exit 1; }
mkdir -p .build
box=$(mktemp -d "$root/.build/bump-box.XXXXXX") || exit 2
trap 'rm -rf "$box"' EXIT
mkdir -p "$box/home" "$box/tmux"
cargo_home=${CARGO_HOME:-$HOME/.cargo}
rustup_home=${RUSTUP_HOME:-$HOME/.rustup}
t() { (cd src/backend && env -u TMUX -u TMUX_PANE HOME="$box/home" TMUX_TMPDIR="$box/tmux" CARGO_HOME="$cargo_home" \
  RUSTUP_HOME="$rustup_home" CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}" nice -n 10 cargo test --lib -q -- "$@" 2>&1); }
sed -i "s/^pub const BUILD_ID: &str = \"$old\";\$/pub const BUILD_ID: \&str = \"$new\";/" "$lib"
echo "  BUILD_ID $old → $new"
out=$(t build_id_guard::tests::adding_a_subcommand_forces_a_build_id_bump)
row=$(printf '%s\n' "$out" | sed -n 's/^ *NEW-ROW: (\\\{0,1\}"<新 id>\\\{0,1\}", \(".*"\))$/\1/p' | head -1)
if [ -z "$row" ]; then
  printf '%s\n' "$out" | grep -q 'test result: ok' || { printf '%s\n' "$out" | tail -15; echo "BUMP: 红 —— 指纹那条没过，也没印 NEW-ROW"; exit 1; }
  echo "  子命令集没变 ⇒ 历史表不动"
else
  python3 - "$guard" "$new" "$row" <<'PY' || { echo "BUMP: 红 —— 改表失败"; exit 1; }
import re, sys
p, new, row = sys.argv[1:]
s = open(p, encoding="utf-8").read()
m = re.search(r"(    const SUBCOMMAND_HISTORY: &\[\(&str, &str\)\] = &\[\n)(.*?)(\n    \];)", s, re.S)
if not m:
    sys.exit("找不到 SUBCOMMAND_HISTORY")
rows = re.findall(r'        \(\n            "[^"\n]+",\n            "[^\n]*",\n        \),', m.group(2))
if len(rows) != 2:
    sys.exit(f"表里不是两行：{len(rows)}")
body = rows[1] + "\n" + f'        (\n            "{new}",\n            {row},\n        ),'
open(p, "w", encoding="utf-8").write(s[: m.start(2)] + body + s[m.end(2):])
PY
  echo "  子命令集变了 ⇒ 历史表删最老那行、追加 $new"
fi
out=$(t build_id_guard hx2_every)
printf '%s\n' "$out" | grep -q 'test result: ok' || { printf '%s\n' "$out" | grep -E 'panicked|FAILED' | head; echo "BUMP: 红 —— 改完 build_id_guard / hx2 没绿"; exit 1; }
echo "  build_id_guard ＋ hx2 绿；接着 lib.rs 版本谱系补一段 → re-embed"
echo "BUMP: OK"
