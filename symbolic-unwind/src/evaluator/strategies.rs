//! Strategies for property-based testing.

use super::*;
use proptest::prelude::*;

fn arb_variable() -> impl Strategy<Value = Variable> {
    r"\$[a-zA-Z0-9]+".prop_map(Variable)
}

pub fn arb_constant() -> impl Strategy<Value = Constant> {
    r"\.?[a-zA-Z][a-zA-Z0-9]*".prop_map(Constant)
}

fn arb_ident() -> impl Strategy<Value = Identifier> {
    prop_oneof![
        arb_variable().prop_map(Identifier::Var),
        arb_constant().prop_map(Identifier::Const),
    ]
}

fn arb_binop() -> impl Strategy<Value = BinOp> {
    prop_oneof![
        Just(BinOp::Add),
        Just(BinOp::Sub),
        Just(BinOp::Mul),
        Just(BinOp::Div),
        Just(BinOp::Mod),
        Just(BinOp::Align),
    ]
}

fn arb_expr<A: Arbitrary + 'static>() -> impl Strategy<Value = Expr<A>> {
    let leaf = prop_oneof![
        arb_variable().prop_map(Expr::Var),
        arb_constant().prop_map(Expr::Const),
        any::<A>().prop_map(Expr::Value),
    ];

    leaf.prop_recursive(5, 10, 1, |inner| {
        prop_oneof![
            (inner.clone(), inner.clone(), arb_binop()).prop_map(|(l, r, op)| Expr::Op(
                Box::new(l),
                Box::new(r),
                op
            )),
            inner.prop_map(|x| Expr::Deref(Box::new(x))),
        ]
    })
}

pub fn arb_rule<A: Arbitrary + 'static>() -> impl Strategy<Value = Rule<A>> {
    (arb_ident(), arb_expr()).prop_map(|(l, r)| Rule(l, r))
}
