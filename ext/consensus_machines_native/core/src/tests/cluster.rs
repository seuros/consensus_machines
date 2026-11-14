//! Several honest servers on a hostile network: messages are reordered,
//! duplicated and dropped, servers restart from their checkpoints, deposed
//! leaders stay alive and keep sending, and after every step the cluster is
//! checked for election safety, log matching, leader completeness and
//! state-machine safety. Every refusal is compared with a clone; every term
//! adoption with what the answer reported; every message from a term the
//! receiver has left with the one answer Raft allows it; every forged reply
//! with a `Malformed` refusal that adopts nothing.
use std::vec::Vec;

use super::*;

mod deposed;
mod forged;

const LOG: usize = 10;
const WIRE: usize = 48;
type Node<const N: usize> = Raft<N, LOG>;

#[derive(Clone)]
enum Message {
    Vote(usize, VoteRequest),
    VoteReply(usize, usize, VoteResponse),
    Append(usize, AppendRequest, Vec<Entry>),
    AppendReply(usize, usize, AppendResponse),
}

#[derive(Default)]
struct Tally {
    refusals: Vec<Refusal>,
    adoptions: usize,
    repeats: usize,
    elections: usize,
    committed: usize,
    restores: usize,
    /// Old-term appends from deposed leaders: all of them, those that would
    /// replace an entry at or below the receiver's commit index, and those
    /// claiming a commit index past everything the cluster has committed.
    haunted: usize,
    haunted_committed: usize,
    haunted_inflated: usize,
    /// Replies no server can send, each refused `Malformed`.
    forged: usize,
}

struct Cluster<const N: usize> {
    nodes: [Node<N>; N],
    members: [u64; N],
    wire: Vec<Message>,
    rng: Rng,
    /// The server that led each term, as `(term, seat)`.
    leaders: Vec<(u64, usize)>,
    /// Every vote given, as `(voter, term, seat)`; survives restarts.
    votes: Vec<(usize, u64, usize)>,
    /// The committed log, with the term each entry was first seen committed in.
    chosen: Vec<(Entry, u64)>,
    command: u64,
    /// Leaders as they were when a higher term took their office from them.
    deposed: Vec<Node<N>>,
}

impl<const N: usize> Cluster<N> {
    fn new(seed: u64) -> Self {
        let members = core::array::from_fn(|seat| 100 + seat as u64);
        Self {
            nodes: core::array::from_fn(|seat| Raft::new(members, seat).expect("membership")),
            members,
            wire: Vec::new(),
            rng: Rng(seed),
            leaders: Vec::new(),
            votes: Vec::new(),
            chosen: Vec::new(),
            command: 0,
            deposed: Vec::new(),
        }
    }

    fn send(&mut self, message: Message) {
        if self.wire.len() == WIRE {
            let lost = self.rng.below(WIRE);
            self.wire.swap_remove(lost);
        }
        self.wire.push(message);
    }

    fn vote(&mut self, voter: usize, term: u64, seat: usize) {
        for (earlier, at, given) in &self.votes {
            assert!(
                *earlier != voter || *at != term || *given == seat,
                "server {voter} voted twice in term {term}"
            );
        }
        self.votes.push((voter, term, seat));
    }

    /// The refusals an honest cluster can produce: late or misrouted messages
    /// and a full log. Any other would mean a server sent something impossible.
    fn refusal(tally: &mut Tally, refusal: Refusal) {
        match refusal {
            Refusal::Stale | Refusal::NotCandidate | Refusal::NotLeader | Refusal::LogFull => {}
            Refusal::UnknownMember
            | Refusal::OwnSeat
            | Refusal::Malformed
            | Refusal::TooManyEntries
            | Refusal::RivalLeader
            | Refusal::Mismatch
            | Refusal::CommittedConflict
            | Refusal::BeyondLog
            | Refusal::TermExhausted => panic!("an honest cluster was refused {refusal:?}"),
        }
        if !tally.refusals.contains(&refusal) {
            tally.refusals.push(refusal);
        }
    }

    /// An honoured repeat: the server equals its clone.
    fn repeat(tally: &mut Tally, before: &Node<N>, after: &Node<N>) {
        assert_eq!(after, before, "an honoured repeat changed the server");
        tally.repeats += 1;
    }

    /// Stage one, checked the same way for all four inbound messages.
    fn reconciled<T>(
        tally: &mut Tally,
        before: &Node<N>,
        after: &Node<N>,
        sent: u64,
        answer: &Result<Received<T>, Refusal>,
    ) {
        assert!(after.term() >= before.term());
        match answer {
            Err(refusal) => {
                assert_eq!(after, before, "{refusal:?} changed the server");
                Self::refusal(tally, *refusal);
            }
            Ok(received) => {
                assert_eq!(received.role, after.role());
                if let Some(term) = received.adopted {
                    assert!(before.term() < term && term == sent);
                    assert_eq!((after.term(), after.role()), (term, Role::Follower));
                    tally.adoptions += 1;
                } else {
                    assert_eq!(after.term(), before.term());
                }
            }
        }
    }

