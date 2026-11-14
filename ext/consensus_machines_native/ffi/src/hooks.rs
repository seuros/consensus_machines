//! Private hooks behind every `Data` constructor in `lib/`: a value exists
//! only when the core's validating constructor accepts it. A wrong type
//! raises `TypeError` while the arguments convert (a nested value that is not
//! exactly this gem's class included); a value of the right type the core
//! rejects raises `RangeError`. Each hook answers nil and the Ruby side keeps
//! the fields it was given.
use consensus_machines::{Outcome, Received, Role};
use magnus::{Error, RModule, Ruby, Symbol, Value, function, prelude::*};
use matryona_machines::ruby::{exact_array, integer, optional};

use crate::read;
use crate::{ENTRIES_ARRAY, MEMBERS_ARRAY, model, names, saved};

/// A value of the right type the core rejects: out of the domain.
pub fn rejected(ruby: &Ruby, what: &'static str) -> Error {
    Error::new(ruby.exception_range_error(), what)
}

fn position(ruby: &Ruby, at: Value, term: Value) -> Result<(), Error> {
    read::position_of(ruby, at, term).map(drop)
}

fn entry(ruby: &Ruby, term: Value, command: Value) -> Result<(), Error> {
    read::entry_of(ruby, term, command).map(drop)
}

fn vote_request(ruby: &Ruby, candidate: Value, term: Value, last: Value) -> Result<(), Error> {
    let (candidate, term) = (integer(candidate, "candidate")?, integer(term, "term")?);
    let last = read::position(ruby, last)?;
    consensus_machines::VoteRequest::new(candidate, term, last)
        .map(drop)
        .ok_or_else(|| {
            rejected(
                ruby,
                "a vote request's term is positive and not below its last position's",
            )
        })
}

fn vote_response(ruby: &Ruby, term: Value, vote: Value) -> Result<(), Error> {
    read::vote_response_fields(ruby, term, vote).map(drop)
}

fn denied(ruby: &Ruby, reason: Value) -> Result<(), Error> {
    names::to_denial(ruby, reason).map(drop)
}

fn append_request(
    ruby: &Ruby,
    leader: Value,
    term: Value,
    previous: Value,
    committed: Value,
) -> Result<(), Error> {
    let (leader, term) = (integer(leader, "leader")?, integer(term, "term")?);
    let committed = integer(committed, "committed")?;
    let previous = read::position(ruby, previous)?;
    consensus_machines::AppendRequest::new(leader, term, previous, committed)
        .map(drop)
        .ok_or_else(|| {
            rejected(
                ruby,
                "an append request's term is positive and not below its previous position's",
            )
        })
}

fn append_response(ruby: &Ruby, term: Value, append: Value) -> Result<(), Error> {
    read::append_response_fields(ruby, term, append).map(drop)
}

fn matched(at: Value) -> Result<(), Error> {
    integer::<usize>(at, "index").map(drop)
}

const IMPOSSIBLE: &str = "no server gives this answer: an adopted term is the reply's, \
    never zero, and leaves a follower, and the outcome must follow the role";

/// The core's `Received::new` for an answer a server can give, or `RangeError`.
fn given<T: Outcome>(
    ruby: &Ruby,
    adopted: Option<u64>,
    role: Role,
    outcome: T,
) -> Result<(), Error> {
    Received::new(adopted, role, outcome)
        .map(drop)
        .ok_or_else(|| rejected(ruby, IMPOSSIBLE))
}

/// A ballot or an ack. `:duplicate` and `:overtaken` name both; the value is
/// valid when either reading is an answer a server gives.
fn given_symbol(
    ruby: &Ruby,
    adopted: Option<u64>,
    role: Role,
    outcome: Symbol,
) -> Result<(), Error> {
    let name = outcome.name()?;
    let (ballot, ack) = (names::to_ballot(&name), names::to_ack(&name));
    if ballot.is_none() && ack.is_none() {
        return Err(rejected(
            ruby,
            "outcome must be a VoteResponse, an AppendResponse, a ballot or an ack",
        ));
    }
    let given = ballot.is_some_and(|ballot| Received::new(adopted, role, ballot).is_some())
        || ack.is_some_and(|ack| Received::new(adopted, role, ack).is_some());
    if given {
        Ok(())
    } else {
        Err(rejected(ruby, IMPOSSIBLE))
    }
}

/// `Received.new`: the outcome is exactly a `VoteResponse`, an
/// `AppendResponse`, or a ballot or ack Symbol.
fn received(ruby: &Ruby, adopted: Value, role: Value, outcome: Value) -> Result<(), Error> {
    let (adopted, role) = (optional(adopted, "adopted")?, names::to_role(ruby, role)?);
    if let Some(name) = Symbol::from_value(outcome) {
        return given_symbol(ruby, adopted, role, name);
    }
    match read::response_kind(ruby, outcome)? {
        Some(true) => given(ruby, adopted, role, read::vote_response(ruby, outcome)?),
        Some(false) => given(ruby, adopted, role, read::append_response(ruby, outcome)?),
        None => Err(Error::new(
            ruby.exception_type_error(),
            "expected a VoteResponse, an AppendResponse or a Symbol (exactly; none can be \
             subclassed)",
        )),
    }
}

/// `Checkpoint.new`: the core's `Checkpoint::new`, for the membership size
/// it names. `members` and `entries` are exactly Arrays (no subclass),
/// `entries` of exactly this gem's `Entry`.
fn checkpoint(
    ruby: &Ruby,
    members: Value,
    own: Value,
    term: Value,
    voted_for: Value,
    committed: Value,
    entries: Value,
) -> Result<(), Error> {
    let members = exact_array(ruby, members, MEMBERS_ARRAY)?;
    let (own, term) = (integer(own, "own")?, integer(term, "term")?);
    let voted_for = optional(voted_for, "voted_for")?;
    let committed = integer(committed, "committed")?;
    let entries = exact_array(ruby, entries, ENTRIES_ARRAY)?;
    let saved = saved(ruby, (members, own, term, voted_for, committed, entries))?;
    model::build(&saved.members, saved.own, Some(&saved))
        .map(drop)
        .ok_or_else(|| {
            rejected(
                ruby,
                "not a state a server can be in: 1..9 distinct members, seats of the \
                 membership, no vote in term zero, positive entry terms never decreasing \
                 nor above the term, at most 64 entries, committed within them",
            )
        })
}

pub fn define(module: RModule) -> Result<(), Error> {
    module.define_singleton_method("__position", function!(position, 2))?;
    module.define_singleton_method("__entry", function!(entry, 2))?;
    module.define_singleton_method("__vote_request", function!(vote_request, 3))?;
    module.define_singleton_method("__vote_response", function!(vote_response, 2))?;
    module.define_singleton_method("__denied", function!(denied, 1))?;
    module.define_singleton_method("__append_request", function!(append_request, 4))?;
    module.define_singleton_method("__append_response", function!(append_response, 2))?;
    module.define_singleton_method("__matched", function!(matched, 1))?;
    module.define_singleton_method("__received", function!(received, 3))?;
    module.define_singleton_method("__checkpoint", function!(checkpoint, 6))
}
