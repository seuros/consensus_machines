//! Private initialization boundary.
use magnus::{Error, Ruby, function, method, prelude::*};
#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let module = ruby.define_module("ConsensusMachines")?;
    module
        .define_class("Checkpoint", ruby.class_object())?
        .undef_default_alloc_func();
    let class = module.define_class("Raft", ruby.class_object())?;
    class.undef_default_alloc_func();
    class.define_singleton_method("new", function!(crate::Wrapper::new, 2))?;
    class.define_method("role", method!(crate::Wrapper::role, 0))?;
    class.define_method("term", method!(crate::Wrapper::term, 0))?;
    class.define_method("committed", method!(crate::Wrapper::committed, 0))?;
    class.define_method("last", method!(crate::Wrapper::last, 0))?;
    class.define_method("entry", method!(crate::Wrapper::entry, 1))?;
    class.define_method("begin_election", method!(crate::Wrapper::begin_election, 0))?;
    class.define_method("request_vote", method!(crate::Wrapper::request_vote, 4))?;
    class.define_method("receive_vote", method!(crate::Wrapper::receive_vote, 3))?;
    class.define_method("propose", method!(crate::Wrapper::propose, 1))?;
    class.define_method("append_entries", method!(crate::Wrapper::append_entries, 6))?;
    class.define_method("acknowledge", method!(crate::Wrapper::acknowledge, 3))?;
    class.define_method("checkpoint", method!(crate::Wrapper::checkpoint, 0))?;
    class.define_method("restore!", method!(crate::Wrapper::restore, 1))
}
