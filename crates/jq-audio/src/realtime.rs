//! Keeps the audio callback on time when the system saves power.
//!
//! On a Windows notebook on battery, cpal's raised thread priority is not
//! enough: the scheduler parks cores and lowers clocks for a process that is
//! mostly idle, and WASAPI callbacks then come late (crackles, short drops).
//! The callback thread joins the multimedia class scheduler (MMCSS, task
//! "Pro Audio"), and the process opts out of power throttling (EcoQoS).
//! Elsewhere both are no-ops.

use std::sync::Once;

/// Process: asks the system not to throttle this process to save power.
/// Idempotent; called whenever an output opens.
pub(crate) fn keep_full_speed() {
    static DONE: Once = Once::new();
    DONE.call_once(imp::keep_full_speed);
}

/// The calling thread joins MMCSS as a pro audio thread for as long as it
/// lives. Call once, from a stream's callback thread.
pub(crate) fn join_pro_audio() {
    imp::join_pro_audio();
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::System::Threading::{
        AVRT_PRIORITY_HIGH, AvSetMmThreadCharacteristicsW, AvSetMmThreadPriority, GetCurrentProcess,
        PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        PROCESS_POWER_THROTTLING_STATE, ProcessPowerThrottling, SetProcessInformation,
    };
    use windows_sys::w;

    pub(super) fn keep_full_speed() {
        // Control the execution speed and set it to "not throttled".
        let state = PROCESS_POWER_THROTTLING_STATE {
            Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            StateMask: 0,
        };
        // Best effort: older Windows versions do not know the class.
        // SAFETY: `state` is a valid PROCESS_POWER_THROTTLING_STATE of the
        // given size; the pseudo handle of the current process needs no close.
        unsafe {
            SetProcessInformation(
                GetCurrentProcess(),
                ProcessPowerThrottling,
                (&raw const state).cast(),
                size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
            );
        }
    }

    pub(super) fn join_pro_audio() {
        let mut task_index = 0u32;
        // SAFETY: a static, NUL-terminated wide string and a valid out
        // pointer. The registration ends with the thread (when cpal tears the
        // stream down), so the handle is not reverted.
        unsafe {
            let handle = AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut task_index);
            if !handle.is_null() {
                AvSetMmThreadPriority(handle, AVRT_PRIORITY_HIGH);
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub(super) fn keep_full_speed() {}
    pub(super) fn join_pro_audio() {}
}
