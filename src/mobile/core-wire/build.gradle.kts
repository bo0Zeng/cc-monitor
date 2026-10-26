// 纯 JVM 模块：线上契约（`src/common/cmd-wire`）的 Kotlin 那一头 —— 只放生成的类与它们的编解码（`gen/`，`tests/scripts/gen-kotlin-wire.mjs`
// 从 ts-rs 导出的声明写，不手改）＋ 装 / 出那几口（`Wire`）。不放任何判定：用它的模块（core-claude · core-ssh · core-data · app）各自判。
plugins {
    alias(libs.plugins.kotlin.jvm)
}

java {
    sourceCompatibility = JavaVersion.VERSION_11
    targetCompatibility = JavaVersion.VERSION_11
}

// 回包金样（Rust 照类型现打）与 ts-rs 导出的声明一变就重跑判据，不吃缓存的绿。
tasks.named<Test>("test") {
    inputs.dir(rootProject.file("../../tests/__fixtures__")).withPathSensitivity(PathSensitivity.RELATIVE)
    inputs.dir(rootProject.file("../frontend/ui/generated")).withPathSensitivity(PathSensitivity.RELATIVE)
}

dependencies {
    // 生成的编解码在公开面上就是 moshi 的 JsonAdapter · JsonReader（用的那一侧也拿它们）。
    api(libs.moshi)
    testImplementation(libs.junit)
}
