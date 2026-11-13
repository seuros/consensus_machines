//! One server fed arbitrary, hostile input. Whatever arrives: a refusal leaves
//! it equal to its clone, an honoured repeat too, a term adoption is exactly
//! what the answer reports, and its own durable promises hold.
use std::vec::Vec;

use super::*;

const LOG: usize = 4;
type Node = Raft<3, LOG>;

#[derive(Default)]
struct Seen {
    refusals: Vec<Refusal>,
    adoptions: usize,
    repeats: usize,
    /// `(term, seat)` of every vote this server gave, across restores.
    votes: Vec<(u64, usize)>,
}

impl Seen {
    fn refusal(&mut self, refusal: Refusal) {
        if !self.refusals.contains(&refusal) {
            self.refusals.push(refusal);
        }
    }

    fn vote(&mut self, term: u64, seat: usize) {
        for (earlier, given) in &self.votes {
            assert!(*earlier != term || *given == seat, "two votes in one term");
        }
        self.votes.push((term, seat));
    }
}

/// What must hold across any one call.
fn durable(before: &Node, after: &Node) {
    assert!(after.term() >= before.term(), "the term went back");
    assert!(after.committed() >= before.committed(), "commitment lost");
    for index in 1..=before.committed() {
        assert_eq!(after.entry(index), before.entry(index), "committed entry");
    }
    if before.role() == Role::Leader && after.term() == before.term() {
        assert_eq!(after.role(), Role::Leader);
        for index in 1..=before.last().index {
            assert_eq!(after.entry(index), before.entry(index), "leader log");
        }
    }
}

/// Stage one of an inbound message: the answer reports the adoption exactly.
fn reconciled<T>(
    seen: &mut Seen,
    before: &Node,
    after: &Node,
    sent: u64,
    answer: &Result<Received<T>, Refusal>,
) {
    match answer {
        Err(refusal) => {
            assert_eq!(after, before, "{refusal:?} changed the server");
            seen.refusal(*refusal);
        }
        Ok(received) => {
            assert_eq!(received.role, after.role());
            if let Some(term) = received.adopted {
                assert_eq!(term, sent);
                assert!(before.term() < term);
                assert_eq!(after.term(), term);
                assert_eq!(after.role(), Role::Follower);
                seen.adoptions += 1;
            } else {
                assert!(sent <= before.term());
                assert_eq!(after.term(), before.term());
            }
        }
    }
}

/// A position around `term`: mostly plausible, sometimes impossible.
fn position(rng: &mut Rng, term: u64) -> Position {
    Position {
        index: rng.below(LOG + 2),
        term: match rng.below(8) {
            0 => term + 1,
            1 => 0,
            _ => term.saturating_sub(rng.next() % 3),
        },
    }
}

fn vote_request(node: &mut Node, rng: &mut Rng, seen: &mut Seen, peer: usize, term: u64) {
    let before = node.clone();
    let request = VoteRequest {
        candidate: peer,
        term,
        last: position(rng, term),
    };
    let answer = node.request_vote(request);
    reconciled(seen, &before, node, term, &answer);
    let Ok(received) = answer else { return };
    assert_eq!(received.outcome.term, node.term());
    let kept = before.checkpoint().voted_for();
    match received.outcome.vote {
        Vote::Granted => seen.vote(node.term(), peer),
        Vote::Duplicate => {
            assert!(seen.votes.contains(&(node.term(), peer)));
            assert_eq!(*node, before);
            seen.repeats += 1;
        }
        Vote::Denied(Denial::TermBehind) => assert_eq!(*node, before),
        Vote::Denied(Denial::AlreadyVoted | Denial::LogBehind) => assert_eq!(
            node.checkpoint().voted_for(),
            kept.filter(|_| received.adopted.is_none())
        ),
    }
}

fn vote_reply(node: &mut Node, rng: &mut Rng, seen: &mut Seen, peer: usize, term: u64) {
    let before = node.clone();
    let vote = match rng.below(5) {
        0 => Vote::Denied(Denial::TermBehind),
        1 => Vote::Denied(Denial::AlreadyVoted),
        2 => Vote::Duplicate,
        _ => Vote::Granted,
    };
    let answer = node.receive_vote(peer, VoteResponse { term, vote });
    reconciled(seen, &before, node, term, &answer);
    let Ok(received) = answer else { return };
    assert_eq!(
        received.outcome == Ballot::Overtaken,
        received.adopted.is_some()
    );
    match received.outcome {
        Ballot::Duplicate | Ballot::Denied => {
            assert_eq!(*node, before);
            seen.repeats += 1;
        }
        Ballot::Elected => assert_eq!(node.role(), Role::Leader),
        Ballot::Counted | Ballot::Overtaken => {}
    }
}

