#!/usr/bin/env python3
# ruff: noqa: E501
"""条 68·A1：**对外文案全集普查** —— 把「连有哪些文案都列不出来」变成「列得出来」。

住址：`<仓根>/tests/evidence/K-T68-A1-outward-copy-census.py`
服务的设计篇：`调研/设计/91-文案与报错-去AI味重写.md`（`§5` 第 1 步 · `§5.2` 的全集对账 · `§6` 那笔「比例没量过」的债）
读数落点：`调研/真相源/52-对外文案全集普查.md`

跑法（仓根下）：
    python3 tests/evidence/K-T68-A1-outward-copy-census.py
    python3 tests/evidence/K-T68-A1-outward-copy-census.py --addresses   # 逐处住址
    python3 tests/evidence/K-T68-A1-outward-copy-census.py --json        # 机读
    python3 tests/evidence/K-T68-A1-outward-copy-census.py --selftest    # 反空真死值验

退出码：
    0 = 每一格都切到了东西（地板全过）
    3 = **有格子空转**（分母触地板）—— 这是失败，不是 0 命中的绿

═══════════════════════════════════════════════════════════════════════════════
 🔴 一、「对外文案」的定义 —— 写死在这里，改定义必须改这段注释
═══════════════════════════════════════════════════════════════════════════════

这个仓踩过「同一个量有三个数同时在盘上，差的是定义不是新旧」。所以先把定义钉死。

**一条「对外文案条目」＝ 同时满足下面四条**：

  (A) **住在生产源码里** —— 文件在 `src/` 下，且不在 `EXCLUDED_DIRS` / `EXCLUDED_FILE_RE` 里；
      Rust 还要不在 `#[cfg(test)]` 块内。注释与文档注释**全部遮掉**（本仓注释里大量逐字引用
      代码与文案，不遮就会把注释里的引文数成真文案）。

  (B) **命中出口白名单** —— 出口是**穷举的一张表**（`SINKS`），不是"看起来像文案"。
      一个字面量必须处在某个已登记出口的实参/右值里才算数。
      ⇒ 白名单之外的一概不算，但**残差会被单列**（面⑦），所以"漏了什么"是看得见的。

  (C) **是字符串字面量**（含模板串） —— 纯变量转出的文本（`el.textContent = String(n)`）不算。

  (D) **含自然语言** —— 判据是「至少一个汉字」。

**本量具的单位有两个，别混**：
  · **条目**（`entry`）＝ 一个对外字面量。**这是 `91 §5.2` 全集对账的左边** ——
    将来「表里条数」要与它**相等**，不是大于等于。
  · **调用点**（`site`）＝ 一处出口调用/赋值。一处 toast 调用带标题 ＋ 正文 ⇒ 1 个调用点、2 条条目。

═══════════════════════════════════════════════════════════════════════════════
 🔴 二、这个定义**排除了什么** —— 每一条都是明写的取舍，不是忘了
═══════════════════════════════════════════════════════════════════════════════

  1. **纯英文文案**（不含汉字的字面量）。理由：今天这个产品是中文界面，判据取「含汉字」
     才能与「是不是自然语言」重合。⇒ 面⑦ 会把**纯英文候选**单独数出来当**已登记缺口**
     （`91 §6` 已承认「英文文案本篇没看」）。
  2. **`console.log/warn/error`**。理由：Tauri 发布构建里用户打不开 devtools ⇒ 不是对外面。
  3. **`tracing::info!/warn!/debug!`**。理由：内部日志。
     ⚠ **`tracing::error!` 是例外**：`91 §2.5` 逐字记了「`设计/00 §1.5.6` 第 3 级一落地它就变成对外的」
     ⇒ 它**不进主集**（今天不对外），但**单列成预备队**（面②的 `log-error` 轨），带住址。
  4. **`panic!` / `expect(` / `assert!`**。理由：崩溃信息不是界面文案，且绝大多数住在测块。
  5. **注释、文档注释、`src/doc/*.md`、`README*`**。理由：`91` 管的是"对外说的话"，不是文档。
  6. **`src/generated/`**（ts-rs 生成物）、`src/bridge/vendor/`（第三方）、`src/bridge/gen/`、
     `tests/`、`*.vitest.ts`。理由：不是人写的对外文案，改它们要改生成器/上游。
  7. **cc-bus 注入给另一个 agent 的文本**。理由：`91 §3.2` 逐字划出去了 ——
     「它不是给人看的话，是对方那一轮的输入，改它等于改对方的 prompt」。
     ⇒ `EXCLUDED_FILE_RE` 里点名了 cc-bus 的注入文本住址，面⑦ 会报它被排除了几条。
  8. **CSS 里的 `content:`**。理由：本拍没扫 `.css`（`styles.css` 18 万字节）⇒ **已登记缺口**，
     面⑨ 会把它作为"已知未扫面"打印出来，不假装扫过。

**包含但有保留的两条（说清楚，别读成确定）**：
  · `throw new Error("中文")`：它本身不是渲染面，但它的文本会被 catch 之后进 toast。
    ⇒ **算进主集**（偏保守：宁可多收一条进表，也不要漏一条没改的）。少数只进 console 的会被多算。
  · `src/backend/`（Linux 常驻端）的 `Err(...)`：它经中转回到 bridge 再决定是否露出。
    ⇒ 算进主集，但**按轨分开数**（面②），可达性存疑这一点在读数里写明。

═══════════════════════════════════════════════════════════════════════════════
 三、尺子的射程（说清楚，别读成证明）
═══════════════════════════════════════════════════════════════════════════════
  · 它数的是**源码文本**：遮注释 + 括号配平 + 逐字子串。**不编译、不跑、不看运行时可达性。**
  · `kind` 取 `91 §4` **订正后的五档**：`title` / `control` / `action` / `body` / `error`。
    TS 这半是**回溯 `document.createElement("tag")` 推出来的**；推不出来的一律记
    `unresolved`，**不猜**。面③ 会把 `unresolved` 的量印出来 ——
    那就是「R5 今天能不能只扫 `title` 档」这个问题的真答案。
  · **三带**（面①）：出口白名单分两层 —— 第 1 层是**直接渲染面**（toast / DOM / Err …），
    第 2 层是**文案字段**（`label:` / `detail:` / `consequence:` … 这些把文案装进数据结构的字段名）。
    两层之和 = **全集**；两层都没收进来的含汉字字面量 = **残差带**（面⑦），
    那是这份普查**说不准**的部分，量会被印出来，不藏。
  · 六类病里，病① 病② 病⑤ 病⑥ 是**词表命中**（可机检）；病③ 病④ 是**代理判据**，
    只能读成**下界/上界**，面⑥ 会逐条标。
"""

import argparse
import json
import re
import sys
import tempfile
from collections import Counter, defaultdict
from pathlib import Path

# ── 仓根 ────────────────────────────────────────────────────────────────────
# `tests/evidence/x.py` ⇒ parents[2] 才是仓根（见 tests/evidence/README.md 那条）。
REPO = Path(__file__).resolve().parents[2]

SRC_ROOT = REPO / "src"

EXCLUDED_DIRS = (
    "src/generated",            # ts-rs 生成物
    "src/bridge/vendor",        # 第三方
    "src/bridge/gen",           # 生成物
    "src/bridge/embedded-daemons",
    "src/bridge/scripts",
    "src/bridge/icons",
    "src/bridge/capabilities",
    "src/doc",                  # 文档
)
EXCLUDED_FILE_RE = re.compile(
    r"(\.vitest\.ts|\.test\.ts|\.spec\.ts|\.d\.ts)$"
    r"|-golden\.ts$"            # 住在 src/ 里的入库夹具（launch-cli-golden.ts …）
    r"|/build\.rs$"             # 构建脚本：println!/panic! 只进构建日志
    r"|/guard_support\.rs$"     # 判据支撑，不是生产面
    r"|/tests?/"
    r"|/fixtures?/"
)
# 91 §3.2：cc-bus 注入给另一个 agent 的文本，两套规矩，别混。
CC_BUS_INJECT_FILES = (
    "src/shared/cc-bus",
    "src/bridge/src/cc_bus.rs",
    "src/bridge/src/cc_bus_deploy.rs",
)

