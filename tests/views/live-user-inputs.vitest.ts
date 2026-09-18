/**
 * K-R45 · `KR45D2`（乙 · 实时窗口）：**同一件事，在那条路上从零接。**
 *
 * # 这一格与甲的真正区别（不是「换个文件再写一遍」）
 *
 * 查看器有 `payloads`（收集阶段全量留着）⇒ 清单是**现算**的。
 * 实时窗口**一条已经上屏的记录，它的文本在前端零处留存**（读数与分母在件 `§5.2`：
 * `Tab` 31 个字段里持有整条 payload 的只有 `TailWindow`（取走即出账）与
 * `midBatchBuffer`（每批清空）；`RecordTimeline` 的条目没有 `message`、没有 `uuid`）。
 * ⇒ 本轮在 `tabs.ts` 里立了一个**今天不存在的账本**（`Tab.userInputs`）。
 * **那是新建，不是共用** —— 件 `§0d` 里「乙那一拍直接 import」那句已被读数推翻。
 *
 * 共用的是三样：挑句子的口径（`collectUserInputs`）· 界面（`UserInputPanel`）·
 * 找卡→展开→滚（`card-jump.ts::revealCard`）。**一个都没有第二份住址。**
 *
 * # 🔴 本文件里最贵的一格：`live-user-inputs.vitest.ts` 独有的那条**状态转移**
 *
 * PM 09-10 第二拍切刀逮到「跳成功时把标记删掉」零判据，并给了一条可达性理由
 * （「查看器分批渲染，T 没渲染、T+1 渲染出来」）。那条理由**对查看器不成立**
 * （`session-viewer-user-inputs.vitest.ts` 里「跳空是永久的」那一格是它的读数）——
 * 但**对实时窗口成立，而且是主路**：live 的门控就是「旧记录不建卡」
 * （`tabs.ts` 的 `tab.window.defer(payload)` 那一支），上翻补一批它就有卡了。
 * ⇒ 下面「上翻补批之后再点」那一格，是这条转移的**活体**。
 *
 * # 台子（🔴 渲染管线是**真的**，这是本文件与 `tabs.vitest.ts` 的关键差别）
 *
 * `tabs.vitest.ts` 把 `render-stream-record` / `cards` 整个 mock 掉了 ⇒ 那里的「卡」
 * 是一个空 `div`，**没有 `data-uuid`** ⇒ 「跳到那张卡上」这件事在那套台子上量不了。
 * 本文件只 mock 掉 IPC 与几个纯外部动作（重启 / 远端拉起 / 通知），
 * 渲染那一路**一个都不 mock**：真 `renderContentRecord` → 真 `markCardUuid` 写 `data-uuid`。
 * jsdom 缺的四样（`ResizeObserver` / `CSS` / rAF / `scrollIntoView`）与造行的工厂
 * 共用 `test-support/session-viewer-rig.ts` **那一份**，没有开第二份台子。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { readFileSync } from "node:fs";

// --- 只 mock「会真的去碰机器」的那几样；渲染管线保持真身 ---
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../../src/tasks-panel", () => ({ fetchSessionTasks: vi.fn().mockResolvedValue([]) }));
vi.mock("../../src/turn-notify", () => ({ turnEndNotifier: { observe: vi.fn() } }));
vi.mock("../../src/behavior", () => ({
  getBehavior: vi.fn().mockResolvedValue({ resumeCommandLocal: "", resumeCommandRemote: "cct" }),
}));
vi.mock("../../src/remote-launch-run", () => ({
  runRemoteResume: vi.fn().mockResolvedValue(undefined),
  runRemoteResumeTmux: vi.fn().mockResolvedValue(undefined),
  runLocalResumeIntoExistingTmux: vi.fn().mockResolvedValue(true),
  runRemoteResumeIntoExistingTmux: vi.fn().mockResolvedValue(true),
  runRemoteAttach: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../../src/account-restart", () => ({
  restartWithAccount: vi.fn().mockResolvedValue(undefined),
  DEFAULT_EXIT_WAIT_MS: 10_000,
}));
vi.mock("../../src/fork-flow", () => ({ runForkFlow: vi.fn().mockResolvedValue(undefined) }));

import {
  installViewerRig,
  assistantLine,
  userLine,
  withSession,
  type RigPayload,
  type ViewerRigHandles,
} from "../test-support/session-viewer-rig";
import { REPO_ROOT } from "../test-support/repo-root";
import { TabManager, type Tab } from "../../src/tabs";

let rig: ViewerRigHandles;
let tm: TabManager;
let streamRootEl: HTMLElement;

/**
 * `onLine` 要的是生成物类型 `JsonlLinePayload`，而台子的 `message` 刻意是 `unknown`
 * （判据要造畸形记录）。**强转只此一处**，别再散开写（先例：`tabs.vitest.ts` 的 `as never`）。
 */
