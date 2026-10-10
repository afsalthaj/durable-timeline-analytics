// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Decodes the `resultJson` document of `golem agent invoke --format json`.
//!
//! Golem encodes results positionally: record fields and variant cases are
//! indices into a schema graph shipped alongside the value. This module joins
//! the two into ordinary named JSON in serde's default (externally tagged)
//! layout, so results deserialize straight into the shared Rust types.
use std::collections::HashMap;
use std::fmt::{self, Display};

use serde::Deserialize;
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError(String);

impl Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot decode invocation result: {}", self.0)
    }
}

impl std::error::Error for DecodeError {}

fn mismatch(expected: &str, found: &WireValue) -> DecodeError {
    DecodeError(format!("expected {expected}, found {found:?}"))
}

/// A `resultJson` document: the value and the schema describing it.
#[derive(Debug, Deserialize)]
pub struct TypedResult {
    graph: SchemaGraph,
    value: WireValue,
}

#[derive(Debug, Deserialize)]
struct SchemaGraph {
    #[serde(default)]
    defs: Vec<SchemaDef>,
    root: SchemaType,
}

#[derive(Debug, Deserialize)]
struct SchemaDef {
    id: String,
    body: SchemaType,
}

#[derive(Debug, Deserialize)]
struct NamedField {
    name: String,
    body: SchemaType,
}

#[derive(Debug, Deserialize)]
struct VariantCase {
    name: String,
    payload: Option<SchemaType>,
}

#[derive(Debug, Deserialize)]
struct ResultSpec {
    ok: Option<Box<SchemaType>>,
    err: Option<Box<SchemaType>>,
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "RawSchemaType")]
enum SchemaType {
    Record {
        fields: Vec<NamedField>,
    },
    Variant {
        cases: Vec<VariantCase>,
    },
    Enum {
        cases: Vec<String>,
    },
    Option {
        inner: Box<SchemaType>,
    },
    List {
        element: Box<SchemaType>,
    },
    Tuple {
        elements: Vec<SchemaType>,
    },
    Result {
        spec: ResultSpec,
    },
    Ref {
        id: String,
    },
    /// Scalars; they carry no structure needed for decoding.
    Primitive,
}

/// A schema node before its `kind` is interpreted.
#[derive(Deserialize)]
struct RawSchemaType {
    kind: String,
    #[serde(default)]
    value: Value,
}

#[derive(Deserialize)]
struct RecordSchema {
    fields: Vec<NamedField>,
}

#[derive(Deserialize)]
struct VariantSchema {
    cases: Vec<VariantCase>,
}

#[derive(Deserialize)]
struct EnumSchema {
    cases: Vec<String>,
}

#[derive(Deserialize)]
struct OptionSchema {
    inner: Box<SchemaType>,
}

#[derive(Deserialize)]
struct ListSchema {
    element: Box<SchemaType>,
}

#[derive(Deserialize)]
struct TupleSchema {
    elements: Vec<SchemaType>,
}

#[derive(Deserialize)]
struct ResultSchema {
    spec: ResultSpec,
}

#[derive(Deserialize)]
struct RefSchema {
    id: String,
}

impl TryFrom<RawSchemaType> for SchemaType {
    type Error = serde_json::Error;

