//! Contains functions for parsing [expressions](super::Expr) and
//! [assignments](super::Assignment).
//!
//! This is brought to you by [`nom`].
use std::error::Error;
use std::fmt;

use nom::branch::alt;
use nom::bytes::complete::{tag, take_while};
use nom::character::complete::{alpha1, alphanumeric0, alphanumeric1, char, multispace0};
use nom::combinator::{all_consuming, map, map_res, recognize, value};
use nom::error::ParseError;
use nom::multi::many0;
use nom::sequence::{delimited, preceded, tuple};
use nom::{Err, Finish, IResult};

use super::*;

/// The error kind for [`ParseExprError`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParseExprErrorKind {
    /// An operator was encountered, but there were not enough operands on the stack.
    NotEnoughOperands,

    /// More than one expression preceded a `=`.
    MalformedAssignment,

    /// Only one expression was expected, but multiple were parsed.
    TooManyExpressions,

    /// An error returned by `nom`.
    Nom(nom::error::ErrorKind),
}

impl fmt::Display for ParseExprErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::NotEnoughOperands => write!(f, "Not enough operands on the stack"),
            Self::MalformedAssignment => write!(f, "Tried to parse an assignment, but there was more than one expression on the stack"),
            Self::TooManyExpressions => write!(f, "Exactly one expression was expected, but multiple were found. Possibly missing postfix operators?"),
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

/// Parses a [variable](super::Variable).
///
/// This accepts identifiers of the form `$[a-zA-Z][a-zA-Z0-9]*`.
fn variable(input: &str) -> IResult<&str, Variable, ParseExprError> {
    let (rest, var) = recognize(tuple((char('$'), alpha1, alphanumeric0)))(input)?;
    Ok((rest, Variable(var.to_string())))
}

/// Parses a [variable](super::Variable).
///
/// This accepts identifiers of the form `$[a-zA-Z][a-zA-Z0-9]*`.
/// It will fail if there is any non-whitespace input remaining afterwards.
pub fn variable_complete(input: &str) -> Result<Variable, ParseExprError> {
    all_consuming(variable)(input).finish().map(|(_, v)| v)
}

/// Parses a [constant](super::Constant).
///
/// This accepts identifiers of the form `[a-zA-Z_.][a-zA-Z0-9_.]*`.
fn constant(input: &str) -> IResult<&str, Constant, ParseExprError> {
    let (rest, con) = recognize(preceded(
        alt((alpha1, tag("_"), tag("."))),
        many0(alt((alphanumeric1, tag("_"), tag(".")))),
    ))(input)?;
    Ok((rest, Constant(con.to_string())))
}

