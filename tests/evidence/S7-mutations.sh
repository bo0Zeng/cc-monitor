#!/usr/bin/env bash
# 秤 7 的**死值验 + 一次变体实验**。
#
# 🔴 纪律：**一个字都不在主树上改。** 先把三棵树（`src/backend` · `src/bridge/crates`
# · `tests/`）按同样的相对布局复制到一个一次性目录，变异只发生在那份副本里。
# 理由：主树上有别的 agent 在写；而「改完记得改回来」是一条靠记性的纪律，靠不住。
#
# 跑三次同一把秤（`cargo bench --bench s7_history_read`），只换被测二进制：
#
#   A 原样      —— 基线
#   B 死值      —— `read_session_tail` 整个掏空成 `Ok(())`。
#                  读数必须**肉眼可分**（数量级掉下来 + 出字节归零）。
#                  掉不下来就说明这把秤根本没在量那个函数。
#   C 变体      —— 只把「这一行是不是空行」的判定从
#                  `String::from_utf8_lossy(整行)` 换成**先看一眼字节**的短路快路
#                  （语义等价：只要有一个「ASCII 且非 ASCII 空白」的字节，这行就
#                   一定不是空行；没有才退回原判定）。
#                  它回答的是：K 那十几毫秒里，有多少是**整文件 UTF-8 校验**。
#
# 用法： tests/evidence/S7-mutations.sh [DV_DIR]
# 夹具复用 `.build/s7/`（由 S7-make-corpus.sh 生成），三跑同一份、同一台机。

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
dv="${1:-${TMPDIR:-/tmp}/s7-dv}"
home="$repo/.build/s7/home"

if [ ! -d "$home/projects/s7-bench" ]; then
  echo "S7-mutations: 夹具不在，先跑 tests/evidence/S7-make-corpus.sh" >&2
  exit 1
fi

rm -rf "$dv"
mkdir -p "$dv/src/bridge" "$dv/tests"
cp -a "$repo/src/backend" "$dv/src/"
cp -a "$repo/src/bridge/crates" "$dv/src/bridge/"
# `src/backend/Cargo.toml` 头注逐字：「`cargo build` **真的独立**，`cargo test` **不独立**」——
# 测块有几条 `include_str!` 伸进 monitor 那棵树与 `src/doc/`。要在副本里跑得动等价性
# 对拍就得把这两棵也带上，否则不是测试红，是**编译**红。
cp -a "$repo/src/bridge/src" "$dv/src/bridge/"
cp -a "$repo/src/doc" "$dv/src/"
cp -a "$repo/tests/backend" "$dv/tests/"
cp -a "$repo/tests/evidence" "$dv/tests/"

target="$dv/src/backend/observe/history_query.rs"
cp "$target" "$dv/history_query.rs.orig"

run() { # run <标签>
  echo
  echo "############ $1 ############"
  # 两跑取第二跑：第一跑与 cargo 的构建/写盘挨着，噪声大（实测进程地板能从
  # 0.6 ms 蹿到 3.0 ms，把整张表一起抬起来）。`taskset` 钉核同理。
  (cd "$dv/src/backend" &&
    S7_HOME="$home" taskset -c 2-5 cargo bench --offline --bench s7_history_read >/dev/null 2>&1 &&
    S7_HOME="$home" taskset -c 2-5 cargo bench --offline --bench s7_history_read 2>&1 |
    sed -n '/^格 /,$p')
}

# ── A 原样 ────────────────────────────────────────────────────────────────
run "A 原样（基线）"

# ── B 死值：read_session_tail 掏空 ─────────────────────────────────────────
python3 - "$target" <<'PY'
import re, sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
sig = "fn read_session_tail(agent_home: &Path, jsonl_path: &str, n: usize) -> Result<(), String> {"
i = s.index(sig)
j = s.index("\n}\n", i) + len("\n}\n")
s = s[:i] + sig + "\n    let _ = (agent_home, jsonl_path, n);\n    Ok(())\n}\n" + s[j:]
open(p, "w", encoding="utf-8").write(s)
print("死值已注入：read_session_tail → Ok(())")
PY
run "B 死值（read_session_tail 掏空成 Ok(())）"

# ── C 变体：空行判定加字节短路快路 ──────────────────────────────────────────
cp "$dv/history_query.rs.orig" "$target"
python3 - "$target" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
old = """        let text = String::from_utf8_lossy(&buf[..buf.len() - 1]);
        if text.trim_start_matches('\\u{feff}').trim().is_empty() {
            continue; // 空行不计（与 watcher/monitor 口径一致）
        }
"""
new = """        // 〔秤 7 变体 C'〕只要有一个「ASCII 且非空白」的字节，这一行就一定不是空行
        // —— 不必把整行拿去 from_utf8_lossy 校验一遍。jsonl 每行首字节是 `{`
        // ⇒ 快路在第 1 个字节就短路。没有这种字节才退回原判定，语义不变
        // （穷举证据：tests/evidence/S7-blank-line-equivalence.rs）。
        let decided = buf[..buf.len() - 1]
            .iter()
            .any(|b| b.is_ascii() && !char::from(*b).is_whitespace());
        if !decided {
            let text = String::from_utf8_lossy(&buf[..buf.len() - 1]);
            if text.trim_start_matches('\\u{feff}').trim().is_empty() {
                continue; // 空行不计（与 watcher/monitor 口径一致）
            }
        }
"""
assert s.count(old) == 1, "空行判定那一段没匹配上 —— 生产代码动过了，先核对再跑"
open(p, "w", encoding="utf-8").write(s.replace(old, new))
print("变体已注入：空行判定 → 字节短路快路")
PY
echo "--- C 的等价性先过既有单测（口径锚点 tail_tests 全在这一族里）"
(cd "$dv/src/backend" && cargo test --offline history_query:: 2>&1 | grep -E "test result|^error" | head -3)
echo "--- C 的等价性再过穷举对拍"
rustc -O -o "$dv/s7eq" "$repo/tests/evidence/S7-blank-line-equivalence.rs" && "$dv/s7eq"
run "C 变体（空行判定走字节短路，不再整行 from_utf8_lossy）"

cp "$dv/history_query.rs.orig" "$target"
echo
echo "三跑完毕。变异只发生在 $dv，主树一个字没动。"
