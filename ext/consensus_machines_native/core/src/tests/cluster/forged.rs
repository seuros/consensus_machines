//! Replies no server can send: a corrupted or forged frame that still
//! authenticates. Each one is refused `Malformed` before its term is looked
//! at, so it never moves the receiver into the term it carries.
use super::*;

/// Every reply a constructor rejects: a zero term, or a "term behind" from
/// term one, which no request can be behind.
const FORGED: [Forged; 6] = [
    Forged::Vote(VoteResponse {
        term: 0,
        vote: Vote::Granted,
    }),
    Forged::Vote(VoteResponse {
        term: 0,
        vote: Vote::Denied(Denial::LogBehind),
    }),
    Forged::Vote(VoteResponse {
        term: 1,
        vote: Vote::Denied(Denial::TermBehind),
    }),
    Forged::Append(AppendResponse {
        term: 0,
        append: Append::Matched(0),
    }),
    Forged::Append(AppendResponse {
        term: 0,
        append: Append::Conflict,
    }),
    Forged::Append(AppendResponse {
        term: 1,
        append: Append::TermBehind,
    }),
];

#[derive(Clone, Copy)]
enum Forged {
    Vote(VoteResponse),
    Append(AppendResponse),
}

impl<const N: usize> Cluster<N> {
    /// Hand one server a forged reply from a real peer, whatever its role.
    pub(super) fn forge(&mut self, tally: &mut Tally) {
        let seat = self.rng.below(N);
        let from = (seat + 1 + self.rng.below(N - 1)) % N;
        let node = &mut self.nodes[seat];
        let before = node.clone();
        let refusal = match FORGED[self.rng.below(FORGED.len())] {
            Forged::Vote(response) => node.receive_vote(from, response).err(),
            Forged::Append(response) => node.acknowledge(from, response).err(),
        };
        assert_eq!(refusal, Some(Refusal::Malformed));
        assert_eq!(*node, before, "a forged reply changed the server");
        tally.forged += 1;
    }
}
