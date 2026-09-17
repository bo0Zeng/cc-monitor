#!/usr/bin/env python3
"""K-R1 · 摸底拍：`relay/` today 对**上游说什么格式**做了哪几处假设。

被测对象（写死住址，别让它跟着 cwd 漂）:
    /home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r1

它量什么（两路，方向相反）
--------------------------
**甲路 · 假设面**：`relay/` + monitor 注入口的生产段里，「上游/客户端说的是
Anthropic 那一套」这件事被**写死**在哪几处。每一处给住址 + 行号 + 它换成别的
供应商时的失效形态。

**乙路 · 透明面（反证）**：`relay/` 的**转发路径**上解析请求体/响应体的地方
应当是 **0 处**。这一路是甲路的对照 —— 它证的是「今天的假设**不在**转发路径
的语义层，只在几个字面量上」，也正是「加转换层」要动的那条性质。

**丙路 · 配置面的静默丢弃（本拍摸出来的、此前没登记过的一格）**：
`Base` 只有 `tls` / `host` / `port` 三个字段，**没有 path 那一格**，而
`Base::parse` 拿 `rest.split('/').next()` 只取 authority ⇒ 写进
`relay-credentials.json` 的 `base_url` **路径部分被丢掉，且 `parse` 照样回
`Some`** ⇒ `table::build` 不把它记进 `Rejected`、`announce` 一个字都不说。
症状：配 `https://vendor.example/anthropic` 的人，请求实际发到
`https://vendor.example/v1/messages`（真路径由客户端给、逐字透传）。
本路数两个数把这一格钉成明文：`Base` 的字段名单，以及**全仓喂给
`Base::parse` / 当 `base_url` 用的字面量里带路径的有几条**（现打：0 条 ⇒
这个问题从来没有被任何判据问过）。

它是怎么量的（分母口径，逐条写出来）
------------------------------------
- **分母不是「所有可能的假设」** —— 分母是**本文件里写死的这张锚点表**。
  这张表是我把 `relay/` 12 个文件的生产段读一遍之后手列的，不是搜出来的。
  ⇒ 它能证「我列的这几处今天确实在」，**不能证「只有这几处」**。
- 每个文件先剥成「生产段」：抄 `guard_core::production_code` 的口径 ——
  剥掉列 0 的 `#[cfg(test)] mod X { ... }` 整段、剥掉整行 `//` 注释、
  剥掉行尾 `//` 注释。⇒ **注释里的字面量不算数**（`relay/` 的头注里到处是
  被引用的字面量，不剥的话本量具会在文档里数出一堆假命中）。

  ★★ 行尾注释这一剥**必须跟引号状态**，不许裸 `find("//")`
  〔本量具的第一版就是裸切的，当场被自己的相等断言逮住〕：
  `const DEFAULT_UPSTREAM: &str = "https://api.anthropic.com";` 这一行里，
  `https://` 的那个 `//` 会被裸切法当成注释起点 ⇒ 锚点 A **恒 0 命中**，
  而那正是本量具要数的第一条假设。⇒ 下面 `strip_trailing_comment` 抄
  `guard_core::strip_trailing_comments` 的写法（逐字节跟引号、转义跳两格），
  连同它那三条**保守边界**（raw/byte string、跨行字符串、引号不配平 ⇒ 整行不动）
  一起抄过来 —— 宁可留洞，不许造假红。
- 锚点**逐字匹配**。命中数与预期数不等 ⇒ 本量具**当红**退出（exit 1）：
  代码搬了而这张表没跟着搬，是本量具唯一会撒谎的形状。

⚠ 诚实边界
----------
1. 本量具量的是**字面量**，不是行为。「上游会不会接受」它一个字都答不了 ——
   那要打真第三方 API，而本拍 🔴 不许发任何外网请求。
2. 乙路那个 0 是**按锚点表数的 0**，不是「不可能有解析」。反射式/间接的解析
   （先把字节递给另一个函数再解）它看不见。
3. 甲路 E 那一条跨到 monitor 半边（`src-tauri/`），它**不在** `relay/` 里 ——
   单列出来正是因为「客户端说什么格式」这个决定不住在中转里。
"""

import os
import re
import sys

ROOT = "/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r1"
RELAY = os.path.join(ROOT, "remote-daemon-proto", "src", "relay")
PAYLOAD = os.path.join(ROOT, "src-tauri", "src", "backend", "control", "payload.rs")


