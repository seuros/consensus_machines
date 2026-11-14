//! Every refusal of every method, each proved against a clone. A refused
//! message never adopts the higher term it carries.
use super::*;

const HIGH: u64 = 9;

#[test]
fn request_vote_refusals_change_nothing_and_adopt_no_term() {
    let mut voter = node::<4>(1);
    let cases = [
        (Refusal::UnknownMember, ask(3, HIGH, EMPTY)),
        (Refusal::OwnSeat, ask(1, HIGH, EMPTY)),
        (Refusal::Malformed, ask(0, 0, EMPTY)),
        (Refusal::Malformed, ask(0, HIGH, at(0, 1))),
        (Refusal::Malformed, ask(0, HIGH, at(1, 0))),
        (Refusal::Malformed, ask(0, HIGH, at(1, HIGH + 1))),
    ];
    for (why, request) in cases {
        refuses(&mut voter, why, |voter| voter.request_vote(request));
    }
    assert_eq!(voter.term(), 0);
}

#[test]
fn receive_vote_refusals_change_nothing_and_adopt_no_term() {
    let grant = |term| ballot(term, Vote::Granted);
    let mut candidate = node::<4>(0);
    candidate.begin_election().expect("term");
    candidate.begin_election().expect("term");
    refuses(&mut candidate, Refusal::UnknownMember, |node| {
        node.receive_vote(3, grant(HIGH))
    });
    refuses(&mut candidate, Refusal::OwnSeat, |node| {
        node.receive_vote(0, grant(HIGH))
    });
    refuses(&mut candidate, Refusal::Stale, |node| {
        node.receive_vote(1, grant(1))
    });
    // The voter answered an older request from the term the candidate is in now.
    refuses(&mut candidate, Refusal::Stale, |node| {
        node.receive_vote(1, ballot(2, Vote::Denied(Denial::TermBehind)))
    });
    assert_eq!((candidate.term(), candidate.role()), (2, Role::Candidate));
    let mut follower = node::<4>(1);
    let beat = follower.append_entries(append(0, 2, EMPTY, 0), &[]);
    assert_eq!(beat.expect("append").adopted, Some(2));
    refuses(&mut follower, Refusal::NotCandidate, |node| {
        node.receive_vote(0, grant(2))
    });
}

#[test]
fn propose_refusals_change_nothing() {
    let mut follower = node::<1>(0);
    refuses(&mut follower, Refusal::NotLeader, |node| node.propose(1));
    follower.begin_election().expect("term");
    refuses(&mut follower, Refusal::NotLeader, |node| node.propose(1));
    let mut leader = leader::<1>();
    assert_eq!((Raft::<3, 1>::MEMBERS, Raft::<3, 1>::CAPACITY), (3, 1));
    assert_eq!(leader.propose(1), Ok(1));
    refuses(&mut leader, Refusal::LogFull, |node| node.propose(2));
}

