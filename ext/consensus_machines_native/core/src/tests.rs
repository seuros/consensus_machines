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

use matryona_machines::testing::{SplitMix64 as Rng, refuses};

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

/// The call is honoured and the whole server equals its clone
/// (`testing::unchanged`); answers what it was honoured with.
fn unchanged<T: core::fmt::Debug, const N: usize, const L: usize>(
    node: &mut Raft<N, L>,
    call: impl FnOnce(&mut Raft<N, L>) -> Result<T, Refusal>,
) -> T {
    matryona_machines::testing::unchanged(node, call).expect("honoured")
}

/// A draw in `0..bound` from the randomized runs' generator.
fn below(rng: &mut Rng, bound: usize) -> usize {
    usize::try_from(rng.below(bound as u64)).expect("small")
}
