use std::path::Path;

pub fn icon_for(exe: &str) -> &'static str {
    let lower = exe.to_ascii_lowercase();
    let l = lower.as_str();
    if l.contains("java") || l.contains("sbt") || l.contains("gradle") || l.contains("maven")
        || l.contains("bloop") || l.contains("metals") || l.contains("scala")
        || l.contains("kotlin") || l.contains("spark") || l.contains("kafka")
        || l.contains("elasticsearch") || l.contains("zookeeper")
    {
        return "☕";
    }
    if l.contains("python") || l == "python3" || l == "python2" || l == "pip" || l == "ipython" {
        return "🐍";
    }
    if l.contains("node") || l.contains("npm") || l.contains("deno") || l.contains("bun") {
        return "🟢";
    }
    if l.contains("rust") || l.contains("cargo") || l.contains("rustc") {
        return "🦀";
    }
    if l.contains("chrome") || l.contains("chromium") {
        return "🌐";
    }
    if l.contains("firefox") {
        return "🦊";
    }
    if l == "code" || l == "codium" || l.contains("vs code") || l == "cursor" {
        return "💻";
    }
    if l.contains("docker") || l.contains("containerd") || l.contains("podman") {
        return "🐳";
    }
    if l.contains("go") && (l == "go" || l.contains("go build") || l.contains("gopls")) {
        return "🔵";
    }
    if l.contains("ruby") || l.contains("irb") || l.contains("rails") || l.contains("bundle") {
        return "💎";
    }
    if l.contains("slack") {
        return "💬";
    }
    if l.contains("discord") {
        return "🎮";
    }
    if l.contains("spotify") {
        return "🎵";
    }
    "  "
}

pub fn friendly_name(raw_cmd: &str) -> String {
    let parts: Vec<&str> = raw_cmd.split_whitespace().collect();
    if parts.is_empty() {
        return raw_cmd.to_string();
    }

    let exe = Path::new(parts[0])
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| parts[0].to_string());

    if is_java_exe(&exe) {
        return identify_java(&parts);
    }

    if is_chromium_exe(&exe) {
        return identify_chromium(&exe, &parts);
    }

    if is_electron_app(&exe, &parts) {
        return identify_electron(&exe, &parts);
    }

    raw_cmd.to_string()
}

fn is_java_exe(exe: &str) -> bool {
    matches!(exe, "java" | "javaw" | "java.exe")
}

fn is_chromium_exe(exe: &str) -> bool {
    matches!(
        exe,
        "chrome" | "chromium" | "chromium-browser" | "google-chrome" | "google-chrome-stable"
    )
}

fn is_electron_app(exe: &str, parts: &[&str]) -> bool {
    let electron_indicators = ["electron", "--ms-enable-electron", "--type="];
    let known_electron_apps = [
        "code", "codium", "slack", "discord", "signal-desktop", "obsidian",
        "spotify", "teams", "cursor",
    ];
    if known_electron_apps.contains(&exe) {
        return true;
    }
    parts
        .iter()
        .any(|p| electron_indicators.iter().any(|ind| p.contains(ind)))
        && !is_chromium_exe(exe)
}

fn identify_java(parts: &[&str]) -> String {
    if let Some(main_class) = find_java_main_class(parts) {
        let friendly = match_known_java_app(&main_class);
        return friendly.unwrap_or_else(|| format!("java: {}", shorten_class(&main_class)));
    }

    if let Some(jar) = find_java_jar(parts) {
        let friendly = match_known_java_jar(jar);
        return friendly.unwrap_or_else(|| format!("java: {}", jar_basename(jar)));
    }

    "java".to_string()
}

fn find_java_main_class(parts: &[&str]) -> Option<String> {
    let mut skip_next = false;
    let mut past_options = false;
    for &part in parts.iter().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if part == "-cp" || part == "-classpath" || part == "--module-path" || part == "-p" {
            skip_next = true;
            continue;
        }
        if part.starts_with('-') {
            if part.starts_with("-D") || part.starts_with("-X") || part.starts_with("-javaagent") {
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

fn find_java_jar<'a>(parts: &[&'a str]) -> Option<&'a str> {
    let mut next_is_jar = false;
    for &part in parts.iter().skip(1) {
        if next_is_jar {
            return Some(part);
        }
        if part == "-jar" {
            next_is_jar = true;
        }
    }
    None
}

fn match_known_java_app(class: &str) -> Option<String> {
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

fn match_known_java_jar(jar: &str) -> Option<String> {
    let basename = jar_basename(jar).to_lowercase();
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
    if let Some(last) = class.rsplit('.').next() {
        last.to_string()
    } else {
        class.to_string()
    }
}

fn jar_basename(jar: &str) -> &str {
    Path::new(jar)
        .file_name()
        .map(|s| s.to_str().unwrap_or(jar))
        .unwrap_or(jar)
}

fn identify_chromium(exe: &str, parts: &[&str]) -> String {
    let base = match exe {
        "chromium" | "chromium-browser" => "Chromium",
        _ => "Chrome",
    };

    let proc_type = parts
        .iter()
        .find_map(|p| p.strip_prefix("--type="))
        .unwrap_or("main");

    let detail = match proc_type {
        "gpu-process" => "GPU",
        "renderer" => "renderer",
        "utility" => extract_utility_type(parts).unwrap_or("utility"),
        "zygote" => "zygote",
        "broker" => "broker",
        "crashpad-handler" => "crashpad",
        "main" => "main",
        other => other,
    };

    format!("{base} ({detail})")
}

fn identify_electron(exe: &str, parts: &[&str]) -> String {
    let app_name = match exe {
        "code" | "codium" => "VS Code",
        "slack" => "Slack",
        "discord" | "Discord" => "Discord",
        "signal-desktop" => "Signal",
        "obsidian" => "Obsidian",
        "spotify" => "Spotify",
        "teams" => "Teams",
        "cursor" => "Cursor",
        _ => exe,
    };

    let proc_type = parts
        .iter()
        .find_map(|p| p.strip_prefix("--type="));

    match proc_type {
        Some("gpu-process") => format!("{app_name} (GPU)"),
        Some("renderer") => format!("{app_name} (renderer)"),
        Some("utility") => format!("{app_name} (utility)"),
        Some(other) => format!("{app_name} ({other})"),
        None => app_name.to_string(),
    }
}

fn extract_utility_type(parts: &[&str]) -> Option<&'static str> {
    for part in parts {
        if let Some(sub) = part.strip_prefix("--utility-sub-type=") {
            if sub.contains("NetworkService") {
                return Some("network");
            }
            if sub.contains("StorageService") {
                return Some("storage");
            }
            if sub.contains("AudioService") {
                return Some("audio");
            }
            if sub.contains("VideoCapture") {
                return Some("video");
            }
        }
    }
    None
}