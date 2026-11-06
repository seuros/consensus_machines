#![allow(clippy::expect_used)]
use super::*;
const MEMBERS: [u64; 3] = [10, 20, 30];
const EMPTY: Position = Position { index: 0, term: 0 };

fn leader() -> Raft<3, 8> {
    let mut node = Raft::new(MEMBERS, 0).expect("membership");
    let request = node.begin_election().expect("term");
    assert_eq!(
        node.receive_vote(
            1,
            VoteResponse {
                term: request.term,
                vote: Vote::Granted
            }
        ),
        Role::Leader
    );
    node
}
#[test]
fn elections_count_distinct_members_and_step_down_on_higher_terms() {
    assert!(Raft::<0, 1>::new([], 0).is_none());
    assert!(Raft::<2, 1>::new([1, 1], 0).is_none());
    let mut node = Raft::<5, 1>::new([1, 2, 3, 4, 5], 0).expect("membership");
    let request = node.begin_election().expect("election");
    for _ in 0..10 {
        assert_eq!(
            node.receive_vote(
                1,
                VoteResponse {
                    term: request.term,
                    vote: Vote::Granted
                }
            ),
            Role::Candidate
        );
    }
    assert_eq!(
        node.receive_vote(
            9,
            VoteResponse {
                term: 99,
                vote: Vote::Granted
            }
        ),
        Role::Candidate
    );
    assert_eq!(
        node.receive_vote(
            2,
            VoteResponse {
                term: request.term,
                vote: Vote::Granted
            }
        ),
        Role::Leader
    );
    assert_eq!(
        node.receive_vote(
            3,
            VoteResponse {
                term: 2,
                vote: Vote::Denied
            }
        ),
        Role::Follower
    );
    assert_eq!(node.propose(1), None);
}
#[test]
fn persistent_votes_prevent_a_second_vote_after_restart() {
    let mut node = Raft::<3, 2>::new(MEMBERS, 2).expect("membership");
    let vote = VoteRequest {
        candidate: 0,
        term: 1,
        last: EMPTY,
    };
    assert_eq!(node.request_vote(vote).vote, Vote::Granted);
    let mut restored = Raft::restore(node.checkpoint()).expect("checkpoint");
    assert_eq!(restored.role(), Role::Follower);
    assert_eq!(
        restored
            .request_vote(VoteRequest {
                candidate: 1,
                ..vote
            })
            .vote,
        Vote::Denied
    );
    assert_eq!(restored.request_vote(vote).vote, Vote::Granted);
}
#[test]
fn log_freshness_and_replication_commit_require_valid_majorities() {
    let mut leader = leader();
    let mut follower = Raft::<3, 8>::new(MEMBERS, 1).expect("membership");
    assert_eq!(leader.propose(42), Some(1));
    assert_eq!(leader.committed(), 0);
    let entry = leader.entry(1).expect("entry");
    let request = AppendRequest {
        leader: 0,
        term: 1,
        previous: EMPTY,
        committed: 0,
    };
    assert_eq!(follower.append_entries(request, &[entry]).matched, Some(1));
    assert!(leader.acknowledge(1, 1, 1));
    assert_eq!(leader.committed(), 1);
    assert_eq!(
        follower
            .append_entries(
                AppendRequest {
                    previous: leader.last(),
                    committed: 1,
                    ..request
                },
                &[]
            )
            .matched,
        Some(1)
    );
    assert_eq!(follower.committed(), 1);
    assert_eq!(
        follower
            .request_vote(VoteRequest {
                candidate: 2,
                term: 2,
                last: EMPTY
            })
            .vote,
        Vote::Denied
    );
    assert_eq!(follower.term(), 2);
    assert_eq!(leader.committed(), 1);
}
#[test]
fn conflicting_uncommitted_suffix_is_replaced_but_committed_prefix_is_immutable() {
    let mut follower = Raft::<3, 4>::new(MEMBERS, 1).expect("membership");
    let request = AppendRequest {
        leader: 0,
        term: 1,
        previous: EMPTY,
        committed: 1,
    };
    assert_eq!(
        follower
            .append_entries(
                request,
                &[
                    Entry {
                        term: 1,
                        command: 1
                    },
                    Entry {
                        term: 1,
                        command: 2
                    }
                ]
            )
            .matched,
        Some(2)
    );
    assert_eq!(follower.committed(), 1);
    let replacement = AppendRequest {
        leader: 2,
        term: 2,
        previous: Position { index: 1, term: 1 },
        committed: 1,
    };
    assert_eq!(
        follower
            .append_entries(
                replacement,
                &[Entry {
                    term: 2,
                    command: 3
                }]
            )
            .matched,
        Some(2)
    );
    assert_eq!(
        follower.entry(2),
        Some(Entry {
            term: 2,
            command: 3
        })
    );
    let before = follower.checkpoint();
    assert_eq!(
        follower
            .append_entries(
                AppendRequest {
                    previous: EMPTY,
                    ..replacement
                },
                &[Entry {
                    term: 2,
                    command: 99
                }]
            )
            .matched,
        None
    );
    assert_eq!(follower.checkpoint(), before);
    assert_eq!(
        follower
            .append_entries(
                AppendRequest {
                    previous: Position { index: 1, term: 9 },
                    ..replacement
                },
                &[]
            )
            .matched,
        None
    );
}
#[test]
fn old_term_entries_need_a_current_term_entry_before_majority_commit() {
    let mut node = leader();
    assert_eq!(node.propose(7), Some(1)); // Term 1, not yet committed.
    let request = node.begin_election().expect("next term");
    assert_eq!(
        node.receive_vote(
            1,
            VoteResponse {
                term: request.term,
                vote: Vote::Granted
            }
        ),
        Role::Leader
    );
    assert!(node.acknowledge(1, 2, 1));
    assert_eq!(node.committed(), 0);
    assert_eq!(node.propose(8), Some(2));
    assert!(node.acknowledge(1, 2, 2));
    assert_eq!(node.committed(), 2);
}
#[test]
fn bounded_single_member_log_and_term_exhaustion_fail_closed() {
    let mut node = Raft::<1, 1>::new([1], 0).expect("membership");
    assert!(node.begin_election().is_some());
    assert_eq!(node.role(), Role::Leader);
    assert_eq!(node.propose(1), Some(1));
    assert_eq!(node.committed(), 1);
    assert_eq!(node.propose(2), None);
    node.state.term = u64::MAX;
    assert_eq!(node.begin_election(), None);
}
#[test]
fn empty_heartbeats_do_not_truncate_suffix_or_commit_unmatched_tail() {
    let mut follower = Raft::<3, 4>::new(MEMBERS, 1).expect("membership");
    let request = AppendRequest {
        leader: 0,
        term: 1,
        previous: EMPTY,
        committed: 0,
    };
    assert_eq!(
        follower
            .append_entries(
                request,
                &[
                    Entry {
                        term: 1,
                        command: 1
                    },
                    Entry {
                        term: 1,
                        command: 2
                    }
                ]
            )
            .matched,
        Some(2)
    );
    assert_eq!(
        follower
            .append_entries(
                AppendRequest {
                    committed: 2,
                    ..request
                },
                &[]
            )
            .matched,
        Some(0)
    );
    assert_eq!(follower.last().index, 2);
    assert_eq!(follower.committed(), 0);
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
                let mut nodes: [Raft<3, 1>; 3] =
                    core::array::from_fn(|own| Raft::new(MEMBERS, own).expect("membership"));
                let mut granted = [[false; 3]; 3];
                for (voter, order) in [first, second, third].into_iter().enumerate() {
                    for candidate in order {
                        let response = nodes[voter].request_vote(VoteRequest {
                            candidate,
                            term: 1,
                            last: EMPTY,
                        });
                        if response.vote == Vote::Granted {
                            granted[candidate][voter] = true;
                        }
                        nodes[voter] =
                            Raft::restore(nodes[voter].checkpoint()).expect("durable vote");
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

#[test]
fn oversized_append_rejects_atomically_and_invalid_acks_cannot_commit() {
    let mut follower = Raft::<3, 1>::new(MEMBERS, 1).expect("membership");
    let request = AppendRequest {
        leader: 0,
        term: 1,
        previous: EMPTY,
        committed: 0,
    };
    assert_eq!(
        follower
            .append_entries(
                request,
                &[Entry {
                    term: 1,
                    command: 7
                }]
            )
            .matched,
        Some(1)
    );
    let before = follower.checkpoint();
    assert_eq!(
        follower
            .append_entries(
                request,
                &[
                    Entry {
                        term: 1,
                        command: 7
                    },
                    Entry {
                        term: 1,
                        command: 8
                    },
                ]
            )
            .matched,
        None
    );
    assert_eq!(follower.checkpoint(), before);
    let mut leader = leader();
    assert_eq!(leader.propose(7), Some(1));
    assert!(!leader.acknowledge(1, 0, 1));
    assert!(!leader.acknowledge(1, 1, 2));
    assert!(!leader.acknowledge(9, 1, 1));
    assert_eq!(leader.committed(), 0);
}
