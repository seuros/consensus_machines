//! Strict integer decoding before any state transition.
use magnus::{Error, Integer, Ruby, TryConvert, Value};
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
