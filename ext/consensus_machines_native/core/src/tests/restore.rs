use super::*;

#[test]
fn a_checkpoint_is_a_plain_value_that_round_trips_through_its_readers() {
    let mut follower = node::<4>(1);
    follower
        .append_entries(append(0, 3, EMPTY, 1), &[entry(1, 5), entry(3, 6)])
        .expect("append");
    let request = VoteRequest {
        candidate: 2,
        term: 4,
        last: Position { index: 2, term: 3 },
    };
    follower.request_vote(request).expect("vote");
    let saved = follower.checkpoint();
    assert_eq!((saved.members(), saved.own()), (MEMBERS, 1));
    assert_eq!((saved.term(), saved.voted_for()), (4, Some(2)));
    assert_eq!(saved.committed(), 1);
    assert_eq!(saved.last(), Position { index: 2, term: 3 });
    let entries = [1, 2].map(|index| saved.entry(index).expect("entry"));
    assert_eq!(saved.entry(3), None);
    let rebuilt = Checkpoint::<3, 4>::new(
        saved.members(),
        saved.own(),
        saved.term(),
        saved.voted_for(),
        saved.committed(),
        &entries,
    )
    .expect("state");
    assert_eq!(rebuilt, saved);
    let restored = Raft::restored(MEMBERS, 1, rebuilt).expect("checkpoint");
    assert_eq!(restored, follower);
}

#[test]
fn restore_loses_only_the_volatile_role() {
    let mut leader = leader::<4>();
    assert_eq!(leader.propose(9), Ok(1));
    let restored = Raft::restored(MEMBERS, 0, leader.checkpoint()).expect("checkpoint");
    assert_eq!(restored.role(), Role::Follower);
    assert_eq!(restored.checkpoint(), leader.checkpoint());
    let mut restored = restored;
    refused(&mut restored, Refusal::NotLeader, |node| node.propose(1));
    refused(&mut restored, Refusal::NotLeader, |node| {
        node.acknowledge(1, matched(1, 1))
    });
}

#[test]
fn restore_refuses_a_checkpoint_of_another_membership_or_seat() {
    let saved = node::<4>(1).checkpoint();
    assert!(Raft::restored(MEMBERS, 1, saved.clone()).is_some());
    assert!(Raft::restored(MEMBERS, 0, saved.clone()).is_none());
    assert!(Raft::restored(MEMBERS, 3, saved.clone()).is_none());
    assert!(Raft::restored([10, 20, 31], 1, saved.clone()).is_none());
    assert!(Raft::restored([20, 10, 30], 1, saved).is_none());
}

#[test]
fn checkpoint_new_refuses_a_state_no_server_can_be_in() {
    let build = |members, own, term, voted_for, committed, entries: &[Entry]| {
        Checkpoint::<3, 2>::new(members, own, term, voted_for, committed, entries)
    };
    let log = [entry(1, 1), entry(2, 2)];
    assert!(build(MEMBERS, 0, 2, Some(2), 2, &log).is_some());
    assert!(build(MEMBERS, 0, 0, None, 0, &[]).is_some());
    assert!(build([10, 10, 30], 0, 2, None, 0, &[]).is_none());
    assert!(build(MEMBERS, 3, 2, None, 0, &[]).is_none());
    assert!(build(MEMBERS, 0, 2, Some(3), 0, &[]).is_none());
    assert!(build(MEMBERS, 0, 2, None, 3, &log).is_none());
    assert!(build(MEMBERS, 0, 1, None, 0, &log).is_none());
    assert!(build(MEMBERS, 0, 2, None, 0, &[entry(0, 1)]).is_none());
    assert!(build(MEMBERS, 0, 2, None, 0, &[entry(2, 1), entry(1, 2)]).is_none());
    let overlong = [entry(1, 1), entry(1, 2), entry(1, 3)];
    assert!(build(MEMBERS, 0, 2, None, 0, &overlong).is_none());
    assert!(Checkpoint::<0, 2>::new([], 0, 0, None, 0, &[]).is_none());
}
