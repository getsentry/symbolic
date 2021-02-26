//! Functionality for evaluating *Breakpad
//! [RPN](https://en.wikipedia.org/wiki/Reverse_Polish_notation) expressions*.
//!
//! These expressions are defined by the following
//! [BNF](https://en.wikipedia.org/wiki/Backus%E2%80%93Naur_form) specification:
//! ```text
//! <expr>     ::=  <contant> | <variable> | <literal> | <expr> <expr> <binop> | <expr> ^
//! <constant> ::=  [a-zA-Z_.][a-zA-Z0-9_.]*
//! <variable> ::=  $[a-zA-Z][a-zA-Z0-9]*
//! <binop>    ::=  + | - | * | / | % | @
//! <literal>  ::=  -?[0-9]+
//! ```
//! Most of this syntax should be familiar. The symbol `^` denotes a dereference operation,
//! i.e. assuming that some representation `m` of a region of memory is available,
//! `x ^` evaluates to `m[x]`. If no memory is available or `m` is not defined at `x`, the
//! expression's value is undefined. The symbol
//! `@` denotes an align operation; it truncates its first operand to a multiple of its
//! second operand.
//!
//! Constants and variables are evaluated by referring to dictionaries
//! (concretely: [`BTreeMap`]s). If an expression contains a constant or variable that is
//! not in the respective dictionary, the expression's value is undefined.
//!
//! In addition to expressions, there are also *assignments*:
//! ```text
//! <assignment> ::=  <variable> <expr> =
//! ```
//! An assignment results in an update of the variable's value in the dictionary, or its
//! insertion if it was not defined before.
//!
use super::base::{Endianness, MemoryRegion, RegisterValue};
use parsing::ExprParsingError;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

/// Structure that encapsulates the information necessary to evaluate Breakpad
/// RPN expressions.
///
/// It is generic over:
/// - A region of memory
/// - An address type, which is used both for basic expressions and for pointers into `memory`
/// - An [`Endianness`](super::base::Endianness) that controls how values are read from memory
pub struct Evaluator<M, A, E> {
    /// A region of memory.
    ///
    /// If this is `None`, evaluation of expressions containing dereference
    /// operations will fail.
    pub memory: Option<M>,

    /// A map containing the values of constants.
    ///
    /// Trying to use a constant that is not in this map will cause evaluation to fail.
    pub constants: BTreeMap<Constant, A>,

    /// A map containing the values of variables.
    ///
    /// Trying to use a variable that is not in this map will cause evaluation to fail.
    /// This map can be modified by the [`assign`](Self::assign) and
    ///  [`process`](Self::process) methods.
    pub variables: BTreeMap<Variable, A>,

    /// The endianness the evaluator uses to read data from memory.
    pub endian: E,
}

impl<A: RegisterValue, M: MemoryRegion, E: Endianness> Evaluator<M, A, E> {
    /// Evaluates a single expression.
    ///
    /// This may fail if the expression tries to dereference unavailable memory
    /// or uses undefined constants or variables.
    pub fn evaluate(&self, expr: &Expr<A>) -> Result<A, EvaluationError<A>> {
        use Expr::*;
        match expr {
            Value(x) => Ok(*x),
            Const(c) => self
                .constants
                .get(&c)
                .copied()
                .ok_or_else(|| EvaluationError::UndefinedConstant(c.clone())),
            Var(v) => self
                .variables
                .get(&v)
                .copied()
                .ok_or_else(|| EvaluationError::UndefinedVariable(v.clone())),
            Op(e1, e2, op) => {
                let e1 = self.evaluate(&*e1)?;
                let e2 = self.evaluate(&*e2)?;
                match op {
                    BinOp::Add => Ok(e1 + e2),
                    BinOp::Sub => Ok(e1 - e2),
                    BinOp::Mul => Ok(e1 * e2),
                    BinOp::Div => Ok(e1 / e2),
                    BinOp::Mod => Ok(e1 % e2),
                    BinOp::Align => Ok(e2 * (e1 / e2)),
                }
            }
            Deref(address) => {
                let address = self.evaluate(&*address)?;
                let memory = self
                    .memory
                    .as_ref()
                    .ok_or(EvaluationError::MemoryUnavailable)?;
                memory
                    .get(address, self.endian)
                    .ok_or(EvaluationError::IllegalMemoryAccess {
                        address,
                        bytes: A::WIDTH,
                    })
            }
        }
    }

