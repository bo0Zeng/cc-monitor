#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R67 摸底量具 —— 「42 件未签收」的四类分册 + 架构分层 + 停车理由时态审计。

**被测对象**：计划仓 `.claude/planned-build/backend-consolidation/`（**不是代码仓**）。
用法：  python3 K-R67-census.py <计划仓绝对路径>
量于：  见输出头一行（现算 mtime + md5，不写死）。

# 这把尺子量什么、不量什么

量：件文件里 `kind: item` 那一行的 `state`/`exit`；`.dispatch.json` 的
`inflight` / `parked` / `parked_全文` / `已回收` 四个键的**键名与交集**。
**不量**：件里那些话是不是真的。判定列（`类` / `层` / `停车理由今天`）是**人写的**，
住在下面 `VERDICTS` 里，每一条都带「依据」；它们**不由本脚本推导**，别把它们读成读数。

# 为什么判定要和读数放在同一份文件里

纪律 ⑭「计数与成员名单同住一处」：本脚本每报一个数，同一行把名单打出来。
而 `CROSS_CHECK` 那一段做的是**反向**：盘上有的件而 `VERDICTS` 里没有 ⇒ 当场喊，
免得「盘变了而判定表没跟」变成一次静默的假读数。
"""
import json
import re
import sys
import hashlib
import os
import time

# ---------------------------------------------------------------- 判定表（人写的）
# id -> (类, 主层, 同拍层, 交付读数的住址, 停车理由今天成不成立, 依据)
#
# 类：甲=已交付未签收（整件交付，只差手续）  乙=部分交付（本拍合入、件内仍有未做阶段）
#     丙=摸底件（exit 待摸底）              丁=真没做（零交付）
#     戊=终态（不该算进「没做」）
# 层：后端本体 / 本机后端 / 中转 / 适配层 / 界面 / 环境部署 / 判据卫生 / 跨层
VERDICTS = {
 "K-P2":  ("乙","后端本体","本机后端","parked_全文.K-P2 · audits/K-P2-PM-2.md · 主干合入",
           "成立","理由自陈的 5 项未做（退出码 2/4 统一 · ccm-acceptance 挂门禁 · 真机往返 · exit 4 下游 · 题面住址）今天没有一项被别的件带走"),
 "K-P3":  ("乙","本机后端","-","parked_全文.K-P3 · audits/K-P3-PM.md",
           "已馊","理由逐字「record_death 还没有生产调用点，账上恒空；三处接线归下一拍单独一件」——K-P3b（已签收）已把三处接上并配相等断言，住 src-tauri/src/daemon_policy.rs:500 DEATH_RECORD_SITES（note_detached_death / daemon_supervise_events / note_never_started）"),
 "K-P4":  ("乙","本机后端","适配层","parked_全文.K-P4 · audits/K-P4-W-PM.md · 主干 3a2ba00",
           "半馊","理由「要真机跑 Windows，而 win11 VM 归用户 ⇒ 归排期」——「真机拿不到」那半今天不成立：K-R42 09-10 夜就在 win11 上取过读数（features/K-R42…#§9 · audits/K-R42-真机读数-PM.md）。剩下的是排期与许可，不是可得性"),
 "K-P6":  ("丁","本机后端","适配层","（零交付；第一拍 KP6D1 交回后 PM 重切，features/K-P6…#§0-订正）",
           "无理由","🔴 .dispatch.json 的 parked 里**没有这一条** —— 44 件未签收里唯一一件「既不在 inflight、也没有任何一处写下为什么不推它」的（K-R41 是终态不算）。它的真阻塞是未闭合的 ROADMAP.md#KU11"),
 "K-R1":  ("乙","中转","-","parked_全文.K-R1 · audits/K-R1-PM-2.md",
           "成立","理由自陈的 5 项（转换层 · message_start usage · 界面入口 · 前缀末段出声 · IPC-PROTOCOL 三行）没有一项被带走"),
 "K-R11": ("丁","判据卫生","-","（零交付）",
           "成立","现打复核：`find /home/zbl/文档/claudecode-frontend -name pb.py -not -path '*/node_modules/*'` = **0**；pb.py 只住上线位（~/.claude/skills/planned-build/bin/pb.py，558567 B）⇒「这个工作区做不了它的一半」今天仍然为真"),
 "K-R12": ("乙","适配层","后端本体","parked_全文.K-R12 · audits/K-R12-PM-3.md",
           "成立","理由自陈的三项（旗真被走到 · SSH 送 env 实测 · K-R23 D4）未见落点"),
 "K-R16": ("乙","界面","本机后端","parked_全文.K-R16 · audits/K-R16-PM.md · 主干 f485290",
           "成立","五条路线（A–E）今天一条都没被点；且这五条**没有登记成 question 节点**，见本件 §8 那笔"),
 "K-R19": ("甲","判据卫生","-","audits/K-R19-PM.md §五 判 accept",
           "成立","卡的是 PM 自己的 [J6 历史改写]，唯一消法是 git reflog expire（判据自登记的洞）⇒ 仍要用户点甲/乙"),
 "K-R2":  ("乙","中转","后端本体","parked_全文.K-R2 · audits/K-R2-PM-2.md",
           "成立","理由自陈「真搬家（引擎按需拉取 · 第三棵树）没开」；K-W2D 接线拍买的是别的格"),
 "K-R22": ("乙","判据卫生","-","parked_全文.K-R22 · audits/K-R22-PM.md · 主干 56c1b42",
           "已馊","理由里两处数值都馊了：①「另立件待办 race_watchdog…」09-05 已注明被 K-R24 D2 在 ab48245 关掉；②「gate.sh 478→627 行」09-05 订正成 891，而**今天现打 1023 行**（wc -l scripts/gate.sh @5be9737）⇒ 同一个数第三次过期"),
 "K-R23": ("乙","后端本体","-","parked_全文.K-R23 · audits/K-R23-PM.md · 主干 140ce0b",
           "成立","理由自陈 D4 明确没做（要碰 Gate 3 语义）"),
 "K-R24": ("乙","判据卫生","适配层","parked_全文.K-R24 · audits/K-R24-PM.md",
           "半馊","四项未做仍在；而理由里 `accounts_query.rs:2236` 那个行号 09-05 已注明漂到 :2351（本条自己就是「行号要带校验位」的活体）"),
 "K-R25": ("乙","判据卫生","-","parked_全文.K-R25 · audits/K-R25-PM.md",
           "半馊","「tool_registry.rs:577 私有剥法进没进登记表未核」09-06 已核并关掉；剩下的四项（第三个单位「窗口」· 姊妹守卫 · .ts 轴 · crates+build.rs）仍在，其中第一项恰是 K-R35 的题面"),
 "K-R35": ("丁","判据卫生","-","（零交付，09-06 第三十五拍立、未派）",
           "已馊","理由逐字「本轮不推的理由只有一条：并发上限 1，K-R36 在跑」（09-06 订正版）—— K-R36 今天 state=已签收 ⇒ 那条档位理由不成立"),
 "K-R38": ("甲","判据卫生","-","主干 2c9c9cc · audits/K-R38-PM.md · PM 独立沙箱门禁全绿",
           "成立","pb accept 被 [J6 历史改写] 拒，PM 已核两个提交 tree sha 逐字节相同；下一步在用户手上（要不要追认签收）"),
 "K-R39": ("丁","判据卫生","-","（零交付，09-06 第三十九拍立、未派）",
           "已馊","理由逐字「本轮不推的理由只有一条：并发上限 1，K-R37 在跑」—— K-R37 今天 state=已签收"),
 "K-R40": ("丁","判据卫生","-","（零交付，09-06 第四十一拍立、未派）",
           "已馊","理由逐字「本轮不推的理由只有一条：并发上限 1，K-R38 在跑」—— K-R38 的代码 09-09 已合入 2c9c9cc、没有 agent 在对应它，它占的是**签收**不是**档位**"),
 "K-R41": ("戊","判据卫生","-","features/K-R41…#§PM 裁定（state=判定不做 · exit=钉不上）",
           "无理由","终态件。**不该算进「还有什么没做」** —— 它是本区 44 件未签收里唯一一件已经有终局的"),
 "K-R42": ("甲","环境部署","本机后端","features/K-R42…#§9 · audits/K-R42-真机读数-PM.md（win11 实测）",
           "成立","四条卡签收的全是 PM 的账（①②③ 登记被手工摘掉 · ④ §8 三十条要重排版）；现打 inflight 里没有 K-R42 ⇒ ①②③ 未修"),
 "K-R43": ("乙","本机后端","-","主干 20e3394 · audits/K-R43-PM.md",
           "成立","三格未做：两格要 PM 先裁、一格要真机（且明写别把 K-R42 那趟读成这条也验过了）"),
 "K-R44": ("丁","环境部署","-","（零交付，09-10 深夜立）",
           "已馊","理由逐字「前提是 P2d（认已有实例）给出「谁还在跑」—— P2d 没做」。现打两笔：① P2d 住 control-parity 工作区，state=**已签收**、exit=**改件**（没交付 ①②，是改了计划出的口）；② P2d 当年判「结构性做不到」的读数是「setsid/process_group/DETACHED_PROCESS/pre_exec 命中数 = 0」，而**今天 src-tauri/src/local_daemon.rs:526 的 spawn_detached 就带 process_group(0)、收尸走 reap_detached** ⇒ 那条依据被推翻，结论要重判"),
 "K-R47": ("丁","界面","-","（零交付）",
           "成立","理由是排序不是冲突：MASTERPLAN.md#K32〔用 09-10〕把 Linux 前端往后排，而本件验收面是界面行为。K32 今天没被改"),
 "K-R48": ("乙","后端本体","环境部署","第二拍主干 07e4e72 · audits/K-R48-第二拍-PM.md",
           "成立","KR48D3（旧副本）本拍没做，且形状已被 K34 改写成「退役，不是更新」⇒ 归 K-R57 那条线重写"),
 "K-R50": ("丁","界面","本机后端","（零交付）",
           "半馊","「排在 K-R55 之后」那半**已满足**（K-R55 09-11 已合入 6bb9399）；仍要用户点的只剩第 3 条（立唯一入口，与 K-P4 射程重叠）"),
 "K-R51": ("丙","后端本体","跨层","四路只读审计已交回，读数住 features/K-R51…#§6/§7 · LEDGER.md「四路只读审计交回三路」与「第四路（分层）也回来了」",
           "已馊","理由逐字「解锁：四路交回、PM 合并成一张图之后…那时再按正常流程派 D2」—— 四路**都已交回**（LEDGER 两节现打）⇒ 解锁条件已到"),
 "K-R52": ("甲","适配层","-","主干 1ca0677 · audits/K-R52-PM.md · PM 门禁 GATE: OK",
           "成立","不签收的唯一原因是件 §1 没有 dod 节点（立件当拍直接派 C）⇒ pb accept 结构上走不了。**不是这件活没做完**"),
 "K-R53": ("甲","本机后端","界面","主干 59fc778 · audits/K-R53-PM.md",
           "成立","四条 dod 派工前落好了，`mutation:` 住址空着 ⇒ 差一步不是差一阶"),
 "K-R54": ("丙","跨层","后端本体","B 摸底交回，17 行裁定表住件文件 §8 · 锚点 evidence/K-R54-摸底-锚点表.tsv · audits/K-R54-B摸底-PM.md",
           "成立","它自陈：这本账**没有一个键能表达「零代码/只读的活」**，写进 parked 是唯一能让判据闭嘴的地方 ⇒ 这一行本身就是那笔代价"),
 "K-R55": ("甲","适配层","-","主干 6bb9399 · audits/K-R55-PM.md · GATE: OK",
           "成立","七条全做了，`mutation:` 住址仍空（本区连续第四件同形）"),
 "K-R56": ("甲","后端本体","-","主干 94cfafa · audits/K-R56-PM.md · GATE: OK",
           "成立","三条 dod 做了、`mutation:` 空；跟进②「真删那条回落」的解锁条件没变（要先有一个运行期计数）"),
 "K-R57": ("丙","环境部署","-","摸底交回并合入 e5222d2（零产品代码，4 份在 evidence/）· audits/K-R57-摸底-PM.md",
           "成立","exit 已由「待摸底」推进到可排期；它给的下一步是「先做那张清单的闭集、让聚合视图去读它，不是先做装」"),
 "K-R58": ("甲","环境部署","-","主干 d4b18a9 · audits/K-R58-PM.md · GATE: OK",
           "成立","三条 dod 全做了，`mutation:` 住址仍空（连续第五件同形）"),
 "K-R64": ("丁","判据卫生","-","（零交付，09-11 第二十九拍立）",
           "半馊","「摸底那一问没有读数」那半 09-11 第三十拍已由 PM 给了口径（features/K-R64…#§0b-2：给历史留痕一个显式记号）；剩下的「只卡并发档」今天由本件占着"),
 "K-R66": ("丙","界面","-","（零交付，09-11 立，用户点名要的）",
           "成立","理由「排在 K-R67 之后 —— 先有顺序再动界面」；本件一交回即满足"),
 "K-R67": ("丙","跨层","-","（本件）",
           "成立","本件即在跑"),
 "K-R68": ("丙","环境部署","后端本体","（零交付，09-11 立，K-R65 盘上普查逮到）",
           "成立","三种载体 vs 闭集一格；PM 裁定住 DECISIONS.md#R24，但裁定一是按**代码同源**裁的、**没验过二进制同源**（件里 §0c-2 自己写着）"),
 "K-R8":  ("乙","后端本体","-","主干 ac86cfe · audits/K-R8-PM.md",
           "成立","D1 答完，真落地那拍没做，它自己列了 8 项写区外文件；要用户点排期（推荐 B′）"),
 "K-R9":  ("乙","判据卫生","-","主干 4443877 · audits/K-R9-R2-PM.md",
           "已馊","理由逐字「那个洞要另立件 —— PM 下一拍立」，09-05 已注明被 K-R25 在 3aab95a 关上（guard_core::assert_block_comment_model_holds 量两个单位）"),
 "K-W1B": ("乙","后端本体","-","主干 d354094 · audits/K-W1B-PM.md",
           "成立","D3/D4/D5 未开；D3 的写区 observe/** 与 K-R12 撞（那是 daemon 侧的 observe/，别与 monitor 侧那个混）"),
 "K-W1C": ("乙","中转","界面","主干合入 · audits/K-W1C-PM.md · GATE: OK",
           "成立","D4 甲留未决（要 Windows 真机）· D2 跨区那一行 PM 落 · 建边 3 条 + verify-fail + emitter→入口那一跳仍 0 判据"),
 "K-W2D": ("乙","后端本体","中转","接线拍主干 8b474e2 · audits/K-W2D-接线拍-PM.md",
           "成立","两处拦路都在 PM 手上：① panorama 族进 inbound::REGISTRY = 动 hello 帧 commands 集 = **跨仓契约面**（仓外 aterm 是消费方）② ratchet_guard::PINS 的 SPAWN_SITES_TODAY 9→10 是放宽断言，要 PM 明裁"),
 "K-W2E": ("乙","后端本体","中转","主干合入 · audits/K-W2E-PM.md · GATE: OK",
           "半馊","「跳①③⑨ 要真 daemon · timeout(1) 缺席 · layering_guard 够不着层外调用方」仍在；而理由里那条 R4 暗路（invoke::run 不 env_clear ⇒ 插件继承 ENV_PORT/ENV_TOKEN 可回连宿主）**已经被 K-R26（已签收）关掉** —— 件住 features/K-R26-插件继承整份环境能回连宿主.md"),
 "K-W4":  ("乙","跨层","环境部署","主干合入 · audits/K-W4-PM.md",
           "半馊","正题 D2/D3/D5/D6 未做仍成立；而「另归 PM 落」那笔 09-06 已现打订正为**已经做完了**（差点让 PM 去做一件做完的活）"),
}

CLASS_NAME = {"甲":"已交付未签收（整件交付，只差手续）","乙":"部分交付（本拍已合入，件内仍有未做阶段）",
              "丙":"摸底件（exit 待摸底）","丁":"真没做（零交付）","戊":"终态（不该算进「没做」）"}


def main() -> int:
    ws = sys.argv[1] if len(sys.argv) > 1 else "."
    dj = os.path.join(ws, ".dispatch.json")
    raw = open(dj, "rb").read()
    print("量于 %s · 被测对象 %s" % (time.strftime("%Y-%m-%d %H:%M:%S %z"), os.path.abspath(ws)))
    print("  .dispatch.json  md5=%s  size=%d  mtime=%s"
          % (hashlib.md5(raw).hexdigest(), len(raw),
             time.strftime("%Y-%m-%d %H:%M:%S", time.localtime(os.path.getmtime(dj)))))
    d = json.loads(raw.decode("utf-8"))

    # ---- 盘上现算：件的 state / exit
    feats = os.path.join(ws, "features")
    state, exitv, title = {}, {}, {}
    for fn in sorted(os.listdir(feats)):
        if not fn.endswith(".md"):
            continue
        txt = open(os.path.join(feats, fn), encoding="utf-8").read()
        m = re.search(r"^>\s*id:\s*([^\s|]+)\s*\|\s*kind:\s*item\b(.*)$", txt, re.M)
        if not m:
            continue
        iid, rest = m.group(1), m.group(2)
        state[iid] = (re.search(r"state:\s*([^|\n]+)", rest) or [None, ""])[1].strip()
        exitv[iid] = (re.search(r"exit:\s*([^|\n]+)", rest) or [None, ""])[1].strip()
        h = re.search(r"^#\s+(.+)$", txt, re.M)
        title[iid] = h.group(1).strip() if h else ""

    signed = {i for i, s in state.items() if s == "已签收"}
    unsigned = set(state) - signed
    parked = set(d["parked"])
    full = set(d["parked_全文"])
    inflight = {e["item"] for e in d["inflight"]}

    def roster(name, s):
        print("%s = %d ：%s" % (name, len(s), " ".join(sorted(s)) if s else "（空）"))

    print()
    print("=== 一 · 盘面（现算，分母 = features/ 下带 `kind: item` 的件文件）===")
    print("件总数 = %d" % len(state))
    roster("已签收", signed)
    roster("未签收", unsigned)
    roster("在跑（inflight）", inflight)

    print()
    print("=== 二 · 这本账自己的三处对不上 ===")
    roster("A 停车条挂着而件已签收（结构性已馊）", parked & signed)
    roster("B 未签收而 parked 里没有它（连「为什么不推」都没有）", unsigned - parked)
    div = sorted(k for k in (parked & full) if "全文住" not in d["parked"][k])
    print("C parked 已被改写、parked_全文 没跟 = %d ：%s" % (len(div), " ".join(div)))
    print("   ⇒ `parked_全文_注` 里那句「要还原：把 parked_全文 拷回 parked 即可」对这几条是**倒退**，")
    print("     拷回去会把它们改回停车理由的旧版本。")

    print()
    print("=== 三 · 判定表与盘面的对账（尺子作用域自查）===")
    miss = sorted(unsigned - set(VERDICTS))
    extra = sorted(set(VERDICTS) - unsigned)
    print("盘上有、判定表没有 = %d ：%s" % (len(miss), " ".join(miss) if miss else "（空）"))
    print("判定表有、盘上不是未签收 = %d ：%s" % (len(extra), " ".join(extra) if extra else "（空）"))
    if miss or extra:
        print("🔴 判定表与盘面对不上 —— 下面每一个数都只在对上的那部分成立。")

    print()
    print("=== 四 · 四类分册（计数与名单同句）===")
    for c in "甲乙丙丁戊":
        s = {i for i in unsigned if i in VERDICTS and VERDICTS[i][0] == c}
        print("%s %s = %d ：%s" % (c, CLASS_NAME[c], len(s), " ".join(sorted(s)) if s else "（空）"))

    print()
    print("=== 五 · 按架构分层（主层；同拍层另列，不相加）===")
    layers = {}
    for i in unsigned:
        if i in VERDICTS:
            layers.setdefault(VERDICTS[i][1], set()).add(i)
    for k in sorted(layers, key=lambda x: -len(layers[x])):
        print("%-6s = %d ：%s" % (k, len(layers[k]), " ".join(sorted(layers[k]))))

    print()
    print("=== 六 · 停车理由时态审计（判定列是人写的，依据逐条在 TSV 里）===")
    for verdict in ("已馊", "半馊", "成立", "无理由"):
        s = {i for i in unsigned if i in VERDICTS and VERDICTS[i][4] == verdict}
        print("%-4s = %d ：%s" % (verdict, len(s), " ".join(sorted(s)) if s else "（空）"))

    print()
    print("=== 七 · 逐件 TSV ===")
    print("\t".join(["id", "state", "exit", "类", "主层", "同拍层", "停车理由今天", "交付读数住址", "依据", "标题"]))
    for i in sorted(unsigned):
        v = VERDICTS.get(i)
        if not v:
            print("\t".join([i, state[i], exitv[i], "?", "?", "?", "?", "?", "判定表里没有它", title.get(i, "")]))
            continue
        c, l1, l2, addr, tense, why = v
        print("\t".join([i, state[i], exitv[i], c, l1, l2, tense, addr, why, title.get(i, "")]))
    return 0


if __name__ == "__main__":
    sys.exit(main())
