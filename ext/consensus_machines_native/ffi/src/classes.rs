//! The `Data` classes, resolved once at load, the rule every value read from
//! Ruby passes, and clone-then-keep for every call whose answer runs Ruby.
//!
//! A value read back must be an instance of exactly this gem's class: the
//! constant of a subclass leaks past the seal and `Data#initialize` can be
//! bound directly, so `is_a?` is not enough. Its fields are read straight
//! from the struct (no reader runs) and rebuilt through the core constructor,
//! which `Marshal.load` skipped.
use std::cell::RefCell;
use std::sync::OnceLock;

use magnus::rb_sys::AsRawValue;
use magnus::typed_data::Obj;
use magnus::value::Opaque;
use magnus::{Error, RClass, RModule, RStruct, Ruby, TypedData, Value, prelude::*};

/// Every `Data` class the binding checks or builds.
pub struct Classes {
    pub position: Opaque<RClass>,
    pub entry: Opaque<RClass>,
    pub vote_request: Opaque<RClass>,
    pub denied: Opaque<RClass>,
    pub vote_response: Opaque<RClass>,
    pub append_request: Opaque<RClass>,
    pub matched: Opaque<RClass>,
    pub append_response: Opaque<RClass>,
    pub received: Opaque<RClass>,
    pub checkpoint: Opaque<RClass>,
}

static CLASSES: OnceLock<Classes> = OnceLock::new();

fn setup_error(ruby: &Ruby) -> Error {
    Error::new(
        ruby.exception_runtime_error(),
        "load consensus_machines, not the extension alone",
    )
}

/// Resolve the classes `consensus_machines.rb` defined before requiring the
/// extension. Each is registered with the GC, which keeps it alive and
/// pinned: compaction must not move a class the binding holds by address.
pub fn resolve(ruby: &Ruby, module: RModule) -> Result<(), Error> {
    let class = |name: &str| -> Result<Opaque<RClass>, Error> {
        let class: RClass = module.const_get(name).map_err(|_| setup_error(ruby))?;
        ruby.gc_register_mark_object(class);
        Ok(Opaque::from(class))
    };
    let classes = Classes {
        position: class("Position")?,
        entry: class("Entry")?,
        vote_request: class("VoteRequest")?,
        denied: class("Denied")?,
        vote_response: class("VoteResponse")?,
        append_request: class("AppendRequest")?,
        matched: class("Matched")?,
        append_response: class("AppendResponse")?,
        received: class("Received")?,
        checkpoint: class("Checkpoint")?,
    };
    CLASSES.set(classes).map_err(|_| setup_error(ruby))
}

/// The classes resolved at load.
pub fn classes(ruby: &Ruby) -> Result<&'static Classes, Error> {
    CLASSES.get().ok_or_else(|| setup_error(ruby))
}

/// Whether `value` is an instance of exactly `class`, never a subclass, by
/// pointer equality (no `==` the caller could redefine runs). A singleton
/// class on the instance itself does not change what it is.
pub fn is(ruby: &Ruby, value: Value, class: Opaque<RClass>) -> bool {
    let (class, actual) = (ruby.get_inner(class), value.class());
    if actual.as_raw() == class.as_raw() {
        return true;
    }
    RStruct::from_value(value).is_some_and(|data| {
        data.singleton_class()
            .is_ok_and(|own| own.as_raw() == actual.as_raw())
            && actual
                .superclass()
                .is_ok_and(|parent| parent.as_raw() == class.as_raw())
    })
}

/// The fields of `value`, in order, read from the struct: `TypeError` unless
/// `value` is an instance of exactly `class` (a subclass included).
pub fn fields<const N: usize>(
    ruby: &Ruby,
    value: Value,
    class: Opaque<RClass>,
) -> Result<[Value; N], Error> {
    let exact = is(ruby, value, class);
    let class = ruby.get_inner(class);
    let data = RStruct::from_value(value)
        .filter(|_| exact)
        .ok_or_else(|| {
            Error::new(
                ruby.exception_type_error(),
                format!("expected a {class} (exactly; it cannot be subclassed)"),
            )
        })?;
    let mut out = [ruby.qnil().as_value(); N];
    for (index, slot) in out.iter_mut().enumerate() {
        *slot = data.aref(index)?;
    }
    Ok(out)
}

/// Clone-then-keep: `decide` runs on a copy of the core and `build` turns its
/// answer into Ruby, with no borrow held. The copy is stored only when the
/// receiver is still unfrozen (`FrozenError`) and unchanged (`RuntimeError`:
/// a re-entrant call changed it); otherwise nothing is applied.
pub fn keep<T, C, A>(
    ruby: &Ruby,
    this: Obj<T>,
    core: &RefCell<C>,
    decide: impl FnOnce(&mut C) -> A,
    build: impl FnOnce(A) -> Result<Value, Error>,
) -> Result<Value, Error>
where
    T: TypedData,
    C: Clone + PartialEq,
{
    this.check_frozen()?;
    let before = core.borrow().clone();
    let mut after = before.clone();
    let answer = build(decide(&mut after))?;
    this.check_frozen()?;
    if *core.borrow() != before {
        return Err(Error::new(
            ruby.exception_runtime_error(),
            "the receiver changed while the answer was being built; the call was not applied",
        ));
    }
    *core.borrow_mut() = after;
    Ok(answer)
}
