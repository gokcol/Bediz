# Bediz 3.1.0 — build 127

Build 127 includes the editable canvas text/image paste improvements from the build 126 draft and fixes the visible one-by-one reload of Recents after capturing.

Capture previously cleared every thumbnail twice: while temporarily hiding the editor and after saving the capture. Temporary capture hiding now retains the bounded caches, and saving a capture refreshes only its own path. Existing previews stay visible while the new capture loads. Clipboard retry uses temporary concealment without restoring old window geometry. Intentional backgrounding still releases caches.

## Validation

- 726 application tests and 19 packaging tests passed locally, with 27 explicit fixture/platform tests ignored. Formatting, strict workspace/all-target Clippy and whitespace checks passed.
- The new regression loads real temporary projects and inspects their rendered texture IDs. Existing previews keep the same textures through temporary hiding, cancellation and every loading frame after Editor or Clipboard capture. Only the new capture receives a new texture.
- The regression also checks that clipboard-retry concealment does not resize, reposition or unmaximize the window.
- Existing clipboard, project-save, capture, thumbnail-cache and stale-worker-reply tests pass.
- Native CI uses GitHub's supported `macos-15-intel` runner after build 126's Apple Silicon job failed to acquire a runner within 15 minutes. The local Apple Silicon checks and signed release build remain required. See [GitHub runner labels](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).

## Release preparation

Target tag: `v3.1.0-build127`. This build supersedes the unpublished build 126 draft; its existing tag is retained. Signed/notarized Mac distribution, Windows packaging, GitHub publication and Mac installation are pending final preparation.

The Microsoft Store package metadata remains 3.1.3.0. This request updates GitHub and the installed Mac app; it does not submit a new Store package. Existing application identities, project format and storage paths are retained.
