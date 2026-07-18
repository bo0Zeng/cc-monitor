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
