// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Renders values in the Rust-syntax argument format of `golem agent invoke`.
//!
//! The format follows the serde data model of the value:
//! structs → `{ field: value }`, unit variants → `Variant`,
//! newtype variants → `Variant(value)`, struct variants → `Variant({ field: value })`,
//! options → `Some(value)` / `None`, sequences → `[a, b]`. Newtype structs
//! (including `#[serde(transparent)]` ones) render as their inner value.
use std::fmt::{self, Display};

use serde::ser::{self, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderError(String);

impl Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot render invocation argument: {}", self.0)
    }
}

impl std::error::Error for RenderError {}

impl ser::Error for RenderError {
    fn custom<T: Display>(msg: T) -> Self {
        RenderError(msg.to_string())
    }
}

/// Renders `value` as a `golem agent invoke` argument.
pub fn to_rust_syntax<T: Serialize + ?Sized>(value: &T) -> Result<String, RenderError> {
    value.serialize(RustSyntax)
}

fn unsupported(what: &str) -> RenderError {
    RenderError(format!("{what} values are not supported"))
}

fn render_f64(v: f64) -> String {
    if v.is_nan() {
        "NaN".to_string()
    } else if v.is_infinite() {
        if v > 0.0 { "Infinity" } else { "-Infinity" }.to_string()
    } else {
        format!("{v:?}")
    }
}

fn render_str(v: &str) -> Result<String, RenderError> {
    serde_json::to_string(v).map_err(|e| RenderError(e.to_string()))
}

struct RustSyntax;

/// Collects rendered elements of a sequence, tuple or record.
pub struct Elements {
    items: Vec<String>,
    style: ElementStyle,
}

enum ElementStyle {
    List,
    Tuple,
    Record,
    /// A struct variant, rendered as `Variant({ field: value })`.
    RecordVariant(&'static str),
    /// A tuple variant, rendered as `Variant((a, b))`.
    TupleVariant(&'static str),
}

impl Elements {
    fn new(style: ElementStyle) -> Self {
        Elements {
            items: Vec::new(),
            style,
        }
    }

    fn push<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), RenderError> {
        self.items.push(value.serialize(RustSyntax)?);
        Ok(())
    }

    fn push_field<T: Serialize + ?Sized>(
        &mut self,
        name: &str,
        value: &T,
    ) -> Result<(), RenderError> {
        self.items
            .push(format!("{name}: {}", value.serialize(RustSyntax)?));
        Ok(())
    }

    fn finish(self) -> String {
        let body = self.items.join(", ");
        let record = |body: &str| {
            if body.is_empty() {
                "{}".to_string()
            } else {
                format!("{{ {body} }}")
            }
        };
        match self.style {
            ElementStyle::List => format!("[{body}]"),
            ElementStyle::Tuple => format!("({body})"),
            ElementStyle::Record => record(&body),
            ElementStyle::RecordVariant(variant) => format!("{variant}({})", record(&body)),
            ElementStyle::TupleVariant(variant) => format!("{variant}(({body}))"),
        }
    }
}

impl ser::SerializeSeq for Elements {
    type Ok = String;
    type Error = RenderError;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), RenderError> {
        self.push(value)
    }

    fn end(self) -> Result<String, RenderError> {
        Ok(self.finish())
    }
}

impl ser::SerializeTuple for Elements {
    type Ok = String;
    type Error = RenderError;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), RenderError> {
        self.push(value)
    }

    fn end(self) -> Result<String, RenderError> {
        Ok(self.finish())
    }
}

impl ser::SerializeTupleStruct for Elements {
    type Ok = String;
    type Error = RenderError;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), RenderError> {
        self.push(value)
    }

    fn end(self) -> Result<String, RenderError> {
        Ok(self.finish())
    }
}

impl ser::SerializeTupleVariant for Elements {
    type Ok = String;
    type Error = RenderError;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), RenderError> {
        self.push(value)
    }

    fn end(self) -> Result<String, RenderError> {
        Ok(self.finish())
    }
}

