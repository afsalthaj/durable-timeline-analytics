// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
use common_lib::{AggregationConfig, AggregationFunction, EventColumnName};

use crate::parser::{parse, ParseError};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn assert_parses_to(input: &str, expected: &str) -> TestResult {
    let parsed = parse(input)?;
    assert_eq!(parsed.op.to_string(), expected);
    Ok(())
}

fn parse_error(input: &str) -> Result<ParseError, String> {
    match parse(input) {
        Ok(parsed) => Err(format!("expected a parse error, got {}", parsed.op)),
        Err(e) => Ok(e),
    }
}

fn aggregation_of(input: &str) -> Result<AggregationConfig, Box<dyn std::error::Error>> {
    parse(input)?
        .aggregation
        .ok_or_else(|| "expected an aggregation".into())
}

#[test]
fn latest_event_to_state() -> TestResult {
    let parsed = parse("latest_event_to_state(status)")?;
    assert_eq!(parsed.op.to_string(), "TlLatestEventToState(status)");
    assert!(parsed.aggregation.is_none());
    Ok(())
}

#[test]
fn latest_event_to_state_with_quoted_column() -> TestResult {
    assert_parses_to(
        r#"latest_event_to_state("player state")"#,
        "TlLatestEventToState(player state)",
    )
}

#[test]
fn has_existed_with_equals() -> TestResult {
    assert_parses_to(
        r#"has_existed(status == "active")"#,
        "TlHasExisted(status == active)",
    )
}

#[test]
fn has_existed_with_greater_than() -> TestResult {
    assert_parses_to("has_existed(score > 100)", "TlHasExisted(score > 100)")
}

#[test]
fn has_existed_with_less_than() -> TestResult {
    assert_parses_to("has_existed(health < 50)", "TlHasExisted(health < 50)")
}

#[test]
fn has_existed_within() -> TestResult {
    assert_parses_to(
        r#"has_existed_within(status == "error", 3600)"#,
        "TlHasExistedWithin(status == error, 3600)",
    )
}

#[test]
fn equal_to_string() -> TestResult {
    assert_parses_to(
        r#"latest_event_to_state(status) == "active""#,
        "EqualTo(TlLatestEventToState(status), active)",
    )
}

#[test]
fn greater_than_int() -> TestResult {
    assert_parses_to(
        "latest_event_to_state(score) > 100",
        "GreaterThan(TlLatestEventToState(score), 100)",
    )
}

#[test]
fn greater_than_or_equal() -> TestResult {
    assert_parses_to(
        "latest_event_to_state(score) >= 50",
        "GreaterThanOrEqual(TlLatestEventToState(score), 50)",
    )
}

#[test]
fn less_than() -> TestResult {
    assert_parses_to(
        "latest_event_to_state(health) < 20",
        "LessThan(TlLatestEventToState(health), 20)",
    )
}

#[test]
fn less_than_or_equal() -> TestResult {
    assert_parses_to(
        "latest_event_to_state(health) <= 0",
        "LessThanOrEqual(TlLatestEventToState(health), 0)",
    )
}

#[test]
fn compare_float() -> TestResult {
    assert_parses_to(
        "latest_event_to_state(temperature) > 36.5",
        "GreaterThan(TlLatestEventToState(temperature), 36.5)",
    )
}

#[test]
fn compare_bool() -> TestResult {
    assert_parses_to(
        "latest_event_to_state(flag) == true",
        "EqualTo(TlLatestEventToState(flag), true)",
    )
}

#[test]
fn negation() -> TestResult {
    assert_parses_to(
        r#"!has_existed(error == "fatal")"#,
        "Not(TlHasExisted(error == fatal))",
    )
}

#[test]
fn and_two_timelines() -> TestResult {
    assert_parses_to(
        r#"has_existed(status == "active") && has_existed(region == "us")"#,
        "And(TlHasExisted(status == active), TlHasExisted(region == us))",
    )
}

#[test]
fn or_two_timelines() -> TestResult {
    assert_parses_to(
        r#"has_existed(status == "a") || has_existed(status == "b")"#,
        "Or(TlHasExisted(status == a), TlHasExisted(status == b))",
    )
}

#[test]
fn and_binds_tighter_than_or() -> TestResult {
    assert_parses_to(
        "has_existed(x == 1) || has_existed(y == 2) && has_existed(z == 3)",
        "Or(TlHasExisted(x == 1), And(TlHasExisted(y == 2), TlHasExisted(z == 3)))",
    )
}

#[test]
fn parentheses_override_precedence() -> TestResult {
    assert_parses_to(
        "(has_existed(x == 1) || has_existed(y == 2)) && has_existed(z == 3)",
        "And(Or(TlHasExisted(x == 1), TlHasExisted(y == 2)), TlHasExisted(z == 3))",
    )
}

#[test]
fn duration_where() -> TestResult {
    assert_parses_to(
        "duration_where(has_existed(online == true))",
        "TlDurationWhere(TlHasExisted(online == true))",
    )
}

#[test]
fn duration_in_cur_state() -> TestResult {
    assert_parses_to(
        r#"duration_in_cur_state(latest_event_to_state(status) == "idle")"#,
        "TlDurationInCurState(EqualTo(TlLatestEventToState(status), idle))",
    )
}

#[test]
fn complex_nested() -> TestResult {
    assert_parses_to(
        r#"duration_where(has_existed(status == "active") && !has_existed_within(error > 0, 300))"#,
        "TlDurationWhere(And(TlHasExisted(status == active), Not(TlHasExistedWithin(error > 0, 300))))",
    )
}

