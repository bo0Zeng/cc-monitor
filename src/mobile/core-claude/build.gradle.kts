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
kotlin {
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
