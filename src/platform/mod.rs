#[cfg(target_os = "macos")]
pub(crate) mod macos;
#[cfg(target_os = "linux")]
pub(crate) mod linux;

// Re-export platform-specific implementations under a common name.
// Shared code uses `use crate::platform::*` and gets the right impl.
#[cfg(target_os = "macos")]
pub(crate) use macos::*;
#[cfg(target_os = "linux")]
pub(crate) use linux::*;
