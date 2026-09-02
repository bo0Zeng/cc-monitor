#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R4 量具：扫「判据的诊断文本里手抄的数 ≠ 它实际断言的数」。

被测对象 = 本量具所在工作树的仓根（`evidence/` 的父目录），不接受外部路径参数
——住址与被测对象绑死，避免同名量具指向另一棵树（brief 第 12 条 5k）。

## 切法（本量具圈的「一处」）

一处 = **一个判据点**（Rust 的一次 assert 宏调用 / TS 的一次 `expect(...)` 链
/ 一个 `it("...")` 标题），它的**诊断文本里出现一个整数字面量**，而该整数
**不在这个判据点实际断言的整数集合里**。

- Rust：`assert_eq!(a, b, "...{}...")` ⇒ 断言集 = a、b 里的整数字面量；
  诊断集 = 第三个参数起的格式串里的整数字面量。
  `assert!(cond, "...")` ⇒ 断言集 = cond 里的整数字面量。
- TS：`expect(x, "msg").toBe(N)` ⇒ 断言集 = {N}；诊断集 = msg 里的整数。
  `it("标题", ...)` ⇒ 诊断集 = 标题里的整数；断言集 = 该 it 体内所有
  `.toBe(N)/.toHaveLength(N)/.toBeGreaterThan(N)/.toBeLessThan(N)` 的 N。

## 输出的两种行（**只输出这两种**）

- `NUM`  ：诊断里的数 ≠ 断言里的数（本件正面要治的形）
- `PRED` ：诊断里**一个数都没有**，但它用「一个都没 / 没有任何 / 只剩」这类
           **零/全称谓词**描述，而断言是个阈值 ⇒ 谓词与断言不同族

⚠ **对得上的那些判据点不出行** —— 它们只进分母（表头那三个数）。
  本文件初版的这一段写着「三档输出：NUM / PRED / **OK**」，而代码**从来没有**
  产出过 `OK` 行 —— **一份治「说明与实际对不上」的量具，说明与实际对不上**。
  订正于 09-01（`K-R4` 收工前按铁律 15 回头打自己的代码时逮到）。

## 三个数各是什么（别混用）

- **分母** = 带诊断文本的**判据点**个数（表头那三个：rust / ts expect / ts it 标题）。
- **一档命中** = 诊断里有个整数不在断言的整数集合里 —— **噪声极大**，
  issue 号 / 日期 / 章节号 / 门号 / 字节窗口一律会进来（本量具不认识它们）。
- **二档命中** = 那个整数还坐在「期望值」位（`应为` / `恰好` / `期望` / `实得` / `==` …）。

🔴 **一档与二档都是「候选行数」，不是「缺陷数」** —— 要逐条人工判。
   09-01 现打：二档 19 行里真病 6 处（Rust 14→2 · TS 5→4）。
