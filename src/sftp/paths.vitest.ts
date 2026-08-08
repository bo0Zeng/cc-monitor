import { describe, it, expect } from "vitest";
import {
  breadcrumbs, normalize, joinPath, parentPath, sortEntries, newTransferId,
  type SftpEntry,
} from "./paths";

const mk = (name: string, isDir: boolean, size = 0, lossy = false): SftpEntry => ({
  name, path: `/${name}`, isDir, isSymlink: false, size, lossyName: lossy,
});

describe("F48 paths", () => {
  it("breadcrumbs 累积路径", () => {
    expect(breadcrumbs("/home/pi/proj")).toEqual([
      { name: "/", path: "/" },
      { name: "home", path: "/home" },
      { name: "pi", path: "/home/pi" },
      { name: "proj", path: "/home/pi/proj" },
    ]);
    expect(breadcrumbs("/")).toEqual([{ name: "/", path: "/" }]);
    expect(breadcrumbs("/home/")).toEqual([
      { name: "/", path: "/" },
      { name: "home", path: "/home" },
    ]);
  });
  it("normalize 去重复/尾随斜杠", () => {
    expect(normalize("/a//b/")).toBe("/a/b");
    expect(normalize("/")).toBe("/");
    expect(normalize("//")).toBe("/");
  });
  it("joinPath 根特判", () => {
    expect(joinPath("/", "x")).toBe("/x");
    expect(joinPath("/home/pi", "x")).toBe("/home/pi/x");
    expect(joinPath("/home/pi/", "x")).toBe("/home/pi/x");
  });
  it("parentPath", () => {
    expect(parentPath("/home/pi/proj")).toBe("/home/pi");
    expect(parentPath("/home")).toBe("/");
    expect(parentPath("/")).toBe("/");
  });
  it("sortEntries 目录恒在前 + by", () => {
    const l = [mk("Zeb", false, 10), mk("apple", false, 99), mk("src", true), mk("a.rs", false, 5)];
    expect(sortEntries(l, "name").map((e) => e.name)).toEqual(["src", "a.rs", "apple", "Zeb"]);
    expect(sortEntries(l, "size").map((e) => e.name)).toEqual(["src", "apple", "Zeb", "a.rs"]);
  });
  it("newTransferId 唯一", () => {
    expect(newTransferId()).not.toBe(newTransferId());
  });
});

import { basename, addBookmark, removeBookmark } from "./paths";
describe("F48 bookmarks", () => {
  it("addBookmark 去重 + 归一 + 保序", () => {
    let l: string[] = [];
    l = addBookmark(l, "/home/pi");
    l = addBookmark(l, "/home/pi/");
    l = addBookmark(l, "/var/log");
    expect(l).toEqual(["/home/pi", "/var/log"]);
  });
  it("removeBookmark 归一匹配", () => {
    expect(removeBookmark(["/home/pi", "/var"], "/home/pi/")).toEqual(["/var"]);
  });
});
describe("F48 basename", () => {
  it("取最后一段(兼容反斜杠/尾斜杠)", () => {
    expect(basename("/home/pi/a.txt")).toBe("a.txt");
    expect(basename("C:\\Users\\me\\b.rs")).toBe("b.rs");
    expect(basename("/home/pi/")).toBe("pi");
    expect(basename("solo")).toBe("solo");
  });
});

// ★★ 跨语言接线：**取消令牌的 id 必须来自那个唯一生成器**〔audit-0805 08-08，第 64 件〕。
//
// Rust 侧 `sftp_pool.rs` 的取消注册表逐字写着：「`transfer_id` **必须全局唯一**
// (前端用 uuid)——并发复用同 id 会互相覆盖 flag(D 审计 R2)」。
// 也就是说**唯一性这件事整个托付给了前端**，而 Rust 那边只留了一句注释。
//
// 前端这边有 `newTransferId()`（`crypto.randomUUID`）也有「两次不同」的单测 ——
// 但**没有任何东西钉住调用方真的用它**。08-08 实测：把 `panel.ts` 里那行改成
// 按标签派生（`t-${label}`），**1288 条 vitest 全绿、tsc 也过**（把「未用导入」
// 那个偶然的红隔离掉之后）。而同名传输从此必撞：取消一个会把另一个也取消。
//
// ⇒ 钉两件：① 生成器确实用 `randomUUID`（不是自己拼一个可预测串）；
// ② `panel.ts` 里每一处 `transferId` 的**赋值**都来自 `newTransferId()`。
describe("SFTP 取消令牌 id 的跨语言接线", () => {
  it("生成器走 crypto.randomUUID，且有降级兜底", async () => {
    const { readFileSync } = await import("node:fs");
    const { resolve, dirname } = await import("node:path");
    const { fileURLToPath } = await import("node:url");
    const HERE = dirname(fileURLToPath(import.meta.url));
    const src = readFileSync(resolve(HERE, "paths.ts"), "utf8");
    const fn = src.slice(src.indexOf("export function newTransferId"));
    const body = fn.slice(0, fn.indexOf("\n}"));
    expect(body, "抽不到 newTransferId 的函数体 —— 抽取坏了，本条此刻无效").toContain(
      "return",
    );
    expect(
      body,
      "`newTransferId` 不再用 `crypto.randomUUID` —— Rust 侧的取消注册表把全局唯一性" +
        "整个托付给了这里（sftp_pool.rs：并发复用同 id 会互相覆盖 flag，D 审计 R2）。",
    ).toContain("randomUUID");
  });

  it("panel 里的 transferId 只能来自那个生成器", async () => {
    const { readFileSync } = await import("node:fs");
    const { resolve, dirname } = await import("node:path");
    const { fileURLToPath } = await import("node:url");
    const HERE = dirname(fileURLToPath(import.meta.url));
    const src = readFileSync(resolve(HERE, "panel.ts"), "utf8");
    // 人群 = 每一处给 transferId 赋值的地方（`const transferId = …`）。
    const assigns = src
      .split("\n")
      .filter((l) => /\bconst\s+transferId\s*=/.test(l))
      .map((l) => l.trim());
    expect(
      assigns.length,
      "`panel.ts` 里一处 `const transferId =` 都没抓到 —— 抽取坏了，本条此刻无效",
    ).toBeGreaterThanOrEqual(1);
    for (const line of assigns) {
      expect(
        line,
        `这一行没用 \`newTransferId()\`：${line}\n` +
          "⚠ 取消令牌按 id 注册，id 一撞就互相覆盖 flag —— 取消一个会把另一个也取消。\n" +
          "Rust 侧只写了一句注释把唯一性托付过来（sftp_pool.rs，D 审计 R2），本条是那句话的落点。",
      ).toContain("newTransferId()");
    }
  });
});
