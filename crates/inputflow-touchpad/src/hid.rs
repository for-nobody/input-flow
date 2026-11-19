use std::io;
use std::mem::size_of;
use std::ptr::null_mut;

use windows_sys::Win32::Devices::HumanInterfaceDevice::*;
use windows_sys::Win32::Foundation::{HANDLE, NTSTATUS};
use windows_sys::Win32::UI::Input::{GetRawInputDeviceInfoW, RIDI_PREPARSEDDATA};

use crate::contact::{Contact, Frame, Point};
use crate::gesture::Scale;

const DIGITIZER: u16 = 0x0d;
const GENERIC_DESKTOP: u16 = 0x01;
const FINGER: u16 = 0x22;
const TIP: u16 = 0x42;
const CONTACT_ID: u16 = 0x51;
const CONTACT_COUNT: u16 = 0x54;
const BUTTONS: u16 = 0x09;

fn api_error(operation: &str) -> io::Error {
    io::Error::other(format!("{operation}: {}", io::Error::last_os_error()))
}

pub struct Descriptor {
    preparsed: Vec<usize>,
    caps: HIDP_CAPS,
    values: Vec<HIDP_VALUE_CAPS>,
    buttons: Vec<HIDP_BUTTON_CAPS>,
}

fn check_status(operation: &str, status: NTSTATUS) -> io::Result<()> {
    if status == HIDP_STATUS_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "{operation}: NTSTATUS 0x{:08x}",
            status as u32
        )))
    }
}