const feed = (p: RigPayload): void => tm.onLine(p as never);

const peek = (sid: string): Tab =>
  (tm as unknown as { tabs: Map<string, Tab> }).tabs.get(sid)!;

const overlayOf = (sid = "s1"): HTMLElement =>
  peek(sid).inputsEl;
const rowsOf = (sid = "s1"): HTMLButtonElement[] => [
  ...overlayOf(sid).querySelectorAll<HTMLButtonElement>(".user-input-row"),
];
const toggleOf = (sid = "s1"): HTMLButtonElement =>
  overlayOf(sid).querySelector<HTMLButtonElement>(".user-inputs-toggle")!;
/**
 * 🔴 找卡**只在那个 tab 自己的流里找**。`[data-uuid]` 在本仓只有一个意思
 * （渲染出来的消息卡）；清单行用的是 `data-input-uuid`，两者不许混着数。
 */
const cardOf = (uuid: string, sid = "s1"): HTMLElement | null =>
  peek(sid).streamEl.querySelector<HTMLElement>(`[data-uuid="${uuid}"]`);

beforeEach(() => {
  rig = installViewerRig();
  const barEl = document.createElement("div");
  streamRootEl = document.createElement("div");
  document.body.append(barEl, streamRootEl);
  tm = new TabManager(barEl, streamRootEl);
});

afterEach(() => vi.unstubAllGlobals());

describe("KR45D2 账本：实时窗口第一次有「我说过的每一句」", () => {
  it("口径与查看器同一个住址：assistant / isMeta / sidechain / tool_result / 无 uuid 全排掉", () => {
    feed(userLine(1, "u1", "第一句"));
    feed(assistantLine(2, "a1", "回复"));
    feed(userLine(3, "u3", "skill 展开的 prompt", { isMeta: true }));
    feed(userLine(4, "u4", "子 agent 里说的", { isSidechain: true }));
    feed(userLine(5, "u5", [{ type: "tool_result", tool_use_id: "t1", content: "结果" }]));
    feed(userLine(6, null, "没有 uuid"));
    feed(userLine(7, "u7", "第二句"));

    expect(rowsOf().map((r) => r.dataset.inputUuid)).toEqual(["u1", "u7"]);
    expect(rowsOf()[0].textContent).toBe("1. 第一句"); // 分母 = 7 条记录里的 2 条主线用户输入
    expect(toggleOf().textContent).toBe("大纲 · 2");
  });

  it("一条用户输入都没有 ⇒ 开关禁用（不给一个点了没反应的入口）", () => {
    feed(assistantLine(1, "a1", "只有回复"));
    expect(rowsOf().length).toBe(0);
    expect(toggleOf().disabled).toBe(true);
    expect(toggleOf().textContent).toBe("大纲"); // 0 条不挂计数
  });

  it("同一条记录重投（SSH 重连 / 截断重读）不许在清单里记两遍", () => {
    feed(userLine(1, "u1", "第一句"));
    feed({ ...userLine(1, "u1", "第一句"), seq: 99 }); // 换新 seq 重投 ⇒ 只有 uuid 去重拦得住
    expect(rowsOf().map((r) => r.dataset.inputUuid)).toEqual(["u1"]);
  });
});

