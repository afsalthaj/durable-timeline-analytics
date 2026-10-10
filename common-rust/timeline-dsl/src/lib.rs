// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
mod lexer;
mod parser;

pub use parser::{ParseError, ParsedTimeline, parse};

#[cfg(test)]
mod tests;