    /// Performs an assignment by first evaluating its right-hand side and then
    /// modifying [`variables`](Self::variables) accordingly.
    ///
    /// This may fail if the right-hand side cannot be evaluated, cf.
    /// [`evaluate`](Self::evaluate). It returns `true` iff the assignment modified
    /// an existing variable.
    pub fn assign(&mut self, Assignment(v, e): &Assignment<A>) -> Result<bool, EvaluationError<A>> {
        let value = self.evaluate(e)?;
        Ok(self.variables.insert(v.clone(), value).is_some())
    }
}
impl<A: RegisterValue + FromStr, M: MemoryRegion, E: Endianness> Evaluator<M, A, E> {
    /// Processes a string of assignments, modifying its [`variables`](Self::variables)
    /// field accordingly.
    ///
    /// This may fail if parsing goes wrong or a parsed assignment cannot be handled,
    /// cf. [`assign`](Self::assign). It returns the set of variables that were assigned
    /// a value by some assignment, even if the variable's value did not change.
    ///
    /// # Example
    /// ```
    /// # use std::collections::{BTreeMap, BTreeSet};
    /// # use symbolic_unwind::base::{BigEndian, MemorySlice};
    /// # use symbolic_unwind::evaluator::{Variable, Evaluator};
    /// let input = "$foo $bar 5 + = $bar 17 =";
    /// let mut variables = BTreeMap::new();
    /// let foo: Variable = "$foo".parse().unwrap();
    /// let bar: Variable = "$bar".parse().unwrap();
    /// variables.insert(bar.clone(), 17u8);
    /// let mut evaluator: Evaluator<MemorySlice, _, _> = Evaluator {
    ///     memory: None,
    ///     constants: BTreeMap::new(),
    ///     variables,
    ///     endian: BigEndian, // does not matter, we don't use memory anyway
    /// };
    ///
    /// let changed_variables = evaluator.process(input).unwrap();
    ///
    /// assert_eq!(changed_variables, vec![foo, bar].into_iter().collect());
    ///
    /// ```
    pub fn process<'a>(
        &'a mut self,
        input: &'a str,
    ) -> Result<BTreeSet<Variable>, ExpressionError<&'a str, A>> {
        let mut changed_variables = BTreeSet::new();
        let assignments = parsing::assignments_complete::<A>(input)?;
        for a in assignments {
            self.assign(&a)?;
            changed_variables.insert(a.0);
        }

        Ok(changed_variables)
    }
}

/// An error encountered while evaluating an expression.
#[derive(Debug)]
pub enum EvaluationError<A: RegisterValue> {
    /// The expression contains an undefined constant.
    UndefinedConstant(Constant),

    /// The expression contains an undefined variable.
    UndefinedVariable(Variable),

    /// The expression contains a dereference, but the evaluator does not have access
    /// to any memory.
    MemoryUnavailable,

    /// The requested piece of memory would exceed the bounds of the memory region.
    IllegalMemoryAccess {
        /// The number of bytes that were tried to read.
        bytes: usize,
        /// The address at which the read was attempted.
        address: A,
    },
}

/// An error encountered while parsing or evaluating an expression.
#[derive(Debug)]
pub enum ExpressionError<I, A: RegisterValue> {
    /// An error was encountered while parsing an expression.
    Parsing(ExprParsingError<I>),

    /// An error was encountered while evaluating an expression.
    Evaluation(EvaluationError<A>),
}

impl<I, A: RegisterValue> From<ExprParsingError<I>> for ExpressionError<I, A> {
    fn from(other: ExprParsingError<I>) -> Self {
        Self::Parsing(other)
    }
}

