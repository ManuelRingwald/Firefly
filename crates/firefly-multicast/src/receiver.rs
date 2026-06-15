//! The receiving half of the CAT062 multicast adapter — the consumer-side
//! counterpart to [`crate::run`] (the sender).
//!
//! A real consumer (the Phoenix ASD, an EFS, a recorder) joins the multicast
//! group with [`receiver_socket`] and decodes each datagram with
//! [`recv_records`]/[`run`]. This closes the loop opened by ADR 0006: the
//! sender (Häppchen C.3) and the decoder (Häppchen D.1/D.2) together prove
//! that a real listener can recover the tracker's output from the wire.

use std::net::Ipv4Addr;

use firefly_asterix::{decode_data_block, DecodeError, DecodedRecord};
use tokio::net::UdpSocket;

/// The largest CAT062 datagram we expect: comfortably above one record's ~36
/// bytes times a generous number of tracks, with headroom.
const RECV_BUFFER_BYTES: usize = 2048;

/// Errors that can occur while receiving and decoding a CAT062 datagram.
#[derive(Debug)]
pub enum ReceiveError {
    /// The socket operation itself failed.
    Io(std::io::Error),
    /// A datagram arrived but was not a valid CAT062 data block.
    Decode(DecodeError),
}

impl std::fmt::Display for ReceiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReceiveError::Io(error) => write!(f, "socket error: {error}"),
            ReceiveError::Decode(error) => write!(f, "decode error: {error}"),
        }
    }
}

impl std::error::Error for ReceiveError {}

impl From<std::io::Error> for ReceiveError {
    fn from(error: std::io::Error) -> Self {
        ReceiveError::Io(error)
    }
}

impl From<DecodeError> for ReceiveError {
    fn from(error: DecodeError) -> Self {
        ReceiveError::Decode(error)
    }
}

/// Bind a UDP socket suitable for *receiving* multicast datagrams on
/// `group`:`port`, and join `group` on all interfaces.
///
/// Binding to the multicast `port` on every interface (`UNSPECIFIED`) and
/// then joining `group` is the standard recipe for "listen to this multicast
/// feed" (ADR 0006, ED-109A-style distribution): the sender never learns this
/// socket exists.
pub async fn receiver_socket(group: Ipv4Addr, port: u16) -> std::io::Result<UdpSocket> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port)).await?;
    socket.join_multicast_v4(group, Ipv4Addr::UNSPECIFIED)?;
    Ok(socket)
}

/// Receive one datagram from `socket` and decode it as a CAT062 data block.
///
/// Each datagram is a self-contained `[CAT][LEN][record…]` block (one per
/// scan, see [`crate::run`]), so one `recv_from` plus one
/// [`decode_data_block`] call is enough — there is no message framing to do.
pub async fn recv_records(socket: &UdpSocket) -> Result<Vec<DecodedRecord>, ReceiveError> {
    let mut buf = [0u8; RECV_BUFFER_BYTES];
    let (n, _) = socket.recv_from(&mut buf).await?;
    Ok(decode_data_block(&buf[..n])?)
}

/// Receive and decode CAT062 data blocks from `socket` until `on_records`
/// returns `false`.
///
/// `on_records` is called with the decoded records of each datagram, in
/// arrival order; it returns whether to keep listening. Returns the number of
/// datagrams received. A socket or decode error stops the run and is returned
/// — the caller decides how to react (e.g. log and keep listening on the next
/// datagram, or give up).
pub async fn run(
    socket: &UdpSocket,
    mut on_records: impl FnMut(Vec<DecodedRecord>) -> bool,
) -> Result<usize, ReceiveError> {
    let mut received = 0usize;
    loop {
        let records = match recv_records(socket).await {
            Ok(records) => records,
            Err(error) => {
                tracing::warn!(%error, "failed to receive/decode CAT062 data block");
                return Err(error);
            }
        };
        received += 1;
        tracing::debug!(records = records.len(), "received CAT062 data block");
        if !on_records(records) {
            return Ok(received);
        }
    }
}