impl ser::SerializeStruct for Elements {
    type Ok = String;
    type Error = RenderError;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), RenderError> {
        self.push_field(key, value)
    }

    fn end(self) -> Result<String, RenderError> {
        Ok(self.finish())
    }
}

impl ser::SerializeStructVariant for Elements {
    type Ok = String;
    type Error = RenderError;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), RenderError> {
        self.push_field(key, value)
    }

    fn end(self) -> Result<String, RenderError> {
        Ok(self.finish())
    }
}

impl ser::SerializeMap for Elements {
    type Ok = String;
    type Error = RenderError;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, _key: &T) -> Result<(), RenderError> {
        Err(unsupported("map"))
    }

    fn serialize_value<T: Serialize + ?Sized>(&mut self, _value: &T) -> Result<(), RenderError> {
        Err(unsupported("map"))
    }

    fn end(self) -> Result<String, RenderError> {
        Err(unsupported("map"))
    }
}

impl ser::Serializer for RustSyntax {
    type Ok = String;
    type Error = RenderError;
    type SerializeSeq = Elements;
    type SerializeTuple = Elements;
    type SerializeTupleStruct = Elements;
    type SerializeTupleVariant = Elements;
    type SerializeMap = Elements;
    type SerializeStruct = Elements;
    type SerializeStructVariant = Elements;

    fn serialize_bool(self, v: bool) -> Result<String, RenderError> {
        Ok(v.to_string())
    }

    fn serialize_i8(self, v: i8) -> Result<String, RenderError> {
        Ok(v.to_string())
    }

    fn serialize_i16(self, v: i16) -> Result<String, RenderError> {
        Ok(v.to_string())
    }

    fn serialize_i32(self, v: i32) -> Result<String, RenderError> {
        Ok(v.to_string())
    }

    fn serialize_i64(self, v: i64) -> Result<String, RenderError> {
        Ok(v.to_string())
    }

    fn serialize_u8(self, v: u8) -> Result<String, RenderError> {
        Ok(v.to_string())
    }

    fn serialize_u16(self, v: u16) -> Result<String, RenderError> {
        Ok(v.to_string())
    }

    fn serialize_u32(self, v: u32) -> Result<String, RenderError> {
        Ok(v.to_string())
    }

    fn serialize_u64(self, v: u64) -> Result<String, RenderError> {
        Ok(v.to_string())
    }

    fn serialize_f32(self, v: f32) -> Result<String, RenderError> {
        Ok(render_f64(f64::from(v)))
    }

    fn serialize_f64(self, v: f64) -> Result<String, RenderError> {
        Ok(render_f64(v))
    }

    fn serialize_char(self, v: char) -> Result<String, RenderError> {
        Ok(match v {
            '\'' => "'\\''".to_string(),
            '\\' => "'\\\\'".to_string(),
            c => format!("'{c}'"),
        })
    }

    fn serialize_str(self, v: &str) -> Result<String, RenderError> {
        render_str(v)
    }

    fn serialize_bytes(self, _v: &[u8]) -> Result<String, RenderError> {
        Err(unsupported("byte"))
    }

    fn serialize_none(self) -> Result<String, RenderError> {
        Ok("None".to_string())
    }

    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<String, RenderError> {
        Ok(format!("Some({})", value.serialize(RustSyntax)?))
    }

