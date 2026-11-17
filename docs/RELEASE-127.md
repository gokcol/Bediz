# Bediz 3.1.0 — build 127

Build 127 includes the editable canvas text/image paste improvements from the build 126 draft and fixes the visible one-by-one reload of Recents after capturing.

Capture previously cleared every thumbnail twice: while temporarily hiding the editor and after saving the capture. Temporary capture hiding now retains the bounded caches, and saving a capture refreshes only its own path. Existing previews stay visible while the new capture loads. Clipboard retry uses temporary concealment without restoring old window geometry. Intentional backgrounding still releases caches.

## Validation

- 726 application tests and 19 packaging tests passed locally, with 27 explicit fixture/platform tests ignored. Formatting, strict workspace/all-target Clippy and whitespace checks passed.
- The new regression loads real temporary projects and inspects their rendered texture IDs. Existing previews keep the same textures through temporary hiding, cancellation and every loading frame after Editor or Clipboard capture. Only the new capture receives a new texture.
- The regression also checks that clipboard-retry concealment does not resize, reposition or unmaximize the window.
- Existing clipboard, project-save, capture, thumbnail-cache and stale-worker-reply tests pass.
- Native CI uses GitHub's supported `macos-15-intel` runner after build 126's Apple Silicon job failed to acquire a runner within 15 minutes. The local Apple Silicon checks and signed release build remain required. See [GitHub runner labels](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).

## Verified release and Mac installation

Published as the latest GitHub release: [`v3.1.0-build127`](https://github.com/gokcol/MyImage/releases/tag/v3.1.0-build127), source commit `9ebd8da0f43edf81ad9e4be5231dccdd6d506fba`. This build includes canvas paste and the Recents fix and supersedes the unpublished build 126 draft; the existing build 126 tag is retained.

- [Native CI](https://github.com/gokcol/MyImage/actions/runs/44125586373) passed on the Intel Mac runner: 726 application tests and 19 packaging tests, with 27 ignored; formatting and strict Clippy passed. The Recents texture-continuity regression passed explicitly.
- [Windows packaging and installer lifecycle](https://github.com/gokcol/MyImage/actions/runs/44125643900) passed. Source/build metadata, hashes, x64 executable, embedded release notes and Bediz installer version 3.1.0.127 were independently verified. GitHub Windows downloads are unsigned; interactive clipboard/capture/device checks remain pending.
- Apple accepted app notarization `682fbac0-a6d7-4bd5-80ae-9f328b137f22` and DMG notarization `a1961f74-14cd-4e2b-8168-23873a0bfcbc`. Developer ID signatures, hardened runtime, timestamps, stapled tickets, Gatekeeper and ZIP/DMG contents passed verification.
- All nine distribution asset hashes match the verified local packages. The attached `release-validation.json` records the artifact, CI and installation checks; its uploaded digest was also verified after publication.
- Bediz 3.1.0 build 127 was installed at `/Applications/Bediz.app` using the verified rollback-preserving installer. Its executable exactly matches the signed release (SHA-256 `19b673d1f676a63d686a2f5bbe9cf19698566b05e387f8881834bb6b4e68ed08`). The app launched, displayed **Build 127 · Installed**, and reopened the previous saved project. User artwork was not changed.

Local evidence is under `dist/release-v3.1.0-build127/`. The native UI check verified installation and project reopening; the automated regression verifies thumbnail continuity through capture completion.

## Microsoft Store update

Store package **3.1.4.0** is prepared from metadata commit `49cd0739ecfb78fac3223d0ba892ac6c4b6078c5`, retaining app **3.1.0 build 127**. It contains the same application logic as the immutable GitHub tag; the bundled Store version and packaging metadata advance together. The published GitHub binaries and installed Mac app remain unchanged.

The Store listing and certification notes include canvas paste and the Recents fix and were saved and verified after reloading. The public English product name is **Bediz**. Partner Center's separate dashboard name was changed from MyImage (OG) to **Bediz**, and the page title and heading were verified. Existing application identities, project format and storage paths are retained.

Submission **8**, `1152921505702431515`, is **in certification / pre-processing**, verified on 17 November 2026, with automatic publication after Microsoft approval. Partner Center validated package 3.1.4.0 and replaced the superseded 3.1.3.0 package. Build 125 / package 3.1.3.0, submission 7, was confirmed published in Partner Center on 17 November 2026. Local Store preparation and validation evidence is under `dist/msix-v3.1.4-build127/`.

- [Store package validation](https://github.com/gokcol/MyImage/actions/runs/44148114126), [disposable MSIX installation/removal](https://github.com/gokcol/MyImage/actions/runs/44148048653), and [native checks](https://github.com/gokcol/MyImage/actions/runs/44148048472) passed for the Store metadata commit. Native checks passed 726 application and 19 packaging tests, with 27 ignored; formatting and strict Clippy passed. Four focused release-metadata tests also passed locally.
- Independently verified package SHA-256: `1dadb6285d83e66a09dc47251c961b8b3db355a06319e5856f9c956045d3f837`. The x64 executable embeds app 3.1.0 build 127 and Store version 3.1.4.0. Manifest identity, all Bediz display names, standard virtualization, disabled startup, checksum sidecar and package block hashes passed verification. `runFullTrust` remains the only restricted capability.
- This unsigned MSIX is a Partner Center upload artifact. Microsoft certification, signing and publication are pending; interactive Windows capture/clipboard/device validation and a Windows App Certification Kit run were not performed here.
