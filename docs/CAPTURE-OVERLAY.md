# Capture overlay and Region dragging — build 113

The user reported a flash when entering capture, inaccessible menu-bar/Dock areas, and intermittent Region drags stopping at an apparent window boundary.

The editor window was revealed before its capture geometry settled. The overlay now paints while hidden through placement, then reveals at its full-screen size. Screen dimming eases in over 160 ms and does not gate pointer input. The direct event-loop wake-up introduced in build 111 remains in place.

On macOS, winit maps AlwaysOnTop to the floating window level, below the menu bar and Dock. During hidden placement, SkyImage now raises only its own capture windows to the Quartz overlay level. Root and secondary capture windows have a dedicated title; restoring the editor restores its normal title and window level. Other application windows are untouched.

A PointerGone event used to cancel an unfinished region drag. It now clears hover while preserving the drag. Focus loss still stops the drag without accepting it. After a pointer exit, native button-state queries recover a mouse release that egui-winit can miss outside a viewport. Recovery completes the old drag at its last known point before any new press and never duplicates a delivered mouse-up event. Windows checks respect swapped mouse buttons.

## Validation

- 684 app tests and 17 packaging tests passed locally; formatting and Clippy passed.
- Regression coverage: hidden preparation and single reveal; first drag during the fade; monotonic, bounded fade; Region drags crossing underlying windows; top/bottom screen coordinates; continuation after pointer exit; mouse-release recovery; safe focus loss; existing background wake-up and multi-display selection tests.
- Source: 82f135416dfb0ca478eef771e6690c41e8287004 (includes transition commit 3edd755).
- Mac CI passed: https://github.com/gokcol/MyImage/actions/runs/43968661110
- Windows build and MSIX validation passed, including test installation/removal: https://github.com/gokcol/MyImage/actions/runs/43968661101
- Build 113 app and DMG accepted by Apple notarization. Installed at /Applications/SkyImage.app.
- Native Region selection succeeded at the top edge (y=2) and bottom edge (y=1910) of the 2940×1912 display. Escape restored the normal editor.
- User confirmed menu-bar and Dock selection work on build 113. The flash remained, so its transition change was insufficient.

The user confirmed Region is the affected mode. The intermittent interruption was not reproduced on demand; tests cover the concrete pointer-loss paths found in code. Further normal-use observation is needed to confirm whether all instances are resolved.

Build 112 was a draft for the transition change alone. Build 113 includes that change and the subsequent selection fixes.

## Follow-up — build 114

User testing confirmed the system-bar fix but still showed a flash. eframe paints before applying viewport visibility commands: the capture-pending update can therefore paint an empty scene before Visible(false) takes effect. On macOS, capture now orders the native editor/launcher out immediately, before returning that empty scene. Automatic window ordering animations are disabled during capture.

Capture windows remain at zero native opacity through placement and native show/unhide. The runtime restores opacity only after eframe has returned from painting and applying viewport commands. Both regular RedrawRequested events and explicit background wake-ups run this final step. Normal editor show/error/reopen paths also restore opacity after painting. The 160 ms dimming transition and working selection behavior remain in place.

Build 114: 684 app tests and 17 packaging tests passed; formatting and Clippy passed. Source: 573756af64ee6e2d803b193402d5950734b309d6. Native CI: https://github.com/gokcol/MyImage/actions/runs/43972014577 (passed).

The temporary Keychain-access failure cleared after the user unlocked the Mac. The app and DMG were accepted by Apple notarization. Build 114 is installed; native checks confirmed a visible Region overlay, first-drag selection and restoration of the normal window on Escape. The user was asked to confirm whether the bright/dark screen blink is gone before publication.

## Follow-up — build 115

The user reports that build 114 reduced the bright/dark blink but did not remove it. Its GitHub release remains a draft.

Snagit was inspected with its selection crosshair active. Its desktop snapshot stayed frozen, it showed a full-screen crosshair and a hovered-window outline, and its compact confirmation toolbar appeared after selection. Capture ran in its separate SnagitHelper application. Static inspection does not establish whether Snagit has a startup blink or measure its duration.