CJK = re.compile(r"[㐀-䶿一-鿿豈-﫿]")

# ── 地板：低于它说明尺子没切到东西，必须报空转而不是报 0 ──────────────────────
FLOORS = {
    "files_scanned": 150,       # 扫到的生产文件数
    "entries_total": 300,       # 对外文案条目总数
    "sites_total": 250,         # 对外文案调用点总数
    "bucket_toast": 20,
    "bucket_dom_text": 100,
    "bucket_title": 10,
    "bucket_confirm": 3,
    "bucket_rust_err": 50,
    "r1_scanned": 300,          # R1 扫过的条目数（分母）
    "kind_resolved": 50,        # 至少这么多条 kind 是靠 tag 回溯定下来的
    "kind_title": 5,            # `title` 档（R5 的射程）不能是空的
    "bucket_copy_field": 30,    # 第 2 层出口（文案字段）不能空转
}

# ═══════════════════════════════════════════════════════════════════════════
#  出口白名单（`SINKS`）—— 定义的 (B) 这一条就是这张表
# ═══════════════════════════════════════════════════════════════════════════
#  mode: "call"   锚点落在 `(` 上，取括号配平到的那一段；按顶层逗号算实参序号
#        "assign" 锚点落在 `=` 上，取到同层 `;` 的那一段
#  kind:  固定 kind；None = 要靠 tag 回溯推（只有 DOM 文本那一族）
#  argkind: {实参序号: kind}，call 模式下按序号给 kind
SINKS = [
    # ── toast（`error-toast.ts` 是唯一的 toast 实现，其余都经它） ──────────
    dict(id="toast.failure", lang="ts", bucket="toast", mode="call", kind=None,
         re=re.compile(r"\bshowActionFailureToast\s*\("),
         argkind={0: "body", 1: "body"}, default_kind="body"),
    dict(id="toast.append", lang="ts", bucket="toast", mode="call", kind=None,
         re=re.compile(r"\bappendToast\s*\("), argkind={}, default_kind="body"),
    dict(id="toast.error", lang="ts", bucket="toast", mode="call", kind=None,
         re=re.compile(r"\bshowErrorToast\s*\("), argkind={}, default_kind="body"),

    # ── 模态确认 / 浏览器原生对话框 ──────────────────────────────────────
    dict(id="dialog.confirm", lang="ts", bucket="confirm", mode="call", kind="body",
         re=re.compile(r"(?<![\w.])(?:window\.)?confirm\s*\(")),
    dict(id="dialog.alert", lang="ts", bucket="confirm", mode="call", kind="body",
         re=re.compile(r"(?<![\w.])(?:window\.)?alert\s*\(")),

    # ── 元素文本（kind 靠回溯 createElement 的 tag 推） ────────────────────
    dict(id="dom.textContent", lang="ts", bucket="dom-text", mode="assign", kind=None,
         re=re.compile(r"([A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)*)\s*\.\s*textContent\s*=(?!=)")),
    dict(id="dom.innerText", lang="ts", bucket="dom-text", mode="assign", kind=None,
         re=re.compile(r"([A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)*)\s*\.\s*innerText\s*=(?!=)")),
    dict(id="dom.innerHTML", lang="ts", bucket="dom-html", mode="assign", kind="unresolved",
         re=re.compile(r"([A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)*)\s*\.\s*innerHTML\s*=(?!=)")),

    # ── 属性面 ──────────────────────────────────────────────────────────
    dict(id="attr.title", lang="ts", bucket="title", mode="assign", kind="body",
         re=re.compile(r"[\w$\].)]\s*\.\s*title\s*=(?!=)")),
    dict(id="attr.title.setAttr", lang="ts", bucket="title", mode="call", kind="body",
         re=re.compile(r"\.setAttribute\s*\(\s*\"title\"")),
    dict(id="attr.placeholder", lang="ts", bucket="placeholder", mode="assign", kind="body",
         re=re.compile(r"[\w$\].)]\s*\.\s*placeholder\s*=(?!=)")),
    dict(id="attr.placeholder.setAttr", lang="ts", bucket="placeholder", mode="call", kind="body",
         re=re.compile(r"\.setAttribute\s*\(\s*\"placeholder\"")),
    dict(id="attr.aria", lang="ts", bucket="a11y", mode="call", kind="unresolved",
         re=re.compile(r"\.setAttribute\s*\(\s*\"aria-label\"")),
    dict(id="attr.ariaLabel", lang="ts", bucket="a11y", mode="assign", kind="unresolved",
         re=re.compile(r"[\w$\].)]\s*\.\s*ariaLabel\s*=(?!=)")),

    # ── 造件包装器（它们把字面量塞进 `.textContent`，不登记就会漏） ────────
    dict(id="wrap.mkBtn", lang="ts", bucket="dom-text", mode="call", kind="action",
         re=re.compile(r"\bmk(?:Row)?Btn\s*\(")),
    dict(id="wrap.tabMenuButton", lang="ts", bucket="dom-text", mode="call", kind="action",
         re=re.compile(r"\bmakeTabMenuButton\s*\(")),
    dict(id="wrap.infoIcon", lang="ts", bucket="title", mode="call", kind="body",
         re=re.compile(r"\bmakeInfoIcon\s*\(")),
    dict(id="wrap.statusLine", lang="ts", bucket="dom-text", mode="call", kind="body",
         re=re.compile(r"\bmakeStatus(?:Line|Row)\s*\(")),
    dict(id="wrap.chip", lang="ts", bucket="dom-text", mode="call", kind="unresolved",
         re=re.compile(r"\bmakeChip\s*\(")),
    dict(id="wrap.sideNote", lang="ts", bucket="dom-text", mode="call", kind="body",
         re=re.compile(r"\bmakeSideNote\s*\(")),
    dict(id="wrap.collapsible", lang="ts", bucket="dom-text", mode="call", kind="title",
         re=re.compile(r"\bmakeCollapsible\s*\(")),

    # ── 前端抛错（保留条：见定义 §二"包含但有保留"） ──────────────────────
    dict(id="ts.throw", lang="ts", bucket="ts-throw", mode="call", kind="error",
         re=re.compile(r"\bthrow\s+new\s+\w*Error\s*\(")),

    # ── 后端报错（`Result<_, String>` 一路回到前端 toast） ─────────────────
    dict(id="rs.err", lang="rs", bucket="rust-err", mode="call", kind="error",
         re=re.compile(r"(?<![\w.])Err\s*\(")),
    dict(id="rs.maperr", lang="rs", bucket="rust-err", mode="call", kind="error",
         re=re.compile(r"\.map_err\s*\(")),
    dict(id="rs.okor", lang="rs", bucket="rust-err", mode="call", kind="error",
         re=re.compile(r"\.ok_or(?:_else)?\s*\(")),

    # ══ 第 2 层出口：**文案字段** ══════════════════════════════════════
    # 文案不一定直接写进 DOM —— 大量是先装进一个结构体/对象字段，再由渲染层取出来贴。
    # ⇒ 字段名白名单（每个名字都是「这一格装的是给人看的话」才登记；
    #    `name:` / `id:` / `cwd:` 这种既可能装文案也可能装内部标识的，**不登记**，留在残差带）。
    dict(id="field.title", lang="both", bucket="copy-field", mode="field", kind="title",
         re=re.compile(r"\b(heading|emptyTitle|subtitle|sidebarHeader|cardHeader|display_name)\s*:")),
    dict(id="field.action", lang="both", bucket="copy-field", mode="field", kind="action",
         re=re.compile(r"\b(label|actionLabel|menuLabel|currentMark)\s*:")),
    dict(id="field.body", lang="both", bucket="copy-field", mode="field", kind="body",
         re=re.compile(
             r"\b(text|detail|detailText|tooltip|note|hint|summary|caption|body|message|msg|"
             r"keywords|consequence|success|successDetail|emptyNext|countSuffix|manifestPrefix|"
             r"unjumpableHint|confirmExtra|mergeNote|activation|what|what_next|fix|target|"
             r"placeholder|description|desc)\s*:")),
    dict(id="field.error", lang="both", bucket="copy-field", mode="field", kind="error",
         re=re.compile(
             r"\b(why|reason|error|failureCopied|failureNotCopied|loadFailed|unknownReason|"
             r"errorText|failure)\s*:")),

    # ══ 第 2 层出口：**展示助手** ══════════════════════════════════════
    dict(id="show.prompt", lang="ts", bucket="confirm", mode="call", kind="body",
         re=re.compile(r"(?<![\w.])(?:window\.)?prompt\s*\(")),
    dict(id="show.status", lang="ts", bucket="dom-text", mode="call", kind="body",
         re=re.compile(r"\.(?:showBanner|showLoading|showMessage|showResultText|"
                       r"renderSidebarStatus|info|note|showToast)\s*\(")),
    dict(id="show.section", lang="ts", bucket="dom-text", mode="call", kind="title",
         re=re.compile(r"\.(?:safeBlock|buildGroup|edgeSection|sidebarHeader|cardHeader)\s*\(|"
                       r"\b(?:buildGroup|edgeSection|sidebarHeader|cardHeader)\s*\(")),
    dict(id="show.field", lang="ts", bucket="dom-text", mode="call", kind="control",
         re=re.compile(r"\b(?:mkField|mkInput|addFact|mkResumeRow)\s*\(")),
    dict(id="show.btn", lang="ts", bucket="dom-text", mode="call", kind="action",
         re=re.compile(r"\b(?:makeBtn|menuAction)\s*\(")),
    dict(id="show.textnode", lang="ts", bucket="dom-text", mode="call", kind="body",
         re=re.compile(r"document\.createTextNode\s*\(")),

    # ── 预备队：今天不对外，`00 §1.5.6` 第 3 级一落地就对外（91 §2.5） ─────
    dict(id="rs.tracing.error", lang="rs", bucket="log-error", mode="call", kind="log",
         re=re.compile(r"\btracing::error!\s*\(|(?<![\w:])error!\s*\(")),
]
# 预备队不进主集
RESERVE_BUCKETS = {"log-error"}

