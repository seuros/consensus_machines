//! Private initialization boundary.
use magnus::{Error, Ruby, function, method, prelude::*};

use crate::Wrapper;
#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let module = ruby.define_module("ConsensusMachines")?;
    let class = module.define_class("Raft", ruby.class_object())?;
    class.undef_default_alloc_func();
    class.define_singleton_method("new", function!(Wrapper::new, 2))?;
    class.define_singleton_method("native_restored", function!(Wrapper::restored, 3))?;
    class.define_method("members", method!(Wrapper::members, 0))?;
    class.define_method("own", method!(Wrapper::own, 0))?;
    class.define_method("role", method!(Wrapper::role, 0))?;
    class.define_method("term", method!(Wrapper::term, 0))?;
    class.define_method("committed", method!(Wrapper::committed, 0))?;
    class.define_private_method("native_last", method!(Wrapper::last, 0))?;
    class.define_private_method("native_entry", method!(Wrapper::entry, 1))?;
    class.define_private_method("native_begin_election", method!(Wrapper::begin_election, 0))?;
    class.define_private_method("native_request_vote", method!(Wrapper::request_vote, 3))?;
    class.define_private_method("native_receive_vote", method!(Wrapper::receive_vote, 3))?;
    class.define_method("propose", method!(Wrapper::propose, 1))?;
    class.define_private_method("native_append_entries", method!(Wrapper::append_entries, 4))?;
    class.define_private_method("native_acknowledge", method!(Wrapper::acknowledge, 3))?;
    class.define_private_method("native_checkpoint", method!(Wrapper::checkpoint, 0))
}