impl Descriptor {
    // Win32 validates the opaque device handle.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn read(device: HANDLE) -> io::Result<Self> {
        let mut bytes = 0;
        // SAFETY: A null buffer queries the size of this device's preparsed data.
        let result =
            unsafe { GetRawInputDeviceInfoW(device, RIDI_PREPARSEDDATA, null_mut(), &mut bytes) };
        if result == u32::MAX {
            return Err(api_error("GetRawInputDeviceInfoW(RIDI_PREPARSEDDATA size)"));
        }
        if bytes == 0 {
            return Err(io::Error::other(
                "RIDI_PREPARSEDDATA returned an empty descriptor",
            ));
        }
        let mut preparsed = vec![0usize; (bytes as usize).div_ceil(size_of::<usize>())];
        // SAFETY: The aligned buffer holds the requested byte count for the synchronous call.
        let result = unsafe {
            GetRawInputDeviceInfoW(
                device,
                RIDI_PREPARSEDDATA,
                preparsed.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        if result == u32::MAX {
            return Err(api_error("GetRawInputDeviceInfoW(RIDI_PREPARSEDDATA data)"));
        }
        if result == 0 {
            return Err(io::Error::other(
                "RIDI_PREPARSEDDATA copied no descriptor bytes",
            ));
        }

        let data = preparsed.as_ptr() as PHIDP_PREPARSED_DATA;
        let mut caps = HIDP_CAPS::default();
        // SAFETY: data points to the collection's Win32 preparsed data and caps is writable.
        check_status("HidP_GetCaps", unsafe { HidP_GetCaps(data, &mut caps) })?;
        let mut value_len = caps.NumberInputValueCaps;
        let mut button_len = caps.NumberInputButtonCaps;
        let mut values = vec![HIDP_VALUE_CAPS::default(); usize::from(value_len)];
        let mut buttons = vec![HIDP_BUTTON_CAPS::default(); usize::from(button_len)];
        // SAFETY: Both initialized arrays have the length reported by HidP_GetCaps.
        if value_len != 0 {
            check_status("HidP_GetValueCaps", unsafe {
                HidP_GetValueCaps(HidP_Input, values.as_mut_ptr(), &mut value_len, data)
            })?;
        }
        if button_len != 0 {
            check_status("HidP_GetButtonCaps", unsafe {
                HidP_GetButtonCaps(HidP_Input, buttons.as_mut_ptr(), &mut button_len, data)
            })?;
        }
        values.truncate(usize::from(value_len));
        buttons.truncate(usize::from(button_len));
        Ok(Self {
            preparsed,
            caps,
            values,
            buttons,
        })
    }

    pub fn describe(&self, device: usize, log: impl Fn(std::fmt::Arguments<'_>)) {
        log(format_args!(
            "descriptor device={device} input_bytes={} preparsed_capacity={} links={} value_caps={} button_caps={}",
            self.caps.InputReportByteLength,
            self.preparsed.len() * size_of::<usize>(),
            self.caps.NumberLinkCollectionNodes,
            self.values.len(),
            self.buttons.len()
        ));
        for value in &self.values {
            let (first, last) = value_usages(value);
            log(format_args!(
                "value device={device} report=0x{:02x} link={} link_usage=0x{:04x}/0x{:04x} usage=0x{:04x}/0x{first:04x}..0x{last:04x} bits={} count={} logical={}..{} physical={}..{} unit=0x{:x} unit_exp=0x{:x} absolute={} alias={}",
                value.ReportID,
                value.LinkCollection,
                value.LinkUsagePage,
                value.LinkUsage,
                value.UsagePage,
                value.BitSize,
                value.ReportCount,
                value.LogicalMin,
                value.LogicalMax,
                value.PhysicalMin,
                value.PhysicalMax,
                value.Units,
                value.UnitsExp,
                value.IsAbsolute,
                value.IsAlias
            ));
        }
        for button in &self.buttons {
            let (first, last) = button_usages(button);
            log(format_args!(
                "button device={device} report=0x{:02x} link={} link_usage=0x{:04x}/0x{:04x} usage=0x{:04x}/0x{first:04x}..0x{last:04x} count={} alias={}",
                button.ReportID,
                button.LinkCollection,
                button.LinkUsagePage,
                button.LinkUsage,
                button.UsagePage,
                button.ReportCount,
                button.IsAlias
            ));
        }
    }
}

fn value_usages(value: &HIDP_VALUE_CAPS) -> (u16, u16) {
    // SAFETY: IsRange selects the initialized union arm returned by HidP_GetValueCaps.
    unsafe {
        if value.IsRange {
            (
                value.Anonymous.Range.UsageMin,
                value.Anonymous.Range.UsageMax,
            )
        } else {
            let usage = value.Anonymous.NotRange.Usage;
            (usage, usage)
        }
    }
}

fn button_usages(button: &HIDP_BUTTON_CAPS) -> (u16, u16) {
    // SAFETY: IsRange selects the initialized union arm returned by HidP_GetButtonCaps.
    unsafe {
        if button.IsRange {
            (
                button.Anonymous.Range.UsageMin,
                button.Anonymous.Range.UsageMax,
            )
        } else {
            let usage = button.Anonymous.NotRange.Usage;
            (usage, usage)
        }
    }
}

#[derive(Clone, Copy)]
struct Field {
    name: &'static str,
    page: u16,
    usage: u16,
    link: u16,
    bits: u16,
    min: i32,
    max: i32,
    span_mm: Option<f64>,
}

impl Field {
    fn new(name: &'static str, usage: u16, cap: &HIDP_VALUE_CAPS) -> io::Result<Self> {
        if cap.ReportCount != 1
            || !(1..=32).contains(&cap.BitSize)
            || cap.LogicalMin > cap.LogicalMax
        {
            return Err(io::Error::other(format!(
                "{name} has an unsupported scalar layout"
            )));
        }
        Ok(Self {
            name,
            page: cap.UsagePage,
            usage,
            link: cap.LinkCollection,
            bits: cap.BitSize,
            min: cap.LogicalMin,
            max: cap.LogicalMax,
            span_mm: physical_millimeters(cap),
        })
    }

    fn read(self, data: PHIDP_PREPARSED_DATA, report: &[u8]) -> io::Result<i32> {
        let mut raw = 0;
        // SAFETY: data and report remain live, with the report ID and length checked by Decoder.
        check_status(self.name, unsafe {
            HidP_GetUsageValue(
                HidP_Input,
                self.page,
                self.link,
                self.usage,
                &mut raw,
                data,
                report.as_ptr(),
                report.len() as u32,
            )
        })?;
        let value = if self.min < 0 {
            let shift = 32 - self.bits;
            i64::from(((raw << shift) as i32) >> shift)
        } else {
            i64::from(raw)
        };
        if !(i64::from(self.min)..=i64::from(self.max)).contains(&value) {
            return Err(io::Error::other(format!(
                "{} outside logical range: {value}",
                self.name
            )));
        }
        Ok(value as i32)
    }
}

struct Finger {
    id: Field,
    x: Field,
    y: Field,
}

pub struct Decoder {
    descriptor: Descriptor,
    pub report_id: u8,
    count: Field,
    first: Finger,
    physical_buttons: bool,
    usages: Vec<u16>,
    report_buffer: Vec<u8>,
}

pub struct Decoded {
    pub frame: Frame,
    pub physical_button: Option<bool>,
}

impl Decoder {
    pub fn new(
        descriptor: Descriptor,
        device: usize,
        log: impl Fn(std::fmt::Arguments<'_>),
    ) -> io::Result<Self> {
        let mut count_caps = descriptor.values.iter().filter(|cap| {
            !cap.IsAlias && cap.LinkCollection == 0 && has_value(cap, DIGITIZER, CONTACT_COUNT)
        });
        let count_cap = count_caps
            .next()
            .ok_or_else(|| io::Error::other("missing root Contact Count"))?;
        if count_caps.next().is_some() {
            return Err(io::Error::other(
                "multiple Contact Count report layouts are unsupported",
            ));
        }
        let report_id = count_cap.ReportID;
        let count = Field::new("Contact Count", CONTACT_COUNT, count_cap)?;
        let mut fingers = Vec::new();
        for cap in descriptor.values.iter().filter(|cap| {
            !cap.IsAlias
                && cap.ReportID == report_id
                && cap.LinkCollection != 0
                && (cap.LinkUsagePage, cap.LinkUsage) == (DIGITIZER, FINGER)
                && has_value(cap, DIGITIZER, CONTACT_ID)
        }) {
            let link = cap.LinkCollection;
            let id = Field::new("Contact ID", CONTACT_ID, cap)?;
            let axis = |name, usage| {
                let mut caps = descriptor.values.iter().filter(|other| {
                    !other.IsAlias
                        && other.ReportID == report_id
                        && other.LinkCollection == link
                        && has_value(other, GENERIC_DESKTOP, usage)
                });
                match (caps.next(), caps.next()) {
                    (Some(axis), None) if axis.IsAbsolute => Field::new(name, usage, axis),
                    _ => Err(io::Error::other(format!(
                        "missing or ambiguous absolute {name} on link {link}"
                    ))),
                }
            };
            let x = axis("X", 0x30)?;
            let y = axis("Y", 0x31)?;
            let tips = descriptor
                .buttons
                .iter()
                .filter(|button| {
                    let (first, last) = button_usages(button);
                    !button.IsAlias
                        && button.ReportID == report_id
                        && button.LinkCollection == link
                        && button.UsagePage == DIGITIZER
                        && (first..=last).contains(&TIP)
                })
                .count();
            if tips != 1 || fingers.iter().any(|(existing, _)| *existing == link) {
                return Err(io::Error::other(format!(
                    "missing or ambiguous finger fields on link {link}"
                )));
            }
            // SAFETY: The union arm follows IsRange. Data indices preserve the descriptor order.
            let data_index = unsafe {
                if cap.IsRange {
                    cap.Anonymous.Range.DataIndexMin
                } else {
                    cap.Anonymous.NotRange.DataIndex
                }
            };
            fingers.push((link, (data_index, Finger { id, x, y })));
        }
        if count.min != 0 || count.max < 1 || count.max as usize > fingers.len() {
            return Err(io::Error::other(format!(
                "Contact Count {}..{} with {} complete finger slots is not supported as parallel input",
                count.min,
                count.max,
                fingers.len()
            )));
        }
        fingers.sort_unstable_by_key(|(_, (index, _))| *index);
        let slot_count = fingers.len();
        let (first_link, (_, first)) = fingers.remove(0);
        let data = descriptor.preparsed.as_ptr() as PHIDP_PREPARSED_DATA;
        // SAFETY: data is the live preparsed data for the input collection.
        let usage_capacity = unsafe { HidP_MaxUsageListLength(HidP_Input, DIGITIZER, data) };
        if usage_capacity == 0 {
            return Err(io::Error::other("no digitizer button usages available"));
        }
        let physical_buttons = descriptor.buttons.iter().any(|button| {
            let (first, last) = button_usages(button);
            !button.IsAlias
                && button.ReportID == report_id
                && button.LinkCollection == 0
                && button.UsagePage == BUTTONS
                && first <= 3
                && last >= 1
        });
        // SAFETY: Same preparsed data as above, queried for the Button usage page.
        let button_capacity = unsafe { HidP_MaxUsageListLength(HidP_Input, BUTTONS, data) };
        let physical_buttons = physical_buttons && button_capacity > 0;
        let report_buffer = vec![0; usize::from(descriptor.caps.InputReportByteLength)];
        log(format_args!(
            "contact_decoder=ready device={device} report=0x{report_id:02x} bytes={} parallel_slots={slot_count} single_link={first_link} button_usage_supported={physical_buttons}",
            report_buffer.len()
        ));
        Ok(Self {
            descriptor,
            report_id,
            count,
            first,
            physical_buttons,
            usages: vec![0; usage_capacity.max(button_capacity) as usize],
            report_buffer,
        })
    }

    pub fn has_physical_button(&self) -> bool {
        self.physical_buttons
    }

    pub fn scale(&self) -> Option<Scale> {
        let span = |field: Field| f64::from(field.max) - f64::from(field.min);
        let (x_span, y_span) = (span(self.first.x), span(self.first.y));
        (x_span > 0.0 && y_span > 0.0).then_some(Scale {
            x_span,
            y_span,
            millimeters: self.first.x.span_mm.zip(self.first.y.span_mm),
        })
    }

    pub fn decode(&mut self, report: &[u8]) -> io::Result<Option<Decoded>> {
        if report.first().copied() != Some(self.report_id) {
            return Ok(None);
        }
        if report.len() != self.report_buffer.len() {
            return Err(io::Error::other(format!(
                "report length {} differs from descriptor length {}",
                report.len(),
                self.report_buffer.len()
            )));
        }
        let data = self.descriptor.preparsed.as_ptr() as PHIDP_PREPARSED_DATA;
        self.report_buffer.copy_from_slice(report);
        let physical_button = if self.physical_buttons {
            Some(
                self.active_usages(data, BUTTONS, 0)?
                    .iter()
                    .any(|usage| (1..=3).contains(usage)),
            )
        } else {
            None
        };
        let frame = match self.count.read(data, report)? {
            0 => Frame::Empty,
            1 => {
                // A parallel frame with Contact Count=1 makes only the first slot valid.
                let id = self.first.id.read(data, report)?;
                let x = self.first.x.read(data, report)?;
                let y = self.first.y.read(data, report)?;
                let tip = self
                    .active_usages(data, DIGITIZER, self.first.id.link)?
                    .contains(&TIP);
                Frame::Single(Contact {
                    id,
                    tip,
                    point: Point { x, y },
                })
            }
            _ => Frame::Multiple,
        };
        Ok(Some(Decoded {
            frame,
            physical_button,
        }))
    }

    fn active_usages(
        &mut self,
        data: PHIDP_PREPARSED_DATA,
        page: u16,
        link: u16,
    ) -> io::Result<&[u16]> {
        let mut usage_len = self.usages.len() as u32;
        // SAFETY: The caller-owned usage list and mutable report copy match HidP's capacities.
        check_status("HidP_GetUsages", unsafe {
            HidP_GetUsages(
                HidP_Input,
                page,
                link,
                self.usages.as_mut_ptr(),
                &mut usage_len,
                data,
                self.report_buffer.as_mut_ptr(),
                self.report_buffer.len() as u32,
            )
        })?;
        self.usages
            .get(..usage_len as usize)
            .ok_or_else(|| io::Error::other("invalid HID usage list length"))
    }
}

fn has_value(cap: &HIDP_VALUE_CAPS, page: u16, usage: u16) -> bool {
    let (first, last) = value_usages(cap);
    cap.UsagePage == page && (first..=last).contains(&usage)
}

fn physical_millimeters(cap: &HIDP_VALUE_CAPS) -> Option<f64> {
    let base_mm = match cap.Units {
        0x11 => 10.0, // SI linear length in centimeters.
        0x13 => 25.4, // English linear length in inches.
        _ => return None,
    };
    let exponent = match cap.UnitsExp {
        0..=7 => cap.UnitsExp as i32,
        8..=15 => cap.UnitsExp as i32 - 16,
        _ => return None,
    };
    let physical_span = f64::from(cap.PhysicalMax) - f64::from(cap.PhysicalMin);
    (physical_span > 0.0).then(|| physical_span * base_mm * 10f64.powi(exponent))
}
