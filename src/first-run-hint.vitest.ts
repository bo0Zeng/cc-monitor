/**
 * N-F3 的判据：**第一次打开说得出下一步**。
 *
 * 被测的不是「能不能画一个 chip」，而是三件本仓真栽过的事：
 *
 * 1. **那张清单今天只住在设置面板深处** —— 主窗口得有一条指路，且它说的数
 *    **必须来自 `summarizeGaps`**，不许在主窗口另开一个家写死一个数（`NF3D2`）。
 * 2. 🔴 **「还差什么」（状态维）与「别再烦我」（意愿维）是两件事，不许压进一个值**
 *    （`NF3D3`）。盘上那个 `cmdkHintSeen`（见过即不再）**刻意没照抄** ——
 *    它对「教一个快捷键」是对的，对「你还差三项没配」是错的。
 *    ⇒ 下面**分开断**：切掉其中一个值 ⇒ 只红对应那一条，另一条仍绿。
 * 3. 没缺口时**一个节点都不新增**（`NF3D4`）——「渲染成空」不算，断的是**节点不存在**。
 *
 * ⚠ **本文件的射程**（`§4` 诚实边界同款）：这里证的是「代码走得到、判据断得住」，
 * **不是**「一个真人打开看见了」。真机新用户读数归 `ROADMAP` 的 `n1`，本件不解锁它。
 */
import { describe, it, expect, beforeEach, vi } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { FirstRunHint, FIRST_RUN_HINT_CLASS } from "./first-run-hint";
import { computeGaps, summarizeGaps } from "./settings/readiness";
import { LOCAL_MACHINE_KEY, type MachineStatus } from "./settings/machine-status";

const T = 1_700_000_000_000;
const ORIGINS = [LOCAL_MACHINE_KEY, "aya"];

/** 一台什么都没测过的机器 —— 新用户装完就是这个形。 */
const NOTHING: MachineStatus = {};
/** 五格全绿。`computeGaps` 见 `ok` 就跳过 ⇒ 这一形不该产出任何缺口。 */
const ALL_OK: MachineStatus = {
  connection: { kind: "ok", at: T },
  daemon: { kind: "ok", at: T },
  ccm: { kind: "ok", at: T },
  acctIso: { kind: "ok", at: T },
  accounts: { kind: "ok", at: T },
};

/** 可切换的账本 —— 「补齐了 ⇒ 消失 / 又缺了 ⇒ 再出现」靠切它，不靠切代码。 */
function ledger() {
  let cur: MachineStatus = NOTHING;
  return {
    statusOf: (): MachineStatus => cur,
    fillEverything: (): void => {
      cur = ALL_OK;
    },
    breakEverything: (): void => {
      cur = NOTHING;
    },
  };
}

interface Rig {
  host: HTMLElement;
  hint: FirstRunHint;
  openList: ReturnType<typeof vi.fn>;
  book: ReturnType<typeof ledger>;
  /** 主窗口里那条指路的节点；**不存在时是 `null`**（`NF3D4` 断的就是这个 null）。 */
  node: () => HTMLElement | null;
  text: () => string;
}

function rig(): Rig {
  const host = document.createElement("div");
  document.body.appendChild(host);
  const book = ledger();
  const openList = vi.fn();
  const hint = new FirstRunHint(host, {
    origins: () => ORIGINS,
    statusOf: book.statusOf,
    hostOs: () => "linux",
    openList,
  });
  return {
    host,
    hint,
    openList,
    book,
    node: () => host.querySelector<HTMLElement>(`.${FIRST_RUN_HINT_CLASS}`),
    text: () => host.textContent ?? "",
  };
}

/** 与被测对象**同一份入参**现算的期望摘要 —— 判据不自己造第二套数。 */
function expectedSummary(statusOf: () => MachineStatus): string | null {
  return summarizeGaps(
    computeGaps({ origins: ORIGINS, statusOf, hostOs: "linux" }),
  );
}

beforeEach(() => {
  document.body.replaceChildren();
  localStorage.clear();
});