"""

import re
import sys
import json
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

SKIP_DIRS = {"node_modules", "target", ".git", "dist", "gen"}


def walk(exts):
    for p in sorted(REPO.rglob("*")):
        if not p.is_file():
            continue
        if p.suffix not in exts:
            continue
        if any(part in SKIP_DIRS for part in p.relative_to(REPO).parts):
            continue
        yield p


# ---------------------------------------------------------------- Rust 切分

MACROS = ("assert_eq!", "assert_ne!", "assert!", "debug_assert!", "debug_assert_eq!")


def split_top_level(src, start):
    """src[start] 是 '('，返回 (args, end_index_of_close_paren)。

    正确跨过：普通串（含 \\ 转义与行尾续行）、原始串 r#"..."#、字符字面量、
    行注释、块注释、以及嵌套的 ()/[]/{}。
    """
    i = start + 1
    depth = 0
    args = []
    cur = []
    n = len(src)
    while i < n:
        c = src[i]
        # 原始串
        if c == "r" and i + 1 < n and src[i + 1] in '#"':
            j = i + 1
            hashes = 0
            while j < n and src[j] == "#":
                hashes += 1
                j += 1
            if j < n and src[j] == '"':
                close = '"' + "#" * hashes
                k = src.find(close, j + 1)
                if k == -1:
                    return None, None
                cur.append(src[i : k + len(close)])
                i = k + len(close)
                continue
        if c == '"':
            j = i + 1
            while j < n:
                if src[j] == "\\":
                    j += 2
                    continue
                if src[j] == '"':
                    break
                j += 1
            if j >= n:
                return None, None
            cur.append(src[i : j + 1])
            i = j + 1
            continue
        if c == "'":
            m = re.match(r"'(?:\\.|[^\\'])'", src[i:])
            if m:
                cur.append(m.group(0))
                i += m.end()
                continue
            # 生命周期 'a —— 当普通字符走
        if c == "/" and i + 1 < n and src[i + 1] == "/":
            j = src.find("\n", i)
            j = n if j == -1 else j
            i = j
            continue
        if c == "/" and i + 1 < n and src[i + 1] == "*":
            j = src.find("*/", i + 2)
            if j == -1:
                return None, None
            i = j + 2
            continue
        if c in "([{":
            depth += 1
            cur.append(c)
            i += 1
            continue
        if c in ")]}":
            if depth == 0 and c == ")":
                args.append("".join(cur))
                return args, i
            depth -= 1
            cur.append(c)
            i += 1
            continue
        if c == "," and depth == 0:
            args.append("".join(cur))
            cur = []
            i += 1
            continue
        cur.append(c)
        i += 1
    return None, None


INT_RE = re.compile(r"(?<![\w.])(\d[\d_]*)(?![\w.])")


def ints(text):
    out = []
    for m in INT_RE.finditer(text):
        v = m.group(1).replace("_", "")
        out.append(int(v))
    return out


STR_LIT_RE = re.compile(r'"(?:\\.|[^"\\])*"', re.S)


def strings_in(text):
    return " ".join(STR_LIT_RE.findall(text))


ZERO_PRED = [
    "一个都没", "一条都没", "一处都没", "一个也没", "没有任何", "都没扫到",
    "是空的", "空集", "只剩", "坏了", "在空转",
]

# ---- 二档筛：那个对不上的整数，是不是坐在「这条判据期望的值」那个位置上 ----
#
# 一档（NUM）只判「诊断里有个数不在断言里」——噪声极大（issue 号 · 日期 ·
# 章节号 · 场景描述里的量都会进来）。本件治的病是**报文在陈述「应该是几」**，
# 所以二档要求那个整数前面挂着一个**断言语**。
EXPECT_POS = re.compile(
    r"(?:真实应为|应当为|应该是|应为|期望恰好|期望|预期|恰好|共计|总数|计数|"
    r"实得|唯一名数|==|＝＝|exactly|expected|should\s+be)"
    r"[^\d\n]{0,12}?(\d[\d_]*)"
)


def expected_position_ints(msg):
    return set(int(m.group(1).replace("_", "")) for m in EXPECT_POS.finditer(msg))


DENOM = {"rs_points": 0, "ts_expect_points": 0, "ts_it_titles": 0}


def scan_rust():
    rows = []
    for p in walk({".rs"}):
        src = p.read_text(encoding="utf-8", errors="replace")
        for mac in MACROS:
            idx = 0
            while True:
                k = src.find(mac, idx)
                if k == -1:
                    break
                idx = k + len(mac)
                # 前一个字符不能是标识符字符（避开 debug_assert! 命中 assert!）
                if k > 0 and (src[k - 1].isalnum() or src[k - 1] == "_"):
                    continue
                popen = k + len(mac)  # 宏名以 '!' 收尾，紧跟的就是 '('
                if popen >= len(src) or src[popen] != "(":
                    continue
                args, end = split_top_level(src, popen)
                if args is None:
                    continue
                line = src.count("\n", 0, k) + 1
                if mac.endswith("_eq!") or mac.endswith("_ne!"):
                    asserted = args[:2]
                    msg_args = args[2:]
                else:
                    asserted = args[:1]
                    msg_args = args[1:]
                if not msg_args:
                    continue
                msg = strings_in(" ".join(msg_args))
                if not msg.strip():
                    continue
                DENOM["rs_points"] += 1
                a_ints = set()
                for a in asserted:
                    # 断言侧只取**字面量**，不取串里的数
                    a_ints |= set(ints(STR_LIT_RE.sub(" ", a)))
                m_ints = set(ints(msg))
                extra = sorted(m_ints - a_ints)
                if extra:
                    t2 = sorted(expected_position_ints(msg) - a_ints)
                    rows.append(
                        dict(kind="NUM", lang="rs", file=str(p.relative_to(REPO)),
                             line=line, mac=mac, asserted=sorted(a_ints),
                             in_msg=sorted(m_ints), mismatch=extra, tier2=t2,
                             expr=" , ".join(x.strip()[:120] for x in asserted),
                             msg=msg[:400])
                    )
                elif not m_ints and any(z in msg for z in ZERO_PRED) and a_ints:
                    rows.append(
                        dict(kind="PRED", lang="rs", file=str(p.relative_to(REPO)),
                             line=line, mac=mac, asserted=sorted(a_ints),
                             in_msg=[], mismatch=[],
                             expr=" , ".join(x.strip()[:120] for x in asserted),
                             msg=msg[:400])
                    )
    return rows


# ------------------------------------------------------------------ TS 切分

TS_EXPECT = re.compile(
    # ⚠ msg 后面那个 `,?` 是实测补的：prettier 把 `expect(` 折成多行时会补一个尾逗号，
    #   少了它，同一处判据在「折行前」数得到、「折行后」数不到 ⇒ **分母会随格式化漂**。
    r"expect\(\s*(?P<actual>[^,()]*(?:\([^()]*\))?[^,()]*)\s*,\s*(?P<msg>`(?:[^`\\]|\\.)*`|\"(?:[^\"\\]|\\.)*\"|'(?:[^'\\]|\\.)*')\s*,?\s*\)\s*"
    r"\.(?P<matcher>toBe|toHaveLength|toBeGreaterThan|toBeGreaterThanOrEqual|toBeLessThan|toEqual)\(\s*(?P<exp>[^)]*)\)",
    re.S,
)

TS_IT = re.compile(r"\b(?:it|test)\(\s*(?P<title>`(?:[^`\\]|\\.)*`|\"(?:[^\"\\]|\\.)*\"|'(?:[^'\\]|\\.)*')")
TS_ASSERT_NUM = re.compile(r"\.(?:toBe|toHaveLength|toBeGreaterThan|toBeGreaterThanOrEqual|toBeLessThan)\(\s*(-?\d+)\s*\)")


def strip_interp(s):
    """把模板串里的 ${...} 挖掉——它是**跟着代码走**的那一半，不算手抄。"""
    out = []
    i = 0
    while i < len(s):
        if s[i : i + 2] == "${":
            depth = 1
            j = i + 2
            while j < len(s) and depth:
                if s[j] == "{":
                    depth += 1
                elif s[j] == "}":
                    depth -= 1
                j += 1
            i = j
            continue
        out.append(s[i])
        i += 1
    return "".join(out)


def scan_ts():
    rows = []
    for p in walk({".ts"}):
        src = p.read_text(encoding="utf-8", errors="replace")
        rel = str(p.relative_to(REPO))
        # (1) expect(x, "msg").toBe(N)
        for m in TS_EXPECT.finditer(src):
            msg = strip_interp(m.group("msg"))
            exp = m.group("exp")
            a_ints = set(ints(exp))
            m_ints = set(ints(msg))
            line = src.count("\n", 0, m.start()) + 1
            extra = sorted(m_ints - a_ints)
            DENOM["ts_expect_points"] += 1
            if extra:
                rows.append(dict(kind="NUM", lang="ts", file=rel, line=line,
                                 mac="expect().%s" % m.group("matcher"),
                                 asserted=sorted(a_ints), in_msg=sorted(m_ints),
                                 mismatch=extra,
                                 tier2=sorted(expected_position_ints(msg) - a_ints),
                                 expr=exp.strip()[:120], msg=msg[:400]))
            elif not m_ints and any(z in msg for z in ZERO_PRED) and a_ints:
                rows.append(dict(kind="PRED", lang="ts", file=rel, line=line,
                                 mac="expect().%s" % m.group("matcher"),
                                 asserted=sorted(a_ints), in_msg=[], mismatch=[],
                                 expr=exp.strip()[:120], msg=msg[:400]))
        # (2) it("标题里有数") vs 体内断言的数
        its = list(TS_IT.finditer(src))
        for i, m in enumerate(its):
            title = strip_interp(m.group("title"))
            DENOM["ts_it_titles"] += 1
            t_ints = set(ints(title))
            if not t_ints:
                continue
            end = its[i + 1].start() if i + 1 < len(its) else len(src)
            body = src[m.end() : end]
            b_ints = set(int(x) for x in TS_ASSERT_NUM.findall(body))
            extra = sorted(t_ints - b_ints)
            if extra and b_ints:
                line = src.count("\n", 0, m.start()) + 1
                rows.append(dict(kind="NUM", lang="ts", file=rel, line=line,
                                 mac="it(title)", asserted=sorted(b_ints),
                                 in_msg=sorted(t_ints), mismatch=extra,
                                 tier2=sorted(expected_position_ints(title) - b_ints),
                                 expr="<it body>", msg=title[:400]))
    return rows


def main():
    rows = scan_rust() + scan_ts()
    rows.sort(key=lambda r: (r["file"], r["line"]))
    as_json = "--json" in sys.argv
    if as_json:
        print(json.dumps(rows, ensure_ascii=False, indent=1))
        return
    only2 = "--tier2" in sys.argv
    n_num = sum(1 for r in rows if r["kind"] == "NUM")
    n_pred = sum(1 for r in rows if r["kind"] == "PRED")
    n_t2 = sum(1 for r in rows if r.get("tier2"))
    print("被测对象仓根: %s" % REPO)
    print("分母（带诊断文本的判据点）: rust %d · ts expect %d · ts it 标题 %d"
          % (DENOM["rs_points"], DENOM["ts_expect_points"], DENOM["ts_it_titles"]))
    print("一档候选 %d 行（NUM %d · PRED %d）；二档（数坐在「期望值」位）%d 行"
          % (len(rows), n_num, n_pred, n_t2))
    print("=" * 78)
    for r in rows:
        if only2 and not r.get("tier2"):
            continue
        print("[%s] %s:%d  %s" % (r["kind"], r["file"], r["line"], r["mac"]))
        print("   断言侧字面量: %s" % r["asserted"])
        print("   诊断侧字面量: %s   对不上: %s   二档: %s"
              % (r["in_msg"], r["mismatch"], r.get("tier2") or "-"))
        print("   断言表达式: %s" % r["expr"])
        print("   诊断: %s" % r["msg"].replace("\n", " ")[:300])
        print("-" * 78)


if __name__ == "__main__":
    main()
