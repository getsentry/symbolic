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
use std::collections::BTreeMap;
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

    /// A map of CFI rules, i.e., rules for computing the value of a register in the
    /// caller's stack frame.
    cfi_rules: BTreeMap<Identifier, Expr<A>>,

    /// The rule for the CFA pseudoregister. It has its own field because it needs to
    /// be evaluated before any other rules.
    cfa_rule: Option<Expr<A>>,

    /// A cache for saving already computed values of caller registers.
    register_cache: BTreeMap<Identifier, A>,
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
            register_cache: BTreeMap::new(),
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

    /// Evaluates all cfi rules that have been added with
    /// [`add_cfi_rule`](Self::add_cfi_rule) and returns the results in a map.
    ///
    /// Results are cached. This may fail if a rule cannot be evaluated.
    pub fn evaluate_cfi_rules(&mut self) -> Result<BTreeMap<Identifier, A>, EvaluationError> {
        if let Some(ref expr) = self.cfa_rule {
            let cfa_val = self.evaluate(expr)?;
            self.constants.insert(Constant::cfa(), cfa_val);
            self.register_cache
                .insert(Identifier::Const(Constant::cfa()), cfa_val);
        }

        let cfi_rules = std::mem::take(&mut self.cfi_rules);
        for (ident, expr) in cfi_rules.iter() {
            if !self.register_cache.contains_key(ident) {
                self.register_cache
                    .insert(ident.clone(), self.evaluate(expr)?);
            }
        }
        self.cfi_rules = cfi_rules;
        Ok(self.register_cache.clone())
    }

    /// Reads a string of CFI rules and adds them to the evaluator.
    pub fn add_cfi_rules_string(&mut self, rules_string: &str) -> Result<(), ParseExprError> {
        for Rule(lhs, rhs) in parsing::rules_complete(rules_string.trim())?.into_iter() {
            self.add_cfi_rule(lhs, rhs);
        }

        Ok(())
    }

    /// Reads a string of cfi rules, adds them to the evaluator, and evaluates them.
    pub fn evaluate_cfi_string(
        &mut self,
        rules_string: &str,
    ) -> Result<BTreeMap<Identifier, A>, ExpressionError> {
        self.add_cfi_rules_string(rules_string)?;
        Ok(self.evaluate_cfi_rules()?)
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

/// A half-open range `[start, end)` with some `contents` attached.
#[derive(Clone, Debug)]
pub struct Range<A, E> {
    /// The left endpoint of the range.
    pub start: A,

    /// The right endpoint of teh range.
    pub end: A,

    /// The contents associated with the range.
    pub contents: E,
}

impl<A: Eq, E> PartialEq for Range<A, E> {
    fn eq(&self, other: &Self) -> bool {
        self.end == other.end && self.start == other.start
    }
}

impl<A: Eq, E> Eq for Range<A, E> {}

impl<A: Ord, E> core::cmp::Ord for Range<A, E> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.end.cmp(&other.end).then(self.start.cmp(&other.start))
    }
}

impl<A: Ord, E> PartialOrd for Range<A, E> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// A structure containing a set of disjoint half-open ranges `[a, b)` with attached contents.
#[derive(Clone, Debug)]
pub struct RangeMap<A, E> {
    inner: Vec<Range<A, E>>,
}

impl<A: Ord + Copy, E> RangeMap<A, E> {
    /// Insert a range into the map.
    ///
    /// The range must be disjoint from all ranges that are already present.
    /// Returns true if the insertion was successful.
    pub fn insert(&mut self, start: A, end: A, contents: E) -> bool {
        let range = Range {
            start,
            end,
            contents,
        };
        let index = match self.inner.binary_search(&range) {
            Ok(_) => return false,
            Err(index) => index,
        };

        if index > 0 {
            let before = &self.inner[index - 1];
            if before.end > start {
                return false;
            }
        }

        match self.inner.get(index) {
            Some(after) if after.start < end => return false,
            _ => self.inner.insert(index, range),
        }

        true
    }

    /// Retrieves the contents associated with the given address.
    pub fn get(&self, address: A) -> Option<&E> {
        self.get_range(address).map(|r| &r.contents)
    }

    /// Retrieves the range covering the given address.
    pub fn get_range(&self, address: A) -> Option<&Range<A, E>> {
        let range = match self.inner.binary_search_by_key(&address, |range| range.end) {
            // This means inner(index).end == address => address might be covered by the next one
            Ok(index) => self.inner.get(index + 1)?,
            // This means that inner(index).end > address => this could be the one
            Err(index) => self.inner.get(index)?,
        };

        (range.start <= address).then(|| range)
    }

