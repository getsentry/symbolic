//! Functionality for evaluating *Breakpad
//! [RPN](https://en.wikipedia.org/wiki/Reverse_Polish_notation) expressions*.
//!
//! These expressions are defined by the following
//! [BNF](https://en.wikipedia.org/wiki/Backus%E2%80%93Naur_form) specification:
//! ```text
//! <rule>       ::=  <identifier>: <expr>
//! <assignment> ::=  <variable> <expr> =
//! <expr>       ::=  <identifier> | <literal> | <expr> <expr> <binop> | <expr> ^
//! <identifier> ::=  <constant> | <variable>
//! <constant>   ::=  \.?[a-zA-Z][a-zA-Z0-9]*
//! <variable>   ::=  \$[a-zA-Z0-9]+
//! <binop>      ::=  + | - | * | / | % | @
//! <literal>    ::=  -?[0-9]+
//! ```
//! Most of this syntax should be familiar. The symbol `^` denotes a dereference operation,
//! i.e. assuming that some representation `m` of a region of memory is available,
//! `x ^` evaluates to `m[x]`. If no memory is available or `m` is not defined at `x`,
//! evaluating the expression will fail. The symbol
//! `@` denotes an align operation; it truncates its first operand to a multiple of its
//! second operand.
//!
//! Constants and variables are evaluated by referring to dictionaries
//! (concretely: [`BTreeMap`]s). If an expression contains a constant or variable that is
//! not in the respective dictionary, evaluating the expression will fail.
//!
//! # Assignments and rules
//!
//! Breakpad `STACK WIN` records (see [here](https://github.com/google/breakpad/blob/main/docs/symbol_files.md#stack-win-records)
//! can contain `program strings` that describe how to compute the values of registers
//! in an earlier call frame. These program strings are effectively sequences of the
//! assignments described above. They can be be parsed with the
//! [assignment](parsing::assignment), [assignment_complete](parsing::assignment_complete),
//! [assignments](parsing::assignments),
//! and [assignments_complete](parsing::assignments_complete) parsers.
//!
//! By contrast, Breakpad `STACK CFI` records (see [here](https://github.com/google/breakpad/blob/main/docs/symbol_files.md#stack-cfi-records)
//! contain sequences of rules for essentially the same purpose. They can be parsed with the
//! [rule](parsing::rule), [rule_complete](parsing::rule_complete),
//! [rules](parsing::rules),
//! and [rules_complete](parsing::rules_complete) parsers.
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::str::FromStr;

use super::base::{Endianness, MemoryRegion, RegisterValue};
use parsing::ParseExprError;

pub mod parsing;

#[cfg(test)]
mod strategies;

/// Structure that encapsulates the information necessary to evaluate Breakpad
/// RPN expressions.
///
/// It is generic over:
/// - An address type, which is used both for basic expressions and for pointers into `memory`
/// - An [`Endianness`](super::base::Endianness) that controls how values are read from memory
pub struct Evaluator<'memory, A, E> {
    /// A region of memory.
    ///
    /// If this is `None`, evaluation of expressions containing dereference
    /// operations will fail.
    memory: Option<MemoryRegion<'memory>>,

    /// A map containing the values of constants.
    ///
    /// Trying to use a constant that is not in this map will cause evaluation to fail.
    constants: BTreeMap<Constant, A>,

    /// A map containing the values of variables.
    ///
    /// Trying to use a variable that is not in this map will cause evaluation to fail.
    /// This map can be modified by the [`assign`](Self::assign) and
    ///  [`process`](Self::process) methods.
    variables: BTreeMap<Variable, A>,

    /// The endianness the evaluator uses to read data from memory.
    endian: E,
}

impl<'memory, A, E> Evaluator<'memory, A, E> {
    /// Creates an Evaluator with the given endianness, no memory, and empty
    /// constant and variable maps.
    pub fn new(endian: E) -> Self {
        Self {
            memory: None,
            constants: BTreeMap::new(),
            variables: BTreeMap::new(),
            endian,
        }
    }

