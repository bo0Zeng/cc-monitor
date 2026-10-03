#!/usr/bin/env python3
"""`G4` 空洞③ 的**反空真那一半**：这一趟到底**跑没跑**门禁。

# 它治的是什么

登记的三个空洞里，③ 逐字是「`gate.sh` 云端零调用、零 git 钩子」。
把 job 写进 `ci.yml`、把 hook 放进 `tests/hooks/`，填的只是那句话的**前半**。

🔴 **后半句是另一件事，而且是更贵的那件**：

    「装了钩子」**不等于**「它跑过」。

这不是修辞。同一族的现打读数本仓已经有一条（`tests/scripts/hooks-are-runnable.sh`
头注，git 2.43.0，合成仓）：一个 hook **没有可执行位**时，git **忽略它并照常提交** ——
`rc=0`，只在 stderr 留一句 `advice.ignoredHook` 就能关掉的提示。
⇒ **闸门整个不在了，而退出码看起来一切正常。**

同一形在 CI 上更容易发作，因为那儿的失败面更多：
job 里那一步 `if:` 求值成 false · 前一步 `continue-on-error` 把红洗成绿 ·
`GATE_ONLY` 打错一个字 · 有人把 `run:` 那行注释掉。
**这几种在流水线的绿勾上长得一模一样。**

# 取法：**收据**，不是「盘上有没有那份文件」

`gate.sh` 每跑一趟落一张收据（`.build/gate-receipt.json`），里面记的都是
**只有真跑过这一趟才拿得到**的东西。本文件判那张收据。

    python3 tests/evidence/K-G4C-gate-receipt.py [--receipt 路径] [--gate 路径]
                                                 [--require "格名 格名 …"]

`--require` 与 `--require-all` 必须给一个（出货那一趟 `tests/hooks/pre-push` 用 `--require-all`，
格名从 `gate.sh` 现算）；两个都不给 ⇒ 红（「一格都不要求」与「要求的都跑了」分不开）。

# 🔴 反空真锚：`R6` 那**两向集合相等**

`ran ∪ skipped` 与 `gate.sh` 里**现打**的格名，两侧互为子集。
· 收据被写空 / 被换成 `{}` ⇒ 左边空，右边 26 ⇒ 分叉。
· `gate.sh` 被读成空串、或 `found_cells()` 的几条正则被改瞎 ⇒ 右边空 ⇒ 分叉。
· 门禁里有一格**静默没跑**（`gate_wants` 漏接、手写格忘了 `GATE_RAN+=`）⇒ 左边少一个 ⇒ 分叉。
**单向包含（「跑过的都在盘上」）在收据被清空时恒真** —— 那正是不能用它当锚的理由。

# ⚠ 它买不到什么（逐条写死，别读成「从此云端有保障」）

1. **它不说那一趟绿不绿之外的任何事。** 收据只记「哪几格判过」，
   一格判过而它的判据本身是空真，本文件一个字都问不出来。
2. **它挡不住有人不跑它。** 一条判据挡不住「没人调用这条判据」—— 今天调它的只有 `tests/hooks/pre-push`。
   CI 的 job 直接调门禁，判的是门禁的退出码，不另跑本文件。
3. **`--require` 只判「这几格在 `ran` 里」**，不判那几格够不够。
"""

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
DEFAULT_RECEIPT = ROOT / ".build/gate-receipt.json"
DEFAULT_GATE = ROOT / "tests/scripts/gate.sh"

# ── `gate.sh` 现打的格名：行首 `run_gate <名> ` · `run_gate_sum <名> ` · `run_e2e <名>` · `if gate_wants <名>;` ──
# 缩进着的是门禁自己的自检探针，不是格 ⇒ 只认行首。e2e 那批在收据里的规范名带前缀。
E2E_PREFIX = "ccm tests/e2e/"
CELL_PATTERNS = [
    (re.compile(r"^run_gate_sum (\S+) ", re.M), lambda m: m.group(1)),
    (re.compile(r"^run_gate (\S+) ", re.M), lambda m: m.group(1)),
    (re.compile(r"^run_e2e (\S+)[ \t]*$", re.M), lambda m: E2E_PREFIX + m.group(1)),
    (re.compile(r"^if gate_wants (\S+);", re.M), lambda m: m.group(1)),
]


