use std::sync::Mutex;

use canon_shm::{FPS, FRAME_BYTES, HEIGHT, WIDTH};
use windows::Win32::Foundation::{E_INVALIDARG, ERROR_SET_NOT_FOUND, S_OK};
use windows::Win32::Media::KernelStreaming::{
    IKsControl, IKsControl_Impl, KSCAMERAPROFILE_Legacy, KSIDENTIFIER, PINNAME_VIDEO_CAPTURE,
};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows_core::{ComObject, GUID, IUnknown, Interface, Ref, implement, w};

use crate::log::log;
use crate::stream::MediaStream;

/// The live source Frame Server pulls from: one NV12 1080p stream.
#[implement(IMFMediaSourceEx, IMFGetService, IKsControl)]
pub struct MediaSource {
    inner: Mutex<Inner>,
}

struct Inner {
    // None once shut down
    queue: Option<IMFMediaEventQueue>,
    attributes: IMFAttributes,
    descriptor: IMFPresentationDescriptor,
    stream: Option<ComObject<MediaStream>>,
    announced: bool,
}

impl MediaSource {
    pub fn create(_activation: &IMFAttributes) -> windows_core::Result<IMFMediaSource> {
        unsafe {
            let queue = MFCreateEventQueue()?;
            let attributes = new_attributes(2)?;
            // A single legacy profile that admits our one mode, which is what Windows expects from a camera
            let profile = MFCreateSensorProfile(&KSCAMERAPROFILE_Legacy, 0, None)?;
            profile.AddProfileFilter(0, w!("((RES==;FRT<=30,1;SUT==))"))?;
            let profiles = MFCreateSensorProfileCollection()?;
            profiles.AddProfile(&profile)?;
            attributes.SetUnknown(&MF_DEVICEMFT_SENSORPROFILE_COLLECTION, &profiles)?;

            let media_type = nv12_type()?;
            let stream_descriptor = MFCreateStreamDescriptor(0, &[Some(media_type.clone())])?;
            stream_descriptor
                .GetMediaTypeHandler()?
                .SetCurrentMediaType(&media_type)?;
            stream_descriptor.SetGUID(&MF_DEVICESTREAM_STREAM_CATEGORY, &PINNAME_VIDEO_CAPTURE)?;
            stream_descriptor.SetUINT32(&MF_DEVICESTREAM_STREAM_ID, 0)?;
            stream_descriptor.SetUINT32(&MF_DEVICESTREAM_FRAMESERVER_SHARED, 1)?;
            stream_descriptor.SetUINT32(
                &MF_DEVICESTREAM_ATTRIBUTE_FRAMESOURCE_TYPES,
                MFFrameSourceTypes_Color.0 as u32,
            )?;

            let descriptor =
                MFCreatePresentationDescriptor(Some(&[Some(stream_descriptor.clone())]))?;
            descriptor.SelectStream(0)?;

            let object = ComObject::new(MediaSource {
                inner: Mutex::new(Inner {
                    queue: Some(queue),
                    attributes,
                    descriptor,
                    stream: None,
                    announced: false,
                }),
            });
            let source: IMFMediaSource = object.to_interface::<IMFMediaSourceEx>().into();
            let stream = MediaStream::create(source.clone(), stream_descriptor)?;
            object.inner.lock().unwrap().stream = Some(stream);
            Ok(source)
        }
    }

    fn queue(&self) -> windows_core::Result<IMFMediaEventQueue> {
        self.inner
            .lock()
            .unwrap()
            .queue
            .clone()
            .ok_or_else(|| MF_E_SHUTDOWN.into())
    }
}

pub fn new_attributes(size: u32) -> windows_core::Result<IMFAttributes> {
    let mut attributes = None;
    unsafe { MFCreateAttributes(&mut attributes, size)? };
    attributes.ok_or_else(|| windows::Win32::Foundation::E_OUTOFMEMORY.into())
}

fn nv12_type() -> windows_core::Result<IMFMediaType> {
    unsafe {
        let media_type = MFCreateMediaType()?;
        media_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        media_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
        // Sizes and ratios pack two u32s into one u64, high half first
        media_type.SetUINT64(
            &MF_MT_FRAME_SIZE,
            (u64::from(WIDTH) << 32) | u64::from(HEIGHT),
        )?;
        media_type.SetUINT64(&MF_MT_FRAME_RATE, (u64::from(FPS) << 32) | 1)?;
        media_type.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1 << 32) | 1)?;
        media_type.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        media_type.SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1)?;
        media_type.SetUINT32(&MF_MT_FIXED_SIZE_SAMPLES, 1)?;
        media_type.SetUINT32(&MF_MT_SAMPLE_SIZE, FRAME_BYTES as u32)?;
        media_type.SetUINT32(&MF_MT_DEFAULT_STRIDE, WIDTH)?;
        Ok(media_type)
    }
}

impl IMFMediaEventGenerator_Impl for MediaSource_Impl {
    fn GetEvent(
        &self,
        flags: MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS,
    ) -> windows_core::Result<IMFMediaEvent> {
        // Never hold the lock here, GetEvent can block until something is queued
        unsafe { self.queue()?.GetEvent(flags.0) }
    }
    fn BeginGetEvent(
        &self,
        callback: Ref<IMFAsyncCallback>,
        state: Ref<IUnknown>,
    ) -> windows_core::Result<()> {
        unsafe {
            self.queue()?
                .BeginGetEvent(callback.as_ref(), state.as_ref())
        }
    }
    fn EndGetEvent(&self, result: Ref<IMFAsyncResult>) -> windows_core::Result<IMFMediaEvent> {
        unsafe { self.queue()?.EndGetEvent(result.as_ref()) }
    }
    fn QueueEvent(
        &self,
        met: u32,
        extended: *const GUID,
        status: windows_core::HRESULT,
        value: *const PROPVARIANT,
    ) -> windows_core::Result<()> {
        unsafe {
            self.queue()?
                .QueueEventParamVar(met, extended, status, value)
        }
    }
}

