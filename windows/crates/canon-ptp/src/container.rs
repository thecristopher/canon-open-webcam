use crate::bytes::{packed, u16_at, u32_at};
use crate::codes::{ContainerType, Operation, ResponseCode};
use crate::error::{PtpError, Result};
use crate::transport::{DataPhase, Reply, Transport};

/// One PTP packet: a 12 byte header (length, type, code, transaction id) and a payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Container {
    pub kind: ContainerType,
    pub code: u16,
    pub transaction_id: u32,
    pub payload: Vec<u8>,
}

impl Container {
    pub const HEADER_SIZE: usize = 12;

    pub fn command(operation: Operation, transaction_id: u32, parameters: &[u32]) -> Self {
        Self { kind: ContainerType::Command, code: operation.code(), transaction_id, payload: packed(parameters) }
    }

    pub fn data(operation: Operation, transaction_id: u32, payload: &[u8]) -> Self {
        Self { kind: ContainerType::Data, code: operation.code(), transaction_id, payload: payload.to_vec() }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(Self::HEADER_SIZE + self.payload.len());
        bytes.extend(((Self::HEADER_SIZE + self.payload.len()) as u32).to_le_bytes());
        bytes.extend((self.kind as u16).to_le_bytes());
        bytes.extend(self.code.to_le_bytes());
        bytes.extend(self.transaction_id.to_le_bytes());
        bytes.extend(&self.payload);
        bytes
    }

    /// Total size the header promises, so a reader knows when a multi packet transfer is complete.
    pub fn declared_length(bytes: &[u8]) -> Option<usize> {
        u32_at(bytes, 0).map(|n| n as usize)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let (Some(length), Some(raw_kind), Some(code), Some(transaction_id)) =
            (u32_at(bytes, 0), u16_at(bytes, 4), u16_at(bytes, 6), u32_at(bytes, 8))
        else {
            return Err(PtpError::Malformed("container shorter than its header".into()));
        };
        let kind = ContainerType::from_raw(raw_kind)
            .ok_or_else(|| PtpError::Malformed(format!("unknown container type {raw_kind}")))?;
        let length = length as usize;
        if length > bytes.len() || length < Self::HEADER_SIZE {
            return Err(PtpError::Malformed(format!("container says {length} bytes but has {}", bytes.len())));
        }
        Ok(Self { kind, code, transaction_id, payload: bytes[Self::HEADER_SIZE..length].to_vec() })
    }

    pub fn parameters(&self) -> Vec<u32> {
        let (words, _) = self.payload.as_chunks::<4>();
        words.iter().map(|w| u32::from_le_bytes(*w)).collect()
    }
}

/// Anything that can move whole PTP containers back and forth. Raw USB in real life, a script in tests.
pub trait ContainerIo {
    fn send(&mut self, bytes: &[u8]) -> Result<()>;
    /// Returns exactly one container, already stitched together from however many USB packets it took.
    fn receive_container(&mut self) -> Result<Vec<u8>>;
}

/// Plain PTP transactions over raw containers: command out, optional data out or in, response back.
pub struct ContainerSession<Io> {
    io: Io,
    next_transaction_id: u32,
}

impl<Io: ContainerIo> ContainerSession<Io> {
    pub fn new(io: Io) -> Self {
        Self { io, next_transaction_id: 0 }
    }

    pub fn io(&self) -> &Io {
        &self.io
    }

    fn take_transaction_id(&mut self) -> u32 {
        self.next_transaction_id = self.next_transaction_id.wrapping_add(1);
        if self.next_transaction_id == 0 || self.next_transaction_id == u32::MAX {
            self.next_transaction_id = 1;
        }
        self.next_transaction_id
    }
}

impl<Io: ContainerIo> Transport for ContainerSession<Io> {
    fn open_session(&mut self) -> Result<()> {
        let opened = self.transact(Operation::OpenSession, &[1], DataPhase::None)?;
        if opened.code == ResponseCode::SessionAlreadyOpen.code() {
            // Someone (probably a crashed us) left a session open, so start clean
            self.transact(Operation::CloseSession, &[], DataPhase::None)?;
            self.require(Operation::OpenSession, &[1], DataPhase::None)?;
        } else if !opened.is_ok() {
            return Err(PtpError::Response { code: opened.code, during: Operation::OpenSession.code() });
        }
        Ok(())
    }

    fn close_session(&mut self) -> Result<()> {
        self.transact(Operation::CloseSession, &[], DataPhase::None).map(|_| ())
    }

