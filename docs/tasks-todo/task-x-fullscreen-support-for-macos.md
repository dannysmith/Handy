# Task: Fullscreen Support for macOS

## Problem Statement

Handy shows no visual feedback when transcription is active in macOS fullscreen apps, unlike alternatives (Whispr Flow, Voiceink etc).

**Related Issues:**
- https://github.com/cjpais/Handy/issues/277
- https://github.com/cjpais/Handy/issues/278

**PR:** https://github.com/dannysmith/Handy/pull/1

> [!WARNING]
> Before opening PR to upstream repo, revert dccb55b979a20973e7f96d2253136bccc268dd2a

## Current Status

### ✅ Completed
- NSPanel integration via tauri-nspanel plugin
- Overlay appears above fullscreen applications
- Works across Mission Control spaces
- No crashes during space transitions
- Visual styling mostly correct (pill-shaped, transparent)
- Race condition fix for rapid toggling

### 🔧 Next Steps

1. **Visual Styling Polish** (Current Priority)
   - Ensure macOS overlay looks identical to Windows/Linux
   - User will provide screenshots for comparison
   - May need CSS adjustments to match original appearance

2. **Final Testing & Documentation**
   - Production build testing
   - Update user-facing documentation
   - Clean up any remaining logging

### Known Limitations

**Audio Initialization Hang on Very Rapid Toggle:**
- OUT OF SCOPE for this task
- Affects only extreme edge case (intentional rapid spam of shortcut)
- This is an audio pipeline issue, not overlay issue
- Should be addressed in separate task focused on audio system robustness

## Technical Implementation

### The Simple Solution

After multiple iterations, the working implementation is surprisingly simple:

**Only `create_recording_overlay()` needs to be platform-conditional.**

All other overlay functions (`show_recording_overlay`, `hide_recording_overlay`, `show_transcribing_overlay`, `update_overlay_position`, `emit_levels`) work identically for both NSPanel (macOS) and NSWindow (Windows/Linux) via the standard `get_webview_window()` API.

**Why This Works:**
- tauri-nspanel registers panels in Tauri's window manager
- Panels are accessible via `get_webview_window()` just like regular windows
- No need for platform-conditional logic in show/hide functions
- No need to use `get_webview_panel()` or `.to_window()` conversions

### Code Changes Summary

**Modified Files:**
1. `src-tauri/Cargo.toml` - Added tauri-nspanel dependency
2. `src-tauri/src/lib.rs` - Initialize tauri-nspanel plugin on macOS
3. `src-tauri/src/overlay.rs` - Platform-conditional `create_recording_overlay()` implementation

**Key Implementation (overlay.rs):**

```rust
// macOS-specific imports
#[cfg(target_os = "macos")]
use tauri_nspanel::{tauri_panel, CollectionBehavior, PanelBuilder, PanelLevel};

// Define panel type for macOS
#[cfg(target_os = "macos")]
tauri_panel! {
    panel!(RecordingOverlayPanel {
        config: {
            can_become_key_window: false,
            is_floating_panel: true
        }
    })
}

// Windows/Linux: Standard NSWindow
#[cfg(not(target_os = "macos"))]
pub fn create_recording_overlay(app_handle: &AppHandle) {
    // ... existing WebviewWindowBuilder code ...
}

// macOS: NSPanel with special configuration
#[cfg(target_os = "macos")]
pub fn create_recording_overlay(app_handle: &AppHandle) {
    if let Some((x, y)) = calculate_overlay_position(app_handle) {
        match PanelBuilder::<_, RecordingOverlayPanel>::new(
            app_handle,
            "recording_overlay"
        )
        .url(WebviewUrl::App("src/overlay/index.html".into()))
        .level(PanelLevel::Status)  // Level 25 - appears above fullscreen
        .collection_behavior(
            CollectionBehavior::new()
                .can_join_all_spaces()      // Appears in all spaces
                .full_screen_auxiliary()     // Works with fullscreen apps
        )
        .has_shadow(false)
        .transparent(true)
        .no_activate(true)
        .corner_radius(0.0)  // Remove NSPanel rounded corners
        .build()
        {
            Ok(panel) => {
                let _ = panel.hide();  // Start hidden
            }
            Err(e) => {
                log::error!("[OVERLAY] Failed to create panel: {}", e);
            }
        }
    }
}
```

### Critical Race Condition Fix

**Issue:** Rapid toggling caused app hang (beach ball)

**Root Cause:** `update_overlay_position()` was called as a separate function, creating race condition:
- `hide()` called on window
- Immediately `show()` + `update_overlay_position()` called
- Position update tried to access window during hide operation
- Window in inconsistent state → deadlock

**Solution:** Inline position update directly in `show_recording_overlay()` AFTER `show()` call:

```rust
pub fn show_recording_overlay(app_handle: &AppHandle) {
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        let _ = overlay_window.show();

        // Update position AFTER showing to avoid race condition
        if let Some((x, y)) = calculate_overlay_position(app_handle) {
            let _ = overlay_window.set_position(
                tauri::Position::Logical(tauri::LogicalPosition { x, y })
            );
        }

        let _ = overlay_window.emit("show-overlay", "recording");
    }
}
```

## Testing Checklist

### Basic Functionality
- [x] App compiles on macOS
- [x] Panel appears when recording starts
- [x] Panel hides when recording stops
- [x] Fade in/out animations work
- [x] Audio visualization works (bars animate)
- [x] Transcribing state works

