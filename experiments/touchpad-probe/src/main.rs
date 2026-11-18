#![cfg(windows)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::mem::{offset_of, size_of};
use std::ptr::null_mut;
use std::slice;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const TOUCHPAD_PAGE: u16 = 0x0d;
const TOUCHPAD_USAGE: u16 = 0x05;
const READY: u32 = WM_APP + 1;
const INSTRUCTIONS: &str = "Keep this window in the foreground.\r\n\r\n\
    Count reports: 1  Idle    2  Touch and lift    3  Slide    4  Mouse\r\n\r\n\
    Track contacts:\r\n5  Short touch and lift\r\n6  Hold still for one second, then lift\r\n\
    7  Slide slowly left to right, then lift\r\n\r\nEnter  End the current sample      Esc  Close";

macro_rules! log {
    ($($arg:tt)*) => { crate::write_log(format_args!($($arg)*)) };
}

mod contact;
mod hid;

use contact::{Frame, Tracker, Transition};

thread_local! {
    static PROBE: RefCell<Option<Probe>> = const { RefCell::new(None) };
    static LOG_FILE: RefCell<Option<File>> = const { RefCell::new(None) };
}

fn write_log(message: fmt::Arguments<'_>) {
    let mut stdout = io::stdout().lock();
    let _ = writeln!(stdout, "{message}");
    LOG_FILE.with_borrow_mut(|file| {
        if let Some(writer) = file.as_mut()
            && let Err(error) = writeln!(writer, "{message}")
        {
            let _ = writeln!(stdout, "log_file_write_failed {error}");
            *file = None;
        }
    });
}

fn open_log_file() -> io::Result<()> {
    let executable = std::env::current_exe()?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let path = executable.with_file_name(format!(
        "touchpad-probe-{}-{}.log",
        std::process::id(),
        now.as_millis()
    ));
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    LOG_FILE.set(Some(file));
    log!("log_file={}", path.display());
    Ok(())
}

#[derive(Clone, Copy)]
enum Source {
    Touchpad,
    Other,
    Unknown,
}

#[derive(Default)]
struct Counts {
    messages: u64,
    foreground: u64,
    sink: u64,
    hid: u64,
    mouse: u64,
    keyboard: u64,
    other: u64,
    zero_handles: u64,
    read_errors: u64,
    invalid_packets: u64,
    reports: u64,
    touchpad_reports: u64,
    unknown_reports: u64,
    other_reports: u64,
}

impl Counts {
    fn log(&self, label: &str) {
        log!(
            "{label} wm_input={} foreground={} sink={} hid_messages={} mouse={} keyboard={} other={} zero_handles={} read_errors={} invalid_packets={} reports={} touchpad_reports={} unknown_reports={} other_reports={}",
            self.messages,
            self.foreground,
            self.sink,
            self.hid,
            self.mouse,
            self.keyboard,
            self.other,
            self.zero_handles,
            self.read_errors,
            self.invalid_packets,
            self.reports,
            self.touchpad_reports,
            self.unknown_reports,
            self.other_reports
        );
    }
}

struct Sample {
    name: &'static str,
    started: Instant,
    foreground_at_start: bool,
    focus_losses: u32,
    counts: Counts,
    contacts: Option<ContactCounts>,
}

struct Probe {
    devices: HashMap<HANDLE, Source>,
    contact_devices: HashMap<HANDLE, ContactDevice>,
    buffer: Vec<usize>,
    totals: Counts,
    sample: Option<Sample>,
}

#[derive(Default)]
struct ContactCounts {
    down: u64,
    up: u64,
    incomplete: u64,
    single: u64,
    empty: u64,
    multiple: u64,
    ignored: u64,
    rejected: u64,
    unsupported: u64,
}

struct ContactDevice {
    number: usize,
    decoder: hid::Decoder,
    tracker: Tracker,
}