    fn transact(&mut self, operation: Operation, parameters: &[u32], phase: DataPhase<'_>) -> Result<Reply> {
        // OpenSession must be transaction 0, everything after counts up from there
        let transaction_id = if operation == Operation::OpenSession { 0 } else { self.take_transaction_id() };
        self.io.send(&Container::command(operation, transaction_id, parameters).encode())?;
        if let DataPhase::Write(outgoing) = phase {
            self.io.send(&Container::data(operation, transaction_id, outgoing).encode())?;
        }

        let mut incoming = Vec::new();
        loop {
            let container = Container::decode(&self.io.receive_container()?)?;
            match container.kind {
                ContainerType::Data => incoming.extend(container.payload),
                ContainerType::Response => {
                    if container.transaction_id != transaction_id {
                        return Err(PtpError::UnexpectedTransaction {
                            expected: transaction_id,
                            got: container.transaction_id,
                        });
                    }
                    return Ok(Reply { code: container.code, data: incoming, parameters: container.parameters() });
                }
                // Stray events on the bulk pipe are noise, the interesting ones come through GetEvent
                ContainerType::Command | ContainerType::Event => continue,
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// Pretends to be a camera on the other end of the cable. Each command pops the next scripted answer.
    #[derive(Default)]
    pub struct FakeCable {
        pub answers: VecDeque<(ResponseCode, Option<Vec<u8>>)>,
        pub sent: Vec<Container>,
        inbox: VecDeque<Vec<u8>>,
    }

    impl FakeCable {
        pub fn answer(&mut self, code: ResponseCode, data: Option<Vec<u8>>) {
            self.answers.push_back((code, data));
        }

        pub fn commands(&self) -> Vec<&Container> {
            self.sent.iter().filter(|c| c.kind == ContainerType::Command).collect()
        }
    }

    impl ContainerIo for FakeCable {
        fn send(&mut self, bytes: &[u8]) -> Result<()> {
            let container = Container::decode(bytes)?;
            let is_command = container.kind == ContainerType::Command;
            let (code, tid) = (container.code, container.transaction_id);
            self.sent.push(container);
            if !is_command {
                return Ok(());
            }
            let (answer, data) = self.answers.pop_front().unwrap_or((ResponseCode::Ok, None));
            if let Some(data) = data {
                self.inbox.push_back(
                    Container { kind: ContainerType::Data, code, transaction_id: tid, payload: data }.encode(),
                );
            }
            self.inbox.push_back(
                Container { kind: ContainerType::Response, code: answer.code(), transaction_id: tid, payload: vec![] }
                    .encode(),
            );
            Ok(())
        }

        fn receive_container(&mut self) -> Result<Vec<u8>> {
            self.inbox.pop_front().ok_or_else(|| PtpError::Transport("fake camera has nothing to say".into()))
        }
    }

    #[test]
    fn encode_and_decode_round_trip() {
        let original = Container::command(Operation::CanonGetViewFinderData, 7, &[0x0020_0000, 0, 0]);
        let bytes = original.encode();
        assert_eq!(Container::declared_length(&bytes), Some(24));
        assert_eq!(Container::decode(&bytes).unwrap(), original);
        assert_eq!(original.parameters(), vec![0x0020_0000, 0, 0]);
    }

    #[test]
    fn a_truncated_container_is_rejected() {
        let mut bytes = Container::command(Operation::OpenSession, 0, &[1]).encode();
        bytes.pop();
        assert!(matches!(Container::decode(&bytes), Err(PtpError::Malformed(_))));
    }

    #[test]
    fn open_session_always_uses_transaction_zero() {
        let mut session = ContainerSession::new(FakeCable::default());
        session.open_session().unwrap();
        assert_eq!(session.io().commands()[0].transaction_id, 0);
    }

    #[test]
    fn transactions_count_up_after_opening() {
        let mut session = ContainerSession::new(FakeCable::default());
        session.open_session().unwrap();
        session.transact(Operation::CanonGetEvent, &[], DataPhase::Read).unwrap();
        session.transact(Operation::CanonGetEvent, &[], DataPhase::Read).unwrap();
        let ids: Vec<u32> = session.io().commands().iter().map(|c| c.transaction_id).collect();
        assert_eq!(ids, vec![0, 1, 2]);
    }

    #[test]
    fn a_stale_session_is_closed_and_reopened() {
        let mut cable = FakeCable::default();
        cable.answer(ResponseCode::SessionAlreadyOpen, None);
        let mut session = ContainerSession::new(cable);
        session.open_session().unwrap();
        let codes: Vec<u16> = session.io().commands().iter().map(|c| c.code).collect();
        assert_eq!(codes, vec![0x1002, 0x1003, 0x1002]);
    }

    #[test]
    fn data_sent_by_the_camera_lands_in_the_reply() {
        let mut cable = FakeCable::default();
        cable.answer(ResponseCode::Ok, Some(vec![1, 2, 3]));
        let reply = ContainerSession::new(cable).transact(Operation::CanonGetEvent, &[], DataPhase::Read).unwrap();
        assert_eq!(reply.data, vec![1, 2, 3]);
    }

    #[test]
    fn outgoing_data_follows_the_command_with_the_same_transaction() {
        let mut session = ContainerSession::new(FakeCable::default());
        session.transact(Operation::CanonSetPropertyValue, &[], DataPhase::Write(&[9])).unwrap();
        let sent = &session.io().sent;
        assert_eq!(sent.iter().map(|c| c.kind).collect::<Vec<_>>(), vec![ContainerType::Command, ContainerType::Data]);
        assert_eq!(sent[0].transaction_id, sent[1].transaction_id);
    }

    #[test]
    fn require_turns_a_bad_answer_into_an_error() {
        let mut cable = FakeCable::default();
        cable.answer(ResponseCode::GeneralError, None);
        let err = ContainerSession::new(cable).require(Operation::CanonGetEvent, &[], DataPhase::Read).unwrap_err();
        assert_eq!(err, PtpError::Response { code: 0x2002, during: 0x9116 });
    }
}
