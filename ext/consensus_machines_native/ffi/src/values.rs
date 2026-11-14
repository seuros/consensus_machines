//! Strict integer decoding before any state transition.
use magnus::{Error, RArray, Value};
use matryona_machines::ruby::integer;

/// Every element as an unsigned 64-bit Integer. Converting an Integer runs no
/// Ruby code, so the array cannot change underneath the loop.
pub fn numbers(array: RArray, name: &str) -> Result<Vec<u64>, Error> {
    (0..array.len())
        .map(|offset| integer(array.entry::<Value>(offset.cast_signed())?, name))
        .collect()
}
