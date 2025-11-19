# Task: Fullscreen Support for macOS - Phased Implementation Plan

## Problem Statement

Handy shows no visual feedback when transcription is active in macOS fullscreen apps, unlike alternatives (Whispr Flow, Voiceink etc).

**Related Issues:**
- https://github.com/cjpais/Handy/issues/277
- https://github.com/cjpais/Handy/issues/278

**PR:** https://github.com/dannysmith/Handy/pull/1

> [!WARNING]
> Before opening PR to upstream repo, revert dccb55b979a20973e7f96d2253136bccc268dd2a

## Requirements

1. **Fullscreen Overlay**: Recording overlay must appear above fullscreen applications (Safari, Chrome, video players, etc.)
2. **Multi-Space Support**: Overlay should appear in all Mission Control spaces when recording is active
3. **Visual Parity**: Panel styling must match existing Windows/Linux overlay (no window chrome, transparent background, pill-shaped)
4. **Stability**: No crashes when switching spaces, entering/exiting fullscreen, or during normal operation

## Technical Background

### Why Standard Tauri Windows Don't Work

On macOS since Big Sur:
- Standard `NSWindow` with `.always_on_top(true)` uses `NSFloatingWindowLevel` (level 3)
- Fullscreen apps run at `NSMainMenuWindowLevel + 1` or higher (level 25+)
- Therefore, standard windows appear **behind** fullscreen content

### NSPanel Solution

Only `NSPanel` windows can reliably appear above fullscreen apps when configured with:
1. **High Window Level**: `PanelLevel::Status` (25) or `PanelLevel::ScreenSaver` (1000)
2. **Collection Behaviors**:
   - `canJoinAllSpaces`: Appears across all Mission Control spaces
   - `fullScreenAuxiliary`: Works alongside fullscreen windows (does NOT take over the screen)

### tauri-nspanel Plugin

**Repository**: https://github.com/ahkohd/tauri-nspanel

**Why This Plugin:**
- Battle-tested in production apps (Cap screen recorder, Overlayed gaming overlay)
- Clean API for creating NSPanel windows in Tauri v2
- Handles all macOS-specific complexity internally
- Minimal code changes required

**API Pattern - CRITICAL:**
The plugin uses **builder pattern methods**, NOT enum constants:
- ❌ `CollectionBehavior::CAN_JOIN_ALL_SPACES` - Does not exist
- ✅ `CollectionBehavior::new().can_join_all_spaces()` - Correct

## Logging Strategy

Throughout implementation, use extensive logging to track panel lifecycle:

```rust
use log::{info, warn, error, debug};

// Panel creation
info!("[OVERLAY] Creating panel at position x={}, y={}", x, y);
info!("[OVERLAY] Panel created successfully and hidden");
error!("[OVERLAY] Failed to create panel: {}", e);

// Panel showing
info!("[OVERLAY] show_recording_overlay called");
info!("[OVERLAY] Found existing panel, reusing");
info!("[OVERLAY] Panel not found, recreating");
info!("[OVERLAY] Panel shown successfully");

// Panel hiding
info!("[OVERLAY] hide_recording_overlay called");
info!("[OVERLAY] Emitting hide event to panel");
info!("[OVERLAY] Panel hidden and destroyed");

// Panel recreation
warn!("[OVERLAY] Panel was destroyed by macOS, recreating");

// Errors
error!("[OVERLAY] get_webview_panel failed: {}", e);
```

**Log Prefixes:**
- `[OVERLAY]` - All overlay operations
- Use `info!` for lifecycle events (create, show, hide, destroy)
- Use `warn!` for unexpected but handled conditions (recreation)
- Use `error!` for actual failures

## CRITICAL INSIGHT (Session 3 - 2025-11-19)

**The original phased plan (Phases 5-9) was INCORRECT.**

### What Actually Works

Only `create_recording_overlay()` needs to be platform-conditional. All other functions (`show_recording_overlay`, `hide_recording_overlay`, `show_transcribing_overlay`, `update_overlay_position`, `emit_levels`) work with both panels and windows via the standard `get_webview_window()` API.

**Why This Works:**
- tauri-nspanel registers panels in Tauri's window manager
- Panels are accessible via `get_webview_window()` just like regular windows
- No need to rewrite all overlay functions with platform-conditional logic
- No need to use `get_webview_panel()` or `.to_window()` conversions in show/hide functions

**What Was Attempted (and Failed):**
- Rewrote all overlay functions to be platform-conditional
- Used `get_webview_panel()` and `.to_window()` everywhere
- Added panel recreation/destruction logic
- Result: App crashed after first use, even after multiple fix attempts
- Root cause: Overcomplicating the solution and fighting against the library's design

**Current Working Implementation (git commit 1c12c5e):**
- ✅ `create_recording_overlay()` is platform-conditional (creates NSPanel on macOS, NSWindow elsewhere)
- ✅ All other functions remain platform-agnostic using `get_webview_window()`
- ✅ Works in fullscreen mode
- ✅ Works across Mission Control spaces
- ✅ Shows and hides correctly

**Fixed Issues (Session 3):**
- ✅ Visual styling: Panel had rounded corners from NSPanel window → Added `.corner_radius(0.0)` in PanelBuilder
- ✅ Rapid toggle hang (partial): Thread spawning in `hide_recording_overlay()` → Removed thread, hide immediately

