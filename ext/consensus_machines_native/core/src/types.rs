/// A log position; index zero has term zero.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Position {
    /// One-based index, or zero for the empty prefix.
    pub index: usize,
    /// Entry term, or zero for the empty prefix.
    pub term: u64,
}

/// One immutable command identity in the bounded log.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Entry {
    /// Term of the leader that created the entry.
    pub term: u64,
    /// Stable identity of command data stored by the runtime.
    pub command: u64,
}

/// Raft role under one static membership.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Not campaigning.
    Follower,
    /// Requesting votes.
    Candidate,
    /// Won a distinct-member majority in this term.
    Leader,
}

/// What an honoured inbound message did, in the two stages Raft defines.
///
/// Stage one reconciles terms: `adopted` is `Some(term)` when the sender was
/// in a higher term and this server moved into it, clearing its vote and
/// becoming a follower. Stage two is the message itself: `outcome`. The two
/// are independent; a request can be denied after its term was adopted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Received<T> {
    /// The higher term this call adopted, or `None` when the term was kept.
    pub adopted: Option<u64>,
    /// The role after the call.
    pub role: Role,
    /// What the message itself came to.
    pub outcome: T,
}

/// `RequestVote`: what a candidate broadcasts and a voter is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoteRequest {
    /// Candidate membership index.
    pub candidate: usize,
    /// Candidate term.
    pub term: u64,
    /// Candidate's final log position.
    pub last: Position,
}

/// The reply to a `RequestVote`; persist node state before sending it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoteResponse {
    /// Responder's term after reconciling with the request.
    pub term: u64,
    /// The responder's ballot.
    pub vote: Vote,
}

/// A voter's answer. Every variant is an honoured reply to a well-formed request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vote {
    /// The voter gave this candidate its one vote of the term.
    Granted,
    /// The voter had already given this candidate its vote of the term;
    /// nothing changed. A candidate counts it like `Granted`.
    Duplicate,
    /// The voter keeps its vote from this candidate.
    Denied(Denial),
}

/// Why Raft has a voter deny a candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denial {
    /// The candidate's term is below the voter's.
    TermBehind,
    /// The voter's vote of this term went to another server.
    AlreadyVoted,
    /// The candidate's log is not as up to date as the voter's.
    LogBehind,
}

/// What an honoured ballot did to the election it was folded into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ballot {
    /// A grant from a member not yet counted this term.
    Counted,
    /// A grant that completed the majority: this server now leads the term.
    Elected,
    /// A grant from a member already counted this term; nothing changed.
    Duplicate,
    /// The member denied its vote; nothing changed.
    Denied,
    /// The member was in a higher term, which was adopted; the ballot counts
    /// for nothing.
    Overtaken,
}

/// `AppendEntries` metadata; the entries travel beside it as a slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppendRequest {
    /// Leader membership index.
    pub leader: usize,
    /// Leader term.
    pub term: u64,
    /// Position immediately preceding the entries.
    pub previous: Position,
    /// Leader's committed index.
    pub committed: usize,
}

/// The reply to an `AppendEntries`; persist node state before sending it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppendResponse {
    /// Responder's term after reconciling with the request.
    pub term: u64,
    /// What became of the entries.
    pub append: Append,
}

/// A follower's answer. Every variant is an honoured reply to a well-formed request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Append {
    /// The log agrees with the leader's up to this index: `previous` matched
    /// and the entries, if any, are stored.
    Matched(usize),
    /// The log holds no entry at `previous` with that term; the leader must
    /// retry from an earlier position. The log is unchanged.
    Conflict,
    /// The leader's term is below the responder's. The log is unchanged.
    TermBehind,
}

/// What an honoured append reply did to the leader's replication progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ack {
    /// The member's match moved up; `committed` may have moved with it.
    Advanced,
    /// The match is at or below the one recorded for the member; nothing changed.
    Duplicate,
    /// The member reported a conflict; nothing changed, retry earlier.
    Conflict,
    /// The member was in a higher term, which was adopted; this server no
    /// longer leads.
    Overtaken,
}