    fn try_from(raw: RawSchemaType) -> Result<Self, Self::Error> {
        let value = raw.value;
        Ok(match raw.kind.as_str() {
            "record" => {
                let RecordSchema { fields } = serde_json::from_value(value)?;
                SchemaType::Record { fields }
            }
            "variant" => {
                let VariantSchema { cases } = serde_json::from_value(value)?;
                SchemaType::Variant { cases }
            }
            "enum" => {
                let EnumSchema { cases } = serde_json::from_value(value)?;
                SchemaType::Enum { cases }
            }
            "option" => {
                let OptionSchema { inner } = serde_json::from_value(value)?;
                SchemaType::Option { inner }
            }
            "list" => {
                let ListSchema { element } = serde_json::from_value(value)?;
                SchemaType::List { element }
            }
            "tuple" => {
                let TupleSchema { elements } = serde_json::from_value(value)?;
                SchemaType::Tuple { elements }
            }
            "result" => {
                let ResultSchema { spec } = serde_json::from_value(value)?;
                SchemaType::Result { spec }
            }
            "ref" => {
                let RefSchema { id } = serde_json::from_value(value)?;
                SchemaType::Ref { id }
            }
            _ => SchemaType::Primitive,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ResultTag {
    Ok,
    Err,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
enum WireValue {
    Bool(bool),
    S8(i64),
    S16(i64),
    S32(i64),
    S64(i64),
    U8(u64),
    U16(u64),
    U32(u64),
    U64(u64),
    F32(Value),
    F64(Value),
    Char(char),
    String(String),
    Record {
        fields: Vec<WireValue>,
    },
    Variant {
        case: usize,
        payload: Option<Box<WireValue>>,
    },
    Enum {
        case: usize,
    },
    Tuple {
        elements: Vec<WireValue>,
    },
    List {
        elements: Vec<WireValue>,
    },
    Option {
        inner: Option<Box<WireValue>>,
    },
    Result {
        tag: ResultTag,
        value: Option<Box<WireValue>>,
    },
}

impl TypedResult {
    /// The result as named JSON, e.g. `{"Ok": {"t1": 5, "t2": null, ...}}`.
    pub fn to_named_json(&self) -> Result<Value, DecodeError> {
        let defs: HashMap<&str, &SchemaType> = self
            .graph
            .defs
            .iter()
            .map(|def| (def.id.as_str(), &def.body))
            .collect();
        Decoder { defs }.decode(&self.graph.root, &self.value)
    }
}

struct Decoder<'a> {
    defs: HashMap<&'a str, &'a SchemaType>,
}

impl<'a> Decoder<'a> {
    fn resolve(&self, schema: &'a SchemaType) -> Result<&'a SchemaType, DecodeError> {
        let mut current = schema;
        while let SchemaType::Ref { id } = current {
            current = self
                .defs
                .get(id.as_str())
                .copied()
                .ok_or_else(|| DecodeError(format!("unknown schema reference '{id}'")))?;
        }
        Ok(current)
    }

    fn decode(&self, schema: &'a SchemaType, value: &WireValue) -> Result<Value, DecodeError> {
        let schema = self.resolve(schema)?;
        match (schema, value) {
            (SchemaType::Record { fields }, WireValue::Record { fields: values }) => {
                if fields.len() != values.len() {
                    return Err(DecodeError(format!(
                        "record has {} fields but schema declares {}",
                        values.len(),
                        fields.len()
                    )));
                }
                let object = fields
                    .iter()
                    .zip(values)
                    .map(|(field, value)| {
                        Ok((field.name.clone(), self.decode(&field.body, value)?))
                    })
                    .collect::<Result<Map<String, Value>, DecodeError>>()?;
                Ok(Value::Object(object))
            }
            (SchemaType::Variant { cases }, WireValue::Variant { case, payload }) => {
                let case_schema = cases
                    .get(*case)
                    .ok_or_else(|| DecodeError(format!("variant case {case} out of range")))?;
                match (&case_schema.payload, payload) {
                    (Some(payload_schema), Some(payload)) => Ok(tagged(
                        &case_schema.name,
                        self.decode(payload_schema, payload)?,
                    )),
                    (None, None) => Ok(Value::String(case_schema.name.clone())),
                    _ => Err(DecodeError(format!(
                        "payload of variant case '{}' does not match its schema",
                        case_schema.name
                    ))),
                }
            }
            (SchemaType::Enum { cases }, WireValue::Enum { case }) => cases
                .get(*case)
                .map(|name| Value::String(name.clone()))
                .ok_or_else(|| DecodeError(format!("enum case {case} out of range"))),
            (SchemaType::Option { inner }, WireValue::Option { inner: value }) => match value {
                Some(value) => self.decode(inner, value),
                None => Ok(Value::Null),
            },
            (SchemaType::List { element }, WireValue::List { elements }) => elements
                .iter()
                .map(|value| self.decode(element, value))
                .collect::<Result<Vec<Value>, DecodeError>>()
                .map(Value::Array),
            (SchemaType::Tuple { elements: schemas }, WireValue::Tuple { elements }) => schemas
                .iter()
                .zip(elements)
                .map(|(schema, value)| self.decode(schema, value))
                .collect::<Result<Vec<Value>, DecodeError>>()
                .map(Value::Array),
            (SchemaType::Result { spec }, WireValue::Result { tag, value }) => {
                let (name, schema) = match tag {
                    ResultTag::Ok => ("Ok", &spec.ok),
                    ResultTag::Err => ("Err", &spec.err),
                };
                let payload = match (schema, value) {
                    (Some(schema), Some(value)) => self.decode(schema, value)?,
                    _ => Value::Null,
                };
                Ok(tagged(name, payload))
            }
            (SchemaType::Primitive, value) => primitive(value),
            (_, value) => Err(mismatch("a value matching the schema", value)),
        }
    }
}

fn tagged(name: &str, payload: Value) -> Value {
    let mut object = Map::new();
    object.insert(name.to_string(), payload);
    Value::Object(object)
}

fn primitive(value: &WireValue) -> Result<Value, DecodeError> {
    match value {
        WireValue::Bool(b) => Ok(Value::Bool(*b)),
        WireValue::S8(i) | WireValue::S16(i) | WireValue::S32(i) | WireValue::S64(i) => {
            Ok(Value::from(*i))
        }
        WireValue::U8(u) | WireValue::U16(u) | WireValue::U32(u) | WireValue::U64(u) => {
            Ok(Value::from(*u))
        }
        WireValue::F32(f) | WireValue::F64(f) => Ok(f.clone()),
        WireValue::Char(c) => Ok(Value::String(c.to_string())),
        WireValue::String(s) => Ok(Value::String(s.clone())),
        other => Err(mismatch("a primitive", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_lib::{AggregationResult, EventValue, StatePoint, TimelineError};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// `resultJson` of `get_derived_result`, captured from Golem 1.6.0-rc1
    /// (schema defs not reachable from the root are trimmed).
    const DERIVED_RESULT: &str = r#"{"graph":{"defs":[
        {"body":{"kind":"variant","value":{"cases":[{"name":"StringValue","payload":{"kind":"string","value":{}}},{"name":"IntValue","payload":{"kind":"s64","value":{}}},{"name":"FloatValue","payload":{"kind":"f64","value":{}}},{"name":"BoolValue","payload":{"kind":"bool","value":{}}}]}},"id":"common_lib.event.EventValue","name":"EventValue"},
        {"body":{"kind":"record","value":{"fields":[{"body":{"kind":"u64","value":{}},"name":"t1"},{"body":{"kind":"option","value":{"inner":{"kind":"u64","value":{}}}},"name":"t2"},{"body":{"kind":"ref","value":{"id":"common_lib.event.EventValue"}},"name":"value"}]}},"id":"timeline_core.types.StatePoint","name":"StatePoint"},
        {"body":{"kind":"enum","value":{"cases":["NotInitialized"]}},"id":"timeline_core.types.TimelineError","name":"TimelineError"}],
        "root":{"kind":"result","value":{"spec":{"err":{"kind":"ref","value":{"id":"timeline_core.types.TimelineError"}},"ok":{"kind":"option","value":{"inner":{"kind":"ref","value":{"id":"timeline_core.types.StatePoint"}}}}}}}},
        "value":{"kind":"result","value":{"tag":"ok","value":{"kind":"option","value":{"inner":{"kind":"record","value":{"fields":[{"kind":"u64","value":5},{"kind":"option","value":{"inner":null}},{"kind":"variant","value":{"case":3,"payload":{"kind":"bool","value":true}}}]}}}}}}}"#;

    /// `resultJson` of `get_aggregation_result`, captured from Golem 1.6.0-rc1.
    const AGGREGATION_RESULT: &str = r#"{"graph":{"defs":[
        {"body":{"kind":"record","value":{"fields":[{"body":{"kind":"u64","value":{}},"name":"count"},{"body":{"kind":"f64","value":{}},"name":"sum"},{"body":{"kind":"f64","value":{}},"name":"avg"},{"body":{"kind":"option","value":{"inner":{"kind":"f64","value":{}}}},"name":"min"},{"body":{"kind":"option","value":{"inner":{"kind":"f64","value":{}}}},"name":"max"}]}},"id":"timeline_core.types.AggregationResult","name":"AggregationResult"}],
        "root":{"kind":"ref","value":{"id":"timeline_core.types.AggregationResult"}}},
        "value":{"kind":"record","value":{"fields":[{"kind":"u64","value":1},{"kind":"f64","value":1.0},{"kind":"f64","value":1.0},{"kind":"option","value":{"inner":null}},{"kind":"option","value":{"inner":null}}]}}}"#;

    fn decode<T: serde::de::DeserializeOwned>(json: &str) -> Result<T, Box<dyn std::error::Error>> {
        let typed: TypedResult = serde_json::from_str(json)?;
        Ok(serde_json::from_value(typed.to_named_json()?)?)
    }

    #[test]
    fn decodes_result_option_record_and_variant() -> TestResult {
        let result: Result<Option<StatePoint>, TimelineError> = decode(DERIVED_RESULT)?;
        assert_eq!(
            result,
            Ok(Some(StatePoint {
                t1: 5,
                t2: None,
                value: EventValue::BoolValue(true),
            }))
        );
        Ok(())
    }

    #[test]
    fn decodes_record_through_reference() -> TestResult {
        let result: AggregationResult = decode(AGGREGATION_RESULT)?;
        assert_eq!(
            result,
            AggregationResult {
                count: 1,
                sum: 1.0,
                avg: 1.0,
                min: None,
                max: None,
            }
        );
        Ok(())
    }

    #[test]
    fn decodes_error_case_and_unit_ok() -> TestResult {
        let err = r#"{"graph":{"defs":[{"body":{"kind":"enum","value":{"cases":["NotInitialized"]}},"id":"E","name":"E"}],
            "root":{"kind":"result","value":{"spec":{"err":{"kind":"ref","value":{"id":"E"}}}}}},
            "value":{"kind":"result","value":{"tag":"err","value":{"kind":"enum","value":{"case":0}}}}}"#;
        let result: Result<(), TimelineError> = decode(err)?;
        assert_eq!(result, Err(TimelineError::NotInitialized));

        let ok = r#"{"graph":{"defs":[],"root":{"kind":"result","value":{"spec":{}}}},
            "value":{"kind":"result","value":{"tag":"ok","value":null}}}"#;
        let result: Result<(), TimelineError> = decode(ok)?;
        assert_eq!(result, Ok(()));
        Ok(())
    }

    #[test]
    fn decodes_lists() -> TestResult {
        let json = r#"{"graph":{"defs":[],"root":{"kind":"list","value":{"element":{"kind":"string","value":{}}}}},
            "value":{"kind":"list","value":{"elements":[{"kind":"string","value":"a"},{"kind":"string","value":"b"}]}}}"#;
        let result: Vec<String> = decode(json)?;
        assert_eq!(result, vec!["a".to_string(), "b".to_string()]);
        Ok(())
    }

    #[test]
    fn rejects_unknown_reference() -> TestResult {
        let json = r#"{"graph":{"defs":[],"root":{"kind":"ref","value":{"id":"missing"}}},
            "value":{"kind":"u64","value":1}}"#;
        let typed: TypedResult = serde_json::from_str(json)?;
        assert!(typed.to_named_json().is_err());
        Ok(())
    }
}
