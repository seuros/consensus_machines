use super::*;

#[test]
fn replication_commits_on_a_majority_and_reports_every_acknowledgement() {
    let mut leader = leader::<8>();
    let mut follower = node::<8>(1);
    assert_eq!(leader.propose(42), Ok(1));
    assert_eq!(leader.committed(), 0);
    let command = leader.entry(1).expect("entry");
    let request = append(0, 1, EMPTY, 0);
    let stored = follower
        .append_entries(request, &[command])
        .expect("append");
    assert_eq!(stored.adopted, Some(1));
    assert_eq!(stored.role, Role::Follower);
    assert_eq!(stored.outcome, matched(1, 1));
    let ack = leader.acknowledge(1, stored.outcome).expect("ack");
    assert_eq!(
        (ack.adopted, ack.role, ack.outcome),
        (None, Role::Leader, Ack::Advanced)
    );
    assert_eq!(leader.committed(), 1);
    // Honoured repeats: the same match, and a lower one.
    for index in [1, 0] {
        let repeat = unchanged(&mut leader, |leader| {
            leader.acknowledge(1, matched(1, index))
        });
        assert_eq!(repeat.outcome, Ack::Duplicate);
    }
    // The same entries again are already held: matched, nothing changes.
    let replay = unchanged(&mut follower, |follower| {
        follower.append_entries(request, &[command])
    });
    assert_eq!((replay.adopted, replay.outcome), (None, matched(1, 1)));
    let heartbeat = append(0, 1, leader.last(), 1);
    let beat = follower.append_entries(heartbeat, &[]).expect("append");
    assert_eq!(beat.outcome, matched(1, 1));
    assert_eq!(follower.committed(), 1);
}

#[test]
fn a_conflict_is_an_answer_and_a_higher_term_is_adopted_before_it() {
    let mut follower = node::<4>(1);
    let missing = Position { index: 1, term: 1 };
    let conflict = follower
        .append_entries(append(0, 3, missing, 0), &[entry(3, 9)])
        .expect("append");
    assert_eq!(conflict.adopted, Some(3));
    assert_eq!(
        conflict.outcome,
        AppendResponse {
            term: 3,
            append: Append::Conflict
        }
    );
    assert_eq!((follower.term(), follower.last()), (3, EMPTY));
    // The leader is told, records nothing, and retries from an earlier position.
    let mut leader = leader::<4>();
    let reply = AppendResponse {
        term: 1,
        append: Append::Conflict,
    };
    let told = unchanged(&mut leader, |leader| leader.acknowledge(1, reply));
    assert_eq!((told.adopted, told.outcome), (None, Ack::Conflict));
    // A reply from a higher term ends the leadership, and says so.
    let ended = leader.acknowledge(1, conflict.outcome).expect("ack");
    assert_eq!(ended.adopted, Some(3));
    assert_eq!(
        (ended.role, ended.outcome),
        (Role::Follower, Ack::Overtaken)
    );
    assert_eq!(leader.term(), 3);
    // A lower term is answered with the follower's own; nothing changes.
    let behind = unchanged(&mut follower, |follower| {
        follower.append_entries(append(0, 2, EMPTY, 0), &[entry(2, 1)])
    });
    assert_eq!(behind.adopted, None);
    assert_eq!(behind.outcome.term, 3);
    assert_eq!(behind.outcome.append, Append::TermBehind);
}

#[test]
fn a_candidate_yields_to_the_leader_of_its_term_without_adopting() {
    let mut candidate = node::<4>(1);
    let own = candidate.begin_election().expect("term");
    let beat = candidate
        .append_entries(append(0, own.term, EMPTY, 0), &[])
        .expect("append");
    assert_eq!(beat.adopted, None);
    assert_eq!(beat.role, Role::Follower);
    assert_eq!(beat.outcome, matched(1, 0));
    assert_eq!(candidate.checkpoint().voted_for(), Some(1));
    refused(&mut candidate, Refusal::NotCandidate, |candidate| {
        candidate.receive_vote(2, ballot(1, Vote::Granted))
    });
}

