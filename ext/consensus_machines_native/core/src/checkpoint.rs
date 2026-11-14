use crate::{Entry, Position, log::Log};

/// A server's durable state: what must reach stable storage before any answer
/// that depends on it leaves the server.
///
/// A plain value. Read it with the accessors to persist it in a format you
/// own, rebuild it with [`Checkpoint::new`], and hand it to
/// [`Raft::restored`](crate::Raft::restored).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint<const N: usize, const L: usize> {
    pub(crate) members: [u64; N],
    pub(crate) own: usize,
    pub(crate) term: u64,
    pub(crate) voted_for: Option<usize>,
    pub(crate) committed: usize,
    pub(crate) log: Log<L>,
}

impl<const N: usize, const L: usize> Checkpoint<N, L> {
    /// Rebuild persisted state.
    ///
    /// `None` unless it is a state a server can be in: distinct members, `own`
    /// and `voted_for` seats of the membership, no vote in term zero (every
    /// vote is cast in an election's term, at least one), at most `L` entries whose
    /// terms are positive, never decrease and never exceed `term`, and
    /// `committed` within the entries.
    #[must_use]
    pub fn new(
        members: [u64; N],
        own: usize,
        term: u64,
        voted_for: Option<usize>,
        committed: usize,
        entries: &[Entry],
    ) -> Option<Self> {
        if !distinct(&members) || own >= N || voted_for.is_some_and(|seat| seat >= N) {
            return None;
        }
        if term == 0 && voted_for.is_some() {
            return None;
        }
        if committed > entries.len() {
            return None;
        }
        let mut log = Log::new();
        let mut floor = 1;
        for entry in entries {
            if entry.term < floor || entry.term > term {
                return None;
            }
            floor = entry.term;
            // `None` here is the capacity check: more than `L` entries.
            log.push(*entry)?;
        }
        Some(Self {
            members,
            own,
            term,
            voted_for,
            committed,
            log,
        })
    }

    /// The membership the state belongs to.
    #[must_use]
    pub const fn members(&self) -> [u64; N] {
        self.members
    }

    /// The server's seat in the membership.
    #[must_use]
    pub const fn own(&self) -> usize {
        self.own
    }

    /// Current term.
    #[must_use]
    pub const fn term(&self) -> u64 {
        self.term
    }

    /// The seat given this server's vote of the term, if it voted.
    #[must_use]
    pub const fn voted_for(&self) -> Option<usize> {
        self.voted_for
    }

    /// Highest committed index.
    #[must_use]
    pub const fn committed(&self) -> usize {
        self.committed
    }

    /// Last log position; its index is the number of entries.
    #[must_use]
    pub fn last(&self) -> Position {
        self.log.last()
    }

    /// The entry at a one-based index.
    #[must_use]
    pub const fn entry(&self, index: usize) -> Option<Entry> {
        self.log.entry(index)
    }
}

/// Whether the membership is non-empty and names no member twice.
pub fn distinct(members: &[u64]) -> bool {
    !members.is_empty()
        && members
            .iter()
            .enumerate()
            .all(|(index, member)| !members[..index].contains(member))
}