# ── kind：DOM 文本靠回溯 `createElement("tag")` 推 ──────────────────────────
# `91 §4` 订正后的五档：title / control / action / body / error
TAG_KIND = {
    "button": "action", "a": "action",
    "h1": "title", "h2": "title", "h3": "title", "h4": "title", "h5": "title", "h6": "title",
    "legend": "title", "th": "title", "caption": "title", "summary": "title",
    # `<label>` / `<option>` 是**控件标签**，不是区块名 ⇒ control（「启用 X」在这一档是标准形）
    "label": "control", "option": "control", "optgroup": "control",
    "p": "body", "li": "body", "td": "body", "pre": "body", "code": "body",
    "small": "body", "figcaption": "body",
    # div/span/section/… 一律 unresolved：它们既能当区块名也能当正文，**不猜**
}
NAME_ACTION_RE = re.compile(r"(btn|button)$", re.I)
NAME_TITLE_RE = re.compile(r"(title|heading|header|caption|legend)$", re.I)

# ═══════════════════════════════════════════════════════════════════════════
#  R1 禁词表（`91 §4` 逐字抄的那张） —— 顺序 = 匹配优先级（长的先吃）
# ═══════════════════════════════════════════════════════════════════════════
R1_WORDS = [
    ("@ccm_sid", re.compile(r"@ccm_sid")),
    ("ccm wrapper", re.compile(r"ccm\s*wrapper", re.I)),
    ("hwnd cache", re.compile(r"hwnd\s*cache", re.I)),
    ("daemon", re.compile(r"daemon", re.I)),
    ("pidfile", re.compile(r"pid\s*file", re.I)),
    ("jsonl", re.compile(r"jsonl", re.I)),
    ("inotify", re.compile(r"inotify", re.I)),
    ("HWND", re.compile(r"\bHWND\b")),
    ("relay", re.compile(r"\brelay\b", re.I)),
    ("sid", re.compile(r"(?<![\w@])sid(?![\w])", re.I)),
    ("判据", re.compile(r"判据")),
    ("守卫", re.compile(r"守卫")),
    ("死亡账", re.compile(r"死亡账")),
    ("尺子", re.compile(r"尺子")),
    ("棘轮", re.compile(r"棘轮")),
    ("空真", re.compile(r"空真")),
    ("恒绿", re.compile(r"恒绿")),
    ("墓碑", re.compile(r"墓碑")),
    ("活体", re.compile(r"活体")),
    ("写区", re.compile(r"写区")),
    ("定框", re.compile(r"定框")),
    ("铸名", re.compile(r"铸名")),
    ("灰", re.compile(r"灰")),
]
# `灰` 的纯颜色义（灰色/灰度/灰阶）不是 `设计/30 §3.5.2` 说的那个状态词。
# ⚠ **`变灰` / `置灰` 不在这里** —— 「tab 稍后自动变灰」正是 §3.5.2 点名的那个用法
#   （指代不明：到底是「已结束」还是「可重连」）。第一版把它一起吞了，读数因此是 0。
HUI_COLOR_RE = re.compile(r"灰色|灰度|灰阶")

# R1 候选增补（**不在 §4 词表里**，单列给规范作者拍板，不混进 R1 读数）
R1_CANDIDATES = [
    ("tmux", re.compile(r"\btmux\b", re.I)),
    ("ccm", re.compile(r"(?<![\w-])ccm(?![\w-])", re.I)),
    ("IPC", re.compile(r"\bIPC\b")),
    ("stderr/stdout", re.compile(r"\bstd(?:err|out)\b", re.I)),
    ("PTY", re.compile(r"\bpty\b", re.I)),
    ("WebView", re.compile(r"webview", re.I)),
    ("socket", re.compile(r"\bsocket\b", re.I)),
    ("fd", re.compile(r"(?<![\w])fd(?![\w])")),
    ("serde/序列化", re.compile(r"\bserde\b|序列化", re.I)),
    ("hook", re.compile(r"\bhook\b|钩子", re.I)),
]

# ═══════════════════════════════════════════════════════════════════════════
#  R5 词表（`91 §4` 逐字）—— **只扫 label**（`§4` 那条 ⚠ 的硬要求）
# ═══════════════════════════════════════════════════════════════════════════
R5_COLLOQUIAL = [
    ("还差", re.compile(r"还差")),
    ("怎么", re.compile(r"怎么")),
    ("有没有", re.compile(r"有没有")),
    ("要不要", re.compile(r"要不要")),
    ("好了吗", re.compile(r"好了吗")),
    ("行不行", re.compile(r"行不行")),
]
R5_QUESTION = re.compile(r"[？?]")
# `91 §4` 订正后新增的第 ③ 检：**以祈使动词开头**（只在 `title` 档里判红）
R5_IMPERATIVE = re.compile(r"^\s*(启用|停用|显示|隐藏|打开|关闭|设置|选择|输入|点击|查看|管理)")
# 插值点里的 `?` 不是问句：Rust 的 `{:?}` / `{name:?}` 会假命中 R5 的第 ① 检。
INTERP_RE = re.compile(r"\{[^{}]*\}")


def speech(text: str) -> str:
    """把插值点剥掉之后的「人话」—— R5 与病④ 的形状判据都对它跑，不对原文跑。"""
    return INTERP_RE.sub("", text)

# ═══════════════════════════════════════════════════════════════════════════
#  六类病的机检代理（`91 §2`）
# ═══════════════════════════════════════════════════════════════════════════
# 病① 内部标识符外泄：R1 里的**技术名**子集
D1_WORDS = {"@ccm_sid", "ccm wrapper", "hwnd cache", "daemon", "pidfile", "jsonl",
            "inotify", "HWND", "relay", "sid"}
# 病② 内部推理外泄 ＋ 病⑤ 自造比喻：R1 里的**行话/比喻**子集
D2_WORDS = {"判据", "守卫", "尺子", "棘轮", "空真", "恒绿", "墓碑", "活体", "写区",
            "定框", "铸名", "死亡账"}
