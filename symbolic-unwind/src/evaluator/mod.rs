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
//! <constant>   ::=  \.?[a-zA-Z][a-zA-Z0-9_]*
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
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::ops::Range;
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
    variables: BTreeMap<Variable, A>,

    /// The endianness the evaluator uses to read data from memory.
    endian: E,

    /// A map of CFI rules, i.e., rules for computing the value of a register in the
    /// caller's stack frame.
    cfi_rules: BTreeMap<Identifier, Expr<A>>,

    /// The rule for the CFA pseudoregister. It has its own field because it needs to
    /// be evaluated before any other rules.
    cfa_rule: Option<Expr<A>>,
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
            cfi_rules: BTreeMap::new(),
            cfa_rule: None,
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

    /// Adds a rule for computing a register's value in the caller's frame
    /// to the evaluator.
    pub fn add_cfi_rule(&mut self, ident: Identifier, expr: Expr<A>) {
        match ident {
            Identifier::Const(c) if c.is_cfa() => self.cfa_rule = Some(expr),
            _ => {
                self.cfi_rules.insert(ident, expr);
            }
        }
    }
}

impl<'memory, A: RegisterValue, E: Endianness> Evaluator<'memory, A, E> {
    /// Evaluates a single expression.
    ///
    /// This may fail if the expression tries to dereference unavailable memory
    /// or uses undefined constants or variables.
    pub fn evaluate(&self, expr: &Expr<A>) -> Result<A, EvaluationError<A>> {
        match expr {
            Expr::Value(x) => Ok(*x),
            Expr::Const(c) => {
                self.constants.get(c).copied().ok_or_else(|| {
                    EvaluationError(EvaluationErrorInner::UndefinedConstant(c.clone()))
                })
            }
            Expr::Var(v) => {
                self.variables.get(v).copied().ok_or_else(|| {
                    EvaluationError(EvaluationErrorInner::UndefinedVariable(v.clone()))
                })
            }
            Expr::Op(e1, e2, op) => {
                let e1 = self.evaluate(&*e1)?;
                let e2 = self.evaluate(&*e2)?;
                let result = match op {
                    BinOp::Add => e1.checked_add(&e2),
                    BinOp::Sub => e1.checked_sub(&e2),
                    BinOp::Mul => e1.checked_mul(&e2),
                    BinOp::Div => e1.checked_div(&e2),
                    BinOp::Mod => e1.checked_rem(&e2),
                    BinOp::Align => e1.checked_div(&e2).and_then(|n| n.checked_mul(&e2)),
                };

                result.ok_or_else(|| {
                    EvaluationError(EvaluationErrorInner::IllegalOperation {
                        left: e1,
                        right: e2,
                        op: *op,
                    })
                })
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

    /// Evaluates all cfi rules that have been added with
    /// [`add_cfi_rule`](Self::add_cfi_rule) and returns the results in a map.
    ///
    /// Results are cached. This may fail if a rule cannot be evaluated.
    pub fn evaluate_cfi_rules(&mut self) -> Result<BTreeMap<Identifier, A>, EvaluationError<A>> {
        let mut computed_registers = BTreeMap::new();
        if let Some(ref expr) = self.cfa_rule {
            let cfa_val = self.evaluate(expr)?;
            self.constants.insert(Constant::cfa(), cfa_val);
            computed_registers.insert(Identifier::Const(Constant::cfa()), cfa_val);
        }

        let cfi_rules = std::mem::take(&mut self.cfi_rules);
        for (ident, expr) in cfi_rules.iter() {
            if !computed_registers.contains_key(ident) {
                computed_registers.insert(ident.clone(), self.evaluate(expr)?);
            }
        }
        self.cfi_rules = cfi_rules;
        Ok(computed_registers)
    }

    /// Reads a string of CFI rules and adds them to the evaluator.
    pub fn add_cfi_rules_string(&mut self, rules_string: &str) -> Result<(), ParseExprError> {
        for Rule(lhs, rhs) in parsing::rules_complete(rules_string.trim())?.into_iter() {
            self.add_cfi_rule(lhs, rhs);
        }

        Ok(())
    }
}

/// An error encountered while evaluating an expression.
#[derive(Debug)]
#[non_exhaustive]
enum EvaluationErrorInner<A> {
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
        address_range: Range<u64>,
    },

    /// An illegal arithmetical operation was attempted.
    IllegalOperation {
        /// The left operand.
        left: A,
        /// The right operand.
        right: A,
        /// The binary operator.
        op: BinOp,
    },
}

impl<A: fmt::Display> fmt::Display for EvaluationErrorInner<A> {
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
            Self::IllegalOperation {
                left, right, op } => write!(f, "Illegal operation {} {} {}", left, op, right),
        }
    }
}

/// An error encountered while evaluating an expression.
#[derive(Debug)]
pub struct EvaluationError<A>(EvaluationErrorInner<A>);

impl<A: fmt::Display> fmt::Display for EvaluationError<A> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<A: fmt::Debug + fmt::Display> Error for EvaluationError<A> {}

/// An error encountered while parsing or evaluating an expression.
#[derive(Debug)]
enum ExpressionErrorInner<A> {
    /// An error was encountered while parsing an expression.
    Parsing(ParseExprError),

    /// An error was encountered while evaluating an expression.
    Evaluation(EvaluationError<A>),
}

impl<A: fmt::Display> fmt::Display for ExpressionErrorInner<A> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Parsing(e) => write!(f, "Error while parsing: {}", e),
            Self::Evaluation(e) => write!(f, "Error while evaluating: {}", e),
        }
    }
}

/// An error encountered while parsing or evaluating an expression.
#[derive(Debug)]
pub struct ExpressionError<A>(ExpressionErrorInner<A>);

impl<A> From<ParseExprError> for ExpressionError<A> {
    fn from(other: ParseExprError) -> Self {
        Self(ExpressionErrorInner::Parsing(other))
    }
}

impl<A> From<EvaluationError<A>> for ExpressionError<A> {
    fn from(other: EvaluationError<A>) -> Self {
        Self(ExpressionErrorInner::Evaluation(other))
    }
}

impl<A: fmt::Display> fmt::Display for ExpressionError<A> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<A: fmt::Debug + fmt::Display + 'static> Error for ExpressionError<A> {
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

impl Constant {
    /// Returns true if this is the CFA (Canonical Frame Address) pseudoregister.
    pub fn is_cfa(&self) -> bool {
        self.0 == ".cfa"
    }

    /// Returns the CFA (Canonical Frame Address) pseudoregister.
    pub fn cfa() -> Self {
        Self(".cfa".to_string())
    }

    /// Returns true if this is the RA (Return Address) pseudoregister.
    pub fn is_ra(&self) -> bool {
        self.0 == ".ra"
    }

    /// Returns the RA (Return Address) pseudoregister.
    pub fn ra() -> Self {
        Self(".ra".to_string())
    }
}

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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::base::LittleEndian;

    #[test]
    fn division_by_zero() {
        let expr = Expr::Op(
            Box::new(Expr::Value(23u32)),
            Box::new(Expr::Value(0)),
            BinOp::Div,
        );
        let eval = Evaluator::new(LittleEndian);
        assert!(eval.evaluate(&expr).is_err());
    }
}
