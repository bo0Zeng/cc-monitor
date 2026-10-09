/**
 * 「运行中」的呼吸点：窗口不在前台（失焦 / 看不见）时不呼吸 —— 点照旧是绿的，只是不闪；切回来接着呼吸。
 * 呼吸动画每一帧都要合成 / 重画（WebKitGTK 上整页重画），窗口开着不看也一直在耗 CPU。
 * 停 / 续只动那几个动画本身（不往根元素上写标记：WebKit 上根上一改整页重算样式，切回窗口那一下要多挨一两百毫秒）。
 *
 * | 性质 | 判据 |
 * |---|---|
 * | 失焦 / 看不见 ⇒ 运行中的点的动画停在起点（不透明，颜色不变）；回到前台 ⇒ 接着放 | 「停与续」 |
 * | 别的动画不碰；页里元素之间换焦点不算离开前台 | 「停与续」 |
 * | 不在前台时新冒出来的呼吸（点刚变成运行中 / 刚挂进页面）⇒ 一冒出来就停 | 「停与续」 |
 * | 装上那一刻按当时的焦点 / 可见性办；不往根元素上写东西 | 「停与续」 |
 * | 三个窗口共用的入口装它 | 「装在哪」 |
 */
import { describe, it, expect, vi, afterEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { installDotRest, statusDot } from "../../../../src/frontend/ui/kit/status-dot";
import { REPO_ROOT } from "../../../test-support/repo-root";

let visibility: DocumentVisibilityState = "visible";
let focused = true;

/** 假的一段动画：只记它被停 / 放成什么样。 */
class FakeAnim {
  paused = false;
  currentTime: number | null = 700;
  constructor(readonly effect: { target: Element }) {}
  pause(): void {
    this.paused = true;
  }
  play(): void {
    this.paused = false;
  }
}

function stubPage(anims: FakeAnim[]): void {
  vi.spyOn(document, "hasFocus").mockImplementation(() => focused);
  vi.spyOn(document, "visibilityState", "get").mockImplementation(() => visibility);
  (document as unknown as { getAnimations: () => FakeAnim[] }).getAnimations = () => anims;
  (Element.prototype as unknown as { getAnimations: () => FakeAnim[] }).getAnimations = function (this: Element) {
    return anims.filter((a) => a.effect.target === this);
  };
}

afterEach(() => {
  vi.restoreAllMocks();
  delete (document as unknown as { getAnimations?: unknown }).getAnimations;
  delete (Element.prototype as unknown as { getAnimations?: unknown }).getAnimations;
  document.body.replaceChildren();
  visibility = "visible";
  focused = true;
});

function rig(): { dot: HTMLElement; other: HTMLElement; breath: FakeAnim; spin: FakeAnim; anims: FakeAnim[] } {
  const dot = statusDot("running", "运行中");
  const other = document.createElement("span");
  document.body.append(dot, other);
  const breath = new FakeAnim({ target: dot });
  const spin = new FakeAnim({ target: other });
  const anims = [breath, spin];
  stubPage(anims);
  return { dot, other, breath, spin, anims };
}

describe("停与续", () => {
  it("失焦 ⇒ 运行中的点停在起点；回来 ⇒ 接着放；看不见 ⇒ 停；又看得见且有焦点 ⇒ 放；别的动画不碰", () => {
    const { breath, spin } = rig();
    const stop = installDotRest();
    expect(breath.paused).toBe(false);
    focused = false;
    window.dispatchEvent(new FocusEvent("blur"));
    expect([breath.paused, breath.currentTime]).toEqual([true, 0]);
    expect(spin.paused, "别的动画不碰").toBe(false);
    focused = true;
    window.dispatchEvent(new FocusEvent("focus"));
    expect(breath.paused).toBe(false);
    visibility = "hidden";
    document.dispatchEvent(new Event("visibilitychange"));
    expect(breath.paused).toBe(true);
    visibility = "visible";
    document.dispatchEvent(new Event("visibilitychange"));
    expect(breath.paused).toBe(false);
    stop();
  });

  it("页里元素之间换焦点不算离开前台", () => {
    const { breath } = rig();
    const stop = installDotRest();
    const a = document.createElement("input");
    const b = document.createElement("input");
    document.body.append(a, b);
    a.focus();
    b.focus();
    expect(breath.paused).toBe(false);
    stop();
  });

  it("不在前台时新冒出来的呼吸（点刚变成运行中 / 刚挂进页面）⇒ 一冒出来就停；别的元素上冒出来的动画不碰", () => {
    const { anims } = rig();
    const stop = installDotRest();
    focused = false;
    window.dispatchEvent(new FocusEvent("blur"));
    const fresh = statusDot("running", "运行中");
    const plain = document.createElement("div");
    document.body.append(fresh, plain);
    const a = new FakeAnim({ target: fresh });
    const b = new FakeAnim({ target: plain });
    anims.push(a, b);
    fresh.dispatchEvent(new Event("animationstart", { bubbles: true }));
    plain.dispatchEvent(new Event("animationstart", { bubbles: true }));
    expect([a.paused, a.currentTime]).toEqual([true, 0]);
    expect(b.paused).toBe(false);
    focused = true;
    window.dispatchEvent(new FocusEvent("focus"));
    expect(a.paused).toBe(false);
    stop();
  });

  it("装上那一刻按当时的状态办：窗口没焦点 ⇒ 先停着；不往根元素上写东西", () => {
    focused = false;
    const { breath } = rig();
    const before = document.documentElement.getAttributeNames().join(",");
    const stop = installDotRest();
    expect(breath.paused).toBe(true);
    expect(document.documentElement.getAttributeNames().join(",")).toBe(before);
    stop();
  });
});

describe("装在哪", () => {
  it("三个窗口共用的入口装它", () => {
    const src = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/entry-common.ts"), "utf8");
    expect(src).toMatch(/^installDotRest\(\);$/m);
  });
});
