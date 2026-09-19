#!/usr/bin/env bash
# 秤 7（`调研/设计/17 §6` 表第 7 行）的**夹具生成器** —— 一条命令重建，产物不进 git。
#
# 🔴 语料纪律（`设计/17 §6`「数据源纪律」2026-09-18 改判）：
#     **结构照真的，内容一律合成。真机会话的正文一个字都不许进仓。**
# 本脚本**只读仓内那份已经合成过的语料** `tests/__fixtures__/scale2-height-records.jsonl`
# （69 条 / 451 573 B，形状指纹见 `tests/evidence/U-scale2-corpus-shape.json`），
# 按**结构**把它放大到 `设计/17 §1` 那一档的单会话字节（max 15.1 MB）。
# **它不读 `~/.claude/projects`，一次都不读。** 放大 = 循环拼接 + uuid 尾号按轮次重编
# （等宽替换 ⇒ 每行字节数一字不变 ⇒ 行长分布逐字保持）。
#
# 产物落 `<repo>/.build/s7/`（`.gitignore` 第 52 行 `/.build/` 已忽略）。15 MB 不该进仓。
#
# 用法：
#   tests/evidence/S7-make-corpus.sh [OUT_DIR] [TARGET_BYTES]
# 默认：OUT_DIR=<repo>/.build/s7   TARGET_BYTES=15833498（= 15.1 MiB）
#
# 产出（daemon 的路径围栏 `fence_under_projects` 要求落在 `<agent_home>/projects/` 下，
# 故造一个假 agent home；bench 靠 `CLAUDE_CONFIG_DIR` 指过去）：
#   <OUT_DIR>/home/projects/s7-bench/s7-p50.jsonl       ← 990 KiB（`设计/17 §1` 单会话字节**中位**）
#   <OUT_DIR>/home/projects/s7-bench/s7-p90.jsonl       ← 4318 KiB（同表 **p90**）
#   <OUT_DIR>/home/projects/s7-bench/s7-main.jsonl      ← 15.1 MiB（同表 **max**），无巨记录
#   <OUT_DIR>/home/projects/s7-bench/s7-longtail.jsonl  ← 同 max + 每 1000 条插一条 631 672 B 巨记录
#                                                         （`设计/17 §1.1` 推论二：按 617 KB 估，不按 1.6 KB）
#   <OUT_DIR>/home/projects/s7-bench/s7-empty.jsonl     ← 0 字节，喂给 bench 的反空真自检
#
# 三档 p50/p90/max 同形同源 ⇒ 「耗时 ∝ 文件字节」这条斜率量得出来（`设计/17 §3.1` 的
# 「O(文件) 不可避免」要的就是这个斜率）。
#
# ⚠ `LC_ALL=C`：awk 的 `length()` 必须按**字节**算（语料含原生 CJK，UTF-8 locale 下
#   它会按字符算 ⇒ 目标字节数偏小三成）。

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
src="$repo/tests/__fixtures__/scale2-height-records.jsonl"
out_dir="${1:-$repo/.build/s7}"
target="${2:-15833498}"

# ── 反空真：源语料必须在、必须非空 ─────────────────────────────────────────
if [ ! -f "$src" ]; then
  echo "S7-make-corpus: 源语料不在：$src" >&2
  exit 1
fi
src_bytes=$(LC_ALL=C wc -c <"$src")
src_lines=$(LC_ALL=C wc -l <"$src")
if [ "$src_bytes" -lt 100000 ] || [ "$src_lines" -lt 10 ]; then
  echo "S7-make-corpus: 源语料太小（${src_bytes} B / ${src_lines} 行），多半不是那份合成语料" >&2
  exit 1
fi

sess="$out_dir/home/projects/s7-bench"
mkdir -p "$sess"
p50="$sess/s7-p50.jsonl"
p90="$sess/s7-p90.jsonl"
main="$sess/s7-main.jsonl"
longtail="$sess/s7-longtail.jsonl"
empty="$sess/s7-empty.jsonl"
# `设计/17 §1` 单会话字节：中位 990 KB · p90 4318 KB · max 15.1 MB（按 KiB/MiB 取）
target_p50=$((990 * 1024))
target_p90=$((4318 * 1024))

