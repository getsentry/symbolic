use std::ops::Range;

use rslint_parser::{ast, SyntaxKind, SyntaxNode, SyntaxNodeExt, SyntaxToken, TextRange};

use crate::scope_name::{NameComponent, ScopeName};

pub fn parse_with_rslint(src: &str) -> Vec<(Range<u32>, Option<ScopeName>)> {
    let parse =
        //rslint_parser::parse_with_syntax(src, 0, rslint_parser::FileKind::TypeScript.into());
        rslint_parser::parse_text(src, 0);

    let syntax = parse.syntax();
    // dbg!(&syntax);

    let mut ranges = vec![];

    for node in syntax.descendants() {
        if let Some(fn_decl) = node.try_to::<ast::FnDecl>() {
            ranges.push(node_range_and_name(&node, fn_decl.name()))
        } else if let Some(fn_expr) = node.try_to::<ast::FnExpr>() {
            ranges.push(node_range_and_name(&node, fn_expr.name()))
        } else if let Some(class_decl) = node.try_to::<ast::ClassDecl>() {
            // NOTE: instead of going for the `constructor`, we will cover the
            // whole class body, as class property definitions are executed as
            // part of the constructor.

            ranges.push(node_range_and_name(&node, class_decl.name()));
        } else if let Some(class_expr) = node.try_to::<ast::ClassExpr>() {
            // Same here, see NOTE above.

            ranges.push(node_range_and_name(&node, class_expr.name()));
        } else if node.is::<ast::ArrowExpr>() || node.is::<ast::Method>() {
            ranges.push(node_range_and_name(&node, None));
        }
    }

    ranges
}

fn node_range_and_name(
    node: &SyntaxNode,
    name: Option<ast::Name>,
) -> (Range<u32>, Option<ScopeName>) {
    let mut name = if let Some(name_token) = name.and_then(|n| n.ident_token()) {
        let mut name = ScopeName::new();
        name.components.push_back(NameComponent::token(name_token));
        Some(name)
    } else {
        find_name_from_ctx(node)
    };

    if node.is::<ast::ClassDecl>() || node.is::<ast::ClassExpr>() {
        if let Some(name) = &mut name {
            name.components.push_front(NameComponent::interp("new "));
        }
    }

    (convert_text_range(node.text_range()), name)
}

/// Converts a [`TextRange`] into a standard [`Range`].
pub(crate) fn convert_text_range(range: TextRange) -> Range<u32> {
    range.start().into()..range.end().into()
}

/// Gets the identifier token of the given [`ast::PropName`] if possible.
fn prop_name_token(prop: Option<ast::PropName>) -> Option<SyntaxToken> {
    match prop {
        Some(ast::PropName::Ident(t)) => t.ident_token(),
        _ => None,
    }
}

/// Tries to infer a name for the given [`SyntaxNode`] by walking up the chain of ancestors.
fn find_name_from_ctx(node: &SyntaxNode) -> Option<ScopeName> {
    let mut scope_name = ScopeName::new();

    fn push_sep(name: &mut ScopeName) {
        if !name.components.is_empty() {
            name.components.push_front(NameComponent::interp("."));
        }
    }

    if let Some(method) = node.try_to::<ast::Method>() {
        // `ast::Method` has no convenient getter for `PrivateName` :-(
        if let Some(name_token) = node
            .child_with_ast::<ast::PrivateName>()
            .and_then(|p| p.name())
            .and_then(|n| n.ident_token())
        {
            scope_name
                .components
                .push_front(NameComponent::token(name_token));

            scope_name.components.push_front(NameComponent::interp("#"));
        } else if let Some(name_token) = prop_name_token(method.name()) {
            scope_name
                .components
                .push_front(NameComponent::token(name_token));
        }
    }

    // the node itself is the first "ancestor"
    for parent in node.ancestors().skip(1) {
        // break on syntax that itself starts a scope
        match parent.kind() {
            SyntaxKind::FN_DECL
            | SyntaxKind::FN_EXPR
            | SyntaxKind::ARROW_EXPR
            | SyntaxKind::METHOD
            | SyntaxKind::CONSTRUCTOR => return None,
            _ => {}
        }
        if let Some(prop) = parent.try_to::<ast::LiteralProp>() {
            if let Some(name_token) = prop_name_token(prop.key()) {
                push_sep(&mut scope_name);
                scope_name
                    .components
                    .push_front(NameComponent::token(name_token));
            }
        } else if let Some(class_decl) = parent.try_to::<ast::ClassDecl>() {
            if let Some(name_token) = class_decl.name().and_then(|n| n.ident_token()) {
                push_sep(&mut scope_name);
                scope_name
                    .components
                    .push_front(NameComponent::token(name_token));
                return Some(scope_name);
            }
        } else if let Some(assign_expr) = parent.try_to::<ast::AssignExpr>() {
            if let Some(ast::PatternOrExpr::Expr(expr)) = assign_expr.lhs() {
                if let Some(mut expr_name) = find_name_of_expr(expr) {
                    push_sep(&mut scope_name);

                    expr_name.components.append(&mut scope_name.components);
                    scope_name.components = expr_name.components;

                    return Some(scope_name);
                }
            }
        } else if let Some(decl) = parent.try_to::<ast::Declarator>() {
            if let Some(ast::Pattern::SinglePattern(sp)) = decl.pattern() {
                if let Some(name_token) = sp.name().and_then(|n| n.ident_token()) {
                    push_sep(&mut scope_name);
                    scope_name
                        .components
                        .push_front(NameComponent::token(name_token));
                    return Some(scope_name);
                }
            }
        }
        // TODO: getter, setter?
    }
    None
}

/// Returns a [`ScopeName`] corresponding to the given [`ast::Expr`].
///
/// This is only possible if the expression is an identifier or a "dot expression".
fn find_name_of_expr(mut expr: ast::Expr) -> Option<ScopeName> {
    let mut scope_name = ScopeName::new();
    loop {
        match expr {
            ast::Expr::NameRef(name) => {
                if let Some(name_token) = name.ident_token() {
                    scope_name
                        .components
                        .push_front(NameComponent::token(name_token));
                }
                return Some(scope_name);
            }

            ast::Expr::DotExpr(dot_expr) => {
                if let Some(name_token) = dot_expr.prop().and_then(|n| n.ident_token()) {
                    scope_name
                        .components
                        .push_front(NameComponent::token(name_token));
                    scope_name.components.push_front(NameComponent::interp("."));
                }

                match dot_expr.object() {
                    Some(obj) => expr = obj,
                    None => return None,
                }
            }

            ast::Expr::ThisExpr(_) => {
                scope_name
                    .components
                    .push_front(NameComponent::interp("this"));
                return Some(scope_name);
            }

            _ => return None,
        }
    }
}
