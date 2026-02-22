use serde::{Deserialize, Serialize};
use crate::Result;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UsbDevice {
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub product: String,
    pub size: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum UsbEventType {
    #[serde(rename = "connected")]
    Connected,
    #[serde(rename = "disconnected")]
    Disconnected,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UsbEvent {
    pub event_type: UsbEventType,
    pub device: UsbDevice,
}

/// Start monitoring USB devices and send events
#[tauri::command]
pub async fn start_usb_monitoring(
    app_handle: tauri::AppHandle,
) -> Result<String> {
    #[cfg(target_os = "windows")]
    {
        crate::platform::windows::start_usb_monitoring(app_handle).await
    }
    #[cfg(target_os = "macos")]
    {
        crate::platform::macos::start_usb_monitoring(app_handle).await
    }
    #[cfg(target_os = "linux")]
    {
        crate::platform::linux::start_usb_monitoring(app_handle).await
    }
}

/// Stop monitoring USB devices
#[tauri::command]
pub async fn stop_usb_monitoring(monitor_id: String) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        crate::platform::windows::stop_usb_monitoring(&monitor_id).await
    }
    #[cfg(target_os = "macos")]
    {
        crate::platform::macos::stop_usb_monitoring(&monitor_id).await
    }
    #[cfg(target_os = "linux")]
    {
        crate::platform::linux::stop_usb_monitoring(&monitor_id).await
    }
}
