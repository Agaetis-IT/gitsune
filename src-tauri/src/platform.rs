//! What differs between macOS, Linux and Windows, as plain functions of the system: how to open
//! an address, a folder, an editor or a terminal, where tools live, how a path is written for a
//! shell. The functions take the system as an argument, so the three cases are tested on any
//! machine; only [`Os::current`] and [`launch`] look at the machine they run on.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

/// `CREATE_NO_WINDOW` of the Windows process creation flags.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Keeps a console program that the application starts for its own needs (git, ssh, `gh`,
/// `az`...) from opening a console window. The application has no console, so Windows would
/// otherwise show a new one, for a moment, each time. Nothing changes on the other systems.
/// Not for a program the user asked for, such as a terminal: that one is meant to be seen.
pub trait NoWindow {
    fn no_window(&mut self) -> &mut Self;
}

impl NoWindow for Command {
    fn no_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            self.creation_flags(CREATE_NO_WINDOW);
        }
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Mac,
    Linux,
    Windows,
}

impl Os {
    pub fn current() -> Os {
        if cfg!(target_os = "macos") {
            Os::Mac
        } else if cfg!(target_os = "windows") {
            Os::Windows
        } else {
            Os::Linux
        }
    }

    fn name(self) -> &'static str {
        match self {
            Os::Mac => "macos",
            Os::Linux => "linux",
            Os::Windows => "windows",
        }
    }

    /// Separator of the entries of `PATH`.
    pub fn path_separator(self) -> char {
        if self == Os::Windows { ';' } else { ':' }
    }

    /// What the program that shows folders is called.
    fn file_manager(self) -> &'static str {
        match self {
            Os::Mac => "Finder",
            Os::Linux => "the file manager",
            Os::Windows => "Explorer",
        }
    }
}

/// A program to start, with its arguments and, when it matters, the folder to start it in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
}

impl Launch {
    fn new(program: &str, args: &[&str]) -> Launch {
        Launch {
            program: program.to_string(),
            args: args.iter().map(|a| (*a).to_string()).collect(),
            cwd: None,
        }
    }

    fn with_path(mut self, path: &Path) -> Launch {
        self.args.push(path.to_string_lossy().into_owned());
        self
    }

    fn in_dir(mut self, dir: &Path) -> Launch {
        self.cwd = Some(dir.to_path_buf());
        self
    }
}

/// Starts the program and does not wait for it.
pub fn launch(launch: &Launch) -> Result<(), String> {
    let mut command = Command::new(&launch.program);
    command.args(&launch.args);
    if let Some(dir) = &launch.cwd {
        command.current_dir(dir);
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("could not start {}: {error}", launch.program))
}

/// Opens an address (already checked to be a plain `https` one) in the browser.
pub fn open_url(os: Os, url: &str) -> Launch {
    match os {
        Os::Mac => Launch::new("open", &[url]),
        Os::Linux => Launch::new("xdg-open", &[url]),
        // `start` would read the address as a command line; this entry point does not.
        Os::Windows => Launch::new("rundll32", &["url.dll,FileProtocolHandler", url]),
    }
}

/// Shows a folder in the file manager.
pub fn reveal_folder(os: Os, path: &Path) -> Launch {
    match os {
        Os::Mac => Launch::new("open", &["-R"]).with_path(path),
        Os::Linux => Launch::new("xdg-open", &[]).with_path(path),
        Os::Windows => Launch::new("explorer", &[]).with_path(path),
    }
}

/// An editor or a terminal the user can pick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppChoice {
    pub id: &'static str,
    pub label: &'static str,
}

const fn choice(id: &'static str, label: &'static str) -> AppChoice {
    AppChoice { id, label }
}

/// The editors offered: the same on every system (started by name outside macOS).
pub fn editors(_os: Os) -> Vec<AppChoice> {
    vec![
        choice("zed", "Zed"),
        choice("vscode", "Visual Studio Code"),
        choice("cursor", "Cursor"),
    ]
}

