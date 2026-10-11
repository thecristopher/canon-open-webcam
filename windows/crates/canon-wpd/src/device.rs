use canon_ptp::{DataPhase, Operation, PtpError, Reply, Result, Transport};
use windows::Win32::Devices::PortableDevices::*;
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PropVariantClear};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
use windows::Win32::System::Variant::{VT_I4, VT_UI4};
use windows::core::{HSTRING, PCWSTR, PWSTR};

// Canon's USB vendor id, as it shows up in the PnP id Windows hands out
const CANON_VENDOR: &str = "vid_04a9";
// Live view frames are about 250KB, so one read usually covers a whole frame
const READ_CHUNK: u64 = 1024 * 1024;
// SECURITY_IMPERSONATION from winbase.h, what Microsoft's WPD samples pass
const SECURITY_IMPERSONATION: u32 = 0x0002_0000;

#[derive(Clone, Debug)]
pub struct DeviceEntry {
    pub id: String,
    pub name: String,
    pub manufacturer: String,
    pub description: String,
}

impl DeviceEntry {
    pub fn is_canon(&self) -> bool {
        self.id.to_lowercase().contains(CANON_VENDOR) || self.manufacturer.to_lowercase().contains("canon")
    }
}

/// Every portable device Windows knows about right now: cameras, phones, media players.
pub fn list_devices() -> windows::core::Result<Vec<DeviceEntry>> {
    unsafe {
        let manager: IPortableDeviceManager = CoCreateInstance(&PortableDeviceManager, None, CLSCTX_INPROC_SERVER)?;
        let mut count = 0u32;
        manager.GetDevices(std::ptr::null_mut(), &mut count)?;
        let mut ids = vec![PWSTR::null(); count as usize];
        manager.GetDevices(ids.as_mut_ptr(), &mut count)?;

        let mut devices = Vec::with_capacity(count as usize);
        for raw in ids.into_iter().take(count as usize) {
            let id = PCWSTR(raw.0);
            devices.push(DeviceEntry {
                id: raw.to_string().unwrap_or_default(),
                name: device_string(|b, n| manager.GetDeviceFriendlyName(id, b, n)),
                manufacturer: device_string(|b, n| manager.GetDeviceManufacturer(id, b, n)),
                description: device_string(|b, n| manager.GetDeviceDescription(id, b, n)),
            });
            CoTaskMemFree(Some(raw.0 as _));
        }
        Ok(devices)
    }
}

