//! Contains functions for parsing [expressions](super::Expr),
//! [assignments](super::Assignment), and [rules](super::Rule).
//!
//! This is brought to you by [`nom`].
use std::error::Error;
use std::fmt;

use nom::branch::alt;
use nom::bytes::complete::{tag, take_while};
use nom::character::complete::{alphanumeric1, multispace0};
use nom::combinator::{all_consuming, map, map_res, not, opt, peek, recognize, value};
use nom::error::ParseError;
use nom::sequence::{pair, preceded, terminated, tuple};
use nom::{Err, Finish, IResult, Parser};

use super::*;

/// The error kind for [`ParseExprError`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
enum ParseExprErrorKind {
    /// An operator was encountered, but there were not enough operands on the stack.
    NotEnoughOperands,

    /// A negative number was encountered in an illegal context (i.e. not in an addition).
    UnexpectedNegativeNumber,

    /// An error returned by `nom`.
    Nom(nom::error::ErrorKind),
}

impl fmt::Display for ParseExprErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::NotEnoughOperands => write!(f, "Not enough operands on the stack"),
            Self::UnexpectedNegativeNumber => write!(f, "Encountered unexpected negative number"),
            Self::Nom(kind) => write!(f, "Error from nom: {}", kind.description()),
        }
    }
}

/// An error encountered while parsing expressions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseExprError {
    /// The kind of error.
    kind: ParseExprErrorKind,

    /// The input that caused the error.
    input: String,
}

impl<'a> ParseError<&'a str> for ParseExprError {
    fn from_error_kind(input: &'a str, kind: nom::error::ErrorKind) -> Self {
        Self {
            input: input.to_string(),
            kind: ParseExprErrorKind::Nom(kind),
        }
    }

    fn append(_input: &'a str, _kind: nom::error::ErrorKind, other: Self) -> Self {
        other
    }
}

impl<'a, E> nom::error::FromExternalError<&'a str, E> for ParseExprError {
    fn from_external_error(input: &'a str, kind: nom::error::ErrorKind, _e: E) -> Self {
        Self::from_error_kind(input, kind)
    }
}

impl fmt::Display for ParseExprError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Error encountered while trying to parse input {}: {}",
            self.input, self.kind
        )
    }
}

impl Error for ParseExprError {}

