#![no_std]
#![doc = include_str!("../README.md")]
mod log;
use log::Log;
use quorum_machines::Quorum;

/// A log position; index zero has term zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// One-based index, or zero for the empty prefix.
    pub index: usize,
    /// Entry term, or zero for the empty prefix.
    pub term: u64,
}
/// One immutable command identity in the bounded log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// Term of the leader that created the entry.
    pub term: u64,
    /// Stable identity of command data stored by the runtime.
    pub command: u64,
}
/// Raft role under one static membership.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Role {
    /// Not campaigning.
    Follower,
    /// Requesting votes.
    Candidate,
    /// Won a distinct-member majority in this term.
    Leader,
}
/// Vote value, excluding transport/persistence success.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Vote {
    /// Vote granted.
    Granted,
    /// Vote denied.
    Denied,
}
/// `RequestVote` input or a proposed outgoing request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoteRequest {
    /// Candidate membership index.
    pub candidate: usize,
    /// Candidate term.
    pub term: u64,
    /// Candidate's final log position.
    pub last: Position,
}
/// Proposed `RequestVote` reply; persist node state before exposing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoteResponse {
    /// Responder's current term.
    pub term: u64,
    /// Ballot.
    pub vote: Vote,
}
/// `AppendEntries` metadata; entry data is passed as a slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppendRequest {
    /// Leader membership index.
    pub leader: usize,
    /// Leader term.
    pub term: u64,
    /// Position immediately preceding incoming entries.
    pub previous: Position,
    /// Leader's committed index.
    pub committed: usize,
}
/// Proposed append reply, requiring durable state before acknowledgement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppendResponse {
    /// Responder's term.
    pub term: u64,
    /// Last matched incoming index, or `None` for rejection.
    pub matched: Option<usize>,
}
/// Opaque persistent state, with no stable wire/disk representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint<const N: usize, const L: usize> {
    members: [u64; N],
    own: usize,
    term: u64,
    voted_for: Option<usize>,
    log: Log<L>,
    committed: usize,
}
/// Deterministic Raft transition model with bounded log and static peers.
#[derive(Debug)]
pub struct Raft<const N: usize, const L: usize> {
    state: Checkpoint<N, L>,
    role: Role,
    votes: Quorum<N>,
    matched: [usize; N],
}
impl<const N: usize, const L: usize> Raft<N, L> {
    /// Validate unique membership and this node's membership index.
    #[must_use]
    pub fn new(members: [u64; N], own: usize) -> Option<Self> {
        if own >= N {
            return None;
        }
        for (index, member) in members.iter().enumerate() {
            if members[..index].contains(member) {
                return None;
            }
        }
        Some(Self {
            state: Checkpoint {
                members,
                own,
                term: 0,
                voted_for: None,
                log: Log::new(),
                committed: 0,
            },
            role: Role::Follower,
            votes: Quorum::majority()?,
            matched: [0; N],
        })
    }
    /// Restore newest committed state, always losing volatile leadership.
    #[must_use]
    pub fn restore(checkpoint: Checkpoint<N, L>) -> Option<Self> {
        let mut node = Self::new(checkpoint.members, checkpoint.own)?;
        node.state = checkpoint;
        Some(node)
    }
    /// Capture state to durably commit before using any proposed decisions.
    #[must_use]
    pub fn checkpoint(&self) -> Checkpoint<N, L> {
        self.state.clone()
    }
    /// Current role.
    #[must_use]
    pub const fn role(&self) -> Role {
        self.role
    }
    /// Current term, never wrapping.
    #[must_use]
    pub const fn term(&self) -> u64 {
        self.state.term
    }
    /// Highest committed index, not an application acknowledgement.
    #[must_use]
    pub const fn committed(&self) -> usize {
        self.state.committed
    }
    /// Last local log position.
    #[must_use]
    pub fn last(&self) -> Position {
        self.state.log.last()
    }
    /// Read one entry; the runtime must not apply uncommitted entries.
    #[must_use]
    pub const fn entry(&self, index: usize) -> Option<Entry> {
        self.state.log.entry(index)
    }

