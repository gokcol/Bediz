# Background capture wake-up — build 111

Reported symptom: a capture shortcut sometimes appears unresponsive, then capture appears when SkyImage is clicked.

Desktop commands previously queued messages and called egui request_repaint_of(ROOT). egui 0.33.3 coalesces requests while a repaint is outstanding, so a second request can invoke no native callback. Inactive or hidden native windows may also defer redraws. The existing fallback relied on hidden/occluded state and receiving a repaint event, leaving commands or completed captures waiting for the next window interaction.

The runtime now installs a command waker in the egui context. It sets an atomic pending flag and sends a native event-loop event directly, bypassing repaint callback coalescing. At about_to_wait, one root update drains queued work regardless of native occlusion state. It does not render recursively from user_event and does not add a polling timer. Ordinary redraws retain their existing coalescing.

Hotkeys, tray commands, native information menus, Dock reopen, Quit and capture-worker completion use this path. File identities, capture shortcuts and permission records are unchanged.

Validation:

- Regression test reproduces suppressed egui repaint callbacks and verifies explicit command notifications remain independent and coalesce into one pending root update.
- Existing child-viewport coalescing and root-window tracking tests pass.
- 679 app tests and 17 packaging tests pass; formatting and Clippy pass.
- Native CI: https://github.com/gokcol/MyImage/actions/runs/43808928495
- Source: 172b88637e37212babf08f6710ed698965f86fce
- Mac app and DMG notarized; build 111 installed and launched. Update dialog closed before the background shortcut test.
- Physical global-hotkey check confirmed by the user on 16 November 2026: with another app in front and without clicking SkyImage, the selection overlay opens immediately. The intermittent symptom still benefits from observation during normal use.
