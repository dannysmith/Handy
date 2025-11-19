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
use tauri_nspanel::{tauri_panel, CollectionBehavior, PanelBuilder, PanelLevel};

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
const OVERLAY_HEIGHT: f64 = 36.0;

#[cfg(target_os = "macos")]
const OVERLAY_TOP_OFFSET: f64 = 46.0;
#[cfg(any(target_os = "windows", target_os = "linux"))]
const OVERLAY_TOP_OFFSET: f64 = 4.0;

#[cfg(target_os = "macos")]
const OVERLAY_BOTTOM_OFFSET: f64 = 15.0;

#[cfg(any(target_os = "windows", target_os = "linux"))]
const OVERLAY_BOTTOM_OFFSET: f64 = 40.0;

fn get_monitor_with_cursor(app_handle: &AppHandle) -> Option<tauri::Monitor> {
    let enigo = Enigo::new(&Default::default());
    if let Ok(enigo) = enigo {
        if let Ok(mouse_location) = enigo.location() {
            if let Ok(monitors) = app_handle.available_monitors() {
                for monitor in monitors {
                    let is_within =
                        is_mouse_within_monitor(mouse_location, monitor.position(), monitor.size());
                    if is_within {
                        return Some(monitor);
                    }
                }
            }
        }
    }

    app_handle.primary_monitor().ok().flatten()
}

fn is_mouse_within_monitor(
    mouse_pos: (i32, i32),
    monitor_pos: &PhysicalPosition<i32>,
    monitor_size: &PhysicalSize<u32>,
) -> bool {
    let (mouse_x, mouse_y) = mouse_pos;
    let PhysicalPosition {
        x: monitor_x,
        y: monitor_y,
    } = *monitor_pos;
    let PhysicalSize {
        width: monitor_width,
        height: monitor_height,
    } = *monitor_size;

    mouse_x >= monitor_x
        && mouse_x < (monitor_x + monitor_width as i32)
        && mouse_y >= monitor_y
        && mouse_y < (monitor_y + monitor_height as i32)
}

fn calculate_overlay_position(app_handle: &AppHandle) -> Option<(f64, f64)> {
    if let Some(monitor) = get_monitor_with_cursor(app_handle) {
        let work_area = monitor.work_area();
        let scale = monitor.scale_factor();
        let work_area_width = work_area.size.width as f64 / scale;
        let work_area_height = work_area.size.height as f64 / scale;
        let work_area_x = work_area.position.x as f64 / scale;
        let work_area_y = work_area.position.y as f64 / scale;

        let settings = settings::get_settings(app_handle);

        let x = work_area_x + (work_area_width - OVERLAY_WIDTH) / 2.0;
        let y = match settings.overlay_position {
            OverlayPosition::Top => work_area_y + OVERLAY_TOP_OFFSET,
            OverlayPosition::Bottom | OverlayPosition::None => {
                // don't subtract the overlay height it puts it too far up
                work_area_y + work_area_height - OVERLAY_BOTTOM_OFFSET
            }
        };

        return Some((x, y));
    }
    None
}

/// Creates the recording overlay window and keeps it hidden by default
#[cfg(not(target_os = "macos"))]  // NEW: Only for Windows/Linux
pub fn create_recording_overlay(app_handle: &AppHandle) {
    if let Some((x, y)) = calculate_overlay_position(app_handle) {
        match WebviewWindowBuilder::new(
            app_handle,
            "recording_overlay",
            tauri::WebviewUrl::App("src/overlay/index.html".into()),
        )
        .title("Recording")
        .position(x, y)
        .resizable(false)
        .inner_size(OVERLAY_WIDTH, OVERLAY_HEIGHT)
        .shadow(false)
        .maximizable(false)
        .minimizable(false)
        .closable(false)
        .accept_first_mouse(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .transparent(true)
        .focused(false)
        .visible(false)
        .build()
        {
            Ok(_window) => {
                debug!("Recording overlay window created successfully (hidden)");
            }
            Err(e) => {
                debug!("Failed to create recording overlay window: {}", e);
            }
        }
    }
}

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
        .corner_radius(0.0)  // Remove rounded corners from NSPanel window
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
                log::error!("[OVERLAY] Failed to create panel: {}", e);
            }
        }
    } else {
        log::warn!("[OVERLAY] Could not calculate overlay position");
    }
}