/// Applies its child parser repeatedly with zero or more spaces in between.
///
/// If the child parser doesn't consume any input, you're going to have a bad time.
fn space_separated<'a, O, P>(
    mut parser: P,
) -> impl FnMut(&'a str) -> IResult<&'a str, Vec<O>, ParseExprError>
where
    P: 'a + Parser<&'a str, O, ParseExprError>,
{
    move |mut input| {
        let mut result = Vec::new();
        match parser.parse(input) {
            Ok((rest, item)) => {
                input = rest;
                result.push(item);
            }
            Err(_) => return Ok((input, result)),
        }

        loop {
            let rest = multispace0(input)?.0;
            if let Ok((rest, item)) = parser.parse(rest) {
                input = rest;
                result.push(item);
            } else {
                break;
            }
        }

        Ok((input, result))
    }
}

/// Parses a [variable](super::Variable).
///
/// This accepts identifiers of the form `$[a-zA-Z0-9]+`.
fn variable(input: &str) -> IResult<&str, Variable, ParseExprError> {
    let (rest, var) = recognize(tuple((tag("$"), alphanumeric1)))(input)?;
    Ok((rest, Variable(var.to_string())))
}

/// Parses a [variable](super::Variable).
///
/// This accepts identifiers of the form `$[a-zA-Z0-9]+`.
/// It will fail if there is any input remaining afterwards.
pub fn variable_complete(input: &str) -> Result<Variable, ParseExprError> {
    all_consuming(variable)(input).finish().map(|(_, v)| v)
}

/// Parses a [constant](super::Constant).
///
/// This accepts identifiers of the form `\.?[a-zA-Z0-9]+`.
fn constant(input: &str) -> IResult<&str, Constant, ParseExprError> {
    let (rest, con) = recognize(preceded(opt(tag(".")), alphanumeric1))(input)?;
    Ok((rest, Constant(con.to_string())))
}

/// Parses a [constant](super::Constant).
///
/// This accepts identifiers of the form `\.[a-zA-Z0-9]+`.
/// It will fail if there is any input remaining afterwards.
pub fn constant_complete(input: &str) -> Result<Constant, ParseExprError> {
    all_consuming(constant)(input).finish().map(|(_, c)| c)
}

/// Parses an [identifier](super::Identifier).
pub fn identifier(input: &str) -> IResult<&str, Identifier, ParseExprError> {
    alt((
        map(variable, Identifier::Var),
        map(constant, Identifier::Const),
    ))(input)
}

/// Parses an [identifier](super::Identifier).
///
/// This will fail if there is any input remaining afterwards.
pub fn identifier_complete(input: &str) -> Result<Identifier, ParseExprError> {
    all_consuming(identifier)(input).finish().map(|(_, i)| i)
}

/// Parses a [binary operator](super::BinOp).
fn bin_op(input: &str) -> IResult<&str, BinOp, ParseExprError> {
    alt((
        value(BinOp::Add, tag("+")),
        value(BinOp::Sub, tag("-")),
        value(BinOp::Mul, tag("*")),
        value(BinOp::Div, tag("/")),
        value(BinOp::Mod, tag("%")),
        value(BinOp::Align, tag("@")),
    ))(input)
}

/// Parses an integer.
///
/// This accepts expressions of the form `-?[0-9]+`.
fn number<T: RegisterValue>(input: &str) -> IResult<&str, T, ParseExprError> {
    map_res(take_while(|c: char| c.is_ascii_digit()), T::from_str)(input)
}

/// Parses a number, variable, or constant.
///
/// Variables or constants followed by ":" don't count; that's the start
/// of a [rule](super::Rule), not an expression.
fn base_expr<T: RegisterValue>(input: &str) -> IResult<&str, Expr<T>, ParseExprError> {
    alt((
        map(number, Expr::Value),
        map(terminated(variable, peek(not(tag(":")))), Expr::Var),
        map(terminated(constant, peek(not(tag(":")))), Expr::Const),
    ))(input)
}

/// Parses an [expression](super::Expr).
///
/// This returns the largest single expression that can be parsed starting from the
/// beginning. Due to this and having to handle the special case of negative numbers,
/// it is internally somewhat complicated.
///
/// # Example
/// ```
/// use symbolic_unwind::evaluator::parsing::expr;
/// use symbolic_unwind::evaluator::{BinOp, Expr};
///
/// let e1 = Expr::Value(1u8);
/// let e2 = Expr::Op(Box::new(e1.clone()), Box::new(Expr::Value(2)), BinOp::Sub);
///
/// assert_eq!(expr("1 -2").unwrap(), (" -2", e1));
/// assert_eq!(expr("1 -2 + 3").unwrap(), (" 3", e2));
/// ```
pub fn expr<T: RegisterValue>(mut input: &str) -> IResult<&str, Expr<T>, ParseExprError> {
    let mut stack = Vec::new();

    // Parse an initial expression. If this fails, we are done.
    let (rest, (sign, e)) = pair(opt(tag("-")), base_expr)(input)?;
    stack.push((e.clone(), sign.is_some()));

    // Invariant: saved_expr is the largest whole expressions we parsed so far, saved_sign
    // is true if that expression was a negative number, and saved_input is the input
    // after parsing saved_expr.
    let (mut saved_input, mut saved_expr, mut saved_sign) = (rest, e, sign.is_some());
    input = saved_input;

    // Parse until we run out of input or nothing matches.
    while !input.is_empty() {
        input = multispace0(input)?.0;

        // Try to parse a constant, variable, or number.
        if let Ok((rest, (sign, e))) = pair(opt(tag("-")), base_expr)(input) {
            stack.push((e, sign.is_some()));
            input = rest;
            if stack.len() == 1 {
                // If there is exactly one expression on the stack, we've just parsed
                // a new whole expression. Save it to maintain invariant.
                saved_input = input;
                saved_expr = stack[0].0.clone();
                saved_sign = stack[0].1;
            }
        }
        // Try to parse a dereference.
        else if let Ok((rest, _)) = tag::<_, _, ParseExprError>("^")(input) {
            let (e, neg) = match stack.pop() {
                Some(p) => p,
                None => {
                    return Err(Err::Error(ParseExprError {
                        input: input.to_owned(),
                        kind: ParseExprErrorKind::NotEnoughOperands,
                    }))
                }
            };

            // If the operand is negative, that's an error.
            if neg {
                return Err(Err::Error(ParseExprError {
                    input: input.to_owned(),
                    kind: ParseExprErrorKind::UnexpectedNegativeNumber,
                }));
            }

            stack.push((Expr::Deref(Box::new(e)), false));
            input = rest;
            if stack.len() == 1 {
                saved_input = input;
                saved_expr = stack[0].0.clone();
                saved_sign = stack[0].1;
            }
        }
        // Try to parse a binary expression.
        else if let Ok((rest, op)) = bin_op(input) {
            let (e2, neg2) = match stack.pop() {
                Some(p) => p,
                None => {
                    return Err(Err::Error(ParseExprError {
                        input: input.to_string(),
                        kind: ParseExprErrorKind::NotEnoughOperands,
                    }))
                }
            };

            let (e1, neg1) = match stack.pop() {
                Some(p) => p,
                None => {
                    return Err(Err::Error(ParseExprError {
                        input: input.to_string(),
                        kind: ParseExprErrorKind::NotEnoughOperands,
                    }))
                }
            };

            // If either the first operand is negative or the second operand is negative
            // and it's not an addition, that's an error.
            if neg1 || (neg2 && op != BinOp::Add) {
                return Err(Err::Error(ParseExprError {
                    input: input.to_owned(),
                    kind: ParseExprErrorKind::UnexpectedNegativeNumber,
                }));
            }

            // Replace `e -n +` by `e n -`.
            let op = match op {
                BinOp::Add if neg2 => BinOp::Sub,
                _ => op,
            };

            stack.push((Expr::Op(Box::new(e1), Box::new(e2), op), false));
            input = rest;
            if stack.len() == 1 {
                saved_input = input;
                saved_expr = stack[0].0.clone();
                saved_sign = stack[0].1;
            }
        } else {
            break;
        }
    }

    // If the last whole expression we parsed was negative, that's an error.
    if saved_sign {
        Err(Err::Error(ParseExprError {
            input: input.to_owned(),
            kind: ParseExprErrorKind::UnexpectedNegativeNumber,
        }))
    } else {
        Ok((saved_input, saved_expr))
    }
}

/// Parses an [expression](super::Expr).
///
/// It will fail if there is any input remaining afterwards.
pub fn expr_complete<T: RegisterValue>(input: &str) -> Result<Expr<T>, ParseExprError> {
    all_consuming(expr)(input).finish().map(|(_, expr)| expr)
}

/// Parses an [assignment](super::Assignment).
pub fn assignment<T: RegisterValue>(input: &str) -> IResult<&str, Assignment<T>, ParseExprError> {
    let (rest, (v, _, e, _, _)) =
        tuple((variable, multispace0, expr, multispace0, tag("=")))(input)?;
    Ok((rest, Assignment(v, e)))
}

/// Parses an [assignment](super::Assignment).
///
/// It will fail if there is any input remaining afterwards.
pub fn assignment_complete<T: RegisterValue>(input: &str) -> Result<Assignment<T>, ParseExprError> {
    all_consuming(assignment)(input).finish().map(|(_, a)| a)
}

/// Parses a sequence of [assignments](super::Assignment).
pub fn assignments<'a, T: 'a + RegisterValue>(
    input: &'a str,
) -> IResult<&'a str, Vec<Assignment<T>>, ParseExprError> {
    space_separated(assignment)(input)
}

