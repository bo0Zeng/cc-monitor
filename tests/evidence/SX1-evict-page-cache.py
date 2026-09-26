#!/usr/bin/env python3
# ruff: noqa: E501
"""SX1：把 `history-search` 那一群会话文件逐份**丢出页缓存** —— 秤的「冷」那一格。

跑法：
    python3 tests/evidence/SX1-evict-page-cache.py <projects 根>

人群与后端那一问同一个：`<根>` 下深度 ≤ 2 的 `*.jsonl`（`search_query::search_counting` 的 walk：
`WalkDir(max_depth 2)`、不跟符号链接、只收普通文件）。

**只读**：逐份 `O_RDONLY` 打开、`posix_fadvise(fd, 0, 0, POSIX_FADV_DONTNEED)` —— 只让内核丢掉这份文件的
干净缓存页，一个字节都不写、不改 mtime、不要 root。`drop_caches` 要 root 且全机生效，本秤不用它。
⚠ 丢不掉的：目录项与 inode 缓存（`fadvise` 碰不到，`设计/17 §6.9` F2 同一格）—— 43 份文件的元数据不是冷读的大头。

**只出数**：文件数 · 总字节 · 丢之前 / 之后仍在页缓存里的字节（`fincore`，没有就报 n/a）。一个字正文都不读。
被 `tests/backend/observe/search_query_reading.rs` 的读数（`SX1_EVICT` 指过来）在相位之间调起；也可以单跑。
"""
import os
import subprocess
import sys


def population(root):
    out = []
    try:
        top = list(os.scandir(root))
    except OSError:
        return out
    for e in top:
        if e.is_file(follow_symlinks=False) and e.name.endswith(".jsonl"):
            out.append(e.path)
        elif e.is_dir(follow_symlinks=False):
            try:
                for f in os.scandir(e.path):
                    if f.is_file(follow_symlinks=False) and f.name.endswith(".jsonl"):
                        out.append(f.path)
            except OSError:
                pass
    return sorted(out)


def resident_bytes(paths):
    if not paths:
        return 0
    try:
        r = subprocess.run(
            ["fincore", "--bytes", "--noheadings", "--raw", "--output", "RES", *paths],
            capture_output=True,
            text=True,
            check=False,
        )
    except FileNotFoundError:
        return None
    if r.returncode != 0:
        return None
    total = 0
    for line in r.stdout.split():
        try:
            total += int(line)
        except ValueError:
            return None
    return total


def main():
    if len(sys.argv) != 2:
        print("用法：SX1-evict-page-cache.py <projects 根>", file=sys.stderr)
        return 2
    paths = population(sys.argv[1])
    size = 0
    for p in paths:
        try:
            size += os.stat(p, follow_symlinks=False).st_size
        except OSError:
            pass
    before = resident_bytes(paths)
    failed = 0
    for p in paths:
        try:
            fd = os.open(p, os.O_RDONLY)
        except OSError:
            failed += 1
            continue
        try:
            os.posix_fadvise(fd, 0, 0, os.POSIX_FADV_DONTNEED)
        except OSError:
            failed += 1
        finally:
            os.close(fd)
    after = resident_bytes(paths)
    fmt = lambda v: "n/a" if v is None else str(v)  # noqa: E731
    print(
        f"SX1-evict files={len(paths)} bytes={size} resident_before={fmt(before)} "
        f"resident_after={fmt(after)} failed={failed}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
