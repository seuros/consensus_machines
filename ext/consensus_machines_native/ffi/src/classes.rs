//! The `Data` classes, resolved once at load, and the rule every value read
//! from Ruby passes. Clone-then-keep is `matryona_machines::ruby::keep`.
//!
//! A value read back must be an instance of exactly this gem's class: the
//! constant of a subclass leaks past the seal and `Data#initialize` can be
//! bound directly, so `is_a?` is not enough. Its fields are read straight
//! from the struct (no reader runs) and rebuilt through the core constructor,
//! which `Marshal.load` skipped.
use magnus::{Error, RModule, Ruby, Value};
use matryona_machines::ruby::{Class, Registry, exact_fields};

/// A `Data` class the binding reads, and the `TypeError` message for any
/// value that is not exactly an instance of it.
#[derive(Clone, Copy)]
pub struct Data {
    pub class: Class,
    expected: &'static str,
}

/// Every `Data` class the binding checks or builds.
pub struct Classes {
    pub position: Data,
    pub entry: Data,
    pub vote_request: Data,
    pub denied: Data,
    pub vote_response: Data,
    pub append_request: Data,
    pub matched: Data,
    pub append_response: Data,
    pub received: Data,
    pub checkpoint: Data,
}

static CLASSES: Registry<Classes> = Registry::new("consensus_machines");

/// Resolve the classes `consensus_machines.rb` defined before requiring the
/// extension. A missing one is a `NameError` (the extension was loaded
/// without the gem). Each is registered with the GC, which keeps it alive
/// and pinned: compaction must not move a class the binding holds.
pub fn resolve(ruby: &Ruby, module: RModule) -> Result<(), Error> {
    macro_rules! data {
        ($name:literal) => {
            Data {
                class: Class::resolve(ruby, module, $name)?,
                expected: concat!(
                    "expected a ConsensusMachines::",
                    $name,
                    " (exactly; it cannot be subclassed)"
                ),
            }
        };
    }
    let classes = Classes {
        position: data!("Position"),
        entry: data!("Entry"),
        vote_request: data!("VoteRequest"),
        denied: data!("Denied"),
        vote_response: data!("VoteResponse"),
        append_request: data!("AppendRequest"),
        matched: data!("Matched"),
        append_response: data!("AppendResponse"),
        received: data!("Received"),
        checkpoint: data!("Checkpoint"),
    };
    CLASSES.set(ruby, classes).map(drop)
}

/// The classes resolved at load.
pub fn classes(ruby: &Ruby) -> Result<&'static Classes, Error> {
    CLASSES.get(ruby)
}

/// The fields of `value`, in order, read from the struct: `TypeError` unless
/// `value` is an instance of exactly `data`'s class (a subclass included).
pub fn fields<const N: usize>(ruby: &Ruby, value: Value, data: Data) -> Result<[Value; N], Error> {
    exact_fields(ruby, value, data.class.get(ruby), data.expected)
}