impl<I, A: RegisterValue> From<EvaluationError<A>> for ExpressionError<I, A> {
    fn from(other: EvaluationError<A>) -> Self {
        Self::Evaluation(other)
    }
}

/// A variable.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Variable(String);

impl fmt::Display for Variable {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for Variable {
    type Err = ExprParsingError<String>;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        parsing::variable_complete(input).map_err(|e| ExprParsingError {
            kind: e.kind,
            input: e.input.to_string(),
        })
    }
}

/// A constant value.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Constant(String);

impl fmt::Display for Constant {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for Constant {
    type Err = ExprParsingError<String>;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        parsing::constant_complete(input).map_err(|e| ExprParsingError {
            kind: e.kind,
            input: e.input.to_string(),
        })
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
///
/// This is generic so that different number types can be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr<T> {
    /// A base value.
    Value(T),
    /// A named constant.
    Const(Constant),
    /// A variable.
    Var(Variable),
    /// An expression `a b §`, where `§` is a [binary operator](BinOp).
    Op(Box<Expr<T>>, Box<Expr<T>>, BinOp),
    /// A dereferenced subexpression.
    Deref(Box<Expr<T>>),
}

impl<T: fmt::Display> fmt::Display for Expr<T> {
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

impl<T: FromStr> FromStr for Expr<T> {
    type Err = ExprParsingError<String>;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        parsing::expr_complete(input).map_err(|e| ExprParsingError {
            kind: e.kind,
            input: e.input.to_string(),
        })
    }
}

/// An assignment `v e =` where `v` is a [variable](Variable) and `e` is an [expression](Expr).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assignment<T>(Variable, Expr<T>);

impl<T: fmt::Display> fmt::Display for Assignment<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {} =", self.0, self.1)
    }
}

impl<T: FromStr> FromStr for Assignment<T> {
    type Err = ExprParsingError<String>;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        parsing::assignment_complete(input).map_err(|e| ExprParsingError {
            kind: e.kind,
            input: e.input.to_string(),
        })
    }
}

pub mod parsing {
    //! Contains functions for parsing [expressions](super::Expr) and
    //! [assignments](super::Assignment).
    //!
    //! This is brought to you by [`nom`].

    use super::*;
    use nom::branch::alt;
    use nom::bytes::complete::tag;
    use nom::character::complete::{
        alpha1, alphanumeric0, alphanumeric1, char, digit1, multispace0,
    };
    use nom::combinator::{all_consuming, map, map_res, opt, recognize, value};
    use nom::error::ParseError;
    use nom::multi::many0;
    use nom::sequence::{delimited, pair, preceded, tuple};
    use nom::{Err, Finish, IResult};
    use std::str::FromStr;

    /// The error kind for [`ExprParsingError`].
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum ExprParsingErrorKind {
        /// An operator was encountered, but there were not enough operands on the stack.
        NotEnoughOperands,

        /// A variable was expected, but the identifier did not start with a `$`.
        IllegalVariableName,

        /// More than one expression preceded a `=`.
        MalformedAssignment,

        /// Only one expression was expected, but multiple were parsed.
        TooManyExpressions,

        /// An error returned by `nom`.
        Nom(nom::error::ErrorKind),
    }

