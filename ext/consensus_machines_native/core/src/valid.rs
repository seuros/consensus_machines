//! Validating constructors: the one definition of a well-formed value. The
//! fields stay public for reading and matching; a value built through `new`
//! is one a server can send, and the checks a server runs on an inbound
//! request are these same constructors.
use crate::{
    Ack, Append, AppendRequest, AppendResponse, Ballot, Denial, Entry, Position, Received, Role,
    Vote, VoteRequest, VoteResponse,
};

impl Position {
    /// `None` unless index and term are both zero (the empty prefix) or both
    /// set.
    #[must_use]
    pub const fn new(index: usize, term: u64) -> Option<Self> {
        if (index == 0) == (term == 0) {
            Some(Self { index, term })
        } else {
            None
        }
    }
}

impl Entry {
    /// `None` for term zero: no leader exists in it to create an entry.
    #[must_use]
    pub const fn new(term: u64, command: u64) -> Option<Self> {
        if term == 0 {
            None
        } else {
            Some(Self { term, command })
        }
    }
}

impl VoteRequest {
    /// `None` for a zero term, or a `last` that is not a position or is from
    /// a term above the request's.
    #[must_use]
    pub fn new(candidate: usize, term: u64, last: Position) -> Option<Self> {
        let last = Position::new(last.index, last.term)?;
        (term != 0 && last.term <= term).then_some(Self {
            candidate,
            term,
            last,
        })
    }
}

impl VoteResponse {
    /// `None` for a zero term, or a `Denied(TermBehind)` below term two: the
    /// responder's term is above a request's, and no request has term zero.
    #[must_use]
    pub const fn new(term: u64, vote: Vote) -> Option<Self> {
        let floor = match vote {
            Vote::Denied(Denial::TermBehind) => 2,
            Vote::Granted
            | Vote::Duplicate
            | Vote::Denied(Denial::AlreadyVoted | Denial::LogBehind) => 1,
        };
        if term < floor {
            None
        } else {
            Some(Self { term, vote })
        }
    }
}

impl AppendRequest {
    /// `None` for a zero term, or a `previous` that is not a position or is
    /// from a term above the request's.
    #[must_use]
    pub fn new(leader: usize, term: u64, previous: Position, committed: usize) -> Option<Self> {
        let previous = Position::new(previous.index, previous.term)?;
        (term != 0 && previous.term <= term).then_some(Self {
            leader,
            term,
            previous,
            committed,
        })
    }
}

impl AppendResponse {
    /// `None` for a zero term, or a `TermBehind` below term two: the
    /// responder's term is above a request's, and no request has term zero.
    #[must_use]
    pub const fn new(term: u64, append: Append) -> Option<Self> {
        let floor = match append {
            Append::TermBehind => 2,
            Append::Matched(_) | Append::Conflict => 1,
        };
        if term < floor {
            None
        } else {
            Some(Self { term, append })
        }
    }
}

/// What a [`Received`] can carry, and which stages each outcome can follow.
pub trait Outcome: Copy {
    /// Whether a server can answer this outcome after adopting `adopted` (or
    /// keeping its term) and ending in `role`.
    fn follows(&self, adopted: Option<u64>, role: Role) -> bool;
}

const fn follower(role: Role) -> bool {
    match role {
        Role::Follower => true,
        Role::Candidate | Role::Leader => false,
    }
}

impl Outcome for VoteResponse {
    /// A grant or its repeat leaves a follower; `Denied(TermBehind)` adopts
    /// nothing; an adopted term is the one the reply carries.
    fn follows(&self, adopted: Option<u64>, role: Role) -> bool {
        Self::new(self.term, self.vote).is_some()
            && adopted.is_none_or(|term| term == self.term)
            && match self.vote {
                Vote::Granted | Vote::Duplicate => follower(role),
                Vote::Denied(Denial::TermBehind) => adopted.is_none(),
                Vote::Denied(Denial::AlreadyVoted | Denial::LogBehind) => true,
            }
    }
}

impl Outcome for AppendResponse {
    /// A match or a conflict leaves a follower; `TermBehind` adopts nothing;
    /// an adopted term is the one the reply carries.
    fn follows(&self, adopted: Option<u64>, role: Role) -> bool {
        Self::new(self.term, self.append).is_some()
            && adopted.is_none_or(|term| term == self.term)
            && match self.append {
                Append::Matched(_) | Append::Conflict => follower(role),
                Append::TermBehind => adopted.is_none(),
            }
    }
}

impl Outcome for Ballot {
    /// `Overtaken` exactly when a term was adopted; `Elected` leaves a
    /// leader; the rest leave the candidate or leader that was asked.
    fn follows(&self, adopted: Option<u64>, role: Role) -> bool {
        match self {
            Self::Overtaken => adopted.is_some(),
            Self::Elected => adopted.is_none() && role == Role::Leader,
            Self::Counted | Self::Duplicate | Self::Denied => adopted.is_none() && !follower(role),
        }
    }
}

impl Outcome for Ack {
    /// `Overtaken` exactly when a term was adopted; the rest leave the leader.
    fn follows(&self, adopted: Option<u64>, role: Role) -> bool {
        match self {
            Self::Overtaken => adopted.is_some(),
            Self::Advanced | Self::Duplicate | Self::Conflict => {
                adopted.is_none() && role == Role::Leader
            }
        }
    }
}

impl<T: Outcome> Received<T> {
    /// `None` unless a server can give this answer: an adopted term is not
    /// zero and leaves a follower, and the outcome can follow those stages.
    #[must_use]
    pub fn new(adopted: Option<u64>, role: Role, outcome: T) -> Option<Self> {
        let moved = adopted.is_none_or(|term| term != 0 && follower(role));
        (moved && outcome.follows(adopted, role)).then_some(Self {
            adopted,
            role,
            outcome,
        })
    }
}