/// Shows the recording overlay window with fade-in animation
pub fn show_recording_overlay(app_handle: &AppHandle) {
    info!("[OVERLAY] show_recording_overlay() called");

    // Check if overlay should be shown based on position setting
    let settings = settings::get_settings(app_handle);
    if settings.overlay_position == OverlayPosition::None {
        info!("[OVERLAY] Overlay position is None, not showing");
        return;
    }

    info!("[OVERLAY] Attempting to get webview window");
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        info!("[OVERLAY] Found overlay window, calling show()");
        let _ = overlay_window.show();

        info!("[OVERLAY] Show() completed, updating position");
        // Update position AFTER showing to avoid race condition with hide()
        if let Some((x, y)) = calculate_overlay_position(app_handle) {
            debug!("[OVERLAY] Calculated position: x={}, y={}", x, y);
            let _ = overlay_window.set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
            debug!("[OVERLAY] Position updated successfully");
        } else {
            log::warn!("[OVERLAY] Could not calculate position");
        }

        info!("[OVERLAY] Position updated, emitting show-overlay event");
        // Emit event to trigger fade-in animation with recording state
        let _ = overlay_window.emit("show-overlay", "recording");
        info!("[OVERLAY] show_recording_overlay() completed successfully");
    } else {
        log::warn!("[OVERLAY] Could not find overlay window!");
    }
}

/// Shows the transcribing overlay window
pub fn show_transcribing_overlay(app_handle: &AppHandle) {
    // Check if overlay should be shown based on position setting
    let settings = settings::get_settings(app_handle);
    if settings.overlay_position == OverlayPosition::None {
        return;
    }

    update_overlay_position(app_handle);

    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        let _ = overlay_window.show();
        // Emit event to switch to transcribing state
        let _ = overlay_window.emit("show-overlay", "transcribing");
    }
}

/// Updates the overlay window position based on current settings
pub fn update_overlay_position(app_handle: &AppHandle) {
    debug!("[OVERLAY] update_overlay_position() called");
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        debug!("[OVERLAY] Found overlay window for position update");
        if let Some((x, y)) = calculate_overlay_position(app_handle) {
            debug!("[OVERLAY] Calculated position: x={}, y={}", x, y);
            let _ = overlay_window
                .set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
            debug!("[OVERLAY] Position updated successfully");
        } else {
            log::warn!("[OVERLAY] Could not calculate position");
        }
    } else {
        log::warn!("[OVERLAY] Could not find overlay window for position update");
    }
}

/// Hides the recording overlay window with fade-out animation
pub fn hide_recording_overlay(app_handle: &AppHandle) {
    info!("[OVERLAY] hide_recording_overlay() called");

    // Always hide the overlay regardless of settings - if setting was changed while recording,
    // we still want to hide it properly
    info!("[OVERLAY] Attempting to get webview window for hiding");
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        info!("[OVERLAY] Found overlay window, emitting hide-overlay event");
        // Emit event to trigger fade-out animation (CSS handles the visual transition)
        let _ = overlay_window.emit("hide-overlay", ());
        info!("[OVERLAY] Hide event emitted, calling hide()");
        // Hide the window immediately - the CSS fade-out animation will complete visually
        // before the window is actually hidden since the window is transparent
        let _ = overlay_window.hide();
        info!("[OVERLAY] hide_recording_overlay() completed successfully");
    } else {
        log::warn!("[OVERLAY] Could not find overlay window to hide!");
    }
}

pub fn emit_levels(app_handle: &AppHandle, levels: &Vec<f32>) {
    // emit levels to main app
    let _ = app_handle.emit("mic-level", levels);

    // also emit to the recording overlay if it's open
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        let _ = overlay_window.emit("mic-level", levels);
    }
}