D5_WORDS = {"死亡账", "尺子", "棘轮", "空真", "恒绿", "墓碑", "活体", "定框", "铸名"}
# 病③ 建议式口吻（**代理判据，读成下界**）
D3_RE = re.compile(r"建议|请先|请改|请重|请手动|请检查|请用|请在|需要你|你可以|可尝试|不妨|最好")
# 病④ AI 味形状（**代理判据**：三个信号各自数，再数同时命中）
D4_DASH_RE = re.compile(r"——|--(?!>)|—")
D4_PAREN_RE = re.compile(r"（[^）]{2,}）|\([^)]{4,}\)")
# 首句汉字数上限。`91 §5.2⑤` 今天**还没定数** ⇒ 这里给一个明写的、有分布支撑的阈值：
# 现打分布（见面⑥）首句汉字数 p95≈24 · p99≈31 · max=39 ⇒ 取 30 ≈ 最长的 1%。
# ⚠ **阈值一改，病④ 的数就变** —— 所以它印在读数里，不藏在代码里。
D4_LONG_CHARS = 30
D4_SENT_SPLIT = re.compile(r"[。！？!?；;\n]")

# 残差里**明写排除**的那些出口（定义 §二 的第 2·3·4 条）—— 要与「说不准」分开数，
# 否则「残差 1286」会被读成「有 1286 条漏了」，而其中一大半是**故意不要**的。
DECLINED_CTX = re.compile(
    r"tracing::(?:warn|info|debug|trace)!|console\.(?:log|warn|error|debug|info)|"
    r"(?<![\w])(?:panic|eprintln|println|unreachable|todo|unimplemented|assert|assert_eq|"
    r"assert_ne|debug_assert)!|\.expect(?:_err)?\s*\(|\.unwrap_err\s*\(|"
    r"(?<![\w:])(?:warn|info|debug)!\("
)


# ═══════════════════════════════════════════════════════════════════════════
#  词法：遮注释（保留字符串）· 剥 `#[cfg(test)]`
# ═══════════════════════════════════════════════════════════════════════════
def mask_comments(src: str, lang: str) -> str:
    """把注释换成空格（长度、行号不变），**字符串原样保留**。

    为什么必须遮：本仓注释里逐字引用文案与代码（`error-toast.ts` 的大段注释里就有
    `failedHosts.join("、")`）。不遮 ⇒ 注释里的引文会被数成真文案，数就假了。
    """
    n = len(src)
    out = list(src)
    i = 0
    prev_sig = ""

    def blank(a: int, b: int) -> None:
        for k in range(a, min(b, n)):
            if out[k] != "\n":
                out[k] = " "

    while i < n:
        c = src[i]
        nxt = src[i + 1] if i + 1 < n else ""

        # Rust 原始串 r"..." / r#"..."#
        if lang == "rs" and c == "r" and (nxt == '"' or nxt == "#"):
            j = i + 1
            hashes = 0
            while j < n and src[j] == "#":
                hashes += 1
                j += 1
            if j < n and src[j] == '"':
                term = '"' + "#" * hashes
                k = src.find(term, j + 1)
                k = n if k < 0 else k + len(term)
                i = k
                prev_sig = '"'
                continue

        if c in ('"', "'") or (lang == "ts" and c == "`"):
            q = c
            j = i + 1
            while j < n:
                ch = src[j]
                if ch == "\\":
                    j += 2
                    continue
                if ch == q:
                    j += 1
                    break
                if q == "`" and ch == "$" and j + 1 < n and src[j + 1] == "{":
                    d = 0
                    k = j + 1
                    while k < n:
                        if src[k] == "{":
                            d += 1
                        elif src[k] == "}":
                            d -= 1
                            if d == 0:
                                break
                        k += 1
                    j = k + 1
                    continue
                if q in ('"', "'") and ch == "\n":
                    break        # 单行串不跨行：防一个失配吞掉整个文件
                j += 1
            i = j
            prev_sig = q
            continue

        if c == "/" and nxt == "/":
            j = src.find("\n", i)
            j = n if j < 0 else j
            blank(i, j)
            i = j
            continue

        if c == "/" and nxt == "*":
            j = src.find("*/", i + 2)
            j = n if j < 0 else j + 2
            blank(i, j)
            i = j
            continue

        # TS 正则字面量：只在"这里能放一个表达式"的位置才认，且必须同行闭合
        if lang == "ts" and c == "/" and prev_sig in "(,=:[!&|?{;+\n" or (
            lang == "ts" and c == "/" and prev_sig == ""
        ):
            eol = src.find("\n", i)
            eol = n if eol < 0 else eol
            j = i + 1
            in_cls = False
            ok = False
            while j < eol:
                ch = src[j]
                if ch == "\\":
                    j += 2
                    continue
                if ch == "[":
                    in_cls = True
                elif ch == "]":
                    in_cls = False
                elif ch == "/" and not in_cls:
                    ok = True
                    j += 1
                    break
                j += 1
            if ok:
                i = j
                prev_sig = "/"
                continue

        if not c.isspace():
            prev_sig = c
        i += 1

    return "".join(out)


def strip_cfg_test(masked: str) -> str:
    """把 `#[cfg(test)]` 后面那个 `{...}` 整块换成空格（行号不变）。"""
    out = list(masked)
    for m in re.finditer(r"#\[cfg\((?:test|all\([^)]*test[^)]*\))\)\]", masked):
        i = masked.find("{", m.end())
        if i < 0:
            continue
        d = 0
        j = i
        while j < len(masked):
            if masked[j] == "{":
                d += 1
            elif masked[j] == "}":
                d -= 1
                if d == 0:
                    break
            j += 1
        for k in range(m.start(), min(j + 1, len(masked))):
            if out[k] != "\n":
                out[k] = " "
    return "".join(out)


def iter_strings(src: str, lang: str, lo: int, hi: int):
    """在 [lo,hi) 里列出字符串字面量：(start, end, 归一化文本)。

    模板串 / `format!` 的插值点归一成 `{…}` —— 这一格服务 `91 §5.1-3`（参数化要留在表里）。
    """
    i = lo
    n = min(hi, len(src))
    while i < n:
        c = src[i]
        if lang == "rs" and c == "r" and i + 1 < n and src[i + 1] in '#"':
            j = i + 1
            hashes = 0
            while j < n and src[j] == "#":
                hashes += 1
                j += 1
            if j < n and src[j] == '"':
                term = '"' + "#" * hashes
                k = src.find(term, j + 1)
                if k < 0:
                    break
                yield (i, k + len(term), src[j + 1:k])
                i = k + len(term)
                continue
        if c in ('"', "'") or (lang == "ts" and c == "`"):
            q = c
            j = i + 1
            buf = []
            while j < n:
                ch = src[j]
                if ch == "\\":
                    buf.append({"n": "\n", "t": "\t"}.get(src[j + 1: j + 2], src[j + 1: j + 2]))
                    j += 2
                    continue
                if ch == q:
                    j += 1
                    break
                if q == "`" and ch == "$" and j + 1 < n and src[j + 1] == "{":
                    d = 0
                    k = j + 1
                    while k < n:
                        if src[k] == "{":
                            d += 1
                        elif src[k] == "}":
                            d -= 1
                            if d == 0:
                                break
                        k += 1
                    buf.append("{…}")
                    j = k + 1
                    continue
                if q in ('"', "'") and ch == "\n":
                    break
                buf.append(ch)
                j += 1
            yield (i, j, "".join(buf))
            i = j
            continue
        i += 1


def span_of_call(src: str, open_paren: int, cap: int = 6000) -> int:
    """从 `(` 起括号配平，返回闭合后一位（跳字符串）。"""
    d = 0
    i = open_paren
    n = min(len(src), open_paren + cap)
    while i < n:
        c = src[i]
        if c in ('"', "'", "`"):
            for s, e, _ in iter_strings(src, "ts", i, n):
                i = e
                break
            else:
                i += 1
            continue
        if c == "(":
            d += 1
        elif c == ")":
            d -= 1
            if d == 0:
                return i + 1
        i += 1
    return n


def span_of_field(src: str, colon: int, cap: int = 2000) -> int:
    """从字段名的 `:` 起到同层 `,` 或 `}`（跳字符串与括号）。"""
    d = 0
    i = colon + 1
    n = min(len(src), colon + cap)
    while i < n:
        c = src[i]
        if c in ('"', "'", "`"):
            for _s, e, _t in iter_strings(src, "ts", i, n):
                i = e
                break
            else:
                i += 1
            continue
        if c in "([{":
            d += 1
        elif c in ")]}":
            if d == 0:
                return i
            d -= 1
        elif c == "," and d == 0:
            return i
        i += 1
    return n


