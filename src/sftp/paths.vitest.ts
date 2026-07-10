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
