#![no_std]
#![doc = include_str!("../README.md")]
mod checkpoint;
mod election;
mod log;
mod refusal;
mod replication;
mod types;

pub use checkpoint::Checkpoint;
pub use refusal::Refusal;
pub use types::{
    Ack, Append, AppendRequest, AppendResponse, Ballot, Denial, Entry, Position, Received, Role,
    Vote, VoteRequest, VoteResponse,
};

use log::Log;
use quorum_machines::Quorum;

/// Deterministic Raft transition model with bounded log and static peers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raft<const N: usize, const L: usize> {
    state: Checkpoint<N, L>,
    role: Role,
    votes: Quorum<N>,
    matched: [usize; N],
}

impl<const N: usize, const L: usize> Raft<N, L> {
    /// A follower in term zero with an empty log.
    ///
    /// `None` unless the members are distinct, at least one, and `own` is the
    /// index of one of them.
    #[must_use]
    pub fn new(members: [u64; N], own: usize) -> Option<Self> {
        if own >= N || !checkpoint::distinct(&members) {
            return None;
        }
        Some(Self {
            state: Checkpoint {
                members,
                own,
                term: 0,
                voted_for: None,
                committed: 0,
                log: Log::new(),
            },
            role: Role::Follower,
            votes: Quorum::majority(N)?,
            matched: [0; N],
        })
    }

    /// The server `members[own]` as its checkpoint left it, as a follower.
    ///
    /// The membership and seat are stated, never read out of the checkpoint:
    /// `None` when the checkpoint belongs to another membership or another
    /// seat. Whether it is the newest checkpoint of that server is the
    /// caller's persistence contract; the model cannot know.
    #[must_use]
    pub fn restored(members: [u64; N], own: usize, checkpoint: Checkpoint<N, L>) -> Option<Self> {
        if checkpoint.members != members || checkpoint.own != own {
            return None;
        }
        let mut node = Self::new(members, own)?;
        node.state = checkpoint;
        Some(node)
    }

    /// Capture state to durably commit before using any proposed decisions.
    #[must_use]
    pub fn checkpoint(&self) -> Checkpoint<N, L> {
        self.state.clone()
    }

    /// The static membership.
    #[must_use]
    pub const fn members(&self) -> [u64; N] {
        self.state.members
    }

    /// This server's seat in the membership.
    #[must_use]
    pub const fn own(&self) -> usize {
        self.state.own
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

    /// Refuse an index that is not another member's seat.
    const fn peer(&self, index: usize) -> Result<(), Refusal> {
        if index >= N {
            Err(Refusal::UnknownMember)
        } else if index == self.state.own {
            Err(Refusal::OwnSeat)
        } else {
            Ok(())
        }
    }

    /// Stage one of every inbound message: move into the sender's term when
    /// it is higher, and say so.
    const fn adopt(&mut self, term: u64) -> Option<u64> {
        if term <= self.state.term {
            return None;
        }
        self.state.term = term;
        self.state.voted_for = None;
        self.role = Role::Follower;
        self.votes.reset();
        Some(term)
    }

    const fn received<T>(&self, adopted: Option<u64>, outcome: T) -> Received<T> {
        Received {
            adopted,
            role: self.role,
            outcome,
        }
    }

    fn become_leader(&mut self) {
        self.role = Role::Leader;
        self.matched = [0; N];
        self.matched[self.state.own] = self.state.log.len();
        self.advance_commit();
    }

    /// Commit the highest current-term entry a majority has matched.
    fn advance_commit(&mut self) {
        for index in (self.state.committed + 1..=self.state.log.len()).rev() {
            if self
                .entry(index)
                .is_none_or(|entry| entry.term != self.term())
            {
                return;
            }
            let holders = self.matched.iter().filter(|held| **held >= index).count();
            if holders >= self.votes.required() {
                self.state.committed = index;
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests;