def strip_trailing_comment(line: str) -> str:
    """剥掉**行尾**注释 —— 逐字节跟引号状态，抄 `guard_core::strip_trailing_comments`。

    🔴 保守边界（与 Rust 那份逐条对齐，宁可留洞、不许造假红）：
    raw / byte string 的开头（`r"` · `r#` · `b"`）出现在行里 ⇒ **整行不动**。
    ⚠ 跨行字符串那一条本量具**没做**（Rust 那份做了）—— 本量具的锚点全在
    单行字面量上，做与不做对读数没有影响；写在这里，别读成「已经关干净」。
    """
    if 'r"' in line or "r#" in line or 'b"' in line:
        return line
    in_str = False
    i = 0
    b = line
    n = len(b)
    while i < n:
        c = b[i]
        if in_str:
            if c == "\\":
                i += 2
                continue
            if c == '"':
                in_str = False
            i += 1
            continue
        if c == '"':
            in_str = True
            i += 1
            continue
        if c == "/" and i + 1 < n and b[i + 1] == "/":
            return b[:i]
        i += 1
    return line


def production_code(text: str) -> list:
    """剥成生产段，返回 [(1-based 行号, 该行生产段内容)]。

    抄 `guard_core::production_code` 的口径：列 0 的 `#[cfg(test)]` 起整段剥掉、
    整行 `//` 剥掉、行尾 `//` 由 [`strip_trailing_comment`] 跟着引号剥。
    ⚠ 不处理块注释 —— 本仓的 Rust 判据同样不处理，两边口径对齐比各自更聪明重要。
    """
    out = []
    in_test = False
    depth = 0
    for i, line in enumerate(text.splitlines(), start=1):
        if not in_test and line.startswith("#[cfg(test)]"):
            in_test = True
            depth = 0
            continue
        if in_test:
            depth += line.count("{") - line.count("}")
            if depth <= 0 and "}" in line:
                in_test = False
            continue
        if line.lstrip().startswith("//"):
            continue
        out.append((i, strip_trailing_comment(line)))
    return out


# 甲路锚点表。(编号, 住址, 逐字锚点, 预期命中数, 它假设了什么 / 换供应商时怎么失效)
ASSUMPTIONS = [
    (
        "A",
        os.path.join(RELAY, "server.rs"),
        '"https://api.anthropic.com"',
        1,
        "默认上游写死官方端点。行里没写 base_url 就落到它（table.rs `None => default_base`）"
        " ⇒ 「配错了」与「没配端点」都会静默发到官方，而不是报错。",
    ),
    (
        "B",
        os.path.join(RELAY, "server.rs"),
        "Authorization: Bearer ",
        1,
        "换头写死 `Bearer` 这一种鉴权方案。只认 `x-api-key` 的上游今天配上去是 401，"
        "而 401 与「key 打错了」同形 ⇒ 一条查不出来的失败。",
    ),
    (
        "C",
        os.path.join(RELAY, "tee.rs"),
        'strip_prefix("data:")',
        1,
        "tee 侧认 SSE 的 `data:` 分帧。⚠ 它**只在抄写那一路**，不在转发路径上 ——"
        "上游换成非 SSE 的流，下游字节一个不少，只有 tee 抄不出事件。",
    ),
    (
        "D",
        os.path.join(RELAY, "server.rs"),
        "Accept-Encoding: identity",
        1,
        "强制上游不压缩（`super` 头注㈢：为的是 tee 拿明文 SSE、省掉解压依赖）。"
        "不认 `identity` 而照旧压缩的上游 ⇒ tee 抄出来是乱码，下游仍然正确。",
    ),
    (
        "E",
        PAYLOAD,
        "export ANTHROPIC_BASE_URL=",
        1,
        "🔴 **客户端那一侧**：注入口写死 Anthropic 的环境变量名。"
        "⇒ 说 Anthropic 格式的**不是中转，是被拉起的那个 agent 进程**。"
        "这一处解释了为什么『中转格式无关』并不等于『接得上 OpenAI 供应商』。",
    ),
]

# 乙路：转发路径上「解析请求体/响应体」的锚点。**预期全 0**。
# ⚠ `json_str` 是**产出** tee 行的转义器，不是解析上游字节 ⇒ 不进这张表。
TRANSPARENCY = [
    "serde_json::from_str",
    "serde_json::from_slice",
    "serde::Deserialize",
    "serde_json::Value",
]