def span_of_assign(src: str, eq: int, cap: int = 3000) -> int:
    """从 `=` 起到同层 `;`（跳字符串与括号）。"""
    d = 0
    i = eq + 1
    n = min(len(src), eq + cap)
    while i < n:
        c = src[i]
        if c in ('"', "'", "`"):
            for s, e, _ in iter_strings(src, "ts", i, n):
                i = e
                break
            else:
                i += 1
            continue
        if c in "([{":
            d += 1
        elif c in ")]}":
            if d == 0:
                return i
            d -= 1
        elif c == ";" and d == 0:
            return i
        i += 1
    return n


def arg_index_at(src: str, open_paren: int, pos: int) -> int:
    """pos 处的字面量是第几个实参（顶层逗号计数）。"""
    d = 0
    idx = 0
    i = open_paren
    while i < pos and i < len(src):
        c = src[i]
        if c in ('"', "'", "`"):
            for s, e, _ in iter_strings(src, "ts", i, pos + 1):
                i = e
                break
            else:
                i += 1
            continue
        if c in "([{":
            d += 1
        elif c in ")]}":
            d -= 1
        elif c == "," and d == 1:
            idx += 1
        i += 1
    return idx


# ═══════════════════════════════════════════════════════════════════════════
#  kind 回溯：`X.textContent = "…"` 里的 X 是哪个 tag 造出来的
# ═══════════════════════════════════════════════════════════════════════════
def build_tag_map(masked: str):
    """→ {表达式文本: [(offset, tag), ...]}（按 offset 升序）。"""
    tags = defaultdict(list)
    pat = re.compile(
        r"(?:const|let|var)?\s*([A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)*)\s*="
        r"\s*document\.createElement\s*\(\s*[\"'`]([a-zA-Z0-9]+)[\"'`]"
    )
    for m in pat.finditer(masked):
        tags[m.group(1)].append((m.start(), m.group(2).lower()))
    return tags


def resolve_kind(expr: str, off: int, tagmap) -> tuple:
    """→ (kind, 来源)；来源 ∈ {tag, name, none}。**推不出来就 unresolved，不猜。**"""
    cands = tagmap.get(expr)
    if cands:
        best = None
        for o, t in cands:
            if o < off:
                best = t
        if best is None:                       # 只在后面出现（很少）⇒ 取第一个
            best = cands[0][1]
        k = TAG_KIND.get(best)
        if k:
            return k, "tag"
        return "unresolved", "tag"             # div/span：tag 认出来了，但它定不了 kind
    leaf = expr.split(".")[-1]
    if NAME_ACTION_RE.search(leaf):
        return "action", "name"
    if NAME_TITLE_RE.search(leaf):
        return "title", "name"
    return "unresolved", "none"


# ═══════════════════════════════════════════════════════════════════════════
#  扫描
# ═══════════════════════════════════════════════════════════════════════════
def production_files(root: Path):
    out = []
    if not root.exists():
        return out
    for p in sorted(root.rglob("*")):
        if not p.is_file() or p.suffix not in (".ts", ".rs"):
            continue
        rel = p.relative_to(REPO).as_posix() if REPO in p.parents or p.is_relative_to(REPO) else p.as_posix()
        if any(rel.startswith(d + "/") or rel == d for d in EXCLUDED_DIRS):
            continue
        if EXCLUDED_FILE_RE.search("/" + rel):
            continue
        out.append((p, rel))
    return out


def line_of(src: str, off: int) -> int:
    return src.count("\n", 0, off) + 1


def scan(root: Path):
    entries = []          # 主集 + 预备队（带 bucket 区分）
    residual = []         # 定义之外的含汉字字面量
    english = []          # 纯英文候选（已登记缺口）
    ccbus_excluded = 0
    files = production_files(root)
    scanned_lines = 0

    for path, rel in files:
        try:
            raw = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        lang = "rs" if path.suffix == ".rs" else "ts"
        masked = mask_comments(raw, lang)
        if lang == "rs":
            masked = strip_cfg_test(masked)
        scanned_lines += masked.count("\n") + 1
        is_ccbus = any(rel.startswith(x) for x in CC_BUS_INJECT_FILES)
        tagmap = build_tag_map(masked) if lang == "ts" else {}

        consumed = set()   # 已被某个出口吃掉的字面量起点
        for sink in SINKS:
            if sink["lang"] not in (lang, "both"):
                continue
            for m in sink["re"].finditer(masked):
                if sink["mode"] == "field":
                    col = masked.find(":", m.end() - 1)
                    if col < 0:
                        continue
                    end = span_of_field(masked, col)
                    lo, hi = col + 1, end
                    op = None
                elif sink["mode"] == "call":
                    op = masked.find("(", m.end() - 1)
                    if op < 0:
                        continue
                    end = span_of_call(masked, op)
                    lo, hi = op, end
                else:
                    eq = masked.rfind("=", m.start(), m.end())
                    if eq < 0:
                        continue
                    end = span_of_assign(masked, eq)
                    lo, hi = eq + 1, end
                    op = None

                got_any = False
                for s, e, text in iter_strings(masked, lang, lo, hi):
                    if s in consumed:
                        continue
                    if not CJK.search(text):
                        if text.strip() and re.search(r"[A-Za-z]{2,}", text) and not re.fullmatch(
                            r"[\w./\-:#{}\[\]$… ]*", text
                        ):
                            english.append(dict(file=rel, line=line_of(masked, s), text=text[:120],
                                                sink=sink["id"]))
                        continue
                    consumed.add(s)
                    got_any = True
                    if is_ccbus:
                        ccbus_excluded += 1
                        continue

                    # kind
                    if sink["kind"] is not None:
                        kind, ksrc = sink["kind"], "sink"
                    elif sink.get("argkind") is not None and op is not None:
                        ai = arg_index_at(masked, op, s)
                        kind = sink["argkind"].get(ai, sink.get("default_kind", "body"))
                        ksrc = "sink"
                    else:
                        expr = m.group(1) if m.groups() else ""
                        kind, ksrc = resolve_kind(expr, m.start(), tagmap)

                    entries.append(dict(
                        file=rel, line=line_of(masked, s), bucket=sink["bucket"],
                        sink=sink["id"], kind=kind, kind_src=ksrc, lang=lang,
                        track=track_of(rel, lang), text=text,
                        params=text.count("{…}") + len(re.findall(r"\{[a-zA-Z_][\w.]*\}", text)),
                        site=(rel, line_of(masked, m.start()), sink["id"]),
                    ))
                if got_any:
                    pass

        # 残差：所有含汉字的字面量里，没被任何出口吃掉的那些
        for s, e, text in iter_strings(masked, lang, 0, len(masked)):
            if s in consumed or not CJK.search(text):
                continue
            ctx = masked[max(0, s - 60):s]
            ctx = re.sub(r"\s+", " ", ctx).strip()[-48:]
            residual.append(dict(file=rel, line=line_of(masked, s), ctx=ctx, text=text[:80]))

    return files, scanned_lines, entries, residual, english, ccbus_excluded


def mask_sanity(files):
    """遮罩自检：遮注释**只该**减少注释里的引文，不该把真代码吞掉。

    本仓注释大量逐字引用代码 ⇒ 差值必然 > 0；差值等于原文数（遮后归零）才是遮罩坏了。
    """
    raw = masked = 0
    anchor = ".textContent ="
    for path, rel in files:
        if not rel.endswith(".ts"):
            continue
        try:
            src = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        raw += src.count(anchor)
        masked += mask_comments(src, "ts").count(anchor)
    return raw, masked


def track_of(rel: str, lang: str) -> str:
    if lang == "ts":
        return "前端 TS"
    if rel.startswith("src/backend/"):
        return "常驻端 Rust"
    return "桥 Rust"


# ═══════════════════════════════════════════════════════════════════════════
#  报告
# ═══════════════════════════════════════════════════════════════════════════
def pct(a: int, b: int) -> str:
    return f"{100.0 * a / b:5.1f}%" if b else "  n/a"


