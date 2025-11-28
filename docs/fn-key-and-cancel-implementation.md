# fn Key Support and Cancel Shortcut Implementation

This document describes the implementation of two features:

1. **fn key as trigger (macOS)** - Use the fn/Globe key to start/stop recording
2. **Escape to cancel recording** - Cancel mid-recording without transcribing

## Feature 1: fn Key Support (macOS)

### How it works

The fn/Globe key is a modifier key that generates `NSEventType::FlagsChanged` events with `NSEventModifierFlags::Function`. Standard shortcut libraries (like `tauri-plugin-global-shortcut`) cannot capture modifier-only keys.

### Implementation

**Parallel input system for fn key:**

1. **`src-tauri/src/shortcut/fn_monitor.rs`** (macOS-only)
   - Uses `NSEvent::addGlobalMonitorForEventsMatchingMask_handler` from objc2
   - Monitors for `NSEventMask::FlagsChanged` events
   - Checks `NSEventModifierFlags::Function` to detect fn press/release
   - Calls the same `dispatch_binding_event()` function as regular shortcuts

2. **`src-tauri/src/shortcut/mod.rs`**
   - Routes "fn" bindings to `fn_monitor` instead of `tauri-plugin-global-shortcut`
   - `is_fn_binding()` helper checks for fn-only bindings
   - `validate_shortcut_string()` allows "fn" as valid on macOS

3. **Dependencies (macOS only in Cargo.toml)**:
   ```toml
   [target.'cfg(target_os = "macos")'.dependencies]
   objc2 = "0.6"
   objc2-app-kit = { version = "0.3", features = ["NSEvent"] }
   objc2-foundation = "0.3"
   block2 = "0.6"
   ```

4. **Frontend**: "Use fn" button in `HandyShortcut.tsx` calls `updateBinding(id, "fn")`

### Permissions

- Requires **Accessibility permission** (same as already needed for `enigo` pasting)
- No additional permission prompts for users

### Limitations

- Stops receiving events when Secure Input is active (password fields, 1Password, etc.)
- fn+key combinations conflict with system shortcuts; fn alone is safe

---

## Feature 2: Escape to Cancel Recording

### How it works

The cancel shortcut is a **dynamic binding** - only registered while recording is active.

### Implementation

1. **`CancelAction`** in `actions.rs`:
   - Calls `cancel_current_operation()` to discard recording
   - Does NOT unregister itself (would deadlock inside callback)

2. **Cancel binding** in `settings.rs`:
   - `dynamic: true` - not registered at startup
   - Default binding: "Escape"

3. **Dynamic registration** in `shortcut/mod.rs`:
   - `register_dynamic_binding()` - idempotent (unregisters first if already registered)
   - `unregister_dynamic_binding()` - removes binding at runtime
   - `init_shortcuts()` skips dynamic bindings

4. **Lifecycle**:
   - `TranscribeAction::start()` registers cancel via `run_on_main_thread()`
   - `TranscribeAction::stop()` unregisters cancel via `run_on_main_thread()`
   - `CancelAction::start()` does NOT unregister (next registration handles cleanup)

### Key design decisions

**Why idempotent registration?**

Unregistering from inside the shortcut's own callback causes a deadlock (global_shortcut holds internal locks). Instead, `register_dynamic_binding()` unregisters first, so `CancelAction` doesn't need to unregister itself.

**Why release toggle lock before calling action?**

`dispatch_binding_event()` releases the toggle state lock BEFORE calling `action.start()`/`stop()`. This prevents deadlock when `CancelAction` calls `cancel_current_operation()` which also needs the lock.

---

## File structure

```
src-tauri/src/
├── shortcut/
│   ├── mod.rs           # Shortcut logic, routing, dispatch_binding_event
│   └── fn_monitor.rs    # macOS-only fn key monitoring
├── actions.rs           # CancelAction, TranscribeAction with cancel registration
├── settings.rs          # ShortcutBinding.dynamic field, cancel binding
└── utils.rs             # cancel_current_operation()
```

---

## Testing checklist

- [x] fn key starts recording (PTT mode)
- [x] fn key release stops recording and transcribes (PTT mode)
- [x] fn key toggles recording (toggle mode)
- [x] Escape cancels recording mid-transcription
- [x] Escape does NOT trigger when not recording (dynamic registration)
- [x] Existing keyboard shortcuts still work
- [x] Changing shortcuts in UI still works
- [x] Reset shortcut still works

---

## Reference PRs

- [PR #136](https://github.com/cjpais/Handy/pull/136) - Original fn key implementation (tekacs)
- [PR #224](https://github.com/cjpais/Handy/pull/224) - Cancel shortcut approach (jacksongoode)
