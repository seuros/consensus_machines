//! Exact-membership type erasure; all transitions delegate to the core.
use consensus_machines::{
    Ack, AppendRequest, AppendResponse, Ballot, Checkpoint, Entry, Position, Raft, Received,
    Refusal, Role, VoteRequest, VoteResponse,
};

/// Log capacity compiled into the binding.
pub const LOG: usize = 64;

/// A core `Checkpoint` of any membership size, field for field.
pub struct Saved {
    pub members: Vec<u64>,
    pub own: usize,
    pub term: u64,
    pub voted_for: Option<usize>,
    pub committed: usize,
    pub entries: Vec<Entry>,
}

pub trait Model: Send {
    fn members(&self) -> Vec<u64>;
    fn own(&self) -> usize;
    fn role(&self) -> Role;
    fn term(&self) -> u64;
    fn committed(&self) -> usize;
    fn last(&self) -> Position;
    fn entry(&self, index: usize) -> Option<Entry>;
    fn begin_election(&mut self) -> Result<VoteRequest, Refusal>;
    fn request_vote(&mut self, request: VoteRequest) -> Result<Received<VoteResponse>, Refusal>;
    fn receive_vote(
        &mut self,
        peer: usize,
        response: VoteResponse,
    ) -> Result<Received<Ballot>, Refusal>;
    fn propose(&mut self, command: u64) -> Result<usize, Refusal>;
    fn append_entries(
        &mut self,
        request: AppendRequest,
        entries: &[Entry],
    ) -> Result<Received<AppendResponse>, Refusal>;
    fn acknowledge(
        &mut self,
        peer: usize,
        response: AppendResponse,
    ) -> Result<Received<Ack>, Refusal>;
    fn checkpoint(&self) -> Saved;
}

impl<const N: usize> Model for Raft<N, LOG> {
    fn members(&self) -> Vec<u64> {
        self.members().to_vec()
    }
    fn own(&self) -> usize {
        self.own()
    }
    fn role(&self) -> Role {
        self.role()
    }
    fn term(&self) -> u64 {
        self.term()
    }
    fn committed(&self) -> usize {
        self.committed()
    }
    fn last(&self) -> Position {
        self.last()
    }
    fn entry(&self, index: usize) -> Option<Entry> {
        self.entry(index)
    }
    fn begin_election(&mut self) -> Result<VoteRequest, Refusal> {
        self.begin_election()
    }
    fn request_vote(&mut self, request: VoteRequest) -> Result<Received<VoteResponse>, Refusal> {
        self.request_vote(request)
    }
    fn receive_vote(
        &mut self,
        peer: usize,
        response: VoteResponse,
    ) -> Result<Received<Ballot>, Refusal> {
        self.receive_vote(peer, response)
    }
    fn propose(&mut self, command: u64) -> Result<usize, Refusal> {
        self.propose(command)
    }
    fn append_entries(
        &mut self,
        request: AppendRequest,
        entries: &[Entry],
    ) -> Result<Received<AppendResponse>, Refusal> {
        self.append_entries(request, entries)
    }
    fn acknowledge(
        &mut self,
        peer: usize,
        response: AppendResponse,
    ) -> Result<Received<Ack>, Refusal> {
        self.acknowledge(peer, response)
    }
    fn checkpoint(&self) -> Saved {
        let saved = self.checkpoint();
        Saved {
            members: saved.members().to_vec(),
            own: saved.own(),
            term: saved.term(),
            voted_for: saved.voted_for(),
            committed: saved.committed(),
            entries: (1..=saved.last().index)
                .filter_map(|index| saved.entry(index))
                .collect(),
        }
    }
}

fn sized<const N: usize>(
    members: &[u64],
    own: usize,
    saved: Option<&Saved>,
) -> Option<Box<dyn Model>> {
    let members: [u64; N] = members.try_into().ok()?;
    let node = match saved {
        None => Raft::<N, LOG>::new(members, own)?,
        Some(saved) => {
            let checkpoint = Checkpoint::new(
                saved.members.as_slice().try_into().ok()?,
                saved.own,
                saved.term,
                saved.voted_for,
                saved.committed,
                &saved.entries,
            )?;
            Raft::restored(members, own, checkpoint)?
        }
    };
    Some(Box::new(node))
}

/// A fresh server, or with `saved` the server that checkpoint describes.
/// `None` for an invalid membership or seat, a checkpoint no server can be
/// in, or one of another membership or seat.
pub fn build(members: &[u64], own: usize, saved: Option<&Saved>) -> Option<Box<dyn Model>> {
    match members.len() {
        1 => sized::<1>(members, own, saved),
        2 => sized::<2>(members, own, saved),
        3 => sized::<3>(members, own, saved),
        4 => sized::<4>(members, own, saved),
        5 => sized::<5>(members, own, saved),
        6 => sized::<6>(members, own, saved),
        7 => sized::<7>(members, own, saved),
        8 => sized::<8>(members, own, saved),
        9 => sized::<9>(members, own, saved),
        _ => None,
    }
}