def word_hits(entries, table, skip_hui_color=False):
    """→ {词: [条目...]}，长词先吃（同一条里 `@ccm_sid` 不再算一次 `sid`）。"""
    hits = defaultdict(list)
    for ent in entries:
        text = ent["text"]
        used = []
        for name, rx in table:
            probe = text
            for a, b in used:
                probe = probe[:a] + " " * (b - a) + probe[b:]
            found = list(rx.finditer(probe))
            if name == "灰" and skip_hui_color:
                found = [f for f in found
                         if not HUI_COLOR_RE.search(text[max(0, f.start() - 1):f.start() + 2])]
            if found:
                hits[name].append(ent)
                for f in found:
                    used.append((f.start(), f.end()))
    return hits


def disease_stats(main):
    r1 = word_hits(main, R1_WORDS, skip_hui_color=True)
    d1 = {id(e) for w in D1_WORDS for e in r1.get(w, [])}
    d2 = {id(e) for w in D2_WORDS for e in r1.get(w, [])}
    d5 = {id(e) for w in D5_WORDS for e in r1.get(w, [])}
    d3 = [e for e in main if D3_RE.search(e["text"])]
    dash = [e for e in main if D4_DASH_RE.search(speech(e["text"]))]
    paren = [e for e in main if D4_PAREN_RE.search(speech(e["text"]))]

    def first_len(e):
        return len(CJK.findall(D4_SENT_SPLIT.split(speech(e["text"]))[0]))

    longs = [e for e in main if first_len(e) >= D4_LONG_CHARS]
    d4_all = [e for e in main
              if sum([bool(D4_DASH_RE.search(speech(e["text"]))),
                      bool(D4_PAREN_RE.search(speech(e["text"]))),
                      first_len(e) >= D4_LONG_CHARS]) >= 2]
    dist = sorted(first_len(e) for e in main)
    return dict(r1=r1, d1=d1, d2=d2, d5=d5, d3=d3, dash=dash, paren=paren,
                longs=longs, d4_all=d4_all, dist=dist)


