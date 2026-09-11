#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R43 变异台：逐刀切，看哪条判据出声。

住址（唯一）：`<代码仓工作树>/evidence/K-R43-mutation-table.py`
被测对象：**跑它的时候所在的那棵树**（`--tree`，缺省 = 本文件的上一级目录）——
  每一行读数的抬头都印这个绝对路径（`5k`：同名量具指向另一棵树，输出长得一模一样）。

纪律（`brief` 第 7 / 8 条，逐条落成代码）
----------------------------------------
· 切之前**断言锚点恰好命中 N 次**，命中数写进表；不对就 ANCHOR-MISS，不是「新红 0」。
· 切完印「变异已落地」，并**回读**确认盘上真的变了。
· 跑完**核判定行数**（等不等于基线那个数），掉了或异常退出按 CRASH 记。
· 每一刀跑完**逐字节还原**并核 md5 —— 还原不了当场 ABORT，绝不把变异留在盘上。
· **空刀也是读数**：预期空刀的那几刀照样进表，它们量的是「射程到不到这里」。
"""
import argparse
import hashlib
import pathlib
import re
import subprocess
import sys

BASE = "8b474e2"  # 本件的基线提交（PM 在沙箱跑过 GATE: OK 的那个）

LD = "src-tauri/src/local_daemon.rs"
LB = "src-tauri/src/backend/control/local_backend.rs"
WS = "src-tauri/src/write_site_registry.rs"

# 每一刀都跑这一组，判定行数恒 = len(TESTS)。
TESTS = [
    "the_two_resolution_paths_still_agree_on_the_order",
    "the_self_extract_path_really_asks_the_product_whether_it_carries_one",
    "the_extraction_refusal_is_a_different_sentence_from_having_no_backend_at_all",
    "a_directory_it_cannot_create_really_takes_the_loud_path",
    "the_missing_sidecar_diagnosis_hands_the_user_a_next_step",
    "the_startup_path_really_calls_this_module",
    "the_production_entry_hands_the_stdio_consumer_down",
    "both_production_spawn_paths_go_through_the_shared_etxtbsy_verdict",
    "the_user_actionable_start_failures_all_reach_the_user",
    "every_local_spawn_is_declared",
]
BAR = re.compile(r"^test (\S+) \.\.\. (ok|FAILED)$", re.M)


def git_show(tree, rev, rel):
    return subprocess.run(["git", "-C", str(tree), "show", f"{rev}:{rel}"],
                          capture_output=True, text=True, check=True).stdout


def span_of(src, head, closer):
    """从 `head` 那一行起，到**恰好等于 `closer`** 的那一行止（含）。回 (start, end) 字节位。"""
    at = src.find(head)
    if at < 0:
        return None
    lines = src[at:].split("\n")
    for i, l in enumerate(lines[1:], 1):
        if l == closer:
            return at, at + len("\n".join(lines[: i + 1])) + 1
    return None


def build_cuts(tree, orig_text):
    """每一刀：(编号, 切在哪, 切的是哪一句声称, 预期, [(相对路径, 锚点, 命中数, 换成什么)])"""
    base_ld = git_show(tree, BASE, LD)
    cur_ld = (tree / LD).read_text(encoding="utf-8")

    # 基线里那份手写实现 / 那条只对拍顺序的旧判据
    s = span_of(base_ld, "fn resolve_daemon_bin(", "}")
    old_resolve = base_ld[s[0]:s[1]]
    s = span_of(base_ld, "    fn the_two_resolution_paths_still_agree_on_the_order() {", "    }")
    old_judge = base_ld[s[0]:s[1]]
    s = span_of(cur_ld, "fn resolve_daemon_bin(", "}")
    new_resolve = cur_ld[s[0]:s[1]]
    s = span_of(cur_ld, "    fn the_two_resolution_paths_still_agree_on_the_order() {", "    }")
    new_judge = cur_ld[s[0]:s[1]]

    return [
        ("M1",
         "local_backend::resolve_or_extract 体内",
         "「释放失败说得出一句**分得开**的话」（`K-R42` 硬要求①）",
         "the_self_extract_path_… 红",
         [(LB, "let reason = extraction_failure_reason(extract_dir, &e);", 1,
           'let reason = format!("exe 旁无 sidecar，且释放内嵌 daemon 失败: {e}");')]),

        ("M2",
         "local_backend::resolve_or_extract 体内",
         "「自释放那条路**真的问了**这份产物自己带没带」（`K-R42` 防空转）",
         "the_self_extract_path_… 红",
         [(LB, "let carried: Option<(&str, &[u8])> = native_embedded_daemon();", 1,
           "let carried: Option<(&str, &[u8])> = None;")]),

        ("M3",
         "local_daemon::resolve_daemon_bin 整个函数",
         "「两条路都走那一份共用的解析」—— **退回 `K-R43` 重构前的形状**（PM 点名的那一刀）",
         "the_two_resolution_paths_… 红",
         [(LD, new_resolve, 1, old_resolve)]),

        # ⚠ M4a **是一刀设计错的刀，读数照写**（`brief` 9：报有牙要说清射程）。
        #   我原本写的预期是「旧判据绿」，实得是红，原话逐字：
        #   `今天那条路里找不到 \`resolve_beside_this_exe(\``（`local_daemon.rs:3124`）。
        #   ★ 错在**射程**：它只退了 `resolve_daemon_bin` 一侧，而 `start_or_extract` 还是
        #   `K-R43` 之后那份（已经把三问抽走了）⇒ 旧判据在**另一个体**上找不到锚点而炸，
        #   它根本没走到「对拍顺序」那一步。**这是一个混合态，历史上不存在。**
        #   ⇒ 真正的反证在 M4b：把本件**整件**退回基线，那才是 `K-R42` 之后盘上的真形状。
        ("M4a",
         "M3 ＋ 把判据也换回 `K-R43` 之前那版（只对拍顺序）",
         "**（设计错的一刀）**想证旧机制逮不到 M3，实际造出了一个历史上不存在的混合态",
         "我当时写的预期是「旧判据绿」——**这个预期是错的**，见表下逐字",
         [(LD, new_resolve, 1, old_resolve), (LD, new_judge, 1, old_judge)]),

        ("M4b",
         "三份文件**整件退回基线 8b474e2**（= `K-R42` 之后、`K-R43` 之前盘上的真形状）",
         "**反证刀**：旧机制（两份手写 ＋ 一条只对拍顺序的判据）\n"
         "   在两条路**内容已经漂开**的那一刻，出不出声",
         "旧判据**绿**（而量具现打：两条路的 ②③ 一个 1 一个 0）—— 这正是它被换掉的理由",
         [(rel, orig_text[rel], 1, git_show(tree, BASE, rel)) for rel in (LD, LB, WS)]),

        ("M5",
         "local_backend::start_or_extract 体内",
         "「旁边不许再长出第二份取法」（新判据的第 ④ 条）",
         "the_two_resolution_paths_… 红",
         [(LB, "    let resolved = resolve_or_extract(target_triple, extract_dir, embedded, make_executable);",
           1,
           "    let resolved = resolve_or_extract(target_triple, extract_dir, embedded, make_executable);\n"
           "    let _second_way = resolve_beside_this_exe(target_triple);")]),

        # M8：**只打该盖的最小面**（`brief` 9）。M3 / M5 都同时踩到 ② 与 ④，
        #   这一刀把 `start_or_extract` 的调用换成一个**不含任何被禁名字**的东西
        #   ⇒ 只有 ②（「恰好一处调它」）会出声，④ 一格都碰不到。
        ("M8",
         "local_backend::start_or_extract 体内（只打 ② 这一格）",
         "「两条路**各自恰好一处**调那份共用的解析」—— 新判据的第 ② 条，单独拎出来",
         "the_two_resolution_paths_… 红（且红的是 ② 那条报文，不是 ④）",
         [(LB, "    let resolved = resolve_or_extract(target_triple, extract_dir, embedded, make_executable);",
           1,
           "    let _ = (target_triple, extract_dir, embedded, make_executable);\n"
           "    let resolved = Resolved::Missing { reason: String::new(), looked_at: Vec::new() };")]),

        ("M6",
         "write_site_registry.rs 的 WRITE_SITES 散文",
         "登记里那句「**唯一调用点是** `local_backend::resolve_or_extract`」",
         "空刀（散文不进任何判据的人群）",
         [(WS, "⚠ **唯一调用点是 `local_backend::resolve_or_extract`**", 1,
           "⚠ **唯一调用点是某个别的东西，而且它写的不是 monitor 自己的目录**")]),

        ("M7",
         "local_daemon::resolve_daemon_bin 的**头注**",
         "注释里那句声称「⚠ **本层不许再自己拼失败串** —— 拼了就是第二份说法」",
         "空刀（注释不进判据人群 —— `brief` 15 那条「写在源码注释里的自证等于埋掉了」）",
         [(LD, "/// ⚠ **本层不许再自己拼失败串** —— 拼了就是第二份说法。由\n", 1, "")]),
    ]


def run_tests(tree):
    cmd = ["cargo", "test", "--lib", "--"] + TESTS
    r = subprocess.run(cmd, cwd=str(tree / "src-tauri"), capture_output=True, text=True)
    bars = BAR.findall(r.stdout)
    return r.returncode, bars, r.stdout + r.stderr


def main():
    ap = argparse.ArgumentParser()
    here = pathlib.Path(__file__).resolve().parent
    ap.add_argument("--tree", default=str(here.parent))
    ap.add_argument("--only", default="", help="只跑这几刀（逗号分隔，如 M4a,M4b）")
    a = ap.parse_args()
    tree = pathlib.Path(a.tree).resolve()

    print(f"[K-R43 变异台] 被测对象那棵树 = {tree}")
    print(f"[K-R43 变异台] 量具自己住 = {pathlib.Path(__file__).resolve()}")
    print(f"[K-R43 变异台] 基线提交 = {BASE} · 每刀跑这 {len(TESTS)} 条，判定行数恒 = {len(TESTS)}")
    print()

    files = {rel: (tree / rel) for rel in (LD, LB, WS)}
    orig = {rel: p.read_text(encoding="utf-8") for rel, p in files.items()}
    md5 = {rel: hashlib.md5(s.encode()).hexdigest() for rel, s in orig.items()}

    print("── M0 基线（不切）")
    code, bars, out = run_tests(tree)
    if len(bars) != len(TESTS):
        print(f"CRASH 基线判定行 {len(bars)} 条（应 {len(TESTS)}）—— 台子就没站住\n{out[-3000:]}")
        return 2
    base_red = [n for n, v in bars if v == "FAILED"]
    print(f"   判定行 {len(bars)} 条 · 红 {len(base_red)} 条 {base_red} · 退出码 {code}\n")

    rows = []
    for tag, where, claim, expect, patches in build_cuts(tree, orig):
        if a.only and tag not in a.only.split(","):
            continue
        print(f"── {tag} 切在 {where}")
        print(f"   切的是哪一句声称：{claim}")
        # ① 锚点自检：恰好命中 N 次
        ok = True
        for rel, anchor, want, _ in patches:
            got = orig[rel].count(anchor)
            print(f"   锚点命中 {got} 次（应 {want}）· 住 {rel}")
            if got != want:
                ok = False
        if not ok:
            print("   ANCHOR-MISS 锚点没对上 ⇒ 这一刀作废，不许记成「新红 0」\n")
            rows.append((tag, expect, "ANCHOR-MISS", claim))
            continue
        # ② 落刀
        for rel, anchor, _, repl in patches:
            cur = files[rel].read_text(encoding="utf-8")
            files[rel].write_text(cur.replace(anchor, repl, 1), encoding="utf-8")
        landed = all(files[rel].read_text(encoding="utf-8") != orig[rel] for rel, *_ in patches)
        print(f"   变异已落地：{landed}")
        # ③ 跑
        code, bars, out = run_tests(tree)
        if len(bars) != len(TESTS):
            got = "CRASH（判定行 %d 条，应 %d）" % (len(bars), len(TESTS))
            print(f"   {got}")
            tail = out[-1500:]
            print("   " + tail.replace("\n", "\n   ")[:1500])
        else:
            red = [n.split("::")[-1] for n, v in bars if v == "FAILED"]
            got = f"红 {len(red)} 条：{red}" if red else "空刀（红 0 条）"
            print(f"   判定行 {len(bars)} 条 · {got} · 退出码 {code}")
        print(f"   预期：{expect}")
        rows.append((tag, expect, got, claim))
        # ④ 还原 + 核 md5
        for rel, p in files.items():
            p.write_text(orig[rel], encoding="utf-8")
        for rel, p in files.items():
            now = hashlib.md5(p.read_text(encoding="utf-8").encode()).hexdigest()
            if now != md5[rel]:
                print(f"   ABORT 还原失败：{rel} md5 {now} ≠ {md5[rel]}")
                return 2
        print("   已还原（三份 md5 逐一核过）\n")

    print("═══ 变异表 ═══")
    for tag, expect, got, claim in rows:
        print(f"{tag} · 预期「{expect}」· 实得「{got}」")
    return 0


if __name__ == "__main__":
    sys.exit(main())
