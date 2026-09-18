#!/usr/bin/env python3
"""K-R48：那四套 e2e 的 **356 条断言**逐条判「删 / 搬 / 判不了」。

分母的来历**不是数出来的，是跑出来的**：`evidence/K-R48-356-baseline.tsv` 是 09-11
在删任何东西之前、在沙箱里真跑那四套抓下来的 356 行 PASS（12+8+264+72）。
本脚本按**区段**给判词，再逐条展开 —— 区段的边界与理由都写在 `VERDICTS` 里，
**三个数由脚本加**，不许手写（手写就会出现「加不回 356」那一族）。

跑法（工作树根）：python3 evidence/K-R48-356-verdicts.py > evidence/K-R48-356-verdicts.tsv

三个判词：
  N  删掉零损失 —— 它测的是「**这个 bash 脚本怎么写的**」，脚本没了它就该没。
  M  搬得过去   —— 它测的是「**一条起会话命令长什么样**」，那是后端今天仍要保证的性质。
  K  判不了     —— 是真契约，但本轮**没有落点**；欠着，逐条写清缺什么。

M 又分两形，**别混**：
  M-repoint  套件本体**一行不动**，只把 `CCM=$REPO/shared/ccm` 换成那个二进制。
             依据是现打的对拍读数：`evidence/K-R48-native-vs-bash-parity.sh` ⇒
             **SAME=27 / DIFF=2**，两条 DIFF 都是有意的版本号。
  M-rust     已经落成 `remote-daemon-proto/src/control/ccm/` 里的 Rust 判据；
             那些判据**逐条切过刀**（`evidence/K-R48-ccm-native-mutations.py`，18 刀 18 红）。
"""
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
BASE = ROOT / "evidence" / "K-R48-356-baseline.tsv"