def main() -> int:
    bad = 0
    print("=" * 72)
    print("甲路 · 上游格式假设面（分母 = 本文件写死的 %d 条锚点，不是「所有假设」）" % len(ASSUMPTIONS))
    print("=" * 72)
    for tag, path, needle, want, why in ASSUMPTIONS:
        with open(path, encoding="utf-8") as fh:
            prod = production_code(fh.read())
        hits = [n for n, line in prod if needle in line]
        rel = os.path.relpath(path, ROOT)
        ok = len(hits) == want
        print("\n[%s] %s" % (tag, rel))
        print("    锚点   : %r" % needle)
        print("    命中   : %d 处（预期 %d）%s" % (len(hits), want, "" if ok else "  ⇐ ⚠ 对不上"))
        print("    行号   : %s" % (", ".join(str(n) for n in hits) or "（无）"))
        print("    假设了 : %s" % why)
        if not ok:
            bad += 1

    print()
    print("=" * 72)
    print("乙路 · 转发路径的透明面（反证：预期全 0）")
    print("=" * 72)
    for needle in TRANSPARENCY:
        total = 0
        where = []
        for name in sorted(os.listdir(RELAY)):
            if not name.endswith(".rs"):
                continue
            path = os.path.join(RELAY, name)
            with open(path, encoding="utf-8") as fh:
                prod = production_code(fh.read())
            for n, line in prod:
                if needle in line:
                    total += 1
                    where.append("%s:%d" % (name, n))
        print("  %-24s ⇒ %d 处 %s" % (needle, total, ("  " + ", ".join(where)) if where else ""))
        if total != 0:
            bad += 1

    print()
    print("=" * 72)
    print("丙路 · 配置面的静默丢弃（`base_url` 的路径部分）")
    print("=" * 72)
    up = os.path.join(RELAY, "upstream.rs")
    with open(up, encoding="utf-8") as fh:
        prod = production_code(fh.read())
    # ㈠ `Base` 有哪几个字段 —— 有没有 path 那一格。
    fields, in_base = [], False
    for n, line in prod:
        if "pub(crate) struct Base {" in line:
            in_base = True
            continue
        if in_base:
            if "}" in line:
                break
            m = re.search(r"pub\(crate\)\s+(\w+)\s*:", line)
            if m:
                fields.append("%s(:%d)" % (m.group(1), n))
    print("  `Base` 的字段     ⇒ %s" % ", ".join(fields))
    has_path = any(f.startswith("path") for f in fields)
    print("  有 path 那一格吗  ⇒ %s" % ("有" if has_path else "**没有** ⇒ 路径存不下来"))
    if has_path:
        print("  ⚠ 盘上多了 path 字段 —— 本路的结论过期了，重读。")
        bad += 1

    # ㈡ authority 是怎么切的 —— 路径被丢在哪一行。
    cut = [n for n, line in prod if "rest.split('/').next()" in line]
    print("  切掉路径的那一行  ⇒ upstream.rs:%s" % (", ".join(map(str, cut)) or "（找不到 ⇐ ⚠）"))
    if len(cut) != 1:
        bad += 1

    # ㈢ 全仓喂过带路径的 base_url 吗 —— 分母 = 两棵树 .rs 里的这两种字面量。
    lits, withpath = [], []
    pat = re.compile(r'(?:Base::parse\("([^"]*)"|"base_url"\s*:\s*"([^"]*)")')
    for tree in ("remote-daemon-proto/src", "src-tauri/src"):
        for dirpath, _, names in os.walk(os.path.join(ROOT, tree)):
            for name in names:
                if not name.endswith(".rs"):
                    continue
                with open(os.path.join(dirpath, name), encoding="utf-8") as fh:
                    for m in pat.finditer(fh.read()):
                        s = m.group(1) or m.group(2)
                        lits.append(s)
                        after = s.split("://", 1)[-1] if "://" in s else s
                        if "/" in after:
                            withpath.append(s)
    print("  喂过的字面量      ⇒ %d 条（分母：两棵树全部 .rs，**含测试段**）" % len(lits))
    print("  其中带路径的      ⇒ %d 条 %s" % (len(withpath), withpath or "⇒ 这个问题从没被问过"))

    print()
    if bad:
        print("⛔ %d 条锚点对不上 —— 代码搬了而这张表没跟着搬。本量具当红，别读它的数。" % bad)
        return 1
    print("✔ 锚点表与盘上一致。⚠ 它证的是「我列的这几处确实在」，不是「只有这几处」。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
