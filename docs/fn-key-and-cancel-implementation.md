# Implementing fn Key Support and Cancel Shortcut

This document captures how to correctly implement these two features, based on a failed attempt that broke the existing shortcut UI.

## Goals

1. **fn key as trigger (macOS)** - Allow users to use the fn/Globe key to start/stop recording
2. **Escape to cancel recording** - Allow users to cancel mid-recording without transcribing

## Critical Lesson Learned

**DO NOT TOUCH THE EXISTING SHORTCUT UI (`HandyShortcut.tsx`)**

The existing UI has a complex interaction pattern between:

- `suspend_binding` (unregisters shortcut while user records new keys)
- `updateBinding` → `change_binding` (saves and registers new shortcut)
- `resume_binding` (re-registers shortcut)

This pattern has subtle bugs but mostly works. Attempting to "fix" or "simplify" it broke everything. The fn key and cancel features can be implemented **entirely in the backend** with minimal frontend changes.

---

## Feature 1: fn Key Support (macOS)

### How it works

The fn/Globe key is a modifier key that generates `NSEventType::FlagsChanged` events with `NSEventModifierFlags::Function`. Standard shortcut libraries (like `tauri-plugin-global-shortcut`) cannot capture modifier-only keys.

### Implementation approach

**Create a parallel input system for fn key only:**

1. **New file: `src-tauri/src/shortcut/fn_monitor.rs`**
   - Uses `NSEvent::addGlobalMonitorForEventsMatchingMask_handler` from objc2
   - Monitors for `NSEventMask::FlagsChanged` events
   - Checks `NSEventModifierFlags::Function` to detect fn press/release
   - Calls the same `dispatch_binding_event()` function as regular shortcuts

2. **Modify `src-tauri/src/shortcut/mod.rs`** (now `shortcut/mod.rs`):
   - Add `#[cfg(target_os = "macos")] mod fn_monitor;`
   - In `register_binding()`: if binding is "fn", route to `fn_monitor::register_fn_binding()`
   - In `unregister_binding()`: if binding is "fn", route to `fn_monitor::unregister_fn_binding()`
   - Add helper: `fn is_fn_binding(binding: &str) -> bool { binding.eq_ignore_ascii_case("fn") }`

3. **Modify `validate_shortcut_string()`**:
   - Add special case to allow "fn" as valid on macOS

4. **Dependencies (macOS only in Cargo.toml)**:

   ```toml
   [target.'cfg(target_os = "macos")'.dependencies]
   objc2 = "0.6"
   objc2-app-kit = { version = "0.3", features = ["NSEvent"] }
   objc2-foundation = "0.3"
   block2 = "0.6"
   ```

5. **Frontend (MINIMAL change)**:
   - Add a "Use fn" button that calls `updateBinding(id, "fn")`
   - That's it. The backend handles everything else.

### Key technical details

```rust
// fn_monitor.rs - core monitoring logic
let handler = RcBlock::new(move |event: NonNull<NSEvent>| {
    let event_ref = unsafe { event.as_ref() };

    if event_ref.r#type() != NSEventType::FlagsChanged {
        return;
    }

    let flags = event_ref.modifierFlags();
    let is_pressed = flags.contains(NSEventModifierFlags::Function);

    // Dispatch press/release to the same handler as regular shortcuts
    let state = if is_pressed { ShortcutState::Pressed } else { ShortcutState::Released };
    dispatch_binding_event(&app_handle, "transcribe", "fn", state);
});

NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
    NSEventMask::FlagsChanged,
    &handler,
);
```

### Permissions

- Requires **Accessibility permission** (same as already needed for `enigo` pasting)
- No additional permission prompts for users
- Can check with `AXIsProcessTrustedWithOptions`

### Must run on main thread

```rust
app.run_on_main_thread(move || {
    // Install the monitor here
});
```

---

## Feature 2: Escape to Cancel Recording

### How it works

This is a **dynamic shortcut** - only registered while recording is active.

