//! Windows Named Pipe transport with current-user ACL and cancellable
//! overlapped I/O.
//!
//! # Safety invariants
//!
//! - Every raw Win32 handle is converted to `OwnedHandle` immediately and is
//!   closed exactly once. Server instances are never reused after a connection.
//! - Every overlapped operation owns its event and keeps its OVERLAPPED and I/O
//!   buffer alive until completion or until cancellation has been reaped with
//!   `GetOverlappedResult`.
//! - Each pipe instance receives a protected DACL containing only LocalSystem
//!   and the user SID from the agent process token. Remote clients are rejected.
//! - Security descriptors and SID strings allocated by Win32 are always released
//!   with `LocalFree` after `CreateNamedPipeW` has consumed the descriptor.

use std::ffi::c_void;
use std::io::{self, Read, Write};
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Foundation::{
    ERROR_INSUFFICIENT_BUFFER, ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, GetLastError,
    INVALID_HANDLE_VALUE, LocalFree, WAIT_FAILED, WAIT_OBJECT_0,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
    PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentProcess, GetCurrentProcessId, OpenProcessToken, SetEvent,
    WaitForMultipleObjects,
};

const PIPE_BUFFER_BYTES: u32 = 64 * 1024;

#[derive(Clone)]
pub struct PipeShutdown {
    event: Arc<OwnedHandle>,
    signalled: Arc<AtomicBool>,
}

impl PipeShutdown {
    pub fn new() -> Result<Self, String> {
        // SAFETY: null security/name pointers request a private, unnamed event.
        let handle = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        let event = owned_handle(handle)
            .map_err(|error| format!("failed to create pipe shutdown event: {error}"))?;
        Ok(Self {
            event: Arc::new(event),
            signalled: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn signal(&self) -> Result<(), String> {
        self.signalled.store(true, Ordering::Release);
        // SAFETY: `event` is a live event handle owned by this object.
        if unsafe { SetEvent(self.raw()) } == 0 {
            Err(format!(
                "failed to signal pipe shutdown event: {}",
                io::Error::last_os_error()
            ))
        } else {
            Ok(())
        }
    }

    pub fn is_signalled(&self) -> bool {
        self.signalled.load(Ordering::Acquire)
    }

    fn raw(&self) -> *mut c_void {
        self.event.as_raw_handle().cast()
    }
}

pub struct NamedPipeListener {
    full_name: Vec<u16>,
    sddl: String,
    max_instances: u32,
    first_instance: bool,
    shutdown: PipeShutdown,
}

impl NamedPipeListener {
    pub fn bind(name: String, max_instances: u32, shutdown: PipeShutdown) -> Result<Self, String> {
        validate_pipe_name(&name)?;
        if max_instances == 0 || max_instances > 255 {
            return Err("Named Pipe max_instances must be in the range 1..=255".to_string());
        }
        let user_sid = current_user_sid_string()?;
        let sddl = format!("D:P(A;;GA;;;SY)(A;;GA;;;{user_sid})");
        // Validate the descriptor before the server thread reports readiness.
        let _ = SecurityDescriptor::from_sddl(&sddl)?;
        let full_name = std::ffi::OsStr::new(&format!(r"\\.\pipe\{name}"))
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        Ok(Self {
            full_name,
            sddl,
            max_instances,
            first_instance: true,
            shutdown,
        })
    }

    pub fn accept(&mut self) -> io::Result<NamedPipeStream> {
        self.accept_after_create(|| {})
    }

    /// Create the next server instance, invoke `created` only after the secure
    /// handle exists, then wait for a client. The first callback lets a host
    /// report readiness without racing the first `CreateNamedPipeW` call.
    pub fn accept_after_create(&mut self, created: impl FnOnce()) -> io::Result<NamedPipeStream> {
        if self.shutdown.is_signalled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "pipe listener is shutting down",
            ));
        }
        let descriptor = SecurityDescriptor::from_sddl(&self.sddl).map_err(io::Error::other)?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.as_ptr(),
            bInheritHandle: 0,
        };
        let mut open_mode = PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED;
        if self.first_instance {
            open_mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
        }
        // SAFETY: the name is NUL-terminated; `attributes` and its descriptor
        // live through this call. All flags and buffer sizes satisfy the API.
        let raw = unsafe {
            CreateNamedPipeW(
                self.full_name.as_ptr(),
                open_mode,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                self.max_instances,
                PIPE_BUFFER_BYTES,
                PIPE_BUFFER_BYTES,
                0,
                &attributes,
            )
        };
        let handle = owned_handle(raw)?;
        self.first_instance = false;
        created();

