# Keyboard Shortcuts: Analysis & Implementation Plan

## Problem Statement

Handy's keyboard shortcut system needs improvement to support:
1. **macOS fn/globe key** as a trigger (most requested feature)
2. **Cancel recording** functionality mid-transcription
3. **Multiple shortcut bindings** for different actions (transcribe, transcribe+LLM, translate)
4. **Extensibility** via settings JSON for power users

Currently, Handy uses `tauri-plugin-global-shortcut` which cannot capture the fn key on macOS, limiting users to multi-key combinations like `Option+Space`.

---

## Related Issues & PRs (upstream cjpais/Handy)

| PR | Author | Goal | Status |
|----|--------|------|--------|
| [#136](https://github.com/cjpais/Handy/pull/136) | @tekacs | fn key support via objc2 | Blocked - wants unified approach |
| [#163](https://github.com/cjpais/Handy/pull/163) | @akshar-dave | fn key support | Superseded by #136 |
| [#224](https://github.com/cjpais/Handy/pull/224) | @jacksongoode | Cancel recording shortcut | Close to merge, some issues |
| [#355](https://github.com/cjpais/Handy/pull/355) | - | Transcribe with post-process hotkey | Awaiting review |

**Key maintainer feedback:**
- @cjpais wants a **unified keyboard handling approach**, not multiple systems bolted together
- Willing to accept **JSON-configurable bindings** before UI exists
- Concerned about **permission prompts** (Input Monitoring vs Accessibility)
- PR #136 demonstrated fn key works with **Accessibility permission only**

---

## Current Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                  Keyboard Shortcut System                   │
├─────────────────────────────────────────────────────────────┤
│  tauri-plugin-global-shortcut                               │
│  └── Registers shortcuts via GlobalShortcutExt              │
│  └── Handles press/release via ShortcutState                │
│  └── Parses shortcuts like "option+space"                   │
├─────────────────────────────────────────────────────────────┤
│  ShortcutBinding (settings.rs)                              │
│  └── id: "transcribe"                                       │
│  └── default_binding / current_binding: "option+space"      │
├─────────────────────────────────────────────────────────────┤
│  ACTION_MAP (actions.rs)                                    │
│  └── "transcribe" → TranscribeAction                        │
│  └── "test" → TestAction                                    │
├─────────────────────────────────────────────────────────────┤
│  Modes:                                                     │
│  └── push_to_talk: true  → start on press, stop on release  │
│  └── push_to_talk: false → toggle on press                  │
└─────────────────────────────────────────────────────────────┘
```

**Key files:**
- `src-tauri/src/shortcut.rs` - Core shortcut registration
- `src-tauri/src/settings.rs` - Default shortcut bindings
- `src-tauri/src/actions.rs` - Action implementations
- `src/components/settings/HandyShortcut.tsx` - Frontend shortcut capture

**Current limitations:**
1. No fn/globe key support - plugin can't capture modifier-only keys
2. Single shortcut only - "transcribe" binding exists, UI shows first binding only
3. No cancel functionality exposed via shortcut
4. No way to add custom bindings without code changes

**Existing assets:**
- `rdev` crate is in Cargo.toml but **unused** (dangling dependency - should remove)
- `cancel_recording()` method already exists in `AudioRecordingManager` (used by tray menu)
- `objc2` crates available as transitive dependencies via `tauri-nspanel`

---

## Research: macOS fn/Globe Key

The fn/globe key is a **modifier key** generating `NSFlagsChanged` events with `NSEventModifierFlags::Function`.

### Approach Comparison

| Method | Permission | Can Block Events? | Thread Requirement |
|--------|------------|-------------------|-------------------|
| `NSEvent.addGlobalMonitor` | **Accessibility** | No (listen-only) | Main thread only |
| `CGEventTap` (listenOnly) | Input Monitoring | No | Flexible |
| `CGEventTap` (defaultTap) | Accessibility | Yes | Flexible |
| `tauri-plugin-global-shortcut` | Accessibility | N/A | N/A - can't capture fn |

**PR #136's approach (recommended):** Uses `NSEvent::addGlobalMonitorForEventsMatchingMask_handler` which:
- Requires only **Accessibility permission** (already needed for `enigo` pasting)
- No additional permission prompts for users
- Listen-only is sufficient for our use case

### Permission Details

```
Accessibility Permission:
├── Required for: enigo (pasting text), NSEvent.addGlobalMonitor
├── User grants once in System Settings > Privacy > Accessibility
└── App must be restarted after granting

Input Monitoring Permission:
├── Required for: CGEventTap with listenOnly option
├── Separate permission from Accessibility
└── Available to sandboxed/App Store apps
```

**Key insight:** Since we already require Accessibility for pasting, using `NSEvent.addGlobalMonitor` adds **no additional permission burden**.

### Competitor Apps

SuperWhisper, MacWhisper, and WisperFlow all:
- Support fn key as default trigger
- Require Accessibility permission
- Work without Input Monitoring permission

---

## Known Limitations & Edge Cases

### 1. Secure Input Blocking

When Secure Input is enabled (password fields, 1Password, Terminal "Secure Keyboard Entry"):
- **All event monitoring stops receiving events**
- Users will wonder why Handy stopped working
- No workaround exists - this is a macOS security feature

**Detection possible via:**
```bash
ioreg -l -w 0 | grep kCGSSessionSecureInputPID
```

**Recommendation:** Document this limitation; consider status indicator.

### 2. Event Tap Timeout (CGEventTap only)

macOS disables event taps if callbacks take too long. Must handle:
```rust
if event_type == kCGEventTapDisabledByTimeout {
    CGEventTapEnable(eventTap, true);  // Re-enable
    return;
}
```

`NSEvent.addGlobalMonitor` (PR #136's approach) is less susceptible since it's listen-only.

### 3. Main Thread Requirement

`NSEvent.addGlobalMonitor` **must** run on main thread. PR #136 handles this correctly:
```rust
app.run_on_main_thread(move || {
    NSEvent::addGlobalMonitorForEventsMatchingMask_handler(...)
});
```

**Important:** Keep callbacks minimal to avoid UI freezes.

### 4. System Shortcut Conflicts

macOS reserves fn+key combinations:
- `fn+Delete` = Forward Delete
- `fn+arrows` = Page navigation
- `fn+F1-F12` = Function keys

**fn alone** (no other key) is safe to use.

### 5. Platform Considerations

| Platform | fn Key Support | Notes |
|----------|---------------|-------|
| macOS | Yes | Via NSEvent.addGlobalMonitor |
| Windows | No fn key | Could support alternative (Caps Lock double-tap?) |
| Linux X11 | No fn key | global-hotkey has [key release issues](https://github.com/tauri-apps/global-hotkey/issues/39) |
| Linux Wayland | N/A | Global shortcuts not supported |

---

## Proposed Architecture

```
┌─────────────────────────────────────────────────────────────┐
│              Unified Keyboard Input System                  │
├─────────────────────────────────────────────────────────────┤
│  Platform Input Layer                                       │
│  ├── macOS: NSEvent.addGlobalMonitor (fn key)              │
│  │          + tauri-plugin-global-shortcut (standard keys)  │
│  ├── Windows: tauri-plugin-global-shortcut only            │
│  └── Linux: tauri-plugin-global-shortcut only              │
├─────────────────────────────────────────────────────────────┤
│  ShortcutManager (unified interface)                        │
│  ├── register_binding(id, keys, action)                     │
│  ├── unregister_binding(id)                                 │
│  ├── is_binding_active(id) -> bool                          │
│  └── Routes to appropriate platform handler                 │
├─────────────────────────────────────────────────────────────┤
│  Binding Registry (expanded settings)                       │
│  ├── transcribe: { binding: "fn", type: "modifier_only" }   │
│  ├── transcribe_llm: { binding: "", type: "key_combo" }     │
│  ├── cancel: { binding: "escape", type: "dynamic" }         │
│  └── settings_version: 2                                    │
├─────────────────────────────────────────────────────────────┤
│  Action System (extended)                                   │
│  ├── TranscribeAction (existing)                            │
│  ├── TranscribeWithPostProcessAction (new)                  │
│  └── CancelAction (new, uses existing cancel_recording())   │
└─────────────────────────────────────────────────────────────┘
```

### Module Structure

```
src-tauri/src/
├── shortcut/
│   ├── mod.rs              # Public API, ShortcutManager trait
│   ├── global_shortcut.rs  # tauri-plugin-global-shortcut wrapper
│   ├── fn_monitor.rs       # macOS fn key (from PR #136)
│   └── types.rs            # ShortcutBinding, ShortcutState, etc.
├── actions/
│   ├── mod.rs              # ACTION_MAP
│   ├── transcribe.rs       # TranscribeAction
│   ├── transcribe_llm.rs   # TranscribeWithPostProcessAction
│   └── cancel.rs           # CancelAction
```

### Settings Schema v2

```json
{
  "bindings": {
    "transcribe": {
      "id": "transcribe",
      "name": "Transcribe",
      "binding": "fn",
      "binding_type": "modifier_only",
      "default_binding": "option+space",
      "enabled": true
    },
    "transcribe_llm": {
      "id": "transcribe_llm",
      "name": "Transcribe with LLM",
      "binding": "",
      "binding_type": "key_combo",
      "default_binding": "",
      "enabled": false
    },
    "cancel": {
      "id": "cancel",
      "name": "Cancel Recording",
      "binding": "escape",
      "binding_type": "dynamic",
      "default_binding": "escape",
      "enabled": true
    }
  },
  "settings_version": 2
}
```

**Binding types:**
- `modifier_only`: Single modifier key (fn, caps lock)
- `key_combo`: Standard shortcut (option+space, ctrl+shift+r)
- `dynamic`: Only registered when relevant (cancel during recording)

---

## Implementation Plan

### Phase 1: Foundation
1. Remove dangling `rdev` dependency from Cargo.toml
2. Add direct dependencies (macOS only):
   ```toml
   [target.'cfg(target_os = "macos")'.dependencies]
   objc2 = "0.6"
   objc2-app-kit = "0.3"
   objc2-foundation = "0.3"
   block2 = "0.6"
   ```
3. Create `shortcut/` module structure
4. Add Accessibility permission pre-flight check
5. Port PR #136's `fn_monitor.rs` with improvements:
   - Better error messages
   - Debug logging
   - Graceful degradation if permission missing

### Phase 2: fn Key Support
1. Integrate fn monitor with existing shortcut system
2. Add "fn" as valid binding option in settings
3. Update frontend `HandyShortcut.tsx` to recognize fn key
4. Test PTT and toggle modes thoroughly
5. Document macOS-specific behavior

### Phase 3: Cancel Shortcut
1. Create `CancelAction` using existing `cancel_recording()`
2. Implement dynamic registration (only active during recording)
3. Add to settings schema with migration from v1
4. Handle Escape key conflicts (unregister when not recording)

### Phase 4: Multiple Bindings
1. Extend settings schema with `settings_version`
2. Add migration logic for existing settings
3. Create `TranscribeWithPostProcessAction`
4. Allow JSON configuration (debug menu initially)
5. Frontend support for multiple bindings

### Phase 5: Polish
1. Document Secure Input limitations in user guide
2. Add platform-specific feature documentation
3. Consider keyboard type detection for UI
4. Status indicator for blocked input (stretch goal)

---

## Key Technical Decisions

1. **Supplement, don't replace `global-shortcut`** - It works well for standard shortcuts cross-platform. Only add fn key handling for macOS.

2. **Use `NSEvent.addGlobalMonitor`** (not raw CGEventTap) - Simpler, same permission requirement, sufficient for listen-only use case.

3. **Dynamic cancel shortcut** - Register only when recording to avoid interfering with other apps (vim, etc.)

4. **Settings-first approach** - Support new bindings via JSON before building UI.

5. **No Input Monitoring permission** - Stick with Accessibility-only to minimize permission prompts.

---

## Open Questions

1. **Single-key triggers on Windows/Linux?** Should we support alternatives like Caps Lock double-tap?

2. **fn key in toggle mode?** Restrict to PTT only to avoid accidental triggers, or support both?

3. **Permission prompting strategy?** On first launch? When user tries to set fn shortcut?

4. **Secure Input status visibility?** Surface to users when password managers block input?

---

## References

### Implementation References
- [PR #136: fn_monitor.rs implementation](https://github.com/cjpais/Handy/pull/136)
- [PR #224: Cancel shortcut approach](https://github.com/cjpais/Handy/pull/224)
- [PR #355: Multiple bindings pattern](https://github.com/cjpais/Handy/pull/355)

### Apple Documentation
- [NSEvent.addGlobalMonitor](https://developer.apple.com/documentation/appkit/nsevent/1535472-addglobalmonitorforeventsmatchi)
- [CGEventTap](https://developer.apple.com/documentation/coregraphics/cgeventtype/kcgeventflagschanged)
- [Run Loop Management](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/Multithreading/RunLoopManagement/RunLoopManagement.html)

### Tauri & Rust
- [Tauri Global Shortcut Plugin](https://v2.tauri.app/plugin/global-shortcut/)
- [global-hotkey crate](https://github.com/tauri-apps/global-hotkey)
- [objc2 bindings](https://github.com/madsmtm/objc2)

### Troubleshooting
- [Secure Input detection](https://alexwlchan.net/2021/secure-input/)
- [CGEventTap timeout handling](https://stackoverflow.com/questions/2969110/cgeventtapcreate-breaks-down-mysteriously-with-key-down-events)
- [Input Monitoring vs Accessibility](https://developer.apple.com/forums/thread/122492)
- [global-hotkey X11 issues](https://github.com/tauri-apps/global-hotkey/issues/39)