describe("NF3D2 · 第一次打开说得出下一步", () => {
  it("★ 「还差什么」非空 ⇒ 主窗口出现一条指路（改前这里零命中，见 §3a ③）", () => {
    const r = rig();
    expect(expectedSummary(r.book.statusOf)).not.toBeNull(); // 前提：这一形真有缺口
    expect(r.node()).toBeNull(); // refresh 之前什么都没有
    r.hint.refresh();
    expect(r.node()).not.toBeNull();
  });

  it("★ 它是**非模态**的：挂在给它的容器里，不是 body 级弹层，不声明 modal 语义", () => {
    const r = rig();
    r.hint.refresh();
    const n = r.node()!;
    // 非模态的三条可断项：① 在宿主容器内（不是挂到 body 抢焦点）
    expect(n.parentElement).toBe(r.host);
    // ② 不自称对话框
    expect(n.getAttribute("role")).toBeNull();
    expect(n.getAttribute("aria-modal")).toBeNull();
    // ③ body 上没有被加任何遮罩/锁滚动的标记
    expect(document.body.className).toBe("");
  });

  it("🔴 ★ 它说的数**逐字等于 `summarizeGaps`** —— 主窗口不另写一套措辞", () => {
    const r = rig();
    r.hint.refresh();
    const summary = expectedSummary(r.book.statusOf)!;
    // 断「文本里含这个现算出来的串」，而不是断某个写死的数字。
    expect(r.text()).toContain(summary);
    // 而且那个串真的带着数（否则上一行可能在断一句恒真的空话）。
    expect(summary).toMatch(/\d/);
  });

  it("🔴 ★ 换一份账本 ⇒ 它说的数**跟着变**（写死一个数就在这里红）", () => {
    const r = rig();
    r.hint.refresh();
    const before = r.text();
    const beforeSummary = expectedSummary(r.book.statusOf)!;

    // 只补掉一台机器上的一格 ⇒ 缺口数必然变，而摘要串也必然变。
    const partial: MachineStatus = { accounts: { kind: "ok", at: T } };
    const hint2Host = document.createElement("div");
    document.body.appendChild(hint2Host);
    const hint2 = new FirstRunHint(hint2Host, {
      origins: () => ORIGINS,
      statusOf: () => partial,
      hostOs: () => "linux",
      openList: vi.fn(),
    });
    hint2.refresh();
    const afterSummary = expectedSummary(() => partial)!;

    expect(afterSummary).not.toBe(beforeSummary); // 前提：这两形的摘要真不同
    expect(hint2Host.textContent).toContain(afterSummary);
    expect(hint2Host.textContent).not.toBe(before);
  });

  it("★ 点它 ⇒ 打开那张清单住的地方（`openList` 恰好一次）", () => {
    const r = rig();
    r.hint.refresh();
    const open = r.host.querySelector<HTMLElement>(
      `.${FIRST_RUN_HINT_CLASS}-open`,
    );
    expect(open).not.toBeNull();
    open!.click();
    expect(r.openList).toHaveBeenCalledTimes(1);
  });

  it("★ 主窗口真的挂了它 —— `main.ts` 里有 import + 构造 + refresh", () => {
    // 这是一条**存在性断言**（判准见 `scanning-guard-registry.vitest.ts` 那张表）：
    // 它买到的是「主窗口那一端没被整段摘掉」（`§3` 的 `F3M1` 从任一端切都红），
    // **买不到**「它在真机上真的画出来了」——那归 `n1`。
    const here = dirname(fileURLToPath(import.meta.url));
    const main = readFileSync(join(here, "main.ts"), "utf8");
    expect(main).toContain("FirstRunHint");
    expect(main).toContain("new FirstRunHint(");
    expect(main).toMatch(/firstRunHint\.refresh\(\)/);
  });
});

