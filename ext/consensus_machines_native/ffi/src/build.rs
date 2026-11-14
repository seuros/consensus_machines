//! The answers of the mutating calls, built by each class's `new` (so Ruby
//! runs) while the decision is held on a copy: see `matryona_machines::ruby::keep`.
use consensus_machines::{AppendResponse, Received, Refusal, VoteRequest, VoteResponse};
use magnus::{ArgList, Error, Ruby, Value, prelude::*};
use matryona_machines::ruby::{Symbols, symbol};

use crate::classes::{Classes, Data};
use crate::names::{self, Denials, Refusals, Roles};

/// A refusal's symbol.
pub fn refused(ruby: &Ruby, refusal: Refusal) -> Value {
    Refusals::symbol(ruby, refusal).as_value()
}

fn new<A: ArgList>(ruby: &Ruby, data: Data, args: A) -> Result<Value, Error> {
    data.class.get(ruby).funcall("new", args)
}

/// A `VoteRequest`, its `last` a `Position`.
pub fn vote_request(ruby: &Ruby, classes: &Classes, request: VoteRequest) -> Result<Value, Error> {
    let last = new(
        ruby,
        classes.position,
        (request.last.index, request.last.term),
    )?;
    new(
        ruby,
        classes.vote_request,
        (request.candidate, request.term, last),
    )
}

/// A `VoteResponse`, its `vote` a Symbol or a `Denied`.
pub fn vote_response(
    ruby: &Ruby,
    classes: &Classes,
    response: VoteResponse,
) -> Result<Value, Error> {
    let vote = match names::vote(response.vote) {
        (_, Some(reason)) => new(ruby, classes.denied, (Denials::symbol(ruby, reason),))?,
        (vote, None) => symbol(ruby, vote).as_value(),
    };
    new(ruby, classes.vote_response, (response.term, vote))
}

/// An `AppendResponse`, its `append` a `Matched` or a Symbol.
pub fn append_response(
    ruby: &Ruby,
    classes: &Classes,
    response: AppendResponse,
) -> Result<Value, Error> {
    let append = match names::append(response.append) {
        (_, Some(at)) => new(ruby, classes.matched, (at,))?,
        (append, None) => symbol(ruby, append).as_value(),
    };
    new(ruby, classes.append_response, (response.term, append))
}

/// A `Received` of `received`'s stages and the already built `outcome`.
pub fn received<T>(
    ruby: &Ruby,
    classes: &Classes,
    received: &Received<T>,
    outcome: Value,
) -> Result<Value, Error> {
    let role = Roles::symbol(ruby, received.role);
    new(ruby, classes.received, (received.adopted, role, outcome))
}