    /// Sets the evaluator's memory to the given `MemoryRegion`.
    pub fn memory(mut self, memory: MemoryRegion<'memory>) -> Self {
        self.memory = Some(memory);
        self
    }

    /// Sets the evaluator's constant map to the given map.
    pub fn constants(mut self, constants: BTreeMap<Constant, A>) -> Self {
        self.constants = constants;
        self
    }

    /// Sets the evaluator's variable map to the given map.
    pub fn variables(mut self, variables: BTreeMap<Variable, A>) -> Self {
        self.variables = variables;
        self
    }
}

impl<'memory, A: RegisterValue, E: Endianness> Evaluator<'memory, A, E> {
    /// Evaluates a single expression.
    ///
    /// This may fail if the expression tries to dereference unavailable memory
    /// or uses undefined constants or variables.
    pub fn evaluate(&self, expr: &Expr<A>) -> Result<A, EvaluationError> {
        match expr {
            Expr::Value(x) => Ok(*x),
            Expr::Const(c) => {
                self.constants.get(&c).copied().ok_or_else(|| {
                    EvaluationError(EvaluationErrorInner::UndefinedConstant(c.clone()))
                })
            }
            Expr::Var(v) => {
                self.variables.get(&v).copied().ok_or_else(|| {
                    EvaluationError(EvaluationErrorInner::UndefinedVariable(v.clone()))
                })
            }
            Expr::Op(e1, e2, op) => {
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

            Expr::Deref(address) => {
                let address = self.evaluate(&*address)?;
                let memory = self
                    .memory
                    .as_ref()
                    .ok_or(EvaluationError(EvaluationErrorInner::MemoryUnavailable))?;
                memory.get(address, self.endian).ok_or_else(|| {
                    EvaluationError(EvaluationErrorInner::IllegalMemoryAccess {
                        address: address.try_into().ok(),
                        bytes: A::WIDTH as usize,
                        address_range: memory.base_addr..memory.base_addr + memory.len() as u64,
                    })
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
    pub fn assign(&mut self, Assignment(v, e): &Assignment<A>) -> Result<bool, EvaluationError> {
        let value = self.evaluate(e)?;
        Ok(self.variables.insert(v.clone(), value).is_some())
    }

    /// Processes a string of assignments, modifying its [`variables`](Self::variables)
    /// field accordingly.
    ///
    /// This may fail if parsing goes wrong or a parsed assignment cannot be handled,
    /// cf. [`assign`](Self::assign). It returns the set of variables that were assigned
    /// a value by some assignment, even if the variable's value did not change.
    ///
    /// # Example
    /// ```
    /// use std::collections::{BTreeMap, BTreeSet};
    /// use symbolic_unwind::evaluator::{Evaluator, Variable};
    /// use symbolic_unwind::BigEndian;
    /// let input = "$foo $bar 5 + = $bar 17 =";
    /// let mut variables = BTreeMap::new();
    /// let foo = "$foo".parse::<Variable>().unwrap();
    /// let bar = "$bar".parse::<Variable>().unwrap();
    /// variables.insert(bar.clone(), 17u8);
    /// let mut evaluator = Evaluator::new(BigEndian).variables(variables);
    ///
    /// let changed_variables = evaluator.process(input).unwrap();
    ///
    /// assert_eq!(changed_variables, vec![foo, bar].into_iter().collect());
    /// ```
    pub fn process(&mut self, input: &str) -> Result<BTreeSet<Variable>, ExpressionError> {
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
#[non_exhaustive]
enum EvaluationErrorInner {
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
        address: Option<usize>,
        /// The range of available addresses.
        address_range: std::ops::Range<u64>,
    },
}

impl fmt::Display for EvaluationErrorInner {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
           Self::UndefinedConstant(c) => write!(f, "Constant {} is not defined", c),
           Self::UndefinedVariable(v) => write!(f, "Variable {} is not defined", v),
           Self::MemoryUnavailable => write!(f, "The evaluator does not have access to memory"),
           Self::IllegalMemoryAccess {
               bytes, address: Some(address), address_range
           } => write!(f, "Tried to read {} bytes at memory address {}. The available address range is [{}, {})", bytes, address, address_range.start, address_range.end),
        Self::IllegalMemoryAccess {
            bytes, address: None, ..
        } => write!(f, "Tried to read {} bytes at address that exceeds the maximum usize value", bytes),
        }
    }
}

/// An error encountered while evaluating an expression.
#[derive(Debug)]
pub struct EvaluationError(EvaluationErrorInner);

impl fmt::Display for EvaluationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Error for EvaluationError {}

/// An error encountered while parsing or evaluating an expression.
#[derive(Debug)]
enum ExpressionErrorInner {
    /// An error was encountered while parsing an expression.
    Parsing(ParseExprError),

    /// An error was encountered while evaluating an expression.
    Evaluation(EvaluationError),
}

impl From<ParseExprError> for ExpressionError {
    fn from(other: ParseExprError) -> Self {
        Self(ExpressionErrorInner::Parsing(other))
    }
}

impl From<EvaluationError> for ExpressionError {
    fn from(other: EvaluationError) -> Self {
        Self(ExpressionErrorInner::Evaluation(other))
    }
}

impl fmt::Display for ExpressionErrorInner {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Parsing(e) => write!(f, "Error while parsing: {}", e),
            Self::Evaluation(e) => write!(f, "Error while evaluating: {}", e),
        }
    }
}

/// An error encountered while parsing or evaluating an expression.
#[derive(Debug)]
pub struct ExpressionError(ExpressionErrorInner);

impl fmt::Display for ExpressionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Error for ExpressionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self.0 {
            ExpressionErrorInner::Parsing(ref e) => Some(e),
            ExpressionErrorInner::Evaluation(ref e) => Some(e),
        }
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
    type Err = ParseExprError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        parsing::variable_complete(input)
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
    type Err = ParseExprError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        parsing::constant_complete(input)
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

impl<T: RegisterValue> FromStr for Expr<T> {
    type Err = ParseExprError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        parsing::expr_complete(input)
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

impl<T: RegisterValue> FromStr for Assignment<T> {
    type Err = ParseExprError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        parsing::assignment_complete(input)
    }
}

/// A variable or constant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Identifier {
    /// A variable.
    Var(Variable),

    /// A constant.
    Const(Constant),
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Var(v) => v.fmt(f),
            Self::Const(c) => c.fmt(f),
        }
    }
}

/// A `STACK CFI` rule `reg: e`, where `reg` is an identifier and `e` is an expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule<A>(Identifier, Expr<A>);

impl<T: fmt::Display> fmt::Display for Rule<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.0, self.1)
    }
}

/// These tests are inspired by the Breakpad PostfixEvaluator unit tests:
/// [https://github.com/google/breakpad/blob/main/src/processor/postfix_evaluator_unittest.cc]
#[cfg(test)]
mod test {
    use super::*;
    use crate::base::BigEndian;

    #[test]
    fn test_assignment() {
        let input = "$rAdd3 2 2 + =$rMul2 9 6 * =";

        let mut eval = Evaluator::<u64, _>::new(BigEndian);
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

        let memory = MemoryRegion {
            base_addr: 9,
            contents: &[0, 0, 0, 0, 0, 0, 0, 10],
        };

        let mut eval = Evaluator::<u64, _>::new(BigEndian).memory(memory);

        let r_deref: Variable = "$rDeref".parse().unwrap();

        let changed_vars = eval.process(input).unwrap();

        assert_eq!(changed_vars, vec![r_deref.clone()].into_iter().collect());

        assert_eq!(eval.variables[&r_deref], 10);
    }
}
