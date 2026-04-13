plugins {
    id("java")
    id("org.jetbrains.kotlin.jvm") version "1.9.25"
    id("org.jetbrains.intellij.platform") version "2.2.1"
}

group = "me.koeda"
version = "0.1.0"

repositories {
    mavenCentral()
    intellijPlatform {
        defaultRepositories()
    }
}

dependencies {
    intellijPlatform {
        intellijIdeaCommunity("2024.1")
        bundledPlugin("org.jetbrains.plugins.textmate")
        pluginVerifier()
    }
}

java {
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
}

tasks.withType<org.jetbrains.kotlin.gradle.tasks.KotlinCompile> {
    kotlinOptions.jvmTarget = "17"
}

tasks {
    patchPluginXml {
        sinceBuild.set("241")
        untilBuild.set("253.*")
    }

    // Copy TextMate bundle files into the plugin sandbox alongside the JAR.
    // At runtime, ChordProBundleProvider locates these files via pluginPath.
    prepareSandbox {
        from(layout.projectDirectory.dir("textmate")) {
            into("${rootProject.name}/textmate")
        }
    }

    buildSearchableOptions {
        enabled = false
    }
}
