# Rust 核心按名字回调 Kotlin（`Core.Sink` 的 data · sign · hostKey）、按名字找 native 方法（`Java_…_Core_*`）：
# 两边的名字都不许被 R8 改。
-keep class com.ccmonitor.mobile.core.rs.Core { *; }
-keep interface com.ccmonitor.mobile.core.rs.Core$Sink { *; }
-keep class * implements com.ccmonitor.mobile.core.rs.Core$Sink { *; }
# 写字那几个 native 口（`Java_…_CopyCore_*`）住 core-copy，同理。
-keep class com.ccmonitor.mobile.core.copy.CopyCore { *; }