#[test]
fn aggregate_single_function() -> TestResult {
    let input = r#"has_existed(status == "active") | aggregate(group_by(region), count)"#;
    assert_parses_to(input, "TlHasExisted(status == active)")?;
    let aggregation = aggregation_of(input)?;
    assert_eq!(aggregation.group_by_column, EventColumnName::new("region"));
    assert_eq!(aggregation.functions, vec![AggregationFunction::Count]);
    Ok(())
}

#[test]
fn aggregate_multiple_functions() -> TestResult {
    let aggregation = aggregation_of(
        "latest_event_to_state(score) > 0 | aggregate(group_by(team), count, sum, avg, min, max)",
    )?;
    assert_eq!(aggregation.group_by_column, EventColumnName::new("team"));
    assert_eq!(
        aggregation.functions,
        vec![
            AggregationFunction::Count,
            AggregationFunction::Sum,
            AggregationFunction::Avg,
            AggregationFunction::Min,
            AggregationFunction::Max,
        ]
    );
    Ok(())
}

#[test]
fn full_complex_query_with_aggregation() -> TestResult {
    let input = r#"
        duration_where(
            has_existed(status == "active")
            && !(has_existed_within(error > 0, 300))
        ) | aggregate(group_by(region), count, avg)
    "#;
    assert_parses_to(
        input,
        "TlDurationWhere(And(TlHasExisted(status == active), Not(TlHasExistedWithin(error > 0, 300))))",
    )?;
    let aggregation = aggregation_of(input)?;
    assert_eq!(aggregation.group_by_column, EventColumnName::new("region"));
    assert_eq!(
        aggregation.functions,
        vec![AggregationFunction::Count, AggregationFunction::Avg]
    );
    Ok(())
}

#[test]
fn chained_and_is_left_associative() -> TestResult {
    assert_parses_to(
        "has_existed(a == 1) && has_existed(b == 2) && has_existed(c == 3)",
        "And(And(TlHasExisted(a == 1), TlHasExisted(b == 2)), TlHasExisted(c == 3))",
    )
}

#[test]
fn chained_or_is_left_associative() -> TestResult {
    assert_parses_to(
        "has_existed(a == 1) || has_existed(b == 2) || has_existed(c == 3)",
        "Or(Or(TlHasExisted(a == 1), TlHasExisted(b == 2)), TlHasExisted(c == 3))",
    )
}

#[test]
fn double_negation() -> TestResult {
    assert_parses_to("!!has_existed(x == 1)", "Not(Not(TlHasExisted(x == 1)))")
}

#[test]
fn whitespace_insensitive() -> TestResult {
    let compact = parse("has_existed(x==1)&&has_existed(y==2)")?;
    let spaced = parse("has_existed(x == 1) && has_existed(y == 2)")?;
    assert_eq!(compact, spaced);
    Ok(())
}

#[test]
fn cirr_query_with_aggregation() -> TestResult {
    let input = r#"
        duration_where(
            has_existed(playerStateChange == "play")
            && !has_existed_within(playerStateChange == "seek", 5)
            && latest_event_to_state(playerStateChange) == "buffer"
        ) | aggregate(group_by(cdn), count, sum, avg)
    "#;
    assert_parses_to(
        input,
        "TlDurationWhere(And(And(TlHasExisted(playerStateChange == play), \
         Not(TlHasExistedWithin(playerStateChange == seek, 5))), \
         EqualTo(TlLatestEventToState(playerStateChange), buffer)))",
    )?;
    let aggregation = aggregation_of(input)?;
    assert_eq!(aggregation.group_by_column, EventColumnName::new("cdn"));
    assert_eq!(
        aggregation.functions,
        vec![
            AggregationFunction::Count,
            AggregationFunction::Sum,
            AggregationFunction::Avg
        ],
    );
    Ok(())
}

#[test]
fn error_unterminated_string() -> TestResult {
    let err = parse_error(r#"latest_event_to_state("oops)"#)?;
    assert!(err.message.contains("unterminated string"));
    Ok(())
}

#[test]
fn error_missing_rparen() -> TestResult {
    let err = parse_error(r#"latest_event_to_state("x""#)?;
    assert!(err.message.contains("expected ')'"));
    Ok(())
}

#[test]
fn error_unexpected_token() -> TestResult {
    let err = parse_error("42")?;
    assert!(err.message.contains("unexpected token"));
    assert_eq!(err.offset, 0);
    Ok(())
}

#[test]
fn error_bad_predicate_op() -> TestResult {
    let err = parse_error("has_existed(col >= 1)")?;
    assert!(err.message.contains("predicate operator"));
    Ok(())
}

#[test]
fn error_missing_aggregate_function() -> TestResult {
    let err = parse_error("has_existed(x == 1) | aggregate(group_by(r), )")?;
    assert!(err.message.contains("aggregation function"));
    Ok(())
}

#[test]
fn error_trailing_input() -> TestResult {
    let err = parse_error("has_existed(x == 1) has_existed(y == 2)")?;
    assert!(err.message.contains("expected 'EOF'"));
    Ok(())
}

#[test]
fn error_empty_input() -> TestResult {
    let err = parse_error("   ")?;
    assert!(err.message.contains("unexpected token 'EOF'"));
    Ok(())
}

#[test]
fn error_single_ampersand() -> TestResult {
    let err = parse_error("has_existed(x == 1) & has_existed(y == 2)")?;
    assert!(err.message.contains("expected '&&'"));
    Ok(())
}

#[test]
fn error_reports_offset_of_offending_token() -> TestResult {
    let err = parse_error("has_existed(x == )")?;
    assert_eq!(err.offset, 17);
    assert!(err.message.contains("expected value"));
    Ok(())
}
