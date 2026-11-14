//! The validating constructors: what a value may be, whoever builds it.
use super::*;

#[test]
fn constructors_accept_exactly_the_values_a_server_can_send() {
    let last = at(2, 3);
    for (index, term, valid) in [(0, 0, true), (3, 2, true), (0, 1, false), (1, 0, false)] {
        assert_eq!(Position::new(index, term), valid.then_some(at(index, term)));
    }
    assert_eq!(Entry::new(1, 0), Some(entry(1, 0)));
    assert_eq!(Entry::new(0, 7), None);
    for (term, position, valid) in [(3, last, true), (0, EMPTY, false), (2, last, false)] {
        let vote = valid.then_some(ask(1, term, position));
        assert_eq!(VoteRequest::new(1, term, position), vote);
        let append = valid.then_some(append(0, term, position, 9));
        assert_eq!(AppendRequest::new(0, term, position, 9), append);
    }
    assert_eq!(VoteRequest::new(1, 3, at(2, 0)), None);
    assert_eq!(AppendRequest::new(0, 3, at(0, 1), 0), None);
    // A "term behind" reply needs a term above some request's, so above one.
    let behind = Vote::Denied(Denial::TermBehind);
    for (term, vote, valid) in [
        (1, Vote::Granted, true),
        (0, Vote::Granted, false),
        (1, behind, false),
        (2, behind, true),
        (1, Vote::Denied(Denial::LogBehind), true),
    ] {
        assert_eq!(
            VoteResponse::new(term, vote),
            valid.then_some(ballot(term, vote))
        );
    }
    for (term, append, valid) in [
        (1, Append::Matched(0), true),
        (0, Append::Conflict, false),
        (1, Append::TermBehind, false),
        (2, Append::TermBehind, true),
    ] {
        let response = AppendResponse { term, append };
        assert_eq!(AppendResponse::new(term, append), valid.then_some(response));
    }
}

#[test]
fn received_holds_only_answers_a_server_can_give() {
    let granted = ballot(4, Vote::Granted);
    assert!(Received::new(None, Role::Follower, granted).is_some());
    assert!(Received::new(Some(4), Role::Follower, granted).is_some());
    // An adopted term is the reply's, never zero, and leaves a follower.
    assert!(Received::new(Some(3), Role::Follower, granted).is_none());
    assert!(Received::new(Some(0), Role::Follower, Ballot::Overtaken).is_none());
    assert!(Received::new(Some(4), Role::Leader, Ballot::Overtaken).is_none());
    assert!(Received::new(None, Role::Candidate, granted).is_none());
    let behind = ballot(4, Vote::Denied(Denial::TermBehind));
    assert!(Received::new(None, Role::Leader, behind).is_some());
    assert!(Received::new(Some(4), Role::Follower, behind).is_none());
    assert!(Received::new(None, Role::Leader, Ballot::Elected).is_some());
    assert!(Received::new(None, Role::Candidate, Ballot::Elected).is_none());
    assert!(Received::new(None, Role::Candidate, Ballot::Denied).is_some());
    assert!(Received::new(None, Role::Follower, Ballot::Counted).is_none());
    assert!(Received::new(None, Role::Follower, Ballot::Overtaken).is_none());
    assert!(Received::new(None, Role::Leader, Ack::Conflict).is_some());
    assert!(Received::new(None, Role::Candidate, Ack::Duplicate).is_none());
    assert!(Received::new(Some(2), Role::Follower, Ack::Overtaken).is_some());
    assert!(Received::new(None, Role::Follower, matched(2, 1)).is_some());
    assert!(Received::new(None, Role::Leader, matched(2, 1)).is_none());
    let invalid = AppendResponse {
        term: 0,
        append: Append::Conflict,
    };
    assert!(Received::new(None, Role::Follower, invalid).is_none());
}
