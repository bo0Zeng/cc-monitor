#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R116 `KR116D1` —— 「`daemon` 这个词出现几次」与「该改几处」是**两个数**，这把尺子把它们分开。

## 它治的是什么

`R61`〔用@09-13〕逐字：「**不要有 daemon 这个说法了 / daemon 就是常驻后端，后端就是 daemon**」。
`R63` 要「全仓改措辞」，`R64` 又逐字收窄「**ccm 不改**」。

⇒ 要改的是**人读的散文**里那个词，**不是代码标识符**：
`remote-daemon-proto` 这个 crate 名、`daemon_*` 函数名、`--daemon-probe` 这类子命令、
`daemon-gate2` 这类 e2e 套件名 —— **每一次出现都会被 `grep` 数进去，而它们一个都不改**。

派工单里那个 **1828** 是 `K-R107` 09-13 拿 `grep` 数出来的**出现次数**，
把它当工作量就是把「不许改的」与「该改的」压成一个数。**本尺子的全部意义是把它们拆开。**

## 分母怎么数的

- **人群**：`git ls-files '*.md'`（**被跟踪的 markdown**，不含 `node_modules` / 未跟踪文件）。
  这个数**现打**，不写死 —— 每加一份 `.md` 它就变。
- **单位是「出现次数」**，不是「行数」：一行里两处算两处（`grep -c` 数的是行，两个数不一样是对的）。
- **大小写不敏感**（`daemon` / `Daemon` / `DAEMON` 都数）。

## 每一处的裁词（闭集，现算见 `VERDICTS`）

  · `冻结·不许改`   落在 `evidence/**` 或 `CHANGELOG.md` —— 那是「某年某月现打是多少」的
                    历史读数与墓碑，**改错了改不回来**（`KR116D2` 守的就是这一档）
  · `写区外·未改`   被跟踪的 `.md`，但不在本轮 `.dispatch.json` 的写区里 ⇒ 本轮不动，**点名**
  · `标识符·不改`   写区内，而这一处的 ASCII token **不是**光秃秃的 `daemon`
                    （`remote-daemon-proto` · `daemon_send_keys.rs` · `--daemon-probe` · `daemonPath`…）
  · `登记例外·不改` 写区内的裸词，但命中 `EXCEPTIONS` 登记表 —— 逐条带理由
                    （用户逐字裁定 · 历史原措辞留档 · markdown 锚点 · 命令行占位符 · CI job 名）
  · `该改`          其余。**这个数就是工作量。**

## 🔴 token 怎么切（「出现次数」与「该改几处」的分水岭就在这里）

从命中处向两侧扩：ASCII 字母 / 数字 / `_` 一律吃进来；`-` `.` `/` `:` 只有在**它后面（前面）
紧跟着 ASCII 标识符字符**时才吃。**汉字不算标识符字符** —— 于是 `ccm做到必须走daemon`
切出来是裸 `daemon`（它是用户逐字，走 `EXCEPTIONS`），而 `remote-daemon-proto` 切出来是整条。

⚠ 这条规则**认不出**的：把 `daemon` 写成中文夹缝里的标识符（`daemon-协议-v1` 这一形，
`-` 后面是汉字 ⇒ 切成裸词）。这一形今天靠 `EXCEPTIONS` 逐条兜，**不是靠 token 规则**。
两侧都写出来：这是**已知的漏**，不是「没有」。

## ⚠ 诚实边界（别把绿读宽）

1. **只看 `.md`**。`.rs` / `.ts` / `.sh` 里的散文注释本尺子一眼都没看
   （`R61` 只改人读的散文那一面，而源码注释归各自那件；本轮写区里也没有它们）。
2. **它不判「改得对不对」** —— 只判「这一处该不该动」。改完读起来通不通顺是评审的活。
3. **`--apply` 只动 `该改` 那一档**，其余四档一个字节都不碰。改完再跑一次默认模式，
   `该改` 应当归零 —— 那是本尺子自带的收尾自检（`--verify`）。
4. 它**不是**门禁。门禁那一侧的两道闸住 `src-tauri/src/doc_claim_registry.rs`
   （`daemon_wording_registry` 与 `frozen_daemon_census`），跟着 `cargo` 那一格跑。
   **写区那 9 份与例外表都从那两道闸里解析出来** —— 本尺子不存第二份。

## 跑法

    python3 evidence/K-R116-ruler.py              # 普查：逐档 + 逐文件
    python3 evidence/K-R116-ruler.py --per-hit    # 再加逐处（很长）
    python3 evidence/K-R116-ruler.py --apply      # 照同一把分类器改写写区那几份
    python3 evidence/K-R116-ruler.py --verify     # 收尾自检：写区里 `该改` 必须是 0

