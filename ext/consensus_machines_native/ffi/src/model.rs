//! Exact-membership type erasure; all transitions delegate to the core.
use consensus_machines::{
    AppendRequest, AppendResponse, Checkpoint, Entry, Position, Raft, Role, VoteRequest,
    VoteResponse,
};

pub trait Durable: Send {
    fn restore(&self) -> Option<Box<dyn Model>>;
}
pub trait Model: Send {
    fn role(&self) -> Role;
    fn term(&self) -> u64;
    fn committed(&self) -> usize;
    fn last(&self) -> Position;
    fn entry(&self, index: usize) -> Option<Entry>;
    fn begin_election(&mut self) -> Option<VoteRequest>;
    fn request_vote(&mut self, request: VoteRequest) -> VoteResponse;
    fn receive_vote(&mut self, peer: usize, response: VoteResponse) -> Role;
    fn propose(&mut self, command: u64) -> Option<usize>;
    fn append_entries(&mut self, request: AppendRequest, entries: &[Entry]) -> AppendResponse;
    fn acknowledge(&mut self, peer: usize, term: u64, index: usize) -> bool;
    fn checkpoint(&self) -> Box<dyn Durable>;
}
impl<const N: usize> Durable for Checkpoint<N, 64> {
    fn restore(&self) -> Option<Box<dyn Model>> {
        Raft::restore(self.clone()).map(|node| Box::new(node) as Box<dyn Model>)
    }
}
impl<const N: usize> Model for Raft<N, 64> {
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
    fn begin_election(&mut self) -> Option<VoteRequest> {
        self.begin_election()
    }
    fn request_vote(&mut self, request: VoteRequest) -> VoteResponse {
        self.request_vote(request)
    }
    fn receive_vote(&mut self, peer: usize, response: VoteResponse) -> Role {
        self.receive_vote(peer, response)
    }
    fn propose(&mut self, command: u64) -> Option<usize> {
        self.propose(command)
    }
    fn append_entries(&mut self, request: AppendRequest, entries: &[Entry]) -> AppendResponse {
        self.append_entries(request, entries)
    }
    fn acknowledge(&mut self, peer: usize, term: u64, index: usize) -> bool {
        self.acknowledge(peer, term, index)
    }
    fn checkpoint(&self) -> Box<dyn Durable> {
        Box::new(self.checkpoint())
    }
}
fn sized<const N: usize>(members: &[u64], own: usize) -> Option<Box<dyn Model>> {
    let members: [u64; N] = members.try_into().ok()?;
    Raft::<N, 64>::new(members, own).map(|node| Box::new(node) as Box<dyn Model>)
}
pub fn new(members: &[u64], own: usize) -> Option<Box<dyn Model>> {
    match members.len() {
        1 => sized::<1>(members, own),
        2 => sized::<2>(members, own),
        3 => sized::<3>(members, own),
        4 => sized::<4>(members, own),
        5 => sized::<5>(members, own),
        6 => sized::<6>(members, own),
        7 => sized::<7>(members, own),
        8 => sized::<8>(members, own),
        9 => sized::<9>(members, own),
        _ => None,
    }
}
