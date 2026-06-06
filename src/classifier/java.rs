use std::path::Path;

use super::{Classifier, Platform};

// A JVM app is one process with many threads, so it isn't groupable; we only
// give it a friendlier name (main class or jar → known tool).

pub(super) struct JavaClassifier;

impl Classifier for JavaClassifier {
    fn matches(&self, exe: &str, _argv: &[&str]) -> bool {
        matches!(exe, "java" | "javaw" | "java.exe")
    }
    fn platform(&self) -> Platform {
        Platform::Java
    }
    fn label(&self, _exe: &str, argv: &[&str]) -> String {
        if let Some(main_class) = Self::main_class(argv) {
            return Self::known_app(&main_class)
                .unwrap_or_else(|| format!("java: {}", Self::shorten_class(&main_class)));
        }
        if let Some(jar) = Self::jar(argv) {
            return Self::known_jar(jar)
                .unwrap_or_else(|| format!("java: {}", Self::jar_basename(jar)));
        }
        "java".to_string()
    }
}

impl JavaClassifier {
    fn main_class(argv: &[&str]) -> Option<String> {
        let mut skip_next = false;
        let mut past_options = false;
        for &part in argv.iter().skip(1) {
            if skip_next {
                skip_next = false;
                continue;
            }
            if part == "-cp" || part == "-classpath" || part == "--module-path" || part == "-p" {
                skip_next = true;
                continue;
            }
            if part.starts_with('-') {
                if part.starts_with("-D") || part.starts_with("-X") || part.starts_with("-javaagent")
                {
                    continue;
                }
                if part == "-jar" {
                    return None;
                }
                continue;
            }
            if !past_options {
                past_options = true;
                if !part.ends_with(".jar") && !part.starts_with('/') {
                    return Some(part.to_string());
                }
            }
        }
        None
    }

    fn jar<'a>(argv: &[&'a str]) -> Option<&'a str> {
        let mut next_is_jar = false;
        for &part in argv.iter().skip(1) {
            if next_is_jar {
                return Some(part);
            }
            if part == "-jar" {
                next_is_jar = true;
            }
        }
        None
    }

    fn known_app(class: &str) -> Option<String> {
        let mappings: &[(&[&str], &str)] = &[
            (&["sbt.boot.Boot", "xsbt.boot.Boot", "sbt.xMain"], "sbt"),
            (&["bloop.Server", "bloop.Bloop"], "bloop"),
            (&["org.jetbrains.idea.Main", "com.intellij.idea.Main"], "IntelliJ IDEA"),
            (&["org.gradle.launcher.daemon.bootstrap.GradleDaemon", "org.gradle.launcher.GradleMain"], "Gradle"),
            (&["org.apache.maven.wrapper.MavenWrapperMain", "org.codehaus.plexus.classworlds.launcher.Launcher"], "Maven"),
            (&["scala.tools.nsc.MainGenericRunner", "dotty.tools.dotc.Main"], "scalac"),
            (&["org.apache.spark.deploy.SparkSubmit"], "Spark"),
            (&["org.apache.kafka.connect.cli.ConnectDistributed", "kafka.Kafka"], "Kafka"),
            (&["org.elasticsearch.bootstrap.Elasticsearch"], "Elasticsearch"),
            (&["org.apache.zookeeper.server.quorum.QuorumPeerMain"], "ZooKeeper"),
            (&["io.confluent.kafka.schemaregistry.rest.SchemaRegistryMain"], "Schema Registry"),
            (&["coursier.bootstrap.launcher.Launcher", "coursier.cli.Coursier"], "Coursier"),
            (&["metals.Main", "scala.meta.metals.Main"], "Metals (LSP)"),
        ];
        for (patterns, name) in mappings {
            if patterns.iter().any(|&p| class.contains(p)) {
                return Some(name.to_string());
            }
        }
        None
    }

    fn known_jar(jar: &str) -> Option<String> {
        let basename = Self::jar_basename(jar).to_lowercase();
        let mappings: &[(&[&str], &str)] = &[
            (&["sbt-launch", "sbt.jar"], "sbt"),
            (&["bloop"], "bloop"),
            (&["gradle-launcher"], "Gradle"),
            (&["metals"], "Metals (LSP)"),
            (&["coursier"], "Coursier"),
        ];
        for (patterns, name) in mappings {
            if patterns.iter().any(|&p| basename.contains(p)) {
                return Some(name.to_string());
            }
        }
        None
    }

    fn shorten_class(class: &str) -> String {
        class.rsplit('.').next().unwrap_or(class).to_string()
    }

    fn jar_basename(jar: &str) -> &str {
        Path::new(jar)
            .file_name()
            .map(|s| s.to_str().unwrap_or(jar))
            .unwrap_or(jar)
    }
}