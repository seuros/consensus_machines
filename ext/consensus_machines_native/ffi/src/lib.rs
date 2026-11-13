//! Native Ruby bridge for a bounded Raft model, not a consensus service.
//! Values cross flattened; `lib/consensus_machines.rb` rebuilds the `Data`.
mod init;
mod model;
mod names;
mod values;
use std::cell::RefCell;

use consensus_machines::{
    AppendRequest, AppendResponse, Entry, Position, Received, Refusal, VoteRequest, VoteResponse,
};
use magnus::{Error, IntoValue, RArray, Ruby, Symbol, Value, prelude::*, typed_data::Obj};
use values::{Id, index, numbers};

/// `[members, own, term, voted_for, committed, entries]`, entries flattened
/// to `term, command, term, command, ...`.
type SavedFields = (RArray, Id, Id, Option<Id>, Id, RArray);

#[magnus::wrap(class = "ConsensusMachines::Raft", free_immediately, size)]
struct Wrapper {
    node: RefCell<Box<dyn model::Model>>,
}

/// A refusal as its symbol, or an honoured answer as whatever `honoured` builds.
fn answer<T, V: IntoValue>(
    ruby: &Ruby,
    outcome: Result<T, Refusal>,
    honoured: impl FnOnce(T) -> V,
) -> Value {
    match outcome {
        Ok(value) => honoured(value).into_value_with(ruby),
        Err(refusal) => ruby.to_symbol(names::refusal(refusal)).as_value(),
    }
}

/// The two stages every inbound answer starts with.
fn stages<T>(ruby: &Ruby, received: &Received<T>) -> (Option<u64>, Symbol) {
    (received.adopted, ruby.to_symbol(names::role(received.role)))
}

fn entries(ruby: &Ruby, flat: RArray) -> Result<Vec<Entry>, Error> {
    let flat = numbers(flat)?;
    let (pairs, odd) = flat.as_chunks::<2>();
    if !odd.is_empty() {
        return Err(Error::new(
            ruby.exception_arg_error(),
            "entries must be term, command pairs",
        ));
    }
    Ok(pairs
        .iter()
        .map(|&[term, command]| Entry { term, command })
        .collect())
}

fn membership(ruby: &Ruby, members: RArray) -> Result<Vec<u64>, Error> {
    if (1..=9).contains(&members.len()) {
        numbers(members)
    } else {
        Err(Error::new(
            ruby.exception_arg_error(),
            "membership must contain 1..9 members",
        ))
    }
}

