use std::ffi::c_void;
use std::sync::Mutex;

use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows_core::{BOOL, GUID, IUnknown, Interface, PCWSTR, PWSTR, Ref, implement};

use crate::log::log;
use crate::source::MediaSource;

/// What Frame Server asks the class factory for. Activating it builds the media source.
#[implement(IMFActivate)]
pub struct Activator {
    attributes: IMFAttributes,
    source: Mutex<Option<IMFMediaSource>>,
}

impl Activator {
    pub fn create() -> windows_core::Result<IMFActivate> {
        let mut attributes = None;
        unsafe { MFCreateAttributes(&mut attributes, 1)? };
        let attributes =
            attributes.ok_or_else(|| windows_core::Error::from_hresult(windows::Win32::Foundation::E_OUTOFMEMORY))?;
        Ok(Self { attributes, source: Mutex::new(None) }.into())
    }
}

impl IMFActivate_Impl for Activator_Impl {
    fn ActivateObject(&self, riid: *const GUID, ppv: *mut *mut c_void) -> windows_core::Result<()> {
        let mut source = self.source.lock().unwrap();
        if source.is_none() {
            log!("activating the virtual camera source");
            *source = Some(MediaSource::create(&self.attributes)?);
        }
        let unknown: IUnknown = source.as_ref().unwrap().cast()?;
        unsafe { unknown.query(riid, ppv).ok() }
    }

    fn ShutdownObject(&self) -> windows_core::Result<()> {
        if let Some(source) = self.source.lock().unwrap().take() {
            unsafe { source.Shutdown()? };
        }
        Ok(())
    }

    fn DetachObject(&self) -> windows_core::Result<()> {
        self.source.lock().unwrap().take();
        Ok(())
    }
}

// IMFActivate is an attribute store too. Every call goes straight to the real one we hold.
impl IMFAttributes_Impl for Activator_Impl {
    fn GetItem(&self, key: *const GUID, value: *mut PROPVARIANT) -> windows_core::Result<()> {
        unsafe { (Interface::vtable(&self.attributes).GetItem)(self.attributes.as_raw(), key, value).ok() }
    }
    fn GetItemType(&self, key: *const GUID) -> windows_core::Result<MF_ATTRIBUTE_TYPE> {
        unsafe { self.attributes.GetItemType(key) }
    }
    fn CompareItem(&self, key: *const GUID, value: *const PROPVARIANT) -> windows_core::Result<BOOL> {
        unsafe { self.attributes.CompareItem(key, value) }
    }
    fn Compare(&self, theirs: Ref<IMFAttributes>, match_type: MF_ATTRIBUTES_MATCH_TYPE) -> windows_core::Result<BOOL> {
        unsafe { self.attributes.Compare(theirs.as_ref(), match_type) }
    }
    fn GetUINT32(&self, key: *const GUID) -> windows_core::Result<u32> {
        unsafe { self.attributes.GetUINT32(key) }
    }
    fn GetUINT64(&self, key: *const GUID) -> windows_core::Result<u64> {
        unsafe { self.attributes.GetUINT64(key) }
    }
    fn GetDouble(&self, key: *const GUID) -> windows_core::Result<f64> {
        unsafe { self.attributes.GetDouble(key) }
    }
    fn GetGUID(&self, key: *const GUID) -> windows_core::Result<GUID> {
        unsafe { self.attributes.GetGUID(key) }
    }
    fn GetStringLength(&self, key: *const GUID) -> windows_core::Result<u32> {
        unsafe { self.attributes.GetStringLength(key) }
    }
    fn GetString(&self, key: *const GUID, value: PWSTR, size: u32, length: *mut u32) -> windows_core::Result<()> {
        unsafe {
            (Interface::vtable(&self.attributes).GetString)(self.attributes.as_raw(), key, value, size, length).ok()
        }
    }
    fn GetAllocatedString(&self, key: *const GUID, value: *mut PWSTR, length: *mut u32) -> windows_core::Result<()> {
        unsafe { self.attributes.GetAllocatedString(key, value, length) }
    }
    fn GetBlobSize(&self, key: *const GUID) -> windows_core::Result<u32> {
        unsafe { self.attributes.GetBlobSize(key) }
    }
    fn GetBlob(&self, key: *const GUID, buffer: *mut u8, size: u32, written: *mut u32) -> windows_core::Result<()> {
        unsafe {
            (Interface::vtable(&self.attributes).GetBlob)(self.attributes.as_raw(), key, buffer, size, written).ok()
        }
    }
    fn GetAllocatedBlob(&self, key: *const GUID, buffer: *mut *mut u8, size: *mut u32) -> windows_core::Result<()> {
        unsafe { self.attributes.GetAllocatedBlob(key, buffer, size) }
    }
    fn GetUnknown(&self, key: *const GUID, riid: *const GUID, ppv: *mut *mut c_void) -> windows_core::Result<()> {
        unsafe { (Interface::vtable(&self.attributes).GetUnknown)(self.attributes.as_raw(), key, riid, ppv).ok() }
    }
    fn SetItem(&self, key: *const GUID, value: *const PROPVARIANT) -> windows_core::Result<()> {
        unsafe { self.attributes.SetItem(key, value) }
    }
    fn DeleteItem(&self, key: *const GUID) -> windows_core::Result<()> {
        unsafe { self.attributes.DeleteItem(key) }
    }
    fn DeleteAllItems(&self) -> windows_core::Result<()> {
        unsafe { self.attributes.DeleteAllItems() }
    }
    fn SetUINT32(&self, key: *const GUID, value: u32) -> windows_core::Result<()> {
        unsafe { self.attributes.SetUINT32(key, value) }
    }
    fn SetUINT64(&self, key: *const GUID, value: u64) -> windows_core::Result<()> {
        unsafe { self.attributes.SetUINT64(key, value) }
    }
    fn SetDouble(&self, key: *const GUID, value: f64) -> windows_core::Result<()> {
        unsafe { self.attributes.SetDouble(key, value) }
    }
    fn SetGUID(&self, key: *const GUID, value: *const GUID) -> windows_core::Result<()> {
        unsafe { self.attributes.SetGUID(key, value) }
    }
    fn SetString(&self, key: *const GUID, value: &PCWSTR) -> windows_core::Result<()> {
        unsafe { self.attributes.SetString(key, *value) }
    }
    fn SetBlob(&self, key: *const GUID, buffer: *const u8, size: u32) -> windows_core::Result<()> {
        unsafe { (Interface::vtable(&self.attributes).SetBlob)(self.attributes.as_raw(), key, buffer, size).ok() }
    }
    fn SetUnknown(&self, key: *const GUID, value: Ref<IUnknown>) -> windows_core::Result<()> {
        unsafe { self.attributes.SetUnknown(key, value.as_ref()) }
    }
    fn LockStore(&self) -> windows_core::Result<()> {
        unsafe { self.attributes.LockStore() }
    }
    fn UnlockStore(&self) -> windows_core::Result<()> {
        unsafe { self.attributes.UnlockStore() }
    }
    fn GetCount(&self) -> windows_core::Result<u32> {
        unsafe { self.attributes.GetCount() }
    }
    fn GetItemByIndex(&self, index: u32, key: *mut GUID, value: *mut PROPVARIANT) -> windows_core::Result<()> {
        unsafe {
            (Interface::vtable(&self.attributes).GetItemByIndex)(self.attributes.as_raw(), index, key, value).ok()
        }
    }
    fn CopyAllItems(&self, destination: Ref<IMFAttributes>) -> windows_core::Result<()> {
        unsafe { self.attributes.CopyAllItems(destination.as_ref()) }
    }
}
