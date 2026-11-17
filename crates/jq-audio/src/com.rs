//! COM for WASAPI: keeps cpal's device list live and tells when the system
//! default output device changes.
//!
//! cpal keeps one `IMMDeviceEnumerator` for the process and creates it on
//! whichever thread asks first, in that thread's single-threaded apartment
//! (cpal issue #1302, open in 0.18.2). Here that is often a supervisor
//! thread, which ends when the audio settings change; the enumerator is then
//! used outside its apartment, which can misreport devices or crash the
//! process. Created first on a thread of the multithreaded apartment that
//! lives as long as the process, it stays valid.
//!
//! The same thread registers for default device changes; the callback only
//! counts them (see [`default_changes`]). That replaces cpal's callbacks for
//! its virtual default device (issue #1402, see `default_device` in the
//! crate root) and goes once that is fixed; the enumerator part goes once
//! #1302 is. Elsewhere all of this is a no-op.

use std::sync::Once;
use std::sync::atomic::{AtomicU64, Ordering};

/// Changes of the system default output device so far.
static DEFAULT_CHANGES: AtomicU64 = AtomicU64::new(0);

/// Call before the first use of cpal's devices; idempotent.
pub(crate) fn ready() {
    static DONE: Once = Once::new();
    DONE.call_once(imp::start);
}

/// How often the system default output device has changed; compare two
/// readings to see whether it did in between. Always 0 outside Windows.
pub(crate) fn default_changes() -> u64 {
    DEFAULT_CHANGES.load(Ordering::Relaxed)
}

#[cfg(windows)]
mod imp {
    use std::sync::atomic::Ordering;
    use std::sync::mpsc;

    use cpal::traits::HostTrait;
    use windows::Win32::Foundation::PROPERTYKEY;
    use windows::Win32::Media::Audio::{
        DEVICE_STATE, EDataFlow, ERole, IMMDeviceEnumerator, IMMNotificationClient,
        IMMNotificationClient_Impl, MMDeviceEnumerator, eConsole, eRender,
    };
    use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
    use windows::core::{PCWSTR, implement};

    use super::DEFAULT_CHANGES;

    pub(super) fn start() {
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new().name("jq-com".into()).spawn(move || {
            // SAFETY: no reserved pointer; the thread never uninitializes, so
            // the apartment (and what lives in it) lasts as long as the process.
            let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            // Creates cpal's enumerator on this thread (cpal's own STA
            // initialization fails with RPC_E_CHANGED_MODE, which it accepts).
            let _ = cpal::default_host().devices();
            // Kept registered for the life of the process.
            let _watch = watch_default();
            let _ = tx.send(());
            loop {
                std::thread::park();
            }
        });
        // Without the thread, cpal creates the enumerator as before.
        if spawned.is_ok() {
            let _ = rx.recv();
        }
    }

    /// Registers for default device changes; `None` if Windows refuses, and
    /// then outputs on the default device do not follow it.
    fn watch_default() -> Option<(IMMDeviceEnumerator, IMMNotificationClient)> {
        // SAFETY: COM is initialized on this thread; the client is a COM
        // object of our own that stays alive with the registration.
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
            let client: IMMNotificationClient = DefaultChanged.into();
            enumerator.RegisterEndpointNotificationCallback(&client).ok()?;
            Some((enumerator, client))
        }
    }

    /// Counts default output changes. Runs on threads of the audio service's
    /// notifications: it must not block, and it makes no COM calls, so it
    /// leaves those threads as it finds them.
    #[implement(IMMNotificationClient)]
    struct DefaultChanged;

    impl IMMNotificationClient_Impl for DefaultChanged_Impl {
        fn OnDefaultDeviceChanged(
            &self,
            flow: EDataFlow,
            role: ERole,
            _id: &PCWSTR,
        ) -> windows::core::Result<()> {
            if flow == eRender && role == eConsole {
                DEFAULT_CHANGES.fetch_add(1, Ordering::Relaxed);
            }
            Ok(())
        }

        fn OnDeviceStateChanged(&self, _id: &PCWSTR, _state: DEVICE_STATE) -> windows::core::Result<()> {
            Ok(())
        }

        fn OnDeviceAdded(&self, _id: &PCWSTR) -> windows::core::Result<()> {
            Ok(())
        }

        fn OnDeviceRemoved(&self, _id: &PCWSTR) -> windows::core::Result<()> {
            Ok(())
        }

        fn OnPropertyValueChanged(&self, _id: &PCWSTR, _key: &PROPERTYKEY) -> windows::core::Result<()> {
            Ok(())
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub(super) fn start() {}
}
