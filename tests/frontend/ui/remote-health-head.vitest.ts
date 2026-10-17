/**
 * 远端健康提示的标题按壳给的类别挑（`stream_source/run.rs::version_health_kind` 按那台回的新旧挑好要更新 · 较新 · 不可比），
 * 界面不自己猜哪一种；壳那边三个类别字每个都在这里有自己那一句，不落到通用「远端提示」。
 */
import { describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

import { headlineFor } from "../../../src/frontend/ui/remote-health";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { REPO_ROOT } from "../../test-support/repo-root";

const src = readFileSync(resolve(REPO_ROOT, "src/frontend/shell/src/stream_source/run.rs"), "utf8");
const kind = (name: string): string => {
  const m = new RegExp(`const ${name}: &str = "([^"]+)";`).exec(src);
  if (!m) throw new Error(`run.rs 里没有 ${name}`);
  return m[1];
};

describe("远端健康提示：版本那条的标题", () => {
  it("要更新 · 较新 · 不可比 各说各的，都不落到通用标题", () => {
    expect(headlineFor(kind("VERSION_KIND_OLDER"), "devbox")).toBe(copyText("remoteHealth.head.backendOld"));
    expect(headlineFor(kind("VERSION_KIND_NEWER"), "devbox")).toBe(copyText("remoteHealth.head.backendNewer"));
    expect(headlineFor(kind("VERSION_KIND_INCOMPARABLE"), "devbox")).toBe(copyText("remoteHealth.head.versionIncomparable"));
    expect(headlineFor("version", "devbox"), "旧的总称类别不再有").toBe(copyText("remoteHealth.head.notice"));
  });
});

describe("远端健康提示：拥塞那一条的标题", () => {
  it("正文是那台后端写的那一句（不带机器名）⇒ 标题是机器名，本机称「本机」那一格", () => {
    expect(headlineFor("overflow", "devbox")).toBe("devbox");
    expect(headlineFor("overflow", "<local>")).toBe(copyText("acct.machine.here"));
  });
});

describe("远端健康提示：壳带了复制详情就出［复制详情］", () => {
  it("文件窗口开了又退：那一句上屏、退出状态与错误输出在详情里；没带详情的那条不出按钮", async () => {
    const { listen } = await import("@tauri-apps/api/event");
    const { bindRemoteHealthToast } = await import("../../../src/frontend/ui/remote-health");
    bindRemoteHealthToast();
    const handler = vi.mocked(listen).mock.calls.at(-1)![1] as (e: { payload: unknown }) => void;
    handler({ payload: { origin: "devbox", kind: "filewin-exit", message: "窗口开了又退 · 夹具", detail: "码：exit status: 7" } });
    handler({ payload: { origin: "devbox", kind: "overflow", message: "丢了几行", detail: "" } });
    const toasts = [...document.querySelectorAll("[role=status], [role=alert]")];
    const withCopy = toasts.filter((t) => [...t.querySelectorAll("button")].some((b) => b.textContent === copyText("detail.act.copy")));
    expect(withCopy.length, "带详情的那条该有［复制详情］，没带的不该有").toBe(1);
    expect(withCopy[0].textContent).toContain("窗口开了又退");
  });
});
