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
    match chromium_type_detail(parts) {
        Some(detail) => format!("{base} — {detail}"),
        // The browser/main process carries no --type= switch (see
        // RenderProcessHostImpl::AppendRendererCommandLine; absence is the signal).
        None => format!("{base} — browser"),
    }
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
    // Electron is Chromium, so the same --type= taxonomy applies. The main
    // process (no --type=) is shown as just the app name.
    match chromium_type_detail(parts) {
        Some(detail) => format!("{app_name} — {detail}"),
        None => app_name.to_string(),
    }
}

/// Maps a Chromium child's `--type=` (plus sub-flags) to a human label.
/// Returns `None` for the main/browser process, which carries no `--type=`.
///
/// Type values are authoritative per Chromium's `content_switches.cc`. The
/// switch is stamped onto the child's argv at launch and exposed via
/// `/proc/<pid>/cmdline` — see docs/chrome-processes.md.
fn chromium_type_detail(parts: &[&str]) -> Option<String> {
    let proc_type = parts.iter().find_map(|p| p.strip_prefix("--type="))?;
    let detail = match proc_type {
        "renderer" => {
            if parts.contains(&"--extension-process") {
                "renderer (extension)"
            } else {
                "renderer"
            }
        }
        "gpu-process" => "GPU",
        "utility" => extract_utility_type(parts).unwrap_or("utility"),
        "zygote" => "zygote",
        "sandbox-ipc" => "sandbox IPC", // Linux-only helper
        // Not content/ process types, but seen in the wild: crashpad-handler
        // comes from the Crashpad component, broker is the Windows sandbox.
        "crashpad-handler" => "crashpad",
        "broker" => "broker",
        other => return Some(other.to_string()),
    };
    Some(detail.to_string())
}

fn extract_utility_type(parts: &[&str]) -> Option<&'static str> {
    let sub = parts
        .iter()
        .find_map(|p| p.strip_prefix("--utility-sub-type="))?;
    Some(if sub.contains("NetworkService") {
        "Network service"
    } else if sub.contains("StorageService") {
        "Storage service"
    } else if sub.contains("AudioService") {
        "Audio service"
    } else if sub.contains("DataDecoderService") {
        "Data Decoder service"
    } else if sub.contains("VideoCapture") {
        "Video capture"
    } else {
        return None;
    })
}

#[cfg(test)]
mod tests {
    use super::friendly_name;

    // Representative cmdlines captured from a live browser. Renderer/helper
    // cmdlines are space-joined (argv rewritten post-launch); friendly_name
    // splits on whitespace, so both NUL- and space-joined inputs work.
    const CHROME: &str = "/opt/google/chrome/chrome";

    #[test]
    fn chrome_process_types() {
        assert_eq!(
            friendly_name(&format!("{CHROME} --type=renderer --renderer-client-id=44")),
            "Chrome — renderer"
        );
        assert_eq!(
            friendly_name(&format!(
                "{CHROME} --type=renderer --extension-process --renderer-client-id=7"
            )),
            "Chrome — renderer (extension)"
        );
        assert_eq!(
            friendly_name(&format!("{CHROME} --type=gpu-process --ozone-platform=wayland")),
            "Chrome — GPU"
        );
        assert_eq!(
            friendly_name(&format!("{CHROME} --type=zygote")),
            "Chrome — zygote"
        );
        assert_eq!(
            friendly_name(&format!("{CHROME} --type=sandbox-ipc")),
            "Chrome — sandbox IPC"
        );
        // Browser/main process: no --type= switch.
        assert_eq!(friendly_name(CHROME), "Chrome — browser");
    }

    #[test]
    fn chrome_utility_subtypes() {
        let case = |svc: &str| {
            friendly_name(&format!(
                "{CHROME} --type=utility --utility-sub-type={svc} --lang=en-US"
            ))
        };
        assert_eq!(case("network.mojom.NetworkService"), "Chrome — Network service");
        assert_eq!(case("storage.mojom.StorageService"), "Chrome — Storage service");
        assert_eq!(case("audio.mojom.AudioService"), "Chrome — Audio service");
        assert_eq!(
            case("data_decoder.mojom.DataDecoderService"),
            "Chrome — Data Decoder service"
        );
        // Unknown sub-type falls back to the bare "utility".
        assert_eq!(case("some.mojom.MysteryService"), "Chrome — utility");
    }

    #[test]
    fn chromium_branding() {
        assert_eq!(
            friendly_name("/usr/bin/chromium --type=gpu-process"),
            "Chromium — GPU"
        );
    }

    #[test]
    fn electron_apps_reuse_taxonomy() {
        assert_eq!(
            friendly_name("/usr/share/code/code --type=renderer"),
            "VS Code — renderer"
        );
        assert_eq!(friendly_name("/usr/bin/slack --type=gpu-process"), "Slack — GPU");
        // Electron main process → just the app name.
        assert_eq!(friendly_name("/usr/share/code/code"), "VS Code");
    }

    #[test]
    fn non_browser_passes_through() {
        assert_eq!(friendly_name("/usr/bin/htop"), "/usr/bin/htop");
    }
}