#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""`K-R122` 死值验的刀 —— 每一刀先断言锚点命中数，再落刀，再印「变异已落地」。

## 纪律（照 `references/brief.md` 第 7 · 11 · 12c 条 · `K-R115` 那一族）

- 🔴 **还原不许 `copy2` / `cp -a`**：`--revert` 是**重写原文**（`write_text`），mtime 必变
  ⇒ 下一趟 `cargo` / `tsc` / `vitest` 一定重算。门禁 `copy2` 那一格正在数这件事，
  本文件自己不许犯它（本文件现打 `shutil` 零命中）。
- 每一刀落刀前 `assert 锚点命中 == want`，对不上**一个字节都不改**、整趟放弃。
- 一趟只许有一把刀在盘上：`--apply` 之前若备份还在，拒绝落刀。

## 刀（`want` 都写在下面 `CUTS` 里，逐刀带锚点命中数）

    d1a         ① 去掉 `acquire.rs` 那三处 `#[cfg(unix)]`      ⇒ `winchk-daemon` 必红（8 错）
    d1b         ① `search_query.rs` 退回 `libc::utimensat`      ⇒ `winchk-daemon` 必红（2 错）
    d1          ① 两刀一起                                      ⇒ `winchk-daemon` 必红（**10 错**，与 CI 同数同档）
    d2          ② `frame_for` 改回那个全大写的名字              ⇒ `shellcheck` 必红（SC1081 ×6）
    d34a        ④ `ci.yml` 把「用量探针（F10）」挪回 build 之前  ⇒ `ci-e2e-prereq` 必红（1 条）
    d34b        ③ `ci.yml` 把那四步搬回 `e2e-tmux`              ⇒ `ci-e2e-prereq` 必红（4 条）
    d5win       ⑤ **活体夹具**：把 `walk` 改成吐 Windows 形态的路径（`\\`），
                  `readFileSync` 侧同拍加一层「两种分隔符都吃」的翻译（＝ Windows 上 fs 的真实行为），
                  **修复仍在** ⇒ `commands.vitest.ts` 必须**仍绿**
    d5winraw    ⑤ 同一份活体夹具，**把修复拿掉** ⇒ 必红，且症状与云端逐字同形
    d3          `KR122D3` `package-lock.json` 退回 `3.7.0`      ⇒ `cargo` 必红
    d3n         `KR122D3` 阴性对照：`d3` ＋ 把本件新判据整块摘掉 ⇒ **必须不红**（本条存在的证据）
    dcount      `KR122D2` 甲：自述格数不跟（`19 格` → `18 格`）  ⇒ 覆盖尺子 `C5b` 必红
    droll       `KR122D2` 甲：逐格点名少一格                     ⇒ 覆盖尺子 `C5b` 必红
    dblinddel   `KR122D2` 乙：删掉裁决那一支印射程的那一行        ⇒ 覆盖尺子 `C5c` 必红
    dblindstale `KR122D2` 乙：射程表里塞回一个**今天已经有格**的键 ⇒ 覆盖尺子 `C5c` 必红
    dall        阴性对照：五条修复**整块**退掉（新判据全留着）    ⇒ 看几格红、几格不红

## 跑法

    python3 evidence/K-R122-cut.py --list
    python3 evidence/K-R122-cut.py --apply <刀名>
    python3 evidence/K-R122-cut.py --revert
