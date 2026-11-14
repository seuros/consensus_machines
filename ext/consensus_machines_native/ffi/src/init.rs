//! Private initialization boundary.
use magnus::{Error, Ruby, function, method, prelude::*};

use consensus_machines::Raft;

use crate::Wrapper;
use crate::model::{LOG, MEMBERS};

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let module = ruby.define_module("ConsensusMachines")?;
    crate::classes::resolve(ruby, module)?;
    crate::hooks::define(module)?;
    let class = module.define_class("Raft", ruby.class_object())?;
    class.undef_default_alloc_func();
    class.const_set("CAPACITY", Raft::<1, LOG>::CAPACITY)?;
    let members = ruby.range_new(*MEMBERS.start(), *MEMBERS.end(), false)?;
    members.as_value().freeze();
    class.const_set("MEMBERS", members)?;
    class.define_singleton_method("new", function!(Wrapper::new, 2))?;
    class.define_singleton_method("restored", function!(Wrapper::restored, 3))?;
    class.define_method("members", method!(Wrapper::members, 0))?;
    class.define_method("own", method!(Wrapper::own, 0))?;
    class.define_method("role", method!(Wrapper::role, 0))?;
    class.define_method("term", method!(Wrapper::term, 0))?;
    class.define_method("committed", method!(Wrapper::committed, 0))?;
    class.define_private_method("native_last", method!(Wrapper::last, 0))?;
    class.define_private_method("native_entry", method!(Wrapper::entry, 1))?;
    class.define_method("begin_election", method!(Wrapper::begin_election, 0))?;
    class.define_method("request_vote", method!(Wrapper::request_vote, 1))?;
    class.define_method("receive_vote", method!(Wrapper::receive_vote, 2))?;
    class.define_method("propose", method!(Wrapper::propose, 1))?;
    class.define_method("append_entries", method!(Wrapper::append_entries, 2))?;
    class.define_method("acknowledge", method!(Wrapper::acknowledge, 2))?;
    class.define_private_method("native_checkpoint", method!(Wrapper::checkpoint, 0))
}