**Remaining Issues:**
- ✅ **FIXED: Rapid toggle hang** (Session 3)
  - **Root Cause**: Race condition in `show_recording_overlay()`
  - When rapidly toggling, `hide()` was called, then immediately `show()` + `update_overlay_position()`
  - `update_overlay_position()` tried to access window while it was mid-hide operation, causing deadlock
  - Logs showed `update_overlay_position()` starting but never completing its internal operations
  - **Fix**: Inlined position update logic directly in `show_recording_overlay()` AFTER `show()` call
  - This ensures window is fully shown before attempting to reposition it
  - Eliminates race condition between hide and position update operations

**Lesson:** Trust the library's design. If something requires extensive workarounds, you're probably doing it wrong.

---

## Phased Implementation Plan

### Phase 1: Add Dependency (Zero Code Changes)

**Goal:** Get tauri-nspanel dependency available without using it

**Files Modified:**
- `src-tauri/Cargo.toml`

**Changes:**
```toml
[target.'cfg(target_os = "macos")'.dependencies]
tauri-nspanel = { git = "https://github.com/ahkohd/tauri-nspanel", branch = "v2.1" }
```

**Commands:**
```bash
cd src-tauri
cargo check  # Download and verify dependency
```

**Testing:**
```bash
bun run tauri dev
```

**Success Criteria:**
- [ ] App compiles without errors
- [ ] App starts normally
- [ ] Recording works in non-fullscreen apps (baseline)
- [ ] No behavior changes at all

**Logging:** None needed (no code changes)

**Rollback:**
```bash
git checkout src-tauri/Cargo.toml
```

---

### Phase 2: Initialize Plugin (Still No Functional Changes)

**Goal:** Load the plugin but don't use it yet

**Files Modified:**
- `src-tauri/src/lib.rs`

**Changes:**
Around line 214-220, after `let mut builder = tauri::Builder::default();`:

```rust
let mut builder = tauri::Builder::default();

#[cfg(target_os = "macos")]
{
    info!("[OVERLAY] Initializing tauri-nspanel plugin");
    builder = builder.plugin(tauri_nspanel::init());
}

builder
    .plugin(
        LogBuilder::new()
```

**Testing:**
```bash
bun run tauri dev
```

Check logs for:
```
[OVERLAY] Initializing tauri-nspanel plugin
```

**Success Criteria:**
- [ ] App compiles without errors
- [ ] App starts and shows initialization log
- [ ] Recording works exactly as before
- [ ] No behavior changes

**Rollback:**
```bash
git checkout src-tauri/src/lib.rs
```

---

### Phase 3: Add Panel Definitions (Compile-Time Only)

**Goal:** Define the panel macro and imports without using them

**Files Modified:**
- `src-tauri/src/overlay.rs`

**Changes:**
Add at top of file (after existing imports, around line 6):

```rust
use crate::settings;
use crate::settings::OverlayPosition;
use enigo::{Enigo, Mouse};
use log::debug;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindowBuilder};

// NEW: Add macOS-specific imports
#[cfg(target_os = "macos")]
use log::info;  // Add info! for panel logging

#[cfg(target_os = "macos")]
use tauri::WebviewUrl;

#[cfg(target_os = "macos")]
use tauri_nspanel::{tauri_panel, CollectionBehavior, ManagerExt, PanelBuilder, PanelLevel};

// NEW: Define panel type
#[cfg(target_os = "macos")]
tauri_panel! {
    panel!(RecordingOverlayPanel {
        config: {
            can_become_key_window: false,
            is_floating_panel: true
        }
    })
}

const OVERLAY_WIDTH: f64 = 172.0;
// ... rest of file unchanged
```

**Testing:**
```bash
cargo check
bun run tauri dev
```

**Success Criteria:**
- [ ] App compiles without errors or warnings
- [ ] App works exactly as before
- [ ] Panel macro defined but not used
- [ ] No behavior changes

**Rollback:**
```bash
git checkout src-tauri/src/overlay.rs
```

---

### Phase 4: Create Panel on macOS (First Behavior Change)

**Goal:** Create NSPanel instead of NSWindow on macOS only

**Files Modified:**
- `src-tauri/src/overlay.rs`

**Changes:**

1. Wrap existing `create_recording_overlay()` with platform attribute:
```rust
/// Creates the recording overlay window and keeps it hidden by default
#[cfg(not(target_os = "macos"))]  // NEW: Only for Windows/Linux
pub fn create_recording_overlay(app_handle: &AppHandle) {
    // ... existing code unchanged
}
```

2. Add new macOS version after it:
```rust
/// Creates the recording overlay panel (macOS only) and keeps it hidden by default
#[cfg(target_os = "macos")]
pub fn create_recording_overlay(app_handle: &AppHandle) {
    info!("[OVERLAY] Creating recording overlay panel (macOS)");

    if let Some((x, y)) = calculate_overlay_position(app_handle) {
        info!("[OVERLAY] Panel position calculated: x={}, y={}", x, y);

        match PanelBuilder::<_, RecordingOverlayPanel>::new(
            app_handle,
            "recording_overlay"
        )
        .url(WebviewUrl::App("src/overlay/index.html".into()))
        .title("Recording")
        .position(tauri::Position::Logical(tauri::LogicalPosition { x, y }))
        .level(PanelLevel::Status)  // Level 25 - appears above most windows
        .size(tauri::Size::Logical(tauri::LogicalSize {
            width: OVERLAY_WIDTH,
            height: OVERLAY_HEIGHT
        }))
        .has_shadow(false)
        .transparent(true)
        .no_activate(true)  // Don't steal focus when shown
        .collection_behavior(
            CollectionBehavior::new()
                .can_join_all_spaces()      // Appears in all Mission Control spaces
                .full_screen_auxiliary()     // Works alongside fullscreen apps
        )
        .build()
        {
            Ok(panel) => {
                // Panel starts visible by default, explicitly hide it
                let _ = panel.hide();
                info!("[OVERLAY] Panel created successfully and hidden");
            }
            Err(e) => {
                error!("[OVERLAY] Failed to create panel: {}", e);
            }
        }
    } else {
        warn!("[OVERLAY] Could not calculate overlay position");
    }
}
```

