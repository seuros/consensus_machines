//! Symbols of every core enum, and the way back for those a caller hands
//! in. Every enum is a `symbols!` table, checked against the enum by the
//! compiler; `Vote` and `Append` carry data, so theirs are output-only
//! (`names_only`) and a bare Symbol is read back by hand.
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

matryona_machines::symbols! {
    /// The core's votes as Symbols; a `Denied` leaves as a `Denied` value.
    pub Votes for Vote as "a vote" => names_only {
        Granted => "granted",
        Duplicate => "duplicate",
        Denied(..) => "denied",
    }
}

matryona_machines::symbols! {
    /// The core's append answers as Symbols; a `Matched` leaves as a
    /// `Matched` value.
    pub Appends for Append as "an append" => names_only {
        Matched(..) => "matched",
        Conflict => "conflict",
        TermBehind => "term_behind",
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