gen() { # gen <out> <target-bytes> <giant-every-n-records:0=off>
  LC_ALL=C awk -v TARGET="$2" -v GIANT_EVERY="$3" '
    { L[NR] = $0 }
    END {
      n = NR
      if (n == 0) { print "源语料一行都没有" > "/dev/stderr"; exit 1 }

      # 巨记录的填充串：ASCII 字母 + 数字 + 原生 CJK，比例粗对齐 U-scale2-corpus-shape.json
      # 的 latin/digits/cjk（88972 / 9650 / 60919）。**合成，不是任何真正文。**
      unit = "abcdefghijklmnopqrstuvwxyz 0123456789 一二三四五六七八九十 "
      pad = unit
      GIANT_LINE = 631672   # = `设计/17 §1` 的「单条记录字节 最大 631672 B ＝ 617 KB」
      while (length(pad) < GIANT_LINE) pad = pad pad

      rep = 0; bytes = 0; emitted = 0
      while (bytes < TARGET) {
        tag = sprintf("%04x", rep % 65536)
        for (i = 1; i <= n && bytes < TARGET; i++) {
          s = L[i]
          # uuid 尾 12 位的高 4 位换成轮次号 —— 等宽 ⇒ 行字节数不变。
          gsub(/0000-4000-8000-0000/, "0000-4000-8000-" tag, s)
          print s
          bytes += length(s) + 1
          emitted++
          if (GIANT_EVERY > 0 && emitted % GIANT_EVERY == 0 && bytes < TARGET) {
            head = "{\"type\":\"assistant\",\"uuid\":\"00000000-0000-4000-8000-" tag \
                sprintf("%08x", emitted) "\",\"parentUuid\":null," \
                "\"timestamp\":\"2026-01-01T00:00:00.000Z\",\"message\":{\"role\":\"assistant\"," \
                "\"model\":\"claude-opus-5\",\"content\":[{\"type\":\"text\",\"text\":\""
            foot = "\"}]}}"
            g = head substr(pad, 1, GIANT_LINE - length(head) - length(foot)) foot
            print g
            bytes += length(g) + 1
            emitted++
          }
        }
        rep++
      }
      printf("reps=%d records=%d bytes=%d\n", rep, emitted, bytes) > "/dev/stderr"
    }
  ' "$src" >"$1"
}

echo "S7-make-corpus: 源 = $src（${src_bytes} B / ${src_lines} 行）"
echo "--- s7-p50.jsonl";      gen "$p50" "$target_p50" 0
echo "--- s7-p90.jsonl";      gen "$p90" "$target_p90" 0
echo "--- s7-main.jsonl";     gen "$main" "$target" 0
echo "--- s7-longtail.jsonl"; gen "$longtail" "$target" 1000
: >"$empty"

# ── 反空真之二：产物必须够大、必须以 \n 收尾（torn 残尾会改口径） ───────────
for spec in "$p50:$target_p50" "$p90:$target_p90" "$main:$target" "$longtail:$target"; do
  f="${spec%:*}"; want="${spec##*:}"
  b=$(LC_ALL=C wc -c <"$f")
  l=$(LC_ALL=C wc -l <"$f")
  if [ "$b" -lt "$want" ]; then
    echo "S7-make-corpus: $f 只有 $b B < 目标 $want B" >&2
    exit 1
  fi
  if [ "$(tail -c 1 "$f" | xxd -p)" != "0a" ]; then
    echo "S7-make-corpus: $f 不是以换行收尾" >&2
    exit 1
  fi
  printf '%s: %s B / %s 行  p50/p90/max 行长: ' "$(basename "$f")" "$b" "$l"
  LC_ALL=C awk '{print length($0)}' "$f" | sort -n |
    awk '{a[NR]=$1} END{printf("%d / %d / %d\n", a[int(NR*0.5)], a[int(NR*0.9)], a[NR])}'
done

echo
echo "agent home = $out_dir/home"
echo "会话文件   = $p50"
echo "            $p90"
echo "            $main"
echo "            $longtail"
echo "空文件     = $empty（反空真自检用）"
