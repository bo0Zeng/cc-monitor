#!/usr/bin/env python3
"""N-F1c 死值验（变异）的量具 —— 一刀一刀地切，切之前先数锚点。

住址（只属于本件，别的 agent 不会同名覆盖）：
    <n-f1c 工作树>/evidence/N-F1c-mutate.py
被测对象：**本脚本所在工作树**（`--worktree` 缺省 = 本文件的上一级目录），
不接受指向别处的默认值 —— 「量具的被测对象指向哪棵树」要能一眼看出来。

用法：
    python3 evidence/N-F1c-mutate.py list
    python3 evidence/N-F1c-mutate.py apply NcM2
    python3 evidence/N-F1c-mutate.py restore          # git checkout -- 受影响的那几份

纪律（brief 第 7 条）：
  · 每一刀先断言锚点**恰好命中 N 次**，不足或多于就拒绝下刀（不许猜）；
  · 落刀后打印「变异已落地」并把改动后的那一段打出来；
  · 换上去的东西**类型契约必须仍然成立** —— 形状对、恒答其中一张脸，
    而不是让台子炸掉（炸了那不是读数，是 CRASH）。
"""

import argparse
import pathlib
import subprocess
import sys

MONITOR = "src-tauri/src/local_accounts.rs"
DAEMON = "remote-daemon-proto/src/observe/accounts_query.rs"
UI = "src/settings/accounts-section.ts"

# (刀号, 文件, 锚点, 期望命中次数, 替换成什么, 这一刀该验哪条 / 预期谁红)
CUTS = {
    "NcM1": (
        MONITOR,
        """        classify_local_accounts(run_query(env!("CCM_TARGET_TRIPLE"), &["--list-accounts"]))
            .into_result()""",
        1,
        """        list_from_dir(&local_accts_dir().unwrap_or_default())""",
        "NF1cD1：读口改回直读磁盘（返回类型仍是 AccountsResult ⇒ 台子不炸）",
    ),
    "NcM2": (
        DAEMON,
        "    if !looks_absolute(p) {",
        1,
        "    if !p.starts_with('/') {",
        "NF1cD2：路径检查退回 starts_with('/')（Windows 形状那几格该红）",
    ),
    "NcM3": (
        DAEMON,
        """            || matches!(
                c,
                '\\'' | '"' | '`' | '$' | ';' | '|' | '&' | '<' | '>' | '*' | '?' | '(' | ')' | '!'
            )
""",
        1,
        "",
        "NF1cD2：只放宽绝对路径那一半，顺手把 shell 元字符也放过（危险形状那几格该红）",
    ),
    "NcM4": (
        MONITOR,
        """            Self::NoBackend(_) | Self::Unreadable(_) => AccountsResult {
                available: false,
                error: Some(copy),
                meta: None,
                accounts: Vec::new(),
                notice: None,
            },""",
        1,
        """            Self::NoBackend(_) => AccountsResult {
                available: true,
                error: None,
                meta: None,
                accounts: Vec::new(),
                notice: None,
            },
            Self::Unreadable(_) => AccountsResult {
                available: false,
                error: Some(copy),
                meta: None,
                accounts: Vec::new(),
                notice: None,
            },""",
        "NF1cD3：「后端不在」那一支改成渲染空列表（够不着被说成「你没有账号」）",
    ),
    "NcM5": (
        UI,
        "        LOCAL_ACCOUNTS_COPY.emptyTitle,",
        1,
        '        "没有账号",',
        "NF1cD4：顺手改一行 accounts-section.ts（12 格里该有红，且 diff 不再为空）",
    ),
    "NcM6": (
        MONITOR,
        "        QueryOutcome::NoBackend(reason) => return LocalAccountsOutcome::NoBackend(reason),",
        1,
        "        QueryOutcome::NoBackend(reason) => return LocalAccountsOutcome::Unreadable(reason),",
        "NF1cD5：改一行生产代码但**不提交**（逐字口径该绿、工作树口径该红）",
    ),
    # ---- `7u`：把实现**整个退掉**，看还有几条新断言仍绿 ----
    # 退法要让台子仍然编得过（炸了那不是读数是 CRASH）⇒ 只退**行为**，不动签名。
    # daemon 那一侧退干净要三刀：NcM2（形式那一半）+ NcU1 + NcU2（性质那一半的两处）。
    "NcU1": (
        DAEMON,
        "                '\\'' | '\"' | '`' | '$' | ';' | '|' | '&' | '<' | '>' | '*' | '?' | '(' | ')' | '!'",
        1,
        "                '\\'' | '\"' | '\\\\' | '`' | '$' | ';' | '|' | '&' | '<' | '>' | '*' | '?' | '(' | ')' | '!'",
        "7u：把 `\\` 放回元字符表（退回 `N-F1c` 之前那一份）",
    ),
    "NcU2": (
        DAEMON,
        """    if p.contains("\\\\..\\\\") || p.ends_with("\\\\..") {
        return false;
    }
""",
        1,
        "",
        "7u：拿掉按反斜杠的上跳检查（退回 `N-F1c` 之前那一份）",
    ),
    # 自加的一刀（不在件计划 §3 那六行里，读数落在 §3a）：
    "NcM7": (
        MONITOR,
        "        Some(meta) => LocalAccountsOutcome::Listed { meta, accounts },",
        1,
        """        Some(meta) => LocalAccountsOutcome::Listed {
            meta,
            accounts: Vec::new(),
        },""",
        "NF1cD1 的正向读数有没有牙：解析对了但把账号丢光（形状对、恒答空表）",
    ),
}


def worktree(arg: str | None) -> pathlib.Path:
    return pathlib.Path(arg).resolve() if arg else pathlib.Path(__file__).resolve().parent.parent


def cmd_list(_args: argparse.Namespace) -> int:
    for name, (rel, _anchor, n, _new, why) in CUTS.items():
        print(f"{name:6s} {rel:48s} 锚点应命中 {n} 次  —— {why}")
    return 0


def cmd_apply(args: argparse.Namespace) -> int:
    wt = worktree(args.worktree)
    rel, anchor, want, new, why = CUTS[args.cut]
    path = wt / rel
    src = path.read_text(encoding="utf-8")
    hits = src.count(anchor)
    print(f"树       : {wt}")
    print(f"刀       : {args.cut} —— {why}")
    print(f"文件     : {rel}")
    print(f"锚点命中 : {hits}（期望 {want}）")
    if hits != want:
        print("❌ 锚点命中数对不上 —— **不下刀**。锚点漂了就先修锚点，别猜。", file=sys.stderr)
        return 3
    path.write_text(src.replace(anchor, new), encoding="utf-8")
    print("变异已落地")
    subprocess.run(
        ["git", "-C", str(wt), "--no-pager", "diff", "--stat", "--", rel],
        check=False,
    )
    return 0


def cmd_combo(args: argparse.Namespace) -> int:
    for cut in args.cuts:
        one = argparse.Namespace(worktree=args.worktree, cut=cut)
        rc = cmd_apply(one)
        if rc != 0:
            print(f"❌ `{cut}` 没落下去 —— 整组作废，别把半组的读数当读数。", file=sys.stderr)
            return rc
        print("-" * 60)
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
    c = sub.add_parser("combo", help="按顺序连下几刀（`7u` 用它把实现整个退掉）")
    c.add_argument("cuts", nargs="+", choices=sorted(CUTS))
    c.set_defaults(fn=cmd_combo)
    sub.add_parser("restore").set_defaults(fn=cmd_restore)
    args = ap.parse_args()
    return args.fn(args)


if __name__ == "__main__":
    raise SystemExit(main())
