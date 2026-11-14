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
use matryona_machines::ruby::{Symbols, exact_array, integer, keep, optional};
use model::Node;
use values::numbers;

use crate::build::refused;
use crate::classes::{classes, fields};
use crate::names::{Acks, Ballots, Roles};

/// The `TypeError` for a membership that is not exactly an Array.
pub(crate) const MEMBERS_ARRAY: &str = "members must be an Array of Integers";
/// The `TypeError` for a log that is not exactly an Array.
pub(crate) const ENTRIES_ARRAY: &str = "entries must be an Array of ConsensusMachines::Entry";

/// `[members, own, term, voted_for, committed, entries]`, the Integers
/// converted, entries an Array of `Entry`.
pub(crate) type SavedFields = (RArray, usize, u64, Option<usize>, usize, RArray);

#[magnus::wrap(class = "ConsensusMachines::Raft", free_immediately, size)]
struct Wrapper {
    node: RefCell<Node>,
}

fn membership(ruby: &Ruby, members: RArray) -> Result<Vec<u64>, Error> {
    if model::MEMBERS.contains(&members.len()) {
        numbers(members, "members")
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
        own,
        term,
        voted_for,
        committed,
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
        let voted_for = optional(voted_for, "voted_for")?;
        let row = (
            exact_array(&ruby, members, MEMBERS_ARRAY)?,
            integer(own, "own")?,
            integer(term, "term")?,
            voted_for,
            integer(committed, "committed")?,
            exact_array(&ruby, entries, ENTRIES_ARRAY)?,
        );
        saved(&ruby, row).map(Self)
    }
}

impl Wrapper {
    fn new(ruby: &Ruby, members: Value, own: Value) -> Result<Self, Error> {
        let members = exact_array(ruby, members, MEMBERS_ARRAY)?;
        let own = integer(own, "own")?;
        let members = membership(ruby, members)?;
        let node = model::build(&members, own, None).ok_or_else(|| {
            Error::new(
                ruby.exception_range_error(),
                "members must be unique and own index must be in membership",
            )
        })?;
        Ok(Self {
            node: RefCell::new(Node(node)),
        })
    }
    fn restored(ruby: &Ruby, members: Value, own: Value, saved: Value) -> Result<Self, Error> {
        let members = exact_array(ruby, members, MEMBERS_ARRAY)?;
        let own = integer(own, "own")?;
        let SavedValue(saved) = SavedValue::try_convert(saved)?;
        let members = membership(ruby, members)?;
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
        Roles::symbol(ruby, rb_self.node.borrow().0.role())
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
    fn entry(rb_self: &Self, position: Value) -> Result<Option<(u64, u64)>, Error> {
        let entry = rb_self.node.borrow().0.entry(integer(position, "index")?);
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
    fn request_vote(ruby: &Ruby, rb_self: Obj<Self>, request: Value) -> Result<Value, Error> {
        rb_self.check_frozen()?;
        let request = read::vote_request(ruby, request)?;
        let classes = classes(ruby)?;
        let decide = |node: &mut Node| node.0.request_vote(request);
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
        peer: Value,
        response: Value,
    ) -> Result<Value, Error> {
        rb_self.check_frozen()?;
        let peer = integer(peer, "peer")?;
        let response = read::vote_response(ruby, response)?;
        let classes = classes(ruby)?;
        let decide = |node: &mut Node| node.0.receive_vote(peer, response);
        keep(
            ruby,
            rb_self,
            &rb_self.node,
            decide,
            |outcome| match outcome {
                Ok(received) => {
                    let ballot = Ballots::symbol(ruby, received.outcome).as_value();
                    build::received(ruby, classes, &received, ballot)
                }
                Err(refusal) => Ok(refused(ruby, refusal)),
            },
        )
    }
    /// Answers an Integer or a Symbol: no Ruby runs, so it decides in place.
    fn propose(ruby: &Ruby, rb_self: Obj<Self>, command: Value) -> Result<Value, Error> {
        rb_self.check_frozen()?;
        let command = integer(command, "command")?;
        let outcome = rb_self.node.borrow_mut().0.propose(command);
        Ok(match outcome {
            Ok(position) => position.into_value_with(ruby),
            Err(refusal) => refused(ruby, refusal),
        })
    }
    fn append_entries(
        ruby: &Ruby,
        rb_self: Obj<Self>,
        request: Value,
        log: Value,
    ) -> Result<Value, Error> {
        rb_self.check_frozen()?;
        let request = read::append_request(ruby, request)?;
        let log = read::entries(ruby, exact_array(ruby, log, ENTRIES_ARRAY)?)?;
        let classes = classes(ruby)?;
        let decide = |node: &mut Node| node.0.append_entries(request, &log);
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
        peer: Value,
        response: Value,
    ) -> Result<Value, Error> {
        rb_self.check_frozen()?;
        let peer = integer(peer, "peer")?;
        let response = read::append_response(ruby, response)?;
        let classes = classes(ruby)?;
        let decide = |node: &mut Node| node.0.acknowledge(peer, response);
        keep(
            ruby,
            rb_self,
            &rb_self.node,
            decide,
            |outcome| match outcome {
                Ok(received) => {
                    let ack = Acks::symbol(ruby, received.outcome).as_value();
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
