use rimu::{to_serde_value, SerdeValueError, SourceId, Span, Spanned, Value};
use serde::Serialize;

#[derive(Debug, Clone)]
pub enum ToRimuError {
    SerdeValue(SerdeValueError),
}

pub fn to_rimu<T>(value: T, source_id: SourceId) -> Result<Spanned<Value>, ToRimuError>
where
    T: Serialize,
{
    let rimu_serde_value = to_serde_value(value).map_err(ToRimuError::SerdeValue)?;
    let rimu_value = rimu_serde_value.with_span(Span::new(source_id, 0, 0));
    Ok(rimu_value)
}