### Bug fix: `cancel_current_operation()` doesn't actually cancel

**Existing bug:** The `cancel_current_operation()` function in `utils.rs` (used by both the tray menu "Cancel" item and the `cancel_operation` Tauri command) has a bug where it calls `action.stop()` before `cancel_recording()`. This means:

1. User clicks "Cancel" in tray menu
2. `cancel_current_operation()` calls `TranscribeAction.stop()`
3. `stop()` calls `rm.stop_recording()` which returns the audio samples
4. **Transcription happens anyway** ❌
5. Then `cancel_recording()` is called (too late, samples already extracted)

**The fix:** Reorder operations in `cancel_current_operation()`:

1. Call `cancel_recording()` **first** - discards audio, sets state to Idle
2. Reset toggle states **without** calling `action.stop()` - we want to discard, not complete
3. Add `hide_recording_overlay()` and `remove_mute()` for complete cleanup

This fixes the existing cancel functionality for tray menu and frontend callers, and enables the new Escape shortcut to work correctly.

### Implementation approach

0. **Fix `cancel_current_operation()` in `src-tauri/src/utils.rs`**:

   ```rust
   pub fn cancel_current_operation(app: &AppHandle) {
       info!("Initiating operation cancellation...");

       // FIRST: Cancel any ongoing recording (BEFORE touching toggle states!)
       // This ensures audio is discarded and state is set to Idle
       let audio_manager = app.state::<Arc<AudioRecordingManager>>();
       audio_manager.cancel_recording();

       // Remove any applied mute
       audio_manager.remove_mute();

       // Reset toggle states WITHOUT calling action.stop()
       // (action.stop() would try to transcribe, which we don't want when cancelling)
       let toggle_state_manager = app.state::<ManagedToggleState>();
       if let Ok(mut states) = toggle_state_manager.lock() {
           for (_, is_active) in states.active_toggles.iter_mut() {
               *is_active = false;
           }
       }

       // Hide the recording overlay
       hide_recording_overlay(app);

       // Update tray icon to idle state
       change_tray_icon(app, crate::tray::TrayIconState::Idle);

       info!("Operation cancellation completed - returned to idle state");
   }
   ```

1. **Add `CancelAction` to `src-tauri/src/actions.rs`**:

   ```rust
   struct CancelAction;

   impl ShortcutAction for CancelAction {
       fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
           // Cancel the recording (handles overlay, mute, tray icon, toggle states)
           utils::cancel_current_operation(app);

           // Unregister ourselves (defer to avoid deadlock)
           let app_clone = app.clone();
           std::thread::spawn(move || {
               std::thread::sleep(std::time::Duration::from_millis(10));
               let _ = crate::shortcut::unregister_dynamic_binding(&app_clone, "cancel");
           });
       }

       fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
           // Instant action, no stop needed
       }
   }
   ```

2. **Add cancel binding to settings defaults** (`settings.rs`):

   ```rust
   ShortcutBinding {
       id: "cancel".to_string(),
       name: "Cancel".to_string(),
       description: "Cancel recording".to_string(),
       default_binding: "Escape".to_string(),
       current_binding: "Escape".to_string(),
       dynamic: true,  // KEY: only registered when needed
   }
   ```

3. **Add dynamic binding functions** (`shortcut/mod.rs`):

   ```rust
   pub fn register_dynamic_binding(app: &AppHandle, binding_id: &str) -> Result<(), String>
   pub fn unregister_dynamic_binding(app: &AppHandle, binding_id: &str) -> Result<(), String>
   ```

4. **Modify `init_shortcuts()`** to skip dynamic bindings:

   ```rust
   for (id, binding) in settings.bindings {
       if binding.dynamic {
           continue;  // Don't register at startup
       }
       register_binding(app, binding);
   }
   ```

5. **Register cancel when recording starts** (`actions.rs` in `TranscribeAction::start`):

   ```rust
   // Queue registration on main thread event loop (runs after callback returns)
   let app_clone = app.clone();
   let _ = app.run_on_main_thread(move || {
       let _ = crate::shortcut::register_dynamic_binding(&app_clone, "cancel");
   });
   ```