# (起, 止, 判词, 形, 理由)  —— 闭区间，1 起数，与 baseline.tsv 的行号一一对应
VERDICTS = [
    # ── ccm-print-parity（12）────────────────────────────────────────────
    (1, 12, "M", "repoint",
     "「renderCli 渲出来的那行 ccm 调用，被真 ccm 解析后展开成什么」—— 这是后端今天仍要"
     "保证的命令契约，与脚本无关。套件把 PATH 上的 `ccm` 指向仓内脚本；换成指向二进制即可，"
     "**12 条断言一个字都不用改**。⚠ 它的输入来自 `npx tsx` 真跑前端 renderCli ⇒ "
     "**跨语言那一半也一起保住了**，这正是重写判据买不到的东西。"),
    # ── ccm-rbind-title（8）─────────────────────────────────────────────
    (13, 13, "M", "repoint",
     "它 `sed` 读 `shared/ccm` 取那行 `set-titles-string` 的真值（刻意不重抄一份）。"
     "那个格式串今天的唯一住址是 `ccm::RBIND_TITLE_FORMAT`（`launch.rs` 里手抄的第二份"
     "已经收掉）⇒ 取法换成问二进制，性质一个字没变。"),
    (14, 20, "M", "repoint",
     "真起一个**私有 socket** 的 tmux、把 pane 标题冲成「⠐ 理解…」再读窗口标题 —— "
     "测的是「marker 不被 claude 的状态标题冲掉」这条**真机性质**，与脚本无关。"
     "套件本体一行不动。⚠ Rust 侧那条只钉得住「格式串长什么样」，**钉不住 tmux 真的这么"
     "解释它** ⇒ 这 7 条**必须靠这套 e2e**，别当成 Rust 判据能顶。"),
    # ── ccm-cli.test（264）──────────────────────────────────────────────
    (21, 37, "M", "repoint",
     "argv 形状与 `--print` 展开的基本面（零修饰 / resume 三种写法 / --base / --model / "
     "--agent / --launcher / -- 透传 / 互斥与报错 / attach）。对拍读数里这 17 形全部 SAME。"),
    (38, 47, "M", "repoint",
     "账号解析三态 + resume/attach 的位置参数纪律。对拍里 `--account z` / 裸终端落默认号 / "
     "报错出口全部 SAME。"),
    (48, 56, "M", "repoint",
     "继承优先（`R08` 那条真机复现过的静默换号）＋ 容器路内层载荷必须显式带账号。"
     "对拍里容器路四形全部 SAME（内层载荷逐字节相同）。"),
    (57, 61, "M", "repoint",
     "`auto` cwd 的三条分支（$HOME / git 仓根 / 仓子目录 / 非 git / 工作区自身）。"
     "⚠ 原生实现把 `git rev-parse` 换成了**自己往上找 `.git`** ⇒ 这 5 条搬过去要**重跑**，"
     "`GIT_DIR`/`GIT_WORK_TREE`/`GIT_CEILING_DIRECTORIES` 那几形两边不等价（已登记）。"),
    (62, 66, "M", "repoint",
     "`deriveTmuxName` 跨语言对拍（`npx tsx` 真跑前端那个函数）。**必须靠这套 e2e** —— "
     "Rust 侧那条只钉得住本侧规则，跨语言那一半重写判据买不到。"),
    (67, 76, "N", "",
     "「在 tmux 里 + 找不到 daemon ⇒ 响亮失败 rc=2」那一族。**同一个二进制之下"
     "「找不到 daemon」这个概念不存在了** —— 敲的那个命令就是后端。这是件文件 `§0-Bx-1㈣` "
     "点名的「G · 随实现语言消失的 IPC 税」那 195 行的一部分。"),
    (77, 78, "K", "",
     "「不在 tmux 里 ⇒ 不拦，但**说一句**（本会话没有 @ccm_sid，cc-monitor 认不出它）」。"
     "🔴 **这是真的 UX 契约，而原生实现今天一个字都不说** —— 身份那条腿本轮没搬"
     "（`identity_tag` 住 daemon 侧，一次性模式怎么接它没裁）。**欠着**，"
     "缺的是：一次性模式在 tmux 内时由谁去打 `@ccm_sid`。"),
    (79, 79, "M", "rust",
     "「codex 没有身份面 ⇒ 不要求后端、不吵」。分家本身由 "
     "`the_agent_set_has_one_address_and_every_member_is_wired` 钉着（`has_identity`）。"),
    (80, 81, "N", "",
     "「账号那条腿先报、码是 4；身份那条是 2 —— 分得开」。两条腿的**码**之所以要分，"
     "是因为它们是两次跨进程往返；同一个进程之下账号解析根本不会「不可达」。"),
    (82, 83, "M", "repoint",
     "「`--print` 不受身份前置检查影响」＋ 它的非空对照。`--print` 的**纯性**是真契约"
     "（它一个请求都不发、不查状态），对拍里 `--print` 那一整族全部 SAME。"),
    (84, 86, "N", "", "夹具自检（这套环境里真的一个 daemon 都找不到 / daemon 与 manifest 刻意答不同 / 账号 f 的目录真存在）—— 夹具没了它们就没了。"),
    (87, 90, "M", "rust",
     "「账号的 configDir 来自**后端**而不是文件」＋两条反向。语义搬家之后是"
     "「**账号表只有一处真相源**」，由 `the_account_table_has_exactly_one_source` 钉住。"
     "⚠ 从前那个「daemon 与 manifest 刻意答不同」的分辨力**消失了** —— 那是 IPC 的产物，"
     "同一个进程之下两者本来就是一件事。"),
    (91, 91, "N", "", "「一趟往返答完全部问题」—— 往返没了，这条读数没有被测对象了。"),
    (92, 93, "M", "rust",
     "「`--base` 与「已继承 `CLAUDE_CONFIG_DIR`」这两条路压根不问账号表」。"
     "省的不再是一次往返而是一次读盘，但**语义一模一样**，落在 `needs_account_table`。"),
    (94, 94, "M", "rust", "「说的目录不存在 ⇒ 照旧 die（目录存在性自己判）」—— `AccountTable::config_dir_of` 的 `is_dir`，判据 `the_account_table_has_exactly_one_source` 末两行。"),
    (95, 96, "M", "rust", "「报不可用必须说出有哪些可用」＋ 那份列表的来历。落在 `picking_an_account_never_falls_back_to_a_different_one`。"),
    (97, 97, "N", "", "夹具自检（manifest 里确实有一个 daemon 没有的账号）—— 随夹具走。"),
    (98, 102, "N", "", "jq 记账尺子的自检（那 7 处继承外层 PATH 的调用与 shim 同源 / 两种读法都看得见 / 拿它量一次真解析）—— 量的是「bash 里有没有再解析一次那份文件」，脚本没了就没了。"),
    (103, 119, "N", "",
     "`KCY2` 那一整族：「没后端 ⇒ 响亮失败 rc=4」「四种原因互斥」「manifest 不是 <目录>/x 形态」"
     "「答不出也是 rc=4」。**四种原因、那个码、那道形态闸，全部是「bash 要跟另一个进程说话」"
     "这件事的税**（`--accts-dir` 表达不了别的路径名，正是因为要经 argv 传给另一个进程）。"),
    (120, 121, "M", "rust",
     "「无账号库 ⇒ **一个字都不说**，rc=0，退化为基座启动器」。这是真语义（空表是合法答案，"
     "不是失败），落在 `AccountTable::load` 的「读不到 ⇒ 空表」与 "
     "`picking_an_account_never_falls_back_to_a_different_one` 末两行。"),
    (122, 125, "N", "", "帧形状那一族（多一个未知字段照样解析对 / 改键名拿到空表）—— 「帧」是 IPC 的东西。"),
    (126, 128, "M", "rust",
     "`--ccm-probe` 那三条：能力 token 在 / 版本号跟着行为走 / 用法块里有那一行。"
     "三条都落在 `the_probe_output_is_the_shape_its_parser_expects` · "
     "`the_version_moved_because_the_implementation_did` · `every_flag_we_accept_has_a_usage_line`。"
     "⚠ 最后那条搬过来时**头一版是空转的**（`contains` 让别的行的括注顶了账），"
     "变异台第 3 刀当场逮到，已收窄成「有没有属于它自己的那一行」。"),
    (129, 152, "N", "",
     "「外部依赖计数尺子」的 15 条自检 ＋ 无 jq 时那条**纯 bash 解析路**（`acct_row_from_slice` / "
     "`daemon_out_to_table` 的 else）。前者量的是「这段 bash 起了几个外部进程」，"
     "后者整条存在的理由就是「bash 没有 JSON 解析器」。两样在 Rust 里都不存在。"),
    (153, 199, "N", "",
     "`JSONENC` 全族 47 条：那个**手写的 bash JSON 编码器**（`json_str` + `_CCM_JSON_CTRL`）"
     "与 `jq -Rs .` 的逐字节对拍、手算表、往返旁证、接线。"
     "件文件 `§0-Bx-1㈣` 逐字把它算进「G · 随实现语言消失」那 195 行（`json_str` 28 行）。"
     "Rust 侧是 `serde_json` —— **不是我们的实现，不必我们来测**。"),
    (200, 233, "N", "",
     "`WIRE` 的 `--resolve` 那半 34 条：「发了」（调用次数 / argv 逐字 / 回帧真被 exec）"
     "「发对了」（落盘 stdin 逐字节 = 那个 JSON / 恰好 23 字节）「分得开」「配方路」。"
     "**同一个进程之下没有「上线字节」这回事** —— resume 那一问改成进程内直接答"
     "（`resolve_query::resolve_json_for_inbound`）。"),
    (234, 253, "N", "",
     "`WIRE/launch` 的「发了 / 不可达 / 腿分开」20 条。同上：没有往返，就没有"
     "「调用了几次」「不可达是哪一格」这些读数的被测对象。"),
    (254, 260, "M", "rust",
     "`WIRE/launch/发对了①–⑤` ＋ `缺省尺寸①②`：mode 逐字 `create-or-attach` · "
     "width/height 真在 · `@ccm_agent` 真在 · 不给尺寸时**没有**那两个键。"
     "这七件今天由 `the_container_launch_goes_through_the_one_door_with_every_field_intact` 逐条钉。"),
    (261, 263, "N", "", "「预言机 / 手算 / 往返」三条 —— 测的是那个手写编码器把 cwd 编对了没有（同 JSONENC 族）。"),
    (264, 268, "N", "",
     "「同一件事①–⑤」：后端那条请求的 `.payload` 逐字节等于**本机配方**跑出来的 send-keys。"
     "🔴 那条判据存在的理由就是「**两条路是两份实现**」；今天 `--print` 与真跑读同一个 `Plan` "
     "⇒ 它买的东西变成结构事实（`print_and_exec_cannot_drift_because_they_read_the_same_plan`），"
     "**不是被删掉，是被结构吃掉了**。"),
    (269, 276, "M", "rust",
     "控制字符那一族 8 条：载荷含换行 ⇒ 照发；含 ESC ⇒ **挡在 ccm 这一侧**、一条会话都没建、"
     "反向对照去掉 ESC 就发得出去。今天这道闸是 `launch::parse_request` 的 `check_field`，"
     "而 ccm 侧**必须经过它** —— 本轮差点漏掉：原实现直接构造 `LaunchRequest`（绕过那道门），"
     "是做这张表时逮到的，已改成走 `parse_request`，并由那条判据的末段钉住。"),
    (277, 280, "M", "rust",
     "撞名那族 4 条：`created:false` ⇒ rc=3、带上是哪个名字、不说降级、后端仍被问过。"
     "落在 `execute` 的撞名出口 ＋ `the_name_taken_message_says_which_name`"
     "（那条判据还顺手钉住「那个 `\\n` 是 printf 的转义、不是真换行」—— 对拍时逮到的）。"),
    (281, 284, "M", "repoint",
     "`--print` 的纯性 4 条：一个请求都不发、吐的仍是本机 tmux 编排、串里没有 `--launch`、"
     "stderr 一个字都没有。对拍里 `--print` 那一族全部 SAME。"),
    # ── ccm-contract-parity（72）────────────────────────────────────────
    (285, 296, "M", "repoint",
     "A 组 12 条「print↔exec 环境一致」（六个场景各一条差分自检 + 一条一致）。"
     "⚠ **别读成「结构上已经保证了所以不用测了」**：Rust 那条只钉得住「渲染函数只有 Plan 一个"
     "输入」，而 A 组钉的是「**真跑那一趟的环境**」—— 真跑要 exec 一个探针，那是 e2e 的活。"),
    (297, 303, "N", "",
     "A″ 组 7 条：账号的 configDir「来自 daemon 不是 manifest」、换一份后端值跟着换、"
     "`CCM_NO_DAEMON=1` ⇒ rc=4。夹具的要害逐字是「daemon 与 manifest 必须答不同的目录」——"
     "而同一个进程之下**两者是同一件事**，这个夹具造不出来了。"),
    (304, 309, "M", "repoint", "A′ 组 6 条「print↔exec argv 一致」（resume / resume+model / new 三场景）。同 A 组：真跑那一半靠 e2e。"),
    (310, 322, "N", "",
     "A′d / A′e 共 13 条：daemon 在位/不在位时 argv 从哪来、部署落点发现、答不出时的静默退路、"
     "`CCM_NO_DAEMON=1` 的 rc=4、两趟 stderr 都空。全是「ccm 去问另一个进程」这件事的形状。"),
    (323, 324, "M", "repoint", "「resume 真跑的 argv 逐字带 `--resume <sid>`」＋ `--print` 串说同一句。"),
    (325, 327, "M", "repoint", "cc-bus 身份 3 条：codex 在 tmux 内真跑 export `CC_BUS_ID` · `--print` 也说这一句 · **claude 不得被注入**。"),
    (328, 333, "M", "repoint", "account / model / base / 继承 / claude 四个嵌套标记清干净 / codex 不清 —— 6 条，对拍里对应各形全部 SAME。"),
    (334, 338, "M", "repoint",
     "`CCM_ENV` 5 条：被 eval 掉、`--print` 里也在、带它时 print↔exec 一致、"
     "它**早于**会话级 env（两侧各钉一条，因为差分对顺序失明）。"
     "顺序那一半今天也落了 Rust 判据 `the_machine_level_env_comes_first_and_the_session_level_one_wins`。"),
    (339, 343, "M", "repoint",
     "`--ccm-probe` 5 条：首行逐字 `name=ccm`、有 `version=` 行、抽取器自检、"
     "capabilities ⊇ TS 侧 `CLI_REQUIRED_CAPS`、agents 行。"
     "⚠ 第 4 条是**跨语言**的（从 TS 源里抽 `CLI_REQUIRED_CAPS`）—— Rust 侧那条是拿"
     "**手抄的 8 个** 比的，强度更低，**这 5 条要靠 e2e**。"),
    (344, 349, "N", "", "A′f 6 条：daemon 坏成 hang / garbage / broken 时仍落回本地、且不永远转圈（`timeout 3`）。超时与降级都是跨进程调用的形状。"),
    (350, 351, "M", "rust",
     "A′g 2 条：后端回的命令里的 `*` 不许被 cwd 的文件名改写、`$(…)` 不许被执行。"
     "🔴 **这条本轮真的差点丢**：原生实现把那条命令串交给 `sh -c` 却没有 `set -f` ⇒ "
     "glob 那一半当场退化。做这张表时逮到，已补 `set -f` 并立判据 "
     "`a_command_from_the_backend_is_never_rewritten_by_the_shell`。"),
    (352, 356, "N", "", "A′h 5 条：`$CCM_DAEMON_BIN` → 部署落点 → PATH 的查找次序，以及一个都没有时 rc=4。查找规则整条随 IPC 消失（件文件 `§0h` 逐字：「这道题消失了，不是被挑了边」）。"),
]


