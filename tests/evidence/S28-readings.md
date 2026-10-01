# S28 读数：步 8 改名的判据影响面普查（2026-09-19 现打）

量具：`tests/evidence/S28-rename-criteria-census.py`（头注里写死定义与射程边界）

⚠ 本件**只读**。一行 `src/**` 都没有动。

---

## 1. 复算

```bash
cd ~/文档/claudecode-frontend/cc-monitor
python3 tests/evidence/S28-rename-criteria-census.py             # 主报告
python3 tests/evidence/S28-rename-criteria-census.py --silent    # 静默嫌疑逐处
python3 tests/evidence/S28-rename-criteria-census.py --addresses # 全部 3 035 处
python3 tests/evidence/S28-rename-criteria-census.py --json
python3 tests/evidence/S28-rename-criteria-census.py --selftest  # 反空真死值验
```

退出码：`0` 每格都切到东西 · `3` **有格子空转（失败，不是 0 命中的绿）** · `4` 自检失败。

---

## 2. 主报告（2026-09-19 现打，原样贴）

```
人群：825 份文件（325 份有命中）· 扫过 88 238 个字符串字面量 · 抽到 3 035 处命中

── 面①：按名字 ──
  cc-monitor-remote      126   needle 19 · prose 65 · row 42
  daemon                1855   needle 82 · path 368 · prose 590 · row 815
  sidecar                 83   needle 2 · path 19 · prose 29 · row 33
  relay                  323   needle 26 · path 38 · prose 120 · row 139
  中转                     119   needle 5 · prose 40 · row 74
  proto                  462   needle 6 · path 185 · prose 94 · row 177
  rbind                   67   needle 5 · path 3 · prose 22 · row 37

── 面②：按四形 ──
  row       1317  登记表的数据行（跟着改字面量）
  needle     145  判据的针（改名会让它失配）
  path       613  住址（跟着文件一起改）
  prose      960  散文/报文（看语义）

── 面③：🔴 静默嫌疑 · 活判据档 ──
  neg         20  (已装地板   12)
  slice       12  (已装地板   10)
  gone        21  (已装地板   20)
  xlang      209  (已装地板  116)
  合计         362  (已装地板  169)

  另档 `dead`  1511  落在**不是活门**的 `tests/evidence/*.py` 里

── 面⑤：活门（现算，不手抄）—— 4 把 ──
  tests/evidence/K-R115-ruler.py   命中 1
  tests/evidence/K-R117-ruler.py   命中 18
  tests/evidence/K-R122-ruler.py   命中 3
  tests/evidence/K-R124-ruler.py   命中 0
  另有 136 份 `tests/evidence/*.py` 有命中但**不是活门**
```

⚠ **人群份数会漂**：本轮观测中别的路在同一棵树上加 `evidence/` 文件，
三趟分别读到 822 / 823 / 825 份。**其余各档与它无关。**

---

## 3. 反空真死值验（`--selftest`，两刀）

### 刀一 · 扫一棵**空树** ⇒ 必须当场红

```
── `--selftest`：扫一棵空树 ──
  命中 0 处 · 文件 0 份 · 触地板 11 条
    × 人群只扫到 0 份文件（地板 200）—— 遍历坏了
    × 只抽到 0 处命中（地板 300）—— 抽取器坏了
    × 七个名字里只有 0 个有命中（地板 5）：[]
    × 四形只切到 []（地板 4 形）—— 分类器坏了
    × 「可能静默」那一档只有 0 处（地板 10）—— 嫌疑判法坏了
    × 嫌疑标 `neg`   一处都没切到 —— 那条判法坏了
    × 嫌疑标 `slice` 一处都没切到 —— 那条判法坏了
    × 嫌疑标 `gone`  一处都没切到 —— 那条判法坏了
    × 嫌疑标 `xlang` 一处都没切到 —— 那条判法坏了
    × 嫌疑标 `dead`  一处都没切到 —— 那条判法坏了
    × 活门表只现算出 0 把尺子（地板 3）：[]
✅ SELFTEST OK：空树当场红（0 命中**不是**绿）
```

### 刀二 · **真树 ＋ 把名字表掏空**（换成一个绝不存在的名字）⇒ 必须当场红

```
── `--selftest` 第二刀：真树 + 空名字表 ⇒ 命中 2 处 · 触地板 8 条
✅ SELFTEST OK：名字表掏空 ⇒ 当场红
```

⚠ 第二刀那 **2 处**是量具**自己头注里那个哨兵词**被扫到（本文件与脚本同在人群里）。
这正是「判据在自己的常量里找到自己」那一族 —— 它不影响本刀的结论（地板仍然全触），
但**如实写下来**：真要把本量具做成门禁，得先给它装 `guard_core::scan_tree_excluding` 那一格。

---

## 4. 🔴 量具自己掉进去过一次 —— 墓碑

第一版在 `gone` 那一格里写了

```python
rel = cand.lstrip("./")          # ← 影子掉了外层的 rel（文件住址）
```

于是**同一份文件里这一处之后的每一条 `Hit` 都挂上了别人的住址**。
它**一个字都不红**（类型对、值也像路径），三条地板（人群份数 / 命中数 / 四形）全过。
是 `--addresses` 印出「`doc/IPC-PROTOCOL.md` 里有 Python 代码行」才看出来的 ——
而 `doc/IPC-PROTOCOL.md` **根本不在人群里**（真住址是 `src/doc/`）。

⇒ 与本仓 `needle_anchor_registry` 头注那句同形：**量具自己先掉进它要治的那一族。**
修法写在脚本里那处的注释上（变量改名 `pcand` ＋ 逐字墓碑）。

📌 **这一刀也改了读数**：修之前 `xlang 257 / gone 28`，修之后 `xlang 209 / gone 21`。
**别拿修之前那两个数。**

---

## 5. 本量具**买不到**什么（脚本头注 `§三` 的摘要）

- 不读语义 —— `prose` 那 960 处里哪些是引文/墓碑/外部契约名，要人判。
- 只看单行 ⇒ `needle` 那 145 处是**下界**。
- `xlang` 只认**逐字相等的整串**字面量 ⇒ 那 209 处也是**下界**。
- 不跑任何判据 ⇒ 「这一处改了会红」是推断，不是实测。
- 默认不数注释里的名字（`--prose-too` 才数）。
- `gone` 那一档 09-19 现打 **0 条真病**（21 处全是量具把运行时文件名/阴性对照假路径
  当成了仓内住址）。**别拿这一档当线索**，逐条判过的结论写在地图 `§2.5`。
