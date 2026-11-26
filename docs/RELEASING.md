# Releasing

A release is made by the **Release** workflow ([`.github/workflows/release.yml`](../.github/workflows/release.yml)) when a pull request is merged into `main`. Nothing is done by hand once the pull request is merged.

## What the workflow does

1. Reads the version of `src-tauri/tauri.conf.json` and checks that `package.json` and `src-tauri/Cargo.toml` have the same one.
2. Stops if a release `v<version>` already exists: a merge that does not change the version publishes nothing.
3. Creates a **draft** release `v<version>` whose text is [`docs/releases/<version>.md`](releases) followed by [`.github/release-footer.md`](../.github/release-footer.md) (how to install, known limits).
4. Builds the packages on macOS (Apple silicon), Linux and Windows and attaches them to the draft: `.dmg`, `.deb`, `.AppImage` and the Windows installer.
5. Publishes the release, which creates the tag `v<version>` on the merged commit. The summary of the run lists the attached packages and warns about a missing one.

Windows is experimental: if its build fails, the release is published without its installer and the summary says so. A failure on macOS or Linux leaves the draft unpublished.

## Releasing a version

1. In a branch, change the version in the three files: `package.json`, `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml` (and the lock file with a build). A test fails when they differ.
2. Write `docs/releases/<version>.md`. It starts with `# Gitsune <version>` and says what is new for a reader, not the list of commits. A test fails when the file is missing.
3. Open the pull request, wait for the CI, merge it into `develop`.
4. Open a pull request from `develop` into `main` and merge it. The release is published a few minutes later.

## Trying it without publishing

Run the **Release** workflow by hand (_Actions_, _Release_, _Run workflow_) and uncheck _Publish_. The release stays a draft with all its packages: download them, check them, then publish the draft on the Releases page. A draft left by a failed run is replaced by the next run.

## How the application finds out about a release

Once a day (when the application starts, a few seconds after the window opens) and whenever the user chooses _Gitsune, Check for Updates…_ or _Settings, General, Check now_, the application reads the **latest published release** of the repository through the public GitHub API (`src-tauri/src/updates.rs`, no token). Drafts and pre-releases are not offered. The tag (`v0.2.0`) is compared with the running version as numbers; when it is newer a window shows the notes of the release and opens its page. Nothing about the user or their repositories is sent, and a failed automatic check says nothing and is tried again at the next start. _Settings, General, Look for a new version once a day_ switches the automatic check off. A release is therefore only found once its tag is published: a draft is invisible until the workflow publishes it.

## Not set up yet

- **Signing and notarization.** The macOS package is not signed or notarized (people open it with a right-click), nor is the Windows installer. For macOS, an Apple Developer account and the secrets `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD` and `APPLE_TEAM_ID` are read by the Tauri action without a change to the workflow.
- **Updates installed by the application itself.** The application only tells that a version is out and opens its release page (see below); installing it is done by hand. An automatic install needs the Tauri updater: a signing key pair for the updates (`tauri signer generate`), its private key as a secret of the repository, and `latest.json` with the signatures attached to each release.
- **Intel Macs.** The macOS package is for Apple silicon only.

## Settings of the repository

- _Settings, Actions, General, Workflow permissions_: **Read and write permissions** (the workflow also asks for `contents: write`).
- Protect `main`: a pull request is required, and the checks `Check (macos-latest)` and `Check (ubuntu-latest)` must pass.
