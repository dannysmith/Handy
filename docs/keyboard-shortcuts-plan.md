# Keyboard Shortcuts: Analysis & Implementation Plan

## Problem Statement

Handy's keyboard shortcut system needs improvement to support:
1. **macOS fn/globe key** as a trigger (most requested feature)
2. **Cancel recording** functionality mid-transcription
3. **Multiple shortcut bindings** for different actions (transcribe, transcribe+LLM, translate)
4. **Extensibility** via settings JSON for power users

Currently, Handy uses `tauri-plugin-global-shortcut` which cannot capture the fn key on macOS, limiting users to multi-key combinations like `Option+Space`.

---

## Related Issues & PRs (on upstream cjpais/Handy)

| PR | Author | Goal | Status |
|----|--------|------|--------|
| [#136](https://github.com/cjpais/Handy/pull/136) | @tekacs | fn key support via rdev | Blocked - wants unified approach |
| [#163](https://github.com/cjpais/Handy/pull/163) | @akshar-dave | fn key support | Superseded by #136 |
| [#224](https://github.com/cjpais/Handy/pull/224) | @jacksongoode | Cancel recording shortcut | Close to merge, some issues |
| [#355](https://github.com/cjpais/Handy/pull/355) | - | Transcribe with post-process hotkey | Awaiting review |

**Key maintainer feedback from PRs:**
- @cjpais wants a **unified keyboard handling approach**, not multiple systems bolted together
- Willing to accept **JSON-configurable bindings** before UI exists
- Concerned about **permission prompts** (Input Monitoring vs Accessibility)
- PR #136 demonstrated fn key works with **Accessibility permission only** (no Input Monitoring)

---

## Current Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                  Keyboard Shortcut System                   │
├─────────────────────────────────────────────────────────────┤
│  tauri-plugin-global-shortcut                               │
│  └── Registers shortcuts via GlobalShortcutExt              │
│  └── Handles press/release events via ShortcutState         │
│  └── Parses shortcuts from strings like "option+space"      │
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

**Limitations:**
1. No fn/globe key support - plugin can't capture modifier-only keys
2. Single shortcut only - "transcribe" binding exists, UI shows first binding only
3. No cancel functionality
4. No way to add custom bindings without code changes

---

## Research: macOS fn/Globe Key

The fn/globe key is a **modifier key** generating `NSFlagsChanged`/`kCGEventFlagsChanged` events.

### Permission Matrix

| Method | Permission | Can Capture fn? | Notes |
|--------|------------|-----------------|-------|
| `tauri-plugin-global-shortcut` | Accessibility | No | Standard shortcuts only |
| `CGEventTap` (listenOnly) | Input Monitoring | Yes | Lighter permission |
| `CGEventTap` (defaultTap) | Accessibility | Yes | Can modify events |
| `rdev` crate | Configurable | Yes | Uses CGEventTap internally |

**Competitor apps** (SuperWhisper, MacWhisper, WisperFlow) all support fn key and require Accessibility permission (needed anyway for pasting text).

**PR #136 proved:** fn key capture works with **Accessibility permission only** using `objc2-app-kit` crates.

---

## Proposed Architecture

```
┌─────────────────────────────────────────────────────────────┐
│              Unified Keyboard Input System                  │
├─────────────────────────────────────────────────────────────┤
│  Platform Input Layer (abstraction)                         │
│  ├── macOS: CGEventTap via objc2 (for fn + standard keys)   │
│  ├── Windows: tauri-plugin-global-shortcut                  │
│  └── Linux: tauri-plugin-global-shortcut                    │
├─────────────────────────────────────────────────────────────┤
│  ShortcutManager (new unified module)                       │
│  ├── register_binding(id, keys, action)                     │
│  ├── unregister_binding(id)                                 │
│  ├── is_binding_active(id) -> bool                          │
│  └── Handles PTT vs Toggle mode internally                  │
├─────────────────────────────────────────────────────────────┤
│  Binding Registry (expanded settings)                       │
│  ├── transcribe: { binding: "option+space", action: "..." } │
│  ├── transcribe_llm: { binding: "", action: "..." }         │
│  ├── cancel: { binding: "escape", dynamic: true }           │
│  └── (extensible via JSON)                                  │
├─────────────────────────────────────────────────────────────┤
│  Action System (existing, extended)                         │
│  ├── TranscribeAction                                       │
│  ├── TranscribeWithPostProcessAction                        │
│  └── CancelAction                                           │
└─────────────────────────────────────────────────────────────┘
```

---

## Implementation Plan

### Phase 1: macOS fn Key Support
- Add macOS-specific keyboard monitor using `objc2-app-kit`
- Listen for `kCGEventFlagsChanged` to detect fn key press/release
- Integrate with existing action system
- Keep `tauri-plugin-global-shortcut` for standard shortcuts
- Use Accessibility permission only (already required for pasting)

### Phase 2: Cancel Shortcut
- Implement dynamic registration (active only during recording)
- Add `CancelAction` to ACTION_MAP
- Default to Escape, configurable via settings JSON
- Based on PR #224 approach

### Phase 3: Multiple Bindings Support
- Extend settings schema for additional named bindings
- Allow JSON configuration for power users
- Minimal UI initially (debug menu)

### Phase 4: UI & Polish
- Design unified shortcuts settings UI
- Add shortcut conflict detection
- Consider URL scheme support (`handy://start`, `handy://stop`)

---

## Key Technical Decisions

1. **Don't replace global-shortcut entirely** - Works well for standard shortcuts cross-platform. Only supplement for macOS fn key.

2. **Use `objc2` crates directly** - Already available as transitive dependencies. PR #136 proved this approach works.

3. **Dynamic cancel shortcut** - Register only when recording to avoid interfering with other apps (vim, etc.)

4. **Settings-first approach** - Support new bindings via JSON before building UI.

---

## References

- [Tauri Global Shortcut Plugin](https://v2.tauri.app/plugin/global-shortcut/)
- [macOS CGEventTap Documentation](https://developer.apple.com/documentation/coregraphics/cgeventtype/kcgeventflagschanged)
- [Apple Developer Forums: Input Monitoring vs Accessibility](https://developer.apple.com/forums/thread/122492)
- [Stack Overflow: Capture fn key on Mac](https://stackoverflow.com/questions/33260278/intercept-function-key-strokes-on-mac)
