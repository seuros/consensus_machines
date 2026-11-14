//! Every value the binding reads from Ruby: exactly this gem's class, its
//! fields read from the struct and rebuilt through the core's validating
//! constructor. A wrong type, a subclass included, is a `TypeError`; a value
//! the constructor rejects is a `RangeError`, so a forged message never
//! reaches the core and never earns `:malformed`.
use consensus_machines::{
    Append, AppendRequest, AppendResponse, Entry, Position, Vote, VoteRequest, VoteResponse,
};
use magnus::{Error, RArray, Ruby, Symbol, Value};
use matryona_machines::ruby::integer;

use crate::classes::{Classes, classes, fields, is};
use crate::hooks::rejected;
use crate::names;

/// The core's `Position::new` of an `index` and a `term` given as Integers.
pub fn position_of(ruby: &Ruby, at: Value, term: Value) -> Result<Position, Error> {
    let (at, term) = (integer(at, "index")?, integer(term, "term")?);
    Position::new(at, term).ok_or_else(|| {
        rejected(
            ruby,
            "a position's index and term are both zero or both set",
        )
    })
}

/// A `Position`.
pub fn position(ruby: &Ruby, value: Value) -> Result<Position, Error> {
    let [at, term] = fields(ruby, value, classes(ruby)?.position)?;
    position_of(ruby, at, term)
}

/// The core's `Entry::new` of a `term` and a `command` given as Integers.
pub fn entry_of(ruby: &Ruby, term: Value, command: Value) -> Result<Entry, Error> {
    let (term, command) = (integer(term, "term")?, integer(command, "command")?);
    Entry::new(term, command).ok_or_else(|| rejected(ruby, "an entry's term is positive"))
}

/// An Array of `Entry`. Reading an element or a struct field runs no Ruby
/// code, so the array cannot change underneath the loop.
pub fn entries(ruby: &Ruby, list: RArray) -> Result<Vec<Entry>, Error> {
    let class = classes(ruby)?.entry;
    (0..list.len())
        .map(|offset| {
            let value: Value = list.entry(offset.cast_signed())?;
            let [term, command] = fields(ruby, value, class)?;
            entry_of(ruby, term, command)
        })
        .collect()
}

/// A vote: `:granted`, `:duplicate` or a `Denied`.
fn vote(ruby: &Ruby, classes: &Classes, value: Value) -> Result<Vote, Error> {
    if let Some(name) = Symbol::from_value(value) {
        return names::to_vote(ruby, name);
    }
    let [reason] = fields(ruby, value, classes.denied)?;
    Ok(Vote::Denied(names::to_denial(ruby, reason)?))
}

/// The core's `VoteResponse::new` of a term and a vote as Ruby holds them.
pub fn vote_response_fields(ruby: &Ruby, term: Value, given: Value) -> Result<VoteResponse, Error> {
    let term = integer(term, "term")?;
    let vote = vote(ruby, classes(ruby)?, given)?;
    VoteResponse::new(term, vote).ok_or_else(|| {
        rejected(
            ruby,
            "a vote response's term is positive, and above one for :term_behind",
        )
    })
}

/// A `VoteResponse`.
pub fn vote_response(ruby: &Ruby, value: Value) -> Result<VoteResponse, Error> {
    let [term, given] = fields(ruby, value, classes(ruby)?.vote_response)?;
    vote_response_fields(ruby, term, given)
}

/// A `VoteRequest`, through `VoteRequest::new`.
pub fn vote_request(ruby: &Ruby, value: Value) -> Result<VoteRequest, Error> {
    let [candidate, term, last] = fields(ruby, value, classes(ruby)?.vote_request)?;
    let (candidate, last) = (integer(candidate, "candidate")?, position(ruby, last)?);
    VoteRequest::new(candidate, integer(term, "term")?, last).ok_or_else(|| {
        rejected(
            ruby,
            "a vote request's term is positive and not below its last position's",
        )
    })
}

/// An append answer: a `Matched`, `:conflict` or `:term_behind`.
fn append(ruby: &Ruby, classes: &Classes, value: Value) -> Result<Append, Error> {
    if let Some(name) = Symbol::from_value(value) {
        return names::to_append(ruby, name);
    }
    let [at] = fields(ruby, value, classes.matched)?;
    Ok(Append::Matched(integer(at, "index")?))
}

/// The core's `AppendResponse::new` of a term and an append answer as Ruby
/// holds them.
pub fn append_response_fields(
    ruby: &Ruby,
    term: Value,
    given: Value,
) -> Result<AppendResponse, Error> {
    let term = integer(term, "term")?;
    let append = append(ruby, classes(ruby)?, given)?;
    AppendResponse::new(term, append).ok_or_else(|| {
        rejected(
            ruby,
            "an append response's term is positive, and above one for :term_behind",
        )
    })
}

/// An `AppendResponse`.
pub fn append_response(ruby: &Ruby, value: Value) -> Result<AppendResponse, Error> {
    let [term, given] = fields(ruby, value, classes(ruby)?.append_response)?;
    append_response_fields(ruby, term, given)
}

/// An `AppendRequest`, through `AppendRequest::new`.
pub fn append_request(ruby: &Ruby, value: Value) -> Result<AppendRequest, Error> {
    let [leader, term, previous, committed] = fields(ruby, value, classes(ruby)?.append_request)?;
    let (leader, previous) = (integer(leader, "leader")?, position(ruby, previous)?);
    let committed = integer(committed, "committed")?;
    AppendRequest::new(leader, integer(term, "term")?, previous, committed).ok_or_else(|| {
        rejected(
            ruby,
            "an append request's term is positive and not below its previous position's",
        )
    })
}

/// Whether `value` is exactly this gem's `VoteResponse` or `AppendResponse`.
pub fn response_kind(ruby: &Ruby, value: Value) -> Result<Option<bool>, Error> {
    let classes = classes(ruby)?;
    Ok(if is(ruby, value, classes.vote_response) {
        Some(true)
    } else if is(ruby, value, classes.append_response) {
        Some(false)
    } else {
        None
    })
}
