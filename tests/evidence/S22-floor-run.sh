#!/usr/bin/env bash
# S22 · `applyIntrinsicSize` 的 `Math.max(24, …)` 地板读数 —— **一键复算**。
# 打包探针 → 两个真引擎各跑一遍 → 出表。**只读仓，不改 src/，不刷秤 2 的金标准。**
#
# 引擎起法照抄 `tests/evidence/U-scale2-run.sh`（它又照抄 `真相源/90 §3.1/§3.7`）：
#   · Chromium 153      —— Playwright headless shell（生产 WebView2 同引擎家族）
#   · WebKitGTK 2.52.6  —— PyGObject + WebKit2-4.1，必须 xvfb + 关合成
# 两个 runner 复用 `/tmp/cv-reparent-probe/` 下 G 路留下的那两个脚本。
# ⚠ 它们住 /tmp 会被清掉；重建法见 `U-scale2-run.sh` 末尾那段。
#
# 用法：bash tests/evidence/S22-floor-run.sh        # 从仓根跑
#
# 🔴 反空真：探针自己带 `ok/failures/counts`，任何一段量到 0 行就 ok=false；
#    本脚本的汇总段见到 ok=false 或某一段计数为 0 **退 1**。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORK=/tmp/s22-floor
RIG=/tmp/cv-reparent-probe

cd "$ROOT"
mkdir -p "$WORK"

echo "── 仓位：$(git rev-parse --short HEAD) / $(git rev-parse --abbrev-ref HEAD)"
echo "── 工作树是否干净：$(git status --porcelain | wc -l) 个改动文件"

echo "── ① 打包探针（vite，产物只落 $WORK/dist）"
npx vite build --config tests/evidence/S22-floor-vite.config.ts

cat > "$WORK/probe.html" <<'HTML'
<!doctype html>
<html lang="zh">
<head>
<meta charset="utf-8">
<title>S22 · contain-intrinsic-size 地板读数</title>
<link rel="stylesheet" href="./dist/cc-monitor.css">
<style>html,body{margin:0;padding:0}</style>
</head>
<body>
<script src="./dist/probe.js"></script>
</body>
</html>
HTML

echo "── ② Chromium 153（Blink）"
node "$RIG/run_chromium.mjs" "$WORK/probe.html" "$WORK/result-chromium.json"

echo "── ③ WebKitGTK 2.52.6（xvfb + 关合成）"
WEBKIT_DISABLE_COMPOSITING_MODE=1 WEBKIT_DISABLE_DMABUF_RENDERER=1 \
  LIBGL_ALWAYS_SOFTWARE=1 GDK_BACKEND=x11 \
  xvfb-run -a -s "-screen 0 1280x1024x24" \
  python3 "$RIG/run_webkitgtk.py" "$WORK/probe.html" "$WORK/result-webkitgtk.json"

echo "── ④ 出表"
python3 - "$WORK/result-chromium.json" "$WORK/result-webkitgtk.json" <<'PY'
import json, sys, collections, statistics

NAMES = ["Chromium 153 (Blink)", "WebKitGTK 2.52.6"]
docs = []
bad = False
for path, name in zip(sys.argv[1:3], NAMES):
    try:
        d = json.load(open(path))
    except Exception as e:                       # noqa: BLE001
        print(f"🔴 {name}: 读不出结果 {path}: {e}")
        bad = True
        continue
    docs.append((name, d))

# ── 反空真门 ────────────────────────────────────────────────────────────────
for name, d in docs:
    c = d.get("counts") or {}
    print(f"\n### {name}")
    print("  UA:", (d.get("env") or {}).get("ua", "?"))
    print("  counts:", json.dumps(c, ensure_ascii=False))
    if not d.get("ok"):
        bad = True
        for f in d.get("failures", []):
            print("  🔴", f)
    for k in ("truthRows", "boxRows", "rememberRows", "stormTrials", "corpusTrials", "degenRows"):
        if c.get(k, 0) == 0:
            print(f"  🔴 {k} = 0 —— 一行都没量到，判失败")
            bad = True
if len(docs) < 2:
    print("🔴 只拿到不足两个引擎的读数")
    bad = True

def sec(t):
    print("\n" + "═" * 78)
    print(t)
    print("═" * 78)

# ── A 段 ────────────────────────────────────────────────────────────────────
sec("A 段 · 逐 class content-box 真高（现打，不读金标准）")
print(f"{'class':20}{'n':>4}{'  ':2}" + "".join(f"{n.split()[0]:>30}" for n, _ in docs))
allcls = sorted({r["cls"] for _, d in docs for r in d.get("truthRows", [])})
for cls in allcls:
    line = f"{cls:20}"
    n0 = None
    cells = []
    for _, d in docs:
        v = [r["trueContentBox"] for r in d["truthRows"] if r["cls"] == cls]
        n0 = n0 or len(v)
        cells.append(f"min {min(v):8.2f} max {max(v):9.2f}" if v else "—")
    print(line + f"{n0:>4}  " + "".join(f"{c:>30}" for c in cells))
for name, d in docs:
    v = [r["trueContentBox"] for r in d["truthRows"]]
    pb = [r["padBorder"] for r in d["truthRows"]]
    print(f"  {name}: 全盘 min content-box = {min(v):.2f}px（border-box {min(x['trueBorderBox'] for x in d['truthRows']):.2f}）")

