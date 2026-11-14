//! Native Ruby bridge for a bounded Raft model, not a consensus service.
//! Every message is read from exactly this gem's `Data` class through the
//! core's validating constructor; every answer of a mutating call is built
//! while the decision is held on a copy (clone-then-keep).
mod build;
mod classes;
mod hooks;
mod init;
mod model;
mod names;
mod read;
mod values;
use std::cell::RefCell;

use magnus::{
    Error, IntoValue, RArray, Ruby, Symbol, TryConvert, Value, prelude::*, typed_data::Obj,
};
use model::Node;
use read::{AppendRequestValue, AppendResponseValue, VoteRequestValue, VoteResponseValue};
use values::{Id, index, numbers};

use crate::build::refused;
use crate::classes::{classes, fields, keep};

/// `[members, own, term, voted_for, committed, entries]`, entries an Array
/// of `Entry`.
pub(crate) type SavedFields = (RArray, Id, Id, Option<Id>, Id, RArray);

#[magnus::wrap(class = "ConsensusMachines::Raft", free_immediately, size)]
struct Wrapper {
    node: RefCell<Node>,
}

fn membership(ruby: &Ruby, members: RArray) -> Result<Vec<u64>, Error> {
    if model::MEMBERS.contains(&members.len()) {
        numbers(members)
    } else {
        let (low, high) = (model::MEMBERS.start(), model::MEMBERS.end());
        Err(Error::new(
            ruby.exception_range_error(),
            format!("membership must contain {low}..{high} members"),
        ))
    }
}

/// A checkpoint's fields as the core's, every conversion done.
pub(crate) fn saved(ruby: &Ruby, fields: SavedFields) -> Result<model::Saved, Error> {
    let (members, own, term, voted_for, committed, log) = fields;
    Ok(model::Saved {
        members: membership(ruby, members)?,
        own: index(ruby, own)?,
        term: term.0,
        voted_for: voted_for.map(|seat| index(ruby, seat)).transpose()?,
        committed: index(ruby, committed)?,
        entries: read::entries(ruby, log)?,
    })
}

/// A `Checkpoint` argument: exactly this gem's, its fields converted.
struct SavedValue(model::Saved);

impl TryConvert for SavedValue {
    fn try_convert(value: Value) -> Result<Self, Error> {
        let ruby = Ruby::get_with(value);
        let [members, own, term, voted_for, committed, entries] =
            fields(&ruby, value, classes(&ruby)?.checkpoint)?;
        let voted_for = Option::<Id>::try_convert(voted_for)?;
        let row = (
            RArray::try_convert(members)?,
            Id::try_convert(own)?,
            Id::try_convert(term)?,
            voted_for,
            Id::try_convert(committed)?,
            RArray::try_convert(entries)?,
        );
        saved(&ruby, row).map(Self)
    }
}

