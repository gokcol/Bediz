# Bediz 3.1.0 — build 126

Pasting plain text on an open canvas now creates an independent editable text box. It uses the remembered Text tool appearance, preserves line breaks and Unicode, and appears at the center of the visible canvas. Double-click edits its content; dragging moves it. Saving as `.myimage` retains the text and appearance. Inserting a text box is one undoable edit, including any required canvas expansion.

Image paste retains separate, movable and resizable image layers. Native image pixels take priority over accompanying text, such as a browser's copied image URL. Focused text fields retain normal paste, and Bediz's editable annotation clipboard format remains supported. Home and empty Editor continue to offer image paste into a new document.

## Validation before release preparation

- 725 application tests and 19 packaging tests passed locally; 27 explicit fixture/platform tests were ignored.
- Formatting, strict workspace/all-target Clippy and whitespace checks passed.
- Regression coverage includes text appearance, Unicode and line breaks, focused text editing, saved-project reopening, undo/redo, placement within a cropped canvas, oversized text rejection, stale asynchronous paste protection, and native editable annotation fallback.
- Clipboard classification checks cover image precedence, native and supplied text, empty content, malformed images and clipboard access errors.
- The sandbox could not access Apple's HEIF encoding service. The full suite passed when rerun with normal macOS system-service access.

## Release status

Source commit `308f44546bb590078037061b296c7c421ac36ff1` and tag `v3.1.0-build126` are pushed. The [GitHub draft release](https://github.com/gokcol/MyImage/releases/tag/untagged-74e3169592cbfa0ec965) has all nine verified Mac/Windows distribution assets attached.

- Mac app notarization accepted: `ae72a010-f09b-42c7-80eb-6b8f6ffbfd75`. DMG notarization accepted: `77763691-cfb4-4109-ac6f-364aca0145a5`. Developer ID signatures, hardened runtime, timestamps, stapled tickets, Gatekeeper, archive contents and the read-only DMG check passed.
- [Windows packaging and installer checks](https://github.com/gokcol/MyImage/actions/runs/44112055497) passed for the tagged source. Setup/portable checksums, source/build metadata, executable identity, Bediz branding and embedded release notes passed verification.
- All nine GitHub distribution asset sizes and SHA-256 digests match the verified local files. Evidence is recorded in `dist/release-v3.1.0-build126/release-validation.json` and attached to the draft.
- [GitHub Native checks](https://github.com/gokcol/MyImage/actions/runs/44111939692) is still queued waiting for a Mac runner as of 17 November 2026. Publication is pending that workflow's successful completion; the local suite passed as recorded above.

Windows GitHub downloads are unsigned. Interactive Windows clipboard/device validation remains pending. The installer workflow tests reinstallation and simulated legacy shortcut migration, rather than an interactive upgrade from build 125. This release preparation did not update the installed Mac app.

The Microsoft Store submission remains a separate build 125 submission (package 3.1.3.0); this GitHub release request does not submit a Store update. Application identifiers, project format and storage paths are retained.
