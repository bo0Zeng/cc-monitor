#!/usr/bin/env python3
"""N-F2 死值验（变异）的量具 —— 一刀一刀地切，切之前先数锚点。

住址（只属于本件，别的 agent 不会同名覆盖）：
    <n-f2 工作树>/evidence/N-F2-mutate.py
被测对象：**本脚本所在工作树**（`--worktree` 缺省 = 本文件的上一级目录），
不接受指向别处的默认值 —— 「量具的被测对象指向哪棵树」要能一眼看出来。

用法：
    python3 evidence/N-F2-mutate.py list
    python3 evidence/N-F2-mutate.py apply N2M1
    python3 evidence/N-F2-mutate.py restore          # git checkout -- 受影响的那几份

纪律（brief 第 7 条）：
  · 每一刀先断言锚点**恰好命中 N 次**，不足或多于就拒绝下刀（不许猜）；
  · 落刀后打印「变异已落地」并把改动后的那一段打出来；
  · 换上去的东西**类型契约必须仍然成立** —— 形状对、恒答其中一张脸，
    而不是让台子炸掉（炸了那不是读数，是 CRASH）。
    ⚠ 本件全部四刀都刻意让 `LOCAL_MACHINE_KEY` 那个 import **仍被用着** ——
      切成「未使用的导入」会让台子红在 lint 上，那不是读数。

一刀 = 一组 (锚点, 期望命中数, 换成什么)。允许一刀切多处（`N2M2` 就是四处一起切），
但**每一处都要各自数锚点**：只数总数会把「四处里漏了一处」读成成功。
"""

import argparse
import pathlib
import subprocess
import sys

UI = "src/settings/accounts-section.ts"

# `N2M6` 切的是这一份 —— 它**在本件写区之外**（写区里只有它的 `.vitest.ts`）。
#
# 为什么还是切它：`NF2D3` 最后那一跳（`if (!summary) { …display = "none"; return; }`）
# 就住在这份文件里，**没有任何在写区内的改动够得到它** ——
# 那两条新判据是直接把账本写绿再渲染的，`accounts-section.ts` 上的任何一刀都碰不到它们。
# ⇒ 不切它，那两条判据就没有死值验，而「一条没证过会红的判据」是本仓判得最重的一种空真。
# 切法是**改一句、当场还原**：`restore` 里点着这份文件，交回时 `git status --short`
# 与 `git diff <基点>` 两个口径都会当场把没还原的情况打红（`NF2D4` 那三个口径就是干这个的）。
# ⚠ 这是实现方的一个判断，已在交回报告里点名报给 PM —— 要是 PM 认为不该碰，这一刀撤掉，
#   那两条判据就只剩测试内那一组正反对照（绿→红→绿），没有源码级的死值验。
SEC = "src/settings/remote-section.ts"

# 本机那四档的写点（`reloadLocal` 里）。缩进是判据的一部分：远端那八行里有
# 逐字相同的句子（`已读取` / `未启用` / `已启用`），只有缩进与 `state.` / `ui.`
# 分得开它们 —— 锚点取宽一点就会连远端那条路一起切，那一刀就不是「最小面」了。
LOCAL_UNREADABLE = (
    '      this.note("accounts", { kind: "fail", detail: "读不动" });\n'
    '      this.note("acctIso", { kind: "fail", detail: "读不动" });'
)
LOCAL_NO_BACKEND = (
    '      this.note("accounts", { kind: "fail", detail: "后端不在" });\n'
    '      this.note("acctIso", { kind: "fail", detail: "后端不在" });'
)
LOCAL_EMPTY = (
    '      this.note("accounts", { kind: "ok", detail: "已读取" });\n'
    '      this.note("acctIso", { kind: "fail", detail: "未启用" });'
)
LOCAL_READY = (
    "    this.note(\"accounts\", { kind: \"ok\", detail: `${state.accounts.length} 个` });\n"
    '    this.note("acctIso", { kind: "ok", detail: "已启用" });'
)
FLAT = (
    '{FILL}this.note("accounts", {{ kind: "ok", detail: "已读取" }});\n'
    '{FILL}this.note("acctIso", {{ kind: "ok", detail: "已读取" }});'
)