/// Parses a [constant](super::Constant).
///
/// This accepts identifiers of the form `[a-zA-Z_.][a-zA-Z0-9_.]*`.
/// It will fail if there is any non-whitespace input remaining afterwards.
pub fn constant_complete(input: &str) -> Result<Constant, ParseExprError> {
    all_consuming(constant)(input).finish().map(|(_, c)| c)
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
/// This accepts expressions of the form `[0-9a-fA-F]+`.
fn number<T: RegisterValue>(input: &str) -> IResult<&str, T, ParseExprError> {
    map_res(
        recognize(take_while(|c: char| !c.is_ascii_whitespace())),
        T::from_str_hex,
    )(input)
}

/// Parses a number, variable, or constant.
fn base_expr<T: RegisterValue>(input: &str) -> IResult<&str, Expr<T>, ParseExprError> {
    alt((
        map(number, Expr::Value),
        map(variable, Expr::Var),
        map(constant, Expr::Const),
    ))(input)
}

/// Parses a stack of [expressions](super::Expr).
///
/// # Example
/// ```rust
/// use symbolic_unwind::evaluator::parsing::expr_stack;
/// use symbolic_unwind::evaluator::BinOp::*;
/// use symbolic_unwind::evaluator::Expr::*;
///
/// let (_, stack) = expr_stack::<u8>("1 2 + 3").unwrap();
/// assert_eq!(stack.len(), 2);
/// assert_eq!(stack[0], Op(Box::new(Value(1)), Box::new(Value(2)), Add));
/// assert_eq!(stack[1], Value(3));
/// ```
pub fn expr_stack<T: RegisterValue>(
    mut input: &str,
) -> IResult<&str, Vec<Expr<T>>, ParseExprError> {
    let mut stack = Vec::new();

    while !input.is_empty() {
        if let Ok((rest, e)) = delimited(multispace0, base_expr, multispace0)(input) {
            stack.push(e);
            input = rest;
        } else if let Ok((rest, _)) =
            delimited::<_, _, _, _, ParseExprError, _, _, _>(multispace0, tag("^"), multispace0)(
                input,
            )
        {
            let e = match stack.pop() {
                Some(e) => e,
                None => {
                    return Err(Err::Error(ParseExprError {
                        input: input.to_owned(),
                        kind: ParseExprErrorKind::NotEnoughOperands,
                    }))
                }
            };

            stack.push(Expr::Deref(Box::new(e)));
            input = rest;
        } else if let Ok((rest, op)) = delimited(multispace0, bin_op, multispace0)(input) {
            let e2 = match stack.pop() {
                Some(e) => e,
                None => {
                    return Err(Err::Error(ParseExprError {
                        input: input.to_string(),
                        kind: ParseExprErrorKind::NotEnoughOperands,
                    }))
                }
            };

            let e1 = match stack.pop() {
                Some(e) => e,
                None => {
                    return Err(Err::Error(ParseExprError {
                        input: input.to_string(),
                        kind: ParseExprErrorKind::NotEnoughOperands,
                    }))
                }
            };
            stack.push(Expr::Op(Box::new(e1), Box::new(e2), op));
            input = rest;
        } else {
            break;
        }
    }

    Ok((input, stack))
}

/// Parses an [expression](super::Expr).
///
/// It will fail if there is any non-whitespace input remaining afterwards.
pub fn expr_complete<T: RegisterValue>(input: &str) -> Result<Expr<T>, ParseExprError> {
    let (_, mut stack) = all_consuming(expr_stack)(input).finish()?;
    if stack.len() > 1 {
        Err(ParseExprError {
            kind: ParseExprErrorKind::TooManyExpressions,
            input: input.to_string(),
        })
    } else {
        // This unwrap cannot fail: if the parser succeded, the stack is nonempty.
        Ok(stack.pop().unwrap())
    }
}

/// Parses an [assignment](super::Assignment).
fn assignment<T: RegisterValue>(input: &str) -> IResult<&str, Assignment<T>, ParseExprError> {
    let (input, v) = delimited(multispace0, variable, multispace0)(input)?;
    let (input, mut stack) = expr_stack(input)?;

    // At this point there should be exactly one expression on the stack, otherwise
    // it's not a well-formed assignment.
    if stack.len() > 1 {
        return Err(Err::Error(ParseExprError {
            input: input.to_string(),
            kind: ParseExprErrorKind::MalformedAssignment,
        }));
    }

    let e = match stack.pop() {
        Some(e) => e,
        None => {
            return Err(Err::Error(ParseExprError {
                input: input.to_string(),
                kind: ParseExprErrorKind::NotEnoughOperands,
            }))
        }
    };

    let (rest, _) = preceded(multispace0, tag("="))(input)?;
    Ok((rest, Assignment(v, e)))
}

/// Parses an [assignment](super::Assignment).
///
/// It will fail if there is any non-whitespace input remaining afterwards.
pub fn assignment_complete<T: RegisterValue>(input: &str) -> Result<Assignment<T>, ParseExprError> {
    all_consuming(assignment)(input).finish().map(|(_, a)| a)
}

/// Parses a sequence of [assignments](super::Assignment).
///
/// It will fail if there is any non-whitespace input remaining afterwards.
pub fn assignments_complete<T: RegisterValue + fmt::Debug>(
    input: &str,
) -> Result<Vec<Assignment<T>>, ParseExprError> {
    let (_, assigns) =
        all_consuming(many0(delimited(multispace0, assignment, multispace0)))(input).finish()?;
    Ok(assigns)
}

#[cfg(test)]
mod test {
    use super::*;
    use nom::Finish;

    #[test]
    fn test_expr_1() {
        use Expr::*;
        let input = "1 2 + 3 *";
        let e = Op(
            Box::new(Op(Box::new(Value(1u8)), Box::new(Value(2)), BinOp::Add)),
            Box::new(Value(3)),
            BinOp::Mul,
        );
        let (rest, parsed) = expr_stack(input).unwrap();
        assert_eq!(rest, "");
        assert_eq!(parsed, vec![e]);
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
        use Expr::*;
        let input = "1 2 ^ + 3 $foo *";
        let e1 = Op(
            Box::new(Value(1u8)),
            Box::new(Deref(Box::new(Value(2)))),
            BinOp::Add,
        );
        let e2 = Op(
            Box::new(Value(3)),
            Box::new(Var(Variable(String::from("$foo")))),
            BinOp::Mul,
        );
        let (rest, parsed) = expr_stack(input).unwrap();
        assert_eq!(rest, "");
        assert_eq!(parsed, vec![e1, e2]);
    }

    #[test]
    fn test_expr_malformed() {
        let input = "3 +";
        let err = expr_stack::<u8>(input).finish().unwrap_err();
        assert_eq!(
            err,
            ParseExprError {
                input: "+".to_string(),
                kind: ParseExprErrorKind::NotEnoughOperands,
            }
        );
    }

    #[test]
    fn test_assignment() {
        use Expr::*;
        let input = "$foo 4 ^ 7 @ =";
        let v = Variable("$foo".to_string());
        let e = Op(
            Box::new(Deref(Box::new(Value(4)))),
            Box::new(Value(7)),
            BinOp::Align,
        );

        let (rest, a) = assignment::<u32>(input).unwrap();
        assert_eq!(rest, "");
        assert_eq!(a, Assignment(v, e));
    }

    #[test]
    fn test_assignment_2() {
        use nom::multi::many1;
        use Expr::*;
        let input = "$foo 4 ^ = $bar baz a7 + = 42";
        let (v1, v2) = (Variable("$foo".to_string()), Variable("$bar".to_string()));
        let e1 = Deref(Box::new(Value(4u8)));
        let e2 = Op(
            Box::new(Const(Constant("baz".to_string()))),
            Box::new(Value(0xa7)),
            BinOp::Add,
        );

        let (rest, assigns) = many1(preceded(multispace0, assignment))(input).unwrap();
        assert_eq!(rest, " 42");
        assert_eq!(assigns[0], Assignment(v1, e1));
        assert_eq!(assigns[1], Assignment(v2, e2));
    }

    #[test]
    fn test_assignment_malformed() {
        let input = "$foo 4 ^ 7 =";
        let err = assignment::<u8>(input).finish().unwrap_err();
        assert_eq!(
            err,
            ParseExprError {
                input: "=".to_string(),
                kind: ParseExprErrorKind::MalformedAssignment,
            }
        );
    }
}
