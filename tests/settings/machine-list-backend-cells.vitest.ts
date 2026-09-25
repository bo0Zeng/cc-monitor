/**
 * 〔第四波 ST2 · `设计/70 §5.3` · 第四刀 步 14〕**机器页去停车场化：DAEMON 开关并进列表行。**
 *
 * # 钉什么
 *
 * 1. 每一台机器在列表里那一行上**恰好一份**后端四格，`data-backend-cells` 就是后端那套 origin
 *    （本机 = `LOCAL_ORIGIN`，不是 UI 账本的 `LOCAL_MACHINE_KEY`）—— 两向集合相等：
 *    行的集合 == 挂了四格的行的集合；四格的 origin 集合 == 这几行该有的 origin 集合。
 * 2. 列表里有、后端清单里没有的一台 ⇒ 状态格说「未登记」＋ ⓘ，**不摆**起停与退出那几格（没有把手）。
 * 3. 后端清单里有、列表里没有的一台 ⇒ 不丢：列表尾巴里另起一行。
 * 4. 构造失败的四格不许把机器列表带走（`guardedExtra`，同 `safeBlock` 的隔离）。
 *
 * 真 `RemoteSection` ＋ 真 `BackendSection`（寄居模式），只把 IPC 与 config 换成桩。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/config", () => ({ loadConfig: vi.fn(), saveConfig: vi.fn() }));
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
vi.mock("../../src/ipc/commands", () => ({
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
          // 〔C4c · 第四波 4B〕「退出行为」那一问改走通道：一发 `chan_call`（op = `exit-policy-read`），回后端那份字节。
          case "chan_call": {
            const u = new TextEncoder().encode(JSON.stringify({ state: "absent", killOnExit: false }));
            return Promise.resolve(u.buffer.slice(u.byteOffset, u.byteOffset + u.byteLength));
          }
          case "list_ssh_host_aliases":
            return Promise.resolve([]);
          default:
            return Promise.resolve(undefined);
        }
      },
    },
  ),
}));
vi.mock("../../src/error-toast", () => ({ showActionFailureToast: () => {} }));

import { loadConfig } from "../../src/config";
import { RemoteSection } from "../../src/settings/remote-section";
import { BACKEND_UNREGISTERED_WHY, BackendSection } from "../../src/settings/backend-section";
import { LOCAL_ORIGIN } from "../../src/backend-policy";
import type { RemoteHostConfig } from "../../src/remote-config";

const tick = () => new Promise((r) => setTimeout(r, 0));

function mkH(label: string, host: string): RemoteHostConfig {
  return {
    label,
    host,
    port: 22,
    user: "u",
    keyPath: "",
    backendPath: "",
    hostKeyFingerprint: "",
    addresses: [],
    jump: "",
    resumeCommand: "",
  };
}

const pages = {
  addMachinePage: () => {},
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
  const sec = new RemoteSection({
    headless: true,
    pages,
    rowExtras: {
      head: () => BackendSection.columnHead(),
      cells: (origin) => {
        if (opts.cellsThrow) throw new Error("四格炸了");
        return backend.cellsFor(origin);
      },
      tail: () => backend.element,
    },
  });
  document.body.appendChild(sec.element);
  for (let i = 0; i < 4; i++) await tick();
  return { sec, backend };
}

beforeEach(() => {
  ipc.calls = [];
  ipc.registered = ["<local>", "甲机"];
  document.body.replaceChildren();
  vi.mocked(loadConfig).mockReset();
});

describe("〔ST2 · 步 14〕DAEMON 开关并进机器列表行", () => {
  it("★★ 每一行恰好一份四格，origin 是后端那套名字（两向集合相等）", async () => {
    const { sec } = await mount([mkH("甲机", "1.1.1.1")]);
    const rows = [...sec.element.querySelectorAll<HTMLElement>(".remote-machine-row")];
    expect(rows.length, "一行都没有 —— 下面的两向相等在空人群上恒绿").toBe(2);
    const withCells = rows.filter((r) => r.querySelectorAll("[data-backend-cells]").length === 1);
    expect(withCells.length, "有的行没有四格、或有的行挂了两份").toBe(rows.length);
    const origins = rows.map((r) => r.querySelector<HTMLElement>("[data-backend-cells]")!.dataset.backendCells);
    expect(origins.sort()).toEqual([LOCAL_ORIGIN, "甲机"].sort());
    // 本机那一行挂的是后端的本机名，不是 UI 账本那个「（本机）」。
    const local = sec.element.querySelector<HTMLElement>(".remote-machine-local")!;
    expect(local.querySelector<HTMLElement>("[data-backend-cells]")!.dataset.backendCells).toBe(LOCAL_ORIGIN);
    // 表头在列表最上面，恰好一份。
    expect(sec.element.querySelectorAll('[data-backend-columns="head"]').length).toBe(1);
  });

  it("★ 四格真的画上了读数（状态 / 起停 / 勾），不是空壳", async () => {
    const { sec } = await mount([mkH("甲机", "1.1.1.1")]);
    for (const cells of sec.element.querySelectorAll<HTMLElement>(".remote-machine-row [data-backend-cells]")) {
      expect(cells.querySelector(".backend-row-state")?.textContent).toBe("已连上（pid 7）");
      expect([...cells.querySelectorAll('[data-col="ops"] button')].map((b) => b.textContent)).toEqual(["起", "停"]);
      expect(cells.querySelector<HTMLInputElement>(".backend-row-kill input")!.disabled).toBe(false);
    }
    const asked = ipc.calls
      .filter((c) => c.name === "backend_status")
      .map((c) => (c.args as { origin: string }).origin);
    expect(new Set(asked)).toEqual(new Set([LOCAL_ORIGIN, "甲机"]));
  });

  it("★★ 列表里有、后端清单里没有 ⇒「未登记」＋ ⓘ，起停与退出那几格不摆控件", async () => {
    ipc.registered = ["<local>"];
    const { sec } = await mount([mkH("甲机", "1.1.1.1")]);
    const cells = sec.element.querySelector<HTMLElement>('[data-backend-cells="甲机"]')!;
    expect(cells.querySelector(".backend-row-state")?.textContent).toBe("未登记");
    expect(cells.querySelector("[aria-label]")?.getAttribute("aria-label")).toBe(BACKEND_UNREGISTERED_WHY);
    expect(cells.querySelectorAll("button").length, "没有把手的一台还摆着起 / 停").toBe(0);
    expect(cells.querySelector(".backend-row-kill")).toBeNull();
    const asked = ipc.calls
      .filter((c) => c.name === "backend_status")
      .map((c) => (c.args as { origin: string }).origin);
    expect(asked, "没登记的那台还去问了状态").not.toContain("甲机");
    // 反向对照：登记了的本机照常画。
    expect(
      sec.element.querySelector<HTMLElement>(`[data-backend-cells="${LOCAL_ORIGIN}"] .backend-row-state`)?.textContent,
    ).toBe("已连上（pid 7）");
  });

  it("★★ 后端清单里有、列表里没有 ⇒ 不丢：列表尾巴里另起一行，且只有这一台", async () => {
    ipc.registered = ["<local>", "甲机", "乙机 (#2)"];
    const { sec, backend } = await mount([mkH("甲机", "1.1.1.1")]);
    expect(backend.element.isConnected, "尾巴没挂进列表").toBe(true);
    expect(sec.element.querySelector(".remote-machines")!.lastElementChild).toBe(backend.element);
    const orphans = [...backend.element.querySelectorAll<HTMLElement>(".backend-row")].map((r) => r.dataset.origin);
    expect(orphans).toEqual(["乙机 (#2)"]);
    // 一台机只许一份四格。
    const all = [...sec.element.querySelectorAll<HTMLElement>("[data-backend-cells]")].map(
      (c) => c.dataset.backendCells,
    );
    expect(all.sort()).toEqual([LOCAL_ORIGIN, "乙机 (#2)", "甲机"].sort());
  });

  it("★ 行上的四格构造抛了 ⇒ 机器列表照常在；后端那几台退到尾巴里，一台不丢", async () => {
    const { sec, backend } = await mount([mkH("甲机", "1.1.1.1")], { cellsThrow: true });
    const rows = sec.element.querySelectorAll(".remote-machine-row");
    expect(rows.length).toBe(2);
    expect(sec.element.querySelectorAll(".remote-machine-row [data-backend-cells]").length).toBe(0);
    const orphans = [...backend.element.querySelectorAll<HTMLElement>(".backend-row")].map((r) => r.dataset.origin);
    expect(orphans.sort()).toEqual([LOCAL_ORIGIN, "甲机"].sort());
  });
});
