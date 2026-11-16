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
