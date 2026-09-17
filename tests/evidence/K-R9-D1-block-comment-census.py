#!/usr/bin/env python3
"""K-R9 · 块注释剥法的量具（D1 的分母 + D2 兜底面的复核）

它做三件事，都是**现打**、不读任何登记表：

  ① 语料清点：`.rs` 分母、行首为 `/*` 的生产行有几处（PM 09-04 报的「曝光 0」就是这个数）；
  ② 兜底面：照 `guard-core/src/lib.rs::try_strip_block_comments` 的**同一套词法**扫一遍，
     报出哪些文件会走兜底（depth 不收口 / 停在字符串里）—— Rust 侧那两条判据
     （`no_monitor_file_falls_back_to_leaving_block_comments_in` 与它的 daemon 兄弟）
     钉的就是这个集合为空；
  ③ 块注释面：哪些文件真的含块注释、各被抹掉多少字节。

⚠ 本脚本是**第二实现**，不是权威源。权威源是 Rust 那一份（一个事实一个权威源，E3）。
   它的岗位是「换个语言重打一遍同一个数」——两边对不上时，先怀疑本脚本。

⚠ 词法必须与 Rust 侧逐条对齐，否则量的不是同一件事：
   · 码 / 普通串 / 原始串(井号配平, 可跨行) / 行注释 / 块注释(带深度)
   · 只在「码」状态做字符字面量掩码（`'"'` 会把串状态整段带偏 ——
     `shared_crate_registry.rs::shared_crate_names` 的 `.trim_end_matches('"')` 就是这一形）
   · 块注释里**不认**字符串（与 rustc 一致）
   · 行注释里的 `/*` 不开块

用法：
    python3 evidence/K-R9-D1-block-comment-census.py [根目录 ...]
    # 缺省 = src-tauri/src remote-daemon-proto/src src-tauri/crates
"""
import os
import sys

# ── 与 Rust 侧 `mask_char_literals` 同一套：等长替换，下标一一对应 ────────────
def mask_char_literals(line: str) -> bytes:
    b = bytearray(line.encode("utf-8"))
    out = bytearray(b)
    i = 0
    while i < len(b):
        if b[i] == 0x27:  # '
            if i + 3 < len(b) and b[i + 1] == 0x5C:  # 反斜杠转义
                k = b.find(0x27, i + 3)
                if k != -1:
                    out[i : k + 1] = b"_" * (k + 1 - i)
                    i = k + 1
                    continue
            rest = line.encode("utf-8")[i + 1 :].decode("utf-8", "replace")
            if rest:
                c = rest[0]
                cl = len(c.encode("utf-8"))
                if c not in ("'", "\\") and i + 1 + cl < len(b) and b[i + 1 + cl] == 0x27:
                    out[i : i + 2 + cl] = b"_" * (2 + cl)
                    i = i + 2 + cl
                    continue
        i += 1
    return bytes(out)


def raw_string_open(sb: bytes, i: int):
    """`r"` / `r#"` / `b"` / `br#"` 的开头 → (井号数, 吃掉几个字节)；否则 None。"""
    def ident(c):
        return (48 <= c <= 57) or (65 <= c <= 90) or (97 <= c <= 122) or c == 0x5F
    if i > 0 and ident(sb[i - 1]):
        return None
    k = i
    if k < len(sb) and sb[k] == ord("b"):
        k += 1
        if k < len(sb) and sb[k] == ord('"'):
            return (0, k + 1 - i)
    if k >= len(sb) or sb[k] != ord("r"):
        return None
    k += 1
    hs = k
    while k < len(sb) and sb[k] == ord("#"):
        k += 1
    if k >= len(sb) or sb[k] != ord('"'):
        return None
    return (k - hs, k + 1 - i)


def scan(src: str):
    """→ (走没走兜底, 被抹掉的字节数)。兜底 = depth 不收口 / 停在串里。"""
    depth = 0
    in_str = False
    raw_hashes = None
    blanked = 0
    for raw in src.split("\n"):
        if depth == 0 and not in_str and raw_hashes is None:
            sb = mask_char_literals(raw)
        else:
            sb = raw.encode("utf-8")
        i = 0
        n = len(sb)
        while i < n:
            if depth > 0:
                if sb[i] == ord("/") and i + 1 < n and sb[i + 1] == ord("*"):
                    depth += 1; blanked += 2; i += 2; continue
                if sb[i] == ord("*") and i + 1 < n and sb[i + 1] == ord("/"):
                    depth -= 1; blanked += 2; i += 2; continue
                blanked += 1; i += 1; continue
            if raw_hashes is not None:
                h = raw_hashes
                if sb[i] == ord('"') and n >= i + 1 + h and all(
                    sb[j] == ord("#") for j in range(i + 1, i + 1 + h)
                ):
                    raw_hashes = None; i += 1 + h; continue
                i += 1; continue
            if in_str:
                if sb[i] == 0x5C:
                    i += 2; continue
                if sb[i] == ord('"'):
                    in_str = False
                i += 1; continue
            op = raw_string_open(sb, i)
            if op is not None:
                h, consumed = op
                if h == 0 and sb[i] == ord("b"):
                    in_str = True
                else:
                    raw_hashes = h
                i += consumed; continue
            if sb[i] == ord('"'):
                in_str = True; i += 1; continue
            if sb[i] == ord("/") and i + 1 < n and sb[i + 1] == ord("/"):
                break
            if sb[i] == ord("/") and i + 1 < n and sb[i + 1] == ord("*"):
                depth += 1; blanked += 2; i += 2; continue
            i += 1
    return (depth != 0 or in_str or raw_hashes is not None), blanked


def main():
    roots = sys.argv[1:] or [
        "src-tauri/src",
        "remote-daemon-proto/src",
        "src-tauri/crates",
    ]
    files = []
    for r in roots:
        for d, _, fs in os.walk(r):
            files += [os.path.join(d, f) for f in fs if f.endswith(".rs")]
    files.sort()

    bailed, withblk, leading = [], [], []
    for p in files:
        src = open(p, encoding="utf-8", errors="replace").read()
        bad, blanked = scan(src)
        if bad:
            bailed.append(p)
        if blanked:
            withblk.append((p, blanked))
        for k, l in enumerate(src.split("\n"), 1):
            if l.lstrip().startswith("/*"):
                leading.append(f"{p}:{k}")

    print(f"分母：{len(files)} 份 .rs（根目录 {roots}）")
    print(f"① 行首为 `/*` 的行：{len(leading)} 处")
    for x in leading:
        print(f"     {x}")
    print(f"② 走兜底（洞会重开、Rust 侧两条判据钉它为空）：{len(bailed)} 份")
    for p in bailed:
        print(f"     {p}")
    print(f"③ 含块注释：{len(withblk)} 份，共抹掉 {sum(b for _, b in withblk)} 字节")
    for p, b in sorted(withblk, key=lambda x: -x[1]):
        print(f"     {b:7d}  {p}")
    return 1 if bailed else 0


if __name__ == "__main__":
    sys.exit(main())