def main_report(args) -> int:
    files, lines, entries, residual, english, ccbus = scan(SRC_ROOT)
    main = [e for e in entries if e["bucket"] not in RESERVE_BUCKETS]
    reserve = [e for e in entries if e["bucket"] in RESERVE_BUCKETS]
    sites = {e["site"] for e in main}
    efiles = {e["file"] for e in main}

    P = print
    P("═" * 78)
    P("条 68·A1  对外文案全集普查   仓根：" + str(REPO))
    P("═" * 78)

    # ── 面0：语料与定义自检 ─────────────────────────────────────────────
    P("\n【面0】语料 —— 定义的 (A) 这一条量出来是多少")
    P(f"  生产源码文件（.ts/.rs，已去生成物/第三方/测试/文档）  : {len(files)}")
    P(f"    · .ts                                             : {sum(1 for _, r in files if r.endswith('.ts'))}")
    P(f"    · .rs                                             : {sum(1 for _, r in files if r.endswith('.rs'))}")
    P(f"  遮注释 + 剥 #[cfg(test)] 之后的行数                  : {lines}")
    P(f"  明写排除：{', '.join(EXCLUDED_DIRS)}")
    P(f"  cc-bus 注入文本（91 §3.2 划出去的）被排除条数         : {ccbus}")
    P("  ⚠ 未扫面（已登记，不假装扫过）：.css 的 content: · .html · README*.md · 英文文案")
    raw_anchor, masked_anchor = mask_sanity(files)
    P(f"  遮罩自检：`.textContent =` 原文 {raw_anchor} 处 / 遮注释后 {masked_anchor} 处 "
      f"（差 {raw_anchor - masked_anchor} = 注释里的引文）")
    if masked_anchor == 0 and raw_anchor > 0:
        P("  🔴 遮罩把真代码一起吞了 —— 读数不可信。")

    # ── 面①：全集 ─────────────────────────────────────────────────────
    P("\n【面①】全集读数 —— 「列不出来」到此为止")
    t1 = [e for e in main if e["bucket"] != "copy-field"]
    t2 = [e for e in main if e["bucket"] == "copy-field"]
    P(f"  🔴 对外文案**条目**（对账的左边，要与表里条数**相等**）      : {len(main)}")
    P(f"     · 第 1 层 直接渲染面（toast / DOM / 属性 / Err …）        : {len(t1)}")
    P(f"     · 第 2 层 文案字段（装进结构体再由渲染层取出来贴）        : {len(t2)}")
    P(f"     对外文案**调用点**（一处调用可带多条条目）                : {len(sites)}")
    P(f"     条目分布的文件数                                          : {len(efiles)}")
    P(f"     带插值的条目（91 §5.1-3：插值点必须留在表里）             : "
      f"{sum(1 for e in main if e['params'])}  ({pct(sum(1 for e in main if e['params']), len(main))})")
    P(f"     去重后的**不同文本**条数                                  : {len({e['text'] for e in main})}")
    P(f"  ▫ 预备队（tracing::error!，今天不对外，00 §1.5.6 第 3 级后对外）: {len(reserve)}")

    # ── 面②：按面/按轨分桶 ──────────────────────────────────────────────
    P("\n【面②】按面分桶 ＋ 按轨分桶")
    bc = Counter(e["bucket"] for e in main)
    P("  面（bucket）                 条目      占比")
    for b, c in bc.most_common():
        P(f"    {b:<24} {c:>6}   {pct(c, len(main))}")
    P("  轨                           条目      占比")
    for t, c in Counter(e["track"] for e in main).most_common():
        P(f"    {t:<24} {c:>6}   {pct(c, len(main))}")
    P("  出口（sink，定义的 (B) 那张表逐条）")
    for s, c in Counter(e["sink"] for e in main).most_common():
        P(f"    {s:<24} {c:>6}")
    P("  条目最多的 12 个文件")
    for f, c in Counter(e["file"] for e in main).most_common(12):
        P(f"    {c:>5}  {f}")

    # ── 面③：kind ────────────────────────────────────────────────────
    P("\n【面③】kind 五档（`91 §4` 订正后）—— R5 能不能只扫 `title` 档，答案在这一格")
    kc = Counter(e["kind"] for e in main)
    for k, c in kc.most_common():
        P(f"    {k:<12} {c:>6}   {pct(c, len(main))}")
    ks = Counter(e["kind_src"] for e in main)
    P("  kind 是怎么定下来的：")
    for k, c in ks.most_common():
        note = {"sink": "出口本身就决定了（toast 标题 / Err / confirm …）",
                "tag": "回溯到 document.createElement(\"tag\")",
                "name": "只认出变量名（btn/title 后缀）—— **推定，不是测量**",
                "none": "推不出来"}.get(k, "")
        P(f"    {k:<12} {c:>6}   {note}")
    unres = kc.get("unresolved", 0)
    P(f"  🔴 **分不开的那部分**：{unres} 条（{pct(unres, len(main))}）住在 div/span 之类的中性件上，"
      f"既可能是区块名也可能是正文。")
    P(f"     ⇒ 逐字答 `91 §4` 那条 ⚠（「分不清 `title` 与 `control`，R5 只能 grep 猜」）：")
    P(f"       今天 `title` 档只认得出 {kc.get('title', 0)} 条，`control` 档 {kc.get('control', 0)} 条，"
      f"而 {unres} 条**分不开**。")
    P("       ⇒ **这就是「R5 的前置是抽表」那句话的量** —— 抽表之前 R5 的射程是猜出来的。")

    # ── 面④：R1 ─────────────────────────────────────────────────────
    P("\n【面④】R1 禁词表 —— 逐词现打命中（`91 §4` 那张表逐字）")
    r1 = word_hits(main, R1_WORDS, skip_hui_color=True)
    r1_all = {id(e) for lst in r1.values() for e in lst}
    P(f"  扫过的条目（分母）: {len(main)}")
    P(f"  {'禁词':<14}{'命中条目':>8}   住址（前 3 条）")
    for name, _ in R1_WORDS:
        lst = r1.get(name, [])
        addr = "  ".join(f"{e['file']}:{e['line']}" for e in lst[:3])
        flag = "🔴" if lst else "  "
        P(f"  {flag}{name:<12}{len(lst):>8}   {addr}")
    P(f"  ── R1 命中的**不同条目**合计: {len(r1_all)}  ({pct(len(r1_all), len(main))})")
    hui_all = [e for e in main if "灰" in e["text"]]
    P(f"  ⚠ `灰` 两种读法：含「灰」的条目共 {len(hui_all)} 条，剔掉**纯颜色义**（灰色/灰度/灰阶）后 "
      f"{len(r1.get('灰', []))} 条。")
    P("     「变灰」「置灰」**故意不剔** —— `设计/30 §3.5.2` 点的正是这个用法（到底是「已结束」还是「可重连」）。")
    for e in hui_all[:4]:
        P(f"        🔴 {e['file']}:{e['line']}  「{e['text'][:52]}」")
    P("  ▫ 预备队（tracing::error!）里的 R1 命中：", end="")
    r1r = word_hits(reserve, R1_WORDS, skip_hui_color=True)
    P(f"{len({id(e) for lst in r1r.values() for e in lst})} 条 / {len(reserve)}")

    P("\n  【面④附】R1 候选增补（**不在 §4 词表里**，列出来给规范作者拍板）")
    for name, rx in R1_CANDIDATES:
        lst = [e for e in main if rx.search(e["text"])]
        addr = "  ".join(f"{e['file']}:{e['line']}" for e in lst[:2])
        P(f"    {name:<14}{len(lst):>6}   {addr}")

    # ── 面⑤：R5 ─────────────────────────────────────────────────────
    P("\n【面⑤】R5（`91 §4` 订正后）—— **射程只到 `title` 档**")
    titles = [e for e in main if e["kind"] == "title"]
    titles_measured = [e for e in titles if e["kind_src"] in ("tag", "sink")]
    P(f"  `title` 档条目（页面名 / 导航项 / 区块标题）: {len(titles)}"
      f"（{len(titles_measured)} 条是测量出来的，{len(titles) - len(titles_measured)} 条按变量名推定）")

    def r5_scan(pool, tag, verbose=True):
        q = [e for e in pool if R5_QUESTION.search(speech(e["text"]))]
        col = word_hits(pool, R5_COLLOQUIAL)
        colset = {id(e) for lst in col.values() for e in lst}
        imp = [e for e in pool if R5_IMPERATIVE.search(e["text"])]
        both = {id(e) for e in q} | colset | {id(e) for e in imp}
        P(f"  ── {tag}（{len(pool)} 条）")
        P(f"     ① 问号 ？/?（已剥插值点）        : {len(q):>4}   {pct(len(q), len(pool))}")
        for e in q[:5]:
            P(f"        🔴 {e['file']}:{e['line']}  「{e['text'][:46]}」")
        nc = 0
        for name, _ in R5_COLLOQUIAL:
            for e in col.get(name, []):
                nc += 1
                if verbose and nc <= 8:
                    P(f"     ② 口语词「{name}」               : 🔴 {e['file']}:{e['line']}  「{e['text'][:44]}」")
        if nc == 0:
            P("     ② 口语词表                       :    0")
        P(f"     ③ 祈使动词开头                   : {len(imp):>4}   {pct(len(imp), len(pool))}")
        for e in imp[:6]:
            P(f"        🔴 {e['file']}:{e['line']}  「{e['text'][:46]}」")
        P(f"     R5 命中合计                      : {len(both):>4}   {pct(len(both), len(pool))}")
        return both

    r5_title = r5_scan(titles, "kind=title 🔴 **R5 只判这一档**")
    P("\n  ── 下面三档 R5 **不管**，列出来是为了证明「不判它们」是有量的决定，不是漏了")
    for k, why in (("control", "复选框/开关标签，「启用 X」是标准形"),
                   ("action", "按钮，本来就该是动词"),
                   ("body", "正文，问句合法（「要现在重试吗？」是真的在问）")):
        pool = [e for e in main if e["kind"] == k]
        q = [e for e in pool if R5_QUESTION.search(speech(e["text"]))]
        imp = [e for e in pool if R5_IMPERATIVE.search(e["text"])]
        P(f"     kind={k:<8}{len(pool):>5} 条 —— 问号 {len(q):>3} · 祈使开头 {len(imp):>3}   （{why}）")
        for e in imp[:3]:
            P(f"        ▫ {e['file']}:{e['line']}  「{e['text'][:46]}」 ← 第一版 R5 会误判红")
    unres_hit = [e for e in main if e["kind"] == "unresolved"
                 and (R5_QUESTION.search(speech(e["text"])) or R5_IMPERATIVE.search(e["text"]))]
    P(f"  🔴 **分不开的那部分**：kind=unresolved 里命中 R5 形状的有 {len(unres_hit)} 条 —— "
      f"今天判不了它们是 `title` 还是 `control`/`body`。")
    P("     ⇒ 这 %d 条逐字就是 `91 §4`「没有抽表，R5 根本落不了地」那句话的**量**。" % len(unres_hit))
    for e in unres_hit[:6]:
        P(f"        ? {e['file']}:{e['line']}  「{e['text'][:50]}」")

    # ── 面⑥：六类病比例 ────────────────────────────────────────────────
    P("\n【面⑥】六类病 —— **比例**（`91 §6` 那笔「比例完全没量过」的债还在这一格）")
    st = disease_stats(main)
    P(f"  分母 = 对外文案条目全集 {len(main)} 条（**不是抽样**）")
    P(f"  {'病':<44}{'条目':>6}   {'占比':>7}   判据强度")
    rows = [
        ("§2.1 内部标识符外泄（daemon/@ccm_sid/sid/relay…）", len(st["d1"]), "机检：R1 技术名子集"),
        ("§2.2 内部推理当报错（判据/守卫/尺子…）", len(st["d2"]), "机检：R1 行话子集"),
        ("§2.3 建议式口吻（把运维派给用户）", len(st["d3"]), "**代理·下界**（词面命中）"),
        ("§2.4 AI 味形状（长句/破折号/括号 ≥2 项同时）", len(st["d4_all"]), "**代理**（形状近似）"),
        ("§2.5 自造比喻当概念名（死亡账/棘轮/空真…）", len(st["d5"]), "机检：R1 比喻子集"),
        ("§2.6 口语腔（只算 `title` 档 R5 命中）", len(r5_title), "机检：R5，**分母是 title 档**"),
    ]
    for label, cnt, strength in rows:
        base = len(titles) if label.startswith("§2.6") else len(main)
        P(f"  {label:<44}{cnt:>6}   {pct(cnt, base):>7}   {strength}")
    P("  §2.4 的三个信号分开看：")
    P(f"     破折号 —— / —                         : {len(st['dash']):>5}   {pct(len(st['dash']), len(main))}")
    P(f"     括号补充（括号里 ≥2 字）              : {len(st['paren']):>5}   {pct(len(st['paren']), len(main))}")
    P(f"     首句 ≥{D4_LONG_CHARS} 个汉字                     : {len(st['longs']):>5}   {pct(len(st['longs']), len(main))}")
    d = st["dist"] or [0]
    qs = {q: d[min(int(len(d) * q / 100), len(d) - 1)] for q in (50, 75, 90, 95, 99)}
    P(f"     ▫ 首句汉字数分布（给 `91 §5.2⑤` 定数用）: p50={qs[50]} p75={qs[75]} "
      f"p90={qs[90]} p95={qs[95]} p99={qs[99]} max={d[-1]}")
    sick = st["d1"] | st["d2"] | st["d5"] | {id(e) for e in st["d3"]} | {id(e) for e in st["d4_all"]} | r5_title
    P(f"  🔴 **至少中一类的条目**                  : {len(sick):>5}   {pct(len(sick), len(main))}")
    P(f"     ⇒ 反过来：{pct(len(main) - len(sick), len(main))} 的条目**一类都没中** —— "
      f"与 `真相源/50 §1`「现有文案大部分不 AI」那条抽样结论同向，现在它有全集读数了。")
    P("  ⚠ 病③ 病④ 是**代理判据**：病③ 只认词面（「建议」两字），说得委婉的漏掉 ⇒ **读成下界**；")
    P("     病④ 的三个信号里「长句」阈值是本量具自己定的（`91 §5.2⑤` 今天还没定数）⇒ 阈值一改，数就变。")

    # ── 面⑦：残差 ────────────────────────────────────────────────────
    P("\n【面⑦】残差 —— 定义之外还剩多少含汉字的字面量（上界在这里）")
    P(f"  生产源码里含汉字的字面量总数（遮注释后）  : {len(residual) + len(entries)}")
    P(f"    · 被出口白名单收进来的（主集＋预备队）  : {len(entries)}")
    P(f"    · 残差（不在任何已登记出口里）          : {len(residual)}")
    declined = [r for r in residual if DECLINED_CTX.search(r["ctx"])]
    unsure = [r for r in residual if not DECLINED_CTX.search(r["ctx"])]
    P(f"    · 其中**明写排除**的出口（tracing:: / console. / panic! / expect( / assert! …）: {len(declined)}")
    P(f"    · 🔴 **存疑带**（既不在白名单、也不在明写排除里）                          : {len(unsure)}")
    P("  ⇒ 读法：**全集 = 主集 ＋ 存疑带里将来被裁进来的那一部分**。存疑带是这份普查**说不准**的量，")
    P("     抽表时必须逐条裁，不许当 0。它主要是两族：")
    P("       ① Rust 里 `let msg = format!(…)` 之后才 `Err(msg)` 的两步写法（本量具只认一步写法）；")
    P("       ② 内部登记表（如 `fenced_block.rs::FENCE_SHAPES`）里**长得像文案但不上界面**的字段。")
    P("  存疑带按**族**分（裁的时候按族裁，不逐条盲看）")
    fam = [
        ("Rust `format!(` 两步写法", re.compile(r"format!\(\s*$")),
        ("字符串拼接续行 `+`", re.compile(r"\+\s*$")),
        ("`const/let X = ` 常量", re.compile(r"(?:const|let|var)\s+[A-Za-z_$][\w$]*\s*(?::[^=]*)?=\s*$")),
        ("`return ` 直接返回", re.compile(r"\breturn\s*$")),
        ("`.push(` / `.push_str(`", re.compile(r"\.(?:push|push_str|join|append)\s*\(\s*$")),
        ("`Some(` / 枚举载荷", re.compile(r"(?:Some|Ok|Stranger|Deploy|Skip)\(\s*$")),
        ("未登记的**字段名**（`x: \"中文\"`）", re.compile(r"[A-Za-z_][\w]*\s*:\s*$")),
        ("未登记的**函数实参**", re.compile(r"[\w)\]]\s*\(\s*$|,\s*$")),
    ]
    left = list(unsure)
    for name, rx in fam:
        hit = [r for r in left if rx.search(r["ctx"])]
        left = [r for r in left if r not in hit]
        P(f"    {name:<34}{len(hit):>5}")
    P(f"    {'其余（形状零散）':<34}{len(left):>5}")
    P("  存疑带的前 18 种上下文（看『是不是漏登记了出口』）")
    for ctx, c in Counter(re.sub(r"[^\w.:!(]+$", "", r["ctx"])[-30:] for r in unsure).most_common(18):
        P(f"    {c:>5}  ...{ctx}")
    P("  存疑带里**最像对外文案**的 10 条（人裁用）")
    shown = 0
    for r in unsure:
        if len(CJK.findall(r["text"])) >= 10:
            P(f"    {r['file']}:{r['line']}  <<{r['text'][:60]}>>")
            shown += 1
            if shown >= 10:
                break
    P(f"  纯英文候选（**已登记缺口**，`91 §6` 承认过没看）: {len(english)}")
    for e in english[:8]:
        P(f"    {e['file']}:{e['line']}  「{e['text'][:50]}」")

    # ── 面⑧：抽表对账的左边 ──────────────────────────────────────────
    P("\n【面⑧】抽表对账（`91 §5.2` 逐字要的「相等断言，不是下界」）")
    P(f"  盘上对外文案条目数 = **{len(main)}**")
    P(f"  ⇒ 将来那条对拍判据写成： assert_eq!(表里条数, {len(main)})，")
    P("     并且这个数每次改文案都必须回来改 —— 地板会被静默绕过，相等不会。")
    P(f"  去重后不同文本 {len({e['text'] for e in main})} 条 ⇒ 重复文本 "
      f"{len(main) - len({e['text'] for e in main})} 条（同一句话今天有多个住址，抽表天然收掉）")
    dup = Counter(e["text"] for e in main)
    P("  重复最多的 6 句：")
    for t, c in dup.most_common(6):
        if c > 1:
            P(f"    ×{c}  「{t[:56]}」")

    # ── 面⑨：反空真地板 ──────────────────────────────────────────────
    P("\n【面⑨】反空真地板 —— 扫到 0 处是**失败**，不是静默绿")
    got = {
        "files_scanned": len(files),
        "entries_total": len(main),
        "sites_total": len(sites),
        "bucket_toast": bc.get("toast", 0),
        "bucket_dom_text": bc.get("dom-text", 0),
        "bucket_title": bc.get("title", 0),
        "bucket_confirm": bc.get("confirm", 0),
        "bucket_rust_err": bc.get("rust-err", 0),
        "r1_scanned": len(main),
        "kind_resolved": sum(1 for e in main if e["kind_src"] == "tag"),
        "kind_title": kc.get("title", 0),
        "bucket_copy_field": bc.get("copy-field", 0),
    }
    bad = []
    for k, floor in FLOORS.items():
        v = got[k]
        ok = v >= floor
        P(f"    {'✔' if ok else '✘'} {k:<18} = {v:>6}   地板 {floor}")
        if not ok:
            bad.append(k)

    if args.addresses:
        P("\n【附】逐处住址（主集，按文件排序）")
        for e in sorted(main, key=lambda x: (x["file"], x["line"])):
            P(f"  {e['file']}:{e['line']}\t{e['bucket']}\t{e['kind']}/{e['kind_src']}\t{e['text'][:90]}")

    if args.json:
        P(json.dumps(dict(entries=len(main), sites=len(sites), files=len(efiles),
                          buckets=dict(bc), kinds=dict(kc), floors_failed=bad,
                          r1=len(r1_all), r5_title=len(r5_title), unresolved=unres),
                     ensure_ascii=False))

    if bad:
        P(f"\n🔴 空转：{', '.join(bad)} 触地板 ⇒ 尺子没切到东西。退出码 3。")
        return 3
    P("\n✔ 每一格都切到了东西。退出码 0。")
    return 0