**Testing:**
```bash
bun run tauri dev
```

**Expected Logs:**
```
[OVERLAY] Initializing tauri-nspanel plugin
[OVERLAY] Creating recording overlay panel (macOS)
[OVERLAY] Panel position calculated: x=874, y=1034
[OVERLAY] Panel created successfully and hidden
```

**Manual Tests:**
1. App starts without crashes
2. Check logs show panel creation
3. Try recording (will probably fail - that's expected)
4. Check that app doesn't crash

**Success Criteria:**
- [x] App starts without crashes
- [x] Logs show panel creation at startup
- [x] Panel created but hidden (not visible)
- [x] Recording works (overlay shows and hides correctly)

**COMPLETED - Session 2 (2025-11-19)**

**Known Issue:**
- Settings window is pushed behind other windows on startup (flashes briefly on top, then goes behind)
- Simple `.set_focus()` fix did not resolve this
- **TODO:** Investigate deeper fix after completing all phases
  - Possible causes: NSPanel creation affecting window ordering, timing issue
  - May need to delay panel creation or use different approach

**Common Issues:**
- Compilation error about `CollectionBehavior` methods → Verify using `.new()` builder pattern
- Panel visible on startup → Check `.hide()` is called after build
- App crashes → Check all method calls are correct

**Rollback:**
```bash
git checkout src-tauri/src/overlay.rs
```

---

### Phase 5-9: OBSOLETE - DO NOT IMPLEMENT

**These phases are NO LONGER NEEDED.**

The original plan incorrectly assumed all overlay functions needed platform-conditional rewrites. In reality:
- Only `create_recording_overlay()` (Phase 4) needs to be platform-conditional
- Phases 5-9 attempted to rewrite `show_recording_overlay()`, `hide_recording_overlay()`, `show_transcribing_overlay()`, `update_overlay_position()`, and `emit_levels()` with platform-specific logic
- This caused crashes and instability
- The standard `get_webview_window()` API works for both panels and windows

**If you're reading this:** Skip directly from Phase 4 to Phase 10 (Fullscreen Testing).

---

### ~~Phase 5: Show Panel on macOS (Make Recording Work)~~ [OBSOLETE]

**Goal:** Show panel properly with recreation fallback when recording starts

**Files Modified:**
- `src-tauri/src/overlay.rs`

**Changes:**

1. Wrap existing `show_recording_overlay()`:
```rust
/// Shows the recording overlay window with fade-in animation
#[cfg(not(target_os = "macos"))]
pub fn show_recording_overlay(app_handle: &AppHandle) {
    // ... existing code unchanged
}
```

2. Add macOS version:
```rust
/// Shows the recording overlay panel (macOS) with fade-in animation
#[cfg(target_os = "macos")]
pub fn show_recording_overlay(app_handle: &AppHandle) {
    info!("[OVERLAY] show_recording_overlay called");

    // Check if overlay should be shown based on position setting
    let settings = settings::get_settings(app_handle);
    if settings.overlay_position == OverlayPosition::None {
        info!("[OVERLAY] Overlay position is None, not showing");
        return;
    }

    // Try to get existing panel
    if let Ok(overlay_panel) = app_handle.get_webview_panel("recording_overlay") {
        info!("[OVERLAY] Found existing panel, showing it");

        // Update position in case monitor changed
        update_overlay_position(app_handle);

        // Show the panel
        overlay_panel.show();

        // Emit event to trigger fade-in animation with recording state
        if let Some(window) = overlay_panel.to_window() {
            let _ = window.emit("show-overlay", "recording");
            info!("[OVERLAY] Panel shown with recording state");
        } else {
            warn!("[OVERLAY] Could not convert panel to window for emit");
        }
    } else {
        // Panel doesn't exist (destroyed by macOS), recreate it
        warn!("[OVERLAY] Panel not found, recreating");
        create_recording_overlay(app_handle);

        // Try again to show the newly created panel
        if let Ok(overlay_panel) = app_handle.get_webview_panel("recording_overlay") {
            info!("[OVERLAY] Recreated panel, showing it");
            overlay_panel.show();
            if let Some(window) = overlay_panel.to_window() {
                let _ = window.emit("show-overlay", "recording");
            }
        } else {
            error!("[OVERLAY] Failed to recreate panel");
        }
    }
}
```

**Testing:**
```bash
bun run tauri dev
```

**Manual Tests:**
1. Start app
2. Trigger recording (keyboard shortcut)
3. Panel should appear at bottom/top of screen
4. Panel should show "recording" animation
5. Try multiple times

**Expected Logs When Recording:**
```
[OVERLAY] show_recording_overlay called
[OVERLAY] Found existing panel, showing it
[OVERLAY] Panel shown with recording state
```

**Or if panel was destroyed:**
```
[OVERLAY] show_recording_overlay called
[OVERLAY] Panel not found, recreating
[OVERLAY] Creating recording overlay panel (macOS)
[OVERLAY] Panel position calculated: x=874, y=1034
[OVERLAY] Panel created successfully and hidden
[OVERLAY] Recreated panel, showing it
```

**Success Criteria:**
- [x] Panel appears when recording starts
- [x] Panel is pill-shaped, transparent, no window chrome
- [x] Panel shows recording animation (audio bars)
- [x] Panel positioned correctly (center bottom or top)
- [x] Panel appears consistently across multiple record attempts

**COMPLETED - Session 2 (2025-11-19)**

**Common Issues:**
- Panel has window chrome → Panels are borderless by default, check no extra styling
- Panel doesn't appear → Check logs for errors, verify `.show()` is called
- Wrong position → Check `update_overlay_position()` is called
- No animation → Check emit is working via `.to_window()`

**Rollback:**
```bash
git checkout src-tauri/src/overlay.rs
```

---

### Phase 6: Hide and Destroy Panel on macOS

**Goal:** Hide panel when recording stops and destroy it to prevent crashes

**Files Modified:**
- `src-tauri/src/overlay.rs`

**Changes:**

1. Wrap existing `hide_recording_overlay()`:
```rust
/// Hides the recording overlay window with fade-out animation
#[cfg(not(target_os = "macos"))]
pub fn hide_recording_overlay(app_handle: &AppHandle) {
    // ... existing code unchanged
}
```

2. Add macOS version:
```rust
/// Hides the recording overlay panel (macOS) with fade-out animation and destroys it
#[cfg(target_os = "macos")]
pub fn hide_recording_overlay(app_handle: &AppHandle) {
    info!("[OVERLAY] hide_recording_overlay called");

    // Always hide the overlay regardless of settings
    // (if setting was changed while recording, we still want to hide it properly)
    if let Ok(overlay_panel) = app_handle.get_webview_panel("recording_overlay") {
        info!("[OVERLAY] Found panel to hide");

        // Emit event to trigger fade-out animation
        if let Some(window) = overlay_panel.to_window() {
            let _ = window.emit("hide-overlay", ());
            info!("[OVERLAY] Emitted hide event to panel");
        }

        // Hide and destroy the panel after animation completes
        let panel_clone = overlay_panel.clone();
        std::thread::spawn(move || {
            // Wait for fade-out animation (300ms)
            std::thread::sleep(std::time::Duration::from_millis(300));

            // Hide the panel first
            let _ = panel_clone.hide();

            // CRITICAL: Destroy the panel by closing its window
            // This prevents crashes when switching Mission Control spaces
            if let Some(window) = panel_clone.to_window() {
                let _ = window.close();
                info!("[OVERLAY] Panel hidden and destroyed");
            } else {
                warn!("[OVERLAY] Could not convert panel to window for destruction");
            }
        });
    } else {
        info!("[OVERLAY] No panel found to hide");
    }
}
```

**Testing:**
```bash
bun run tauri dev
```

**Manual Tests:**
1. Start recording
2. Wait for panel to appear
3. Stop recording
4. Panel should fade out and disappear
5. Try multiple record/stop cycles
6. Verify no "ghost panels" stay visible

**Expected Logs:**
```
[OVERLAY] hide_recording_overlay called
[OVERLAY] Found panel to hide
[OVERLAY] Emitted hide event to panel
[OVERLAY] Panel hidden and destroyed
```

**Success Criteria:**
- [ ] Panel fades out when recording stops
- [ ] Panel completely disappears after 300ms
- [ ] No panels stay visible after recording
- [ ] Multiple record/stop cycles work reliably
- [ ] No crashes

**Common Issues:**
- Panel stays visible → Verify `window.close()` is being called
- No fade animation → Check 300ms delay, verify emit is working
- Crashes → Verify panel is being destroyed in spawned thread

**Rollback:**
```bash
git checkout src-tauri/src/overlay.rs
```

---

### Phase 7: Update `show_transcribing_overlay()` for macOS

**Goal:** Show "transcribing" state when transcription is processing

**Files Modified:**
- `src-tauri/src/overlay.rs`

**Changes:**

1. Wrap existing function:
```rust
/// Shows the transcribing overlay window
#[cfg(not(target_os = "macos"))]
pub fn show_transcribing_overlay(app_handle: &AppHandle) {
    // ... existing code unchanged
}
```

2. Add macOS version:
```rust
/// Shows the transcribing overlay panel (macOS)
#[cfg(target_os = "macos")]
pub fn show_transcribing_overlay(app_handle: &AppHandle) {
    info!("[OVERLAY] show_transcribing_overlay called");

    // Check if overlay should be shown based on position setting
    let settings = settings::get_settings(app_handle);
    if settings.overlay_position == OverlayPosition::None {
        info!("[OVERLAY] Overlay position is None, not showing");
        return;
    }

    // Update position in case monitor changed
    update_overlay_position(app_handle);

    if let Ok(overlay_panel) = app_handle.get_webview_panel("recording_overlay") {
        // Panel exists, show it
        let _ = overlay_panel.show();

        // Emit event to switch to transcribing state
        if let Some(window) = overlay_panel.to_window() {
            let _ = window.emit("show-overlay", "transcribing");
            info!("[OVERLAY] Panel switched to transcribing state");
        }
    } else {
        warn!("[OVERLAY] Panel not found when showing transcribing state");
    }
}
```

**Testing:**
```bash
bun run tauri dev
```

**Manual Tests:**
1. Start recording and speak
2. Stop recording
3. Watch for panel transition to "transcribing" state
4. Text should appear instead of audio bars

**Expected Logs:**
```
[OVERLAY] show_transcribing_overlay called
[OVERLAY] Panel switched to transcribing state
```

**Success Criteria:**
- [ ] Panel transitions from "recording" to "transcribing"
- [ ] Visual state changes (audio bars → text)
- [ ] Panel remains visible during transcription

**Rollback:**
```bash
git checkout src-tauri/src/overlay.rs
```

---

### Phase 8: Update `update_overlay_position()` for macOS

**Goal:** Reposition panel when settings change or monitor changes

**Files Modified:**
- `src-tauri/src/overlay.rs`

**Changes:**

1. Wrap existing function:
```rust
/// Updates the overlay window position based on current settings
#[cfg(not(target_os = "macos"))]
pub fn update_overlay_position(app_handle: &AppHandle) {
    // ... existing code unchanged
}
```

2. Add macOS version:
```rust
/// Updates the overlay panel position (macOS) based on current settings
#[cfg(target_os = "macos")]
pub fn update_overlay_position(app_handle: &AppHandle) {
    if let Ok(overlay_panel) = app_handle.get_webview_panel("recording_overlay") {
        if let Some((x, y)) = calculate_overlay_position(app_handle) {
            if let Some(window) = overlay_panel.to_window() {
                let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
                debug!("[OVERLAY] Panel position updated: x={}, y={}", x, y);
            }
        }
    }
}
```

**Testing:**
```bash
bun run tauri dev
```

**Manual Tests:**
1. Start recording
2. Change overlay position setting (top ↔ bottom) while recording
3. Panel should move to new position
4. Try with multiple monitors if available

**Success Criteria:**
- [ ] Panel moves when position setting changes
- [ ] Panel tracks correct monitor (where cursor is)
- [ ] Position updates work during recording

**Rollback:**
```bash
git checkout src-tauri/src/overlay.rs
```

---

### Phase 9: Update `emit_levels()` for macOS

**Goal:** Send microphone levels to panel for audio visualization

**Files Modified:**
- `src-tauri/src/overlay.rs`

**Changes:**

1. Wrap existing function:
```rust
#[cfg(not(target_os = "macos"))]
pub fn emit_levels(app_handle: &AppHandle, levels: &Vec<f32>) {
    // ... existing code unchanged
}
```

2. Add macOS version:
```rust
#[cfg(target_os = "macos")]
pub fn emit_levels(app_handle: &AppHandle, levels: &Vec<f32>) {
    // Emit levels to main app
    let _ = app_handle.emit("mic-level", levels);

    // Also emit to the recording overlay panel if it's open
    if let Ok(overlay_panel) = app_handle.get_webview_panel("recording_overlay") {
        if let Some(window) = overlay_panel.to_window() {
            let _ = window.emit("mic-level", levels);
        }
    }
}
```

**Testing:**
```bash
bun run tauri dev
```

**Manual Tests:**
1. Start recording
2. Speak into microphone
3. Watch audio bars animate in panel
4. Bars should move with voice amplitude

**Success Criteria:**
- [ ] Audio visualization works in panel
- [ ] Bars animate smoothly
- [ ] Levels reflect actual microphone input

**Rollback:**
```bash
git checkout src-tauri/src/overlay.rs
```

---

### Phase 10: Fullscreen Testing (Main Goal)

**Goal:** Verify panel appears above fullscreen applications

**No Code Changes - Testing Only**

**Test Suite:**

1. **Safari Fullscreen:**
   - Open Safari
   - Enter fullscreen (Cmd+Ctrl+F or green button)
   - Start recording
   - **Verify:** Panel appears ABOVE fullscreen content
   - Stop recording
   - Exit fullscreen

2. **Chrome Fullscreen:**
   - Open Chrome
   - Enter fullscreen
   - Start recording
   - **Verify:** Panel visible above fullscreen
   - Stop recording

3. **Video Player Fullscreen:**
   - Open QuickTime or VLC
   - Play a video in fullscreen
   - Start recording
   - **Verify:** Panel visible above video
   - Stop recording

4. **YouTube Fullscreen:**
   - Open YouTube in browser
   - Make video fullscreen
   - Start recording
   - **Verify:** Panel visible
   - Stop recording

**Expected Logs:**
```
[OVERLAY] show_recording_overlay called
[OVERLAY] Found existing panel, showing it
[OVERLAY] Panel shown with recording state
```

**Success Criteria:**
- [ ] ✅ Panel appears in Safari fullscreen
- [ ] ✅ Panel appears in Chrome fullscreen
- [ ] ✅ Panel appears over fullscreen videos
- [ ] ✅ Panel positioned correctly (not behind menu bar)
- [ ] ✅ Panel is readable (not obscured)

**If Panel NOT Visible in Fullscreen:**

Try increasing panel level in Phase 4's code:

```rust
// Change from:
.level(PanelLevel::Status)  // Level 25

// To:
.level(PanelLevel::ScreenSaver)  // Level 1000 - above everything
```

Rebuild and test again:
```bash
bun run tauri dev
```

**Trade-offs of Higher Levels:**
- `Status` (25): Works with most fullscreen apps, more stable
- `ScreenSaver` (1000): Works with ALL fullscreen apps, may be destroyed more often by macOS

---

### Phase 11: Multi-Space Stability Testing

**Goal:** Verify no crashes when switching Mission Control spaces

**No Code Changes - Testing Only**

**Setup:**
1. Open Mission Control (F3 or swipe up with 3 fingers)
2. Create 3 spaces:
   - Space 1: Normal desktop
   - Space 2: Normal desktop with different apps
   - Space 3: Safari in fullscreen

**Test Suite:**

1. **Basic Space Switching:**
   - Start in Space 1
   - Start recording
   - Verify panel appears
   - Switch to Space 2 (Ctrl+→)
   - **Verify:** Panel appears in Space 2
   - Switch to Space 3
   - **Verify:** Panel appears above fullscreen Safari
   - Switch back to Space 1
   - Stop recording
   - **Verify:** No crashes

2. **Rapid Space Switching:**
   - Start recording in Space 1
   - Rapidly switch: 1 → 2 → 3 → 2 → 1
   - **Verify:** No crashes, panel follows

3. **Recording Across Spaces:**
   - Start recording in Space 1
   - Switch to Space 2, keep recording
   - Switch to Space 3 (fullscreen), keep recording
   - Stop recording in Space 3
   - **Verify:** Panel disappears, no crashes

4. **Space Switching While Hiding:**
   - Start recording in Space 1
   - Stop recording (panel starting to fade)
   - Immediately switch to Space 2
   - **Verify:** No crashes during fade/destroy

**Expected Logs:**
```
[OVERLAY] show_recording_overlay called
[OVERLAY] Found existing panel, showing it
[OVERLAY] Panel shown with recording state
# ... space switches ...
[OVERLAY] hide_recording_overlay called
[OVERLAY] Found panel to hide
[OVERLAY] Emitted hide event to panel
[OVERLAY] Panel hidden and destroyed
```

**Success Criteria:**
- [ ] ✅ Panel appears in all spaces
- [ ] ✅ Panel visible above Space 3 fullscreen app
- [ ] ✅ No crashes during space transitions
- [ ] ✅ Panel properly destroyed after recording
- [ ] ✅ No "ghost panels" in any space

**If Crashes Occur:**

Check that Phase 6 properly destroys the panel:
```rust
// Verify this code exists in hide_recording_overlay:
if let Some(window) = panel_clone.to_window() {
    let _ = window.close();  // CRITICAL for preventing crashes
    info!("[OVERLAY] Panel hidden and destroyed");
}
```

**Common Crash Patterns:**
- Crash when switching TO fullscreen space → Panel level too high, try `Status` instead of `ScreenSaver`
- Crash when switching FROM fullscreen space → Panel not being destroyed properly
- Crash after recording ends → Check 300ms delay before destroy

---

### Phase 12: Stress Testing

**Goal:** Find edge case bugs before considering complete

**No Code Changes - Testing Only**

**Test Suite:**

1. **Rapid Start/Stop Cycles:**
   - Start recording
   - Immediately stop
   - Repeat 10 times rapidly
   - **Verify:** No crashes, panel appears/disappears correctly each time

2. **Settings Changes During Recording:**
   - Start recording
   - Open settings
   - Change overlay position (top ↔ bottom)
   - **Verify:** Panel moves to new position
   - Change to "None"
   - **Verify:** Panel disappears
   - Change back to "Bottom"
   - **Verify:** Panel reappears

3. **Window Focus Changes:**
   - Start recording
   - Click settings window
   - **Verify:** Panel stays visible
   - Click other apps
   - **Verify:** Panel stays on top

4. **Monitor Changes (if multiple monitors):**
   - Start recording on monitor 1
   - Move cursor to monitor 2
   - Start new recording
   - **Verify:** Panel appears on monitor 2

5. **Fullscreen Entry/Exit During Recording:**
   - Start recording (not fullscreen)
   - Enter fullscreen in Safari
   - **Verify:** Panel appears above fullscreen
   - Exit fullscreen
   - **Verify:** Panel still visible
   - Stop recording

6. **Long Recording Sessions:**
   - Start recording
   - Wait 60 seconds
   - Switch spaces multiple times
   - **Verify:** Panel stable
   - Stop recording

**Expected Behavior:**
- Panel creation logs on first show
- Panel recreation logs if macOS destroys it
- Clean hide/destroy logs when recording stops
- No error logs or crashes

**Success Criteria:**
- [ ] ✅ Handles rapid start/stop without crashes
- [ ] ✅ Setting changes work during recording
- [ ] ✅ Panel stays on top when clicking other windows
- [ ] ✅ Multi-monitor support works
- [ ] ✅ Fullscreen transitions don't cause issues
- [ ] ✅ Long sessions are stable

---

## Complete Testing Checklist

Before considering this task complete, verify:

### Basic Functionality
- [ ] App compiles without errors on macOS
- [ ] App starts without crashes
- [ ] Panel created but hidden on startup
- [ ] Recording shortcut shows panel
- [ ] Panel has correct styling (pill shape, transparent, no chrome)
- [ ] Recording animation works (audio bars)
- [ ] Transcribing state works (text display)
- [ ] Panel hides when recording ends
- [ ] Fade in/out animations work

### Fullscreen Support (Primary Goal)
- [ ] Panel visible in Safari fullscreen
- [ ] Panel visible in Chrome fullscreen
- [ ] Panel visible over fullscreen videos
- [ ] Panel positioned correctly in fullscreen
- [ ] Panel readable and not obscured

### Multi-Space Support
- [ ] Panel appears in all Mission Control spaces
- [ ] Panel follows when switching spaces during recording
- [ ] No crashes when switching spaces
- [ ] Panel appears above fullscreen apps in different spaces

### Stability
- [ ] No crashes during normal operation
- [ ] No crashes during space transitions
- [ ] No crashes during fullscreen entry/exit
- [ ] No "ghost panels" staying visible
- [ ] Panel properly destroyed after each recording

### Performance
- [ ] Panel creation < 100ms
- [ ] No noticeable lag when showing/hiding
- [ ] Smooth animations

### Windows/Linux Compatibility
- [ ] Code still compiles on Windows
- [ ] Code still compiles on Linux
- [ ] Overlay still works on Windows
- [ ] Overlay still works on Linux

## Files Modified Summary

1. **src-tauri/Cargo.toml** - Added tauri-nspanel dependency (Phase 1)
2. **src-tauri/src/lib.rs** - Initialize plugin (Phase 2)
3. **src-tauri/src/overlay.rs** - Complete rewrite with conditional compilation (Phases 3-9)

**No Changes Needed:**
- Frontend code (src/overlay/RecordingOverlay.tsx)
- Settings (settings work identically)
- Other Rust modules
- Build configuration

## API Reference

### Panel Creation Pattern

```rust
use tauri_nspanel::{tauri_panel, CollectionBehavior, ManagerExt, PanelBuilder, PanelLevel};

// Define panel type
tauri_panel! {
    panel!(MyPanel {
        config: {
            can_become_key_window: false,
            is_floating_panel: true
        }
    })
}

// Create panel
PanelBuilder::<_, MyPanel>::new(app_handle, "panel_id")
    .url(WebviewUrl::App("path.html".into()))
    .level(PanelLevel::Status)
    .collection_behavior(
        CollectionBehavior::new()
            .can_join_all_spaces()
            .full_screen_auxiliary()
    )
    .build()
```

### Panel Access Pattern

```rust
// Get panel - returns Result, not Option
if let Ok(panel) = app_handle.get_webview_panel("panel_id") {
    // Direct panel methods
    panel.show();
    panel.hide();

    // Window methods require .to_window() conversion
    if let Some(window) = panel.to_window() {
        window.emit("event", data);
        window.set_position(...);
        window.close();  // Destroys the panel
    }
}
```

### CollectionBehavior Methods (Builder Pattern)

```rust
CollectionBehavior::new()
    .can_join_all_spaces()      // Appears in all Mission Control spaces
    .full_screen_auxiliary()     // Works alongside fullscreen windows
```

**NOT** enum constants:
- ❌ `CollectionBehavior::CAN_JOIN_ALL_SPACES`
- ❌ `CollectionBehavior::FULL_SCREEN_AUXILIARY`

### PanelLevel Enum Variants

```rust
PanelLevel::Floating        // Level 4 (not enough for fullscreen)
PanelLevel::Status          // Level 25 (recommended for fullscreen)
PanelLevel::ModalPanel      // Level 8 (between floating and status)
PanelLevel::ScreenSaver     // Level 1000 (highest, use if Status insufficient)
```

### PanelBuilder Methods

**Available:**
- `.url()` - Set webview URL
- `.title()` - Set panel title
- `.position()` - Set initial position
- `.level()` - Set window level
- `.size()` - Set panel size (NOT `.inner_size()`)
- `.has_shadow()` - Enable/disable shadow (NOT `.shadow()`)
- `.transparent()` - Enable transparency
- `.no_activate()` - Prevent focus stealing
- `.collection_behavior()` - Set NSPanel behaviors
- `.build()` - Create the panel

**NOT Available (vs WebviewWindowBuilder):**
- ❌ `.resizable()` - Panels have fixed size
- ❌ `.maximizable()`, `.minimizable()`, `.closable()` - Not applicable
- ❌ `.decorations()` - Panels are borderless by default
- ❌ `.skip_taskbar()` - Panels don't appear in Dock
- ❌ `.accept_first_mouse()` - Use `.no_activate(true)`
- ❌ `.focused()` - Not available
- ❌ `.visible()` - Control with `.show()`/`.hide()`
- ❌ `.inner_size()` - Use `.size()`
- ❌ `.shadow()` - Use `.has_shadow()`

### Panel Trait Methods

```rust
panel.show()              // Show the panel (returns ())
panel.hide()              // Hide the panel (returns ())
panel.clone()             // Clone panel reference
panel.to_window()         // Convert to Window (returns Option<Window>)
```

**NOT Available:**
- ❌ `panel.close()` - Must convert to window first
- ❌ `panel.emit()` - Must convert to window first
- ❌ `panel.set_position()` - Must convert to window first

## Known Issues & Solutions

### Issue: Panel Stays Visible After Recording
**Cause:** Panel not destroyed, persists at high window level
**Solution:** Call `window.close()` in `hide_recording_overlay()`
**Code:**
```rust
if let Some(window) = panel_clone.to_window() {
    let _ = window.close();  // CRITICAL
}
```

### Issue: Crash When Switching Spaces
**Cause:** Persistent panel at high level conflicts with space management
**Solution:** Destroy panel after hiding (same fix as above)
**Prevention:** Always destroy in `hide_recording_overlay()`

### Issue: "Failed to get overlay panel"
**Cause:** macOS destroyed panel, `get_webview_panel()` returns error
**Solution:** Recreation fallback in `show_recording_overlay()`
**Code:**
```rust
if let Ok(panel) = app_handle.get_webview_panel("recording_overlay") {
    // Use existing
} else {
    // Recreate
    create_recording_overlay(app_handle);
}
```

### Issue: Compilation Error - Constants Not Found
**Cause:** Trying to use `CollectionBehavior::CAN_JOIN_ALL_SPACES` etc.
**Solution:** Use builder pattern: `CollectionBehavior::new().can_join_all_spaces()`

### Issue: Panel Has Window Chrome
**Cause:** Unnecessary style configuration
**Solution:** Remove `.style_mask()` - panels are borderless by default

### Issue: Panel Not Visible in Fullscreen
**Cause:** Panel level too low
**Solution:** Try `PanelLevel::ScreenSaver` (1000) instead of `Status` (25)
**Trade-off:** Higher levels may be destroyed more often by macOS

### Issue: Random Crashes (No Stack Trace)
**Cause:** Panel not destroyed before space transition
**Solution:** Verify `window.close()` is called in destroy thread
**Debugging:** Add extensive logging around hide/destroy

### Issue: App Hangs on Rapid Toggle (Session 3)
**Cause:** Race condition between `hide()` and `update_overlay_position()`
**Symptom:** When rapidly toggling overlay on/off, app hangs (beach ball). Logs show `update_overlay_position()` called but never completing.
**Root Cause:**
- `hide()` called on window
- Immediately after, `show()` + `update_overlay_position()` called
- `update_overlay_position()` tries to get window and reposition it while it's mid-hide
- This creates a deadlock - window is in inconsistent state
**Solution:** Inline position update directly in `show_recording_overlay()` AFTER `show()` call
**Code:**
```rust
pub fn show_recording_overlay(app_handle: &AppHandle) {
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        let _ = overlay_window.show();

        // Update position AFTER showing to avoid race condition with hide()
        if let Some((x, y)) = calculate_overlay_position(app_handle) {
            let _ = overlay_window.set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
        }

        let _ = overlay_window.emit("show-overlay", "recording");
    }
}
```
**Prevention:** Don't call separate position update function that accesses window - inline the logic to ensure proper sequencing

## Performance Notes

- Panel creation: ~50ms (measured in Session 1)
- Panel destruction: ~300ms (includes animation delay)
- Transcription: 77-104ms (existing baseline)
- **Verdict:** Recreation overhead is acceptable

## Lessons Learned (Session 1)

### API Misunderstandings
1. **Builder Pattern**: tauri-nspanel uses methods, not constants
   - Wrong: `CollectionBehavior::CAN_JOIN_ALL_SPACES`
   - Right: `CollectionBehavior::new().can_join_all_spaces()`

2. **Panel Methods**: Panel trait ≠ Window trait
   - Must use `.to_window()` for emit/position operations
   - `.close()` only exists on Window, not Panel

3. **Return Types**:
   - `get_webview_panel()` returns `Result<Panel>` not `Option`
   - `.show()` returns `()` not `Result`

### macOS Behavior
1. **Panel Lifecycle**: macOS destroys high-level panels unpredictably
   - Always implement recreation fallback
   - Don't rely on persistent panels

2. **Space Transitions**: Persistent high-level panels cause crashes
   - Must destroy panel (not just hide) to prevent crashes
   - Crash manifests as random failures, not specific errors

3. **Window Chrome**: Panels are borderless by default
   - No need for `.style_mask()` configuration
   - Adding `.style_mask()` may cause issues

### Debugging Tips
- Log levels critical: `info!()` for lifecycle events
- Crashes may not show stack traces (space transition crashes)
- Test in fullscreen Safari/Chrome, not just dev mode
- Test space switching extensively - most crash-prone scenario
- Use `[OVERLAY]` prefix consistently for easy log filtering
- **Race Conditions**: If a function's logs show it starting but never completing internal steps, suspect deadlock from concurrent window operations

## Lessons Learned (Session 3)

### Race Conditions in Window Operations
1. **Window State Transitions**: Window operations like `hide()` and `show()` may not be atomic
   - Don't assume window is immediately ready after `hide()` or `show()` call
   - Accessing window properties during state transition can cause deadlocks

2. **Function Sequencing**: When calling multiple operations on same window, inline them instead of separate function calls
   - Wrong: `show_window()` then `update_position()` (two separate window accesses)
   - Right: `show_window()` { show, then update position inline } (single coordinated access)

3. **Rapid Toggling Detection**: Look for logs showing function entry but no internal completion logs
   - Example: Function logs "starting" but never logs "position updated" or "operation completed"
   - This indicates the function blocked/deadlocked on a window operation

4. **Dev Mode vs Production**: Race conditions may be more apparent in dev mode due to debug overhead
   - But don't assume it's "just dev mode" - fix the race condition properly
   - The fix should work in both environments

## References

- tauri-nspanel plugin: https://github.com/ahkohd/tauri-nspanel
- Cap (production example): https://github.com/CapSoftware/Cap
- Tauri Issue #5793: Show window on top of full-screen app
- Tauri Issue #11488: visibleOnAllWorkspaces not working with fullscreen
- macOS Window Levels: https://developer.apple.com/documentation/appkit/nswindowlevel
- NSWindow.CollectionBehavior: https://developer.apple.com/documentation/appkit/nswindow/collectionbehavior
- Stack Overflow - NSPanel above fullscreen: https://stackoverflow.com/questions/36205834/allow-an-nswindow-nspanel-to-float-above-full-screen-apps

## Known Issues to Fix

### Settings Window Focus Issue (Discovered in Phase 4)

**Problem:** Settings window is pushed behind other application windows on startup
- Window flashes briefly on top (as expected)
- Then immediately goes behind other windows
- Overlay functionality works correctly, this is just a focus issue

**Attempted Fix:** Simple `.set_focus()` call after panel creation - did not work

**Possible Solutions to Try:**
1. Delay panel creation until after main window is fully shown and focused
2. Create panel in background thread
3. Use different NSPanel configuration that doesn't affect window ordering
4. Explicitly set activation policy after panel creation
5. Check if panel's `is_floating_panel: true` config is causing the issue

**Priority:** Medium - doesn't affect core functionality but impacts UX

---

## Success Criteria (Final)

1. ✅ Panel appears above fullscreen applications on macOS
2. ✅ Panel appears in all Mission Control spaces when recording
3. ✅ No crashes when switching spaces
4. ✅ Visual parity with Windows/Linux overlay
5. ✅ Panel properly hides after recording
6. ✅ Performance acceptable (<100ms panel recreation)
7. ✅ All stress tests pass
8. ✅ Windows/Linux builds still work