SkyImage now creates a dedicated immediate viewport for every captured display, including the primary display. The editor remains hidden at its normal geometry; it is no longer restyled, resized and revealed as the first overlay. The existing resident runtime continues draining child input/repaints through the hidden root. The first overlay gains keyboard focus once it is ready. Reopening the app during capture focuses that overlay, and completion/cancellation closes every capture viewport.

The current capture controls and the menu-bar/Dock level, hidden placement, post-paint reveal, dimming transition and Region drag recovery are retained. Native testing and user confirmation of the remaining blink are pending; this candidate must not be published as a confirmed flash fix.

Local validation: 684 app tests and 17 packaging tests passed; formatting and Clippy passed. Lifecycle coverage verifies that the editor receives no reveal, focus, style or geometry commands during selection; the dedicated overlay gains focus once and accepts the first drag during dimming. Reopen routes focus to the overlay, and cancellation closes all display viewports. When capture ends, the editor is painted in the same update before its post-paint reveal.

Build 115 is signed, notarized and installed. Mac CI passed: https://github.com/gokcol/MyImage/actions/runs/43976730888. The dedicated overlay opens, and Escape restores the normal window. Automated mouse input moved the crosshair but did not create a selection; the user separately confirmed that selection works with their mouse. The user also confirmed that the startup blink remains. Dedicated windows alone did not resolve the visual issue; publication stays on hold.

Windows build and MSIX validation, including disposable installation/removal, passed: https://github.com/gokcol/MyImage/actions/runs/43977029329. Source: 53a45f1124d048c522c84af52843df53774de1af. The notarized app ZIP, DMG and checksums are stored in draft v3.0.1-build115. No Store submission was made. The next diagnosis needs the remaining blink's sequence: brightness briefly reverses, or the desktop simply darkens once. Snagit and SkyImage both have frozen-background selection, crosshairs and window highlighting; Snagit's compact confirmation toolbar appears after selection.

## Follow-up — build 116

The user confirmed Snagit does not blink, and clarified that SkyImage shows a very brief blink followed by abrupt darkening. This is not being treated as normal capture behavior.

Idle capture now keeps the screenshot at its original brightness, including while hovering over a window. Crosshairs, window outlines, magnifier and controls remain. Once a real selection exists, only its outside area dims over 160 ms; clearing the selection resets that transition. Empty Multi-region capture also stays undimmed.

The capture viewport requests native transparency on macOS. eframe 0.33 configures transparency support for the shared wgpu painter from the root viewport, not from each child. Mac startup therefore enables transparent backbuffers at that root setting too. The app already clears to transparent, and its editor panels paint their normal opaque surfaces. Previously, the capture viewport and shared painter used opaque defaults, allowing a transparent clear frame to become a solid backing surface while the first Metal image was being presented. Hidden placement, native opacity protection and post-paint reveal are retained; the brief-blink outcome still requires native/user validation.

Local checks passed: 686 app tests and 17 packaging tests, formatting and Clippy. New rendering regressions exercise all six capture modes before selection and during window hover, and verify that a drawn Region shades exactly its outside area and resets the dimming state when cleared.

Build 116 is signed, notarized and installed. The user confirmed both the brief blink and startup darkening are gone, Region selection works, and the transition is almost identical to the Snagit benchmark. Mac CI passed: https://github.com/gokcol/MyImage/actions/runs/43980251450. Windows build/MSIX validation passed: https://github.com/gokcol/MyImage/actions/runs/43980267833. Source: 043387c581f73b6fc3c88e1b12f79ae25a5a4b53.

## Follow-up — build 117

ScreenCaptureKit now excludes SkyImage's own process explicitly from display captures, retaining desktop, menu bar and Dock content. This allows the fixed 250 ms compositor-settling wait to be removed on Mac. User-configured capture delays, transparent backbuffers, hidden placement and post-paint reveal remain unchanged. Windows retains its GDI settling delay.

