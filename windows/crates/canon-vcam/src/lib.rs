//! The Canon Webcam virtual camera.
//!
//! Windows 11's Frame Server loads this DLL when an app opens "Canon Webcam", asks the class
//! factory below for an `IMFActivate`, and pulls NV12 frames from the media source it activates.
//! The frames come from the engine through shared memory (see `canon-shm`), so this side stays
//! small: no camera code, no decoding, nothing that can hang the camera service.
//!
//! The registration that points Frame Server here is done by `CanonWebcam.exe install`.

#[cfg(windows)]
mod activate;
#[cfg(windows)]
mod feed;
#[cfg(windows)]
mod log;
#[cfg(windows)]
mod source;
#[cfg(windows)]
mod stream;

#[cfg(windows)]
mod exports {
    use std::ffi::c_void;

    use windows::Win32::Foundation::{
        CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_POINTER, S_FALSE,
    };
    use windows::Win32::System::Com::{IClassFactory, IClassFactory_Impl};
    use windows_core::{BOOL, GUID, HRESULT, IUnknown, Interface, Ref, implement};

    use crate::activate::Activator;

    const CLSID_CANON_WEBCAM: GUID = GUID::from_u128(canon_shm::CLSID_U128);

    #[implement(IClassFactory)]
    struct ClassFactory;

    impl IClassFactory_Impl for ClassFactory_Impl {
        fn CreateInstance(
            &self,
            outer: Ref<IUnknown>,
            riid: *const GUID,
            ppv: *mut *mut c_void,
        ) -> windows_core::Result<()> {
            if !outer.is_null() {
                return Err(CLASS_E_NOAGGREGATION.into());
            }
            let activator: IUnknown = Activator::create()?.into();
            unsafe { activator.query(riid, ppv).ok() }
        }

        fn LockServer(&self, _lock: BOOL) -> windows_core::Result<()> {
            Ok(())
        }
    }

    #[unsafe(no_mangle)]
    extern "system" fn DllGetClassObject(
        clsid: *const GUID,
        riid: *const GUID,
        ppv: *mut *mut c_void,
    ) -> HRESULT {
        if clsid.is_null() || riid.is_null() || ppv.is_null() {
            return E_POINTER;
        }
        unsafe { *ppv = std::ptr::null_mut() };
        if unsafe { *clsid } != CLSID_CANON_WEBCAM {
            return CLASS_E_CLASSNOTAVAILABLE;
        }
        let factory: IClassFactory = ClassFactory.into();
        unsafe { factory.query(riid, ppv) }
    }

    // Frame Server keeps sources around and unloading under it isn't worth the risk
    #[unsafe(no_mangle)]
    extern "system" fn DllCanUnloadNow() -> HRESULT {
        S_FALSE
    }
}
