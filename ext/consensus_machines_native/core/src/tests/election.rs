use super::*;

#[test]
fn elections_count_distinct_members_and_report_every_ballot() {
    assert!(Raft::<0, 1>::new([], 0).is_none());
    assert!(Raft::<2, 1>::new([1, 1], 0).is_none());
    assert!(Raft::<2, 1>::new([1, 2], 2).is_none());
    let mut node = Raft::<5, 1>::new([1, 2, 3, 4, 5], 0).expect("membership");
    assert_eq!((node.members(), node.own()), ([1, 2, 3, 4, 5], 0));
    let request = node.begin_election().expect("election");
    assert_eq!(
        (request.candidate, request.term, request.last),
        (0, 1, EMPTY)
    );
    let grant = ballot(1, Vote::Granted);
    let first = node.receive_vote(1, grant).expect("ballot");
    assert_eq!(
        (first.adopted, first.role, first.outcome),
        (None, Role::Candidate, Ballot::Counted)
    );
    for vote in [Vote::Granted, Vote::Duplicate] {
        let repeat = unchanged(&mut node, |node| node.receive_vote(1, ballot(1, vote)));
        assert_eq!(repeat.outcome, Ballot::Duplicate);
    }
    let denied = unchanged(&mut node, |node| {
        node.receive_vote(3, ballot(1, Vote::Denied(Denial::AlreadyVoted)))
    });
    assert_eq!(denied.outcome, Ballot::Denied);
    // A voter that re-sent its grant answers `Duplicate`; it counts like a grant.
    let won = node
        .receive_vote(2, ballot(1, Vote::Duplicate))
        .expect("ballot");
    assert_eq!((won.role, won.outcome), (Role::Leader, Ballot::Elected));
    // A late grant of the term already won is counted, and elects nobody twice.
    let late = node.receive_vote(4, grant).expect("ballot");
    assert_eq!((late.role, late.outcome), (Role::Leader, Ballot::Counted));
    let again = unchanged(&mut node, |node| node.receive_vote(4, grant));
    assert_eq!(again.outcome, Ballot::Duplicate);
}

#[test]
fn a_response_from_a_higher_term_is_adopted_and_reported() {
    let mut node = leader::<2>();
    let reply = ballot(4, Vote::Denied(Denial::TermBehind));
    let overtaken = node.receive_vote(2, reply).expect("ballot");
    assert_eq!(overtaken.adopted, Some(4));
    assert_eq!(overtaken.role, Role::Follower);
    assert_eq!(overtaken.outcome, Ballot::Overtaken);
    assert_eq!((node.term(), node.role()), (4, Role::Follower));
    assert_eq!(node.checkpoint().voted_for(), None);
    refused(&mut node, Refusal::NotLeader, |node| node.propose(1));
    // The same response again is now of the current term: nothing to adopt.
    refused(&mut node, Refusal::NotCandidate, |node| {
        node.receive_vote(2, reply)
    });
}

#[test]
fn request_vote_reconciles_the_term_then_answers_the_ballot() {
    let mut voter = node::<4>(1);
    let ask = |candidate, term, last| VoteRequest {
        candidate,
        term,
        last,
    };
    // Adopted and granted.
    let granted = voter.request_vote(ask(0, 2, EMPTY)).expect("vote");
    assert_eq!(granted.adopted, Some(2));
    assert_eq!(granted.role, Role::Follower);
    assert_eq!(granted.outcome, ballot(2, Vote::Granted));
    assert_eq!(voter.checkpoint().voted_for(), Some(0));
    // Honoured repeat: the same candidate, the same term, nothing changes.
    let repeat = unchanged(&mut voter, |voter| voter.request_vote(ask(0, 2, EMPTY)));
    assert_eq!(repeat.adopted, None);
    assert_eq!(repeat.outcome, ballot(2, Vote::Duplicate));
    // Kept term, vote already given away.
    let taken = unchanged(&mut voter, |voter| voter.request_vote(ask(2, 2, EMPTY)));
    assert_eq!(taken.adopted, None);
    assert_eq!(taken.outcome, ballot(2, Vote::Denied(Denial::AlreadyVoted)));
    // Lower term: answered with the voter's own, nothing changes.
    let behind = unchanged(&mut voter, |voter| voter.request_vote(ask(2, 1, EMPTY)));
    assert_eq!(behind.adopted, None);
    assert_eq!(behind.outcome, ballot(2, Vote::Denied(Denial::TermBehind)));
    // Adopted and still denied: the term moves, the vote stays free.
    let heartbeat = append(0, 2, EMPTY, 0);
    let stored = voter
        .append_entries(heartbeat, &[entry(2, 7)])
        .expect("append");
    assert_eq!(stored.outcome.append, Append::Matched(1));
    let stale_log = voter.request_vote(ask(2, 5, EMPTY)).expect("vote");
    assert_eq!(stale_log.adopted, Some(5));
    assert_eq!(
        stale_log.outcome,
        ballot(5, Vote::Denied(Denial::LogBehind))
    );
    assert_eq!(voter.term(), 5);
    assert_eq!(voter.checkpoint().voted_for(), None);
    // The freed vote goes to a candidate whose log is as up to date.
    let last = Position { index: 1, term: 2 };
    let fresh = voter.request_vote(ask(0, 5, last)).expect("vote");
    assert_eq!(fresh.adopted, None);
    assert_eq!(fresh.outcome, ballot(5, Vote::Granted));
}

