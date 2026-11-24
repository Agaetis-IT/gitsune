//! Opening things outside the application: web pages, an editor, a terminal.

use std::path::{Component, Path, PathBuf};
use std::process::Command;

/// An address the application may open: `https` only, without spaces or control characters, so a
/// crafted value cannot pass an option to `open` or reach a local file or another program.
pub fn check_web_url(url: &str) -> Result<(), String> {
    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| "only https addresses are opened".to_string())?;
    if rest.is_empty()
        || rest.starts_with('/')
        || url.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err("the address is not valid".to_string());
    }
    Ok(())
}

/// The macOS application behind a known editor or terminal id.
pub fn app_name(kind: &str, id: &str) -> Result<&'static str, String> {
    let name = match (kind, id) {
        ("editor", "zed") => "Zed",
        ("editor", "vscode") => "Visual Studio Code",
        ("editor", "cursor") => "Cursor",
        ("terminal", "terminal") => "Terminal",
        ("terminal", "iterm") => "iTerm",
        ("terminal", "ghostty") => "Ghostty",
        _ => return Err(format!("unknown {kind}: {id}")),
    };
    Ok(name)
}

/// A path inside the working directory: relative, without `..`, and existing (what is opened
/// must be there).
pub fn inside_workdir(workdir: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    let safe = path
        .components()
        .all(|part| matches!(part, Component::Normal(_) | Component::CurDir));
    if relative.is_empty() || path.is_absolute() || !safe {
        return Err("the path is not inside the repository".to_string());
    }
    let full = workdir.join(path);
    if !full.exists() {
        return Err(format!("{relative} does not exist"));
    }
    Ok(full)
}

pub fn open_url(url: &str) -> Result<(), String> {
    check_web_url(url)?;
    Command::new("open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Opens `path` with an application (`open -a`).
pub fn open_with(app: &str, path: &Path) -> Result<(), String> {
    Command::new("open")
        .arg("-a")
        .arg(app)
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("could not open {app}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_https_addresses_are_opened() {
        assert!(check_web_url("https://github.com/a/b/compare/x?expand=1").is_ok());
        for bad in [
            "http://github.com",
            "file:///etc/passwd",
            "-a Calculator",
            "https://",
            "https:///x",
            "https://a b.com",
            "https://a.com/\n",
            "javascript:alert(1)",
        ] {
            assert!(check_web_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn only_known_apps_are_launched() {
        assert_eq!(app_name("editor", "zed").unwrap(), "Zed");
        assert_eq!(app_name("terminal", "iterm").unwrap(), "iTerm");
        assert!(app_name("editor", "iterm").is_err());
        assert!(app_name("editor", "rm -rf").is_err());
    }

    #[test]
    fn paths_stay_inside_the_repository() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/a.rs"), "").unwrap();
        assert!(inside_workdir(dir.path(), "src/a.rs").is_ok());
        assert!(inside_workdir(dir.path(), "./src").is_ok());
        for bad in ["", "../x", "src/../../x", "/etc/passwd", "missing.txt"] {
            assert!(inside_workdir(dir.path(), bad).is_err(), "{bad:?}");
        }
    }
}
