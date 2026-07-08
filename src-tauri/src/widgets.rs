//! Widget plugin system (jarvisos-app#18): manifest format + local registry.
//!
//! A widget is a directory containing manifest.json plus its own
//! index.html/CSS/JS -- consistent with this app's own "no framework, no
//! bundler" approach. Widgets are discovered from two places: bundled
//! defaults shipped inside the app (`resources/widgets/`) and user-installed
//! ones under `app_data_dir()/widgets/`. Both are scanned the same way.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

fn default_trust_status() -> String {
    "unreviewed".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WidgetManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon: Option<String>,
    pub entry: String,
    #[serde(default = "default_trust_status")]
    pub trust_status: String,
    #[serde(default)]
    pub subscribed_events: Vec<String>,
    /// "bundled" or "installed" -- not part of manifest.json itself, filled
    /// in by the scanner based on which directory it came from.
    #[serde(default)]
    pub source: String,
    /// Resolved absolute directory the manifest was loaded from -- not part
    /// of manifest.json itself, needed to serve the widget's own files.
    /// Deliberately not serialized to the frontend: it's a local filesystem
    /// path with no use there (window creation is Rust-side only), and
    /// there's no reason to expose it.
    #[serde(default, skip_serializing)]
    pub dir: PathBuf,
}

/// Loads and validates one widget's manifest.json. None on any parse
/// failure or missing required field (id/name/entry) -- a malformed widget
/// is silently skipped rather than crashing widget discovery for every
/// other widget.
fn load_manifest_from_dir(dir: &Path, source: &str) -> Option<WidgetManifest> {
    let text = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
    let mut manifest: WidgetManifest = serde_json::from_str(&text).ok()?;
    if manifest.id.trim().is_empty()
        || manifest.name.trim().is_empty()
        || manifest.entry.trim().is_empty()
    {
        return None;
    }
    manifest.source = source.to_string();
    manifest.dir = dir.to_path_buf();
    Some(manifest)
}

fn scan_widgets_dir(dir: &Path, source: &str) -> Vec<WidgetManifest> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut widgets: Vec<WidgetManifest> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter_map(|path| load_manifest_from_dir(&path, source))
        .collect();
    widgets.sort_by(|a, b| a.id.cmp(&b.id));
    widgets
}

/// Discovers every widget across both scopes. Bundled widgets are listed
/// first, then installed ones; a duplicate id from `installed` after a
/// `bundled` one is kept as a separate entry deliberately -- deduping by id
/// isn't this function's job, since "which one wins" is a product decision
/// for the settings UI, not a discovery-time one.
pub fn discover_widgets(
    bundled_dir: Option<&Path>,
    installed_dir: Option<&Path>,
) -> Vec<WidgetManifest> {
    let mut widgets = Vec::new();
    if let Some(dir) = bundled_dir {
        widgets.extend(scan_widgets_dir(dir, "bundled"));
    }
    if let Some(dir) = installed_dir {
        widgets.extend(scan_widgets_dir(dir, "installed"));
    }
    widgets
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "jarvis-ui-widget-test-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_widget(base: &Path, id: &str, manifest_json: &str) {
        let dir = base.join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("manifest.json"), manifest_json).unwrap();
    }

    #[test]
    fn discovers_a_valid_widget() {
        let dir = temp_dir("valid");
        write_widget(
            &dir,
            "wake-word-orb",
            r#"{"id":"wake-word-orb","name":"Wake Word Indicator","description":"desc","entry":"index.html","trustStatus":"verified","subscribedEvents":["ipc-state"]}"#,
        );

        let widgets = discover_widgets(Some(&dir), None);

        assert_eq!(widgets.len(), 1);
        assert_eq!(widgets[0].id, "wake-word-orb");
        assert_eq!(widgets[0].name, "Wake Word Indicator");
        assert_eq!(widgets[0].trust_status, "verified");
        assert_eq!(widgets[0].subscribed_events, vec!["ipc-state".to_string()]);
        assert_eq!(widgets[0].source, "bundled");
        assert_eq!(widgets[0].dir, dir.join("wake-word-orb"));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn defaults_trust_status_when_omitted() {
        let dir = temp_dir("default-trust");
        write_widget(
            &dir,
            "plain",
            r#"{"id":"plain","name":"Plain","entry":"index.html"}"#,
        );

        let widgets = discover_widgets(Some(&dir), None);

        assert_eq!(widgets.len(), 1);
        assert_eq!(widgets[0].trust_status, "unreviewed");
        assert_eq!(widgets[0].description, "");
        assert!(widgets[0].subscribed_events.is_empty());

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn skips_a_widget_missing_required_fields() {
        let dir = temp_dir("missing-fields");
        write_widget(&dir, "broken", r#"{"id":"broken","name":"Broken"}"#); // no entry
        write_widget(
            &dir,
            "ok",
            r#"{"id":"ok","name":"OK","entry":"index.html"}"#,
        );

        let widgets = discover_widgets(Some(&dir), None);

        assert_eq!(widgets.len(), 1);
        assert_eq!(widgets[0].id, "ok");

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn skips_a_directory_with_invalid_json() {
        let dir = temp_dir("invalid-json");
        let widget_dir = dir.join("bad");
        fs::create_dir_all(&widget_dir).unwrap();
        fs::write(widget_dir.join("manifest.json"), "{ not json").unwrap();

        assert_eq!(discover_widgets(Some(&dir), None), Vec::new());

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ignores_a_directory_with_no_manifest() {
        let dir = temp_dir("no-manifest");
        fs::create_dir_all(dir.join("not-a-widget")).unwrap();

        assert_eq!(discover_widgets(Some(&dir), None), Vec::new());

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn combines_bundled_and_installed_scopes() {
        let bundled = temp_dir("bundled-scope");
        let installed = temp_dir("installed-scope");
        write_widget(
            &bundled,
            "wake-word-orb",
            r#"{"id":"wake-word-orb","name":"Wake Word","entry":"index.html"}"#,
        );
        write_widget(
            &installed,
            "custom-widget",
            r#"{"id":"custom-widget","name":"Custom","entry":"index.html"}"#,
        );

        let widgets = discover_widgets(Some(&bundled), Some(&installed));

        assert_eq!(widgets.len(), 2);
        assert!(widgets
            .iter()
            .any(|w| w.id == "wake-word-orb" && w.source == "bundled"));
        assert!(widgets
            .iter()
            .any(|w| w.id == "custom-widget" && w.source == "installed"));

        fs::remove_dir_all(&bundled).unwrap();
        fs::remove_dir_all(&installed).unwrap();
    }

    #[test]
    fn missing_directories_return_empty_not_error() {
        let nonexistent = std::env::temp_dir().join("jarvis-ui-widget-test-does-not-exist");
        assert_eq!(discover_widgets(Some(&nonexistent), None), Vec::new());
        assert_eq!(discover_widgets(None, None), Vec::new());
    }
}