describe("KR45D2 分水岭：账本是全量的，DOM 只有渲染窗口里的那些", () => {
  /**
   * 造收纳（不建卡）的办法就是 live 真实的到达序：**启动重放末块先发**。
   * 先来 seq=100 ⇒ 钉 floor=100 直渲；再来 seq=1 ⇒ `admit(1)` 假 ⇒ `defer` 收纳。
   */
  const tailFirst = (): void => {
    feed(userLine(100, "u100", "最新那一句"));
    feed(userLine(1, "u1", "很久以前那一句"));
  };

  it("收纳的那一条：清单里有它，流里没有它的卡", () => {
    tailFirst();
    expect(rowsOf().map((r) => r.dataset.inputUuid)).toEqual(["u100", "u1"]);
    expect(peek("s1").window.pendingCount).toBe(1); // 先证明它真的被收纳了，不是没到
    expect(cardOf("u100")).not.toBeNull();
    expect(cardOf("u1")).toBeNull();
  });

  it("点已经建了卡的那一条 ⇒ 滚给了那张卡（跳转走的是与查看器同一份 revealCard）", () => {
    tailFirst();
    const target = cardOf("u100")!;
    rowsOf()[0].click();
    expect(rig.scrollIntoView).toHaveBeenCalledTimes(1);
    expect(rig.scrollIntoView.mock.instances[0]).toBe(target); // 点名滚给了谁
    expect(target.classList.contains("search-hit-flash")).toBe(true);
    expect(rowsOf()[0].dataset.unjumpable).toBeUndefined();
  });

  it("点还没建卡的那一条 ⇒ 不许静默：标出来，并说清楚是「还没加载出来」", () => {
    tailFirst();
    rowsOf()[1].click();
    expect(rowsOf()[1].dataset.unjumpable).toBe("1");
    expect(rowsOf()[1].title).toContain("还没加载出来");
    // 🔴 实时这一侧落空**不许顺手滚一下**：这条流本来就贴在底部，
    //    再滚一次就是「点了一下，好像动了一下，其实什么都没发生」。
    expect(rig.scrollIntoView).not.toHaveBeenCalled();
  });

  // ★★★ 本文件最贵的一格：`不可跳 → 可跳` 这条转移的**活体**。
  //     PM 那一刀（把 `delete row.dataset.unjumpable;` 整句删掉）在这一格上当场红。
  it("🔴 上翻补批把它渲出来之后再点 ⇒ 跳得过去，标记与提示都跟着撤掉", () => {
    tailFirst();
    const row = rowsOf()[1];
    row.click();
    expect(row.dataset.unjumpable, "先得真的标上，不然下面那半是空真").toBe("1");

    // 上翻补批 —— 走的是真的那条路（scroll 事件 → fillHandler → fillAbove → takeTail）
    peek("s1").streamEl.dispatchEvent(new Event("scroll"));

    expect(peek("s1").window.pendingCount, "补批没真的跑 ⇒ 下面那半是空真").toBe(0);
    expect(cardOf("u1"), "补批跑了但没建出卡 ⇒ 夹具选错了记录").not.toBeNull();

    rowsOf()[1].click();

    expect(rowsOf()[1], "清单被整表重建过 ⇒ 这一格量的不是同一行").toBe(row);
    expect(row.dataset.unjumpable, "跳得过去了还灰着 ⇒ 一行只要跳空过一次就永远灰着").toBeUndefined();
    expect(row.title, "跳得过去了还挂着「跳不过去」那句 ⇒ 同一形的第二处").toBe("很久以前那一句");
    expect(rig.scrollIntoView.mock.instances[0]).toBe(cardOf("u1"));
  });
});

