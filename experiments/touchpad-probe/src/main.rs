#![cfg(windows)]

use inputflow_touchpad::raw::{HidBatch, Packet, device_info, raw_devices, read_packet};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::mem::size_of;
use std::ptr::null_mut;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const TOUCHPAD_PAGE: u16 = 0x0d;
const TOUCHPAD_USAGE: u16 = 0x05;
const READY: u32 = WM_APP + 1;
const INSTRUCTIONS: &str = "Keep this window in the foreground.\r\n\r\n\
    Stage 1: 1-4 Count reports   5 Short touch   6 Hold   7 Slide\r\n\r\n\
    Touch Click: Q Left-side tap   W Right-side tap   E Hold\r\n\
    R Move/fold back   T Double tap   Y Press/drag   U Two fingers\r\n\
    I Mixed   +/- Threshold   Enter End   Esc Close\r\n\
    F6 Right only   F7 Both   F6/F7 stops output when active\r\n\r\n\
    Aim the pointer at the text area below. It receives Windows clicks.";

macro_rules! log {
    ($($arg:tt)*) => { crate::write_log(format_args!($($arg)*)) };
}

use inputflow_touchpad::{contact, gesture, hid};
mod output;
mod stage2;

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
    stage2: stage2::Stage2,
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
    occupied: bool,
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
        stage2: &mut stage2::Stage2,
    ) {
        for report in batch.bytes.chunks_exact(batch.report_size) {
            let decoded = match self.decoder.decode(report) {
                Ok(Some(decoded)) => decoded,
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
                    self.occupied = true;
                    stage2.interrupt("decode_error", received);
                    continue;
                }
            };
            self.occupied = match decoded.frame {
                Frame::Empty => false,
                Frame::Single(contact) => contact.tip,
                Frame::Multiple => true,
            };
            match decoded.frame {
                Frame::Single(_) => counts.single += 1,
                Frame::Empty => counts.empty += 1,
                Frame::Multiple => counts.multiple += 1,
            }
            let transition = self.tracker.observe(decoded.frame, received);
            stage2.observe(&decoded, &transition, self.decoder.scale(), received);
            self.log_transition(sample, counts, transition);
        }
    }

    fn observe_occupancy(&mut self, batch: HidBatch<'_>) {
        for report in batch.bytes.chunks_exact(batch.report_size) {
            match self.decoder.decode(report) {
                Ok(Some(decoded)) => {
                    self.occupied = match decoded.frame {
                        Frame::Empty => false,
                        Frame::Single(contact) => contact.tip,
                        Frame::Multiple => true,
                    };
                }
                Ok(None) => {}
                Err(_) => self.occupied = true,
            }
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

fn classify_device(handle: HANDLE) -> io::Result<Source> {
    let info = device_info(handle)?;
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

impl Probe {
    fn on_input(&mut self, input: HRAWINPUT, code: WPARAM) {
        let received = Instant::now();
        self.stage2.guard_target(received);
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

        let Ok(Some(Packet {
            header,
            hid: Some(batch),
        })) = packet
        else {
            if self.stage2.active()
                && !matches!(packet, Ok(Some(Packet { header, .. })) if header.dwType != RIM_TYPEHID)
            {
                self.interrupt_contacts("raw_input_unreadable");
            }
            return;
        };
        if !matches!(source, Source::Touchpad) {
            return;
        }
        let device = self.contact_devices.get_mut(&header.hDevice);
        if let Some(sample) = &mut self.sample
            && let Some(counts) = &mut sample.contacts
        {
            if let Some(device) = device {
                device.observe_batch(sample.name, counts, batch, received, &mut self.stage2);
            } else {
                counts.unsupported += batch.count();
            }
        } else if let Some(device) = device {
            device.observe_occupancy(batch);
        }
    }

    fn interrupt_contacts(&mut self, reason: &'static str) {
        self.stage2.interrupt(reason, Instant::now());
        if let Some(sample) = &mut self.sample
            && let Some(counts) = &mut sample.contacts
        {
            for device in self.contact_devices.values_mut() {
                device.interrupt(sample.name, counts, reason);
            }
        }
    }

    fn finish(&mut self, foreground: bool, completion: &'static str) {
        self.stage2.end(completion, Instant::now());
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

fn handle_key(window: HWND, key: WPARAM, flags: LPARAM) -> bool {
    if (flags as usize) & (1 << 30) != 0 {
        return false;
    }
    let sample = match key {
        0x31 => Some(("idle", false, false)),
        0x32 => Some(("stationary_touch_and_lift", false, false)),
        0x33 => Some(("slow_slide", false, false)),
        0x34 => Some(("external_mouse", false, false)),
        0x35 => Some(("short_touch", true, false)),
        0x36 => Some(("stationary_hold", true, false)),
        0x37 => Some(("left_to_right", true, false)),
        0x51 => Some(("left_tap", true, true)),
        0x57 => Some(("right_tap", true, true)),
        0x45 => Some(("hold", true, true)),
        0x52 => Some(("move", true, true)),
        0x54 => Some(("double_tap", true, true)),
        0x59 => Some(("press_drag", true, true)),
        0x55 => Some(("two_fingers", true, true)),
        0x49 => Some(("mixed", true, true)),
        _ => None,
    };
    // SAFETY: The experiment uses the foreground state of its own top-level window.
    let foreground = unsafe { GetForegroundWindow() } == window;
    if let Some((name, track_contacts, observe_gesture)) = sample {
        let started = with_probe(|probe| {
            if observe_gesture && (!foreground || probe.contact_devices.len() != 1) {
                log!(
                    "stage2_start_blocked foreground={foreground} usable_devices={}",
                    probe.contact_devices.len()
                );
                return false;
            }
            probe.finish(foreground, "switched");
            if track_contacts {
                for device in probe.contact_devices.values_mut() {
                    device.tracker = Tracker::default();
                    if observe_gesture && device.occupied {
                        device.tracker.interrupt("sample_started_mid_contact");
                    }
                }
            }
            let now = Instant::now();
            if observe_gesture {
                probe.stage2.start(name, now);
            }
            probe.sample = Some(Sample {
                name,
                started: now,
                foreground_at_start: foreground,
                focus_losses: 0,
                counts: Counts::default(),
                contacts: track_contacts.then(ContactCounts::default),
            });
            true
        });
        if started {
            log!("sample_started={name} foreground={foreground}");
            title(window, &format!("Recording {name} (Enter to end)"));
        }
    } else {
        match key {
            0x0d => {
                with_probe(|probe| probe.finish(foreground, "enter"));
                title(window, "Ready");
            }
            0x1b => {
                with_probe(|probe| {
                    probe.stage2.disarm("escape", Instant::now());
                    probe.finish(foreground, "escape");
                });
                // SAFETY: The probe owns this window and is exiting via its normal close path.
                unsafe { DestroyWindow(window) };
            }
            0xbb | 0x6b => with_probe(|probe| probe.stage2.adjust_movement(0.005)),
            0xbd | 0x6d => with_probe(|probe| probe.stage2.adjust_movement(-0.005)),
            0x75 | 0x76 => with_probe(|probe| {
                let clear = probe.contact_devices.len() == 1
                    && probe
                        .contact_devices
                        .values()
                        .all(|device| !device.occupied);
                let mode = if key == 0x75 {
                    output::Mode::RightOnly
                } else {
                    output::Mode::Both
                };
                probe.stage2.toggle_output(mode, clear, Instant::now());
            }),
            _ => return false,
        }
    }
    true
}

fn flush_output() {
    while let Some(plan) = with_probe(|probe| probe.stage2.next_output(Instant::now())) {
        // SendInput can re-enter the target procedure. Release the Probe borrow first.
        let result = output::send(plan);
        with_probe(|probe| probe.stage2.output_result(plan, result));
    }
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
            let result = unsafe { DefWindowProcW(window, message, wparam, lparam) };
            flush_output();
            result
        }
        WM_INPUT_DEVICE_CHANGE if wparam == GIDC_REMOVAL as WPARAM => {
            with_probe(|probe| {
                if probe.contact_devices.contains_key(&(lparam as HANDLE)) {
                    probe.stage2.disarm("device_removed", Instant::now());
                    probe.interrupt_contacts("device_removed");
                    probe.contact_devices.remove(&(lparam as HANDLE));
                }
                probe.devices.remove(&(lparam as HANDLE));
            });
            0
        }
        WM_TIMER => {
            with_probe(|probe| probe.stage2.timer(wparam, Instant::now()));
            flush_output();
            0
        }
        stage2::SHOW_STATUS => {
            let (status, text) = with_probe(|probe| probe.stage2.status_text());
            // SAFETY: The text is terminated and the status control belongs to this window.
            unsafe { SetWindowTextW(status, text.as_ptr()) };
            0
        }
        WM_ACTIVATE if wparam & 0xffff == 0 => {
            with_probe(|probe| {
                if let Some(sample) = &mut probe.sample {
                    sample.focus_losses += 1;
                }
                probe.stage2.disarm("focus_lost", Instant::now());
                probe.interrupt_contacts("focus_lost");
            });
            unsafe { DefWindowProcW(window, message, wparam, lparam) }
        }
        WM_KEYDOWN if handle_key(window, wparam, lparam) => 0,
        WM_DESTROY => {
            let foreground = unsafe { GetForegroundWindow() } == window;
            with_probe(|probe| {
                probe.stage2.disarm("window_closed", Instant::now());
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

fn run(sink: bool, parameters: gesture::Parameters) -> io::Result<()> {
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
            descriptor.describe(number, write_log);
            hid::Decoder::new(descriptor, number, write_log)
        });
        match decoder {
            Ok(decoder) => {
                contact_devices.insert(
                    handle,
                    ContactDevice {
                        number,
                        decoder,
                        tracker: Tracker::default(),
                        occupied: false,
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
        stage2: stage2::Stage2::new(parameters),
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
            WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            740,
            500,
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
            680,
            165,
            window,
            null_mut(),
            module,
            null_mut(),
        )
    };
    if child.is_null() {
        return Err(api_error("CreateWindowExW(instructions)"));
    }
    let empty = wide("");
    let target_class = wide("EDIT");
    let target_content = wide(
        "CLICK AND DRAG TEST AREA\r\n\r\nSelect this sentence by pressing and dragging across the words.\r\n\r\nThe target records button down, up, double-click, and scrolling.\r\n\r\nScroll with two fingers to move this text.\r\n\r\nKeep the pointer within this box for taps and holds.\r\n\r\nUse Enter to finish each sample, or Esc to close.\r\n\r\nThis area also receives the original Windows touchpad clicks.",
    );
    // SAFETY: The labels and edit are children of the live experiment window.
    let (status, target) = unsafe {
        let status = CreateWindowExW(
            0,
            label_class.as_ptr(),
            empty.as_ptr(),
            WS_CHILD | WS_VISIBLE,
            20,
            184,
            680,
            65,
            window,
            null_mut(),
            module,
            null_mut(),
        );
        let target = CreateWindowExW(
            WS_EX_CLIENTEDGE,
            target_class.as_ptr(),
            target_content.as_ptr(),
            WS_CHILD
                | WS_VISIBLE
                | WS_VSCROLL
                | (ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL) as u32,
            20,
            258,
            680,
            195,
            window,
            null_mut(),
            module,
            null_mut(),
        );
        (status, target)
    };
    if status.is_null() || target.is_null() {
        return Err(api_error("CreateWindowExW(Stage 2 controls)"));
    }
    stage2::install_target(target)?;
    with_probe(|probe| probe.stage2.attach(window, target, status));

    let registration = RAWINPUTDEVICE {
        usUsagePage: TOUCHPAD_PAGE,
        usUsage: TOUCHPAD_USAGE,
        dwFlags: RIDEV_DEVNOTIFY | if sink { RIDEV_INPUTSINK } else { 0 },
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
    // SAFETY: Initial display update is confined to our window.
    unsafe { PostMessageW(window, stage2::SHOW_STATUS, 0, 0) };

    let mut message = MSG::default();
    loop {
        // SAFETY: The writable message stays alive through retrieval and dispatch.
        match unsafe { GetMessageW(&mut message, null_mut(), 0, 0) } {
            -1 => return Err(api_error("GetMessageW")),
            0 => return Ok(()),
            _ if message.hwnd == target
                && message.message == WM_KEYDOWN
                && handle_key(window, message.wParam, message.lParam) =>
            {
                0
            }
            _ => unsafe { DispatchMessageW(&message) },
        };
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut sink = false;
    let mut describe = false;
    let mut parameters = gesture::Parameters::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input-sink" => sink = true,
            "--describe" => describe = true,
            "--short-ms" => {
                parameters.short =
                    Duration::from_millis(args.next().ok_or("missing --short-ms value")?.parse()?)
            }
            "--hold-ms" => {
                parameters.hold =
                    Duration::from_millis(args.next().ok_or("missing --hold-ms value")?.parse()?)
            }
            "--movement" => {
                parameters.movement = args.next().ok_or("missing --movement value")?.parse()?
            }
            "--movement-mm" => {
                parameters.movement_mm =
                    args.next().ok_or("missing --movement-mm value")?.parse()?;
            }
            "--help" | "-h" => {
                log!(
                    "Usage: inputflow-touchpad-probe [--input-sink | --describe] [--short-ms N] [--hold-ms N] [--movement-mm N] [--movement FRACTION]"
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    if parameters.short.is_zero()
        || parameters.short >= parameters.hold
        || parameters.hold > Duration::from_secs(60)
        || !(0.005..=0.2).contains(&parameters.movement)
        || !(0.5..=25.0).contains(&parameters.movement_mm)
    {
        return Err("require 0 < short-ms < hold-ms <= 60000, movement-mm 0.5..=25 and movement 0.005..=0.2".into());
    }
    if describe {
        let devices = enumerate();
        for (index, (&handle, _)) in devices
            .iter()
            .filter(|(_, source)| matches!(source, Source::Touchpad))
            .enumerate()
        {
            hid::Descriptor::read(handle)?.describe(index + 1, write_log);
        }
        return Ok(());
    }
    run(sink, parameters)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use inputflow_touchpad::raw::hid_batch;

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