pub fn terminals(os: Os) -> Vec<AppChoice> {
    match os {
        Os::Mac => vec![
            choice("terminal", "Terminal"),
            choice("iterm", "iTerm"),
            choice("ghostty", "Ghostty"),
        ],
        Os::Linux => vec![
            choice("gnome-terminal", "GNOME Terminal"),
            choice("konsole", "Konsole"),
            choice("kitty", "kitty"),
            choice("alacritty", "Alacritty"),
            choice("xterm", "xterm"),
        ],
        Os::Windows => vec![
            choice("wt", "Windows Terminal"),
            choice("powershell", "PowerShell"),
            choice("cmd", "Command Prompt"),
        ],
    }
}

/// Opens a file or folder in an editor.
pub fn open_editor(os: Os, id: &str, path: &Path) -> Result<Launch, String> {
    let (mac_app, command) = match id {
        "zed" => ("Zed", "zed"),
        "vscode" => ("Visual Studio Code", "code"),
        "cursor" => ("Cursor", "cursor"),
        _ => return Err(format!("unknown editor: {id}")),
    };
    Ok(match os {
        Os::Mac => Launch::new("open", &["-a", mac_app]).with_path(path),
        Os::Linux => Launch::new(command, &[]).with_path(path),
        // `code` and `cursor` are batch files there, which only a shell starts.
        Os::Windows => Launch::new("cmd", &["/C", command]).with_path(path),
    })
}

/// Opens a terminal in a folder.
pub fn open_terminal(os: Os, id: &str, dir: &Path) -> Result<Launch, String> {
    if !terminals(os).iter().any(|t| t.id == id) {
        return Err(format!("unknown terminal: {id}"));
    }
    Ok(match (os, id) {
        (Os::Mac, "terminal") => Launch::new("open", &["-a", "Terminal"]).with_path(dir),
        (Os::Mac, "iterm") => Launch::new("open", &["-a", "iTerm"]).with_path(dir),
        (Os::Mac, _) => Launch::new("open", &["-a", "Ghostty"]).with_path(dir),
        (Os::Linux, "gnome-terminal") => Launch::new(
            "gnome-terminal",
            &[&format!("--working-directory={}", dir.to_string_lossy())],
        ),
        (Os::Linux, "konsole") => Launch::new("konsole", &["--workdir"]).with_path(dir),
        (Os::Linux, "kitty") => Launch::new("kitty", &["--directory"]).with_path(dir),
        (Os::Linux, "alacritty") => {
            Launch::new("alacritty", &["--working-directory"]).with_path(dir)
        }
        (Os::Linux, _) => Launch::new("xterm", &[]).in_dir(dir),
        (Os::Windows, "wt") => Launch::new("wt", &["-d"]).with_path(dir),
        (Os::Windows, "powershell") => {
            Launch::new("cmd", &["/C", "start", "powershell", "-NoExit"]).in_dir(dir)
        }
        (Os::Windows, _) => Launch::new("cmd", &["/C", "start", "cmd"]).in_dir(dir),
    })
}

/// The home folder of the user.
pub fn home_dir() -> String {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default()
}

/// Folders that tools are often installed in without being on the `PATH` of an application that
/// was not started from a shell (the Finder, a desktop launcher, the Start menu).
pub fn common_tool_dirs(os: Os, home: &str) -> Vec<String> {
    let (system, under_home, sep): (&[&str], &[&str], char) = match os {
        Os::Mac => (
            &[
                "/opt/homebrew/bin",
                "/opt/homebrew/sbin",
                "/usr/local/bin",
                "/usr/local/sbin",
            ],
            &[
                ".bun/bin",
                ".volta/bin",
                ".cargo/bin",
                ".deno/bin",
                ".local/bin",
                "Library/pnpm",
            ],
            '/',
        ),
        Os::Linux => (
            &[
                "/usr/local/bin",
                "/usr/local/sbin",
                "/snap/bin",
                "/home/linuxbrew/.linuxbrew/bin",
            ],
            &[
                ".bun/bin",
                ".volta/bin",
                ".cargo/bin",
                ".deno/bin",
                ".local/bin",
                ".local/share/pnpm",
            ],
            '/',
        ),
        Os::Windows => (
            &[
                "C:\\Program Files\\Git\\cmd",
                "C:\\Program Files\\GitHub CLI",
                "C:\\Program Files\\Microsoft SDKs\\Azure\\CLI2\\wbin",
            ],
            &[
                ".bun\\bin",
                ".cargo\\bin",
                ".volta\\bin",
                "AppData\\Local\\Programs\\Microsoft VS Code\\bin",
            ],
            '\\',
        ),
    };
    let home = home.trim_end_matches(['/', '\\']);
    system
        .iter()
        .map(|dir| (*dir).to_string())
        .chain(
            under_home
                .iter()
                .filter(|_| !home.is_empty())
                .map(|dir| format!("{home}{sep}{dir}")),
        )
        .collect()
}

