pub mod benchmark;

#[cfg(target_os = "windows")]
pub mod boot;
#[cfg(target_os = "windows")]
pub mod diskpart;
#[cfg(target_os = "windows")]
pub mod image;
#[cfg(target_os = "windows")]
pub mod vhd;
#[cfg(target_os = "windows")]
pub mod write;

#[cfg(target_os = "macos")]
pub mod write_macos;
