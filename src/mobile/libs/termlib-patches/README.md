# termlib 补丁

`../m2/org/connectbot/termlib/0.1.1/` 里那份 aar 是 [connectbot/termlib](https://github.com/connectbot/termlib)（Apache-2.0，内含 libvterm，MIT）打上本目录补丁后编出来的，不是上游发布的二进制。

| 补丁 | 基于上游 | 改了什么 |
|---|---|---|
| `0001-pinch-zoom-font-size-and-alt-screen-wheel.patch` | `da47e388c309bcc90ad6bb91353cd526ccca5a11`（0.1.0 之后、版本号改成 0.1.1-SNAPSHOT 的那一个提交） | 捏合缩放把累计比例折进字号并回调给宿主保存；新增 JNI `mouseWheel`，应用开了鼠标上报时 alt-screen 竖滑发 SGR 滚轮；改正两个写错的 VTERM 属性常量（TITLE 7→4、CURSORSHAPE 6→7）；`ndkVersion` 定在 27.3.13750724 |

## 重编 aar

```sh
git clone https://github.com/connectbot/termlib && cd termlib
git checkout da47e388c309bcc90ad6bb91353cd526ccca5a11
git apply <本仓>/src/mobile/libs/termlib-patches/0001-pinch-zoom-font-size-and-alt-screen-wheel.patch
./gradlew :lib:publishToMavenLocal   # 要 NDK 27.3.13750724
```

再把 `~/.m2/repository/org/connectbot/termlib/<版本>/` 拷进 `../m2/` 的新版本目录，改 `gradle/libs.versions.toml` 里的 `termlib`。
