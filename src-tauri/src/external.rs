//! Opening things outside the application: web pages, an editor, a terminal. How each is started
//! on each system is in [`crate::platform`]; this module checks what is asked.

use std::path::{Component, Path, PathBuf};

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
    crate::platform::launch(&crate::platform::open_url(
        crate::platform::Os::current(),
        url,
    ))
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