def selftest() -> int:
    """死值验：把语料换成空目录，本脚本**必须**报空转（退出码 3）。

    没有这一条，「扫到 0 处」会安安静静地绿 —— 那正是本仓治过的那一族。
    """
    global SRC_ROOT
    keep = SRC_ROOT
    print("═" * 78)
    print("反空真死值验：把 SRC_ROOT 换成空目录，本脚本必须报空转")
    print("═" * 78)
    with tempfile.TemporaryDirectory() as td:
        SRC_ROOT = Path(td)
        import io
        import contextlib
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            rc = main_report(argparse.Namespace(addresses=False, json=False))
    SRC_ROOT = keep
    print(f"  空语料下的退出码 = {rc}（期望 3）")
    if rc != 3:
        print("  ✘ 死值验**没死** —— 本脚本会把「扫到 0 处」报成绿。这条量具不可信。")
        return 1
    print("  ✔ 死值验死了：0 命中被报成失败，不是静默绿。")
    # 第二刀：把出口白名单清空，主集必须塌到 0 并触地板
    global SINKS
    keep_sinks = SINKS
    SINKS = []
    import io
    import contextlib
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        rc2 = main_report(argparse.Namespace(addresses=False, json=False))
    SINKS = keep_sinks
    print(f"  清空出口白名单后的退出码 = {rc2}（期望 3）")
    if rc2 != 3:
        print("  ✘ 出口白名单被清空还能绿 ⇒ 这张表没在决定读数。")
        return 1
    print("  ✔ 出口白名单确实在决定读数。")
    return 0


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description="条 68·A1 对外文案全集普查")
    ap.add_argument("--addresses", action="store_true", help="打印逐处住址")
    ap.add_argument("--json", action="store_true", help="末尾追加机读摘要")
    ap.add_argument("--selftest", action="store_true", help="反空真死值验")
    a = ap.parse_args()
    sys.exit(selftest() if a.selftest else main_report(a))