"""

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BACKUP = ROOT / ".k-r122-cut-backup.json"

ACQ = "remote-daemon-proto/src/sidecars/codepicture/acquire.rs"
SQ = "remote-daemon-proto/src/observe/search_query.rs"
PROBE = "e2e/usage-probe-acceptance.sh"
CI = ".github/workflows/ci.yml"
VITEST = "src/ipc/commands.vitest.ts"
LOCK = "package-lock.json"
DCR = "src-tauri/src/doc_claim_registry.rs"
GATE = "scripts/gate.sh"

TOUCHED = [ACQ, SQ, PROBE, CI, VITEST, LOCK, DCR, GATE]

CFG_UNIX = "    #[cfg(unix)]\n    #[test]\n"
CFG_PLAIN = "    #[test]\n"

LIBC_SET_MTIME = '''    /// 只用 std + libc 设 mtime（本 crate 不引 filetime）。
    fn set_mtime(p: &Path, t: std::time::SystemTime) {
        let d = t.duration_since(std::time::UNIX_EPOCH).unwrap();
        let times = [
            libc::timespec {
                tv_sec: d.as_secs() as libc::time_t,
                tv_nsec: d.subsec_nanos() as _,
            },
            libc::timespec {
                tv_sec: d.as_secs() as libc::time_t,
                tv_nsec: d.subsec_nanos() as _,
            },
        ];
        let c = std::ffi::CString::new(p.as_os_str().as_encoded_bytes()).unwrap();
        let rc = unsafe { libc::utimensat(libc::AT_FDCWD, c.as_ptr(), times.as_ptr(), 0) };
        assert_eq!(rc, 0, "utimensat 失败，本条判据的前提没建起来");
    }
'''

WALK_NOW = '''    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, ext, out);
    else if (name.endsWith(ext)) out.push(p.split(sep).join("/"));
'''
# 活体夹具：`join` 产 Windows 形态的路径。`statSync` / 递归那两处翻回来 —— 那正是
# Windows 上 fs 的真实行为（两种分隔符都吃），不是给判据放水。
WALK_WIN_FIX = '''    const p = join(dir, name).split("/").join("\\\\");
    const real = p.split("\\\\").join("/");
    if (statSync(real).isDirectory()) walk(real, ext, out);
    else if (name.endsWith(ext)) out.push(p.split("\\\\").join("/"));
'''
WALK_WIN_RAW = '''    const p = join(dir, name).split("/").join("\\\\");
    const real = p.split("\\\\").join("/");
    if (statSync(real).isDirectory()) walk(real, ext, out);
    else if (name.endsWith(ext)) out.push(p);
'''
READ_NOW = 'readFileSync(f, "utf8")'
READ_WIN = 'readFileSync(String(f).split("\\\\").join("/"), "utf8")'


def read(rel):
    return (ROOT / rel).read_text(encoding="utf-8")


def hits(rel, needle):
    return read(rel).count(needle)


def sub(state, rel, old, new, want):
    """在 `state` 上做一次替换，落刀前断言锚点命中恰好 `want` 次。"""
    got = state[rel].count(old)
    assert got == want, (
        f"锚点在 `{rel}` 里命中 {got} 次，要求 {want} 次 —— **一个字节都没改**，整趟放弃。"
        f"\n锚点：{old[:80]!r}"
    )
    state[rel] = state[rel].replace(old, new)
    print(f"  · 锚点命中 {got}/{want}：`{rel}` ← {old.strip().splitlines()[0][:60]!r}")


def cut_d1a(st):
    sub(st, ACQ, CFG_UNIX, CFG_PLAIN, 3)


def cut_d1b(st):
    now = st[SQ]
    head = "    /// 只用 std 设 mtime（本 crate 不引 `filetime`）。"
    tail = '            .expect("set_times 失败，本条判据的前提没建起来");\n    }\n'
    i = now.index(head)
    j = now.index(tail, i) + len(tail)
    assert now.count(head) == 1, "锚点不唯一"
    st[SQ] = now[:i] + LIBC_SET_MTIME + now[j:]
    print(f"  · 锚点命中 1/1：`{SQ}` ← set_mtime 整块换回 libc 版")


def cut_d2(st):
    sub(st, PROBE, "frame_for() { LINE", "FOR() { LINE", 1)
    sub(st, PROBE, "$(frame_for ", "$(FOR ", 5)


def _ci_move(st, block_start, block_end, dest_after):
    """把 `ci.yml` 里 [block_start, block_end] 那几行挪到 `dest_after` 之后。"""
    lines = st[CI].split("\n")
    i = lines.index(block_start)
    j = lines.index(block_end, i)
    blk = lines[i:j + 1]
    rest = lines[:i] + lines[j + 1:]
    k = rest.index(dest_after)
    st[CI] = "\n".join(rest[:k + 1] + [""] + blk + rest[k + 1:])
    print(f"  · 锚点命中 1/1：`{CI}` ← 挪走 {len(blk)} 行")


def cut_d34a(st):
    # 把「用量探针（F10）」挪回 `build debug daemon` **之前**（＝ `K-R119` 那趟云端的形状）。
    _ci_move(st,
             "      - name: 用量探针（F10）",
             "        run: bash e2e/assert-pass-floor.sh usage-probe 11",
             "      - name: install tmux + jq + Tauri Linux build deps")


def cut_d34b(st):
    # 把那四步搬回 `e2e-tmux`（那个 job 没有任何 build）。
    for name, run in [
        ("      - name: ccm CLI 契约（F02）", "        run: bash e2e/assert-pass-floor.sh ccm-cli 46"),
        ("      - name: ccm --print 平价预言机（F03）", "        run: bash e2e/assert-pass-floor.sh ccm-print-parity 12"),
        ("      - name: ccm 契约差分对拍（U9a · S10 保住清单）", "        run: bash e2e/assert-pass-floor.sh ccm-contract-parity 45"),
        ("      - name: cc-spawn 收编等价（B02）", "        run: bash e2e/assert-pass-floor.sh cc-spawn-uplift 72"),
    ]:
        _ci_move(st, name, run, "        run: bash e2e/assert-pass-floor.sh tmux-target 26")


def cut_d5win(st):
    sub(st, VITEST, WALK_NOW, WALK_WIN_FIX, 1)
    sub(st, VITEST, READ_NOW, READ_WIN, 4)


def cut_d5winraw(st):
    sub(st, VITEST, WALK_NOW, WALK_WIN_RAW, 1)
    sub(st, VITEST, READ_NOW, READ_WIN, 4)


def cut_d3(st):
    sub(st, LOCK, '\n  "version": "3.8.0",', '\n  "version": "3.7.0",', 1)
    sub(st, LOCK,
        '\n    "": {\n      "name": "cc-monitor",\n      "version": "3.8.0",',
        '\n    "": {\n      "name": "cc-monitor",\n      "version": "3.7.0",', 1)


def cut_d3n(st):
    cut_d3(st)
    sub(st, DCR, "    fn the_npm_lockfile_claims_the_version_we_ship() {",
        "    fn the_npm_lockfile_claims_the_version_we_ship_DISABLED() {\n        return;\n        #[allow(unreachable_code)]", 1)


def cut_dcount(st):
    sub(st, GATE, "# │ 〔自述·格数〕19 格", "# │ 〔自述·格数〕18 格", 1)


def cut_droll(st):
    # ⚠ 锚点要带 `〔自述·点名〕` 那个前缀 —— 不带的话它在**裁决行**上也命中一次
    #   （第一版就是这样，落刀前的断言当场把它挡下：命中 2 次 ≠ 1 次，一个字节没改）。
    sub(st, GATE, "〔自述·点名〕hooks · copy2 · shellcheck · ci-e2e-prereq",
        "〔自述·点名〕hooks · copy2 · ci-e2e-prereq", 1)


def cut_dblinddel(st):
    sub(st, GATE, "  gate_print_blind\n  exit 0\n", "  exit 0\n", 1)


def cut_dblindstale(st):
    sub(st, GATE, '  "windows-runner|',
        '  "shellcheck|这一条今天是假的：本门禁已经有这一格了（活体，给 C5c 用）"\n  "windows-runner|', 1)


def cut_dall(st):
    cut_d1a(st)
    cut_d1b(st)
    cut_d2(st)
    cut_d34a(st)
    cut_d34b(st)
    cut_d3(st)
    # ⑤ 那一条在 Linux 上退回去**不可能红**（`sep` 恒是 `/`）—— 如实不切，
    #   本刀的报告里单列一行「⑤ 在本机判不了」。


CUTS = {
    "d1a": (cut_d1a, "① 去掉 acquire.rs 三处 #[cfg(unix)] ⇒ winchk-daemon 必红（8 错）"),
    "d1b": (cut_d1b, "① search_query.rs 退回 libc::utimensat ⇒ winchk-daemon 必红（2 错）"),
    "d1": (lambda st: (cut_d1a(st), cut_d1b(st)), "① 两刀一起 ⇒ winchk-daemon 必红（10 错，与 CI 同数）"),
    "d2": (cut_d2, "② 函数名改回全大写 ⇒ shellcheck 必红（SC1081 ×6）"),
    "d34a": (cut_d34a, "④ 用量探针挪回 build 之前 ⇒ ci-e2e-prereq 必红"),
    "d34b": (cut_d34b, "③ 四步搬回 e2e-tmux ⇒ ci-e2e-prereq 必红"),
    "d5win": (cut_d5win, "⑤ 活体夹具（Windows 形态路径）＋ 修复在 ⇒ 必须仍绿"),
    "d5winraw": (cut_d5winraw, "⑤ 同一份夹具、修复拿掉 ⇒ 必红，症状与云端同形"),
    "d3": (cut_d3, "KR122D3 package-lock 退回 3.7.0 ⇒ cargo 必红"),
    "d3n": (cut_d3n, "KR122D3 阴性对照：d3 ＋ 摘掉新判据 ⇒ 必须不红"),
    "dcount": (cut_dcount, "KR122D2 甲 自述格数不跟 ⇒ C5b 必红"),
    "droll": (cut_droll, "KR122D2 甲 逐格点名少一格 ⇒ C5b 必红"),
    "dblinddel": (cut_dblinddel, "KR122D2 乙 删掉印射程那一行 ⇒ C5c 必红"),
    "dblindstale": (cut_dblindstale, "KR122D2 乙 射程表塞回一个已经有格的键 ⇒ C5c 必红"),
    "dall": (cut_dall, "阴性对照：五条修复整块退掉（⑤ 除外，本机判不了）"),
}


def main():
    argv = sys.argv[1:]
    if not argv or argv[0] == "--list":
        for k, (_, why) in CUTS.items():
            print(f"{k:<12} {why}")
        return 0
    if argv[0] == "--revert":
        if not BACKUP.exists():
            print("没有备份 —— 盘上没有本文件落下的刀")
            return 1
        for rel, text in json.loads(BACKUP.read_text(encoding="utf-8")).items():
            (ROOT / rel).write_text(text, encoding="utf-8")
            print(f"  · 还原 `{rel}`（重写原文，mtime 必变 —— 不走 copy2）")
        BACKUP.unlink()
        print("还原完成")
        return 0
    if argv[0] != "--apply" or len(argv) < 2:
        print(__doc__)
        return 2
    name = argv[1]
    if name not in CUTS:
        print(f"没有这一刀：{name}")
        return 2
    if BACKUP.exists():
        print("盘上还有上一刀的备份 —— 先 `--revert`。一趟只许有一把刀在盘上。")
        return 2
    state = {rel: read(rel) for rel in TOUCHED}
    BACKUP.write_text(json.dumps(state, ensure_ascii=False), encoding="utf-8")
    try:
        CUTS[name][0](state)
    except AssertionError as e:
        BACKUP.unlink()
        print(f"落刀前断言失败，一个字节都没改：\n{e}")
        return 2
    changed = []
    for rel, text in state.items():
        if text != read(rel):
            (ROOT / rel).write_text(text, encoding="utf-8")
            changed.append(rel)
    print(f"变异已落地：{name} —— 改了 {len(changed)} 份：" + " · ".join(changed))
    return 0


if __name__ == "__main__":
    sys.exit(main())
