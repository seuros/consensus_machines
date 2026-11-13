//! Symbol names of every core enum, and the way back for the two a caller
//! hands in. Each match lists every variant.
use consensus_machines::{Ack, Append, Ballot, Denial, Refusal, Role, Vote};
use magnus::{Error, Ruby, Symbol};

use crate::values::{Id, index};

pub const fn role(role: Role) -> &'static str {
    match role {
        Role::Follower => "follower",
        Role::Candidate => "candidate",
        Role::Leader => "leader",
    }
}

pub const fn refusal(refusal: Refusal) -> &'static str {
    match refusal {
        Refusal::UnknownMember => "unknown_member",
        Refusal::OwnSeat => "own_seat",
        Refusal::Malformed => "malformed",
        Refusal::TooManyEntries => "too_many_entries",
        Refusal::Stale => "stale",
        Refusal::NotCandidate => "not_candidate",
        Refusal::NotLeader => "not_leader",
        Refusal::RivalLeader => "rival_leader",
        Refusal::Mismatch => "mismatch",
        Refusal::CommittedConflict => "committed_conflict",
        Refusal::BeyondLog => "beyond_log",
        Refusal::LogFull => "log_full",
        Refusal::TermExhausted => "term_exhausted",
    }
}

const fn denial(denial: Denial) -> &'static str {
    match denial {
        Denial::TermBehind => "term_behind",
        Denial::AlreadyVoted => "already_voted",
        Denial::LogBehind => "log_behind",
    }
}

/// A vote as its variant name and, for `Denied`, the reason.
pub const fn vote(vote: Vote) -> (&'static str, Option<&'static str>) {
    match vote {
        Vote::Granted => ("granted", None),
        Vote::Duplicate => ("duplicate", None),
        Vote::Denied(reason) => ("denied", Some(denial(reason))),
    }
}

pub const fn ballot(ballot: Ballot) -> &'static str {
    match ballot {
        Ballot::Counted => "counted",
        Ballot::Elected => "elected",
        Ballot::Duplicate => "duplicate",
        Ballot::Denied => "denied",
        Ballot::Overtaken => "overtaken",
    }
}

/// An append answer as its variant name and, for `Matched`, the index.
pub const fn append(append: Append) -> (&'static str, Option<usize>) {
    match append {
        Append::Matched(index) => ("matched", Some(index)),
        Append::Conflict => ("conflict", None),
        Append::TermBehind => ("term_behind", None),
    }
}

pub const fn ack(ack: Ack) -> &'static str {
    match ack {
        Ack::Advanced => "advanced",
        Ack::Duplicate => "duplicate",
        Ack::Conflict => "conflict",
        Ack::Overtaken => "overtaken",
    }
}

fn unknown(ruby: &Ruby, what: &'static str) -> Error {
    Error::new(ruby.exception_arg_error(), what)
}

pub fn to_vote(ruby: &Ruby, name: Symbol, reason: Option<Symbol>) -> Result<Vote, Error> {
    let reason = reason.map(Symbol::name).transpose()?;
    match (&*name.name()?, reason.as_deref()) {
        ("granted", None) => Ok(Vote::Granted),
        ("duplicate", None) => Ok(Vote::Duplicate),
        ("denied", Some("term_behind")) => Ok(Vote::Denied(Denial::TermBehind)),
        ("denied", Some("already_voted")) => Ok(Vote::Denied(Denial::AlreadyVoted)),
        ("denied", Some("log_behind")) => Ok(Vote::Denied(Denial::LogBehind)),
        _ => Err(unknown(
            ruby,
            "vote must be :granted, :duplicate or a Denied with a known reason",
        )),
    }
}

pub fn to_append(ruby: &Ruby, name: Symbol, matched: Option<Id>) -> Result<Append, Error> {
    match (&*name.name()?, matched) {
        ("matched", Some(position)) => Ok(Append::Matched(index(ruby, position)?)),
        ("conflict", None) => Ok(Append::Conflict),
        ("term_behind", None) => Ok(Append::TermBehind),
        _ => Err(unknown(
            ruby,
            "append must be Matched(index), :conflict or :term_behind",
        )),
    }
}