describe("NF3D3 · 🔴 「还差什么」与「别再烦我」是两件事，不许压进一个值", () => {
  it("★ ①状态维 a：清单补齐了（`summarizeGaps` 返回 null）⇒ 那条指路**消失**", () => {
    const r = rig();
    r.hint.refresh();
    expect(r.node()).not.toBeNull();

    r.book.fillEverything();
    expect(expectedSummary(r.book.statusOf)).toBeNull(); // 前提：这一形真的清空了
    r.hint.refresh();
    expect(r.node()).toBeNull();
    // 意愿维一个字都没动 —— 消失的原因只能是状态维。
    expect(r.hint.isDismissedThisRun()).toBe(false);
  });

  it("★ ①状态维 b：又缺了 ⇒ **再出现**（一次性广告在这里红）", () => {
    const r = rig();
    r.hint.refresh();
    r.book.fillEverything();
    r.hint.refresh();
    expect(r.node()).toBeNull();

    r.book.breakEverything();
    r.hint.refresh();
    expect(r.node()).not.toBeNull();
  });

  it("★ ②意愿维：用户主动关掉这一次 ⇒ 这一次不再打扰（而清单**仍然非空**）", () => {
    const r = rig();
    r.hint.refresh();
    const close = r.host.querySelector<HTMLElement>(
      `.${FIRST_RUN_HINT_CLASS}-dismiss`,
    );
    expect(close).not.toBeNull();
    close!.click();
    expect(r.node()).toBeNull();
    // 再 refresh 多少次都不该回来 —— 这一次的意愿是这一次的。
    r.hint.refresh();
    r.hint.refresh();
    expect(r.node()).toBeNull();
  });

  it("🔴 ★ 两个值是**独立**的：关掉之后状态维的值一个字没变", () => {
    const r = rig();
    r.hint.refresh();
    const before = r.hint.gapSummary();
    r.hint.dismissThisRun();
    // 意愿维改的只是「说不说」，不是「还差什么」。压成一个值的实现在这里红。
    expect(r.hint.gapSummary()).toBe(before);
    expect(r.hint.gapSummary()).not.toBeNull();
    expect(r.hint.isDismissedThisRun()).toBe(true);
  });

  it("🔴 ★ 意愿维**不落盘**：关掉不写 localStorage，下次启动照说", () => {
    const r = rig();
    r.hint.refresh();
    const keysBefore = [...Array(localStorage.length).keys()]
      .map((i) => localStorage.key(i))
      .sort();
    r.hint.dismissThisRun();
    const keysAfter = [...Array(localStorage.length).keys()]
      .map((i) => localStorage.key(i))
      .sort();
    // 这一刀正对着 `cmdkHintSeen` 那个形状：它一落盘就退化成「见过即不再」。
    expect(keysAfter).toEqual(keysBefore);

    // 「下次启动」= 新建一个实例（意愿维是进程级的），账本没变 ⇒ 它该回来。
    const host2 = document.createElement("div");
    document.body.appendChild(host2);
    const next = new FirstRunHint(host2, {
      origins: () => ORIGINS,
      statusOf: r.book.statusOf,
      hostOs: () => "linux",
      openList: vi.fn(),
    });
    next.refresh();
    expect(host2.querySelector(`.${FIRST_RUN_HINT_CLASS}`)).not.toBeNull();
  });
});

describe("NF3D4 · 没缺口时一个字都不多说", () => {
  it("★ `summarizeGaps` 返回 null ⇒ 宿主容器**零新增节点**（断节点不存在，不是文本为空）", () => {
    const r = rig();
    r.book.fillEverything();
    expect(expectedSummary(r.book.statusOf)).toBeNull();
    const before = r.host.childElementCount;
    r.hint.refresh();
    expect(r.host.childElementCount).toBe(before); // 零新增
    expect(r.host.childElementCount).toBe(0);
    expect(r.node()).toBeNull(); // 是「不存在」，不是「存在但空」
    expect(r.text()).toBe("");
  });

  it("★ 从有到无：整块**被移除**，不是留一个藏起来的空壳", () => {
    const r = rig();
    r.hint.refresh();
    expect(r.host.childElementCount).toBe(1);

    r.book.fillEverything();
    r.hint.refresh();
    // 断「树里没有这个节点」——`display:none` 的实现在这里红。
    expect(r.host.childElementCount).toBe(0);
    expect(r.host.querySelectorAll("*").length).toBe(0);
  });
});