/// The manager's string getters all follow the same ask for the length, then fill the buffer dance.
fn device_string(get: impl Fn(PWSTR, *mut u32) -> windows::core::Result<()>) -> String {
    let mut length = 0u32;
    if get(PWSTR::null(), &mut length).is_err() || length == 0 {
        return String::new();
    }
    let mut buffer = vec![0u16; length as usize];
    if get(PWSTR(buffer.as_mut_ptr()), &mut length).is_err() {
        return String::new();
    }
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

/// A Canon opened through WPD, running PTP operations as MTP vendor extension commands.
pub struct WpdTransport {
    device: IPortableDevice,
    name: String,
}

impl WpdTransport {
    pub fn open(entry: &DeviceEntry) -> windows::core::Result<Self> {
        unsafe {
            let device: IPortableDevice = CoCreateInstance(&PortableDeviceFTM, None, CLSCTX_INPROC_SERVER)?;
            let client = new_values()?;
            client.SetStringValue(&WPD_CLIENT_NAME, &HSTRING::from("Canon Webcam"))?;
            client.SetUnsignedIntegerValue(&WPD_CLIENT_MAJOR_VERSION, 0)?;
            client.SetUnsignedIntegerValue(&WPD_CLIENT_MINOR_VERSION, 1)?;
            client.SetUnsignedIntegerValue(&WPD_CLIENT_REVISION, 0)?;
            client.SetUnsignedIntegerValue(&WPD_CLIENT_SECURITY_QUALITY_OF_SERVICE, SECURITY_IMPERSONATION)?;
            device.Open(&HSTRING::from(entry.id.as_str()), &client)?;
            Ok(Self { device, name: entry.name.clone() })
        }
    }

    /// Opcodes beyond the PTP standard that the camera told the driver it understands.
    pub fn vendor_operations(&self) -> windows::core::Result<Vec<u32>> {
        unsafe {
            let results = self.send(&WPD_COMMAND_MTP_EXT_GET_SUPPORTED_VENDOR_OPCODES, |_| Ok(()))?;
            let codes =
                results.GetIPortableDevicePropVariantCollectionValue(&WPD_PROPERTY_MTP_EXT_VENDOR_OPERATION_CODES)?;
            collection_to_u32s(&codes)
        }
    }

    fn send(
        &self,
        command: &PROPERTYKEY,
        fill: impl FnOnce(&IPortableDeviceValues) -> windows::core::Result<()>,
    ) -> windows::core::Result<IPortableDeviceValues> {
        unsafe {
            let params = new_values()?;
            params.SetGuidValue(&WPD_PROPERTY_COMMON_COMMAND_CATEGORY, &command.fmtid)?;
            params.SetUnsignedIntegerValue(&WPD_PROPERTY_COMMON_COMMAND_ID, command.pid)?;
            fill(&params)?;
            let results = self.device.SendCommand(0, &params)?;
            // SendCommand only fails for transport trouble, the command's own verdict rides in the results
            results.GetErrorValue(&WPD_PROPERTY_COMMON_HRESULT)?.ok()?;
            Ok(results)
        }
    }

    fn start(
        &self,
        command: &PROPERTYKEY,
        operation: Operation,
        parameters: &[u32],
        extra: impl FnOnce(&IPortableDeviceValues) -> windows::core::Result<()>,
    ) -> windows::core::Result<IPortableDeviceValues> {
        let collection = u32s_to_collection(parameters)?;
        self.send(command, |p| unsafe {
            p.SetUnsignedIntegerValue(&WPD_PROPERTY_MTP_EXT_OPERATION_CODE, operation.code() as u32)?;
            p.SetIPortableDevicePropVariantCollectionValue(&WPD_PROPERTY_MTP_EXT_OPERATION_PARAMS, &collection)?;
            extra(p)
        })
    }

    fn without_data(&self, operation: Operation, parameters: &[u32]) -> windows::core::Result<Reply> {
        let results =
            self.start(&WPD_COMMAND_MTP_EXT_EXECUTE_COMMAND_WITHOUT_DATA_PHASE, operation, parameters, |_| Ok(()))?;
        response(&results, Vec::new())
    }

    fn reading(&self, operation: Operation, parameters: &[u32]) -> windows::core::Result<Reply> {
        let started =
            self.start(&WPD_COMMAND_MTP_EXT_EXECUTE_COMMAND_WITH_DATA_TO_READ, operation, parameters, |_| Ok(()))?;
        let context = unsafe { started.GetStringValue(&WPD_PROPERTY_MTP_EXT_TRANSFER_CONTEXT)? };
        let total = unsafe { started.GetUnsignedLargeIntegerValue(&WPD_PROPERTY_MTP_EXT_TRANSFER_TOTAL_DATA_SIZE)? };

        let mut data = Vec::with_capacity(total.min(16 * READ_CHUNK) as usize);
        // Keep reading until the promised size arrives or the camera runs dry, whichever comes first
        while (data.len() as u64) < total {
            let want = (total - data.len() as u64).min(READ_CHUNK);
            let scratch = vec![0u8; want as usize];
            let results = self.send(&WPD_COMMAND_MTP_EXT_READ_DATA, |p| unsafe {
                p.SetStringValue(&WPD_PROPERTY_MTP_EXT_TRANSFER_CONTEXT, PCWSTR(context.0))?;
                p.SetUnsignedLargeIntegerValue(&WPD_PROPERTY_MTP_EXT_TRANSFER_NUM_BYTES_TO_READ, want)?;
                p.SetBufferValue(&WPD_PROPERTY_MTP_EXT_TRANSFER_DATA, &scratch)
            })?;
            let read = unsafe { results.GetUnsignedLargeIntegerValue(&WPD_PROPERTY_MTP_EXT_TRANSFER_NUM_BYTES_READ)? };
            if read == 0 {
                break;
            }
            data.extend_from_slice(&take_buffer(&results, &WPD_PROPERTY_MTP_EXT_TRANSFER_DATA)?[..read as usize]);
        }
        self.finish(context, data)
    }

    fn writing(&self, operation: Operation, parameters: &[u32], outgoing: &[u8]) -> windows::core::Result<Reply> {
        let started =
            self.start(&WPD_COMMAND_MTP_EXT_EXECUTE_COMMAND_WITH_DATA_TO_WRITE, operation, parameters, |p| unsafe {
                p.SetUnsignedLargeIntegerValue(&WPD_PROPERTY_MTP_EXT_TRANSFER_TOTAL_DATA_SIZE, outgoing.len() as u64)
            })?;
        let context = unsafe { started.GetStringValue(&WPD_PROPERTY_MTP_EXT_TRANSFER_CONTEXT)? };
        self.send(&WPD_COMMAND_MTP_EXT_WRITE_DATA, |p| unsafe {
            p.SetStringValue(&WPD_PROPERTY_MTP_EXT_TRANSFER_CONTEXT, PCWSTR(context.0))?;
            p.SetUnsignedLargeIntegerValue(&WPD_PROPERTY_MTP_EXT_TRANSFER_NUM_BYTES_TO_WRITE, outgoing.len() as u64)?;
            p.SetBufferValue(&WPD_PROPERTY_MTP_EXT_TRANSFER_DATA, outgoing)
        })?;
        self.finish(context, Vec::new())
    }

    /// Closes a data transfer, which is also where the camera's response code finally shows up.
    fn finish(&self, context: PWSTR, data: Vec<u8>) -> windows::core::Result<Reply> {
        let results = self.send(&WPD_COMMAND_MTP_EXT_END_DATA_TRANSFER, |p| unsafe {
            p.SetStringValue(&WPD_PROPERTY_MTP_EXT_TRANSFER_CONTEXT, PCWSTR(context.0))
        });
        unsafe { CoTaskMemFree(Some(context.0 as _)) };
        response(&results?, data)
    }
}

impl Transport for WpdTransport {
    // The MTP driver opened the session when we opened the device, and closes it when we let go
    fn open_session(&mut self) -> Result<()> {
        Ok(())
    }

    fn close_session(&mut self) -> Result<()> {
        Ok(())
    }

    fn transact(&mut self, operation: Operation, parameters: &[u32], phase: DataPhase<'_>) -> Result<Reply> {
        let result = match phase {
            DataPhase::None => self.without_data(operation, parameters),
            DataPhase::Read => self.reading(operation, parameters),
            DataPhase::Write(outgoing) => self.writing(operation, parameters, outgoing),
        };
        result.map_err(|e| PtpError::Transport(format!("WPD said {e} during 0x{:04X}", operation.code())))
    }

    fn model_name(&self) -> Option<String> {
        // WPD names a Canon after its model, like "Canon EOS 1500D"
        (!self.name.is_empty()).then(|| self.name.clone())
    }
}

impl Drop for WpdTransport {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.Close();
        }
    }
}