impl ContactDevice {
    fn log_transition(
        &self,
        sample: &str,
        counts: &mut ContactCounts,
        transition: Option<Transition>,
    ) {
        let device = self.number;
        match transition {
            Some(Transition::Down { id, start }) => {
                counts.down += 1;
                log!(
                    "DOWN sample={sample} device={device} id={id} start=({},{})",
                    start.x,
                    start.y
                );
            }
            Some(Transition::Up {
                id,
                start,
                end,
                duration,
                max_gap,
                reports,
            }) => {
                counts.up += 1;
                log!(
                    "UP sample={sample} device={device} id={id} start=({},{}) end=({},{}) duration_ms={} reports={reports} max_inter_report_ms={}",
                    start.x,
                    start.y,
                    end.x,
                    end.y,
                    duration.as_millis(),
                    max_gap.as_millis()
                );
            }
            Some(Transition::Incomplete { id, reason }) => {
                counts.incomplete += 1;
                log!("INCOMPLETE sample={sample} device={device} id={id} reason={reason}");
            }
            None => {}
        }
    }

    fn observe_batch(
        &mut self,
        sample: &str,
        counts: &mut ContactCounts,
        batch: HidBatch<'_>,
        received: Instant,
    ) {
        for report in batch.bytes.chunks_exact(batch.report_size) {
            let frame = match self.decoder.decode(report) {
                Ok(Some(frame)) => frame,
                Ok(None) => {
                    counts.ignored += 1;
                    continue;
                }
                Err(error) => {
                    if counts.rejected == 0 {
                        log!(
                            "first_contact_decode_error sample={sample} device={} {error}",
                            self.number
                        );
                    }
                    counts.rejected += 1;
                    self.interrupt(sample, counts, "decode_error");
                    continue;
                }
            };
            match frame {
                Frame::Single(_) => counts.single += 1,
                Frame::Empty => counts.empty += 1,
                Frame::Multiple => counts.multiple += 1,
            }
            let transition = self.tracker.observe(frame, received);
            self.log_transition(sample, counts, transition);
        }
    }

    fn interrupt(&mut self, sample: &str, counts: &mut ContactCounts, reason: &'static str) {
        let transition = self.tracker.interrupt(reason);
        self.log_transition(sample, counts, transition);
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

fn api_error(name: &str) -> io::Error {
    io::Error::other(format!("{name}: {}", io::Error::last_os_error()))
}

fn raw_devices() -> io::Result<Vec<RAWINPUTDEVICELIST>> {
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

fn classify_device(handle: HANDLE) -> io::Result<Source> {
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
    if info.dwType != RIM_TYPEHID {
        return Ok(Source::Other);
    }
    // SAFETY: The device type selects the HID arm of the Win32 union.
    let hid = unsafe { info.Anonymous.hid };
    if (hid.usUsagePage, hid.usUsage) == (TOUCHPAD_PAGE, TOUCHPAD_USAGE) {
        log!(
            "touchpad_collection vendor=0x{:04x} product=0x{:04x} usage=0x{:04x}/0x{:04x}",
            hid.dwVendorId,
            hid.dwProductId,
            hid.usUsagePage,
            hid.usUsage
        );
        Ok(Source::Touchpad)
    } else {
        Ok(Source::Other)
    }
}

fn enumerate() -> HashMap<HANDLE, Source> {
    let devices = match raw_devices() {
        Ok(devices) => devices,
        Err(error) => {
            log!("enumeration_failed {error}");
            return HashMap::new();
        }
    };
    let mut sources = HashMap::new();
    let mut hid_count = 0;
    let mut candidates = 0;
    let mut errors = 0;
    for device in devices.iter().filter(|device| device.dwType == RIM_TYPEHID) {
        hid_count += 1;
        match classify_device(device.hDevice) {
            Ok(source) => {
                if matches!(source, Source::Touchpad) {
                    candidates += 1;
                }
                sources.insert(device.hDevice, source);
            }
            Err(error) => {
                errors += 1;
                log!("enumeration_device_info_failed {error}");
            }
        }
    }
    log!(
        "enumeration devices={} hid_collections={hid_count} touchpad_candidates={candidates} info_errors={errors}",
        devices.len()
    );
    sources
}

#[derive(Clone, Copy)]
struct Packet<'a> {
    header: RAWINPUTHEADER,
    hid: Option<HidBatch<'a>>,
}

#[derive(Clone, Copy)]
struct HidBatch<'a> {
    report_size: usize,
    bytes: &'a [u8],
}

impl HidBatch<'_> {
    fn count(self) -> u64 {
        (self.bytes.len() / self.report_size) as u64
    }
}

