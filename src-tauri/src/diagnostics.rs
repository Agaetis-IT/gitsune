//! What the application can tell about its surroundings, to help with "it works in my terminal
//! but not in the app" (a missing `PATH` entry, no ssh key in the agent...).
//!
//! The report is meant to be pasted in a ticket, so it holds nothing personal: the home folder
//! is written `~`, there is no account, e-mail, address of a remote, branch or repository name,
//! and nothing is read from the network.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde::Serialize;

use crate::hosting::{host_statuses, tool_line};
use crate::platform::{Os, executable_names, home_dir};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub found: bool,
    /// Where it was found, with the home folder written `~`.
    pub path: Option<String>,
    /// Its version line, for the tools that answer quickly.
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HostLine {
    pub host: String,
    pub tool: String,
    /// `signedIn`, `signedOut` or `missing` (never the account).
    pub state: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Signing {
    /// `gpg.format` (`ssh`, `openpgp`, `x509`), unset when git uses its default.
    pub format: Option<String>,
    /// `commit.gpgsign` is on.
    pub enabled: bool,
    /// A `user.signingkey` is set (its value is never read out).
    pub has_key: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub app_version: String,
    pub os: String,
    pub arch: String,
    /// The folders the application looks for tools in, in order.
    pub path_dirs: Vec<String>,
    pub tools: Vec<ToolInfo>,
    pub hosts: Vec<HostLine>,
    /// Which agent ssh uses: `system (launchd)`, `1Password`, `named by ssh config`,
    /// `environment` or `none`.
    pub ssh_agent: String,
    /// Keys the agent holds; `None` when it cannot be asked.
    pub ssh_keys: Option<usize>,
    pub signing: Signing,
}

/// Writes the home folder as `~` in a text, so the report does not carry a user name.
pub fn anonymize(text: &str, home: &str) -> String {
    let home = home.trim_end_matches('/');
    if home.is_empty() || home == "/" {
        return text.to_string();
    }
    text.replace(home, "~")
}

/// The first executable called `name` in `dirs`.
pub fn find_tool(name: &str, dirs: &[String]) -> Option<String> {
    let names = executable_names(Os::current(), name);
    dirs.iter().find_map(|dir| {
        names.iter().find_map(|file| {
            let candidate = Path::new(dir).join(file);
            is_executable(&candidate).then(|| candidate.to_string_lossy().into_owned())
        })
    })
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// The tools the application relies on, and how to read their version (`None`: found only,
/// because asking is slow or there is no flag for it).
const TOOLS: [(&str, Option<&[&str]>); 9] = [
    ("git", Some(&["--version"])),
    ("ssh", Some(&["-V"])),
    ("ssh-keygen", None),
    ("ssh-add", None),
    ("gh", Some(&["--version"])),
    ("glab", Some(&["--version"])),
    ("az", None),
    ("zed", None),
    ("code", None),
];

fn describe_agent(env_socket: &str, named: Option<&str>) -> String {
    match named {
        Some(path) if path.to_lowercase().contains("1password") => "1Password".to_string(),
        Some(_) => "named by ssh config".to_string(),
        None if env_socket.is_empty() => "none".to_string(),
        None if env_socket.contains("com.apple.launchd") => "system (launchd)".to_string(),
        None => "environment".to_string(),
    }
}

/// The number of keys `ssh-add -l` lists (it exits with 1 for an agent without keys, 2 when it
/// cannot reach one).
fn count_keys(output_code: i32, stdout: &str) -> Option<usize> {
    match output_code {
        0 => Some(stdout.lines().filter(|l| !l.trim().is_empty()).count()),
        1 => Some(0),
        _ => None,
    }
}

fn git_config(name: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["config", "--get", name])
        .env("PATH", crate::git::tool_path())
        .stdin(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
}

fn ssh_keys(socket: &str) -> Option<usize> {
    let mut command = Command::new("ssh-add");
    command
        .arg("-l")
        .env("PATH", crate::git::tool_path())
        .stdin(Stdio::null());
    if !socket.is_empty() {
        command.env("SSH_AUTH_SOCK", socket);
    }
    let output = command.output().ok()?;
    count_keys(
        output.status.code().unwrap_or(2),
        &String::from_utf8_lossy(&output.stdout),
    )
}

/// Gathers the report. It never reaches the network and writes nothing.
pub fn collect() -> Diagnostics {
    let home = home_dir();
    let dirs: Vec<String> = crate::git::tool_path()
        .split(Os::current().path_separator())
        .filter(|d| !d.is_empty())
        .map(str::to_string)
        .collect();

    let tools = TOOLS
        .iter()
        .map(|(name, version_args)| {
            let path = find_tool(name, &dirs);
            let version = path.as_ref().and_then(|_| {
                version_args.and_then(|args| tool_line(name, args, Duration::from_secs(5)))
            });
            ToolInfo {
                name: (*name).to_string(),
                found: path.is_some(),
                path: path.map(|p| anonymize(&p, &home)),
                version: version.map(|v| anonymize(&v, &home)),
            }
        })
        .collect();

    let env_socket = std::env::var("SSH_AUTH_SOCK").unwrap_or_default();
    let named = crate::git::ssh_agent_socket();
    let socket = named.map_or(env_socket.as_str(), |path| path);

    Diagnostics {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        path_dirs: dirs.iter().map(|d| anonymize(d, &home)).collect(),
        tools,
        hosts: host_statuses()
            .into_iter()
            .map(|status| HostLine {
                host: status.host,
                tool: status.tool,
                state: status.state,
            })
            .collect(),
        ssh_agent: describe_agent(&env_socket, named),
        ssh_keys: ssh_keys(socket),
        signing: Signing {
            format: git_config("gpg.format"),
            enabled: git_config("commit.gpgsign").is_some_and(|v| v == "true"),
            has_key: git_config("user.signingkey").is_some(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_home_folder_is_never_written_out() {
        let home = "/Users/fakename";
        assert_eq!(
            anonymize("/Users/fakename/.cargo/bin:/usr/bin", home),
            "~/.cargo/bin:/usr/bin"
        );
        assert_eq!(anonymize("/usr/bin", home), "/usr/bin");
        assert_eq!(anonymize("/Users/fakename/", "/Users/fakename/"), "~/");
        // An empty or root home must not turn every slash into a tilde.
        assert_eq!(anonymize("/usr/bin", ""), "/usr/bin");
        assert_eq!(anonymize("/usr/bin", "/"), "/usr/bin");
    }

    #[cfg(unix)]
    #[test]
    fn a_tool_is_found_in_the_first_folder_that_has_it() {
        let one = tempfile::TempDir::new().unwrap();
        let two = tempfile::TempDir::new().unwrap();
        let program = two.path().join("mytool");
        std::fs::write(&program, "#!/bin/sh\n").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let dirs = vec![
            one.path().to_string_lossy().into_owned(),
            two.path().to_string_lossy().into_owned(),
        ];
        assert_eq!(
            find_tool("mytool", &dirs),
            Some(program.to_string_lossy().into_owned())
        );
        assert_eq!(find_tool("missing-tool", &dirs), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_file_that_is_not_executable_is_not_a_tool() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("plain"), "x").unwrap();
        let dirs = vec![dir.path().to_string_lossy().into_owned()];
        assert_eq!(find_tool("plain", &dirs), None);
    }

    #[test]
    fn the_agent_is_named_without_its_path() {
        assert_eq!(
            describe_agent(
                "",
                Some("/Users/x/Library/Group Containers/2BUA.com.1password/t/agent.sock")
            ),
            "1Password"
        );
        assert_eq!(
            describe_agent("", Some("/run/user/1/my-agent.sock")),
            "named by ssh config"
        );
        assert_eq!(
            describe_agent("/private/tmp/com.apple.launchd.abc/Listeners", None),
            "system (launchd)"
        );
        assert_eq!(describe_agent("/tmp/ssh-abc/agent.1", None), "environment");
        assert_eq!(describe_agent("", None), "none");
    }

    #[test]
    fn keys_are_counted_from_the_exit_code_of_ssh_add() {
        assert_eq!(
            count_keys(0, "256 SHA256:a x (ED25519)\n3072 SHA256:b y (RSA)\n"),
            Some(2)
        );
        assert_eq!(count_keys(1, "The agent has no identities.\n"), Some(0));
        assert_eq!(count_keys(2, ""), None);
    }

    #[test]
    fn the_collected_report_holds_no_home_folder() {
        let report = collect();
        let home = home_dir();
        let text = serde_json::to_string(&report).unwrap();
        if home.len() > 1 {
            assert!(!text.contains(home.trim_end_matches('/')), "{text}");
        }
        assert!(report.tools.iter().any(|t| t.name == "git"));
        assert!(!report.app_version.is_empty());
    }
}