impl Wrapper {
    fn new(ruby: &Ruby, members: RArray, own: Id) -> Result<Self, Error> {
        let members = membership(ruby, members)?;
        let node = model::build(&members, index(ruby, own)?, None).ok_or_else(|| {
            Error::new(
                ruby.exception_arg_error(),
                "members must be unique and own index must be in membership",
            )
        })?;
        Ok(Self {
            node: RefCell::new(node),
        })
    }
    fn restored(ruby: &Ruby, members: RArray, own: Id, saved: SavedFields) -> Result<Self, Error> {
        let members = membership(ruby, members)?;
        let (saved_members, saved_own, term, voted_for, committed, log) = saved;
        let saved = model::Saved {
            members: numbers(saved_members)?,
            own: index(ruby, saved_own)?,
            term: term.0,
            voted_for: voted_for.map(|seat| index(ruby, seat)).transpose()?,
            committed: index(ruby, committed)?,
            entries: entries(ruby, log)?,
        };
        let node = model::build(&members, index(ruby, own)?, Some(&saved)).ok_or_else(|| {
            Error::new(
                ruby.exception_arg_error(),
                "not a checkpoint of this membership and seat",
            )
        })?;
        Ok(Self {
            node: RefCell::new(node),
        })
    }
    fn members(&self) -> Vec<u64> {
        self.node.borrow().members()
    }
    fn own(&self) -> usize {
        self.node.borrow().own()
    }
    fn role(ruby: &Ruby, rb_self: &Self) -> Symbol {
        ruby.to_symbol(names::role(rb_self.node.borrow().role()))
    }
    fn term(&self) -> u64 {
        self.node.borrow().term()
    }
    fn committed(&self) -> usize {
        self.node.borrow().committed()
    }
    fn last(&self) -> (usize, u64) {
        let last = self.node.borrow().last();
        (last.index, last.term)
    }
    fn entry(ruby: &Ruby, rb_self: &Self, position: Id) -> Result<Option<(u64, u64)>, Error> {
        let entry = rb_self.node.borrow().entry(index(ruby, position)?);
        Ok(entry.map(|entry| (entry.term, entry.command)))
    }
    fn begin_election(ruby: &Ruby, rb_self: Obj<Self>) -> Result<Value, Error> {
        rb_self.check_frozen()?;
        let outcome = rb_self.node.borrow_mut().begin_election();
        Ok(answer(ruby, outcome, |request| {
            (
                request.candidate,
                request.term,
                request.last.index,
                request.last.term,
            )
        }))
    }
    fn request_vote(
        ruby: &Ruby,
        rb_self: Obj<Self>,
        candidate: Id,
        term: Id,
        last: (Id, Id),
    ) -> Result<Value, Error> {
        rb_self.check_frozen()?;
        let request = VoteRequest {
            candidate: index(ruby, candidate)?,
            term: term.0,
            last: Position {
                index: index(ruby, last.0)?,
                term: last.1.0,
            },
        };
        let outcome = rb_self.node.borrow_mut().request_vote(request);
        Ok(answer(ruby, outcome, |received| {
            let (adopted, role) = stages(ruby, &received);
            let (vote, reason) = names::vote(received.outcome.vote);
            let reason = reason.map(|reason| ruby.to_symbol(reason));
            let response = (received.outcome.term, ruby.to_symbol(vote), reason);
            (adopted, role, response)
        }))
    }
    fn receive_vote(
        ruby: &Ruby,
        rb_self: Obj<Self>,
        peer: Id,
        term: Id,
        vote: (Symbol, Option<Symbol>),
    ) -> Result<Value, Error> {
        rb_self.check_frozen()?;
        let response = VoteResponse {
            term: term.0,
            vote: names::to_vote(ruby, vote.0, vote.1)?,
        };
        let peer = index(ruby, peer)?;
        let outcome = rb_self.node.borrow_mut().receive_vote(peer, response);
        Ok(answer(ruby, outcome, |received| {
            let (adopted, role) = stages(ruby, &received);
            (
                adopted,
                role,
                ruby.to_symbol(names::ballot(received.outcome)),
            )
        }))
    }
    fn propose(ruby: &Ruby, rb_self: Obj<Self>, command: Id) -> Result<Value, Error> {
        rb_self.check_frozen()?;
        let outcome = rb_self.node.borrow_mut().propose(command.0);
        Ok(answer(ruby, outcome, |position| position))
    }
    fn append_entries(
        ruby: &Ruby,
        rb_self: Obj<Self>,
        leader: (Id, Id),
        previous: (Id, Id),
        committed: Id,
        log: RArray,
    ) -> Result<Value, Error> {
        rb_self.check_frozen()?;
        let request = AppendRequest {
            leader: index(ruby, leader.0)?,
            term: leader.1.0,
            previous: Position {
                index: index(ruby, previous.0)?,
                term: previous.1.0,
            },
            committed: index(ruby, committed)?,
        };
        let log = entries(ruby, log)?;
        let outcome = rb_self.node.borrow_mut().append_entries(request, &log);
        Ok(answer(ruby, outcome, |received| {
            let (adopted, role) = stages(ruby, &received);
            let (append, matched) = names::append(received.outcome.append);
            let response = (received.outcome.term, ruby.to_symbol(append), matched);
            (adopted, role, response)
        }))
    }
    fn acknowledge(
        ruby: &Ruby,
        rb_self: Obj<Self>,
        peer: Id,
        term: Id,
        append: (Symbol, Option<Id>),
    ) -> Result<Value, Error> {
        rb_self.check_frozen()?;
        let response = AppendResponse {
            term: term.0,
            append: names::to_append(ruby, append.0, append.1)?,
        };
        let peer = index(ruby, peer)?;
        let outcome = rb_self.node.borrow_mut().acknowledge(peer, response);
        Ok(answer(ruby, outcome, |received| {
            let (adopted, role) = stages(ruby, &received);
            (adopted, role, ruby.to_symbol(names::ack(received.outcome)))
        }))
    }
    fn checkpoint(&self) -> (Vec<u64>, usize, u64, Option<usize>, usize, Vec<u64>) {
        let saved = self.node.borrow().checkpoint();
        let log = saved
            .entries
            .iter()
            .flat_map(|entry| [entry.term, entry.command])
            .collect();
        (
            saved.members,
            saved.own,
            saved.term,
            saved.voted_for,
            saved.committed,
            log,
        )
    }
}