/// `path` with the usual tool folders added at the end when they are missing.
pub fn extend_path(os: Os, home: &str, path: &str) -> String {
    let sep = os.path_separator();
    let mut parts: Vec<String> = path
        .split(sep)
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect();
    for dir in common_tool_dirs(os, home) {
        if !parts.contains(&dir) {
            parts.push(dir);
        }
    }
    parts.join(&sep.to_string())
}

/// The names a program may have as a file: on Windows with each extension it can run with.
pub fn executable_names(os: Os, name: &str) -> Vec<String> {
    match os {
        Os::Windows => ["exe", "cmd", "bat", "com"]
            .iter()
            .map(|ext| format!("{name}.{ext}"))
            .collect(),
        _ => vec![name.to_string()],
    }
}

/// A path as a shell (git runs scripts through `sh`, which on Windows is the one of Git for
/// Windows) reads it: with slashes.
pub fn shell_path(os: Os, path: &str) -> String {
    if os == Os::Windows {
        path.replace('\\', "/")
    } else {
        path.to_string()
    }
}

/// What the interface needs to know about the system it runs on.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    pub os: &'static str,
    pub editors: Vec<AppChoice>,
    pub terminals: Vec<AppChoice>,
    /// "Finder", "Explorer" or "the file manager".
    pub file_manager: &'static str,
    /// The system has a native menu bar (the others use the command palette).
    pub native_menu: bool,
}

