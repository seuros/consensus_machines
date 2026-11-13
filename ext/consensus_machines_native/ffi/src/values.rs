//! Strict integer decoding before any state transition.
use magnus::{Error, Integer, RArray, Ruby, TryConvert, Value};
#[derive(Clone, Copy)]
pub struct Id(pub u64);
impl TryConvert for Id {
    fn try_convert(value: Value) -> Result<Self, Error> {
        if Integer::from_value(value).is_none() {
            return Err(Error::new(
                Ruby::get_with(value).exception_type_error(),
                "expected an Integer",
            ));
        }
        u64::try_convert(value).map(Self)
    }
}
pub fn index(ruby: &Ruby, id: Id) -> Result<usize, Error> {
    usize::try_from(id.0)
        .map_err(|_| Error::new(ruby.exception_range_error(), "index outside native range"))
}
/// Every element as an unsigned 64-bit Integer. Converting an Integer runs no
/// Ruby code, so the array cannot change underneath the loop.
pub fn numbers(array: RArray) -> Result<Vec<u64>, Error> {
    (0..array.len())
        .map(|offset| Ok(array.entry::<Id>(offset.cast_signed())?.0))
        .collect()
}
