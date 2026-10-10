# ───────────────────────────────────────────────────────────────────────────
# R8 full-mode keep 规则。BC/sshj 的 keep 来自 core-ssh/consumer-rules.pro
# （经 consumerProguardFiles 传入）。本文件补 :app 直接依赖里需反射保留的部分。
# ───────────────────────────────────────────────────────────────────────────

# ── Tink（core-data crypto：AEAD 包裹 + Keystore master key）──
# Tink 经反射 + service-loader 按 typeUrl 注册/实例化 key manager，内部用 protobuf-lite。
-keep class com.google.crypto.tink.** { *; }
-keepclassmembers class * extends com.google.protobuf.GeneratedMessageLite { <fields>; }
-keep class * extends com.google.crypto.tink.shaded.protobuf.GeneratedMessageLite { *; }
-dontwarn com.google.crypto.tink.**
-dontwarn com.google.protobuf.**
-dontwarn com.google.errorprone.annotations.**
-dontwarn javax.annotation.**
-dontwarn javax.annotation.concurrent.**

# ── termlib（org.connectbot.terminal）JNI ──
# 原生 libjni_cb_term.so 经 JNI 按名字查字段(如 CellRun.fgRed/fgGreen 等 int 字段)、调回调
# (TerminalCallbacks)、注册 native 方法(TerminalNative.nativeInit)。R8 混淆字段/方法名 →
# native GetFieldID/CallMethod 找不到 → NoSuchFieldError → SIGABRT（连接后开终端即崩）。整包 keep。
-keep class org.connectbot.terminal.** { *; }
# 通用 JNI 安全：任何含 native 方法的类，保留其类名+native 方法名（防 RegisterNatives 失配）。
-keepclasseswithmembernames class * { native <methods>; }

# ── moshi ──
# 本项目仅用 moshi.adapter(Any)（内建 Map/List/基元适配器），不反射 Kotlin data class，
# 故无需 keep 业务模型。仅压制告警。
-dontwarn com.squareup.moshi.**

# ── Koin / Compose / Room / Markwon ──
# 这些库各自 ship consumer proguard 规则（随 AAR 传入），通常无需额外 keep。
# 若 release 运行期出现 ClassNotFound/NoSuchMethod，再按需补 -keep。

# ── Prism4j 语法高亮 ──
# bundler 生成的 GrammarLocatorDef 直接 new Prism_xxx 语法类（非按名反射），R8 正常可达即保留；
# 保守整包 keep + 生成的 locator（同包），防 full-mode 误删语法类导致高亮静默失效。
-keep class io.noties.prism4j.** { *; }
-keep class com.ccmonitor.mobile.ui.claude.GrammarLocatorDef { *; }
-dontwarn io.noties.prism4j.**

# ── JLaTeXMath 数学渲染 ──
# JLaTeXMath 经反射/资源加载符号定义(.xml)+字体；R8 误删 → 渲染崩/空。整包 keep + dontwarn。
-keep class io.noties.markwon.ext.latex.** { *; }
-keep class ru.noties.jlatexmath.** { *; }
-keep class org.scilab.forge.jlatexmath.** { *; }
-dontwarn ru.noties.jlatexmath.**
-dontwarn org.scilab.forge.jlatexmath.**