impl Wrapper {
    fn new(ruby: &Ruby, members: RArray, own: Id) -> Result<Self, Error> {
        let members = membership(ruby, members)?;
        let node = model::build(&members, index(ruby, own)?, None).ok_or_else(|| {
            Error::new(
                ruby.exception_range_error(),
                "members must be unique and own index must be in membership",
            )
        })?;
        Ok(Self {
            node: RefCell::new(Node(node)),
        })
    }
    fn restored(ruby: &Ruby, members: RArray, own: Id, saved: SavedValue) -> Result<Self, Error> {
        let SavedValue(saved) = saved;
        let members = membership(ruby, members)?;
        let own = index(ruby, own)?;
        let node = model::build(&members, own, Some(&saved)).ok_or_else(|| {
            Error::new(
                ruby.exception_range_error(),
                "not a checkpoint of this membership and seat",
            )
        })?;
        Ok(Self {
            node: RefCell::new(Node(node)),
        })
    }
    fn members(&self) -> Vec<u64> {
        self.node.borrow().0.members()
    }
    fn own(&self) -> usize {
        self.node.borrow().0.own()
    }
    fn role(ruby: &Ruby, rb_self: &Self) -> Symbol {
        ruby.to_symbol(names::role(rb_self.node.borrow().0.role()))
    }
    fn term(&self) -> u64 {
        self.node.borrow().0.term()
    }
    fn committed(&self) -> usize {
        self.node.borrow().0.committed()
    }
    fn last(&self) -> (usize, u64) {
        let last = self.node.borrow().0.last();
        (last.index, last.term)
    }
    fn entry(ruby: &Ruby, rb_self: &Self, position: Id) -> Result<Option<(u64, u64)>, Error> {
        let entry = rb_self.node.borrow().0.entry(index(ruby, position)?);
        Ok(entry.map(|entry| (entry.term, entry.command)))
    }
    fn begin_election(ruby: &Ruby, rb_self: Obj<Self>) -> Result<Value, Error> {
        let classes = classes(ruby)?;
        let decide = |node: &mut Node| node.0.begin_election();
        keep(
            ruby,
            rb_self,
            &rb_self.node,
            decide,
            |outcome| match outcome {
                Ok(request) => build::vote_request(ruby, classes, request),
                Err(refusal) => Ok(refused(ruby, refusal)),
            },
        )
    }
    fn request_vote(
        ruby: &Ruby,
        rb_self: Obj<Self>,
        request: VoteRequestValue,
    ) -> Result<Value, Error> {
        let classes = classes(ruby)?;
        let decide = |node: &mut Node| node.0.request_vote(request.0);
        keep(
            ruby,
            rb_self,
            &rb_self.node,
            decide,
            |outcome| match outcome {
                Ok(received) => {
                    let response = build::vote_response(ruby, classes, received.outcome)?;
                    build::received(ruby, classes, &received, response)
                }
                Err(refusal) => Ok(refused(ruby, refusal)),
            },
        )
    }
    fn receive_vote(
        ruby: &Ruby,
        rb_self: Obj<Self>,
        peer: Id,
        response: VoteResponseValue,
    ) -> Result<Value, Error> {
        let (classes, peer) = (classes(ruby)?, index(ruby, peer)?);
        let decide = |node: &mut Node| node.0.receive_vote(peer, response.0);
        keep(
            ruby,
            rb_self,
            &rb_self.node,
            decide,
            |outcome| match outcome {
                Ok(received) => {
                    let ballot = ruby.to_symbol(names::ballot(received.outcome)).as_value();
                    build::received(ruby, classes, &received, ballot)
                }
                Err(refusal) => Ok(refused(ruby, refusal)),
            },
        )
    }
    /// Answers an Integer or a Symbol: no Ruby runs, so it decides in place.
    fn propose(ruby: &Ruby, rb_self: Obj<Self>, command: Id) -> Result<Value, Error> {
        rb_self.check_frozen()?;
        let outcome = rb_self.node.borrow_mut().0.propose(command.0);
        Ok(match outcome {
            Ok(position) => position.into_value_with(ruby),
            Err(refusal) => refused(ruby, refusal),
        })
    }
    fn append_entries(
        ruby: &Ruby,
        rb_self: Obj<Self>,
        request: AppendRequestValue,
        log: RArray,
    ) -> Result<Value, Error> {
        let (classes, log) = (classes(ruby)?, read::entries(ruby, log)?);
        let decide = |node: &mut Node| node.0.append_entries(request.0, &log);
        keep(
            ruby,
            rb_self,
            &rb_self.node,
            decide,
            |outcome| match outcome {
                Ok(received) => {
                    let response = build::append_response(ruby, classes, received.outcome)?;
                    build::received(ruby, classes, &received, response)
                }
                Err(refusal) => Ok(refused(ruby, refusal)),
            },
        )
    }
    fn acknowledge(
        ruby: &Ruby,
        rb_self: Obj<Self>,
        peer: Id,
        response: AppendResponseValue,
    ) -> Result<Value, Error> {
        let (classes, peer) = (classes(ruby)?, index(ruby, peer)?);
        let decide = |node: &mut Node| node.0.acknowledge(peer, response.0);
        keep(
            ruby,
            rb_self,
            &rb_self.node,
            decide,
            |outcome| match outcome {
                Ok(received) => {
                    let ack = ruby.to_symbol(names::ack(received.outcome)).as_value();
                    build::received(ruby, classes, &received, ack)
                }
                Err(refusal) => Ok(refused(ruby, refusal)),
            },
        )
    }
    fn checkpoint(&self) -> (Vec<u64>, usize, u64, Option<usize>, usize, Vec<u64>) {
        let saved = self.node.borrow().0.checkpoint();
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
