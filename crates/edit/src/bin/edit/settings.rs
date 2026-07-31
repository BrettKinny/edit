use std::ops::Range;
use std::path::PathBuf;

use edit::buffer::TextBuffer;
use edit::cell::{Ref, SemiRefCell};
use edit::json;
use edit::lsh::{LANGUAGES, Language};
use stdext::arena::{read_to_string, scratch_arena};
use stdext::arena_format;

use crate::apperr;

/// Controls whether the menubar occupies the top row of the screen.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum MenuBarVisibility {
    /// The menubar is always visible.
    #[default]
    Classic,
    /// The menubar is hidden, but Alt+<accelerator> and F10 bring it back
    /// until it loses focus again.
    Toggle,
    /// The menubar is never visible.
    Hidden,
}

impl MenuBarVisibility {
    /// The `window.menuBarVisibility` value that maps onto this variant.
    fn as_str(self) -> &'static str {
        match self {
            MenuBarVisibility::Classic => "classic",
            MenuBarVisibility::Toggle => "toggle",
            MenuBarVisibility::Hidden => "hidden",
        }
    }
}

pub struct Settings {
    pub path: PathBuf,
    pub file_associations: Vec<(String, &'static Language)>,
    pub menu_bar_visibility: MenuBarVisibility,
}

struct SettingsCell(SemiRefCell<Settings>);
unsafe impl Sync for SettingsCell {}
static SETTINGS: SettingsCell = SettingsCell(SemiRefCell::new(Settings::new()));

impl Settings {
    /// Fills the given settings.json text buffer with some initial contents for convenience.
    pub fn bootstrap(tb: &mut TextBuffer) {
        tb.set_crlf(false);
        tb.write_raw(b"{\n}\n");
        tb.cursor_move_to_logical(Default::default());
        tb.mark_as_clean();
    }

    const fn new() -> Self {
        Settings {
            path: PathBuf::new(),
            file_associations: Vec::new(),
            menu_bar_visibility: MenuBarVisibility::Classic,
        }
    }

    pub fn borrow() -> Ref<'static, Settings> {
        SETTINGS.0.borrow()
    }

    pub fn reload() -> apperr::Result<()> {
        let s = &mut *SETTINGS.0.borrow_mut();

        // Reset all members if we had been loaded previously.
        if !s.path.as_os_str().is_empty() {
            *s = Settings::new();
        }

        s.load()
    }

    /// Writes `window.menuBarVisibility` back to settings.json, so that
    /// showing or hiding the menubar survives the session.
    ///
    /// The file belongs to the user, so we replace just that one key and leave the rest
    /// of it -- other settings, key order, formatting -- byte for byte intact. Rendering
    /// our own state as a whole new document would throw all of that away.
    pub fn store_menu_bar_visibility(visibility: MenuBarVisibility) -> apperr::Result<()> {
        let s = &mut *SETTINGS.0.borrow_mut();
        s.menu_bar_visibility = visibility;

        // No config directory means there's nowhere to persist to.
        if s.path.as_os_str().is_empty() {
            return Ok(());
        }

        let scratch = scratch_arena(None);
        let existing = match read_to_string(&scratch, &s.path) {
            Ok(str) => String::from(&*str),
            // A missing settings.json is business as usual: we're the ones creating it.
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::from("{\n}\n"),
            Err(err) => return Err(err.into()),
        };

        let contents = replace_menu_bar_visibility(&existing, visibility.as_str())?;

        // The config directory may not exist yet either.
        if let Some(parent) = s.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&s.path, contents)?;
        Ok(())
    }

    fn load(&mut self) -> apperr::Result<()> {
        self.path = match settings_json_path() {
            Some(p) => p,
            None => return Ok(()),
        };

        let scratch = scratch_arena(None);
        let str = match read_to_string(&scratch, &self.path) {
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(err.into()),
            Ok(str) => str,
        };
        let Ok(json) = json::parse(&scratch, &str) else {
            return Err(apperr::Error::SettingsInvalid("Invalid JSON"));
        };
        let Some(root) = json.as_object() else {
            return Err(apperr::Error::SettingsInvalid("Non-object root"));
        };

        if let Some(f) = root.get_object("files.associations") {
            for &(mut key, ref value) in f.iter() {
                if !key.contains('/') {
                    key = arena_format!(&*scratch, "**/{key}").leak();
                }

                let Some(id) = value.as_str() else {
                    return Err(apperr::Error::SettingsInvalid("files.associations"));
                };
                let Some(language) = LANGUAGES.iter().find(|lang| lang.id == id) else {
                    return Err(apperr::Error::SettingsInvalid("language ID"));
                };

                self.file_associations.push((key.to_string(), language));
            }
        }

        if let Some(v) = root.get_str("window.menuBarVisibility") {
            self.menu_bar_visibility = match v {
                "classic" => MenuBarVisibility::Classic,
                "toggle" => MenuBarVisibility::Toggle,
                "hidden" => MenuBarVisibility::Hidden,
                _ => return Err(apperr::Error::SettingsInvalid("window.menuBarVisibility")),
            };
        }

        Ok(())
    }
}

