#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PtpError {
    #[error("camera sent something odd: {0}")]
    Malformed(String),
    #[error("camera answered 0x{code:04X} to operation 0x{during:04X}")]
    Response { code: u16, during: u16 },
    #[error("USB trouble: {0}")]
    Transport(String),
    #[error("camera answered transaction {got} while we waited for {expected}")]
    UnexpectedTransaction { expected: u32, got: u32 },
}

pub type Result<T> = std::result::Result<T, PtpError>;