def found_cells(text):
    return [name_of(m) for pat, name_of in CELL_PATTERNS for m in pat.finditer(text)]

REQUIRED_KEYS = ("verdict", "gate_sha256", "tree", "head", "dirty", "when", "ran", "skipped")


def sha256_of(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


def head_tree():
    out = subprocess.run(["git", "-C", str(ROOT), "rev-parse", "HEAD^{tree}"],
                         capture_output=True, text=True)
    return out.stdout.strip() if out.returncode == 0 else ""


# ── 判一张收据。**真判据与阳性对照走同一个函数** —— 两份必漂 ────────────────────
def judge(receipt_path, gate_path, require):
    """返回 (fails, passed)。`fails` 空 = 这一趟真跑过、且该跑的都跑了。"""
    fails, passed = [], 0

    def ok():
        nonlocal passed
        passed += 1

    rp, gp = Path(receipt_path), Path(gate_path)

    # R0 被测对象在不在
    if not gp.is_file():
        fails.append(f"R0 门禁本体 `{gp}` 盘上不存在 —— 没有它就数不出「盘上有几格」，判不了")
        return fails, passed
    ok()

    # R1 🔴 **收据不在 = 这一趟没跑门禁。** 这条是本文件存在的全部理由。
    if not rp.is_file():
        fails.append(
            f"R1 收据 `{rp}` **不在盘上** —— 这一趟**没跑门禁**（或它在落收据之前就死了）。"
            f"🔴 「装了钩子 / 写了 job」与「它真跑了」在终端上一模一样，"
            f"**这张收据是唯一能把两者分开的东西**；没有它就一律按红记，不许当成绿")
        return fails, passed
    ok()

    try:
        data = json.loads(rp.read_text(encoding="utf-8"))
    except Exception as e:  # noqa: BLE001
        fails.append(f"R2 收据 `{rp}` 解析不了（{e}）—— 半张收据不算收据")
        return fails, passed
    if not isinstance(data, dict):
        fails.append(f"R2 收据 `{rp}` 的顶层不是对象")
        return fails, passed
    miss = [k for k in REQUIRED_KEYS if k not in data]
    if miss:
        fails.append(f"R2 收据少了这几个字段：{miss} —— 字段缺了就等于那一维没记，"
                     f"而「没记」与「记着是空的」在下面几条上长得一样 ⇒ 先红在这儿")
        return fails, passed
    ok()

    # R3 这张收据是**这一份**门禁开的吗
    want_sha = sha256_of(gp)
    if data["gate_sha256"] != want_sha:
        fails.append(f"R3 收据里的 `gate_sha256` 与盘上 `{gp}` 现打**对不上**"
                     f"（收据 {str(data['gate_sha256'])[:12]}… vs 现打 {want_sha[:12]}…）—— "
                     f"这张收据是**另一份门禁**开的。改了门禁而没重跑，"
                     f"与「跑过了」在流水线的绿勾上一模一样")
    else:
        ok()

    # R4 这张收据是**这棵树**上跑的吗
    want_tree = head_tree()
    if not want_tree:
        fails.append("R4 现打不出 `git rev-parse HEAD^{tree}` —— 不在 git 仓里，本条判不了（不许当成绿）")
    elif data["tree"] != want_tree:
        fails.append(f"R4 收据里的 `tree` 与现打 `HEAD^{{tree}}` **对不上**"
                     f"（收据 {str(data['tree'])[:12]}… vs 现打 {want_tree[:12]}…）—— "
                     f"那是**别的树**上跑的那一趟。陈收据比没有收据更坏：它看起来像跑过了")
    else:
        ok()

    # R5 工作树脏 ⇒ 这张收据证不了「这棵树跑过门禁」
    if data["dirty"] is not False:
        fails.append("R5 收据记着**工作树是脏的**（`dirty: true`）—— 收据只记得住 `HEAD` 的 tree，"
                     "而那一趟门禁跑的是工作树。两者不同 ⇒ 这张收据证不了「`HEAD` 这棵树跑过门禁」。"
                     "⚠ 本机边跑边改时这一条会红，**那是对的**：要出货就先提交再跑")
    else:
        ok()

    # R6 🔴 反空真锚：`ran ∪ skipped` ↔ `gate.sh` 现打的格名，**两向集合相等**
    text = gp.read_text(encoding="utf-8")
    on_disk = set(found_cells(text))
    ran = set(data["ran"]) if isinstance(data["ran"], list) else set()
    skipped_raw = set(data["skipped"]) if isinstance(data["skipped"], list) else set()
    # `skipped` 记的是**短名**（`GATE_ONLY` 的记号），归一到规范名 —— 与 `C5d`/`C8d` 同一跳
    skipped = {t if t in on_disk else E2E_PREFIX + t for t in skipped_raw}
    covered = ran | skipped
    if covered != on_disk:
        fails.append(f"R6 收据里 `ran ∪ skipped` 与 `{gp}` 现打的格名**对不上**："
                     f"收据有而盘上没有 {sorted(covered - on_disk)} · "
                     f"盘上有而收据没记 {sorted(on_disk - covered)} —— "
                     f"🔴 这是本文件的**反空真锚**：有一格既没跑也没登记成跳过，"
                     f"就是**静默没跑**，而它在门禁自己的输出上一个字都不会说")
    else:
        ok()

    # R7 一格不能既跑了又跳了
    both = ran & skipped
    if both:
        fails.append(f"R7 收据里这几格**既记在 `ran` 又记在 `skipped`**：{sorted(both)} —— "
                     f"两张表本该互斥，重叠说明记的那两处不同源")
    else:
        ok()

    # R8 该跑的真跑了吗
    require = set(require)
    if not require:
        fails.append("R8 `--require` 解出来是**空集** —— 「一格都不要求」与「要求的都跑了」"
                     "在本条上一模一样（空真）。给 `--require` 名单或 `--require-all`")
    else:
        missing = sorted(require - ran)
        if missing:
            fails.append(f"R8 要求真跑过的这几格**不在收据的 `ran` 里**：{missing} —— "
                         f"要么那一趟根本没点它们（`GATE_ONLY` 少写/打错），"
                         f"要么它们在门禁里静默掉了队")
        else:
            ok()
        unknown = sorted(require - on_disk)
        if unknown:
            fails.append(f"R8 要求的这几格**盘上根本不存在**：{unknown} —— "
                         f"要求一格不存在的东西，永远要求不到；这是名单陈了")
        else:
            ok()

    # R9 裁词与那两张表**自洽**
    v = data["verdict"]
    if v not in ("OK", "PARTIAL", "FAIL"):
        fails.append(f"R9 收据里的裁词 {v!r} 不在闭集 OK/PARTIAL/FAIL 里")
    elif v == "FAIL":
        fails.append("R9 收据记着那一趟是 **FAIL** —— 门禁红过，这棵树不许出货")
    elif v == "OK" and skipped:
        fails.append(f"R9 收据自称 **OK**，却记着跳过了 {len(skipped)} 格 —— "
                     f"🔴 `GATE: OK` 只许有一个意思：**盘上每一格都跑过了**。"
                     f"这一形是本条最要挡的：跳着跑却拿到一行 OK")
    elif v == "PARTIAL" and not skipped:
        fails.append("R9 收据自称 **PARTIAL**，却一格都没跳过 —— 两处记的对不上")
    else:
        ok()
    return fails, passed


# ── 阳性对照（`K-R79`/`K-R81`：自检别写成地板）────────────────────────────────
# 🔴 **正反两条都要**：坏的必须被逮到（挡「尺子瞎了」）、好的必须放行（挡「尺子恒红」）。
#   合成收据一律**只在内存里造、落进 `mktemp`**，不碰真收据、不碰工作树。
def selftest():
    import tempfile
    fails, passed = [], 0
    gp = DEFAULT_GATE
    if not gp.is_file():
        return [f"S0 `{gp}` 不在盘上 —— 阳性对照跑不了，本文件判不了（不许当成绿）"], 0
    text = gp.read_text(encoding="utf-8")
    cells = sorted(set(found_cells(text)))
    good = {
        "verdict": "OK",
        "gate_sha256": sha256_of(gp),
        "tree": head_tree(),
        "head": "x",
        "dirty": False,
        "when": "1970-01-01T00:00:00Z",
        "gate_only": "",
        "ran": cells,
        "skipped": [],
    }

    def run(obj):
        with tempfile.TemporaryDirectory() as d:
            f = Path(d) / "r.json"
            f.write_text(json.dumps(obj), encoding="utf-8")
            return judge(f, gp, set(cells))[0]

    def expect_red(tag, obj, why):
        nonlocal passed
        if run(obj):
            passed += 1
        else:
            fails.append(f"{tag} 这把尺子对**{why}**放行了 —— 它瞎了，本文件的绿不算数")

    def expect_green(tag, obj, why):
        nonlocal passed
        bad = run(obj)
        if bad:
            fails.append(f"{tag} 这把尺子对**{why}**也红（{bad[0][:80]}…）—— "
                         f"恒红的尺子和没有尺子一样，会被人关掉")
        else:
            passed += 1

    # S1 阴性对照：一张**真的**收据必须放行
    expect_green("S1", dict(good), "一张完全正确的收据")
    # S2 收据不在 ⇒ 红（这一条走另一条路：直接指一个不存在的路径）
    with tempfile.TemporaryDirectory() as d:
        if judge(Path(d) / "nope.json", gp, set(cells))[0]:
            passed += 1
        else:
            fails.append("S2 收据**根本不在盘上**时这把尺子放行了 —— 那正是「没跑门禁」那一形，"
                         "它是本文件存在的全部理由")
    # S3 门禁换了版本
    expect_red("S3", dict(good, gate_sha256="0" * 64), "另一份门禁开的收据")
    # S4 别的树
    expect_red("S4", dict(good, tree="0" * 40), "别的树上跑的那一趟")
    # S5 工作树脏
    expect_red("S5", dict(good, dirty=True), "工作树脏时开的收据")
    # S6 🔴 反空真：收据被清空 ⇒ 两向分叉
    expect_red("S6", dict(good, ran=[], skipped=[]), "一张 `ran`/`skipped` 都空的收据")
    # S7 静默掉一格（既没跑也没登记成跳过）
    if len(cells) >= 2:
        expect_red("S7", dict(good, ran=cells[1:]), "有一格既没跑、也没登记成跳过")
    # S8 🔴 跳着跑却自称 OK
    if len(cells) >= 2:
        expect_red("S8", dict(good, ran=cells[1:], skipped=[cells[0]]),
                   "跳过了一格却仍然自称 `OK`")
        # S9 同一份内容，裁词改成 PARTIAL ⇒ 必须放行（挡「一见 skipped 就红」）
        #    ⚠ `require` 那一侧要跟着收窄，否则红的是 R8 不是 R9 —— 那就测不到本条
        with tempfile.TemporaryDirectory() as d:
            f = Path(d) / "r.json"
            f.write_text(json.dumps(dict(good, verdict="PARTIAL",
                                         ran=cells[1:], skipped=[cells[0]])), encoding="utf-8")
            bad = judge(f, gp, set(cells[1:]))[0]
            if bad:
                fails.append(f"S9 这把尺子对**一张诚实的 PARTIAL 收据**也红（{bad[0][:80]}…）—— "
                             f"子集跑法从此没人用得了")
            else:
                passed += 1
    # S10 红过的那一趟
    expect_red("S10", dict(good, verdict="FAIL"), "一张裁词是 FAIL 的收据")
    # S11 要求一格盘上没有的东西
    with tempfile.TemporaryDirectory() as d:
        f = Path(d) / "r.json"
        f.write_text(json.dumps(good), encoding="utf-8")
        if judge(f, gp, {"no-such-cell"})[0]:
            passed += 1
        else:
            fails.append("S11 `--require` 里点了一格**盘上不存在**的名字，这把尺子放行了 —— "
                         "永远要求不到的东西，要求它等于没要求")
    return fails, passed


def main(argv=None):
    ap = argparse.ArgumentParser(description="这一趟到底跑没跑门禁（G4 空洞③ 的反空真那一半）")
    ap.add_argument("--receipt", default=str(DEFAULT_RECEIPT))
    ap.add_argument("--gate", default=str(DEFAULT_GATE))
    ap.add_argument("--require", default=None,
                    help="空格分隔的格名（e2e 用套件短名）")
    # `--require-all` 刻意不接受一个数字、也不接受一份名单 —— 它从 `gate.sh` **现算**。
    ap.add_argument("--require-all", action="store_true",
                    help="要求**盘上每一格**都真跑过（格名从 gate.sh 现算）。出货那一趟用它")
    args = ap.parse_args(argv)

    if args.require_all:
        if args.require is not None:
            print("KG4C: FAIL=1")
            print("  ✗ `--require-all` 与 `--require` 同时给了 —— 两份要求必漂，只许给一份")
            return 1
        gp = Path(args.gate)
        if not gp.is_file():
            print("KG4C: FAIL=1")
            print(f"  ✗ `--require-all` 要从 `{gp}` 现算格名，而它盘上不存在")
            return 1
        require = set(found_cells(gp.read_text(encoding="utf-8")))
        src = f"`{gp}` 现算的**全部**格（--require-all）"
    elif args.require is None:
        print("KG4C: FAIL=1")
        print("  ✗ `--require` 与 `--require-all` 一个都没给 —— 「一格都不要求」与「要求的都跑了」分不开")
        return 1
    else:
        on_disk = set(found_cells(Path(args.gate).read_text(encoding="utf-8"))) \
            if Path(args.gate).is_file() else set()
        require = {t if t in on_disk else E2E_PREFIX + t
                   for t in args.require.split() if t}
        src = "--require"

    sfails, spassed = selftest()
    fails, passed = judge(args.receipt, args.gate, require)

    print(f"# `K-G4C` 这一趟跑没跑门禁 —— 收据 `{args.receipt}`")
    print()
    print(f"要求真跑过的格来自 **{src}**，现打 **{len(require)}** 个：{' · '.join(sorted(require))}")
    print()
    all_fails = sfails + fails
    if all_fails:
        print(f"KG4C: FAIL={len(all_fails)}")
        for f in all_fails:
            print(f"  ✗ {f}")
        return 1
    # 🔴 门禁那一格的读数行不在这儿 —— **本文件刻意不是门禁的一格**：
    #   门禁自己跑得了它，就等于「跑了门禁」这件事由门禁自己作证，那是同源恒真。
    print(f"KG4C: OK —— 判过 {passed + spassed} 条"
          f"（真收据 {passed} 条 ＋ 阳性对照 {spassed} 条）；"
          f"反空真锚是 `R6` 那两向集合相等。"
          f"⚠ 它买到的只是「这张收据是这份门禁、这棵树、这一趟开的，且该跑的那几格真跑过」——"
          f"**一个字都没说那几格的判据本身买得到什么**，也没说云端那一趟会绿")
    return 0


if __name__ == "__main__":
    sys.exit(main())
