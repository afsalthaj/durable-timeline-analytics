// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
use std::collections::BTreeMap;
use std::fmt;

use common_lib::{
    AggregationConfig, AggregationFunction, CompareOp, EventColumnName, EventPredicate, EventValue,
    NodeNumber, TimeLineOp,
};

use crate::lexer::{LexError, Lexer, Spanned, Token};

/// A parsed DSL query: the timeline expression plus optional aggregation.
///
/// Captions come from `.describe("...")` and are keyed by the same
/// pre-order node numbers as `TimeLineOp::numbered_nodes`. They are
/// display-only — they do not create agents.
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedTimeline {
    pub op: TimeLineOp,
    pub aggregation: Option<AggregationConfig>,
    pub captions: BTreeMap<NodeNumber, String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub offset: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "parse error at offset {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for ParseError {}

impl From<LexError> for ParseError {
    fn from(e: LexError) -> Self {
        ParseError {
            offset: e.offset,
            message: e.message,
        }
    }
}

/// Parse a timeline DSL query into a `ParsedTimeline`.
///
/// Grammar (simplified):
/// ```text
/// query       = expr ("|" aggregate)?
/// expr        = or_expr
/// or_expr     = and_expr ("||" and_expr)*
/// and_expr    = described ("&&" described)*
/// described   = unary (".describe" "(" string ")")*
/// unary       = "!" unary | postfix
/// postfix     = primary (".describe" "(" string ")")* (cmp_op value)?
/// primary     = "(" expr ")"
///             | "latest_event_to_state" "(" column ")"
///             | "has_existed" "(" predicate ")"
///             | "has_existed_within" "(" predicate "," int ")"
///             | "duration_where" "(" expr ")"
///             | "duration_in_cur_state" "(" expr ")"
/// predicate   = ident pred_op value
/// pred_op     = "==" | ">" | "<"
/// cmp_op      = "==" | ">" | ">=" | "<" | "<="
/// value       = string | int | float | bool
/// aggregate   = "aggregate" "(" "group_by" "(" column ")" "," agg_fn ("," agg_fn)* ")"
/// agg_fn      = "count" | "sum" | "avg" | "min" | "max"
/// ```
pub fn parse(input: &str) -> Result<ParsedTimeline, ParseError> {
    let tokens = Lexer::new(input).tokenize()?;
    let mut parser = Parser::new(tokens)?;
    let result = parser.parse_query()?;
    parser.expect(Token::Eof)?;
    Ok(result)
}

struct Parser {
    tokens: Vec<Spanned>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Spanned>) -> Result<Self, ParseError> {
        match tokens.last() {
            Some(last) if last.token == Token::Eof => Ok(Self { tokens, pos: 0 }),
            _ => Err(ParseError {
                offset: 0,
                message: "token stream must end with EOF".to_string(),
            }),
        }
    }

    fn current(&self) -> &Spanned {
        // `new` guarantees a trailing EOF and `advance` never moves past it.
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn peek(&self) -> &Token {
        &self.current().token
    }

    fn peek_at(&self, offset: usize) -> &Token {
        let i = self.pos.saturating_add(offset).min(self.tokens.len() - 1);
        &self.tokens[i].token
    }

    fn at_describe(&self) -> bool {
        matches!(self.peek(), Token::Dot)
            && matches!(self.peek_at(1), Token::Ident(name) if name == "describe")
    }

    fn is_compare_op(token: &Token) -> bool {
        matches!(
            token,
            Token::EqualEqual
                | Token::Greater
                | Token::GreaterEqual
                | Token::Less
                | Token::LessEqual
        )
    }

    /// True when `.describe("...")*` is followed by a comparison, so the
    /// caption belongs to the primary and not the surrounding unary/`&&`.
    fn describe_then_compare(&self) -> bool {
        let mut offset = 0;
        loop {
            if !matches!(self.peek_at(offset), Token::Dot) {
                return Self::is_compare_op(self.peek_at(offset));
            }
            if !matches!(self.peek_at(offset + 1), Token::Ident(name) if name == "describe") {
                return false;
            }
            if !matches!(self.peek_at(offset + 2), Token::LParen) {
                return false;
            }
            if !matches!(self.peek_at(offset + 3), Token::StringLit(_)) {
                return false;
            }
            if !matches!(self.peek_at(offset + 4), Token::RParen) {
                return false;
            }
            offset += 5;
        }
    }

    fn advance(&mut self) {
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
    }

    fn expect(&mut self, expected: Token) -> Result<(), ParseError> {
        if *self.peek() == expected {
            self.advance();
            Ok(())
        } else {
            Err(self.error(format!("expected '{}', found '{}'", expected, self.peek())))
        }
    }

    fn error(&self, message: String) -> ParseError {
        ParseError {
            offset: self.current().offset,
            message,
        }
    }

    fn parse_query(&mut self) -> Result<ParsedTimeline, ParseError> {
        let expr = self.parse_expr()?;
        let aggregation = if *self.peek() == Token::Pipe {
            self.advance();
            Some(self.parse_aggregate()?)
        } else {
            None
        };
        let (op, captions) = expr.flatten();
        Ok(ParsedTimeline {
            op,
            aggregation,
            captions,
        })
    }

    fn parse_expr(&mut self) -> Result<Annotated, ParseError> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Annotated, ParseError> {
        let mut left = self.parse_and()?;
        while *self.peek() == Token::PipePipe {
            self.advance();
            let right = self.parse_and()?;
            left = Annotated::or(left, right);
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Annotated, ParseError> {
        let mut left = self.parse_described()?;
        while *self.peek() == Token::AmpAmp {
            self.advance();
            let right = self.parse_described()?;
            left = Annotated::and(left, right);
        }
        Ok(left)
    }

    fn parse_described(&mut self) -> Result<Annotated, ParseError> {
        let mut expr = self.parse_unary()?;
        while self.at_describe() {
            expr = expr.with_caption(self.parse_describe_call()?);
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Annotated, ParseError> {
        if *self.peek() == Token::Bang {
            self.advance();
            let inner = self.parse_unary()?;
            return Ok(Annotated::not(inner));
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Annotated, ParseError> {
        let mut expr = self.parse_primary()?;
        if self.at_describe() && self.describe_then_compare() {
            while self.at_describe() {
                expr = expr.with_caption(self.parse_describe_call()?);
            }
        }
        let compare_op = match self.peek() {
            Token::EqualEqual => CompareOp::EqualTo,
            Token::Greater => CompareOp::GreaterThan,
            Token::GreaterEqual => CompareOp::GreaterThanOrEqual,
            Token::Less => CompareOp::LessThan,
            Token::LessEqual => CompareOp::LessThanOrEqual,
            _ => return Ok(expr),
        };
        self.advance();
        let value = self.parse_value()?;
        Ok(Annotated::compare(compare_op, expr, value))
    }

    fn parse_describe_call(&mut self) -> Result<String, ParseError> {
        self.expect(Token::Dot)?;
        match self.peek() {
            Token::Ident(name) if name == "describe" => self.advance(),
            other => {
                return Err(self.error(format!("expected 'describe', found '{other}'")));
            }
        }
        self.expect(Token::LParen)?;
        let caption = match self.peek() {
            Token::StringLit(s) => s.clone(),
            other => return Err(self.error(format!("expected string, found '{other}'"))),
        };
        self.advance();
        self.expect(Token::RParen)?;
        Ok(caption)
    }

    fn parse_primary(&mut self) -> Result<Annotated, ParseError> {
        match self.peek() {
            Token::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(Token::RParen)?;
                Ok(expr)
            }
            Token::LatestEventToState => {
                self.advance();
                self.expect(Token::LParen)?;
                let column = self.parse_column_name()?;
                self.expect(Token::RParen)?;
                Ok(Annotated::leaf(TimeLineOp::TlLatestEventToState(column)))
            }
            Token::HasExisted => {
                self.advance();
                self.expect(Token::LParen)?;
                let predicate = self.parse_predicate()?;
                self.expect(Token::RParen)?;
                Ok(Annotated::leaf(TimeLineOp::TlHasExisted(predicate)))
            }
            Token::HasExistedWithin => {
                self.advance();
                self.expect(Token::LParen)?;
                let predicate = self.parse_predicate()?;
                self.expect(Token::Comma)?;
                let within = self.parse_duration()?;
                self.expect(Token::RParen)?;
                Ok(Annotated::leaf(TimeLineOp::TlHasExistedWithin {
                    predicate,
                    within,
                }))
            }
            Token::DurationWhere => {
                self.advance();
                self.expect(Token::LParen)?;
                let inner = self.parse_expr()?;
                self.expect(Token::RParen)?;
                Ok(Annotated::duration_where(inner))
            }
            Token::DurationInCurState => {
                self.advance();
                self.expect(Token::LParen)?;
                let inner = self.parse_expr()?;
                self.expect(Token::RParen)?;
                Ok(Annotated::duration_in_cur_state(inner))
            }
            other => Err(self.error(format!("unexpected token '{other}'"))),
        }
    }

    fn parse_predicate(&mut self) -> Result<EventPredicate, ParseError> {
        let column = EventColumnName(self.parse_ident()?);
        let make: fn(EventColumnName, EventValue) -> EventPredicate = match self.peek() {
            Token::EqualEqual => |column, value| EventPredicate::Equals { column, value },
            Token::Greater => |column, value| EventPredicate::GreaterThan { column, value },
            Token::Less => |column, value| EventPredicate::LessThan { column, value },
            other => {
                return Err(self.error(format!(
                    "expected predicate operator ('==', '>', '<'), found '{other}'"
                )))
            }
        };
        self.advance();
        let value = self.parse_value()?;
        Ok(make(column, value))
    }

    fn parse_value(&mut self) -> Result<EventValue, ParseError> {
        let value = match self.peek() {
            Token::StringLit(s) => EventValue::StringValue(s.clone()),
            Token::IntLit(n) => EventValue::IntValue(*n),
            Token::FloatLit(f) => EventValue::FloatValue(*f),
            Token::True => EventValue::BoolValue(true),
            Token::False => EventValue::BoolValue(false),
            other => return Err(self.error(format!("expected value, found '{other}'"))),
        };
        self.advance();
        Ok(value)
    }

    fn parse_column_name(&mut self) -> Result<EventColumnName, ParseError> {
        let name = match self.peek() {
            Token::Ident(s) | Token::StringLit(s) => s.clone(),
            other => return Err(self.error(format!("expected column name, found '{other}'"))),
        };
        self.advance();
        Ok(EventColumnName(name))
    }

    fn parse_duration(&mut self) -> Result<u64, ParseError> {
        match self.peek() {
            Token::IntLit(n) => {
                let within = u64::try_from(*n)
                    .map_err(|_| self.error(format!("duration must be non-negative, found {n}")))?;
                self.advance();
                Ok(within)
            }
            other => Err(self.error(format!("expected integer, found '{other}'"))),
        }
    }

    fn parse_ident(&mut self) -> Result<String, ParseError> {
        match self.peek() {
            Token::Ident(s) => {
                let ident = s.clone();
                self.advance();
                Ok(ident)
            }
            other => Err(self.error(format!("expected identifier, found '{other}'"))),
        }
    }

    fn parse_aggregate(&mut self) -> Result<AggregationConfig, ParseError> {
        self.expect(Token::Aggregate)?;
        self.expect(Token::LParen)?;
        self.expect(Token::GroupBy)?;
        self.expect(Token::LParen)?;
        let group_by_column = self.parse_column_name()?;
        self.expect(Token::RParen)?;
        self.expect(Token::Comma)?;

        let mut functions = vec![self.parse_agg_fn()?];
        while *self.peek() == Token::Comma {
            self.advance();
            functions.push(self.parse_agg_fn()?);
        }

        self.expect(Token::RParen)?;
        Ok(AggregationConfig {
            group_by_column,
            functions,
        })
    }

    fn parse_agg_fn(&mut self) -> Result<AggregationFunction, ParseError> {
        let function = match self.peek() {
            Token::Count => AggregationFunction::Count,
            Token::Sum => AggregationFunction::Sum,
            Token::Avg => AggregationFunction::Avg,
            Token::Min => AggregationFunction::Min,
            Token::Max => AggregationFunction::Max,
            other => {
                return Err(self.error(format!(
                    "expected aggregation function (count, sum, avg, min, max), found '{other}'"
                )))
            }
        };
        self.advance();
        Ok(function)
    }
}

/// Parse-time tree that carries `.describe` captions without putting them
/// on `TimeLineOp` (captions never become agents).
struct Annotated {
    caption: Option<String>,
    kind: AnnotatedKind,
}

enum AnnotatedKind {
    Leaf(TimeLineOp),
    Compare {
        op: CompareOp,
        timeline: Box<Annotated>,
        value: EventValue,
    },
    And {
        left: Box<Annotated>,
        right: Box<Annotated>,
    },
    Or {
        left: Box<Annotated>,
        right: Box<Annotated>,
    },
    Not(Box<Annotated>),
    DurationWhere(Box<Annotated>),
    DurationInCurState(Box<Annotated>),
}

impl Annotated {
    fn leaf(op: TimeLineOp) -> Self {
        Annotated {
            caption: None,
            kind: AnnotatedKind::Leaf(op),
        }
    }

    fn compare(op: CompareOp, timeline: Annotated, value: EventValue) -> Self {
        Annotated {
            caption: None,
            kind: AnnotatedKind::Compare {
                op,
                timeline: Box::new(timeline),
                value,
            },
        }
    }

    fn and(left: Annotated, right: Annotated) -> Self {
        Annotated {
            caption: None,
            kind: AnnotatedKind::And {
                left: Box::new(left),
                right: Box::new(right),
            },
        }
    }

    fn or(left: Annotated, right: Annotated) -> Self {
        Annotated {
            caption: None,
            kind: AnnotatedKind::Or {
                left: Box::new(left),
                right: Box::new(right),
            },
        }
    }

    fn not(inner: Annotated) -> Self {
        Annotated {
            caption: None,
            kind: AnnotatedKind::Not(Box::new(inner)),
        }
    }

    fn duration_where(inner: Annotated) -> Self {
        Annotated {
            caption: None,
            kind: AnnotatedKind::DurationWhere(Box::new(inner)),
        }
    }

    fn duration_in_cur_state(inner: Annotated) -> Self {
        Annotated {
            caption: None,
            kind: AnnotatedKind::DurationInCurState(Box::new(inner)),
        }
    }

    fn with_caption(mut self, caption: String) -> Self {
        if !caption.is_empty() {
            self.caption = Some(caption);
        }
        self
    }

    fn flatten(self) -> (TimeLineOp, BTreeMap<NodeNumber, String>) {
        let mut captions = BTreeMap::new();
        let op = self.into_op(&mut captions, &mut 1);
        (op, captions)
    }

    fn into_op(self, captions: &mut BTreeMap<NodeNumber, String>, next: &mut u64) -> TimeLineOp {
        let number = NodeNumber(*next);
        *next = next.saturating_add(1);
        if let Some(caption) = self.caption {
            captions.insert(number, caption);
        }
        match self.kind {
            AnnotatedKind::Leaf(op) => op,
            AnnotatedKind::Compare {
                op,
                timeline,
                value,
            } => TimeLineOp::compare(op, timeline.into_op(captions, next), value),
            AnnotatedKind::And { left, right } => {
                TimeLineOp::and(left.into_op(captions, next), right.into_op(captions, next))
            }
            AnnotatedKind::Or { left, right } => {
                TimeLineOp::or(left.into_op(captions, next), right.into_op(captions, next))
            }
            AnnotatedKind::Not(inner) => !inner.into_op(captions, next),
            AnnotatedKind::DurationWhere(inner) => {
                TimeLineOp::duration_where(inner.into_op(captions, next))
            }
            AnnotatedKind::DurationInCurState(inner) => {
                TimeLineOp::duration_in_cur_state(inner.into_op(captions, next))
            }
        }
    }
}