🔴 **分类与改写共用同一个函数** —— 一个闭集只许有一个住址（`brief` 13b）：
「我数成该改的」与「我真改掉的」不可能对不上，因为它们是同一段代码。
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(
    subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True,
        text=True,
        check=True,
        cwd=Path(__file__).resolve().parent,
    ).stdout.strip()
)

# ── 闭集：裁词 ───────────────────────────────────────────────────────────────
VERDICTS = ["冻结·不许改", "写区外·未改", "标识符·不改", "登记例外·不改", "该改"]

# ── 🔴 闭集不在这里 —— 它住门禁那一侧 ────────────────────────────────────────
#
# `WRITE_ZONE`（本轮改哪 9 份）与 `EXCEPTIONS`（写区里哪几处裸词不许动）这**两个闭集**
# 的唯一住址是 `src-tauri/src/doc_claim_registry.rs::daemon_wording_registry` 的
# `SITES` 与 `EXEMPT`（`brief` 13b：一个闭集只许有一个住址，而**闸**那一侧才是承重的）。
# 本尺子**解析它们**，不抄第二份 ⇒ 「我数成该改的」与「门禁认的」不可能对不上。
JUDGE = "src-tauri/src/doc_claim_registry.rs"

_LIT = re.compile(r'"((?:[^"\\]|\\.)*)"')


def _judge_table(head: str) -> list[str]:
    """从判据源码里把 `const <名> … = &[ … ];` 那一块的字符串字面量逐个取出来。"""
    src = (ROOT / JUDGE).read_text(encoding="utf-8")
    if src.count(head) != 1:
        raise SystemExit(f"🔴 {JUDGE} 里 {head!r} 命中 {src.count(head)} 次（要求 1 次）")
    body = src[src.index(head) + len(head) :]
    stop = body.index("\n    ];")
    return [m.group(1) for m in _LIT.finditer(body[:stop])]


def load_write_zone() -> list[str]:
    v = _judge_table("const SITES: &[&str] = &[")
    if len(v) < 5:
        raise SystemExit(f"🔴 从判据里只解析出 {len(v)} 份写区 —— 解析器坏了，本尺子在空转")
    return v


def load_exceptions() -> list[tuple[str, str, str]]:
    v = _judge_table("const EXEMPT: &[(&str, &str, &str)] = &[")
    if len(v) % 3 or len(v) < 9:
        raise SystemExit(f"🔴 例外表解析出 {len(v)} 个字面量（要求 3 的倍数且够多）—— 解析器坏了")
    return [(v[i], v[i + 1], v[i + 2]) for i in range(0, len(v), 3)]


WRITE_ZONE = load_write_zone()

# ── 冻结面：`KR116D2` 那两档 ─────────────────────────────────────────────────
FROZEN_DIRS = ("evidence/",)
FROZEN_FILES = ("CHANGELOG.md",)

EXCEPTIONS = load_exceptions()

# ── 登记的整句改写：一般规则会撞车的那几处，逐句登记 ──────────────────────────
#
# `(文件, 原文逐字, 改成, 理由)`。在一般规则之前跑；每条必须**恰好命中一次**（有自检）。
REWRITES = [
    (
        "remote-daemon-proto/README.md",
        "cc-monitor 的 SSH-远端功能后端 daemon（issue #15 起",
        "cc-monitor 的 SSH-远端功能后端（issue #15 起",
        "一般规则会产出「后端后端」—— 这一处原文里「后端 daemon」本来就是同义反复",
    ),
    (
        "README.md",
        "daemon/client build_id 不符",
        "后端/client build_id 不符",
        "`daemon/client` 这一处是散文里的「两端」写法，不是路径 —— token 规则按 `/` 把它切成一整条，"
        "会误判成标识符（这是那条规则的已知漏，逐处登记，不去放宽规则）",
    ),
    (
        "README.en.md",
        "a daemon/client build_id mismatch",
        "a backend/client build_id mismatch",
        "同上，英文那份里的同一句",
    ),
    (
        "README.en.md",
        "daemon-spawned plugins no longer inherit",
        "backend-spawned plugins no longer inherit",
        "英文连字符构成的**形容词**（`daemon-spawned`），不是标识符 —— token 规则同样把它整条吃掉了",
    ),
    (
        "doc/CONTRIBUTING.md",
        "后端 / 远端 daemon / e2e 冒烟是**各自独立的 job**",
        "本机后端 / 远端后端 / e2e 冒烟是**各自独立的 job**",
        "一般规则会产出「后端 / 远端后端」，前一个「后端」指的是本机那一份 ⇒ 点明",
    ),
]

