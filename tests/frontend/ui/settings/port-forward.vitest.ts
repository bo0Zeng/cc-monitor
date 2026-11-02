/**
 * 端口转发面板「起一条」：面板不判端口 · 主机 · 有没有选机器，敲进去的原文原样交 `forward-start`；
 * 本机后端拒了 ⇒ 弹出的那一条就是后端写好的那一句（`beForwards.spec.*`），面板一个字不补。期望手写。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const { started, toasts } = vi.hoisted(() => ({
  started: [] as unknown[],
  toasts: [] as Array<{ title: string; e: unknown }>,
}));

vi.mock("../../../../src/frontend/ui/port-forward-reads", () => ({
  listForwards: vi.fn(async () => []),
  stopForward: vi.fn(async () => undefined),
  startForward: vi.fn(async (spec: unknown) => {
    started.push(spec);
    throw Object.assign(new Error("后端那一句"), { detail: "D-detail" });
  }),
}));
vi.mock("../../../../src/frontend/ui/remote-config", () => ({ readRemoteConfig: vi.fn(async () => ({ hosts: [] })) }));
vi.mock("../../../../src/frontend/ui/kit/toast", () => ({
  toast: (title: string) => void toasts.push({ title, e: null }),
  failToast: (title: string, e: unknown) => void toasts.push({ title, e }),
}));

import { openPortForwardPanel } from "../../../../src/frontend/ui/settings/port-forward";
import { copyText } from "../../../../src/frontend/ui/copy-table";

const flush = () => new Promise((r) => setTimeout(r, 0));

describe("端口转发面板起一条", () => {
  beforeEach(() => {
    started.length = 0;
    toasts.length = 0;
  });

  it("★ 没选机器、端口空着 ⇒ 照样原样交 forward-start；拒了 ⇒ 弹的就是后端那一句", async () => {
    openPortForwardPanel();
    await flush();
    const panel = document.querySelector<HTMLElement>(".pf-panel")!;
    const [local, , rport] = [...panel.querySelectorAll<HTMLInputElement>("input.pf-input")];
    local.value = " 15432x";
    rport.value = "";
    [...panel.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === copyText("backend.buildCells.start"))!.click();
    await flush();
    expect(started).toEqual([{ origin: "", localPort: " 15432x", remoteHost: "localhost", remotePort: "" }]);
    expect(toasts.map((t) => t.title)).toEqual(["后端那一句"]);
  });
});