    fn deliver(&mut self, message: Message, tally: &mut Tally) {
        let (Message::Vote(to, _)
        | Message::VoteReply(to, ..)
        | Message::Append(to, ..)
        | Message::AppendReply(to, ..)) = message;
        let before = self.nodes[to].clone();
        self.receive(message, tally);
        self.depose(&before, to);
    }

    fn receive(&mut self, message: Message, tally: &mut Tally) {
        match message {
            Message::Vote(to, request) => {
                let before = self.nodes[to].clone();
                let answer = self.nodes[to].request_vote(request);
                Self::reconciled(tally, &before, &self.nodes[to], request.term, &answer);
                if let Ok(received) = answer {
                    let behind = received.outcome.vote == Vote::Denied(Denial::TermBehind);
                    assert_eq!(behind, request.term < before.term(), "a stale candidate");
                    match received.outcome.vote {
                        Vote::Granted => self.vote(to, received.outcome.term, request.candidate),
                        Vote::Duplicate => Self::repeat(tally, &before, &self.nodes[to]),
                        Vote::Denied(Denial::TermBehind) => assert_eq!(self.nodes[to], before),
                        Vote::Denied(Denial::AlreadyVoted | Denial::LogBehind) => {}
                    }
                    self.send(Message::VoteReply(request.candidate, to, received.outcome));
                }
            }
            Message::VoteReply(to, from, response) => {
                let before = self.nodes[to].clone();
                let answer = self.nodes[to].receive_vote(from, response);
                Self::reconciled(tally, &before, &self.nodes[to], response.term, &answer);
                match answer.map(|received| received.outcome) {
                    Ok(Ballot::Duplicate | Ballot::Denied) => {
                        Self::repeat(tally, &before, &self.nodes[to]);
                    }
                    Ok(Ballot::Elected) => tally.elections += 1,
                    Ok(Ballot::Counted | Ballot::Overtaken) | Err(_) => {}
                }
                // Only a higher term moves a server that is not standing.
                if before.role() == Role::Follower && self.nodes[to].term() == before.term() {
                    assert_eq!(self.nodes[to], before, "a follower counted a ballot");
                }
            }
            Message::Append(to, request, entries) => {
                let before = self.nodes[to].clone();
                let answer = self.nodes[to].append_entries(request, &entries);
                Self::reconciled(tally, &before, &self.nodes[to], request.term, &answer);
                if let Ok(received) = answer {
                    let behind = received.outcome.append == Append::TermBehind;
                    assert_eq!(behind, request.term < before.term(), "a stale leader");
                    match received.outcome.append {
                        Append::TermBehind => assert_eq!(self.nodes[to], before),
                        Append::Conflict => assert_eq!(self.nodes[to].last(), before.last()),
                        Append::Matched(_) => {}
                    }
                    self.send(Message::AppendReply(request.leader, to, received.outcome));
                }
            }
            Message::AppendReply(to, from, response) => {
                let before = self.nodes[to].clone();
                let answer = self.nodes[to].acknowledge(from, response);
                Self::reconciled(tally, &before, &self.nodes[to], response.term, &answer);
                match answer.map(|received| received.outcome) {
                    Ok(Ack::Duplicate | Ack::Conflict) => {
                        Self::repeat(tally, &before, &self.nodes[to]);
                    }
                    Ok(Ack::Advanced | Ack::Overtaken) | Err(_) => {}
                }
                // Only a higher term moves a server that is not leading.
                if before.role() != Role::Leader && self.nodes[to].term() == before.term() {
                    assert_eq!(self.nodes[to], before, "a non-leader took an ack");
                }
            }
        }
    }

    /// One leader sends one peer a slice of its log from a position it picks
    /// at random, which stands in for every retry strategy a runtime could use.
    fn replicate(&mut self, from: usize) {
        let leader = &self.nodes[from];
        let to = (from + 1 + self.rng.below(N - 1)) % N;
        let length = leader.last().index;
        let start = self.rng.below(length + 1);
        let count = self.rng.below(length - start + 1);
        let term = leader.entry(start).map_or(0, |entry| entry.term);
        let request = append(
            from,
            leader.term(),
            Position { index: start, term },
            leader.committed(),
        );
        let entries = (start + 1..=start + count)
            .map(|index| leader.entry(index).expect("held"))
            .collect();
        self.send(Message::Append(to, request, entries));
    }

