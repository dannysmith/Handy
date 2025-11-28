use crate::managers::audio::AudioRecordingManager;
use crate::ManagedToggleState;
use log::{info, warn};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

// Re-export all utility modules for easy access
// pub use crate::audio_feedback::*;
pub use crate::clipboard::*;
pub use crate::overlay::*;
pub use crate::tray::*;

/// Centralized cancellation function that can be called from anywhere in the app.
/// Handles cancelling recording operations and updates UI state.
///
/// IMPORTANT: This function discards the recording without transcribing.
/// It does NOT call action.stop() because that would trigger transcription.
pub fn cancel_current_operation(app: &AppHandle) {
    info!("cancel_current_operation: Starting...");

    // FIRST: Cancel any ongoing recording (BEFORE touching toggle states!)
    // This ensures audio is discarded and state is set to Idle.
    // Must happen first so that if TranscribeAction.stop() is called later
    // (e.g., user releases PTT key), stop_recording() returns None.
    info!("cancel_current_operation: Getting audio_manager...");
    let audio_manager = app.state::<Arc<AudioRecordingManager>>();
    info!("cancel_current_operation: Calling cancel_recording...");
    audio_manager.cancel_recording();
    info!("cancel_current_operation: cancel_recording done");

    // Remove any applied mute (in case mute-while-recording was enabled)
    info!("cancel_current_operation: Calling remove_mute...");
    audio_manager.remove_mute();
    info!("cancel_current_operation: remove_mute done");

    // Reset all shortcut toggle states WITHOUT calling action.stop()
    // We intentionally don't call action.stop() because that would trigger
    // transcription - we want to discard, not complete.
    info!("cancel_current_operation: Getting toggle_state_manager...");
    let toggle_state_manager = app.state::<ManagedToggleState>();
    info!("cancel_current_operation: Attempting to acquire toggle lock...");
    if let Ok(mut states) = toggle_state_manager.lock() {
        info!("cancel_current_operation: Toggle lock acquired");
        for (binding_id, is_active) in states.active_toggles.iter_mut() {
            if *is_active {
                info!("Resetting toggle state for binding: {}", binding_id);
                *is_active = false;
            }
        }
        info!("cancel_current_operation: Toggle states reset");
    } else {
        warn!("Failed to lock toggle state manager during cancellation");
    }

    // Hide the recording/transcribing overlay
    info!("cancel_current_operation: Hiding overlay...");
    hide_recording_overlay(app);
    info!("cancel_current_operation: Overlay hidden");

    // Update tray icon and menu to idle state
    info!("cancel_current_operation: Changing tray icon...");
    change_tray_icon(app, TrayIconState::Idle);
    info!("cancel_current_operation: Tray icon changed");

    info!("cancel_current_operation: Completed - returned to idle state");
}

/// Check if using the Wayland display server protocol
#[cfg(target_os = "linux")]
pub fn is_wayland() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok()
        || std::env::var("XDG_SESSION_TYPE")
            .map(|v| v.to_lowercase() == "wayland")
            .unwrap_or(false)
}