Output size (100%, 75%, 50%, 25%) scales the completed image once, before Library, editor and clipboard delivery. Existing settings default to 100%. Selection, magnifier, window capture and scrolling composition retain full-resolution input; text recognition is unaffected. Alpha-aware filtering preserves transparent edges, and clipboard retries reuse the already resized image.

The Capture Now shortcut is clickable in both Home and the resident panel. A centered editor chooses modifiers and key, checks OS conflicts, saves the binding and preserves the period aliases. Registration/persistence failure rolls back the replacement. Quick Start and the shortcut editor have accessible X controls and Escape dismissal.

Build 117 local validation: 695 app tests and 17 packaging tests passed; formatting and Clippy passed. Tests cover output scaling for both destinations and the saved Library image, original pixel preservation, transparent edges, preference migration, hotkey conflicts and rollback, retained aliases, modal input blocking and Escape cancellation.

Build 117 is signed, notarized, installed and published with Mac DMG/ZIP and Windows installer/portable downloads: https://github.com/gokcol/MyImage/releases/tag/v3.0.1-build117. Source: b13f6d779e4fb1690969752ef5cc19661d727901. Mac CI passed: https://github.com/gokcol/MyImage/actions/runs/43983071096. Windows installer lifecycle checks passed: https://github.com/gokcol/MyImage/actions/runs/43983094038. Windows MSIX validation passed: https://github.com/gokcol/MyImage/actions/runs/43983098391. No new Store submission was made.

Native checks confirmed both new X controls, opening the hotkey editor from the resident keycaps, successful registration and persistence of a temporary shortcut, and restoration of the original shortcut. A real 2940 × 1912 full-screen capture at 50% was delivered to Editor and Library as 1470 × 956. Region, 100% output and Control+Shift+C were restored after testing. Preview remains undimmed and excludes the editor. The menu bar and Dock are configured to auto-hide on this Mac. First-activation speed has not been quantitatively measured. On 17 November 2026, the user described background hotkey activation as “smoother i think. see no difference”. This supports preserving the current transition, but does not establish a noticeable startup-speed improvement.

## Follow-up — builds 118–119

The selection toolbar now uses compact mode/destination menus, an output percentage, and named icon actions with tooltips. Its measured width is used to place it next to the selected region. Region and Window selections offer a scrolling action that preserves the rectangle and the user's preferred capture mode.

Scrolling regions are sampled automatically with the existing overlap matcher. A small Done/Pause/Restart/Cancel controller and a growing preview replace the editor-sized scrolling window. Four thin, mouse-transparent windows outline the target; the editor remains hidden and input reaches the underlying document. Restart samples the current visible area. Done waits for an in-flight sample and takes a final sample before output; ambiguous matches pause collection while preserving accepted sections. A one-section result requires explicit confirmation. Output scaling still runs once after stitching, with 100% the default.

Escape cancels a selection even with a popup open. A temporary native Escape registration also routes cancellation while the document has focus during scrolling; registration is removed outside capture. Canceled worker results are rejected by the capture generation. ScreenCaptureKit excludes this app's controls; Windows HUD windows request WDA_EXCLUDEFROMCAPTURE.

Scrolling is no longer offered as a capture mode. The saved active mode remains Region or Window; a separate secondary scroll action sits beside Capture after selection. Old Scrolling preferences migrate to Region. The resident runtime retains delayed repaint deadlines so automatic collection continues while its editor is hidden.

Local validation of the initial candidate: 701 app tests and 17 packaging tests passed, as did formatting and Clippy. Regressions cover popup cancellation, compact layout in every mode, Retina/negative-origin geometry, automatic region sampling and restart, canceled work, controller-only reopen, and hidden-editor/input-transparent HUD lifecycle. Native Mac and Windows CI validation are pending.

The local Dock had retained an entry pointing to the missing /Applications/MyImage.app. That single entry was repaired to /Applications/SkyImage.app after backing up the Dock preferences. The installed build 117 was observed opening normally; no app crash report explained the earlier launch delay.

Build 118 was installed as a local candidate. The revised selection/scrolling flow is build 119 so the installer can perform a forward upgrade. The revised code passed 703 app tests and 17 packaging tests, formatting and Clippy before the metadata-only build-number increment.
