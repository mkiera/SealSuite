# SealSuite 🦭

A Windows desktop app that manages the SealSuite apps:
[FinFetcher](https://github.com/mkiera/FinFetcher),
[FlipperClipper](https://github.com/mkiera/FlipperClipper) and
[BlubberBound](https://github.com/mkiera/BlubberBound). It shows which apps are
installed and their versions, handles their updates on the stable, beta and
alpha channels, manages the tools they share (FFmpeg, ffprobe, yt-dlp) and
passes files from one app to another.

This repository is a skeleton. The window is a placeholder, and none of the
features above exist yet.

## Design rules

- Each app stays installable on its own and runs fully without SealSuite.
- SealSuite hooks into the apps that are already installed. It finds them
  through their entries under
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall` (`InstallLocation`),
  the same way FinFetcher finds FlipperClipper. It never moves, takes over or
  reinstalls an app.
- Uninstalling SealSuite breaks no app. SealSuite is never a hard dependency.
  A shared tool, such as one shared FFmpeg, lives where SealSuite's uninstaller
  does not delete it, or each app checks it and falls back to its own copy.
  SealSuite's uninstaller removes only SealSuite's own files and registry
  entries.
- Every integration is optional on the app side. An app behaves the same when
  SealSuite is absent.

## Build

Built with [Tauri 2](https://tauri.app), Vite and TypeScript.

```
npm ci
npm test
npm run build
npm run tauri dev
cargo test --manifest-path src-tauri/Cargo.toml
```

On Linux, `cargo` needs Tauri's system packages (WebKitGTK 4.1 and others, see
`.github/workflows/build-test.yml`).

## License

GPL-3.0-or-later. Outfit font: SIL Open Font License, see `src/fonts/OFL.txt`.
