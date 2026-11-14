//! Symbol names of every core enum, and the way back for the two a caller
//! hands in. Each match lists every variant.
use consensus_machines::{Ack, Append, Ballot, Denial, Refusal, Role, Vote};
use magnus::{Error, Ruby, Symbol};

use crate::hooks::rejected;
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

pub fn to_role(ruby: &Ruby, name: Symbol) -> Result<Role, Error> {
    match &*name.name()? {
        "follower" => Ok(Role::Follower),
        "candidate" => Ok(Role::Candidate),
        "leader" => Ok(Role::Leader),
        _ => Err(rejected(
            ruby,
            "role must be :follower, :candidate or :leader",
        )),
    }
}

pub fn to_denial(ruby: &Ruby, name: Symbol) -> Result<Denial, Error> {
    match &*name.name()? {
        "term_behind" => Ok(Denial::TermBehind),
        "already_voted" => Ok(Denial::AlreadyVoted),
        "log_behind" => Ok(Denial::LogBehind),
        _ => Err(rejected(
            ruby,
            "reason must be :term_behind, :already_voted or :log_behind",
        )),
    }
}

/// The `Ballot` a symbol names, if it names one.
pub fn to_ballot(name: &str) -> Option<Ballot> {
    match name {
        "counted" => Some(Ballot::Counted),
        "elected" => Some(Ballot::Elected),
        "duplicate" => Some(Ballot::Duplicate),
        "denied" => Some(Ballot::Denied),
        "overtaken" => Some(Ballot::Overtaken),
        _ => None,
    }
}

/// The `Ack` a symbol names, if it names one.
pub fn to_ack(name: &str) -> Option<Ack> {
    match name {
        "advanced" => Some(Ack::Advanced),
        "duplicate" => Some(Ack::Duplicate),
        "conflict" => Some(Ack::Conflict),
        "overtaken" => Some(Ack::Overtaken),
        _ => None,
    }
}

pub fn to_vote(ruby: &Ruby, name: Symbol, reason: Option<Symbol>) -> Result<Vote, Error> {
    match (&*name.name()?, reason) {
        ("granted", None) => Ok(Vote::Granted),
        ("duplicate", None) => Ok(Vote::Duplicate),
        ("denied", Some(reason)) => Ok(Vote::Denied(to_denial(ruby, reason)?)),
        _ => Err(rejected(
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
        _ => Err(rejected(
            ruby,
            "append must be Matched(index), :conflict or :term_behind",
        )),
    }
}