/// Parses a sequence of [assignments](super::Assignment).
///
/// It will fail if there is any input remaining afterwards.
pub fn assignments_complete<T: RegisterValue>(
    input: &str,
) -> Result<Vec<Assignment<T>>, ParseExprError> {
    all_consuming(assignments)(input).finish().map(|(_, a)| a)
}

///Parses a [rule](super::Rule).
pub fn rule<T: RegisterValue>(input: &str) -> IResult<&str, Rule<T>, ParseExprError> {
    let (input, ident) = terminated(identifier, tag(":"))(input)?;
    let (rest, expr) = preceded(multispace0, expr)(input)?;

    Ok((rest, Rule(ident, expr)))
}

///Parses a [rule](super::Rule).
///
/// It will fail if there is any input remaining afterwards.
pub fn rule_complete<T: RegisterValue>(input: &str) -> Result<Rule<T>, ParseExprError> {
    all_consuming(rule)(input).finish().map(|(_, r)| r)
}

/// Parses a sequence of [rules](super::Rule).
pub fn rules<'a, T: 'a + RegisterValue>(
    input: &'a str,
) -> IResult<&'a str, Vec<Rule<T>>, ParseExprError> {
    space_separated(rule)(input)
}

/// Parses a sequence of [rules](super::Rule).
///
/// It will fail if there is any input remaining afterwards.
pub fn rules_complete<T: RegisterValue>(input: &str) -> Result<Vec<Rule<T>>, ParseExprError> {
    all_consuming(rules)(input).finish().map(|(_, a)| a)
}