    /// Start a new term and self-vote. Persist before broadcasting this request.
    ///
    /// # Panics
    /// Panics only if the validated own-member invariant is violated.
    pub fn begin_election(&mut self) -> Option<VoteRequest> {
        let term = self.state.term.checked_add(1)?;
        self.state.term = term;
        self.state.voted_for = Some(self.state.own);
        self.role = Role::Candidate;
        self.votes.reset();
        assert!(self.votes.acknowledge(self.state.own));
        if self.votes.reached() {
            self.become_leader();
        }
        Some(VoteRequest {
            candidate: self.state.own,
            term,
            last: self.last(),
        })
    }
    /// Evaluate an authenticated `RequestVote` and propose a reply.
    #[must_use]
    pub fn request_vote(&mut self, request: VoteRequest) -> VoteResponse {
        if request.candidate >= N || request.candidate == self.state.own || request.term == 0 {
            return VoteResponse {
                term: self.term(),
                vote: Vote::Denied,
            };
        }
        if request.term > self.term() {
            self.step_down(request.term);
        }
        let local = self.last();
        let valid_position = request.last.term <= request.term
            && ((request.last.index == 0) == (request.last.term == 0));
        let fresh = (request.last.term, request.last.index) >= (local.term, local.index);
        let available = self
            .state
            .voted_for
            .is_none_or(|peer| peer == request.candidate);
        let vote = if request.term == self.term() && valid_position && fresh && available {
            self.state.voted_for = Some(request.candidate);
            self.role = Role::Follower;
            Vote::Granted
        } else {
            Vote::Denied
        };
        VoteResponse {
            term: self.term(),
            vote,
        }
    }
    /// Fold one authenticated, term-correlated ballot into the election.
    #[must_use]
    pub fn receive_vote(&mut self, peer: usize, response: VoteResponse) -> Role {
        if peer >= N || peer == self.state.own {
            return self.role;
        }
        if response.term > self.term() {
            self.step_down(response.term);
        }
        if self.role == Role::Candidate
            && response.term == self.term()
            && response.vote == Vote::Granted
            && self.votes.acknowledge(peer)
            && self.votes.reached()
        {
            self.become_leader();
        }
        self.role
    }
    /// Add a current-term command identity. Persist before replication/application.
    pub fn propose(&mut self, command: u64) -> Option<usize> {
        if self.role != Role::Leader {
            return None;
        }
        let index = self.state.log.push(Entry {
            term: self.term(),
            command,
        })?;
        self.matched[self.state.own] = index;
        self.advance_commit();
        Some(index)
    }
    /// Match and append an authenticated leader prefix, never truncating commitment.
    #[must_use]
    pub fn append_entries(&mut self, request: AppendRequest, entries: &[Entry]) -> AppendResponse {
        if request.leader >= N || request.leader == self.state.own || request.term == 0 {
            return AppendResponse {
                term: self.term(),
                matched: None,
            };
        }
        if request.term > self.term() {
            self.step_down(request.term);
        }
        if request.term < self.term() {
            return AppendResponse {
                term: self.term(),
                matched: None,
            };
        }
        self.role = Role::Follower;
        let mut previous_term = request.previous.term;
        for entry in entries {
            if entry.term == 0 || entry.term < previous_term || entry.term > request.term {
                return AppendResponse {
                    term: self.term(),
                    matched: None,
                };
            }
            previous_term = entry.term;
        }
        let matched = self
            .state
            .log
            .append(request.previous, entries, self.state.committed);
        if let Some(index) = matched {
            self.state.committed = self.state.committed.max(request.committed.min(index));
        }
        AppendResponse {
            term: self.term(),
            matched,
        }
    }
    /// Record successful durable replication, authenticated and correlated by runtime.
    #[must_use]
    pub fn acknowledge(&mut self, peer: usize, term: u64, index: usize) -> bool {
        if peer >= N || peer == self.state.own {
            return false;
        }
        if term > self.term() {
            self.step_down(term);
        }
        if term != self.term() || self.role != Role::Leader || index > self.state.log.len() {
            return false;
        }
        self.matched[peer] = self.matched[peer].max(index);
        self.advance_commit();
        true
    }
    const fn step_down(&mut self, term: u64) {
        self.state.term = term;
        self.state.voted_for = None;
        self.role = Role::Follower;
        self.votes.reset();
    }
    fn become_leader(&mut self) {
        self.role = Role::Leader;
        self.matched = [0; N];
        self.matched[self.state.own] = self.state.log.len();
        self.advance_commit();
    }
    fn advance_commit(&mut self) {
        for index in (self.state.committed + 1..=self.state.log.len()).rev() {
            if self
                .entry(index)
                .is_none_or(|entry| entry.term != self.term())
            {
                continue;
            }
            let Some(mut quorum) = Quorum::<N>::majority() else {
                return;
            };
            for (peer, matched) in self.matched.iter().enumerate() {
                if *matched >= index && !quorum.acknowledge(peer) {
                    return;
                }
            }
            if quorum.reached() {
                self.state.committed = index;
                break;
            }
        }
    }
}
#[cfg(test)]
mod tests;
