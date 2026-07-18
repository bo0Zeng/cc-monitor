// F87（#50+#51）MCP 管理纯函数断言：groupByScope / serverSummary / parseServerConfig。
import { describe, it, expect, vi } from "vitest";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../error-toast", () => ({ showActionFailureToast: vi.fn() }));

// jsdom 无 scrollIntoView（McpSection.beginEdit 会调它，真 webview 有）→ 补空实现，免 uncaught。
if (!("scrollIntoView" in Element.prototype)) {
  (Element.prototype as unknown as { scrollIntoView: () => void }).scrollIntoView = () => {};
}

import {
  groupByScope,
  serverSummary,
  parseServerConfig,
  stableStringify,
  catalogKey,
  McpSection,
  type McpServerEntry,
} from "./mcp-section";
import { invoke } from "@tauri-apps/api/core";

const ent = (scope: McpServerEntry["scope"], name: string, server: unknown): McpServerEntry => ({
  scope,
  name,
  server,
  sourcePath: "",
});

describe("F87 groupByScope", () => {
  it("按 scope 分组、保序、忽略未知 scope", () => {
    const g = groupByScope([
      ent("project", "p1", {}),
      ent("user", "u1", {}),
      ent("project", "p2", {}),
      // @ts-expect-error 测未知 scope 被忽略
      ent("weird", "w", {}),
    ]);
    expect(g.user.map((e) => e.name)).toEqual(["u1"]);
    expect(g.project.map((e) => e.name)).toEqual(["p1", "p2"]); // 保序
    expect(g.local).toEqual([]);
  });
});

describe("F87 serverSummary", () => {
  it("远程型：type · url（缺 type 默认 http）", () => {
    expect(serverSummary({ type: "sse", url: "https://x" })).toBe("sse · https://x");
    expect(serverSummary({ url: "https://y" })).toBe("http · https://y");
  });
  it("stdio 型：stdio · command args", () => {
    expect(serverSummary({ command: "npx", args: ["-y", "@x/mcp"] })).toBe("stdio · npx -y @x/mcp");
    expect(serverSummary({ command: "server" })).toBe("stdio · server");
  });
  it("未知形态 / 非对象 → (未知形态)", () => {
    expect(serverSummary({})).toBe("(未知形态)");
    expect(serverSummary(null)).toBe("(未知形态)");
    expect(serverSummary("x")).toBe("(未知形态)");
  });
});

describe("F87 parseServerConfig", () => {
  it("合法对象 → ok", () => {
    const r = parseServerConfig('{ "command": "npx" }');
    expect(r.ok).toBe(true);
    if (r.ok) expect((r.value as { command: string }).command).toBe("npx");
  });
  it("空 / 无效 JSON / 非对象 / 数组 → error", () => {
    expect(parseServerConfig("").ok).toBe(false);
    expect(parseServerConfig("  ").ok).toBe(false);
    expect(parseServerConfig("{bad").ok).toBe(false);
    expect(parseServerConfig("42").ok).toBe(false);
    expect(parseServerConfig('"str"').ok).toBe(false);
    expect(parseServerConfig("[1,2]").ok).toBe(false);
  });
});

// F87b-fix(batch18)：编辑 project 条目时**锁名**——防「改名再存 = 后端 insert 新 key 静默生副本」脚枪。
describe("F87b-fix 编辑锁名", () => {
  const flush = () => new Promise((r) => setTimeout(r, 0));
  it("点编辑 → 名 readonly + 横幅 + JSON 预填；取消 → 复位", async () => {
    document.body.replaceChildren();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "read_mcp_servers")
        return [
          { scope: "project", name: "srv1", server: { command: "npx" }, sourcePath: "/proj/.mcp.json" },
        ];
      return []; // list_mcp_project_dirs / list_remote_mcp_origins
    });
    const section = new McpSection();
    document.body.appendChild(section.element);
    await flush(); // 构造期 reload 完成
    // 设项目目录（writable 需 dir）并「读取」重渲染出带「编辑」钮的 project 条目
    const dirInput = section.element.querySelector<HTMLInputElement>('input[placeholder^="项目目录"]')!;
    dirInput.value = "/proj";
    [...section.element.querySelectorAll("button")].find((b) => b.textContent === "读取")!.click();
    await flush();

    const nameInput = section.element.querySelector<HTMLInputElement>('input[placeholder="server 名"]')!;
    const jsonInput = section.element.querySelector<HTMLTextAreaElement>(".mcp-json-input")!;
    const banner = section.element.querySelector<HTMLElement>(".mcp-edit-banner")!;
    expect(nameInput.readOnly).toBe(false);
    expect(banner.style.display).toBe("none");

    // 点「编辑」
    [...section.element.querySelectorAll("button")].find((b) => b.textContent === "编辑")!.click();
    expect(nameInput.value).toBe("srv1");
    expect(nameInput.readOnly).toBe(true); // 锁名 = 编辑只改配置、不改名
    expect(banner.style.display).not.toBe("none");
    expect(banner.textContent).toContain("srv1");
    expect(jsonInput.value).toContain("npx"); // JSON 预填

    // 点「取消编辑」→ 复位
    [...section.element.querySelectorAll("button")].find((b) => b.textContent === "取消编辑")!.click();
    expect(nameInput.readOnly).toBe(false);
    expect(nameInput.value).toBe("");
    expect(banner.style.display).toBe("none");
  });
});