# ── B 段 ────────────────────────────────────────────────────────────────────
sec("B 段 · 声明值扫描：盒模型 / 引擎下限 / 负值")
for name, d in docs:
    print(f"\n-- {name}")
    rows = d["boxRows"]
    by = collections.defaultdict(list)
    for r in rows:
        by[r["cls"]].append(r)
    for cls, rs in by.items():
        print(f"  {cls}  padBorder={rs[0]['padBorder']}")
        for r in rs:
            lit = r["declaredLiteral"] or "(无 inline → CSS auto 120px)"
            note = ""
            if r["declaredPx"] is not None and r["declaredPx"] >= 0:
                delta = r["effectiveIntrinsic"] - r["declaredPx"]
                note = "（= 声明值，无夹取）" if abs(delta) < 0.51 else f"🔴 与声明差 {delta:+.2f}"
            print(f"    {lit:30} computed={r['computedCIS']:16} measured={r['measured']:8.2f} "
                  f"effective={r['effectiveIntrinsic']:8.2f} skipped={r['skipped']} {note}")
        break  # 每个引擎只详列第一个 class，其余压成一行
    # 其余 class 的一句话结论
    for cls, rs in list(by.items())[1:]:
        ok = all(abs(r["effectiveIntrinsic"] - r["declaredPx"]) < 0.51
                 for r in rs if r["declaredPx"] is not None and r["declaredPx"] >= 0)
        print(f"  {cls}: padBorder={rs[0]['padBorder']}  "
              f"{'所有 ≥0 声明值均 effective==declared（无夹取）' if ok else '🔴 有夹取/偏差'}")

# ── C 段 ────────────────────────────────────────────────────────────────────
sec("C 段 · auto 的「记住真实尺寸」")
for name, d in docs:
    print(f"-- {name}")
    for r in d["rememberRows"]:
        print(f"   declared={r['declared']:3}px  首次skipped={r['beforeSkipped']:7.2f}  "
              f"渲染时={r['whileVisible']:7.2f}  再skipped={r['afterSkipped']:7.2f}  "
              f"skipped(前/后)={r['skippedBefore']}/{r['skippedAfter']}  记住了={r['remembered']}")

# ── D 段 ────────────────────────────────────────────────────────────────────
sec("D 段 · 300 张 card-api-retry 的重试风暴")
for name, d in docs:
    print(f"\n-- {name}")
    truth = next((t for t in d["stormTrials"] if "真高" in t["label"]), None)
    th = truth["estScrollHeight"] if truth else None
    print(f"   真 scrollHeight = {th}")
    print(f"   {'declared':>10}{'estScrollH':>12}{'误差':>12}{'相对':>9}"
          f"{'跳转落点err':>13}{'重发后':>9}{'Σ|ΔscrollH|':>13}{'单步max':>9}{'滚位slip':>10}")
    for t in d["stormTrials"]:
        if "真高" in t["label"]:
            continue
        err = t["estScrollHeight"] - th if th else float("nan")
        rel = err / th * 100 if th else float("nan")
        print(f"   {t['label'].split()[-1]:>10}{t['estScrollHeight']:>12.0f}{err:>12.0f}{rel:>8.1f}%"
              f"{t['jumpLandErr1']:>13.1f}{t['jumpLandErr2']:>9.1f}"
              f"{t['sweepSumAbsDelta']:>13.0f}{t['sweepMaxAbsDelta']:>9.0f}{t['sweepMaxScrollSlip']:>10.1f}")

# ── E 段 ────────────────────────────────────────────────────────────────────
sec("E 段 · 真语料（83 张卡）上改地板的差额")
for name, d in docs:
    print(f"\n-- {name}")
    truth = next((t for t in d["corpusTrials"] if "真高" in t["label"]), None)
    th = truth["estScrollHeight"] if truth else None
    print(f"   真 scrollHeight = {th}")
    for t in d["corpusTrials"]:
        if "真高" in t["label"]:
            continue
        err = t["estScrollHeight"] - th if th else float("nan")
        print(f"   {t['label']:>16}  estScrollH={t['estScrollHeight']:>9.0f}  误差={err:>8.0f}"
              f"  ({err/th*100 if th else 0:+.2f}%)  跳转落点err={t['jumpLandErr1']:>8.1f}"
              f"  重发后={t['jumpLandErr2']:>7.1f}  Σ|ΔscrollH|={t['sweepSumAbsDelta']:>8.0f}")


# ── G 段 ────────────────────────────────────────────────────────────────────
sec("G 段 · 地板自陈「防 0/负值」—— 0 出不出得来？出来了对不对？")
for name, d in docs:
    print(f"-- {name}")
    for r in d.get("degenRows", []):
        if not r["built"]:
            print(f"   {r['label']:22} renderMessage 没建出卡（这条路在生产里不存在）")
            continue
        print(f"   {r['label']:22} cls={r['cls']:14} estRaw={r['estRaw']} round={r['estRounded']:>4} "
              f"真content-box={r['trueContentBox']:7.2f} 真border-box={r['trueBorderBox']:7.2f} "
              f"无地板误差={r['errNoFloor']:+7.2f}  24地板误差={r['errFloor24']:+7.2f}")

# ── 环境 ────────────────────────────────────────────────────────────────────
sec("F 段 · 环境 / 特性支持")
for name, d in docs:
    print(f"-- {name}: {json.dumps(d['env']['supports'], ensure_ascii=False)}")
    print(f"   viewport={d['env']['viewport']}  tokens={json.dumps(d['env']['tokens'], ensure_ascii=False)}")

print()
if bad:
    print("🔴 读数不完整或某一段为空 —— 退 1（反空真）")
    sys.exit(1)
print("✅ 两个引擎的五段读数都非空")
PY