6. **Unregister cancel when recording stops** (`actions.rs` in `TranscribeAction::stop`):

   ```rust
   // Queue unregistration on main thread event loop
   let app_clone = app.clone();
   let _ = app.run_on_main_thread(move || {
       let _ = crate::shortcut::unregister_dynamic_binding(&app_clone, "cancel");
   });
   ```

### Critical: Use run_on_main_thread for dynamic binding operations

**Never call `register_dynamic_binding` or `unregister_dynamic_binding` synchronously from within an action handler, and never from a background thread.**

- Synchronous calls deadlock because we're inside the NSEvent callback
- Background thread calls corrupt state because `tauri-plugin-global-shortcut` is NOT thread-safe

**The solution:** Use `app.run_on_main_thread()` to queue the work. This schedules execution on the main thread's event loop AFTER the current callback returns.

```rust
let app_clone = app.clone();
let _ = app.run_on_main_thread(move || {
    // Safe: runs on main thread after callback returns
    let _ = crate::shortcut::register_dynamic_binding(&app_clone, "cancel");
});
```

This pattern:
1. Doesn't block the current callback (returns immediately)
2. Ensures shortcut operations run on the main thread (required by tauri-plugin-global-shortcut)
3. Runs after the NSEvent callback completes (avoids deadlock)

### No frontend changes needed

The cancel shortcut is entirely backend. The UI doesn't need to show it or allow editing it (it's marked `dynamic: true`).

---

## What NOT to do (lessons from failed attempt)

### 1. Don't rewrite the shortcut UI

The existing `HandyShortcut.tsx` has bugs but works. Leave it alone except for adding the "Use fn" button.

### 2. Don't try to "fix" suspend/resume

The suspend → change_binding → resume flow has a double-registration issue (change_binding registers, then resume tries to register again). The `.catch(console.error)` swallows the error. It's ugly but functional. Don't touch it.

### 3. Don't add multiple bindings UI

The plan called for showing transcribe, transcribe_llm, cancel in the UI. This is scope creep. Start with just fn key support for the existing transcribe binding.

### 4. Don't change change_binding behavior

I modified `change_binding` to handle empty bindings differently. This had cascading effects. The original behavior should be preserved.

### 5. Test incrementally

I made changes to:

- shortcut/mod.rs
- shortcut/fn_monitor.rs (new)
- actions.rs
- settings.rs
- HandyShortcut.tsx

All at once. Should have:

1. Added fn_monitor.rs and tested fn key works
2. Then added CancelAction and tested escape works
3. Then (maybe) touched the UI

---

## File structure

```
src-tauri/src/
├── shortcut/
│   ├── mod.rs           # Main shortcut logic, routing to fn_monitor for "fn"
│   └── fn_monitor.rs    # macOS-only fn key monitoring
├── actions.rs           # Add CancelAction, register/unregister cancel in transcribe start/stop
└── settings.rs          # Add cancel binding with dynamic: true
```

---

## Testing checklist

- [ ] fn key starts recording (PTT mode)
- [ ] fn key release stops recording and transcribes (PTT mode)
- [ ] fn key toggles recording (toggle mode)
- [ ] Escape cancels recording mid-transcription
- [ ] Escape does NOT cancel when not recording (dynamic registration)
- [ ] Existing keyboard shortcuts still work (option+space etc)
- [ ] Changing shortcuts in UI still works
- [ ] Reset shortcut still works

---

## Reference PRs