#[test]
fn append_entries_refusals_change_nothing_and_adopt_no_term() {
    let mut follower = node::<2>(1);
    let first = at(1, 1);
    let stored = follower
        .append_entries(append(0, 1, EMPTY, 1), &[entry(1, 7)])
        .expect("append");
    assert_eq!(stored.outcome.append, Append::Matched(1));
    let cases: [(Refusal, AppendRequest, &[Entry]); 12] = [
        (Refusal::UnknownMember, append(3, HIGH, EMPTY, 0), &[]),
        (Refusal::OwnSeat, append(1, HIGH, EMPTY, 0), &[]),
        (Refusal::Malformed, append(0, 0, EMPTY, 0), &[]),
        (Refusal::Malformed, append(0, HIGH, at(0, 1), 0), &[]),
        (Refusal::Malformed, append(0, HIGH, at(1, 0), 0), &[]),
        (Refusal::Malformed, append(0, HIGH, at(1, HIGH + 1), 0), &[]),
        (
            Refusal::Malformed,
            append(0, HIGH, EMPTY, 0),
            &[entry(0, 1)],
        ),
        (
            Refusal::Malformed,
            append(0, HIGH, EMPTY, 0),
            &[entry(HIGH + 1, 1)],
        ),
        (
            Refusal::Malformed,
            append(0, HIGH, EMPTY, 0),
            &[entry(2, 1), entry(1, 2)],
        ),
        (
            Refusal::TooManyEntries,
            append(0, HIGH, first, 0),
            &[entry(HIGH, 1), entry(HIGH, 2)],
        ),
        (Refusal::Mismatch, append(0, HIGH, EMPTY, 0), &[entry(1, 8)]),
        (
            Refusal::CommittedConflict,
            append(0, HIGH, EMPTY, 0),
            &[entry(HIGH, 7)],
        ),
    ];
    for (why, request, entries) in cases {
        refuses(&mut follower, why, |node| {
            node.append_entries(request, entries)
        });
    }
    assert_eq!(follower.term(), 1);
    refuses(&mut follower, Refusal::TooManyEntries, |node| {
        let far = at(usize::MAX, 1);
        node.append_entries(append(0, 1, far, 0), &[entry(1, 1)])
    });
    let mut leader = leader::<2>();
    refuses(&mut leader, Refusal::RivalLeader, |node| {
        node.append_entries(append(1, 1, EMPTY, 0), &[])
    });
}

#[test]
fn acknowledge_refusals_change_nothing_and_adopt_no_term() {
    let mut leader = leader::<4>();
    assert_eq!(leader.propose(7), Ok(1));
    leader.begin_election().expect("term");
    let won = leader.receive_vote(1, ballot(2, Vote::Granted));
    assert_eq!(won.expect("ballot").outcome, Ballot::Elected);
    let behind = AppendResponse {
        term: 2,
        append: Append::TermBehind,
    };
    let cases = [
        (Refusal::UnknownMember, 3, matched(HIGH, 1)),
        (Refusal::OwnSeat, 0, matched(HIGH, 1)),
        (Refusal::Stale, 1, matched(1, 1)),
        (Refusal::Stale, 1, behind),
        (Refusal::BeyondLog, 1, matched(2, 2)),
    ];
    for (why, peer, response) in cases {
        refuses(&mut leader, why, |node| node.acknowledge(peer, response));
    }
    assert_eq!((leader.term(), leader.committed()), (2, 0));
    let mut follower = node::<4>(1);
    follower.begin_election().expect("term");
    refuses(&mut follower, Refusal::NotLeader, |node| {
        node.acknowledge(0, matched(1, 0))
    });
}

/// Regression: an inbound reply its own constructor rejects (term zero, or a
/// "term behind" from a term no request could be behind) was folded in, and a
/// higher impossible term was adopted. It is `Malformed`, before any term
/// reconciliation, whatever the receiver's role and term.
#[test]
fn impossible_replies_are_malformed_and_adopt_no_term() {
    let votes = [
        ballot(0, Vote::Granted),
        ballot(0, Vote::Denied(Denial::AlreadyVoted)),
        ballot(1, Vote::Denied(Denial::TermBehind)),
    ];
    let appends = [
        matched(0, 0),
        AppendResponse {
            term: 0,
            append: Append::Conflict,
        },
        AppendResponse {
            term: 1,
            append: Append::TermBehind,
        },
    ];
    let mut candidate = node::<4>(0);
    candidate.begin_election().expect("term");
    for mut server in [node::<4>(0), candidate, leader::<4>()] {
        for response in votes {
            refuses(&mut server, Refusal::Malformed, |node| {
                node.receive_vote(1, response)
            });
        }
        for response in appends {
            refuses(&mut server, Refusal::Malformed, |node| {
                node.acknowledge(1, response)
            });
        }
    }
}

#[test]
fn refusals_read_as_sentences() {
    use std::string::ToString;
    let all = [
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
        Refusal::TermExhausted,
    ];
    for (index, refusal) in all.iter().enumerate() {
        let text = refusal.to_string();
        assert_ne!(text, "");
        assert!(all[..index].iter().all(|other| other.to_string() != text));
        let error: &dyn core::error::Error = refusal;
        assert!(error.source().is_none());
    }
}