fn new_values() -> windows::core::Result<IPortableDeviceValues> {
    unsafe { CoCreateInstance(&PortableDeviceValues, None, CLSCTX_INPROC_SERVER) }
}

fn response(results: &IPortableDeviceValues, data: Vec<u8>) -> windows::core::Result<Reply> {
    unsafe {
        let code = results.GetUnsignedIntegerValue(&WPD_PROPERTY_MTP_EXT_RESPONSE_CODE)? as u16;
        let parameters = results
            .GetIPortableDevicePropVariantCollectionValue(&WPD_PROPERTY_MTP_EXT_RESPONSE_PARAMS)
            .and_then(|c| collection_to_u32s(&c))
            .unwrap_or_default();
        Ok(Reply { code, data, parameters })
    }
}

fn take_buffer(values: &IPortableDeviceValues, key: &PROPERTYKEY) -> windows::core::Result<Vec<u8>> {
    unsafe {
        let mut pointer = std::ptr::null_mut();
        let mut length = 0u32;
        values.GetBufferValue(key, &mut pointer, &mut length)?;
        let bytes = std::slice::from_raw_parts(pointer, length as usize).to_vec();
        CoTaskMemFree(Some(pointer as _));
        Ok(bytes)
    }
}

fn u32s_to_collection(values: &[u32]) -> windows::core::Result<IPortableDevicePropVariantCollection> {
    unsafe {
        let collection: IPortableDevicePropVariantCollection =
            CoCreateInstance(&PortableDevicePropVariantCollection, None, CLSCTX_INPROC_SERVER)?;
        for &value in values {
            let mut item = PROPVARIANT::default();
            (*item.Anonymous.Anonymous).vt = VT_UI4;
            (*item.Anonymous.Anonymous).Anonymous.ulVal = value;
            collection.Add(&item)?;
        }
        Ok(collection)
    }
}

fn collection_to_u32s(collection: &IPortableDevicePropVariantCollection) -> windows::core::Result<Vec<u32>> {
    unsafe {
        // The metadata marks these out parameters const, but the driver writes through them
        let mut count = 0u32;
        collection.GetCount((&raw mut count).cast_const())?;
        let mut values = Vec::with_capacity(count as usize);
        for index in 0..count {
            let mut item = PROPVARIANT::default();
            collection.GetAt(index, (&raw mut item).cast_const())?;
            let inner = &*item.Anonymous.Anonymous;
            // PTP parameters are 32 bit words, some drivers just call them signed
            if inner.vt == VT_UI4 || inner.vt == VT_I4 {
                values.push(inner.Anonymous.ulVal);
            }
            PropVariantClear(&mut item)?;
        }
        Ok(values)
    }
}