    fn step(&mut self, tally: &mut Tally) {
        let seat = self.rng.below(N);
        match self.rng.below(120) {
            0 => {
                let before = self.nodes[seat].clone();
                let request = self.nodes[seat].begin_election().expect("terms remain");
                self.depose(&before, seat);
                assert_eq!(self.nodes[seat].role(), Role::Candidate);
                self.vote(seat, request.term, seat);
                for peer in (0..N).filter(|peer| *peer != seat) {
                    self.send(Message::Vote(peer, request));
                }
            }
            1 | 2 => {
                let saved = self.nodes[seat].checkpoint();
                self.nodes[seat] = Raft::restored(self.members, seat, saved).expect("checkpoint");
                tally.restores += 1;
            }
            3..=8 if !self.wire.is_empty() => {
                let lost = self.rng.below(self.wire.len());
                self.wire.swap_remove(lost);
            }
            9..=24 => {
                self.command += 1;
                let before = self.nodes[seat].clone();
                if let Err(refusal) = self.nodes[seat].propose(self.command) {
                    assert_eq!(self.nodes[seat], before, "{refusal:?} changed the server");
                    Self::refusal(tally, refusal);
                }
            }
            25..=59 if self.nodes[seat].role() == Role::Leader => self.replicate(seat),
            60..=69 if !self.deposed.is_empty() => self.haunt(tally),
            70..=72 => self.forge(tally),
            _ if !self.wire.is_empty() => {
                let chosen = self.rng.below(self.wire.len());
                let message = if self.rng.below(6) == 0 {
                    self.wire[chosen].clone()
                } else {
                    self.wire.swap_remove(chosen)
                };
                self.deliver(message, tally);
            }
            _ => {}
        }
        self.check(tally);
    }

    fn check(&mut self, tally: &mut Tally) {
        // Election safety: one leader per term, ever.
        for (seat, node) in self.nodes.iter().enumerate() {
            if node.role() != Role::Leader {
                continue;
            }
            match self.leaders.iter().find(|(term, _)| *term == node.term()) {
                Some((term, other)) => assert_eq!(*other, seat, "two leaders in term {term}"),
                None => self.leaders.push((node.term(), seat)),
            }
        }
        // Log matching: the same term at an index means the same log up to it.
        for (seat, node) in self.nodes.iter().enumerate() {
            for other in &self.nodes[seat + 1..] {
                let shared = node.last().index.min(other.last().index);
                let agree = (1..=shared)
                    .rev()
                    .find(|index| term_at(node, *index) == term_at(other, *index))
                    .unwrap_or(0);
                for index in 1..=agree {
                    assert_eq!(node.entry(index), other.entry(index), "log matching");
                }
            }
        }
        // State-machine safety: no two servers commit different entries at an
        // index, and a committed entry is never lost or replaced.
        for node in &self.nodes {
            for index in 1..=node.committed() {
                let entry = node.entry(index).expect("committed entries are held");
                if let Some((chosen, _)) = self.chosen.get(index - 1) {
                    assert_eq!(entry, *chosen, "state-machine safety");
                } else {
                    assert_eq!(self.chosen.len(), index - 1);
                    self.chosen.push((entry, node.term()));
                    tally.committed += 1;
                }
            }
        }
        // Leader completeness: an entry committed in a term is in the log of
        // the leader of every term from that one on.
        for node in self.nodes.iter().filter(|node| node.role() == Role::Leader) {
            for (offset, (entry, term)) in self.chosen.iter().enumerate() {
                if node.term() >= *term {
                    assert_eq!(node.entry(offset + 1), Some(*entry), "leader completeness");
                }
            }
        }
    }
}

fn term_at<const N: usize>(node: &Node<N>, index: usize) -> u64 {
    node.entry(index).map_or(0, |entry| entry.term)
}

fn explore<const N: usize>(seeds: u64, steps: usize, tally: &mut Tally) {
    for seed in 0..seeds {
        let mut cluster = Cluster::<N>::new(seed.wrapping_mul(N as u64 + 1));
        for _ in 0..steps {
            cluster.step(tally);
        }
    }
}

#[test]
fn honest_servers_on_a_hostile_network_keep_every_raft_safety_property() {
    let mut tally = Tally::default();
    explore::<3>(150, 3_000, &mut tally);
    explore::<5>(80, 4_000, &mut tally);
    explore::<2>(30, 2_000, &mut tally);
    // The run went somewhere: leaders were elected and replaced, entries
    // committed, servers restarted, and every honest refusal was met.
    assert!(tally.elections > 1_000, "elections: {}", tally.elections);
    assert!(tally.committed > 1_000, "committed: {}", tally.committed);
    assert!(tally.adoptions > 1_000, "adoptions: {}", tally.adoptions);
    assert!(tally.repeats > 1_000, "repeats: {}", tally.repeats);
    assert!(tally.restores > 1_000, "restores: {}", tally.restores);
    // Deposed leaders kept sending into terms that had left them behind, with
    // entries the cluster had since committed otherwise and with commit
    // indices no server had reached.
    let haunted = [
        tally.haunted,
        tally.haunted_committed,
        tally.haunted_inflated,
    ];
    assert!(haunted.iter().all(|count| *count > 1_000), "{haunted:?}");
    assert!(tally.forged > 1_000, "forged: {}", tally.forged);
    assert_eq!(tally.refusals.len(), 4, "refusals: {:?}", tally.refusals);
}
