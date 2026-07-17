// F87（#50+#51）MCP 管理纯函数断言：groupByScope / serverSummary / parseServerConfig。
import { describe, it, expect, vi } from "vitest";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../error-toast", () => ({ showActionFailureToast: vi.fn() }));

import {
  groupByScope,
  serverSummary,
  parseServerConfig,
  type McpServerEntry,
} from "./mcp-section";

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
