use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;
use windows_sys::Win32::{
    System::Power::{
        PowerRegisterSuspendResumeNotification, PowerUnregisterSuspendResumeNotification,
        DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS,
    },
    UI::WindowsAndMessaging::{DEVICE_NOTIFY_CALLBACK, PBT_APMRESUMEAUTOMATIC},
};

// The callback only queues work; it must never perform network I/O or touch a window.
pub static EVENTS: WakeEvents = WakeEvents {
    pending: AtomicBool::new(false),
    changed: Notify::const_new(),
};

pub struct WakeEvents {
    pending: AtomicBool,
    pub changed: Notify,
}
impl WakeEvents {
    fn receive(&self, event: u32) {
        // Windows also sends RESUMESUSPEND on user interaction after resume.
        // Handling only RESUMEAUTOMATIC avoids scheduling the same wake twice.
        if event == PBT_APMRESUMEAUTOMATIC {
            self.pending.store(true, Ordering::SeqCst);
            self.changed.notify_one();
        }
    }
    pub fn take(&self) -> bool {
        self.pending.swap(false, Ordering::SeqCst)
    }
}

unsafe extern "system" fn callback(
    _: *const core::ffi::c_void,
    event: u32,
    _: *const core::ffi::c_void,
) -> u32 {
    EVENTS.receive(event);
    0
}

// Store the opaque registration as an integer so the guard can be managed by Tauri.
// It is never dereferenced, and unregistering is safe from any thread.
pub struct Observer {
    handle: usize,
    parameters: usize,
}
impl Observer {
    pub fn register() -> Result<Self, String> {
        let parameters = Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
            Callback: Some(callback),
            Context: std::ptr::null_mut(),
        });
        let mut handle = std::ptr::null_mut();
        // Keep parameters at a stable address throughout registration. The callback
        // uses static data, which also remains valid during shutdown.
        let error = unsafe {
            PowerRegisterSuspendResumeNotification(
                DEVICE_NOTIFY_CALLBACK,
                parameters.as_ref() as *const _ as *mut _,
                &mut handle,
            )
        };
        if error != 0 {
            return Err(format!("无法监听休眠唤醒通知（Windows 错误 {error}）"));
        }
        Ok(Self {
            handle: handle as usize,
            parameters: Box::into_raw(parameters) as usize,
        })
    }
}
impl Drop for Observer {
    fn drop(&mut self) {
        unsafe {
            if PowerUnregisterSuspendResumeNotification(self.handle as isize) == 0 {
                drop(Box::from_raw(
                    self.parameters as *mut DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS,
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_real_resume_queues_work_and_duplicate_events_coalesce() {
        let events = WakeEvents {
            pending: AtomicBool::new(false),
            changed: Notify::new(),
        };
        for event in [4, 7, 10] {
            events.receive(event);
        }
        assert!(!events.take());
        events.receive(PBT_APMRESUMEAUTOMATIC);
        events.receive(PBT_APMRESUMEAUTOMATIC);
        assert!(events.take());
        assert!(!events.take());
    }
    #[test]
    fn windows_accepts_registration_and_cleanup() {
        drop(Observer::register().unwrap());
    }
}