fn append_request(node: &mut Node, rng: &mut Rng, seen: &mut Seen, peer: usize, term: u64) {
    let before = node.clone();
    let request = AppendRequest {
        leader: peer,
        term,
        previous: position(rng, term),
        committed: rng.below(LOG + 2),
    };
    let mut entries = [Entry::default(); LOG + 1];
    let count = rng.below(LOG + 2);
    let mut floor = request.previous.term;
    for entry in &mut entries[..count] {
        floor = (floor + rng.next() % 2).min(term.max(1));
        *entry = Entry {
            term: match rng.below(24) {
                0 => 0,
                1 => term + 1,
                _ => floor,
            },
            command: rng.next() % 2,
        };
    }
    let answer = node.append_entries(request, &entries[..count]);
    reconciled(seen, &before, node, term, &answer);
    let Ok(received) = answer else { return };
    assert_eq!(received.outcome.term, node.term());
    match received.outcome.append {
        Append::TermBehind => assert_eq!(*node, before),
        Append::Conflict => assert_eq!(node.checkpoint(), {
            let mut expected = before.clone();
            expected.adopt(term);
            expected.checkpoint()
        }),
        Append::Matched(end) => {
            assert_eq!(end, request.previous.index + count);
            for (offset, entry) in entries[..count].iter().enumerate() {
                let index = request.previous.index + offset + 1;
                assert_eq!(node.entry(index), Some(*entry));
            }
        }
    }
}

fn append_reply(node: &mut Node, rng: &mut Rng, seen: &mut Seen, peer: usize, term: u64) {
    let before = node.clone();
    let append = match rng.below(5) {
        0 => Append::TermBehind,
        1 => Append::Conflict,
        _ => Append::Matched(rng.below(LOG + 2)),
    };
    let answer = node.acknowledge(peer, AppendResponse { term, append });
    reconciled(seen, &before, node, term, &answer);
    let Ok(received) = answer else { return };
    assert_eq!(
        received.outcome == Ack::Overtaken,
        received.adopted.is_some()
    );
    match received.outcome {
        Ack::Duplicate | Ack::Conflict => {
            assert_eq!(*node, before);
            seen.repeats += 1;
        }
        Ack::Advanced | Ack::Overtaken => {}
    }
}

fn local(node: &mut Node, rng: &mut Rng, seen: &mut Seen) {
    let before = node.clone();
    let answer = if rng.below(3) == 0 {
        node.begin_election().map(|request| {
            assert_eq!(request.term, before.term() + 1);
            seen.vote(request.term, before.own());
        })
    } else {
        node.propose(rng.next() % 3)
            .map(|index| assert_eq!(index, before.last().index + 1))
    };
    if let Err(refusal) = answer {
        assert_eq!(*node, before, "{refusal:?} changed the server");
        seen.refusal(refusal);
    }
}

fn step(node: &mut Node, rng: &mut Rng, seen: &mut Seen) {
    let before = node.clone();
    let peer = rng.below(4);
    // Below, at and above the server's own term.
    let term = (before.term() + rng.next() % 4).saturating_sub(2);
    match rng.below(12) {
        0..=2 => local(node, rng, seen),
        3 | 4 => vote_request(node, rng, seen, peer, term),
        5 | 6 => vote_reply(node, rng, seen, peer, term),
        7 | 8 => append_request(node, rng, seen, peer, term),
        9 | 10 => append_reply(node, rng, seen, peer, term),
        _ => {
            *node = Raft::restored(MEMBERS, before.own(), node.checkpoint()).expect("checkpoint");
            assert_eq!(node.checkpoint(), before.checkpoint());
            assert_eq!(node.role(), Role::Follower);
            return;
        }
    }
    durable(&before, node);
}

#[test]
fn hostile_input_never_breaks_a_refusal_a_repeat_or_a_durable_promise() {
    let mut seen = Seen::default();
    for seed in 0..400 {
        let mut rng = Rng(seed);
        let mut node: Node = node(rng.below(3));
        seen.votes.clear();
        for _ in 0..2_000 {
            step(&mut node, &mut rng, &mut seen);
        }
    }
    let expected = [
        Refusal::UnknownMember,
        Refusal::OwnSeat,
        Refusal::Malformed,
        Refusal::TooManyEntries,
        Refusal::Stale,
        Refusal::NotCandidate,
        Refusal::NotLeader,
        Refusal::RivalLeader,
        Refusal::Mismatch,
        Refusal::CommittedConflict,
        Refusal::BeyondLog,
        Refusal::LogFull,
    ];
    for refusal in expected {
        assert!(seen.refusals.contains(&refusal), "never saw {refusal:?}");
    }
    assert_eq!(seen.refusals.len(), expected.len());
    assert!(seen.adoptions > 1_000);
    assert!(seen.repeats > 1_000);
}