- [PR #136](https://github.com/cjpais/Handy/pull/136) - Original fn key implementation (tekacs)
- [PR #224](https://github.com/cjpais/Handy/pull/224) - Cancel shortcut approach (jacksongoode)

---

## Debugging Status (2025-11-28)

### Current State

**fn key works perfectly when all dynamic binding code is disabled.**

The cancel shortcut implementation causes the app to freeze. After extensive debugging, the root cause remains unclear.

### What's Been Implemented

All the code described above has been implemented:

1. ✅ Fixed `cancel_current_operation()` in utils.rs
2. ✅ Added `dynamic: bool` field to ShortcutBinding with `#[serde(default)]`
3. ✅ Added "cancel" binding to defaults (Escape, dynamic: true)
4. ✅ Added `register_dynamic_binding()` and `unregister_dynamic_binding()`
5. ✅ Modified `init_shortcuts()` to skip dynamic bindings
6. ✅ Created CancelAction and added to ACTION_MAP
7. ⚠️ Wired up cancel registration in TranscribeAction start/stop (CAUSES FREEZE)

### What's Been Tried

1. **Synchronous calls** - Deadlocked (expected, we're inside NSEvent callback)
2. **`std::thread::spawn` with 10ms delay** - Still froze, hypothesis was thread-safety issue
3. **`app.run_on_main_thread()`** - Still froze

### Current Test Configuration

In `actions.rs`, only the registration in `TranscribeAction::start()` is enabled:

```rust
// TEST: Enable registration only - no unregistration anywhere
let app_clone = app.clone();
let _ = app.run_on_main_thread(move || {
    debug!("run_on_main_thread closure executing for cancel registration");
    if let Err(e) = crate::shortcut::register_dynamic_binding(&app_clone, "cancel") {
        debug!("Failed to register cancel binding: {}", e);
    }
    debug!("cancel registration complete");
});
```

All unregistration code (in TranscribeAction::stop and CancelAction::start) is commented out.

### The Freeze Behavior

**Test:** Toggle mode (not PTT). Press fn once to start recording, press fn again to stop.

**Expected:** Recording starts, then stops and transcribes.

**Actual:** Recording starts. On second fn press, app freezes completely (requires force quit).

### Critical Log Evidence

Last logs before freeze:

```
[2025-11-28][00:04:09][handy_app_lib::shortcut] dispatch_binding_event called: binding_id=transcribe, shortcut=fn, state=Pressed
[2025-11-28][00:04:09][handy_app_lib::shortcut] Handling toggle mode for binding 'transcribe', current state: false
[2025-11-28][00:04:09][handy_app_lib::shortcut] Toggle mode: Activating binding 'transcribe'
[2025-11-28][00:04:09][handy_app_lib::actions] TranscribeAction::start called for binding: transcribe
...
[2025-11-28][00:04:09][handy_app_lib::actions] run_on_main_thread closure executing for cancel registration
[2025-11-28][00:04:09][handy_app_lib::shortcut] Registering dynamic binding: cancel
[2025-11-28][00:04:09][handy_app_lib::shortcut] register_binding called: id=cancel, binding=Escape
[2025-11-28][00:04:09][handy_app_lib::shortcut] Shortcut 'Escape' registered successfully
[2025-11-28][00:04:09][handy_app_lib::actions] cancel registration complete
[2025-11-28][00:04:09][handy_app_lib::actions] TranscribeAction::start completed in 45.550958ms
```

Then on second fn press:

```
[2025-11-28][00:04:11][handy_app_lib::shortcut::fn_monitor] fn key released, dispatching to 1 binding(s)
[2025-11-28][00:04:11][handy_app_lib::shortcut] dispatch_binding_event called: binding_id=transcribe, shortcut=fn, state=Released
[2025-11-28][00:04:11][handy_app_lib::shortcut] Handling toggle mode for binding 'transcribe', current state: true
[2025-11-28][00:04:11][handy_app_lib::shortcut] Toggle mode: Deactivating binding 'transcribe'
[2025-11-28][00:04:11][handy_app_lib::actions] TranscribeAction::stop called for binding: transcribe
...
[2025-11-28][00:04:11][handy_app_lib::utils] Initiating operation cancellation...
```

**KEY OBSERVATION:** `cancel_current_operation()` is being called even though the user only pressed fn (not Escape). The user did NOT press Escape. Only fn was pressed twice.

The log shows "Initiating operation cancellation..." but never shows "Operation cancellation completed" - indicating the freeze happens INSIDE `cancel_current_operation()`.

### Hypothesis

Something about registering the Escape shortcut via `tauri-plugin-global-shortcut` is causing either:

1. CancelAction to be triggered erroneously (without Escape being pressed), OR
2. Some interaction between the registered Escape shortcut and the fn_monitor that causes a deadlock

The fact that `cancel_current_operation()` is called without the user pressing Escape is the key mystery. It appears that either:
- The Escape key registration somehow triggers its own action immediately
- Or there's some corruption in the shortcut dispatch system

### Files Modified

- `src-tauri/src/utils.rs` - Fixed cancel_current_operation
- `src-tauri/src/settings.rs` - Added dynamic field, cancel binding
- `src-tauri/src/shortcut/mod.rs` - Added register/unregister_dynamic_binding, skip dynamic in init
- `src-tauri/src/actions.rs` - Added CancelAction, wiring in TranscribeAction (currently mostly commented out)

### Next Steps to Investigate

1. **Why is CancelAction being triggered?** Add logging to see what's calling `dispatch_binding_event` for "cancel"
2. **Check for shortcut string conflicts** - Maybe "Escape" is being parsed or matched incorrectly
3. **Try a different shortcut** - Register something other than Escape to see if it's Escape-specific
4. **Check tauri-plugin-global-shortcut source** - Look for any auto-trigger behavior on registration
5. **Minimal reproduction** - Create a tiny test that just registers Escape via run_on_main_thread to isolate the issue

---

## Debugging Session 2 (2025-11-28, continued)

### Root Causes Found and Fixed

#### Issue 1: Cancel binding routed to fn_monitor instead of global_shortcut

**Problem:** The cancel binding was being dispatched with `shortcut='fn'` instead of `shortcut='Escape'`:
```
dispatch_binding_event: binding_id='cancel', shortcut='fn', state=Released
```

**Root cause:** The stored settings file (`settings_store.json`) had an incorrect value for the cancel binding's `current_binding` field. It was `"fn"` instead of `"Escape"`. This caused `register_binding()` to route it to `fn_monitor` instead of `_register_shortcut`.

**Fix:** Delete the settings file to regenerate fresh defaults with correct `current_binding: "Escape"`.

**Verification:** After deleting settings, logs show correct routing:
```
register_dynamic_binding: id='cancel', current_binding='Escape', dynamic=true
register_binding: id='cancel', current_binding='Escape', is_fn=false
register_binding: routing to _register_shortcut for 'cancel'
```

#### Issue 2: Deadlock when cancel_current_operation acquires toggle lock

**Problem:** After fixing Issue 1, pressing Escape caused a freeze at:
```
cancel_current_operation: Attempting to acquire toggle lock...
```

**Root cause:** `dispatch_binding_event()` held the toggle lock while calling `action.start()`. When `CancelAction::start()` called `cancel_current_operation()`, which tried to acquire the same lock, it deadlocked.

**Fix:** Modified `dispatch_binding_event()` in `shortcut/mod.rs` to release the lock BEFORE calling the action:

```rust
// Toggle mode: toggle on press only
if state == ShortcutState::Pressed {
    // Determine action and update state while holding the lock,
    // but RELEASE the lock before calling the action to avoid deadlocks.
    let should_start: bool;
    {
        let toggle_state_manager = app.state::<ManagedToggleState>();
        let mut states = toggle_state_manager.lock().expect("...");
        let is_currently_active = states.active_toggles.entry(binding_id.to_string()).or_insert(false);
        should_start = !*is_currently_active;
        *is_currently_active = should_start;
    } // Lock released here

    // Now call the action without holding the lock
    if should_start {
        action.start(app, binding_id, shortcut_string);
    } else {
        action.stop(app, binding_id, shortcut_string);
    }
}
```

#### Issue 3: Cancel shortcut not unregistered after use

**Problem:** On second recording, got error: `Shortcut 'Escape' is already in use`

**Root cause:** The unregistration code in `TranscribeAction::stop` and `CancelAction::start` was commented out for debugging.

**Fix:** Re-enabled the unregistration calls:

In `TranscribeAction::stop`:
```rust
let app_clone = app.clone();
let _ = app.run_on_main_thread(move || {
    if let Err(e) = crate::shortcut::unregister_dynamic_binding(&app_clone, "cancel") {
        debug!("Failed to unregister cancel binding: {}", e);
    }
});
```

In `CancelAction::start`:
```rust
let app_clone = app.clone();
let _ = app.run_on_main_thread(move || {
    if let Err(e) = crate::shortcut::unregister_dynamic_binding(&app_clone, "cancel") {
        debug!("Failed to unregister cancel binding: {}", e);
    }
});
```

### Test Results After Fixes

With all three fixes applied, the following flow was tested:

1. **fn press** - Recording starts, cancel shortcut registered ✅
2. **fn press** - Recording stops, transcription completes ✅
3. **fn press** - New recording starts, cancel shortcut registered ✅
4. **Escape press** - Cancel triggered, logs show:
   ```
   CancelAction::start - cancelling current operation
   cancel_current_operation: Starting...
   cancel_current_operation: Toggle lock acquired
   Resetting toggle state for binding: transcribe
   Resetting toggle state for binding: cancel
   cancel_current_operation: Completed - returned to idle state
   ```

The logs indicate cancel_current_operation completed successfully, but the **app still hangs/freezes after this point**.

### Current State

**The logs show cancel_current_operation completes**, but the app becomes unresponsive (beach ball) anyway. This suggests the freeze happens AFTER cancel_current_operation returns, possibly in:

1. The unregister_dynamic_binding call that's queued via run_on_main_thread
2. Something in dispatch_binding_event after the action returns
3. The Escape Released event processing
4. Some interaction with the main thread event loop

### Files Modified in This Session

All changes are in the `keyboard-shortcuts` branch:

1. **`src-tauri/src/shortcut/mod.rs`**:
   - Fixed deadlock by releasing toggle lock before calling action
   - Added extensive INFO-level logging throughout

2. **`src-tauri/src/actions.rs`**:
   - Re-enabled cancel unregistration in TranscribeAction::stop
   - Re-enabled cancel self-unregistration in CancelAction::start
   - Added INFO-level logging

3. **`src-tauri/src/utils.rs`**:
   - Added INFO-level logging throughout cancel_current_operation

### Remaining Mystery

The logs show complete execution through cancel_current_operation, but the app freezes anyway. The freeze happens somewhere after:
```
cancel_current_operation: Completed - returned to idle state
```

But before any visible response in the UI.

### Next Steps to Try

1. **Add logging after CancelAction::start returns** to see if dispatch_binding_event continues
2. **Check if Escape Released event causes issues** - maybe add logging for that path
3. **Check if run_on_main_thread unregistration is blocking** - maybe the main thread is stuck waiting
4. **Try making unregistration synchronous** - if we're already on main thread for the callback, maybe run_on_main_thread isn't needed
5. **Check for UI/window interactions** - maybe hide_recording_overlay or change_tray_icon is blocking
6. **Profile the hang** - use Activity Monitor or Instruments to see what thread is stuck and where

### Quick Start for Next Session

1. Read this doc section (Debugging Session 2)
2. Key files to look at:
   - `src-tauri/src/shortcut/mod.rs` - dispatch_binding_event function (around line 788)
   - `src-tauri/src/actions.rs` - CancelAction::start (around line 431)
   - `src-tauri/src/utils.rs` - cancel_current_operation (line 18)
3. The freeze happens AFTER `cancel_current_operation: Completed` but the logs stop there
4. Main suspects: the run_on_main_thread unregistration call, or something with Escape Released event
5. **Important:** Delete settings file before testing (or the cancel binding might have wrong current_binding value)