impl IMFMediaSource_Impl for MediaSource_Impl {
    fn GetCharacteristics(&self) -> windows_core::Result<u32> {
        self.queue()?;
        Ok(MFMEDIASOURCE_IS_LIVE.0 as u32)
    }

    fn CreatePresentationDescriptor(&self) -> windows_core::Result<IMFPresentationDescriptor> {
        let inner = self.inner.lock().unwrap();
        inner.queue.as_ref().ok_or(MF_E_SHUTDOWN)?;
        unsafe { inner.descriptor.Clone() }
    }

    fn Start(
        &self,
        descriptor: Ref<IMFPresentationDescriptor>,
        time_format: *const GUID,
        start: *const PROPVARIANT,
    ) -> windows_core::Result<()> {
        let mut inner = self.inner.lock().unwrap();
        let queue = inner.queue.clone().ok_or(MF_E_SHUTDOWN)?;
        if descriptor.is_null() {
            return Err(E_INVALIDARG.into());
        }
        if !time_format.is_null() && unsafe { *time_format } != GUID::zeroed() {
            return Err(MF_E_UNSUPPORTED_TIME_FORMAT.into());
        }
        let stream = inner.stream.clone().ok_or(MF_E_SHUTDOWN)?;
        let stream_unknown: IUnknown = stream.to_interface();
        let event = if inner.announced {
            MEUpdatedStream
        } else {
            MENewStream
        };
        inner.announced = true;
        drop(inner);

        log!("source starting");
        unsafe {
            queue.QueueEventParamUnk(event.0 as u32, &GUID::zeroed(), S_OK, &stream_unknown)?;
            queue.QueueEventParamVar(MESourceStarted.0 as u32, &GUID::zeroed(), S_OK, start)?;
        }
        stream.start(start)
    }

    fn Stop(&self) -> windows_core::Result<()> {
        let inner = self.inner.lock().unwrap();
        let queue = inner.queue.clone().ok_or(MF_E_SHUTDOWN)?;
        let stream = inner.stream.clone();
        drop(inner);
        log!("source stopping");
        if let Some(stream) = stream {
            stream.stop()?;
        }
        unsafe {
            queue.QueueEventParamVar(
                MESourceStopped.0 as u32,
                &GUID::zeroed(),
                S_OK,
                std::ptr::null(),
            )
        }
    }

    // A live camera can't pause
    fn Pause(&self) -> windows_core::Result<()> {
        Err(MF_E_INVALID_STATE_TRANSITION.into())
    }

    fn Shutdown(&self) -> windows_core::Result<()> {
        let mut inner = self.inner.lock().unwrap();
        let queue = inner.queue.take();
        let stream = inner.stream.take();
        drop(inner);
        log!("source shutting down");
        if let Some(stream) = stream {
            stream.shutdown();
        }
        if let Some(queue) = queue {
            unsafe { queue.Shutdown()? };
        }
        Ok(())
    }
}

impl IMFMediaSourceEx_Impl for MediaSource_Impl {
    fn GetSourceAttributes(&self) -> windows_core::Result<IMFAttributes> {
        let inner = self.inner.lock().unwrap();
        inner.queue.as_ref().ok_or(MF_E_SHUTDOWN)?;
        Ok(inner.attributes.clone())
    }

    fn GetStreamAttributes(&self, stream_id: u32) -> windows_core::Result<IMFAttributes> {
        let inner = self.inner.lock().unwrap();
        let stream = inner.stream.as_ref().ok_or(MF_E_SHUTDOWN)?;
        if stream_id != 0 {
            return Err(MF_E_INVALIDSTREAMNUMBER.into());
        }
        stream.descriptor().cast()
    }

    // We hand out system memory frames, so a D3D device manager is welcome but unused
    fn SetD3DManager(&self, _manager: Ref<IUnknown>) -> windows_core::Result<()> {
        Ok(())
    }
}

impl IMFGetService_Impl for MediaSource_Impl {
    fn GetService(
        &self,
        _service: *const GUID,
        _riid: *const GUID,
        _ppv: *mut *mut std::ffi::c_void,
    ) -> windows_core::Result<()> {
        Err(MF_E_UNSUPPORTED_SERVICE.into())
    }
}

/// No camera controls yet. Frame Server asks anyway, and "no such property set" is the polite answer.
pub fn ks_not_found() -> windows_core::Error {
    windows_core::HRESULT::from_win32(ERROR_SET_NOT_FOUND.0).into()
}

impl IKsControl_Impl for MediaSource_Impl {
    fn KsProperty(
        &self,
        _: *const KSIDENTIFIER,
        _: u32,
        _: *mut std::ffi::c_void,
        _: u32,
        _: *mut u32,
    ) -> windows_core::Result<()> {
        Err(ks_not_found())
    }
    fn KsMethod(
        &self,
        _: *const KSIDENTIFIER,
        _: u32,
        _: *mut std::ffi::c_void,
        _: u32,
        _: *mut u32,
    ) -> windows_core::Result<()> {
        Err(ks_not_found())
    }
    fn KsEvent(
        &self,
        _: *const KSIDENTIFIER,
        _: u32,
        _: *mut std::ffi::c_void,
        _: u32,
        _: *mut u32,
    ) -> windows_core::Result<()> {
        Err(ks_not_found())
    }
}
