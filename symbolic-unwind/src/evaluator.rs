use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variable(String);

impl fmt::Display for Variable {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Constant(String);

impl fmt::Display for Constant {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A binary operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    /// Addition.
    Add,
    /// Subtraction.
    Sub,
    /// Multiplication.
    Mul,
    /// Division.
    Div,
    /// Remainder.
    Mod,
    /// Alignment.
    ///
    /// Truncates the first operand to a multiple of the second operand.
    Align,
}

impl fmt::Display for BinOp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Add => write!(f, "+"),
            Self::Sub => write!(f, "-"),
            Self::Mul => write!(f, "*"),
            Self::Div => write!(f, "/"),
            Self::Mod => write!(f, "%"),
            Self::Align => write!(f, "@"),
        }
    }
}

/// An expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    /// An integer value.
    Value(i64),
    /// A named constant.
    Const(Constant),
    /// A variable.
    Var(Variable),
    /// An expression `a b §`, where `§` is a [binary operator](BinOp).
    Op(Box<Expr>, Box<Expr>, BinOp),
    /// A dereferenced subexpression.
    Deref(Box<Expr>),
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Value(n) => write!(f, "{}", n),
            Self::Const(c) => write!(f, "{}", c),
            Self::Var(v) => write!(f, "{}", v),
            Self::Op(x, y, op) => write!(f, "{} {} {}", x, y, op),
            Self::Deref(x) => write!(f, "{} ^", x),
        }
    }
}

/// An assignment `v e =` where `v` is a [variable](Variable) and `e` is an [expression](Expr).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assignment(Variable, Expr);

impl fmt::Display for Assignment {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {} =", self.0, self.1)
    }
}

pub mod parsing {
    //! Contains functions for parsing [expressions](super::Expr).
    //!
    //! This is implemented using `nom`.
    use super::*;
    use nom::branch::alt;
    use nom::bytes::complete::tag;
    use nom::character::complete::{alphanumeric1, digit1, space0};
    use nom::combinator::{map, map_res, not, opt, recognize, value};
    use nom::error::ParseError;
    use nom::sequence::{delimited, pair, preceded};
    use nom::{Err, IResult};

    /// The error kind for [`ExpressionError`].
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum ExpressionErrorKind {
        /// An operator was encountered, but there were not enough operands on the stack.
        NotEnoughOperands,

        /// More than one expression preceded a `=`.
        MalformedAssignment,

        /// An error returned by `nom`.
        Nom(nom::error::ErrorKind),
    }