def main():
    rows = [l.rstrip("\n").split("\t") for l in BASE.read_text().splitlines()]
    if len(rows) != 356:
        print(f"CRASH：基线不是 356 行，是 {len(rows)} —— 分母坏了，本表作废", file=sys.stderr)
        return 2
    covered = {}
    for lo, hi, verdict, form, why in VERDICTS:
        for i in range(lo, hi + 1):
            if i in covered:
                print(f"CRASH：第 {i} 条被判了两次 —— 区段重叠", file=sys.stderr)
                return 2
            covered[i] = (verdict, form, why)
    missing = [i for i in range(1, 357) if i not in covered]
    if missing:
        print(f"CRASH：这些条一条判词都没有：{missing[:20]}（共 {len(missing)}）", file=sys.stderr)
        return 2
    print("#\t套件\t断言\t判词\t形\t理由")
    for i, (suite, desc) in enumerate(rows, 1):
        v, f, why = covered[i]
        print(f"{i}\t{suite}\t{desc}\t{v}\t{f}\t{why}")
    n = sum(1 for v, _, _ in covered.values() if v == "N")
    m = sum(1 for v, _, _ in covered.values() if v == "M")
    k = sum(1 for v, _, _ in covered.values() if v == "K")
    mr = sum(1 for v, f, _ in covered.values() if v == "M" and f == "repoint")
    mu = sum(1 for v, f, _ in covered.values() if v == "M" and f == "rust")
    print(f"\n# 合计 删 N={n} · 搬 M={m}（其中 repoint {mr} · rust {mu}） · 判不了 K={k}", file=sys.stderr)
    print(f"# N+M+K = {n + m + k}（分母 356）", file=sys.stderr)
    return 0 if n + m + k == 356 else 2


if __name__ == "__main__":
    sys.exit(main())
