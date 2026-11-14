#![allow(clippy::expect_used)]
extern crate std;

mod cluster;
mod election;
mod fuzz;

mod refusals;
mod replication;
mod restore;
mod values;

use core::fmt::Debug;

use super::*;

const MEMBERS: [u64; 3] = [10, 20, 30];
const EMPTY: Position = at(0, 0);

fn node<const L: usize>(own: usize) -> Raft<3, L> {
    Raft::new(MEMBERS, own).expect("membership")
}

const fn ask(candidate: usize, term: u64, last: Position) -> VoteRequest {
    VoteRequest {
        candidate,
        term,
        last,
    }
}

const fn at(index: usize, term: u64) -> Position {
    Position { index, term }
}

const fn ballot(term: u64, vote: Vote) -> VoteResponse {
    VoteResponse { term, vote }
}

const fn matched(term: u64, index: usize) -> AppendResponse {
    AppendResponse {
        term,
        append: Append::Matched(index),
    }
}

const fn entry(term: u64, command: u64) -> Entry {
    Entry { term, command }
}

const fn append(leader: usize, term: u64, previous: Position, committed: usize) -> AppendRequest {
    AppendRequest {
        leader,
        term,
        previous,
        committed,
    }
}

/// Seat 0 of three, elected in term 1 by seat 1.
fn leader<const L: usize>() -> Raft<3, L> {
    let mut node = node(0);
    let request = node.begin_election().expect("term");
    let tally = node
        .receive_vote(1, ballot(request.term, Vote::Granted))
        .expect("ballot");
    assert_eq!(tally.outcome, Ballot::Elected);
    assert_eq!(tally.role, Role::Leader);
    node
}

/// The call is refused for `why` and the whole server equals its clone.
fn refused<T: Debug, const N: usize, const L: usize>(
    node: &mut Raft<N, L>,
    why: Refusal,
    call: impl FnOnce(&mut Raft<N, L>) -> Result<T, Refusal>,
) {
    let before = node.clone();
    assert_eq!(call(node).err(), Some(why));
    assert_eq!(*node, before, "a refusal changed the server");
}

/// The call is honoured and the whole server equals its clone.
fn unchanged<T, const N: usize, const L: usize>(
    node: &mut Raft<N, L>,
    call: impl FnOnce(&mut Raft<N, L>) -> Result<T, Refusal>,
) -> T {
    let before = node.clone();
    let answer = call(node).expect("honoured");
    assert_eq!(*node, before, "an honoured repeat changed the server");
    answer
}

/// A small deterministic generator for the randomized runs.
struct Rng(u64);

impl Rng {
    const fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut mixed = self.0;
        mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        mixed ^ (mixed >> 31)
    }

    fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.next() % (bound as u64)).expect("small")
    }
}
