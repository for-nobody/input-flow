//! Versioned InputFlow IPC contract and bounded Named Pipe server.
//!
//! The wire format is a four-byte little-endian JSON length followed by one
//! UTF-8 JSON document. Win32 handle, ACL, and overlapped-I/O ownership remains
//! in `inputflow-windows`; this crate owns only framing and protocol semantics.

mod codec;
mod contract;
mod server;

pub use codec::{CodecError, read_json_frame, write_json_frame};
pub use contract::*;
pub use server::{ProtocolHandler, ProtocolServer, ProtocolServerHandle};