CJK = re.compile(r"[　-〿一-鿿＀-￯]")
HIT = re.compile(r"(?i)daemon")


def _idc(c: str) -> bool:
    """ASCII 标识符字符 —— **汉字不算**，这一条是两个数的分水岭。"""
    return c.isascii() and (c.isalnum() or c == "_")


def token_at(s: str, a: int, b: int) -> tuple[int, int, str]:
    """把 `[a, b)` 这处命中扩成它所属的 ASCII token，返回 `(起, 止, 文本)`。"""
    while a > 0 and (_idc(s[a - 1]) or (s[a - 1] in "-./:" and a - 1 > 0 and _idc(s[a - 2]))):
        a -= 1
    while b < len(s) and (_idc(s[b]) or (s[b] in "-./:" and b + 1 < len(s) and _idc(s[b + 1]))):
        b += 1
    return a, b, s[a:b]


def tracked_md() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "-z", "*.md"], capture_output=True, cwd=ROOT, check=True
    ).stdout.decode()
    return sorted(f for f in out.split("\0") if f)


def exception_spans(rel: str, text: str) -> list[tuple[int, int, str]]:
    """本文件登记的例外片段在文中的区间。片段必须唯一命中，否则当场报错。"""
    spans = []
    for f, frag, why in EXCEPTIONS:
        if f != rel:
            continue
        n = text.count(frag)
        if n != 1:
            raise SystemExit(
                f"🔴 例外登记片段在 {rel} 里命中 {n} 次（要求恰好 1 次）：{frag!r}\n"
                f"   命中 0 次 = 那句话被改过了，这条例外此刻在空转；"
                f"命中多次 = 片段太短，说不清点的是哪一处。"
            )
        i = text.index(frag)
        spans.append((i, i + len(frag), why))
    return spans


def rewrite_spans(rel: str, text: str) -> list[tuple[int, int, str]]:
    """整句改写登记在文中的区间。

    改写之前每条恰好命中 1 次；**改写之后命中 0 次**（那正是它跑过了的样子）⇒ 0 次放行。
    命中 2 次以上 ⇒ 报错：说不清点的是哪一处。
    """
    spans = []
    for f, old, _new, why in REWRITES:
        if f != rel:
            continue
        n = text.count(old)
        if n > 1:
            raise SystemExit(f"🔴 整句改写登记在 {rel} 里命中 {n} 次（要求 1 次）：{old!r}")
        if n == 1:
            i = text.index(old)
            spans.append((i, i + len(old), why))
    return spans


def classify(rel: str, text: str):
    """逐处出裁词。返回 `[(起, 止, token, 裁词, 理由)]`。"""
    frozen = rel.startswith(FROZEN_DIRS) or rel in FROZEN_FILES
    in_zone = rel in WRITE_ZONE
    spans = exception_spans(rel, text) if in_zone else []
    rw = rewrite_spans(rel, text) if in_zone else []
    rows = []
    for m in HIT.finditer(text):
        a, b, tok = token_at(text, m.start(), m.end())
        if frozen:
            rows.append((a, b, tok, "冻结·不许改", "历史读数 / 墓碑"))
        elif not in_zone:
            rows.append((a, b, tok, "写区外·未改", "被跟踪的 .md，但不在本轮写区"))
        elif any(x <= a < y for x, y, _ in rw):
            # 🔴 整句改写登记压在 token 规则**前面** —— 那三处正是 token 规则误判成标识符的
            #    散文写法（`daemon/client` · `daemon-spawned`）。压在后面的话，
            #    「普查说该改 357」与「改写真动了 360」会对不上，而那正是本件治的病。
            why = next(w for x, y, w in rw if x <= a < y)
            rows.append((a, b, tok, "该改", f"整句改写登记：{why}"))
        elif tok.lower() != "daemon":
            rows.append((a, b, tok, "标识符·不改", "复合 ASCII 标识符"))
        else:
            why = next((w for x, y, w in spans if x <= a < y), None)
            if why:
                rows.append((a, b, tok, "登记例外·不改", why))
            else:
                rows.append((a, b, tok, "该改", "人读的散文"))
    return rows


def replacement(rel: str, tok: str) -> str:
    """裸词换成什么 —— 英文那份换英文词，别的换「后端」。"""
    if rel.endswith(".en.md"):
        return "Backend" if tok[0].isupper() else "backend"
    return "后端"