#[test]
fn a_candidate_adopting_a_higher_term_loses_its_candidacy() {
    let mut candidate = node::<2>(0);
    candidate.begin_election().expect("term");
    let request = VoteRequest {
        candidate: 1,
        term: 3,
        last: EMPTY,
    };
    let answer = candidate.request_vote(request).expect("vote");
    assert_eq!(answer.adopted, Some(3));
    assert_eq!(answer.role, Role::Follower);
    assert_eq!(answer.outcome.vote, Vote::Granted);
    // Its own candidacy of the same term denies a rival without adopting.
    let mut rival = node::<2>(0);
    let own = rival.begin_election().expect("term");
    let same = VoteRequest {
        term: own.term,
        ..request
    };
    let denied = unchanged(&mut rival, |rival| rival.request_vote(same));
    assert_eq!(denied.adopted, None);
    assert_eq!(denied.role, Role::Candidate);
    assert_eq!(denied.outcome.vote, Vote::Denied(Denial::AlreadyVoted));
}

#[test]
fn persistent_votes_prevent_a_second_vote_after_restart() {
    let mut node = node::<2>(2);
    let vote = VoteRequest {
        candidate: 0,
        term: 1,
        last: EMPTY,
    };
    assert_eq!(
        node.request_vote(vote).expect("vote").outcome.vote,
        Vote::Granted
    );
    let mut restored = Raft::restored(MEMBERS, 2, node.checkpoint()).expect("checkpoint");
    assert_eq!(restored.role(), Role::Follower);
    let other = VoteRequest {
        candidate: 1,
        ..vote
    };
    assert_eq!(
        restored.request_vote(other).expect("vote").outcome.vote,
        Vote::Denied(Denial::AlreadyVoted)
    );
    assert_eq!(
        restored.request_vote(vote).expect("vote").outcome.vote,
        Vote::Duplicate
    );
}

#[test]
fn a_single_member_leads_at_once_and_the_last_term_is_refused() {
    let mut node = Raft::<1, 1>::new([1], 0).expect("membership");
    node.begin_election().expect("term");
    assert_eq!(node.role(), Role::Leader);
    assert_eq!(node.propose(1), Ok(1));
    assert_eq!(node.committed(), 1);
    refused(&mut node, Refusal::LogFull, |node| node.propose(2));
    let last = Checkpoint::<1, 1>::new([1], 0, u64::MAX, None, 0, &[]).expect("state");
    let mut node = Raft::restored([1], 0, last).expect("checkpoint");
    refused(&mut node, Refusal::TermExhausted, Raft::begin_election);
}

#[test]
fn all_three_voter_request_orders_preserve_one_majority_per_term_across_restarts() {
    let permutations = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    for first in permutations {
        for second in permutations {
            for third in permutations {
                let mut nodes: [Raft<3, 1>; 3] = core::array::from_fn(node);
                let mut granted = [[false; 3]; 3];
                for (voter, order) in [first, second, third].into_iter().enumerate() {
                    for candidate in order {
                        let request = VoteRequest {
                            candidate,
                            term: 1,
                            last: EMPTY,
                        };
                        match nodes[voter].request_vote(request) {
                            Ok(answer) => match answer.outcome.vote {
                                Vote::Granted | Vote::Duplicate => granted[candidate][voter] = true,
                                Vote::Denied(_) => {}
                            },
                            Err(refusal) => assert_eq!(refusal, Refusal::OwnSeat),
                        }
                        nodes[voter] = Raft::restored(MEMBERS, voter, nodes[voter].checkpoint())
                            .expect("durable vote");
                    }
                }
                let majorities = granted
                    .iter()
                    .filter(|votes| votes.iter().filter(|vote| **vote).count() >= 2)
                    .count();
                assert!(majorities <= 1);
            }
        }
    }
}
