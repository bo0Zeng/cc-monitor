#!/usr/bin/env bash
# K-R2 摸底 · `KR22` 那把尺子的死值验：**判「解析发生在哪一侧」，不判「结果回来了没有」**。
#
# 件文件 `§3` 逐字要求：造一个「把文件拉回本机解析」的实现，**它必须红**。
# 09-04 实测：**造得出来**，而且这把尺子逮得住它。下面是那一趟的读数。
#
# ── 先把「造不出反例」这个判断打一次（`§3` 要求的那一打） ──────────────
# 顺手的说法是：「今天根本没有远端那一侧 ⇒ 反例造不出来。」**那是错的。**
# 这把尺子的**主语是本机侧的指纹**，而本机侧今天完整存在（引擎 + 索引落址 + 源文件）。
# 「远端」在反例里只需要**一个别的目录**来扮演 —— 它买到的是判据的形状与阴性对照，
# 买不到的是端到端真跨机（那要真 daemon，本拍不起）。⇒ 造得出，**只是射程要写清楚**。
#
# ── 三个指纹（都钉「在哪一侧发生」） ──────────────────────────────────
#   ① 本机侧出现被分析仓的**源文件**吗（按扩展名数文件 + 数字节）
#   ② 本机侧出现该仓的**索引库** `.codepicture/` 吗
#   ③ 从对侧收回来的字节量纲：O(符号数) 还是 O(仓大小)
#
# ── 09-04 实测读数（量于 e1944e8） ───────────────────────────────────
#   分母：夹具仓 **23 个源文件 / 189,178 字节**（vendor 引擎的 .rs + 前端 panorama 的 .ts）
#
#   | 组 | 收回本机的字节 | 本机侧源文件 | 本机侧 .codepicture | 判定 |
#   |----|---------------|-------------|--------------------|------|
#   | 绿：在代码所在地解析，只收 JSON | 68 B | 0 个 / 0 字节 | 0 处 | 🟢 |
#   | 红：把源文件拉回本机再解析      | 68 B | 23 个 / 189,178 字节 | 1 处 | 🔴 |
#
#   🔴🔴 **最要紧的那一条读数**：两组的 `result.json` 用 `cmp` 比过 —— **字节完全相同**
#   （`{"files":23,"symbols":253,"unresolved_calls":1021,"parse_errors":0}`）。
#   ⇒ 件文件 `§1 KR22` 那句「外部行为一模一样」**不是担心，是实测**。
#     只钉「返回了非空结果」的判据，在这两组上给出的是同一个答案。
#
# ── 这把尺子**认不出**什么（射程，别读成全覆盖） ──────────────────────
#   · 坏实现把源文件落在 tmpfs / 内存里、解析完就删 ⇒ 指纹①②都可能消失。
#     ⇒ 落地那天①②要配第三条（量收回字节的**量纲**：O(符号数) vs O(仓大小)），
#       同族先例是中转那条「逐块透传」——**量时序不量内容**。
#   · 它判的是「源文件/索引库出现在本机侧」，**不判**「远端那台机器上真起了解析进程」。
#     后者要等真有远端那一侧才量得了。
#   · 本趟的「远端」由同机另一个目录扮演 ⇒ 买到的是**判据形状 + 阴性对照**，不是端到端。
#
# 用法：bash evidence/K-R2-kr22-which-side.sh <带引擎的 daemon 二进制> <夹具源目录…>
#   （那个二进制由 K-R2-size-and-arch.sh 的变量组编出来，带 `--panorama-probe <仓>`）
set -uo pipefail
BIN="${1:?用法: K-R2-kr22-which-side.sh <带引擎的二进制> <夹具源目录…>}"; shift
W=$(mktemp -d)
REMOTE=$W/remote-side/fixture-repo; LOCAL_G=$W/local-green; LOCAL_R=$W/local-red
mkdir -p "$REMOTE" "$LOCAL_G" "$LOCAL_R"
for d in "$@"; do cp "$d"/*.rs "$d"/*.ts "$REMOTE/" 2>/dev/null; done

judge() {  # judge <本机侧目录> <标签>：报三个指纹，红了返回非 0
  local d=$1 tag=$2 n b idx
  n=$(find "$d" -type f \( -name '*.rs' -o -name '*.ts' \) 2>/dev/null | wc -l)
  b=$(find "$d" -type f \( -name '*.rs' -o -name '*.ts' \) -exec stat -c %s {} + 2>/dev/null | paste -sd+ | bc)
  idx=$(find "$d" -type d -name '.codepicture' 2>/dev/null | wc -l)
  echo "[$tag] 本机侧源文件=$n 个 / ${b:-0} 字节 · 本机侧 .codepicture=$idx 处"
  { [ "$n" -gt 0 ] || [ "$idx" -gt 0 ]; } && { echo "[$tag] 🔴 解析发生在本机"; return 1; }
  echo "[$tag] 🟢 本机侧既无源文件也无索引库"; return 0
}

SRC_N=$(find "$REMOTE" -type f | wc -l)
echo "分母：夹具仓 $SRC_N 个源文件 / $(find "$REMOTE" -type f -exec stat -c %s {} + | paste -sd+ | bc) 字节"

echo; echo "=== 绿组：在代码所在地解析，只把结果收回本机 ==="
( cd "$REMOTE" && "$BIN" --panorama-probe "$REMOTE" ) > "$LOCAL_G/result.json" 2>/dev/null
echo "收回本机 $(stat -c %s "$LOCAL_G/result.json") B：$(cat "$LOCAL_G/result.json")"
judge "$LOCAL_G" 绿组; G=$?

echo; echo "=== 红组：把源文件拉回本机再解析 ==="
cp "$REMOTE"/* "$LOCAL_R/" 2>/dev/null
( cd "$LOCAL_R" && "$BIN" --panorama-probe "$LOCAL_R" ) > "$LOCAL_R/result.json" 2>/dev/null
echo "收回本机 $(stat -c %s "$LOCAL_R/result.json") B：$(cat "$LOCAL_R/result.json")"
cmp -s "$LOCAL_G/result.json" "$LOCAL_R/result.json" \
  && echo "★ 两组结果字节**完全相同** ⇒ 只钉「结果回来了」的判据分不出两者" \
  || echo "★ 两组结果不同（夹具变了？口径要重讲）"
judge "$LOCAL_R" 红组; R=$?

echo; [ "$G" -eq 0 ] && [ "$R" -ne 0 ] \
  && echo "KR22-尺子：✅ 红绿都动 —— 反例造得出来，尺子逮得住" \
  || echo "KR22-尺子：❌ 有一侧没动 —— 尺子在空转"
rm -rf "$W"
