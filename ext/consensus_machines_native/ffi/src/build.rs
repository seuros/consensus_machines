//! The answers of the mutating calls, built by each class's `new` (so Ruby
//! runs) while the decision is held on a copy: see [`crate::classes::keep`].
use consensus_machines::{AppendResponse, Received, Refusal, VoteRequest, VoteResponse};
use magnus::value::Opaque;
use magnus::{ArgList, Error, RClass, Ruby, Value, prelude::*};

use crate::classes::Classes;
use crate::names;

/// A refusal's symbol.
pub fn refused(ruby: &Ruby, refusal: Refusal) -> Value {
    ruby.to_symbol(names::refusal(refusal)).as_value()
}

fn new<A: ArgList>(ruby: &Ruby, class: Opaque<RClass>, args: A) -> Result<Value, Error> {
    ruby.get_inner(class).funcall("new", args)
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
        (_, Some(reason)) => new(ruby, classes.denied, (ruby.to_symbol(reason),))?,
        (vote, None) => ruby.to_symbol(vote).as_value(),
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
        (append, None) => ruby.to_symbol(append).as_value(),
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
    let role = ruby.to_symbol(names::role(received.role));
    new(ruby, classes.received, (received.adopted, role, outcome))
}
