// Platform-specific I/O backend abstraction

// Re-export the platform-specific implementation
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(not(target_os = "linux"))]
mod portable;
#[cfg(not(target_os = "linux"))]
pub use portable::*;