    /// Retrieves the range covering the given address, allowing mutation.
    pub fn get_range_mut(&mut self, address: A) -> Option<&mut Range<A, E>> {
        let range = match self.inner.binary_search_by_key(&address, |range| range.end) {
            // This means inner(index).end == address => address might be covered by the next one
            Ok(index) => self.inner.get_mut(index + 1)?,
            // This means that inner(index).end > address => this could be the one
            Err(index) => self.inner.get_mut(index)?,
        };

        (range.start <= address).then(|| range)
    }

    /// Retrieves the range covering the given address, or else the last range before the
    /// address.
    ///
    /// # Example
    /// ```
    /// use symbolic_unwind::evaluator::{Range, RangeMap};
    /// let mut map = RangeMap::default();
    /// map.insert(0u8, 2, "First");
    /// map.insert(2, 4, "Second");
    /// map.insert(5, 7, "Third");
    ///
    /// // map now looks like this:
    /// // |0     1|2      3|   4   |5     6|7 …
    /// // |"First"|"Second"|<empty>|"Third"|
    ///
    /// let range = map.get_nearest_range(4).unwrap();
    /// assert_eq!(range.start, 2);
    /// assert_eq!(range.end, 4);
    /// assert_eq!(range.contents, "Second");
    pub fn get_nearest_range(&self, address: A) -> Option<&Range<A, E>> {
        match self
            .inner
            .binary_search_by_key(&address, |range| range.start)
        {
            Ok(index) => self.inner.get(index),
            Err(index) if index > 0 => self.inner.get(index - 1),
            _ => None,
        }
    }
}

impl<A, E> Default for RangeMap<A, E> {
    fn default() -> Self {
        Self { inner: Vec::new() }
    }
}

/// A structure containing
///  - "init rules" that each cover a half-open range of addresses and
///  - "delta rules" that each cover a single address.
///
/// It is generic, but the intended use case are Breakpad's
/// `STACK CFI INIT` and `STACK CFI` records.
#[derive(Clone, Debug)]
pub struct CfiRules<A: Ord, E> {
    inner: RangeMap<A, Vec<(A, E)>>,
}

impl<A: Ord + Copy, E: Clone> CfiRules<A, E> {
    /// Insert an "init rule", comprising a half-open range `[start, end)`
    /// and some attached `contents`, into the structure.
    ///
    /// The range must be disjoint from all ranges that are already present.
    /// Returns true if the insertion was successful.
    pub fn insert_init(&mut self, start: A, end: A, contents: E) -> bool {
        self.inner.insert(start, end, vec![(start, contents)])
    }

    /// Insert a "delta rule", comprising an `address` and some attached
    /// `contents`, into the structure.
    ///
    /// This fails if there is already a rule for the given address. Returns true if
    /// the insertion was successful.
    pub fn insert_delta(&mut self, address: A, contents: E) -> bool {
        if let Some(range) = self.inner.get_range_mut(address) {
            let rules = &mut range.contents;
            match rules.binary_search_by_key(&address, |p| p.0) {
                Ok(_) => false,
                Err(i) => {
                    rules.insert(i, (address, contents));
                    true
                }
            }
        } else {
            false
        }
    }

    /// Returns all rules applicable to the given address.
    ///
    /// If the given address is not covered by any init rule, this returns `None`.
    /// Otherwise it returns the covering init rule and all delta rules with addresses in
    /// `[init_rule.start, address]`.
    pub fn get_rules(&self, address: A) -> Option<Vec<&E>> {
        let range = self.inner.get_range(address)?;
        Some(
            range
                .contents
                .iter()
                .take_while(|(a, _)| a <= &address)
                .map(|(_, e)| e)
                .collect(),
        )
    }
}

impl<A: Ord, E> Default for CfiRules<A, E> {
    fn default() -> Self {
        Self {
            inner: RangeMap::default(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_init_rules() {
        let mut map = CfiRules::default();

        assert!(map.insert_init(50usize, 100, "First"));
        assert!(map.insert_init(130, 140, "Second"));
        assert!(map.insert_init(100, 130, "Third"));
        assert!(!map.insert_init(0, 60, "Uh oh"));

        assert!(map.insert_delta(51, "First delta"));
        assert!(map.insert_delta(52, "Second delta"));
        assert!(map.insert_delta(53, "Third delta"));
        assert!(!map.insert_delta(50, "Uh oh"));

        assert_eq!(
            map.get_rules(52).unwrap(),
            vec![&"First", &"First delta", &"Second delta"]
        );
        assert!(map.get_rules(145).is_none());
    }
}
