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

### Implementation approach

1. **Add `CancelAction` to `src-tauri/src/actions.rs`**:

   ```rust
   struct CancelAction;

   impl ShortcutAction for CancelAction {
       fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
           // Cancel the recording
           utils::cancel_current_operation(app);
           utils::hide_recording_overlay(app);

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
       enabled: true,
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

5. **Register cancel when recording starts** (`actions.rs` in `start_transcription_recording`):

   ```rust
   crate::shortcut::register_dynamic_binding(app, "cancel");
   ```

6. **Unregister cancel when recording stops** (`actions.rs` in `stop_transcription_recording`):
   ```rust
   crate::shortcut::unregister_dynamic_binding(app, "cancel");
   ```

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