    /// An error encountered while parsing expressions.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct ExpressionError<I> {
        kind: ExpressionErrorKind,
        input: I,
    }

    impl<I> ParseError<I> for ExpressionError<I> {
        fn from_error_kind(input: I, kind: nom::error::ErrorKind) -> Self {
            Self {
                input,
                kind: ExpressionErrorKind::Nom(kind),
            }
        }

        fn append(_input: I, _kind: nom::error::ErrorKind, other: Self) -> Self {
            other
        }
    }

    impl<I, E> nom::error::FromExternalError<I, E> for ExpressionError<I> {
        fn from_external_error(input: I, kind: nom::error::ErrorKind, _e: E) -> Self {
            Self::from_error_kind(input, kind)
        }
    }

    /// Parses a [variable](super::Variable).
    fn variable(input: &str) -> IResult<&str, Variable, ExpressionError<&str>> {
        let (input, _) = tag("$")(input)?;
        let (rest, var) = alphanumeric1(input)?;
        Ok((rest, Variable(format!("${}", var))))
    }

    /// Parses a [constant](super::Constant).
    fn constant(input: &str) -> IResult<&str, Constant, ExpressionError<&str>> {
        let (input, _) = not(tag("$"))(input)?;
        let (rest, var) = alphanumeric1(input)?;
        Ok((rest, Constant(var.to_string())))
    }

    /// Parses a [binary operator](super::BinOp).
    fn bin_op(input: &str) -> IResult<&str, BinOp, ExpressionError<&str>> {
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
    fn number(input: &str) -> IResult<&str, i64, ExpressionError<&str>> {
        map_res(recognize(pair(opt(tag("-")), digit1)), |s: &str| {
            s.parse::<i64>()
        })(input)
    }

    /// Parses a number, variable, or constant.
    fn base_expr(input: &str) -> IResult<&str, Expr, ExpressionError<&str>> {
        alt((
            map(number, Expr::Value),
            map(variable, Expr::Var),
            map(constant, Expr::Const),
        ))(input)
    }

    /// Parses a stack of expressions.
    ///
    /// # Example
    /// ```rust
    /// use symbolic_unwind::evaluator::Expr::*;
    /// use symbolic_unwind::evaluator::BinOp::*;
    /// # use symbolic_unwind::evaluator::parsing::expr;
    ///
    /// let (_, stack) = expr("1 2 + 3").unwrap();
    /// assert_eq!(stack.len(), 2);
    /// assert_eq!(stack[0], Op(Box::new(Value(1)), Box::new(Value(2)), Add));
    /// assert_eq!(stack[1], Value(3));
    /// ```
    pub fn expr(mut input: &str) -> IResult<&str, Vec<Expr>, ExpressionError<&str>> {
        let mut stack = Vec::new();

        while !input.is_empty() {
            if let Ok((rest, e)) = delimited(space0, base_expr, space0)(input) {
                stack.push(e);
                input = rest;
            } else if let Ok((rest, op)) = delimited(space0, bin_op, space0)(input) {
                let e2 = match stack.pop() {
                    Some(e) => e,
                    None => {
                        return Err(Err::Error(ExpressionError {
                            input,
                            kind: ExpressionErrorKind::NotEnoughOperands,
                        }))
                    }
                };

                let e1 = match stack.pop() {
                    Some(e) => e,
                    None => {
                        return Err(Err::Error(ExpressionError {
                            input,
                            kind: ExpressionErrorKind::NotEnoughOperands,
                        }))
                    }
                };
                stack.push(Expr::Op(Box::new(e1), Box::new(e2), op));
                input = rest;
            } else if let Ok((rest, _)) =
                delimited::<_, _, _, _, ExpressionError<&str>, _, _, _>(space0, tag("^"), space0)(
                    input,
                )
            {
                let e = match stack.pop() {
                    Some(e) => e,
                    None => {
                        return Err(Err::Error(ExpressionError {
                            input,
                            kind: ExpressionErrorKind::NotEnoughOperands,
                        }))
                    }
                };

                stack.push(Expr::Deref(Box::new(e)));
                input = rest;
            } else {
                break;
            }
        }

        Ok((input, stack))
    }

    /// Parses an [assignment](Assignment).
    pub fn assignment(input: &str) -> IResult<&str, Assignment, ExpressionError<&str>> {
        let (input, v) = delimited(space0, variable, space0)(input)?;
        let (input, mut stack) = expr(input)?;

        // At this point there should be exactly one expression on the stack, otherwise
        // it's not a well-formed assignment.
        if stack.len() > 1 {
            return Err(Err::Error(ExpressionError {
                input,
                kind: ExpressionErrorKind::MalformedAssignment,
            }));
        }

        let e = match stack.pop() {
            Some(e) => e,
            None => {
                return Err(Err::Error(ExpressionError {
                    input,
                    kind: ExpressionErrorKind::NotEnoughOperands,
                }))
            }
        };

        let (rest, _) = preceded(space0, tag("="))(input)?;

        Ok((rest, Assignment(v, e)))
    }

    #[cfg(test)]
    mod test {
        use super::*;
        use nom::Finish;

        #[test]
        fn test_expr_1() {
            use Expr::*;
            let input = "1 2 + -3 *";
            let e = Op(
                Box::new(Op(Box::new(Value(1)), Box::new(Value(2)), BinOp::Add)),
                Box::new(Value(-3)),
                BinOp::Mul,
            );
            let (rest, parsed) = expr(input).unwrap();
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
            let input = "1 2 ^ + -3 $foo *";
            let e1 = Op(
                Box::new(Value(1)),
                Box::new(Deref(Box::new(Value(2)))),
                BinOp::Add,
            );
            let e2 = Op(
                Box::new(Value(-3)),
                Box::new(Var(Variable(String::from("$foo")))),
                BinOp::Mul,
            );
            let (rest, parsed) = expr(input).unwrap();
            assert_eq!(rest, "");
            assert_eq!(parsed, vec![e1, e2]);
        }

        #[test]
        fn test_expr_malformed() {
            let input = "3 +";
            let err = expr(input).finish().unwrap_err();
            assert_eq!(
                err,
                ExpressionError {
                    input: "+",
                    kind: ExpressionErrorKind::NotEnoughOperands,
                }
            );
        }

        #[test]
        fn test_assignment() {
            use Expr::*;
            let input = "$foo -4 ^ 7 @ =";
            let v = Variable("$foo".to_string());
            let e = Op(
                Box::new(Deref(Box::new(Value(-4)))),
                Box::new(Value(7)),
                BinOp::Align,
            );

            let (rest, a) = assignment(input).unwrap();
            assert_eq!(rest, "");
            assert_eq!(a, Assignment(v, e));
        }

        #[test]
        fn test_assignment_2() {
            use nom::multi::many1;
            use Expr::*;
            let input = "$foo -4 ^ = $bar baz 17 + = -42";
            let (v1, v2) = (Variable("$foo".to_string()), Variable("$bar".to_string()));
            let e1 = Deref(Box::new(Value(-4)));
            let e2 = Op(
                Box::new(Const(Constant("baz".to_string()))),
                Box::new(Value(17)),
                BinOp::Add,
            );

            let (rest, assigns) = many1(assignment)(input).unwrap();
            assert_eq!(rest, " -42");
            assert_eq!(assigns[0], Assignment(v1, e1));
            assert_eq!(assigns[1], Assignment(v2, e2));
        }

        #[test]
        fn test_assignment_malformed() {
            let input = "$foo -4 ^ 7 =";
            let err = assignment(input).finish().unwrap_err();
            assert_eq!(
                err,
                ExpressionError {
                    input: "=",
                    kind: ExpressionErrorKind::MalformedAssignment,
                }
            );
        }
    }
}
