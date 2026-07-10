// F54:fileInputPath 纯逻辑——工具卡里可 SFTP 定位的文件路径(仅远端绝对路径)。
import { describe, it, expect } from "vitest";
import { fileInputPath } from "./index";

describe("F54 fileInputPath", () => {
  it("Read/Write/Edit 的绝对 file_path → 返回", () => {
    expect(fileInputPath({ file_path: "/home/pi/proj/foo.ts" })).toBe("/home/pi/proj/foo.ts");
  });
  it("NotebookEdit 用 notebook_path", () => {
    expect(fileInputPath({ notebook_path: "/home/pi/nb.ipynb" })).toBe("/home/pi/nb.ipynb");
  });
  it("file_path 优先于 notebook_path", () => {
    expect(fileInputPath({ file_path: "/a", notebook_path: "/b" })).toBe("/a");
  });
  it("相对路径 → null(parentPath 会定位错目录)", () => {
    expect(fileInputPath({ file_path: "src/foo.ts" })).toBeNull();
    expect(fileInputPath({ file_path: "" })).toBeNull();
  });
  it("非文件工具 / 非字符串 / 非对象 → null", () => {
    expect(fileInputPath({ command: "ls -la" })).toBeNull(); // Bash
    expect(fileInputPath({ file_path: 123 })).toBeNull();
    expect(fileInputPath("plain string")).toBeNull();
    expect(fileInputPath(null)).toBeNull();
    expect(fileInputPath(["/a"])).toBeNull();
  });
});