describe("KR45D2 清单跟着 tab 走", () => {
  it("每个 tab 一份账本，互不串（可见的只有 active 那一份）", () => {
    feed(userLine(1, "u1", "一号会话说的"));
    // 首个 tab 自动 active（`ensureTab` 的 activeId===null 那一支）
    expect(overlayOf("s1").classList.contains("active")).toBe(true);

    // ⚠ 喂 s2 的这一条**会把 active 带走** —— 真用户输入触发 auto-follow
    // （`onRealUserInput` → `userActive`，issue #2 的自动切 tab）。
    //   这不是台子的怪癖，是产品行为；把它写出来，免得下一个人读成「切 tab 坏了」。
    feed(withSession(userLine(2, "v1", "二号会话说的"), "s2"));

    expect(rowsOf("s1").map((r) => r.dataset.inputUuid)).toEqual(["u1"]);
    expect(rowsOf("s2").map((r) => r.dataset.inputUuid)).toEqual(["v1"]);
    expect(overlayOf("s1").classList.contains("active")).toBe(false);
    expect(overlayOf("s2").classList.contains("active")).toBe(true);

    tm.switchTo("s1");

    expect(overlayOf("s1").classList.contains("active")).toBe(true);
    expect(overlayOf("s2").classList.contains("active")).toBe(false);
  });

  it("关掉 tab ⇒ 账本清空、悬浮层从 DOM 上摘掉（不许留着上一个会话的句子）", () => {
    feed(userLine(1, "u1", "第一句"));
    const overlay = overlayOf("s1");
    expect(streamRootEl.contains(overlay)).toBe(true);

    tm.archiveTab("s1"); // 只有 archived 的 tab 关得掉
    tm.closeTab("s1");

    expect(streamRootEl.querySelectorAll(".live-user-inputs").length).toBe(0);
    expect(overlay.querySelectorAll(".user-input-row").length).toBe(0);
  });
});

/**
 * 与上面那几格**是一对，各买各的**：上面量的是「JS 这边把 `.active` 翻对了没有」，
 * 这一格量的是「CSS 那边真有宿主」—— jsdom **不加载** `styles.css`，
 * 把 `.live-user-inputs` 那几条规则整段删掉，上面**一格都不会红**，
 * 而真实后果是每个 tab 的清单**一起挂在屏幕上**（`.active` 翻得再对也没用）。
 * 同形先例：`session-viewer-user-inputs.vitest.ts` 里三条共用规则那一格。
 */
describe("KR45D2 实时那块悬浮层在 styles.css 里真有宿主", () => {
  it("两条规则都在，而且靠 visibility 收起（不是 display —— 切 tab 要 0 reflow）", () => {
    const cssLines = readFileSync(`${REPO_ROOT}/src/styles.css`, "utf8")
      .split("\n")
      .map((l) => l.trim());
    // 抽取器自检：先确认这把尺子够得着那个文件（不然下面几条是空真）。
    expect(cssLines.length, "读到的 styles.css 只有几行 —— 尺子坏了").toBeGreaterThan(1000);
    expect(cssLines, "读到的不是 styles.css —— 连基准那条规则都没有").toContain(".stream {");

    expect(cssLines, "悬浮层没有 CSS 宿主 ⇒ 每个 tab 的清单一起挂在屏幕上").toContain(
      ".live-user-inputs {",
    );
    expect(cssLines, "没有 .active 那条 ⇒ 切过去的那个 tab 的清单也显示不出来").toContain(
      ".live-user-inputs.active {",
    );

    const open = cssLines.indexOf(".live-user-inputs {");
    const body = cssLines.slice(open + 1, cssLines.indexOf("}", open));
    expect(body, "默认不 hidden ⇒ 非 active 的 tab 的清单照样挂在屏幕上").toContain(
      "visibility: hidden;",
    );
    // 🔴 与 `.stream` 同一条纪律：切 active 不许走 display（整棵子树重建 layout tree）。
    expect(
      body.filter((l) => /^display\s*:/.test(l)),
      "`.live-user-inputs` 里出现了 display ⇒ 切 tab 从 0 reflow 退回整棵子树重建",
    ).toEqual([]);
  });
});