### Fullscreen Support (Primary Goal)
- [x] Panel visible in Safari fullscreen
- [x] Panel visible in Chrome fullscreen
- [x] Panel visible over fullscreen videos
- [x] Panel positioned correctly

### Multi-Space Support
- [x] Panel appears in all Mission Control spaces
- [x] No crashes when switching spaces
- [x] Panel follows during recording

### Stability
- [x] No crashes during normal operation
- [x] No crashes during space transitions
- [x] No "ghost panels" staying visible
- [x] Handles rapid toggling without crashes (except extreme edge case in audio system)

### Visual Parity
- [ ] **TODO:** Ensure macOS overlay matches Windows/Linux appearance exactly

### Cross-Platform
- [x] Windows build still works
- [x] Linux build still works

---

## Additional Context

### Why Standard Tauri Windows Don't Work

On macOS since Big Sur:
- Standard `NSWindow` with `.always_on_top(true)` uses `NSFloatingWindowLevel` (level 3)
- Fullscreen apps run at `NSMainMenuWindowLevel + 1` or higher (level 25+)
- Therefore, standard windows appear **behind** fullscreen content

Only `NSPanel` windows with high window levels can appear above fullscreen apps.

### tauri-nspanel Plugin

**Repository:** https://github.com/ahkohd/tauri-nspanel

**Why This Plugin:**
- Battle-tested in production apps (Cap screen recorder, Overlayed gaming overlay)
- Clean API for creating NSPanel windows in Tauri v2
- Handles all macOS-specific complexity internally
- Minimal code changes required

**Critical API Pattern:**
The plugin uses **builder pattern methods**, NOT enum constants:
- ❌ `CollectionBehavior::CAN_JOIN_ALL_SPACES` - Does not exist
- ✅ `CollectionBehavior::new().can_join_all_spaces()` - Correct

### Panel Levels

```rust
PanelLevel::Floating        // Level 4 (not enough for fullscreen)
PanelLevel::Status          // Level 25 (recommended, what we use)
PanelLevel::ModalPanel      // Level 8 (between floating and status)
PanelLevel::ScreenSaver     // Level 1000 (highest, overkill for our needs)
```

### Collection Behaviors

```rust
CollectionBehavior::new()
    .can_join_all_spaces()      // Appears across all Mission Control spaces
    .full_screen_auxiliary()     // Works alongside fullscreen windows
```

### Implementation Sessions

**Session 1 (2025-11-18):**
- Added dependency and plugin initialization
- Created initial NSPanel implementation
- Multiple iterations debugging API usage

**Session 2 (2025-11-19):**
- Fixed corner radius issue (NSPanel rounded corners)
- Removed unnecessary panel destruction/recreation
- Identified that only `create_recording_overlay()` needs platform-conditional logic

**Session 3 (2025-11-19):**
- Added comprehensive logging
- Fixed race condition in `show_recording_overlay()`
- Identified audio initialization hang as separate issue
- Settings window focus bug fixed as side effect

### Key Lessons Learned

1. **Trust the Library Design:** If implementation requires extensive workarounds, you're probably doing it wrong. The simple solution (only platform-conditional creation) is the correct one.

2. **Race Conditions in Window Operations:** Window state transitions (`hide()`, `show()`) are not atomic. Accessing window properties during transitions can cause deadlocks. Inline related operations instead of separate function calls.

3. **Logging is Critical:** Comprehensive logging with clear prefixes (`[OVERLAY]`, `[ACTION]`) made debugging race conditions possible. Without logs showing function entry without completion, the deadlock would have been much harder to identify.

4. **Builder Patterns vs Enums:** Always check library documentation for API patterns. tauri-nspanel uses builder methods, not enum constants.

### Files Modified

1. **src-tauri/Cargo.toml**
   ```toml
   [target.'cfg(target_os = "macos")'.dependencies]
   tauri-nspanel = { git = "https://github.com/ahkohd/tauri-nspanel", branch = "v2.1" }
   ```

2. **src-tauri/src/lib.rs**
   ```rust
   #[cfg(target_os = "macos")]
   {
       info!("[OVERLAY] Initializing tauri-nspanel plugin");
       builder = builder.plugin(tauri_nspanel::init());
   }
   ```

3. **src-tauri/src/overlay.rs**
   - Added macOS-specific imports
   - Defined `RecordingOverlayPanel` type with `tauri_panel!` macro
   - Split `create_recording_overlay()` into platform-conditional versions
   - Inlined position update in `show_recording_overlay()` to fix race condition
   - Added logging throughout for debugging

### References

- tauri-nspanel plugin: https://github.com/ahkohd/tauri-nspanel
- Cap (production example): https://github.com/CapSoftware/Cap
- Tauri Issue #5793: Show window on top of full-screen app
- Tauri Issue #11488: visibleOnAllWorkspaces not working with fullscreen
- macOS Window Levels: https://developer.apple.com/documentation/appkit/nswindowlevel
- NSWindow.CollectionBehavior: https://developer.apple.com/documentation/appkit/nswindow/collectionbehavior
- Stack Overflow - NSPanel above fullscreen: https://stackoverflow.com/questions/36205834/allow-an-nswindow-nspanel-to-float-above-full-screen-apps
