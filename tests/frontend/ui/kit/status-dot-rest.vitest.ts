/**
 * 「运行中」的呼吸点：窗口不在前台（失焦 / 看不见）时不呼吸 —— 点照旧是绿的，只是不闪；切回来接着呼吸。
 * 呼吸动画每一帧都要合成 / 重画（WebKitGTK 上整页重画），窗口开着不看也一直在耗 CPU。
 *
 * | 性质 | 判据 |
 * |---|---|
 * | 失焦 / 看不见 ⇒ 根元素标「不在前台」；回到前台（有焦点且看得见）⇒ 摘掉 | 「标记」 |
 * | 装上那一刻就按当时的焦点 / 可见性标 | 「标记」 |
 * | 标着「不在前台」时运行中的点不带动画（颜色不变） | 「样式」 |
 * | 三个窗口共用的入口装它 | 「装在哪」 |
 */
import { describe, it, expect, vi, afterEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { installDotRest, WINDOW_AWAY_ATTR } from "../../../../src/frontend/ui/kit/status-dot";
import { REPO_ROOT } from "../../../test-support/repo-root";

const root = (): HTMLElement => document.documentElement;
let visibility: DocumentVisibilityState = "visible";
let focused = true;

function stubWindowState(): void {
  vi.spyOn(document, "hasFocus").mockImplementation(() => focused);
  vi.spyOn(document, "visibilityState", "get").mockImplementation(() => visibility);
}

afterEach(() => {
  vi.restoreAllMocks();
  root().removeAttribute(WINDOW_AWAY_ATTR);
  visibility = "visible";
  focused = true;
});

describe("标记", () => {
  it("失焦 ⇒ 标「不在前台」；回来 ⇒ 摘掉；看不见 ⇒ 标；又看得见且有焦点 ⇒ 摘掉", () => {
    stubWindowState();
    const stop = installDotRest();
    expect(root().hasAttribute(WINDOW_AWAY_ATTR)).toBe(false);
    focused = false;
    window.dispatchEvent(new FocusEvent("blur"));
    expect(root().hasAttribute(WINDOW_AWAY_ATTR)).toBe(true);
    focused = true;
    window.dispatchEvent(new FocusEvent("focus"));
    expect(root().hasAttribute(WINDOW_AWAY_ATTR)).toBe(false);
    visibility = "hidden";
    document.dispatchEvent(new Event("visibilitychange"));
    expect(root().hasAttribute(WINDOW_AWAY_ATTR)).toBe(true);
    visibility = "visible";
    document.dispatchEvent(new Event("visibilitychange"));
    expect(root().hasAttribute(WINDOW_AWAY_ATTR)).toBe(false);
    stop();
  });

  it("页里换焦点（元素之间）不算离开前台", () => {
    stubWindowState();
    const stop = installDotRest();
    const a = document.createElement("input");
    const b = document.createElement("input");
    document.body.append(a, b);
    a.focus();
    b.focus();
    expect(root().hasAttribute(WINDOW_AWAY_ATTR)).toBe(false);
    a.remove();
    b.remove();
    stop();
  });

  it("装上那一刻就按当时的状态标：窗口没焦点 ⇒ 先标着", () => {
    focused = false;
    stubWindowState();
    const stop = installDotRest();
    expect(root().hasAttribute(WINDOW_AWAY_ATTR)).toBe(true);
    stop();
  });
});

describe("样式", () => {
  it("标着「不在前台」时运行中的点不带动画，颜色那一条不动", () => {
    const css = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/kit/status-dot.module.css"), "utf8").replace(/\s+/g, " ");
    expect(css).toContain(`:root[${WINDOW_AWAY_ATTR}] .dot[data-state="running"] { animation: none; }`);
    expect(css).toContain(`.dot[data-state="running"] { background: var(--success); animation: breathe var(--dur-breathe) ease-in-out infinite; }`);
  });
});

describe("装在哪", () => {
  it("三个窗口共用的入口装它", () => {
    const src = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/entry-common.ts"), "utf8");
    expect(src).toMatch(/^installDotRest\(\);$/m);
  });
});
