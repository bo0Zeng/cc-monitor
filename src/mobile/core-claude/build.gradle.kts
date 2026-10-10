// 纯 JVM 模块（kotlin("jvm")）：编译器保证没有 Android 耦合。
// 任务名因此不同：测试是 `test`，静态分析是 `detektMain`（带类型解析；plain `detekt` 不做类型解析，会漏规则）。
plugins {
    alias(libs.plugins.kotlin.jvm)
}

java {
    sourceCompatibility = JavaVersion.VERSION_11
    targetCompatibility = JavaVersion.VERSION_11
}

// Kotlin jvmTarget 在根 subprojects{} 里统一配置。

// 与 `:app` 接同一个共享测试源目录 `testing/src/test/kotlin`（源码词法扫描器）；目录里的自检在本模块也跑一遍。
// 手机要和那台后端是同一个 `BUILD_ID` 才接（`后端.md` §1「版本」）。身份只有一处：本仓后端 `src/backend/lib.rs` 的
// `pub const BUILD_ID`；构建时抄进生成的 `EmbeddedBuild.ID`，不在手机里另写一份。认不出那一行 ⇒ 构建失败，不猜。
val backendLib = rootProject.file("../backend/lib.rs")
val embeddedBuildDir = layout.buildDirectory.dir("generated/embeddedBuild")
val genEmbeddedBuild =
    tasks.register("genEmbeddedBuild") {
        inputs.file(backendLib)
        outputs.dir(embeddedBuildDir)
        doLast {
            val id =
                Regex("(?m)^pub const BUILD_ID: &str = \"([^\"]+)\";")
                    .find(backendLib.readText())
                    ?.groupValues
                    ?.get(1)
                    ?: throw GradleException("src/backend/lib.rs 里认不出 `pub const BUILD_ID: &str = \"…\";`")
            val out = embeddedBuildDir.get().file("com/ccmonitor/mobile/core/claude/link/EmbeddedBuild.kt").asFile
            out.parentFile.mkdirs()
            out.writeText(
                "// 生成物：core-claude/build.gradle.kts 的 genEmbeddedBuild 从 src/backend/lib.rs 抄来，勿手改。\n" +
                    "package com.ccmonitor.mobile.core.claude.link\n\n" +
                    "/** 这一版手机带的后端身份（与本仓后端同一个 `BUILD_ID`）。 */\n" +
                    "object EmbeddedBuild {\n    const val ID: String = \"$id\"\n}\n",
            )
        }
    }

// 生成物落在加进源码集的目录里：读源码集的任务（ktlint · detekt）都要排在生成之后。
tasks.matching { it.name.startsWith("runKtlint") || it.name.startsWith("ktlint") || it.name.startsWith("detekt") }.configureEach {
    dependsOn(genEmbeddedBuild)
}

kotlin {
    sourceSets.named("main") {
        kotlin.srcDir(genEmbeddedBuild.map { embeddedBuildDir.get() })
    }
    sourceSets.named("test") {
        kotlin.srcDir(rootProject.file("testing/src/test/kotlin"))
    }
}

dependencies {
    // 用 api：RemoteCommandChannel 出现在公开构造参数里（TailTransport / DaemonTransport / ClaudeSessionCatalog /
    // CodexSessionCatalog），消费方的 compile classpath 上要见到它。
    api(project(":core-remote"))
    implementation(libs.moshi)
    // 传输用 Flow<ByteArray>。用 -core（纯 JVM artifact），不引 Android 耦合。
    implementation(libs.kotlinx.coroutines.core)
    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test)
}