pub fn info(os: Os) -> PlatformInfo {
    PlatformInfo {
        os: os.name(),
        editors: editors(os),
        terminals: terminals(os),
        file_manager: os.file_manager(),
        native_menu: os == Os::Mac,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(launch: &Launch) -> Vec<&str> {
        launch.args.iter().map(String::as_str).collect()
    }

    #[test]
    fn an_address_is_opened_the_way_each_system_does() {
        let url = "https://example.invalid/a?b=c&d=e";
        let mac = open_url(Os::Mac, url);
        assert_eq!((mac.program.as_str(), args(&mac)), ("open", vec![url]));
        let linux = open_url(Os::Linux, url);
        assert_eq!(
            (linux.program.as_str(), args(&linux)),
            ("xdg-open", vec![url])
        );
        let windows = open_url(Os::Windows, url);
        assert_eq!(windows.program, "rundll32");
        assert_eq!(args(&windows), vec!["url.dll,FileProtocolHandler", url]);
    }

    #[test]
    fn a_folder_is_shown_in_the_file_manager() {
        let dir = Path::new("/work/app");
        assert_eq!(args(&reveal_folder(Os::Mac, dir)), vec!["-R", "/work/app"]);
        assert_eq!(reveal_folder(Os::Linux, dir).program, "xdg-open");
        assert_eq!(reveal_folder(Os::Windows, dir).program, "explorer");
    }

    #[test]
    fn an_editor_is_started_by_application_on_a_mac_and_by_name_elsewhere() {
        let file = Path::new("/work/app/a.rs");
        let mac = open_editor(Os::Mac, "vscode", file).unwrap();
        assert_eq!(mac.program, "open");
        assert_eq!(
            args(&mac),
            vec!["-a", "Visual Studio Code", "/work/app/a.rs"]
        );
        let linux = open_editor(Os::Linux, "vscode", file).unwrap();
        assert_eq!(
            (linux.program.as_str(), args(&linux)),
            ("code", vec!["/work/app/a.rs"])
        );
        let windows = open_editor(Os::Windows, "cursor", file).unwrap();
        assert_eq!(windows.program, "cmd");
        assert_eq!(args(&windows), vec!["/C", "cursor", "/work/app/a.rs"]);
        assert!(open_editor(Os::Linux, "rm -rf", file).is_err());
        assert!(open_editor(Os::Mac, "iterm", file).is_err());
    }

    #[test]
    fn every_terminal_of_every_system_opens_in_the_folder() {
        let dir = Path::new("/work/app");
        for os in [Os::Mac, Os::Linux, Os::Windows] {
            for terminal in terminals(os) {
                let launch = open_terminal(os, terminal.id, dir).unwrap();
                let in_args = launch.args.iter().any(|a| a.contains("/work/app"));
                assert!(
                    in_args || launch.cwd.as_deref() == Some(dir),
                    "{os:?} {}: the folder is not given",
                    terminal.id
                );
            }
        }
        assert!(
            open_terminal(Os::Linux, "terminal", dir).is_err(),
            "a Mac terminal"
        );
        assert!(open_terminal(Os::Windows, "xterm", dir).is_err());
        let gnome = open_terminal(Os::Linux, "gnome-terminal", dir).unwrap();
        assert_eq!(args(&gnome), vec!["--working-directory=/work/app"]);
        let wt = open_terminal(Os::Windows, "wt", dir).unwrap();
        assert_eq!(args(&wt), vec!["-d", "/work/app"]);
    }

    #[test]
    fn the_usual_tool_folders_depend_on_the_system() {
        let mac = common_tool_dirs(Os::Mac, "/Users/fakename");
        assert!(mac.contains(&"/opt/homebrew/bin".to_string()));
        assert!(mac.contains(&"/Users/fakename/Library/pnpm".to_string()));
        let linux = common_tool_dirs(Os::Linux, "/home/fakename/");
        assert!(linux.contains(&"/snap/bin".to_string()));
        assert!(linux.contains(&"/home/fakename/.cargo/bin".to_string()));
        assert!(
            !linux
                .iter()
                .any(|d| d.contains("homebrew/bin") && d.starts_with("/opt"))
        );
        let windows = common_tool_dirs(Os::Windows, "C:\\Users\\fakename");
        assert!(windows.contains(&"C:\\Program Files\\Git\\cmd".to_string()));
        assert!(windows.contains(&"C:\\Users\\fakename\\.cargo\\bin".to_string()));
        // Without a home folder only the system folders remain.
        assert!(
            common_tool_dirs(Os::Linux, "")
                .iter()
                .all(|d| d.starts_with('/') && !d.contains("//"))
        );
    }

    #[test]
    fn the_path_is_extended_without_repeats_and_with_the_right_separator() {
        let extended = extend_path(Os::Linux, "/home/x", "/usr/bin:/bin");
        assert!(extended.starts_with("/usr/bin:/bin:"));
        assert_eq!(extend_path(Os::Linux, "/home/x", &extended), extended);
        let windows = extend_path(
            Os::Windows,
            "C:\\Users\\x",
            "C:\\Windows\\System32;C:\\Tools",
        );
        assert!(windows.starts_with("C:\\Windows\\System32;C:\\Tools;"));
        assert!(windows.contains(";C:\\Program Files\\Git\\cmd"));
        assert!(!windows.contains(':') || windows.matches(';').count() > 2);
    }

    #[test]
    fn programs_have_the_names_each_system_gives_them() {
        assert_eq!(executable_names(Os::Linux, "git"), vec!["git"]);
        assert_eq!(
            executable_names(Os::Windows, "git"),
            vec!["git.exe", "git.cmd", "git.bat", "git.com"]
        );
    }

    #[test]
    fn a_windows_path_is_given_to_a_shell_with_slashes() {
        assert_eq!(
            shell_path(Os::Windows, "C:\\Users\\x\\a b\\s.sh"),
            "C:/Users/x/a b/s.sh"
        );
        assert_eq!(shell_path(Os::Linux, "/tmp/a\\b"), "/tmp/a\\b");
    }

    #[test]
    fn the_interface_is_told_what_the_system_offers() {
        let mac = info(Os::Mac);
        assert!(mac.native_menu);
        assert_eq!(mac.file_manager, "Finder");
        let windows = info(Os::Windows);
        assert!(!windows.native_menu);
        assert_eq!(windows.terminals[0].id, "wt");
        assert_eq!(info(Os::Linux).os, "linux");
    }
}
