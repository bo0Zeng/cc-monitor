/**
 * 设置窗那一行「时间轴 · 默认轮换」点进来：当前标签页在那台 ⇒ 开它的面板；不在那台 ⇒ 不替人切，提示里点了才切；
 * 那台一个会话都没有 ⇒ 只提示。三种都先把主窗口拉到前面。宿主是替身，期望手写。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn() }));

import { jumpToAccountPanel, type AcctJumpHost } from "../../../src/frontend/ui/acct-jump";
import { toast } from "../../../src/frontend/ui/kit/toast";
import type { ToastOptions } from "../../../src/frontend/ui/kit/toast";
import { copyText } from "../../../src/frontend/ui/copy-table";

let host: AcctJumpHost & { [K in keyof AcctJumpHost]: ReturnType<typeof vi.fn> };
const lastToast = (): [string, string, ToastOptions] => vi.mocked(toast).mock.calls.at(-1) as never;

beforeEach(() => {
  vi.mocked(toast).mockReset();
  host = {
    raise: vi.fn(),
    active: vi.fn(() => ({ sid: "a", origin: "devbox" })),
    firstOn: vi.fn((o: string) => (o === "gpu-01" ? { sid: "g", title: `ranker ${copyText("history.filter.sort")}${copyText("gridMonitor.fact.model")}` } : null)),
    switchTo: vi.fn(),
    openAt: vi.fn(),
  } as never;
});

describe("open-account-panel", () => {
  it("当前标签页就在那台 ⇒ 拉到前面、开它的面板到那一节，不出提示、不切", () => {
    jumpToAccountPanel({ machine: "devbox", anchor: "default-rotation" }, host);
    expect(host.raise).toHaveBeenCalledTimes(1);
    expect(host.openAt).toHaveBeenCalledWith("a", "devbox", "default-rotation");
    expect(host.switchTo).not.toHaveBeenCalled();
    expect(toast).not.toHaveBeenCalled();
  });

  it("不在那台、那台有会话 ⇒ 不切、不开；提示说在哪、第二行写那个会话，［切过去］点了才切并开面板", () => {
    jumpToAccountPanel({ machine: "gpu-01", anchor: "default-rotation" }, host);
    expect(host.raise).toHaveBeenCalledTimes(1);
    expect(host.switchTo, "不替人切标签页").not.toHaveBeenCalled();
    expect(host.openAt).not.toHaveBeenCalled();
    const [title, detail, opts] = lastToast();
    expect(title).toBe(copyText("acct.jump.rotationElsewhere", { machine: "gpu-01" }));
    expect(detail, "第二行是要切过去的那个会话").toBe(`ranker ${copyText("history.filter.sort")}${copyText("gridMonitor.fact.model")}`);
    const act = opts.action as { label: string; run: () => void };
    expect(act.label).toBe(copyText("acct.jump.switchTo"));
    act.run();
    expect(host.switchTo).toHaveBeenCalledWith("g");
    expect(host.openAt).toHaveBeenCalledWith("g", "gpu-01", "default-rotation");
    jumpToAccountPanel({ machine: "gpu-01", anchor: "timeline" }, host);
    expect(lastToast()[0]).toBe(copyText("acct.jump.timelineElsewhere", { machine: "gpu-01" }));
  });

  it("那台一个会话都没开 ⇒ 只一条提示，不开面板", () => {
    host.active.mockReturnValue(null);
    jumpToAccountPanel({ machine: "<local>", anchor: "timeline" }, host);
    expect(host.raise).toHaveBeenCalledTimes(1);
    expect(host.openAt).not.toHaveBeenCalled();
    const [title, , opts] = lastToast();
    expect(title).toBe(copyText("acct.jump.noSession", { machine: copyText("control.machine.local") }));
    expect(opts.action, "没有可切的就不给按钮").toBeUndefined();
  });
});