fn read_packet(input: HRAWINPUT, buffer: &mut Vec<usize>) -> io::Result<Option<Packet<'_>>> {
    let header_size = size_of::<RAWINPUTHEADER>();
    let mut bytes = 0;
    // SAFETY: input comes from WM_INPUT and a null buffer queries its size.
    let result =
        unsafe { GetRawInputData(input, RID_INPUT, null_mut(), &mut bytes, header_size as u32) };
    if result == u32::MAX {
        return Err(api_error("GetRawInputData(size)"));
    }
    if (bytes as usize) < header_size {
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

fn hid_batch(hid: &[u8]) -> Option<HidBatch<'_>> {
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

impl Probe {
    fn on_input(&mut self, input: HRAWINPUT, code: WPARAM) {
        let received = Instant::now();
        let packet = read_packet(input, &mut self.buffer);
        if let Err(error) = &packet
            && self.totals.read_errors == 0
        {
            log!("first_read_error {error}");
        }
        let source = match &packet {
            Ok(Some(packet)) if packet.hid.is_some() && !packet.header.hDevice.is_null() => *self
                .devices
                .entry(packet.header.hDevice)
                .or_insert_with(|| {
                    classify_device(packet.header.hDevice).unwrap_or_else(|error| {
                        log!("message_device_info_failed {error}");
                        Source::Unknown
                    })
                }),
            _ => Source::Unknown,
        };

        for counts in std::iter::once(&mut self.totals)
            .chain(self.sample.iter_mut().map(|sample| &mut sample.counts))
        {
            counts.messages += 1;
            match (code & 0xff) as u32 {
                RIM_INPUT => counts.foreground += 1,
                RIM_INPUTSINK => counts.sink += 1,
                _ => {}
            }
            let packet = match &packet {
                Err(_) => {
                    counts.read_errors += 1;
                    continue;
                }
                Ok(None) => {
                    counts.invalid_packets += 1;
                    continue;
                }
                Ok(Some(packet)) => packet,
            };
            counts.zero_handles += u64::from(packet.header.hDevice.is_null());
            match packet.header.dwType {
                RIM_TYPEHID => {
                    counts.hid += 1;
                    let Some(reports) = packet.hid.map(HidBatch::count) else {
                        counts.invalid_packets += 1;
                        continue;
                    };
                    counts.reports += reports;
                    match source {
                        Source::Touchpad => counts.touchpad_reports += reports,
                        Source::Unknown => counts.unknown_reports += reports,
                        Source::Other => counts.other_reports += reports,
                    }
                }
                RIM_TYPEMOUSE => counts.mouse += 1,
                RIM_TYPEKEYBOARD => counts.keyboard += 1,
                _ => counts.other += 1,
            }
        }

        if let Some(sample) = &mut self.sample
            && let Some(counts) = &mut sample.contacts
        {
            let Ok(Some(Packet {
                header,
                hid: Some(batch),
            })) = packet
            else {
                return;
            };
            if !matches!(source, Source::Touchpad) {
                return;
            }
            if let Some(device) = self.contact_devices.get_mut(&header.hDevice) {
                device.observe_batch(sample.name, counts, batch, received);
            } else {
                counts.unsupported += batch.count();
            }
        }
    }

    fn interrupt_contacts(&mut self, reason: &'static str) {
        if let Some(sample) = &mut self.sample
            && let Some(counts) = &mut sample.contacts
        {
            for device in self.contact_devices.values_mut() {
                device.interrupt(sample.name, counts, reason);
            }
        }
    }

    fn finish(&mut self, foreground: bool, completion: &str) {
        self.interrupt_contacts("sample_ended");
        if let Some(sample) = self.sample.take() {
            if let Some(counts) = sample.contacts {
                log!(
                    "contact_sample={} down={} up={} incomplete={} single_frames={} empty_frames={} multi_frames={} ignored_reports={} rejected_reports={} unsupported_reports={}",
                    sample.name,
                    counts.down,
                    counts.up,
                    counts.incomplete,
                    counts.single,
                    counts.empty,
                    counts.multiple,
                    counts.ignored,
                    counts.rejected,
                    counts.unsupported
                );
            }
            sample.counts.log(&format!(
                "sample={} completion={completion} elapsed_ms={} foreground_at_start={} foreground_at_end={foreground} focus_losses={}",
                sample.name, sample.started.elapsed().as_millis(), sample.foreground_at_start, sample.focus_losses
            ));
        }
    }
}

fn with_probe<R>(operation: impl FnOnce(&mut Probe) -> R) -> R {
    PROBE.with_borrow_mut(|state| {
        operation(state.as_mut().expect("probe initialized before dispatch"))
    })
}

fn title(window: HWND, suffix: &str) {
    let text = wide(&format!("InputFlow Touchpad Probe - {suffix}"));
    // SAFETY: The owner window is valid and text is NUL-terminated for this call.
    unsafe { SetWindowTextW(window, text.as_ptr()) };
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        READY => {
            log!(
                "message_loop=ready foreground={}",
                unsafe { GetForegroundWindow() } == window
            );
            0
        }
        WM_INPUT => {
            with_probe(|probe| probe.on_input(lparam as HRAWINPUT, wparam));
            // SAFETY: Default processing performs the required foreground WM_INPUT cleanup.
            unsafe { DefWindowProcW(window, message, wparam, lparam) }
        }
        WM_ACTIVATE if wparam & 0xffff == 0 => {
            with_probe(|probe| {
                if let Some(sample) = &mut probe.sample {
                    sample.focus_losses += 1;
                }
                probe.interrupt_contacts("focus_lost");
            });
            unsafe { DefWindowProcW(window, message, wparam, lparam) }
        }
        WM_KEYDOWN if (lparam as usize) & (1 << 30) == 0 => {
            let foreground = unsafe { GetForegroundWindow() } == window;
            let sample = match wparam {
                0x31 => Some(("idle", false)),
                0x32 => Some(("stationary_touch_and_lift", false)),
                0x33 => Some(("slow_slide", false)),
                0x34 => Some(("external_mouse", false)),
                0x35 => Some(("short_touch", true)),
                0x36 => Some(("stationary_hold", true)),
                0x37 => Some(("left_to_right", true)),
                _ => None,
            };
            if let Some((name, track_contacts)) = sample {
                with_probe(|probe| {
                    probe.finish(foreground, "switched");
                    if track_contacts {
                        for device in probe.contact_devices.values_mut() {
                            device.tracker = Tracker::default();
                        }
                    }
                    probe.sample = Some(Sample {
                        name,
                        started: Instant::now(),
                        foreground_at_start: foreground,
                        focus_losses: 0,
                        counts: Counts::default(),
                        contacts: track_contacts.then(ContactCounts::default),
                    });
                });
                log!("sample_started={name} foreground={foreground}");
                title(window, &format!("Recording {name} (Enter to end)"));
            } else if wparam == 0x0d {
                with_probe(|probe| probe.finish(foreground, "enter"));
                title(window, "Ready");
            } else if wparam == 0x1b {
                with_probe(|probe| probe.finish(foreground, "escape"));
                unsafe { DestroyWindow(window) };
            } else {
                return unsafe { DefWindowProcW(window, message, wparam, lparam) };
            }
            0
        }
        WM_DESTROY => {
            let foreground = unsafe { GetForegroundWindow() } == window;
            with_probe(|probe| {
                probe.finish(foreground, "window_closed");
                probe.totals.log("session_total");
            });
            unsafe { PostQuitMessage(0) };
            0
        }
        // SAFETY: Forward native messages unchanged to the default window procedure.
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

fn run(sink: bool) -> io::Result<()> {
    if let Err(error) = open_log_file() {
        log!("log_file_open_failed {error}");
    }
    log!("inputflow-touchpad-probe {}", env!("CARGO_PKG_VERSION"));
    let devices = enumerate();
    let mut contact_devices = HashMap::new();
    for (index, (&handle, _)) in devices
        .iter()
        .filter(|(_, source)| matches!(source, Source::Touchpad))
        .enumerate()
    {
        let number = index + 1;
        let decoder = hid::Descriptor::read(handle).and_then(|descriptor| {
            descriptor.describe(number);
            hid::Decoder::new(descriptor, number)
        });
        match decoder {
            Ok(decoder) => {
                contact_devices.insert(
                    handle,
                    ContactDevice {
                        number,
                        decoder,
                        tracker: Tracker::default(),
                    },
                );
            }
            Err(error) => log!("contact_decoder=blocked device={number} {error}"),
        }
    }
    PROBE.set(Some(Probe {
        devices,
        contact_devices,
        buffer: Vec::new(),
        totals: Counts::default(),
        sample: None,
    }));

    // SAFETY: The module handle and resources belong to the current running process.
    let module = unsafe { GetModuleHandleW(std::ptr::null()) };
    if module.is_null() {
        return Err(api_error("GetModuleHandleW"));
    }
    let class_name = wide("InputFlowTouchpadProbe");
    let class = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: module,
        hCursor: unsafe { LoadCursorW(null_mut(), IDC_ARROW) },
        hbrBackground: ((windows_sys::Win32::Graphics::Gdi::COLOR_WINDOW + 1) as usize) as _,
        lpszClassName: class_name.as_ptr(),
        ..Default::default()
    };
    // SAFETY: The class has a valid callback and its name lives through registration.
    if unsafe { RegisterClassW(&class) } == 0 {
        return Err(api_error("RegisterClassW"));
    }
    let caption = wide("InputFlow Touchpad Probe - Ready");
    // SAFETY: The buffers contain NUL-terminated strings and the class is registered.
    let window = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            caption.as_ptr(),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            640,
            330,
            null_mut(),
            null_mut(),
            module,
            null_mut(),
        )
    };
    if window.is_null() {
        return Err(api_error("CreateWindowExW(probe)"));
    }
    let label_class = wide("STATIC");
    let label = wide(INSTRUCTIONS);
    let child = unsafe {
        CreateWindowExW(
            0,
            label_class.as_ptr(),
            label.as_ptr(),
            WS_CHILD | WS_VISIBLE,
            20,
            18,
            545,
            260,
            window,
            null_mut(),
            module,
            null_mut(),
        )
    };
    if child.is_null() {
        return Err(api_error("CreateWindowExW(instructions)"));
    }

    let registration = RAWINPUTDEVICE {
        usUsagePage: TOUCHPAD_PAGE,
        usUsage: TOUCHPAD_USAGE,
        dwFlags: if sink { RIDEV_INPUTSINK } else { 0 },
        hwndTarget: window,
    };
    // SAFETY: A single aligned registration references the valid probe window.
    if unsafe { RegisterRawInputDevices(&registration, 1, size_of::<RAWINPUTDEVICE>() as u32) } == 0
    {
        return Err(api_error("RegisterRawInputDevices(0x000d/0x0005)"));
    }
    log!(
        "registration=ok usage=0x000d/0x0005 mode={}",
        if sink { "input_sink" } else { "foreground" }
    );
    // SAFETY: Only the window owned by this process is shown and posted to.
    unsafe { ShowWindow(window, SW_SHOWNORMAL) };
    if unsafe { PostMessageW(window, READY, 0, 0) } == 0 {
        return Err(api_error("PostMessageW(ready)"));
    }

    let mut message = MSG::default();
    loop {
        // SAFETY: The writable message stays alive through retrieval and dispatch.
        match unsafe { GetMessageW(&mut message, null_mut(), 0, 0) } {
            -1 => return Err(api_error("GetMessageW")),
            0 => return Ok(()),
            _ => unsafe { DispatchMessageW(&message) },
        };
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut sink = false;
    let mut describe = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--input-sink" => sink = true,
            "--describe" => describe = true,
            "--help" | "-h" => {
                log!("Usage: inputflow-touchpad-probe [--input-sink | --describe]");
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    if describe {
        let devices = enumerate();
        for (index, (&handle, _)) in devices
            .iter()
            .filter(|(_, source)| matches!(source, Source::Touchpad))
            .enumerate()
        {
            hid::Descriptor::read(handle)?.describe(index + 1);
        }
        return Ok(());
    }
    run(sink)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::hid_batch;

    #[test]
    fn counts_each_report_in_a_batch() {
        let header = [3, 0, 0, 0, 2, 0, 0, 0];
        let batch = [header.as_slice(), &[0; 6]].concat();
        let parsed = hid_batch(&batch).unwrap();
        assert_eq!(parsed.count(), 2);
        assert_eq!(parsed.bytes.chunks_exact(parsed.report_size).len(), 2);
        assert!(hid_batch(&batch[..batch.len() - 1]).is_none());
    }

    #[test]
    fn rejects_header_truncation_and_impossible_lengths() {
        assert!(hid_batch(&[0; 7]).is_none());
        assert!(hid_batch(&[0, 0, 0, 0, 1, 0, 0, 0]).is_none());
        assert!(hid_batch(&[1, 0, 0, 0, 0, 0, 0, 0]).is_none());
        assert!(hid_batch(&[0xff; 12]).is_none());
    }
}
