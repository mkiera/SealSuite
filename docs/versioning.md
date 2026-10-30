# Versioning

SealSuite uses the same release process as FlipperClipper.

| Build | Tag | Application version |
| --- | --- | --- |
| Stable | `v1.0.0` | `1.0.0` |
| Beta | `v1.0.0-beta.1` | `1.0.0-beta.1` |
| Branch build two commits after `v1.0.0` | none | `1.0.1-alpha.2` |

`package.json` holds the local core with three numeric parts, and `version.txt`
holds the same core (a test checks that they match). Release builds take the
version from the tag without editing either file. The installer uses the same
semantic version and derives its Windows resource version as
`major.minor.patch.0`. The Cargo crate version is independent of the
application version that Tauri and the updater read.

## Prepare a release

1. Integrate changes into `beta`. Merge work with several commits using
   `git merge --no-ff`. Add user-visible changes to `CHANGELOG.md` under
   `## Unreleased`.
2. Choose a patch for compatible fixes, a minor for compatible features, or a
   major for incompatible changes. Keep that core through its beta series.
   When the core changes, update `package.json`, its lockfile and `version.txt`
   together.
3. Move the changes into `## 1.0.0-beta.1 - YYYY-MM-DD`, using the actual
   version and date. Leave an empty `## Unreleased` heading. Stable notes
   collect the results of the beta series under a separate stable heading.
4. Run `npm ci`, `npm test`, `npm run build` and
   `cargo test --manifest-path src-tauri/Cargo.toml --locked`.
5. On an explicit release request, create a lightweight beta tag at the
   prepared `beta` head. For stable, merge `beta` into `main` with `--no-ff`
   and tag that merge. Push the branch and the tag in one atomic push. Check
   the `Build and Release` run and its `SealSuite-Setup.exe` asset.

Tagged changelog sections stay frozen. Each release needs one exact, nonempty
changelog section. Tags accept only the stable and numbered beta forms. CI
checks that a beta tag is the remote `beta` head and that a stable tag is the
remote `main` merge from `beta`. Do not merge `main` back into `beta`.

## Branch builds

Every pushed branch runs `.github/workflows/build-test.yml` on `windows-latest`.
A newer push cancels an older run on the same branch. A successful run uploads
the installer as the `SealSuite-Setup` artifact, kept for 30 days.

`scripts/versioning.mjs` calculates alpha versions from the full Git history:
the commit distance from the nearest version tag, accounting for stable tags
on `main` outside `beta` ancestry. A build exactly on a tag keeps that tag's
version. A repository without version tags uses the workflow run number.
Alpha builds get no tag and are installed by hand from the Alpha channel.

## Update channels

SealSuite updates itself from this repository. The managed apps update from
their own repositories.

| Channel | Stored value | Source | Offered automatically |
| --- | --- | --- | --- |
| Stable | `stable` | Releases without a prerelease version | Yes |
| Beta | `prerelease` | Stable releases and tagged betas | Yes |
| Alpha | `alpha` | Newest successful Build Test run of each live branch | No |

The channel is saved in `self-update.json` in SealSuite's config folder.

## Icon

The files in `src-tauri/icons` are placeholders until SealSuite's own icon is
made. `tauri.conf.json` and `installer.iss` read their icons from there.

## Names that must not change

Keep the installer `AppId` (`package_id` in `app_profile.json`), the Tauri
identifier `com.mkiera.sealsuite`, the executable name, the release asset name
and the workflow file names, so installed copies keep their update and
downgrade paths.

## Uninstalling

The uninstaller removes the install folder, SealSuite's own uninstall entry
and the two `com.mkiera.sealsuite` data folders. It removes nothing that
belongs to FinFetcher, FlipperClipper or BlubberBound. A tool that SealSuite
shares between the apps must live outside those folders.