def rewrite(rel: str, text: str) -> tuple[str, int]:
    """把 `该改` 那一档换掉。返回 `(新文本, 改了几处)`。"""
    n = 0
    for f, old, new, _why in REWRITES:
        if f != rel:
            continue
        c = text.count(old)
        if c != 1:
            raise SystemExit(f"🔴 整句改写登记在 {rel} 里命中 {c} 次（要求 1 次）：{old!r}")
        text = text.replace(old, new)
        n += old.lower().count("daemon")
    rows = classify(rel, text)  # 整句改写已落地 ⇒ 那几处此刻不在 `该改` 里了
    for a, b, tok, verdict, _ in reversed(rows):
        if verdict != "该改":
            continue
        rep = replacement(rel, tok)
        head, tail = text[:a], text[b:]
        # 中文里换上汉字之后，紧贴的那一个半角空格是多余的（`远端 daemon 的` → `远端后端的`）
        if not rel.endswith(".en.md"):
            if head.endswith(" ") and CJK.search(head[-2:-1] or ""):
                head = head[:-1]
            if tail.startswith(" ") and CJK.search(tail[1:2] or ""):
                tail = tail[1:]
        text = head + rep + tail
        n += 1
    return text, n


def census(per_hit: bool):
    files = tracked_md()
    tally = {v: 0 for v in VERDICTS}
    per_file = []
    for rel in files:
        text = (ROOT / rel).read_text(encoding="utf-8")
        rows = classify(rel, text)
        if not rows:
            continue
        c = {v: 0 for v in VERDICTS}
        for _, _, _, v, _ in rows:
            c[v] += 1
            tally[v] += 1
        per_file.append((rel, len(rows), c, rows, text))

    total = sum(tally.values())
    print("=" * 100)
    print("K-R116 `KR116D1` —— 「daemon 出现几次」 vs 「该改几处」")
    print("=" * 100)
    print(f"· 被测树   ：{ROOT}")
    print(f"· 人群     ：`git ls-files '*.md'` 现打 {len(files)} 份；有命中的 {len(per_file)} 份")
    print(f"· 单位     ：**出现次数**（大小写不敏感），不是行数")
    print(f"· 裁词闭集 ：{len(VERDICTS)} 档 —— {' / '.join(VERDICTS)}")
    print("-" * 100)
    print(f"🔴 出现次数（`grep` 数得到的那个数）：{total}")
    for v in VERDICTS:
        pct = 100.0 * tally[v] / total if total else 0.0
        print(f"     {v:<14} {tally[v]:>5}  {pct:5.1f}%")
    print(f"🔴 **该改几处** ：{tally['该改']}  ← 这个数才是工作量")
    print(f"   两个数差    ：{total} − {tally['该改']} = {total - tally['该改']}")
    print("-" * 100)
    print(f"{'文件':<52}{'合计':>6}" + "".join(f"{v[:4]:>8}" for v in VERDICTS))
    for rel, n, c, _, _ in per_file:
        print(f"{rel:<52}{n:>6}" + "".join(f"{c[v]:>8}" for v in VERDICTS))
    print("-" * 100)
    if per_hit:
        for rel, _, _, rows, text in per_file:
            if not any(v == "该改" for _, _, _, v, _ in rows):
                continue
            print(f"## {rel}")
            for a, b, tok, v, why in rows:
                if v == "该改":
                    ln = text.count("\n", 0, a) + 1
                    ctx = text[max(0, a - 30) : b + 24].replace("\n", "⏎")
                    print(f"   {v:<12} :{ln:<5} …{ctx}…")
        print("-" * 100)
    return tally


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--per-hit", action="store_true", help="逐处印出 `该改` 那一档")
    ap.add_argument("--apply", action="store_true", help="改写写区那几份")
    ap.add_argument("--verify", action="store_true", help="收尾自检：写区里 `该改` 必须是 0")
    a = ap.parse_args()

    if a.apply:
        tot = 0
        for rel in WRITE_ZONE:
            p = ROOT / rel
            old = p.read_text(encoding="utf-8")
            new, n = rewrite(rel, old)
            if new != old:
                # 🔴 刻意**不用** `shutil` 那一族写回（`K-R115` 那条纪律：别把旧 mtime 搬回来）
                p.write_text(new, encoding="utf-8")
            print(f"  {rel:<44} 改 {n:>4} 处")
            tot += n
        print(f"合计改了 {tot} 处")
        return 0

    tally = census(a.per_hit)
    if a.verify:
        if tally["该改"] != 0:
            print(f"KR116D1: FAIL —— 写区里还剩 {tally['该改']} 处 `该改` 没动")
            return 1
        print("KR116D1: OK —— 写区里 `该改` 归零")
    print(f"KR116-census: {sum(tally.values())} passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