#[test]
fn conflicting_uncommitted_suffix_is_replaced_but_committed_prefix_is_immutable() {
    let mut follower = node::<4>(1);
    let request = append(0, 1, EMPTY, 1);
    let stored = follower
        .append_entries(request, &[entry(1, 1), entry(1, 2)])
        .expect("append");
    assert_eq!(stored.outcome.append, Append::Matched(2));
    assert_eq!(follower.committed(), 1);
    let replacement = append(2, 2, Position { index: 1, term: 1 }, 1);
    let replaced = follower
        .append_entries(replacement, &[entry(2, 3)])
        .expect("append");
    assert_eq!(replaced.adopted, Some(2));
    assert_eq!(replaced.outcome.append, Append::Matched(2));
    assert_eq!(follower.entry(2), Some(entry(2, 3)));
    refused(&mut follower, Refusal::CommittedConflict, |follower| {
        follower.append_entries(append(2, 2, EMPTY, 1), &[entry(2, 99)])
    });
    // Not even a higher term replaces a committed entry, and it is not adopted.
    refused(&mut follower, Refusal::CommittedConflict, |follower| {
        follower.append_entries(append(2, 9, EMPTY, 1), &[entry(9, 99)])
    });
    let wrong = append(2, 9, Position { index: 1, term: 9 }, 1);
    let conflict = follower.append_entries(wrong, &[]).expect("append");
    assert_eq!(conflict.adopted, Some(9));
    assert_eq!(conflict.outcome.append, Append::Conflict);
    assert_eq!(follower.entry(1), Some(entry(1, 1)));
}

#[test]
fn old_term_entries_need_a_current_term_entry_before_majority_commit() {
    let mut node = leader::<8>();
    assert_eq!(node.propose(7), Ok(1)); // Term 1, not yet committed.
    let request = node.begin_election().expect("next term");
    let won = node
        .receive_vote(1, ballot(request.term, Vote::Granted))
        .expect("ballot");
    assert_eq!(won.outcome, Ballot::Elected);
    let first = node.acknowledge(1, matched(2, 1)).expect("ack");
    assert_eq!(first.outcome, Ack::Advanced);
    assert_eq!(node.committed(), 0);
    assert_eq!(node.propose(8), Ok(2));
    let second = node.acknowledge(1, matched(2, 2)).expect("ack");
    assert_eq!(second.outcome, Ack::Advanced);
    assert_eq!(node.committed(), 2);
}

#[test]
fn empty_heartbeats_do_not_truncate_suffix_or_commit_unmatched_tail() {
    let mut follower = node::<4>(1);
    let request = append(0, 1, EMPTY, 0);
    let stored = follower
        .append_entries(request, &[entry(1, 1), entry(1, 2)])
        .expect("append");
    assert_eq!(stored.outcome.append, Append::Matched(2));
    let beat = unchanged(&mut follower, |follower| {
        follower.append_entries(append(0, 1, EMPTY, 2), &[])
    });
    assert_eq!(beat.outcome.append, Append::Matched(0));
    assert_eq!(follower.last().index, 2);
    assert_eq!(follower.committed(), 0);
}

#[test]
fn a_leader_never_yields_its_log_to_a_rival_of_its_own_term() {
    let mut leader = leader::<4>();
    assert_eq!(leader.propose(5), Ok(1));
    refused(&mut leader, Refusal::RivalLeader, |leader| {
        leader.append_entries(append(1, 1, EMPTY, 0), &[entry(1, 6)])
    });
    refused(&mut leader, Refusal::RivalLeader, |leader| {
        leader.append_entries(append(1, 1, EMPTY, 0), &[])
    });
    assert_eq!(leader.entry(1), Some(entry(1, 5)));
    assert_eq!(leader.role(), Role::Leader);
}
