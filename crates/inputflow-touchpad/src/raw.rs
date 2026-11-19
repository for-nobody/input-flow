use std::{
    io,
    mem::{offset_of, size_of},
    ptr::null_mut,
    slice,
};

use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, GetLastError};
use windows_sys::Win32::UI::Input::*;

fn api_error(operation: &str) -> io::Error {
    io::Error::other(format!("{operation}: {}", io::Error::last_os_error()))
}

pub fn raw_devices() -> io::Result<Vec<RAWINPUTDEVICELIST>> {
    let item_size = size_of::<RAWINPUTDEVICELIST>() as u32;
    for _ in 0..3 {
        let mut count = 0;
        // SAFETY: A null list queries the count into a writable u32.
        if unsafe { GetRawInputDeviceList(null_mut(), &mut count, item_size) } == u32::MAX {
            return Err(api_error("GetRawInputDeviceList(count)"));
        }
        if count == 0 {
            return Ok(Vec::new());
        }
        let mut devices = vec![RAWINPUTDEVICELIST::default(); count as usize];
        // SAFETY: The aligned array has room for count records of item_size.
        let copied = unsafe { GetRawInputDeviceList(devices.as_mut_ptr(), &mut count, item_size) };
        if copied != u32::MAX {
            devices.truncate(copied as usize);
            return Ok(devices);
        }
        if unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
            return Err(api_error("GetRawInputDeviceList(list)"));
        }
    }
    Err(io::Error::other(
        "GetRawInputDeviceList: device count changed during three attempts",
    ))
}

// Win32 validates the opaque handle and writes only to the initialized output buffer.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn device_info(handle: windows_sys::Win32::Foundation::HANDLE) -> io::Result<RID_DEVICE_INFO> {
    let mut bytes = size_of::<RID_DEVICE_INFO>() as u32;
    let mut info = RID_DEVICE_INFO {
        cbSize: bytes,
        ..Default::default()
    };
    // SAFETY: info is aligned, with cbSize and the output capacity set.
    let result = unsafe {
        GetRawInputDeviceInfoW(
            handle,
            RIDI_DEVICEINFO,
            (&mut info as *mut RID_DEVICE_INFO).cast(),
            &mut bytes,
        )
    };
    if result == u32::MAX {
        return Err(api_error("GetRawInputDeviceInfoW(RIDI_DEVICEINFO)"));
    }
    Ok(info)
}

#[derive(Clone, Copy)]
pub struct Packet<'a> {
    pub header: RAWINPUTHEADER,
    pub hid: Option<HidBatch<'a>>,
}

#[derive(Clone, Copy)]
pub struct HidBatch<'a> {
    pub report_size: usize,
    pub bytes: &'a [u8],
}

impl HidBatch<'_> {
    pub fn count(self) -> u64 {
        (self.bytes.len() / self.report_size) as u64
    }
}

// Win32 validates the opaque message handle and writes only to our aligned buffer.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn read_packet(input: HRAWINPUT, buffer: &mut Vec<usize>) -> io::Result<Option<Packet<'_>>> {
    let header_size = size_of::<RAWINPUTHEADER>();
    let mut bytes = 0;
    // SAFETY: input comes from WM_INPUT and a null buffer queries its size.
    let result =
        unsafe { GetRawInputData(input, RID_INPUT, null_mut(), &mut bytes, header_size as u32) };
    if result == u32::MAX {
        return Err(api_error("GetRawInputData(size)"));
    }
    if (bytes as usize) < header_size || bytes > 64 * 1024 {
        return Ok(None);
    }

    buffer.resize((bytes as usize).div_ceil(size_of::<usize>()), 0);
    // SAFETY: buffer is aligned for Win32 and holds at least bytes writable bytes.
    let copied = unsafe {
        GetRawInputData(
            input,
            RID_INPUT,
            buffer.as_mut_ptr().cast(),
            &mut bytes,
            header_size as u32,
        )
    };
    if copied == u32::MAX {
        return Err(api_error("GetRawInputData(data)"));
    }
    let copied = copied as usize;
    if copied < header_size || copied > buffer.len() * size_of::<usize>() {
        return Ok(None);
    }
    // SAFETY: The initialized allocation contains the complete header and copied bytes.
    let header = unsafe { buffer.as_ptr().cast::<RAWINPUTHEADER>().read_unaligned() };
    let size = header.dwSize as usize;
    if size < header_size || size > copied {
        return Ok(None);
    }
    let hid = if header.dwType == RIM_TYPEHID {
        // SAFETY: size lies within the populated, initialized allocation.
        let packet = unsafe { slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), size) };
        packet.get(offset_of!(RAWINPUT, data)..).and_then(hid_batch)
    } else {
        None
    };
    Ok(Some(Packet { header, hid }))
}

pub fn hid_batch(hid: &[u8]) -> Option<HidBatch<'_>> {
    let (fields, payload) = hid.split_at_checked(offset_of!(RAWHID, bRawData))?;
    let size = u32::from_le_bytes(fields.get(..4)?.try_into().ok()?) as usize;
    let count = u32::from_le_bytes(fields.get(4..8)?.try_into().ok()?) as usize;
    if size == 0 || count == 0 {
        return None;
    }
    Some(HidBatch {
        report_size: size,
        bytes: payload.get(..count.checked_mul(size)?)?,
    })
}