    fn serialize_unit(self) -> Result<String, RenderError> {
        Ok("()".to_string())
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<String, RenderError> {
        Ok("{}".to_string())
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
    ) -> Result<String, RenderError> {
        Ok(variant.to_string())
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<String, RenderError> {
        value.serialize(RustSyntax)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<String, RenderError> {
        Ok(format!("{variant}({})", value.serialize(RustSyntax)?))
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Elements, RenderError> {
        Ok(Elements::new(ElementStyle::List))
    }

    fn serialize_tuple(self, _len: usize) -> Result<Elements, RenderError> {
        Ok(Elements::new(ElementStyle::Tuple))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Elements, RenderError> {
        Ok(Elements::new(ElementStyle::Tuple))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Elements, RenderError> {
        Ok(Elements::new(ElementStyle::TupleVariant(variant)))
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Elements, RenderError> {
        Err(unsupported("map"))
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Elements, RenderError> {
        Ok(Elements::new(ElementStyle::Record))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Elements, RenderError> {
        Ok(Elements::new(ElementStyle::RecordVariant(variant)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_lib::{
        AggregationConfig, AggregationFunction, CompareOp, Event, EventColumnName, EventField,
        EventPredicate, EventValue, TimeLineOp,
    };

    type TestResult = Result<(), RenderError>;

    #[test]
    fn scalars() -> TestResult {
        assert_eq!(to_rust_syntax(&42u64)?, "42");
        assert_eq!(to_rust_syntax(&-3i64)?, "-3");
        assert_eq!(to_rust_syntax(&1.0f64)?, "1.0");
        assert_eq!(to_rust_syntax(&f64::NAN)?, "NaN");
        assert_eq!(to_rust_syntax(&f64::NEG_INFINITY)?, "-Infinity");
        assert_eq!(to_rust_syntax(&true)?, "true");
        assert_eq!(
            to_rust_syntax("a \"quoted\"\nline")?,
            r#""a \"quoted\"\nline""#
        );
        Ok(())
    }

    #[test]
    fn options_and_lists() -> TestResult {
        assert_eq!(to_rust_syntax(&None::<u64>)?, "None");
        assert_eq!(to_rust_syntax(&Some(5u64))?, "Some(5)");
        assert_eq!(to_rust_syntax(&vec![1u64, 2])?, "[1, 2]");
        assert_eq!(to_rust_syntax(&Vec::<u64>::new())?, "[]");
        Ok(())
    }

    #[test]
    fn event() -> TestResult {
        let event = Event {
            time: 100,
            fields: vec![EventField::new(
                "playerStateChange",
                EventValue::StringValue("play".into()),
            )],
        };
        assert_eq!(
            to_rust_syntax(&event)?,
            r#"{ time: 100, fields: [{ column: "playerStateChange", value: StringValue("play") }] }"#
        );
        Ok(())
    }

    #[test]
    fn timeline_with_struct_variants() -> TestResult {
        let timeline = TimeLineOp::compare(
            CompareOp::EqualTo,
            TimeLineOp::TlLatestEventToState(EventColumnName::new("state")),
            EventValue::StringValue("buffer".into()),
        );
        assert_eq!(
            to_rust_syntax(&timeline)?,
            r#"Compare({ op: EqualTo, timeline: TlLatestEventToState("state"), value: StringValue("buffer") })"#
        );

        let within = TimeLineOp::TlHasExistedWithin {
            predicate: EventPredicate::equals("x", EventValue::IntValue(1)),
            within: 5,
        };
        assert_eq!(
            to_rust_syntax(&within)?,
            r#"TlHasExistedWithin({ predicate: Equals({ column: "x", value: IntValue(1) }), within: 5 })"#
        );
        Ok(())
    }

    #[test]
    fn aggregation_config() -> TestResult {
        let config = Some(AggregationConfig {
            group_by_column: EventColumnName::new("cdn"),
            functions: vec![AggregationFunction::Count, AggregationFunction::Avg],
        });
        assert_eq!(
            to_rust_syntax(&config)?,
            r#"Some({ group_by_column: "cdn", functions: [Count, Avg] })"#
        );
        Ok(())
    }

    #[test]
    fn maps_are_rejected() {
        let map: std::collections::BTreeMap<String, u64> = [("a".to_string(), 1)].into();
        assert!(to_rust_syntax(&map).is_err());
    }
}
