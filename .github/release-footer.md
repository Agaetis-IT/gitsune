## Install

You need `git` installed. The pull request features use the sign-in of `gh` (GitHub), `glab` (GitLab) or `az` (Azure DevOps) when you have them.

| System                           | File                                                                | Notes                                                                                                                                                                                       |
| -------------------------------- | ------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS 11 or later, Apple silicon | `Gitsune_<version>_aarch64.dmg`                                     | Open it and drag Gitsune to Applications. The app is not signed yet: the first time, right-click it and choose **Open**, or run `xattr -dr com.apple.quarantine /Applications/Gitsune.app`. |
| Linux (x86_64)                   | `Gitsune_<version>_amd64.deb` or `Gitsune_<version>_amd64.AppImage` | `sudo apt install ./Gitsune_<version>_amd64.deb`, or `chmod +x` the AppImage and run it.                                                                                                    |
| Windows (x64)                    | `Gitsune_<version>_x64-setup.exe`                                   | Run the installer. SmartScreen may warn since the app is not signed: choose **More info**, then **Run anyway**. [Git for Windows](https://gitforwindows.org) is required.                   |

## Known limits

- The packages are **not signed or notarized** yet.
- macOS is the system Gitsune is made and tried on. **Linux and Windows are experimental**: they are built and tested by the CI, not by hand yet.
- Not included yet: submodules, Git LFS and editing files in the application.

## Feedback

Something does not work, or is missing? Open an issue, and paste the report of **Settings, General, Diagnostics** (it contains no personal data).