/// Returns `json` with `window.menuBarVisibility` set to `value`, adding the key as the
/// first member of the root object if it isn't there yet.
///
/// This is a targeted text edit rather than a parse-and-reserialize, because the latter
/// would lose everything about the file we don't model. It bails instead of guessing if
/// the key is there but doesn't hold a plain string.
fn replace_menu_bar_visibility(json: &str, value: &str) -> apperr::Result<String> {
    // A plain substring search could in principle match inside some other string
    // in the file. Nothing we ship reads back what it matched, so the worst case is
    // an edit in a place the user can see and undo.
    const KEY: &str = "\"window.menuBarVisibility\"";

    if let Some(key_at) = json.find(KEY) {
        let span = string_value_span(json, key_at + KEY.len())
            .ok_or(apperr::Error::SettingsInvalid("window.menuBarVisibility"))?;
        let mut out = String::with_capacity(json.len() + value.len());
        out.push_str(&json[..span.start]);
        out.push_str(value);
        out.push_str(&json[span.end..]);
        return Ok(out);
    }

    let brace = json.find('{').ok_or(apperr::Error::SettingsInvalid("Non-object root"))?;
    let rest = &json[brace + 1..];
    let trimmed = rest.trim_start();
    let separator = if trimmed.is_empty() || trimmed.starts_with('}') { "" } else { "," };
    Ok(format!("{}\n    {KEY}: \"{value}\"{separator}{rest}", &json[..=brace]))
}

/// Returns the range of the string value that `json[from..]` assigns, quotes excluded.
/// [`None`] if what follows isn't `: "..."`.
fn string_value_span(json: &str, from: usize) -> Option<Range<usize>> {
    let rest = &json[from..];
    let colon = rest.find(':')?;
    if !rest[..colon].trim().is_empty() {
        return None;
    }

    let rest = &rest[colon + 1..];
    let open = rest.find('"')?;
    if !rest[..open].trim().is_empty() {
        return None;
    }

    let len = rest[open + 1..].find('"')?;
    let start = from + colon + 1 + open + 1;
    Some(start..start + len)
}

fn settings_json_path() -> Option<PathBuf> {
    let mut config_dir = config_dir()?;
    config_dir.push("settings.json");
    Some(config_dir)
}

fn config_dir() -> Option<PathBuf> {
    fn var_path(key: &str) -> Option<PathBuf> {
        std::env::var_os(key).map(PathBuf::from)
    }

    fn push(mut path: PathBuf, suffix: &str) -> PathBuf {
        path.push(suffix);
        path
    }

    #[cfg(target_os = "windows")]
    {
        var_path("APPDATA").map(|p| push(p, "Microsoft\\Edit"))
    }
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        var_path("HOME").map(|p| push(p, "Library/Application Support/com.microsoft.edit"))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "ios")))]
    {
        var_path("XDG_CONFIG_HOME")
            .or_else(|| var_path("HOME").map(|p| push(p, ".config")))
            .map(|p| push(p, "msedit"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replace_menu_bar_visibility_inserts() {
        assert_eq!(
            replace_menu_bar_visibility("{\n}\n", "toggle").unwrap(),
            "{\n    \"window.menuBarVisibility\": \"toggle\"\n}\n"
        );
    }

    #[test]
    fn test_replace_menu_bar_visibility_inserts_before_existing_keys() {
        assert_eq!(
            replace_menu_bar_visibility("{\n    \"files.associations\": {}\n}\n", "hidden")
                .unwrap(),
            "{\n    \"window.menuBarVisibility\": \"hidden\",\n    \"files.associations\": {}\n}\n"
        );
    }

    #[test]
    fn test_replace_menu_bar_visibility_replaces() {
        // Only the value changes: the odd spacing and the neighboring key stay put.
        assert_eq!(
            replace_menu_bar_visibility(
                "{\n  \"window.menuBarVisibility\" :  \"toggle\" ,\n  \"a\": 1\n}",
                "classic"
            )
            .unwrap(),
            "{\n  \"window.menuBarVisibility\" :  \"classic\" ,\n  \"a\": 1\n}"
        );
    }

    #[test]
    fn test_replace_menu_bar_visibility_rejects_non_string() {
        assert!(
            replace_menu_bar_visibility("{\"window.menuBarVisibility\": 1}", "toggle").is_err()
        );
        assert!(replace_menu_bar_visibility("", "toggle").is_err());
    }
}
