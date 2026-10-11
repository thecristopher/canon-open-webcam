use crate::codes::{Operation, ResponseCode};
use crate::error::{PtpError, Result};

/// Which way the optional data phase of a transaction goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataPhase<'a> {
    None,
    /// The camera answers with data, like GetEvent or GetViewFinderData.
    Read,
    /// We send data after the command, like SetPropertyValue.
    Write(&'a [u8]),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    pub code: u16,
    pub data: Vec<u8>,
    pub parameters: Vec<u32>,
}

impl Reply {
    pub fn is_ok(&self) -> bool {
        self.code == ResponseCode::Ok.code()
    }
}

/// Anything that can run whole PTP transactions.
///
/// Windows' MTP driver owns the session and the container framing itself, so the seam sits at
/// the operation level: run this operation, with these parameters and this data phase.
pub trait Transport {
    /// Opens a PTP session. Transports whose driver already holds one just return Ok.
    fn open_session(&mut self) -> Result<()>;

    fn close_session(&mut self) -> Result<()>;

    fn transact(&mut self, operation: Operation, parameters: &[u32], phase: DataPhase<'_>) -> Result<Reply>;

    /// The model name when the driver already knows it, so we can skip parsing DeviceInfo.
    fn model_name(&self) -> Option<String> {
        None
    }

    /// Same as transact, but anything other than OK becomes an error.
    fn require(&mut self, operation: Operation, parameters: &[u32], phase: DataPhase<'_>) -> Result<Reply> {
        let reply = self.transact(operation, parameters, phase)?;
        if !reply.is_ok() {
            return Err(PtpError::Response { code: reply.code, during: operation.code() });
        }
        Ok(reply)
    }
}

impl<T: Transport + ?Sized> Transport for Box<T> {
    fn open_session(&mut self) -> Result<()> {
        (**self).open_session()
    }

    fn close_session(&mut self) -> Result<()> {
        (**self).close_session()
    }

    fn transact(&mut self, operation: Operation, parameters: &[u32], phase: DataPhase<'_>) -> Result<Reply> {
        (**self).transact(operation, parameters, phase)
    }

    fn model_name(&self) -> Option<String> {
        (**self).model_name()
    }
}
