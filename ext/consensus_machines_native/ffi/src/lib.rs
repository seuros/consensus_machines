//! Native Ruby bridge for a bounded Raft model, not a consensus service.
mod init;
mod model;
mod values;
use consensus_machines::{AppendRequest, Entry, Position, Role, Vote, VoteRequest, VoteResponse};
use magnus::{Error, RArray, Ruby, Value, prelude::*, typed_data::Obj};
use std::{cell::RefCell, sync::Arc};
use values::{Id, index};

struct State {
    node: Box<dyn model::Model>,
    revision: u64,
}
#[magnus::wrap(class = "ConsensusMachines::Checkpoint", free_immediately, size)]
struct Saved {
    owner: Arc<()>,
    revision: u64,
    checkpoint: Box<dyn model::Durable>,
}
#[magnus::wrap(class = "ConsensusMachines::Raft", free_immediately, size)]
struct Wrapper {
    owner: Arc<()>,
    state: RefCell<State>,
}
const fn role(role: Role) -> &'static str {
    match role {
        Role::Follower => "follower",
        Role::Candidate => "candidate",
        Role::Leader => "leader",
        _ => "unknown",
    }
}
impl Wrapper {
    fn new(ruby: &Ruby, members: RArray, own: Id) -> Result<Self, Error> {
        let count = members.len();
        if !(1..=9).contains(&count) {
            return Err(Error::new(
                ruby.exception_arg_error(),
                "membership must contain 1..9 actual members",
            ));
        }
        let mut ids = [0; 9];
        for (offset, id) in ids[..count].iter_mut().enumerate() {
            *id = members.entry::<Id>(offset.cast_signed())?.0;
        }
        let node = model::new(&ids[..count], index(ruby, own)?).ok_or_else(|| {
            Error::new(
                ruby.exception_arg_error(),
                "members must be unique and own index must be in membership",
            )
        })?;
        Ok(Self {
            owner: Arc::new(()),
            state: RefCell::new(State { node, revision: 0 }),
        })
    }
    fn next_revision(&self, ruby: &Ruby) -> Result<u64, Error> {
        let revision = self.state.borrow().revision.checked_add(1);
        revision.ok_or_else(|| Error::new(ruby.exception_range_error(), "revision exhausted"))
    }
    fn role(&self) -> &'static str {
        role(self.state.borrow().node.role())
    }
    fn term(&self) -> u64 {
        self.state.borrow().node.term()
    }
    fn committed(&self) -> usize {
        self.state.borrow().node.committed()
    }
    fn last(&self) -> (usize, u64) {
        let last = self.state.borrow().node.last();
        (last.index, last.term)
    }
    fn entry(ruby: &Ruby, rb_self: &Self, position: Id) -> Result<Option<(u64, u64)>, Error> {
        let index = index(ruby, position)?;
        Ok(rb_self
            .state
            .borrow()
            .node
            .entry(index)
            .map(|entry| (entry.term, entry.command)))
    }
    fn begin_election(
        ruby: &Ruby,
        rb_self: Obj<Self>,
    ) -> Result<Option<(usize, u64, usize, u64)>, Error> {
        rb_self.check_frozen()?;
        let revision = rb_self.next_revision(ruby)?;
        let request = {
            let mut state = rb_self.state.borrow_mut();
            state.revision = revision;
            state.node.begin_election()
        };
        Ok(request.map(|request| {
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
        last_index: Id,
        last_term: Id,
    ) -> Result<(u64, bool), Error> {
        rb_self.check_frozen()?;
        let request = VoteRequest {
            candidate: index(ruby, candidate)?,
            term: term.0,
            last: Position {
                index: index(ruby, last_index)?,
                term: last_term.0,
            },
        };
        let revision = rb_self.next_revision(ruby)?;
        let response = {
            let mut state = rb_self.state.borrow_mut();
            state.revision = revision;
            state.node.request_vote(request)
        };
        Ok((response.term, response.vote == Vote::Granted))
    }
    fn receive_vote(
        ruby: &Ruby,
        rb_self: Obj<Self>,
        peer: Id,
        term: Id,
        ballot: Id,
    ) -> Result<&'static str, Error> {
        rb_self.check_frozen()?;
        let peer = index(ruby, peer)?;
        let vote = match ballot.0 {
            0 => Vote::Denied,
            1 => Vote::Granted,
            _ => {
                return Err(Error::new(
                    ruby.exception_arg_error(),
                    "ballot must be Integer 0 or 1",
                ));
            }
        };
        let revision = rb_self.next_revision(ruby)?;
        let result = {
            let mut state = rb_self.state.borrow_mut();
            state.revision = revision;
            state
                .node
                .receive_vote(peer, VoteResponse { term: term.0, vote })
        };
        Ok(role(result))
    }
    fn propose(ruby: &Ruby, rb_self: Obj<Self>, command: Id) -> Result<Option<usize>, Error> {
        rb_self.check_frozen()?;
        let revision = rb_self.next_revision(ruby)?;
        let result = {
            let mut state = rb_self.state.borrow_mut();
            state.revision = revision;
            state.node.propose(command.0)
        };
        Ok(result)
    }
    // Mirrors AppendEntries metadata without Ruby policy objects.
    #[allow(clippy::too_many_arguments)]
    fn append_entries(
        ruby: &Ruby,
        rb_self: Obj<Self>,
        leader: Id,
        term: Id,
        previous_index: Id,
        previous_term: Id,
        committed: Id,
        entries: RArray,
    ) -> Result<(u64, Option<usize>), Error> {
        rb_self.check_frozen()?;
        let count = entries.len();
        if count > 64 {
            return Err(Error::new(ruby.exception_arg_error(), "at most 64 entries"));
        }
        let mut decoded = [Entry {
            term: 0,
            command: 0,
        }; 64];
        for (offset, entry) in decoded[..count].iter_mut().enumerate() {
            // RArray::try_convert can invoke to_ary, permitting reentrant input
            // mutation or freezing. Nested entries must already be Arrays.
            let value = entries.entry::<Value>(offset.cast_signed())?;
            let pair = RArray::from_value(value)
                .ok_or_else(|| Error::new(ruby.exception_type_error(), "entry must be an Array"))?;
            if pair.len() != 2 {
                return Err(Error::new(
                    ruby.exception_arg_error(),
                    "entry must be [term, command_id]",
                ));
            }
            *entry = Entry {
                term: pair.entry::<Id>(0)?.0,
                command: pair.entry::<Id>(1)?.0,
            };
        }
        let request = AppendRequest {
            leader: index(ruby, leader)?,
            term: term.0,
            previous: Position {
                index: index(ruby, previous_index)?,
                term: previous_term.0,
            },
            committed: index(ruby, committed)?,
        };
        rb_self.check_frozen()?;
        let revision = rb_self.next_revision(ruby)?;
        let response = {
            let mut state = rb_self.state.borrow_mut();
            state.revision = revision;
            state.node.append_entries(request, &decoded[..count])
        };
        Ok((response.term, response.matched))
    }
    fn acknowledge(
        ruby: &Ruby,
        rb_self: Obj<Self>,
        peer: Id,
        term: Id,
        position: Id,
    ) -> Result<bool, Error> {
        rb_self.check_frozen()?;
        let peer = index(ruby, peer)?;
        let position = index(ruby, position)?;
        let revision = rb_self.next_revision(ruby)?;
        let result = {
            let mut state = rb_self.state.borrow_mut();
            state.revision = revision;
            state.node.acknowledge(peer, term.0, position)
        };
        Ok(result)
    }
    fn checkpoint(&self) -> Saved {
        let (checkpoint, revision) = {
            let state = self.state.borrow();
            (state.node.checkpoint(), state.revision)
        };
        Saved {
            owner: Arc::clone(&self.owner),
            revision,
            checkpoint,
        }
    }
    fn restore(ruby: &Ruby, rb_self: Obj<Self>, saved: &Saved) -> Result<bool, Error> {
        rb_self.check_frozen()?;
        if !Arc::ptr_eq(&rb_self.owner, &saved.owner)
            || rb_self.state.borrow().revision != saved.revision
        {
            return Err(Error::new(
                ruby.exception_arg_error(),
                "cross-instance or stale checkpoint",
            ));
        }
        let revision = rb_self.next_revision(ruby)?;
        let node = saved
            .checkpoint
            .restore()
            .ok_or_else(|| Error::new(ruby.exception_arg_error(), "invalid checkpoint"))?;
        *rb_self.state.borrow_mut() = State { node, revision };
        Ok(true)
    }
}
