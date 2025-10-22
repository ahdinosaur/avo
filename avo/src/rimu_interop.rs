use rimu::{Spanned, Value};

pub trait FromRimu {
    type Error: Clone;

    fn from_rimu(value: Value) -> Result<Self, Self::Error>
    where
        Self: Sized;

    fn from_rimu_spanned(value: Spanned<Value>) -> Result<Spanned<Self>, Spanned<Self::Error>>
    where
        Self: Sized + Clone,
    {
        let (value, span) = value.take();
        match Self::from_rimu(value) {
            Ok(this) => Ok(Spanned::new(this, span)),
            Err(error) => Err(Spanned::new(error, span)),
        }
    }
}
