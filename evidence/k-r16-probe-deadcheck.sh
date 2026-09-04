#!/bin/bash
# K-R16 D4 · 对 D2 量具（k-r16-pidfile-cadence-probe.py）做死值验。
#
# 为什么非做不可：本件的结论（「CC 没有心跳，只在状态转换时写」）**整个压在**
# 探针报出来的「静置段零写入」上。而「真的零写入」与「探针自己哑了」
# （看错目录 / 变更判据失效 / 进程早退）在输出上**长得一模一样** —— 正是本仓最恨的静默失效。
#
# 判据两臂：
#   绿臂：探针盯着真目录 → 静置段该报 0，注入一次写后该报**恰好 1**。
#   红臂：把同一台探针指到一个**空的诱饵目录**（模拟「尺子看错地方」），
#         写照样发生在真目录 → 探针该报 0 ⇒ 判据必须**当场翻红**。
#         🔴 红臂不红 = 这条判据本身是死值，绿臂那个 PASS 一文不值。
#
# 用法：bash evidence/k-r16-probe-deadcheck.sh [工作目录]   （默认建在 $TMPDIR）
set -u
PROBE="$(cd "$(dirname "$0")" && pwd)/k-r16-pidfile-cadence-probe.py"
WORK="${1:-${TMPDIR:-/tmp}/k-r16-deadcheck}"
rm -rf "$WORK"; mkdir -p "$WORK/real" "$WORK/decoy"

seed() {
  printf '{"pid":999999,"sessionId":"dead-check","cwd":"/x","status":"idle","updatedAt":1,"statusUpdatedAt":1}\n' \
    > "$WORK/real/999999.json"
}

run_arm() {         # $1=臂名  $2=探针盯的目录  $3=期望的 write 次数
  local arm="$1" watch="$2" want="$3" out="$WORK/$1.jsonl"
  rm -f "$out"; seed
  python3 "$PROBE" --dir "$watch" --out "$out" --interval 0.2 --seconds 6 --tick 999 &
  local probe=$!
  python3 - "$WORK/real" <<'PY'
import sys, time, pathlib
time.sleep(2.5)   # 静置段：这 2.5s 内不许出现 write
pathlib.Path(sys.argv[1], "999999.json").write_text(
    '{"pid":999999,"sessionId":"dead-check","cwd":"/x","status":"busy","updatedAt":2,"statusUpdatedAt":2}\n'
)
PY
  wait $probe
  python3 - "$out" "$arm" "$want" <<'PY'
import json, sys
rows = [json.loads(l) for l in open(sys.argv[1])]
arm, want = sys.argv[2], int(sys.argv[3])
wr = [r for r in rows if r["kind"] == "write"]
print(f"[{arm}] seen={sum(r['kind']=='seen' for r in rows)} write={len(wr)} (期望 {want})")
for r in wr:
    print(f"    {r['iso']} gap={r['gap_s']}s {r['prev_status']} -> {r['status']} changed={r['changed']}")
ok = len(wr) == want and all(r["status"] == "busy" and r["prev_status"] == "idle" for r in wr)
print(f"[{arm}] {'PASS' if ok else 'FAIL'}")
sys.exit(0 if ok else 1)
PY
}

echo "=== 绿臂：探针盯真目录，注入一次写 ⇒ 该报 1 ==="
run_arm green "$WORK/real" 1; g=$?

echo
echo "=== 红臂：同一台探针指到空诱饵目录，写仍发生在真目录 ⇒ 该报 0，判据该翻红 ==="
run_arm decoy "$WORK/decoy" 1; d=$?

echo
if [ "$g" -eq 0 ] && [ "$d" -ne 0 ]; then
  echo "DEADCHECK: PASS —— 绿臂 PASS 且红臂**真的红了** ⇒ 探针的「零写入」是读数，不是死值。"
  exit 0
fi
echo "DEADCHECK: FAIL —— 绿臂 rc=$g 红臂 rc=$d（红臂必须非 0；它若也 PASS，说明这条判据分不出真假）"
exit 1
