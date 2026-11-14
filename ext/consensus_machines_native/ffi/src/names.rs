//! Symbols of every core enum, and the way back for those a caller hands
//! in. The unit-variant enums are `symbols!` tables, checked against the
//! enum by the compiler; `Vote` and `Append` carry data and are matched here.
use consensus_machines::{Ack, Append, Ballot, Denial, Refusal, Role, Vote};
use magnus::{Error, Ruby, Symbol, Value};
use matryona_machines::ruby::Symbols;

use crate::hooks::rejected;

matryona_machines::symbols! {
    /// The core's roles as Symbols.
    pub Roles for Role as "role" {
        Follower => "follower",
        Candidate => "candidate",
        Leader => "leader",
    }
}

matryona_machines::symbols! {
    /// The core's refusals as Symbols: the snake case of each variant.
    pub Refusals for Refusal as "a refusal" {
        UnknownMember => "unknown_member",
        OwnSeat => "own_seat",
        Malformed => "malformed",
        TooManyEntries => "too_many_entries",
        Stale => "stale",
        NotCandidate => "not_candidate",
        NotLeader => "not_leader",
        RivalLeader => "rival_leader",
        Mismatch => "mismatch",
        CommittedConflict => "committed_conflict",
        BeyondLog => "beyond_log",
        LogFull => "log_full",
        TermExhausted => "term_exhausted",
    }
}

matryona_machines::symbols! {
    /// The reasons a vote is denied, as Symbols.
    pub Denials for Denial as "reason" {
        TermBehind => "term_behind",
        AlreadyVoted => "already_voted",
        LogBehind => "log_behind",
    }
}

matryona_machines::symbols! {
    /// The answers to a vote response, as Symbols.
    pub Ballots for Ballot as "a ballot" {
        Counted => "counted",
        Elected => "elected",
        Duplicate => "duplicate",
        Denied => "denied",
        Overtaken => "overtaken",
    }
}

matryona_machines::symbols! {
    /// The answers to an append response, as Symbols.
    pub Acks for Ack as "an ack" {
        Advanced => "advanced",
        Duplicate => "duplicate",
        Conflict => "conflict",
        Overtaken => "overtaken",
    }
}

/// A vote as its variant name and, for `Denied`, the reason.
pub const fn vote(vote: Vote) -> (&'static str, Option<Denial>) {
    match vote {
        Vote::Granted => ("granted", None),
        Vote::Duplicate => ("duplicate", None),
        Vote::Denied(reason) => ("denied", Some(reason)),
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

/// The `Role` the Symbol `value` names.
pub fn to_role(ruby: &Ruby, value: Value) -> Result<Role, Error> {
    Roles::parse(ruby, value)
}

/// The `Denial` the Symbol `value` names.
pub fn to_denial(ruby: &Ruby, value: Value) -> Result<Denial, Error> {
    Denials::parse(ruby, value)
}

/// The variant of table `T` named `name`, if it names one.
fn named<T: Symbols>(name: &str) -> Option<T::Variant> {
    T::ALL
        .iter()
        .copied()
        .find(|variant| T::name(*variant) == name)
}

/// The `Ballot` a symbol names, if it names one.
pub fn to_ballot(name: &str) -> Option<Ballot> {
    named::<Ballots>(name)
}

/// The `Ack` a symbol names, if it names one.
pub fn to_ack(name: &str) -> Option<Ack> {
    named::<Acks>(name)
}

/// A vote given as a bare Symbol; a denial is a `Denied`, never a Symbol.
pub fn to_vote(ruby: &Ruby, name: Symbol) -> Result<Vote, Error> {
    match &*name.name()? {
        "granted" => Ok(Vote::Granted),
        "duplicate" => Ok(Vote::Duplicate),
        _ => Err(rejected(
            ruby,
            "vote must be :granted, :duplicate or a Denied with a known reason",
        )),
    }
}

/// An append answer given as a bare Symbol; a match is a `Matched`.
pub fn to_append(ruby: &Ruby, name: Symbol) -> Result<Append, Error> {
    match &*name.name()? {
        "conflict" => Ok(Append::Conflict),
        "term_behind" => Ok(Append::TermBehind),
        _ => Err(rejected(
            ruby,
            "append must be Matched(index), :conflict or :term_behind",
        )),
    }
}
