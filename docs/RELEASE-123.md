# SkyImage 3.1.0 — build 123

Window capture keeps its selected native window when starting scrolling capture. Separate side-by-side scrolling panes retain independent histories and their original horizontal positions in the combined image. Scroll each pane down in turn, then choose Done. Shorter panes leave neutral space below their captured content; fixed headers and footers are composed once.

The implementation stages each sample before committing it, so a failed alignment cannot partially advance another pane. The combined canvas has bounded dimensions and pixel count. Region capture retains its existing selection and alignment behavior. The preview identifies the number of detected panes.

## Validation

- 717 application tests and 17 packaging tests passed on macOS; 27 platform/fixture tests were ignored in that workspace run.
- Clippy with `-D warnings`, formatting and whitespace checks passed.
- Tests cover either discovery order, alternating and simultaneous unequal offsets, a later third pane, unequal captured and viewport heights, fixed edges and footers, adjacent panes, idle samples, atomic rejection, and Region compatibility.
- A separate replay of native Firefox screenshots collected two independent panes into one 2940 × 2356 image with five accepted sections. This validates the compositor; a complete live Window → scroll → Done operation was not repeated in build 123. The Window capture pipeline was checked live in build 122.
- The local Mac build was signed, notarized, installed and launched before release preparation. Release packages are rebuilt from the tagged source with Store package metadata 3.1.1.0; current release validation is attached to the GitHub release.

## Current limits

Detection supports separate side-by-side panes. Vertically stacked areas with overlapping horizontal spans are not supported. Sparse or animated content may need slower scrolling with more overlap. Small rounded clipping artifacts can remain at moving content edges. Physical Windows capture testing remains pending; hosted packaging checks are reported separately from native interaction tests.
