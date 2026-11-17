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

Release tag: `v3.1.0-build126`. Release preparation is pending commit, push and packaging verification. Mac distribution requires Developer ID signing and Apple notarization; Windows Setup/portable packages require the existing installer validation workflow. Interactive Windows clipboard/device validation remains pending.

The Microsoft Store submission remains a separate build 125 submission (package 3.1.3.0); this GitHub release request does not submit a Store update. Application identifiers, project format and storage paths are retained.
