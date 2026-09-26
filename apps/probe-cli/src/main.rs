// Minimal Win32 probe: proves that the `windows-sys` crate links against the
// Windows SDK import libraries (kernel32) through the MSVC toolchain.
use windows_sys::Win32::System::SystemInformation::GetTickCount;

fn main() {
    // SAFETY: GetTickCount takes no arguments and returns the number of
    // milliseconds elapsed since the system was started. It is safe to call.
    let ticks = unsafe { GetTickCount() };
    println!("probe-cli: GetTickCount() = {ticks} ms");
}
