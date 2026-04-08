import com.vanniktech.maven.publish.JavadocJar
import com.vanniktech.maven.publish.KotlinJvm

plugins {
    id("org.jetbrains.kotlin.jvm") version "1.9.25"
    id("java-library")
    // The Vanniktech maven-publish plugin auto-applies `maven-publish`
    // and `signing`, configures the Central Portal upload endpoint, and
    // wraps the close+release steps into a single Gradle task. This
    // replaces the legacy raw `maven-publish` + `signing` setup that
    // targeted the (sunset) `s01.oss.sonatype.org` OSSRH endpoint.
    // See: https://vanniktech.github.io/gradle-maven-publish-plugin/central
    //
    // Pinned to 0.30.0: 0.34.0+ requires Kotlin 2.2.0, but the JVM
    // target above is on Kotlin 1.9.25. 0.30.0 is the last release that
    // supports Kotlin 1.9.x while also supporting the Central Portal.
    id("com.vanniktech.maven.publish") version "0.30.0"
}

group = "com.koedame"
// Read version from crates/ffi/Cargo.toml to keep in sync with Rust crate.
version = Regex("""^version\s*=\s*"([^"]+)"""", RegexOption.MULTILINE)
    .find(file("${rootDir}/../../crates/ffi/Cargo.toml").readText())
    ?.groupValues?.get(1)
    ?: error("Failed to parse version from crates/ffi/Cargo.toml — expected a line matching: version = \"...\"")


repositories {
    mavenCentral()
}

dependencies {
    implementation("net.java.dev.jna:jna:5.17.0")
    testImplementation("org.jetbrains.kotlin:kotlin-test")
    testImplementation("org.jetbrains.kotlin:kotlin-test-junit5")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

java {
    toolchain {
        languageVersion.set(JavaLanguageVersion.of(17))
    }
}

tasks.test {
    useJUnitPlatform()
}

mavenPublishing {
    // Override coordinates so the published artifactId is `chordsketch`
    // rather than the Gradle module name (`lib`).
    coordinates("com.koedame", "chordsketch", project.version.toString())

    // Maven Central requires every published artifact to ship a sources
    // jar and a (possibly empty) javadoc jar. KotlinJvm() configures the
    // plugin to build both. JavadocJar.Empty() ships a stub since we do
    // not run a Dokka task, while still satisfying Central's metadata
    // validation.
    configure(
        KotlinJvm(
            javadocJar = JavadocJar.Empty(),
            sourcesJar = true,
        ),
    )

    // Publish to the Central Portal (https://central.sonatype.com/), and
    // automatically progress the deployment from VALIDATED to RELEASED
    // so a successful CI run leaves the artifact actually consumable.
    publishToMavenCentral(automaticRelease = true)

    // Maven Central requires every artifact to be GPG-signed. The key
    // material is supplied via Gradle properties (see kotlin.yml's
    // ORG_GRADLE_PROJECT_signingInMemoryKey* env vars).
    signAllPublications()

    pom {
        name.set("ChordSketch")
        description.set("ChordPro file format parser and renderer")
        inceptionYear.set("2025")
        url.set("https://github.com/koedame/chordsketch")

        licenses {
            license {
                name.set("MIT")
                url.set("https://github.com/koedame/chordsketch/blob/main/LICENSE")
                distribution.set("https://github.com/koedame/chordsketch/blob/main/LICENSE")
            }
        }

        developers {
            developer {
                id.set("koedame")
                name.set("koedame")
                url.set("https://github.com/koedame")
            }
        }

        scm {
            url.set("https://github.com/koedame/chordsketch")
            connection.set("scm:git:git://github.com/koedame/chordsketch.git")
            developerConnection.set("scm:git:ssh://git@github.com/koedame/chordsketch.git")
        }
    }
}