describe("F89b catalog dedup 纯函数", () => {
  it("stableStringify: 键序无关、递归", () => {
    expect(stableStringify({ a: 1, b: 2 })).toBe(stableStringify({ b: 2, a: 1 }));
    expect(stableStringify({ x: { p: 1, q: 2 } })).toBe(stableStringify({ x: { q: 2, p: 1 } }));
    expect(stableStringify([1, { a: 1, b: 2 }])).toBe(stableStringify([1, { b: 2, a: 1 }]));
    expect(stableStringify(null)).toBe("null");
    expect(stableStringify("s")).toBe('"s"');
  });
  it("catalogKey: 同配置不同键序 → 同键；不同配置/名 → 异键", () => {
    expect(catalogKey("s", { command: "x", args: ["-y"] })).toBe(
      catalogKey("s", { args: ["-y"], command: "x" }),
    );
    expect(catalogKey("s", { command: "x" })).not.toBe(catalogKey("s", { command: "y" }));
    expect(catalogKey("s1", { command: "x" })).not.toBe(catalogKey("s2", { command: "x" }));
  });
});

// F89a：远端项目级 MCP——空/新远端项目仍须出加表单（否则无法建第一条 server；审计逮到的阻塞）。
describe("F89a 远端项目管理", () => {
  const flush = () => new Promise((r) => setTimeout(r, 0));
  it("空远端项目 → 仍渲染 project scope + 加表单（可建首条）", async () => {
    document.body.replaceChildren();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "list_remote_mcp_origins") return ["pi"];
      if (cmd === "read_remote_project_mcp") return []; // 空远端项目 .mcp.json
      return []; // list_remote_mcp_project_dirs / read_mcp_servers / read_remote_mcp_servers
    });
    const section = new McpSection();
    document.body.appendChild(section.element);
    await flush();
    // 切到远端 pi
    section.element
      .querySelectorAll<HTMLElement>(".mcp-machine-btn")
      .forEach((b) => b.textContent === "pi" && b.click());
    await flush();
    // 填远端项目目录并「读取」→ 走 reloadRemoteProject（空）
    const dirInput = section.element.querySelector<HTMLInputElement>('input[placeholder^="项目目录"]')!;
    dirInput.value = "/remote/proj";
    [...section.element.querySelectorAll("button")].find((b) => b.textContent === "读取")!.click();
    await flush();
    // 阻塞修：空远端项目也出加表单（否则建不了第一条）
    expect(section.element.querySelector(".mcp-add-form")).not.toBeNull();
    // 且「添加/更新」钮可用（有 dir）
    const save = [...section.element.querySelectorAll("button")].find(
      (b) => b.textContent === "添加/更新到 .mcp.json",
    ) as HTMLButtonElement | undefined;
    expect(save && !save.disabled).toBe(true);
  });
});

describe("F89b 库 UI（累积 + 已在本项目 + 注册）", () => {
  const flush = () => new Promise((r) => setTimeout(r, 0));
  it("跨项目累积；当前项目已有 → 标注；不在 → 注册钮写入", async () => {
    document.body.replaceChildren();
    const writes: { projectDir?: string; name?: string }[] = [];
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      const a = args as { projectDir?: string; name?: string } | undefined;
      if (cmd === "read_mcp_servers") {
        if (a?.projectDir === "/p1")
          return [{ scope: "project", name: "a", server: { command: "x" }, sourcePath: "" }];
        if (a?.projectDir === "/p2")
          return [{ scope: "project", name: "b", server: { command: "y" }, sourcePath: "" }];
        return [];
      }
      if (cmd === "write_project_mcp_server") {
        writes.push({ projectDir: a?.projectDir, name: a?.name });
        return undefined;
      }
      return []; // list_* / origins
    });
    const section = new McpSection();
    document.body.appendChild(section.element);
    await flush();
    const dirInput = section.element.querySelector<HTMLInputElement>('input[placeholder^="项目目录"]')!;
    const readBtn = () =>
      [...section.element.querySelectorAll("button")].find((b) => b.textContent === "读取")!;
    // 读 /p1 → 库 {a}；a 已在本项目 → 标注、无注册钮
    dirInput.value = "/p1";
    readBtn().click();
    await flush();
    expect(section.element.querySelector(".mcp-catalog")).not.toBeNull();
    expect(section.element.querySelectorAll(".mcp-catalog-here").length).toBe(1); // a 已在本项目
    expect(section.element.querySelectorAll(".mcp-catalog-reg").length).toBe(0);
    // 读 /p2 → 库 {a,b}；b 已在 /p2 标注，a 不在 → 注册钮
    dirInput.value = "/p2";
    readBtn().click();
    await flush();
    const regBtns = section.element.querySelectorAll<HTMLButtonElement>(".mcp-catalog-reg");
    expect(regBtns.length).toBe(1); // 只有 a 可注册
    regBtns[0].click();
    await flush();
    expect(writes.some((w) => w.name === "a" && w.projectDir === "/p2")).toBe(true); // a 注册进 /p2
  });
});
