/**
 * **后端那几格住每台机器页的「这台上的 cc-monitor」里**（列表那一行只有点 · 名字 · 地址 · ⋯）。
 *
 * # 钉什么
 *
 * 1. 每一台机器页上**恰好一份**后端几格，`data-backend-cells` 就是后端那套 origin
 *    （本机 = `LOCAL_ORIGIN`，不是 UI 账本的 `LOCAL_MACHINE_KEY`）—— 两向集合相等。
 * 2. 列表里有、后端清单里没有的一台 ⇒ 状态格说「未登记」＋ ⓘ，**不摆**起停与退出那几格（没有把手）。
 * 3. 后端清单里有、列表里没有的一台 ⇒ 不丢：列表尾巴里另起一行。
 * 4. 构造失败的那几格不许把机器列表带走。
 *
 * 真 `RemoteSection` ＋ 真 `BackendSection`（寄居模式），页宿主照设置窗那样给每页挂那台的几格；只把 IPC 与 config 换成桩。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../../../src/frontend/ui/config", () => ({ loadConfig: vi.fn(), patchConfig: vi.fn(), patchConfigFrom: vi.fn() })); // 写口换成按键补丁
vi.mock("@tauri-apps/api/core", () => ({
  Channel: class {
    onmessage: ((v: unknown) => void) | null = null;
  },
  invoke: vi.fn(),
}));
const { ipc } = vi.hoisted(() => ({
  ipc: {
    calls: [] as { name: string; args: unknown }[],
    registered: ["<local>", "甲机"] as string[],
  },
}));
vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
  commands: new Proxy(
    {},
    {
      get: (_t, name: string) => (args: unknown) => {
        ipc.calls.push({ name, args });
        switch (name) {
          case "backend_machines":
            return Promise.resolve([...ipc.registered]);
          case "backend_status":
            return Promise.resolve({ channel: true, pid: 7 });
          // 「退出行为」那一问改走通道：一发 `chan_call`（op = `exit-policy-read`），回后端那份字节。
          case "chan_call": {
            // 「导入」下拉那一问（`ssh-config-aliases`）也经这一跳：回空清单。
            const op = (args as { op?: string } | undefined)?.op;
            const body =
              op === "ssh-config-aliases"
                ? { aliases: [] }
                : { state: "absent", killOnExit: false, said: "退出时后台照常跑" };
            const u = new TextEncoder().encode(JSON.stringify(body));
            return Promise.resolve(u.buffer.slice(u.byteOffset, u.byteOffset + u.byteLength));
          }
          default:
            return Promise.resolve(undefined);
        }
      },
    },
  ),
}));
vi.mock("../../../../src/frontend/ui/kit/toast", () => ({ toast: () => {} }));

import { loadConfig } from "../../../../src/frontend/ui/config";
import { RemoteSection } from "../../../../src/frontend/ui/settings/remote-section";
import { BACKEND_UNREGISTERED_WHY, BackendSection } from "../../../../src/frontend/ui/settings/backend-section";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/backend-policy";
import type { RemoteHostConfig } from "../../../../src/frontend/ui/remote-config";
import { copyText } from "../../../../src/frontend/ui/copy-table";

const tick = () => new Promise((r) => setTimeout(r, 0));

function mkH(label: string, host: string): RemoteHostConfig {
  return {
    label,
    host,
    port: 22,
    user: "u",
    keyPath: "",
    hostKeyFingerprint: "",
    addresses: [],
    jump: "",
    resumeCommand: "",
    connect: true,
  };
}

const cur: { sec?: RemoteSection; backend?: BackendSection; cellsThrow?: boolean } = {};
const pageBox = document.createElement("div");
const pages = {
  addMachinePage: (id: string) => {
    const page = document.createElement("div");
    page.dataset.pageId = id;
    const origin = id === "machine:（本机）" ? LOCAL_ORIGIN : cur.sec!.originOfPage(id);
    try {
      if (cur.cellsThrow) throw new Error("四格炸了");
      if (origin) page.appendChild(cur.backend!.cellsFor(origin));
    } catch {
      // 设置窗那一侧同样收住
    }
    pageBox.appendChild(page);
  },
  removeMachinePage: () => {},
  navigateToMachinePage: () => {},
};

async function mount(
  hosts: RemoteHostConfig[],
  opts: { cellsThrow?: boolean } = {},
): Promise<{ sec: RemoteSection; backend: BackendSection }> {
  vi.mocked(loadConfig).mockResolvedValue({
    remote: { enabled: true, hosts },
  } as unknown as Awaited<ReturnType<typeof loadConfig>>);
  const backend = new BackendSection({ headless: true, hosted: true });
  cur.backend = backend;
  cur.cellsThrow = opts.cellsThrow;
  pageBox.replaceChildren();
  const sec = new RemoteSection({ headless: true, pages, rowExtras: { tail: () => backend.element } });
  cur.sec = sec;
  document.body.append(sec.element, pageBox);
  for (let i = 0; i < 4; i++) await tick();
  return { sec, backend };
}

beforeEach(() => {
  ipc.calls = [];
  ipc.registered = ["<local>", "甲机"];
  document.body.replaceChildren();
  vi.mocked(loadConfig).mockReset();
});

describe("后端那几格住机器页", () => {
  it("★★ 每一页恰好一份，origin 是后端那套名字（两向集合相等）；列表那一行上一份都没有", async () => {
    const { sec } = await mount([mkH("甲机", "1.1.1.1")]);
    const rows = [...pageBox.querySelectorAll<HTMLElement>("[data-page-id]")];
    expect(rows.length, "一页都没有 —— 下面的两向相等在空人群上恒绿").toBe(2);
    const withCells = rows.filter((r) => r.querySelectorAll("[data-backend-cells]").length === 1);
    expect(withCells.length, "有的页没有、或挂了两份").toBe(rows.length);
    const origins = rows.map((r) => r.querySelector<HTMLElement>("[data-backend-cells]")!.dataset.backendCells);
    expect(origins.sort()).toEqual([LOCAL_ORIGIN, "甲机"].sort());
    expect(sec.element.querySelectorAll(".remote-machine-row [data-backend-cells]").length).toBe(0);
    expect(sec.element.querySelectorAll(".remote-machine-row").length).toBe(2);
  });

  it("★ 四格真的画上了读数（状态 / 起停 / 勾），不是空壳", async () => {
    const { sec } = await mount([mkH("甲机", "1.1.1.1")]);
    void sec;
    const all = pageBox.querySelectorAll<HTMLElement>("[data-backend-cells]");
    expect(all.length).toBe(2);
    for (const cells of all) {
      expect(cells.querySelector(".backend-row-state")?.textContent).toBe(copyText("backend.status.connected"));
      expect([...cells.querySelectorAll('[data-col="state"] [data-op]')].map((b) => b.textContent)).toEqual([copyText("backend.buildCells.stop"), copyText("backend.buildCells.restart"), copyText("backend.buildCells.start")]);
      expect([...cells.querySelectorAll('[data-col="ops"] button')].map((b) => b.textContent)).toEqual([copyText("backend.buildCells.log"), copyText("backend.buildCells.resync")]);
      expect(cells.querySelector('.backend-row-kill [role="switch"]')!.getAttribute("aria-disabled")).toBeNull();
    }
    const asked = ipc.calls
      .filter((c) => c.name === "backend_status")
      .map((c) => (c.args as { origin: string }).origin);
    expect(new Set(asked)).toEqual(new Set([LOCAL_ORIGIN, "甲机"]));
  });

  it("★★ 列表里有、后端清单里没有 ⇒「未登记」＋ ⓘ，起停与退出那几格不摆控件", async () => {
    ipc.registered = ["<local>"];
    const { sec } = await mount([mkH("甲机", "1.1.1.1")]);
    void sec;
    const cells = pageBox.querySelector<HTMLElement>('[data-backend-cells="甲机"]')!;
    expect(cells.querySelector(".backend-row-state")?.textContent).toBe(copyText("backend.paintUnregistered.unregistered"));
    expect(cells.querySelector("[aria-label]")?.getAttribute("aria-label")).toBe(BACKEND_UNREGISTERED_WHY());
    expect(cells.querySelectorAll("button").length, "没有把手的一台还摆着起 / 停").toBe(0);
    expect(cells.querySelector(".backend-row-kill")).toBeNull();
    const asked = ipc.calls
      .filter((c) => c.name === "backend_status")
      .map((c) => (c.args as { origin: string }).origin);
    expect(asked, "没登记的那台还去问了状态").not.toContain("甲机");
    // 反向对照：登记了的本机照常画。
    expect(
      pageBox.querySelector<HTMLElement>(`[data-backend-cells="${LOCAL_ORIGIN}"] .backend-row-state`)?.textContent,
    ).toBe(copyText("backend.status.connected"));
  });

  it("★★ 后端清单里有、列表里没有 ⇒ 不丢：列表尾巴里另起一行，且只有这一台", async () => {
    ipc.registered = ["<local>", "甲机", "乙机 (#2)"];
    const { sec, backend } = await mount([mkH("甲机", "1.1.1.1")]);
    expect(backend.element.isConnected, "尾巴没挂进列表").toBe(true);
    expect(sec.element.querySelector(".remote-machines")!.lastElementChild).toBe(backend.element);
    const orphans = [...backend.element.querySelectorAll<HTMLElement>(".backend-row")].map((r) => r.dataset.origin);
    expect(orphans).toEqual(["乙机 (#2)"]);
    // 一台机只许一份四格。
    const all = [...document.querySelectorAll<HTMLElement>("[data-backend-cells]")].map(
      (c) => c.dataset.backendCells,
    );
    expect(all.sort()).toEqual([LOCAL_ORIGIN, "乙机 (#2)", "甲机"].sort());
  });

  it("★ 页上那几格构造抛了 ⇒ 机器列表照常在；后端那几台退到尾巴里，一台不丢", async () => {
    const { sec, backend } = await mount([mkH("甲机", "1.1.1.1")], { cellsThrow: true });
    const rows = sec.element.querySelectorAll(".remote-machine-row");
    expect(rows.length).toBe(2);
    expect(sec.element.querySelectorAll(".remote-machine-row [data-backend-cells]").length).toBe(0);
    const orphans = [...backend.element.querySelectorAll<HTMLElement>(".backend-row")].map((r) => r.dataset.origin);
    expect(orphans.sort()).toEqual([LOCAL_ORIGIN, "甲机"].sort());
  });
});