    /// An error encountered while parsing expressions.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct ExprParsingError<I> {
        /// The kind of error.
        pub kind: ExprParsingErrorKind,
        /// The input that caused the error.
        pub input: I,
    }

    impl<I> ParseError<I> for ExprParsingError<I> {
        fn from_error_kind(input: I, kind: nom::error::ErrorKind) -> Self {
            Self {
                input,
                kind: ExprParsingErrorKind::Nom(kind),
            }
        }

        fn append(_input: I, _kind: nom::error::ErrorKind, other: Self) -> Self {
            other
        }
    }

    impl<I, E> nom::error::FromExternalError<I, E> for ExprParsingError<I> {
        fn from_external_error(input: I, kind: nom::error::ErrorKind, _e: E) -> Self {
            Self::from_error_kind(input, kind)
        }
    }

    /// Parses a [variable](super::Variable).
    ///
    /// This accepts identifiers of the form `$[a-zA-Z][a-zA-Z0-9]*`.
    fn variable(input: &str) -> IResult<&str, Variable, ExprParsingError<&str>> {
        let (rest, var) = recognize(tuple((char('$'), alpha1, alphanumeric0)))(input)?;
        Ok((rest, Variable(var.to_string())))
    }

    /// Parses a [variable](super::Variable).
    ///
    /// This accepts identifiers of the form `$[a-zA-Z][a-zA-Z0-9]*`.
    /// It will fail if there is any non-whitespace input remaining afterwards.
    pub fn variable_complete(input: &str) -> Result<Variable, ExprParsingError<&str>> {
        all_consuming(variable)(input).finish().map(|(_, v)| v)
    }

    /// Parses a [constant](super::Constant).
    ///
    /// This accepts identifiers of the form `[a-zA-Z_.][a-zA-Z0-9_.]*`.
    fn constant(input: &str) -> IResult<&str, Constant, ExprParsingError<&str>> {
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
    pub fn constant_complete(input: &str) -> Result<Constant, ExprParsingError<&str>> {
        all_consuming(constant)(input).finish().map(|(_, c)| c)
    }

    /// Parses a [binary operator](super::BinOp).
    fn bin_op(input: &str) -> IResult<&str, BinOp, ExprParsingError<&str>> {
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
    fn number<T: FromStr>(input: &str) -> IResult<&str, T, ExprParsingError<&str>> {
        map_res(recognize(pair(opt(tag("-")), digit1)), |s: &str| {
            s.parse::<T>()
        })(input)
    }

    /// Parses a number, variable, or constant.
    fn base_expr<T: FromStr>(input: &str) -> IResult<&str, Expr<T>, ExprParsingError<&str>> {
        alt((
            map(number, Expr::Value),
            map(variable, Expr::Var),
            map(constant, Expr::Const),
        ))(input)
    }

    /// Parses a stack of [expressions](super::Expression).
    ///
    /// # Example
    /// ```rust
    /// use symbolic_unwind::evaluator::Expr::*;
    /// use symbolic_unwind::evaluator::BinOp::*;
    /// # use symbolic_unwind::evaluator::parsing::expr_stack;
    ///
    /// let (_, stack) = expr_stack("1 2 + 3").unwrap();
    /// assert_eq!(stack.len(), 2);
    /// assert_eq!(stack[0], Op(Box::new(Value(1)), Box::new(Value(2)), Add));
    /// assert_eq!(stack[1], Value(3));
    /// ```
    pub fn expr_stack<T: FromStr>(
        mut input: &str,
    ) -> IResult<&str, Vec<Expr<T>>, ExprParsingError<&str>> {
        let mut stack = Vec::new();

        while !input.is_empty() {
            if let Ok((rest, e)) = delimited(multispace0, base_expr, multispace0)(input) {
                stack.push(e);
                input = rest;
            } else if let Ok((rest, _)) = delimited::<_, _, _, _, ExprParsingError<&str>, _, _, _>(
                multispace0,
                tag("^"),
                multispace0,
            )(input)
            {
                let e = match stack.pop() {
                    Some(e) => e,
                    None => {
                        return Err(Err::Error(ExprParsingError {
                            input,
                            kind: ExprParsingErrorKind::NotEnoughOperands,
                        }))
                    }
                };

                stack.push(Expr::Deref(Box::new(e)));
                input = rest;
            } else if let Ok((rest, op)) = delimited(multispace0, bin_op, multispace0)(input) {
                let e2 = match stack.pop() {
                    Some(e) => e,
                    None => {
                        return Err(Err::Error(ExprParsingError {
                            input,
                            kind: ExprParsingErrorKind::NotEnoughOperands,
                        }))
                    }
                };

                let e1 = match stack.pop() {
                    Some(e) => e,
                    None => {
                        return Err(Err::Error(ExprParsingError {
                            input,
                            kind: ExprParsingErrorKind::NotEnoughOperands,
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
    pub fn expr_complete<T: FromStr>(input: &str) -> Result<Expr<T>, ExprParsingError<&str>> {
        let (_, mut stack) = all_consuming(expr_stack)(input).finish()?;
        if stack.len() > 1 {
            Err(ExprParsingError {
                kind: ExprParsingErrorKind::TooManyExpressions,
                input,
            })
        } else {
            // This unwrap cannot fail: if the parser succeded, the stack is nonempty.
            Ok(stack.pop().unwrap())
        }
    }

    /// Parses an [assignment](super::Assignment).
    fn assignment<T: FromStr>(input: &str) -> IResult<&str, Assignment<T>, ExprParsingError<&str>> {
        let (input, v) = delimited(multispace0, variable, multispace0)(input)?;
        let (input, mut stack) = expr_stack(input)?;

        // At this point there should be exactly one expression on the stack, otherwise
        // it's not a well-formed assignment.
        if stack.len() > 1 {
            return Err(Err::Error(ExprParsingError {
                input,
                kind: ExprParsingErrorKind::MalformedAssignment,
            }));
        }

        let e = match stack.pop() {
            Some(e) => e,
            None => {
                return Err(Err::Error(ExprParsingError {
                    input,
                    kind: ExprParsingErrorKind::NotEnoughOperands,
                }))
            }
        };

        let (rest, _) = preceded(multispace0, tag("="))(input)?;
        Ok((rest, Assignment(v, e)))
    }

    /// Parses an [assignment](super::Assignment).
    ///
    /// It will fail if there is any non-whitespace input remaining afterwards.
    pub fn assignment_complete<T: FromStr>(
        input: &str,
    ) -> Result<Assignment<T>, ExprParsingError<&str>> {
        all_consuming(assignment)(input).finish().map(|(_, a)| a)
    }

    /// Parses a sequence of [assignments](super::Assignment).
    ///
    /// It will fail if there is any non-whitespace input remaining afterwards.
    pub fn assignments_complete<T: FromStr + std::fmt::Debug>(
        input: &str,
    ) -> Result<Vec<Assignment<T>>, ExprParsingError<&str>> {
        let (_, assigns) =
            all_consuming(many0(delimited(multispace0, assignment, multispace0)))(input)
                .finish()?;
        Ok(assigns)
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
            let (rest, parsed) = expr_stack(input).unwrap();
            assert_eq!(rest, "");
            assert_eq!(parsed, vec![e1, e2]);
        }

        #[test]
        fn test_expr_malformed() {
            let input = "3 +";
            let err = expr_stack::<i8>(input).finish().unwrap_err();
            assert_eq!(
                err,
                ExprParsingError {
                    input: "+",
                    kind: ExprParsingErrorKind::NotEnoughOperands,
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
            let err = assignment::<i8>(input).finish().unwrap_err();
            assert_eq!(
                err,
                ExprParsingError {
                    input: "=",
                    kind: ExprParsingErrorKind::MalformedAssignment,
                }
            );
        }
    }
}

/// These tests are inspired by the Breakpad PostfixEvaluator unit tests:
/// [https://github.com/google/breakpad/blob/main/src/processor/postfix_evaluator_unittest.cc]
#[cfg(test)]
mod test {
    use super::*;
    use crate::base::BigEndian;

    /// A fake [`MemoryRegion`](MemoryRegion) that always returns the requested address + 1.
    struct FakeMemoryRegion;

    impl MemoryRegion for FakeMemoryRegion {
        fn base_addr(&self) -> u64 {
            0
        }

        fn size(&self) -> usize {
            0
        }

        fn is_empty(&self) -> bool {
            true
        }

        fn get<A: RegisterValue, E: Endianness>(&self, address: A, _e: E) -> Option<A> {
            Some(address + 1.into())
        }
    }

    #[test]
    fn test_assignment() {
        let input = "$rAdd3 2 2 + =$rMul2 9 6 * =";

        let mut eval: Evaluator<FakeMemoryRegion, u64, BigEndian> = Evaluator {
            memory: None,
            variables: BTreeMap::new(),
            constants: BTreeMap::new(),
            endian: BigEndian,
        };
        let r_add3: Variable = "$rAdd3".parse().unwrap();
        let r_mul2: Variable = "$rMul2".parse().unwrap();

        let changed_vars = eval.process(input).unwrap();

        assert_eq!(
            changed_vars,
            vec![r_add3.clone(), r_mul2.clone(),].into_iter().collect()
        );

        assert_eq!(eval.variables[&r_add3], 4);
        assert_eq!(eval.variables[&r_mul2], 54);
    }

    #[test]
    fn test_deref() {
        let input = "$rDeref 9 ^ =";

        let mut eval: Evaluator<_, u64, BigEndian> = Evaluator {
            memory: Some(FakeMemoryRegion),
            variables: BTreeMap::new(),
            constants: BTreeMap::new(),
            endian: BigEndian,
        };

        let r_deref: Variable = "$rDeref".parse().unwrap();

        let changed_vars = eval.process(input).unwrap();

        assert_eq!(changed_vars, vec![r_deref.clone()].into_iter().collect());

        assert_eq!(eval.variables[&r_deref], 10);
    }

    #[test]
    fn test_intermediate() {
        let ebp: Variable = "$ebp".parse().unwrap();
        let eip: Variable = "$eip".parse().unwrap();
        let esp: Variable = "$esp".parse().unwrap();
        let ebx: Variable = "$ebx".parse().unwrap();
        let t0: Variable = "$T0".parse().unwrap();
        let t1: Variable = "$T1".parse().unwrap();
        let t2: Variable = "$T2".parse().unwrap();
        let l: Variable = "$L".parse().unwrap();
        let p: Variable = "$P".parse().unwrap();

        let variables = vec![
            (ebp.clone(), 0xbfff_0010),
            (eip.clone(), 0x1000_0000),
            (esp.clone(), 0xbfff_0000),
        ]
        .into_iter()
        .collect();
        let cb_saved_regs = Constant(".cbSavedRegs".to_string());
        let cb_params = Constant(".cbParams".to_string());
        let ra_search_start = Constant(".raSearchStart".to_string());

        let constants = vec![
            (cb_saved_regs, 4),
            (cb_params, 4),
            (ra_search_start, 0xbfff_0020),
        ]
        .into_iter()
        .collect();

        let mut eval: Evaluator<_, u64, BigEndian> = Evaluator {
            memory: Some(FakeMemoryRegion),
            variables,
            constants,
            endian: BigEndian,
        };

        let mut changed_vars = BTreeSet::new();

        changed_vars.append(
            &mut eval
                .process(
                    "$T0 $ebp = $eip $T0 4 + ^ = $ebp $T0 ^ = $esp $T0 8 + = 
             $L $T0 .cbSavedRegs - = $P $T0 8 + .cbParams + =",
                )
                .unwrap(),
        );

        changed_vars.append(
            &mut eval
                .process(
                    "$T0 $ebp = $eip $T0 4 + ^ = $ebp $T0 ^ = $esp $T0 8 + = 
             $L $T0 .cbSavedRegs - = $P $T0 8 + .cbParams + = $ebx $T0 28 - ^ =",
                )
                .unwrap(),
        );

        changed_vars.append(
            &mut eval
                .process(
                    "$T0 $ebp = $T2 $esp = $T1 .raSearchStart = $eip $T1 ^ = $ebp $T0 = 
             $esp $T1 4 + = $L $T0 .cbSavedRegs - = $P $T1 4 + .cbParams + =
             $ebx $T0 28 - ^ =",
                )
                .unwrap(),
        );

        for (var, val) in [
            (ebp, 0xbfff_0012),
            (ebx, 0xbffe_fff7),
            (eip, 0xbfff_0021),
            (esp, 0xbfff_0024),
            (l, 0xbfff_000e),
            (p, 0xbfff_0028),
            (t0, 0xbfff_0012),
            (t1, 0xbfff_0020),
            (t2, 0xbfff_0019),
        ]
        .iter()
        {
            assert_eq!(eval.variables[var], *val);
        }
    }
}
