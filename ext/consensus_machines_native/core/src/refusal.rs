use core::fmt;

/// Why a call was not honoured. Nothing changed: no term was adopted, no vote
/// recorded, no role left, no entry written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The index names no seat of the membership.
    UnknownMember,
    /// The index names this server's own seat; a server exchanges no message
    /// with itself.
    OwnSeat,
    /// The message contradicts itself: a zero term, a position whose index and
    /// term are not both zero or both set, a position or entry from a term
    /// above the message's own, or entry terms that go backwards.
    Malformed,
    /// The entries end past the capacity of the log.
    TooManyEntries,
    /// The response is from a term this server has left, or answers a request
    /// it sent in one.
    Stale,
    /// A ballot for the current term, which this server is not campaigning in.
    NotCandidate,
    /// The call is for the leader of the current term, which this server is not.
    NotLeader,
    /// An append from another server in the term this server leads. A term has
    /// one leader; the leader never yields its log to a rival.
    RivalLeader,
    /// An entry differs from the one this log holds at the same index and term.
    Mismatch,
    /// The entries would replace one this server has committed.
    CommittedConflict,
    /// The response reports a match past the end of the leader's log.
    BeyondLog,
    /// The log holds as many entries as it can.
    LogFull,
    /// The term is `u64::MAX`; no later term exists.
    TermExhausted,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UnknownMember => "the index names no seat of the membership",
            Self::OwnSeat => "the index names this server's own seat",
            Self::Malformed => "the message contradicts itself",
            Self::TooManyEntries => "the entries end past the capacity of the log",
            Self::Stale => "the response is from a term this server has left",
            Self::NotCandidate => "this server is not campaigning in the current term",
            Self::NotLeader => "this server does not lead the current term",
            Self::RivalLeader => "another server claims the term this server leads",
            Self::Mismatch => "an entry differs from the one held at the same index and term",
            Self::CommittedConflict => "the entries would replace a committed entry",
            Self::BeyondLog => "the response reports a match past the end of the log",
            Self::LogFull => "the log is full",
            Self::TermExhausted => "no later term exists",
        })
    }
}

impl core::error::Error for Refusal {}
