//! Every value the binding reads from Ruby: exactly this gem's class, its
//! fields read from the struct and rebuilt through the core's validating
//! constructor. A wrong type, a subclass included, is a `TypeError`; a value
//! the constructor rejects is a `RangeError`, so a forged message never
//! reaches the core and never earns `:malformed`.
use consensus_machines::{
    Append, AppendRequest, AppendResponse, Entry, Position, Vote, VoteRequest, VoteResponse,
};
use magnus::{Error, RArray, Ruby, Symbol, TryConvert, Value};

use crate::classes::{Classes, classes, fields, is};
use crate::hooks::rejected;
use crate::names;
use crate::values::{Id, index};

fn id(value: Value) -> Result<Id, Error> {
    Id::try_convert(value)
}

/// The core's `Position::new`.
pub fn position_of(ruby: &Ruby, at: Id, term: Id) -> Result<Position, Error> {
    Position::new(index(ruby, at)?, term.0).ok_or_else(|| {
        rejected(
            ruby,
            "a position's index and term are both zero or both set",
        )
    })
}

/// A `Position`.
pub fn position(ruby: &Ruby, value: Value) -> Result<Position, Error> {
    let [at, term] = fields(ruby, value, classes(ruby)?.position)?;
    position_of(ruby, id(at)?, id(term)?)
}

/// The core's `Entry::new`.
pub fn entry_of(ruby: &Ruby, term: Id, command: Id) -> Result<Entry, Error> {
    Entry::new(term.0, command.0).ok_or_else(|| rejected(ruby, "an entry's term is positive"))
}

/// An Array of `Entry`. Reading an element or a struct field runs no Ruby
/// code, so the array cannot change underneath the loop.
pub fn entries(ruby: &Ruby, list: RArray) -> Result<Vec<Entry>, Error> {
    let class = classes(ruby)?.entry;
    (0..list.len())
        .map(|offset| {
            let value: Value = list.entry(offset.cast_signed())?;
            let [term, command] = fields(ruby, value, class)?;
            entry_of(ruby, id(term)?, id(command)?)
        })
        .collect()
}

/// A vote: `:granted`, `:duplicate` or a `Denied`.
fn vote(ruby: &Ruby, classes: &Classes, value: Value) -> Result<Vote, Error> {
    if let Some(name) = Symbol::from_value(value) {
        return names::to_vote(ruby, name, None);
    }
    let [reason] = fields(ruby, value, classes.denied)?;
    let reason = Symbol::from_value(reason)
        .ok_or_else(|| Error::new(ruby.exception_type_error(), "a reason is a Symbol"))?;
    Ok(Vote::Denied(names::to_denial(ruby, reason)?))
}

/// The core's `VoteResponse::new` of a term and a vote as Ruby holds them.
pub fn vote_response_fields(ruby: &Ruby, term: Id, given: Value) -> Result<VoteResponse, Error> {
    let vote = vote(ruby, classes(ruby)?, given)?;
    VoteResponse::new(term.0, vote).ok_or_else(|| {
        rejected(
            ruby,
            "a vote response's term is positive, and above one for :term_behind",
        )
    })
}

/// A `VoteResponse`.
pub fn vote_response(ruby: &Ruby, value: Value) -> Result<VoteResponse, Error> {
    let [term, given] = fields(ruby, value, classes(ruby)?.vote_response)?;
    vote_response_fields(ruby, id(term)?, given)
}

/// A `VoteRequest`, through `VoteRequest::new`.
pub fn vote_request(ruby: &Ruby, value: Value) -> Result<VoteRequest, Error> {
    let [candidate, term, last] = fields(ruby, value, classes(ruby)?.vote_request)?;
    let (candidate, last) = (index(ruby, id(candidate)?)?, position(ruby, last)?);
    VoteRequest::new(candidate, id(term)?.0, last).ok_or_else(|| {
        rejected(
            ruby,
            "a vote request's term is positive and not below its last position's",
        )
    })
}

/// An append answer: a `Matched`, `:conflict` or `:term_behind`.
fn append(ruby: &Ruby, classes: &Classes, value: Value) -> Result<Append, Error> {
    if let Some(name) = Symbol::from_value(value) {
        return names::to_append(ruby, name, None);
    }
    let [at] = fields(ruby, value, classes.matched)?;
    Ok(Append::Matched(index(ruby, id(at)?)?))
}

/// The core's `AppendResponse::new` of a term and an append answer as Ruby
/// holds them.
pub fn append_response_fields(
    ruby: &Ruby,
    term: Id,
    given: Value,
) -> Result<AppendResponse, Error> {
    let append = append(ruby, classes(ruby)?, given)?;
    AppendResponse::new(term.0, append).ok_or_else(|| {
        rejected(
            ruby,
            "an append response's term is positive, and above one for :term_behind",
        )
    })
}

/// An `AppendResponse`.
pub fn append_response(ruby: &Ruby, value: Value) -> Result<AppendResponse, Error> {
    let [term, given] = fields(ruby, value, classes(ruby)?.append_response)?;
    append_response_fields(ruby, id(term)?, given)
}

/// An `AppendRequest`, through `AppendRequest::new`.
pub fn append_request(ruby: &Ruby, value: Value) -> Result<AppendRequest, Error> {
    let [leader, term, previous, committed] = fields(ruby, value, classes(ruby)?.append_request)?;
    let (leader, previous) = (index(ruby, id(leader)?)?, position(ruby, previous)?);
    let committed = index(ruby, id(committed)?)?;
    AppendRequest::new(leader, id(term)?.0, previous, committed).ok_or_else(|| {
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

macro_rules! argument {
    ($name:ident, $core:ty, $read:path) => {
        #[doc = concat!("A `", stringify!($core), "` argument, read by `", stringify!($read), "`.")]
        #[derive(Clone, Copy)]
        pub struct $name(pub $core);

        impl TryConvert for $name {
            fn try_convert(value: Value) -> Result<Self, Error> {
                $read(&Ruby::get_with(value), value).map(Self)
            }
        }
    };
}

argument!(VoteRequestValue, VoteRequest, vote_request);
argument!(VoteResponseValue, VoteResponse, vote_response);
argument!(AppendRequestValue, AppendRequest, append_request);
argument!(AppendResponseValue, AppendResponse, append_response);