#[cfg(test)]
mod test {
    use super::*;
    use nom::Finish;

    #[test]
    fn test_expr_1() {
        let input = "1 2 + 3 *";
        let (rest, parsed) = expr::<u8>(input).unwrap();
        assert_eq!(rest, "");
        insta::assert_debug_snapshot!(
            parsed,
            @r###"
        Op(
            Op(
                Value(
                    1,
                ),
                Value(
                    2,
                ),
                Add,
            ),
            Value(
                3,
            ),
            Mul,
        )
        "###
        );
    }

    #[test]
    fn test_var() {
        let input = "$foo bar";
        let v = Variable(String::from("$foo"));
        let (rest, parsed) = variable(input).unwrap();
        assert_eq!(rest, " bar");
        assert_eq!(parsed, v);
    }

    #[test]
    fn test_expr_2() {
        let input = "1 2 ^ + 3 $foo *";
        let (rest, parsed) = expr::<u8>(input).unwrap();
        assert_eq!(rest, " 3 $foo *");
        insta::assert_debug_snapshot!(parsed, @r###"
        Op(
            Value(
                1,
            ),
            Deref(
                Value(
                    2,
                ),
            ),
            Add,
        )
        "###);
    }

    #[test]
    fn test_negative() {
        let input = "13 -2 + .cfa";
        let (rest, parsed) = expr::<u8>(input).unwrap();
        assert_eq!(rest, " .cfa");
        insta::assert_debug_snapshot!(parsed, @r###"
        Op(
            Value(
                13,
            ),
            Value(
                2,
            ),
            Sub,
        )
        "###);
    }

    #[test]
    fn test_negative_bad_1() {
        let input = "-13 2 + .cfa";
        expr::<u8>(input).finish().unwrap_err();
    }

    #[test]
    fn test_negative_bad_2() {
        let input = "13 -2 * .cfa";
        expr::<u8>(input).finish().unwrap_err();
    }

    #[test]
    fn test_assignment() {
        let input = "$foo 4 ^ 7 @ =";
        let (rest, a) = assignment::<u8>(input).unwrap();
        assert_eq!(rest, "");
        insta::assert_debug_snapshot!(a, @r###"
        Assignment(
            Variable(
                "$foo",
            ),
            Op(
                Deref(
                    Value(
                        4,
                    ),
                ),
                Value(
                    7,
                ),
                Align,
            ),
        )
        "###);
    }

    #[test]
    fn test_assignment_2() {
        let input = "$foo 4 ^ = $bar .baz 17 + = 42";
        let (rest, assigns) = assignments::<u8>(input).unwrap();
        assert_eq!(rest, " 42");
        insta::assert_debug_snapshot!(assigns[0], @r###"
        Assignment(
            Variable(
                "$foo",
            ),
            Deref(
                Value(
                    4,
                ),
            ),
        )
        "###);
        insta::assert_debug_snapshot!(assigns[1], @r###"
        Assignment(
            Variable(
                "$bar",
            ),
            Op(
                Const(
                    Constant(
                        ".baz",
                    ),
                ),
                Value(
                    17,
                ),
                Add,
            ),
        )
        "###);
    }

    #[test]
    fn test_assignment_malformed() {
        let input = "$foo 4 ^ 7 =";
        assignment::<u8>(input).finish().unwrap_err();
    }

    #[test]
    fn rule_lhs() {
        let input = "$foo: ";

        base_expr::<u8>(input).unwrap_err();
    }

    #[test]
    fn test_rules() {
        let input = ".cfa: 7 $rax + $r0: $r1 ^   ";
        let (rest, rules) = rules::<u8>(input).unwrap();

        assert_eq!(rest, "   ");
        insta::assert_debug_snapshot!(rules[0], @r###"
        Rule(
            Const(
                Constant(
                    ".cfa",
                ),
            ),
            Op(
                Value(
                    7,
                ),
                Var(
                    Variable(
                        "$rax",
                    ),
                ),
                Add,
            ),
        )
        "###);
        insta::assert_debug_snapshot!(rules[1], @r###"
        Rule(
            Var(
                Variable(
                    "$r0",
                ),
            ),
            Deref(
                Var(
                    Variable(
                        "$r1",
                    ),
                ),
            ),
        )
        "###);
    }

    #[test]
    fn test_rules_complete() {
        let input = ".cfa: sp 80 + x29: .cfa -80 + ^ .ra: .cfa -72 + ^";
        rules_complete::<u64>(input).unwrap();
    }
}