        let mut operation = OverlappedOperation::new()?;
        // SAFETY: handle is an overlapped server pipe; the OVERLAPPED remains
        // alive until `finish` completes or reaps cancellation.
        let connected = unsafe { ConnectNamedPipe(raw_handle(&handle), operation.overlapped()) };
        if connected == 0 {
            // SAFETY: captured immediately after the failing call.
            let error = unsafe { GetLastError() };
            if error == ERROR_PIPE_CONNECTED {
                return Ok(NamedPipeStream {
                    handle,
                    shutdown: self.shutdown.clone(),
                });
            }
            if error != ERROR_IO_PENDING {
                return Err(io::Error::from_raw_os_error(error as i32));
            }
            operation.finish(raw_handle(&handle), &self.shutdown)?;
        }
        Ok(NamedPipeStream {
            handle,
            shutdown: self.shutdown.clone(),
        })
    }
}

pub struct NamedPipeStream {
    handle: OwnedHandle,
    shutdown: PipeShutdown,
}

impl Read for NamedPipeStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        let length = buffer.len().min(u32::MAX as usize) as u32;
        let mut operation = OverlappedOperation::new()?;
        let mut immediate = 0;
        // SAFETY: buffer and OVERLAPPED remain alive until `complete_io` reaps
        // success or cancellation; handle was opened for overlapped duplex I/O.
        let started = unsafe {
            ReadFile(
                raw_handle(&self.handle),
                buffer.as_mut_ptr(),
                length,
                &mut immediate,
                operation.overlapped(),
            )
        };
        complete_io(
            started,
            immediate,
            &mut operation,
            &self.handle,
            &self.shutdown,
        )
        .map(|count| count as usize)
    }
}

impl Write for NamedPipeStream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        let length = buffer.len().min(u32::MAX as usize) as u32;
        let mut operation = OverlappedOperation::new()?;
        let mut immediate = 0;
        // SAFETY: buffer and OVERLAPPED remain alive until `complete_io` reaps
        // success or cancellation; handle was opened for overlapped duplex I/O.
        let started = unsafe {
            WriteFile(
                raw_handle(&self.handle),
                buffer.as_ptr(),
                length,
                &mut immediate,
                operation.overlapped(),
            )
        };
        complete_io(
            started,
            immediate,
            &mut operation,
            &self.handle,
            &self.shutdown,
        )
        .map(|count| count as usize)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct OverlappedOperation {
    event: OwnedHandle,
    state: OVERLAPPED,
}

impl OverlappedOperation {
    fn new() -> io::Result<Self> {
        // SAFETY: null security/name pointers request a private, unnamed event.
        let event =
            owned_handle(unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) })?;
        let state = OVERLAPPED {
            hEvent: raw_handle(&event),
            ..Default::default()
        };
        Ok(Self { event, state })
    }

    fn overlapped(&mut self) -> *mut OVERLAPPED {
        &mut self.state
    }

    fn finish(&mut self, file: *mut c_void, shutdown: &PipeShutdown) -> io::Result<u32> {
        let handles = [shutdown.raw(), raw_handle(&self.event)];
        // SAFETY: both handles remain live throughout the wait.
        let wait = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, u32::MAX) };
        if wait == WAIT_OBJECT_0 {
            // SAFETY: this exact OVERLAPPED belongs to the pending operation.
            unsafe { CancelIoEx(file, self.overlapped()) };
            let mut ignored = 0;
            // Reap cancellation before the OVERLAPPED or I/O buffer can drop.
            // SAFETY: handle and OVERLAPPED remain valid here.
            unsafe { GetOverlappedResult(file, self.overlapped(), &mut ignored, 1) };
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "pipe operation cancelled for server shutdown",
            ));
        }
        if wait == WAIT_FAILED {
            return Err(io::Error::last_os_error());
        }
        if wait != WAIT_OBJECT_0 + 1 {
            return Err(io::Error::other(format!(
                "unexpected pipe wait result {wait}"
            )));
        }
        let mut transferred = 0;
        // SAFETY: the I/O event signalled for this OVERLAPPED; no wait is needed.
        if unsafe { GetOverlappedResult(file, self.overlapped(), &mut transferred, 0) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(transferred)
        }
    }
}

fn complete_io(
    started: i32,
    immediate: u32,
    operation: &mut OverlappedOperation,
    handle: &OwnedHandle,
    shutdown: &PipeShutdown,
) -> io::Result<u32> {
    if started == 0 {
        // SAFETY: captured immediately after ReadFile/WriteFile.
        let error = unsafe { GetLastError() };
        if error != ERROR_IO_PENDING {
            return Err(io::Error::from_raw_os_error(error as i32));
        }
        return operation.finish(raw_handle(handle), shutdown);
    }
    // Synchronous completion populated the immediate byte count and has
    // finished using the supplied buffer.
    Ok(immediate)
}

struct SecurityDescriptor(PSECURITY_DESCRIPTOR);

impl SecurityDescriptor {
    fn from_sddl(sddl: &str) -> Result<Self, String> {
        let wide: Vec<u16> = std::ffi::OsStr::new(sddl)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut descriptor = std::ptr::null_mut();
        // SAFETY: SDDL is NUL-terminated and the output pointer is valid.
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        } == 0
        {
            Err(format!(
                "failed to create current-user pipe security descriptor: {}",
                io::Error::last_os_error()
            ))
        } else {
            Ok(Self(descriptor))
        }
    }

    fn as_ptr(&self) -> *mut c_void {
        self.0.cast()
    }
}

impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: descriptor was allocated by LocalAlloc inside the SDDL API.
            unsafe { LocalFree(self.0) };
        }
    }
}

fn current_user_sid_string() -> Result<String, String> {
    let mut token_raw = std::ptr::null_mut();
    // SAFETY: current-process pseudo handle is valid; output points to storage.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token_raw) } == 0 {
        return Err(format!(
            "failed to open process token for pipe ACL: {}",
            io::Error::last_os_error()
        ));
    }
    let token = owned_handle(token_raw)
        .map_err(|error| format!("invalid process token handle: {error}"))?;

    let mut byte_count = 0;
    // SAFETY: a null buffer with zero length queries required storage.
    let queried = unsafe {
        GetTokenInformation(
            raw_handle(&token),
            TokenUser,
            std::ptr::null_mut(),
            0,
            &mut byte_count,
        )
    };
    // SAFETY: captured immediately after GetTokenInformation.
    let query_error = unsafe { GetLastError() };
    if queried != 0 || query_error != ERROR_INSUFFICIENT_BUFFER || byte_count == 0 {
        return Err(format!(
            "failed to size process token user information (last_error={query_error})"
        ));
    }
    let words = (byte_count as usize).div_ceil(size_of::<usize>());
    let mut storage = vec![0_usize; words];
    // SAFETY: usize storage is sufficiently aligned and has `byte_count` bytes.
    if unsafe {
        GetTokenInformation(
            raw_handle(&token),
            TokenUser,
            storage.as_mut_ptr().cast(),
            byte_count,
            &mut byte_count,
        )
    } == 0
    {
        return Err(format!(
            "failed to read process token user information: {}",
            io::Error::last_os_error()
        ));
    }
    // SAFETY: GetTokenInformation populated TOKEN_USER at the aligned buffer.
    let token_user = unsafe { &*(storage.as_ptr().cast::<TOKEN_USER>()) };
    let mut sid_text = std::ptr::null_mut();
    // SAFETY: SID belongs to the still-live token buffer; output is valid.
    if unsafe { ConvertSidToStringSidW(token_user.User.Sid, &mut sid_text) } == 0 {
        return Err(format!(
            "failed to convert current user SID for pipe ACL: {}",
            io::Error::last_os_error()
        ));
    }
    let text = wide_pointer_to_string(sid_text);
    // SAFETY: ConvertSidToStringSidW allocated the returned string with LocalAlloc.
    unsafe { LocalFree(sid_text.cast()) };
    text
}

fn wide_pointer_to_string(pointer: *const u16) -> Result<String, String> {
    if pointer.is_null() {
        return Err("Windows returned a null SID string".to_string());
    }
    let mut length = 0;
    // SAFETY: pointer is a NUL-terminated string returned by Win32.
    unsafe {
        while *pointer.add(length) != 0 {
            length += 1;
        }
        String::from_utf16(std::slice::from_raw_parts(pointer, length))
            .map_err(|error| format!("current user SID is not valid UTF-16: {error}"))
    }
}

pub fn default_pipe_name() -> Result<String, String> {
    let mut session_id = 0;
    // SAFETY: output points to a valid u32; process id is current and live.
    if unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session_id) } == 0 {
        return Err(format!(
            "failed to resolve current Windows session for pipe name: {}",
            io::Error::last_os_error()
        ));
    }
    Ok(format!("InputFlow.Agent.v1.{session_id}"))
}

fn validate_pipe_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 128 {
        return Err("Named Pipe name must contain 1 to 128 ASCII bytes".to_string());
    }
    if !name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err("Named Pipe name contains an unsupported character".to_string());
    }
    Ok(())
}

fn owned_handle(handle: *mut c_void) -> io::Result<OwnedHandle> {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        Err(io::Error::last_os_error())
    } else {
        // SAFETY: caller just acquired sole ownership of a new Win32 handle.
        Ok(unsafe { OwnedHandle::from_raw_handle(handle as RawHandle) })
    }
}

fn raw_handle(handle: &OwnedHandle) -> *mut c_void {
    handle.as_raw_handle().cast()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_name_is_session_qualified_and_safe() {
        let name = default_pipe_name().unwrap();
        assert!(name.starts_with("InputFlow.Agent.v1."));
        validate_pipe_name(&name).unwrap();
        assert!(validate_pipe_name(r"bad\name").is_err());
    }

    #[test]
    fn current_user_acl_descriptor_is_constructible() {
        let sid = current_user_sid_string().unwrap();
        assert!(sid.starts_with("S-1-"));
        SecurityDescriptor::from_sddl(&format!("D:P(A;;GA;;;SY)(A;;GA;;;{sid})")).unwrap();
    }
}