# 刀号 -> (文件, [(锚点, 期望命中, 换成什么)], 这一刀该验哪条 / 预期谁红)
CUTS: dict[str, tuple[str, list[tuple[str, int, str]], str]] = {
    "N2M1": (
        UI,
        [
            (
                "    recordFacet(this.origin || LOCAL_MACHINE_KEY, facet, state);",
                1,
                "    if (!this.origin) return; // 变异 N2M1：回到今天的早返回\n"
                "    recordFacet(this.origin || LOCAL_MACHINE_KEY, facet, state);",
            )
        ],
        "NF2D2：把本机那条写点整个去掉（回到今天的早返回）—— 本机那两格该全部回到「没测过」",
    ),
    "N2M2": (
        UI,
        [
            (LOCAL_UNREADABLE, 1, FLAT.format(FILL="      ")),
            (LOCAL_NO_BACKEND, 1, FLAT.format(FILL="      ")),
            (LOCAL_EMPTY, 1, FLAT.format(FILL="      ")),
            (LOCAL_READY, 1, FLAT.format(FILL="    ")),
        ],
        "NF2D2：三档都写成同一个值（恒 ok「已读取」）—— 只断「调了 recordFacet」的判据不会红",
    ),
    "N2M3": (
        UI,
        [(LOCAL_READY, 1, "")],
        "NF2D3：只把「读出来了·有号」那一档的两行写点去掉 ——"
        " 本机那两格回到 unknown ⇒「全绿就整块不出现」又走不到（**最小面那一刀**）",
    ),
    "N2M4": (
        UI,
        [
            (
                '      this.note("accounts", { kind: "fail", detail: "拉取失败" });',
                1,
                '      this.note("accounts", { kind: "fail", detail: "失败" });',
            )
        ],
        "NF2D2 两侧：顺手改一行**远端**那条路的写点 —— 只断本机那一侧的判据不会红",
    ),
    "N2M6": (
        SEC,
        [
            (
                '      this.gapsBox.style.display = "none";',
                1,
                '      this.gapsBox.style.display = "";',
            )
        ],
        "NF2D3 最后那一跳：把「整块不出现」那一句打掉（分支照跑照 return，只是不再藏）"
        " —— 只有 remote-section.vitest.ts 那两条新判据够得到它",
    ),
    "N2M5": (
        UI,
        [
            (
                '        "accounts-info accounts-local-empty-title",',
                1,
                '        "accounts-info accounts-local-empty-title n2m5",',
            )
        ],
        "NF2D4：改一行生产代码但**不提交** —— 逐字口径应判绿、工作树口径应判红",
    ),
}


def worktree(arg: str | None) -> pathlib.Path:
    return pathlib.Path(arg).resolve() if arg else pathlib.Path(__file__).resolve().parent.parent


def cmd_list(_args: argparse.Namespace) -> int:
    for name, (rel, edits, why) in CUTS.items():
        want = " + ".join(str(n) for _, n, _ in edits)
        print(f"{name:6s} {rel:36s} 锚点应命中 {want} 次  —— {why}")
    return 0


def cmd_apply(args: argparse.Namespace) -> int:
    wt = worktree(args.worktree)
    rel, edits, why = CUTS[args.cut]
    path = wt / rel
    src = path.read_text(encoding="utf-8")
    print(f"树       : {wt}")
    print(f"刀       : {args.cut} —— {why}")
    print(f"文件     : {rel}")
    for i, (anchor, want, new) in enumerate(edits, 1):
        hits = src.count(anchor)
        head = anchor.splitlines()[0].strip()
        print(f"锚点 {i}/{len(edits)} : 命中 {hits}（期望 {want}）  ← {head}")
        if hits != want:
            print("锚点命中数对不上 —— **不下刀**。锚点漂了就先修锚点，别猜。", file=sys.stderr)
            return 3
        src = src.replace(anchor, new)
    path.write_text(src, encoding="utf-8")
    print("变异已落地")
    subprocess.run(["git", "-C", str(wt), "--no-pager", "diff", "--", rel], check=False)
    return 0


def cmd_restore(args: argparse.Namespace) -> int:
    wt = worktree(args.worktree)
    files = sorted({rel for rel, *_ in CUTS.values()})
    subprocess.run(["git", "-C", str(wt), "checkout", "--", *files], check=True)
    out = subprocess.run(
        ["git", "-C", str(wt), "status", "--short"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    print("已还原；git status --short 现在是：")
    print(out if out.strip() else "  （干净）")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--worktree", default=None, help="被测工作树；缺省 = 本文件的上一级目录")
    sub = ap.add_subparsers(dest="how", required=True)
    sub.add_parser("list").set_defaults(fn=cmd_list)
    a = sub.add_parser("apply")
    a.add_argument("cut", choices=sorted(CUTS))
    a.set_defaults(fn=cmd_apply)
    sub.add_parser("restore").set_defaults(fn=cmd_restore)
    args = ap.parse_args()
    return int(args.fn(args))


if __name__ == "__main__":
    raise SystemExit(main())
