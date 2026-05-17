// ABOUTME: Integration tests exercising the spec's dual-mode scenarios.
// ABOUTME: Eager, lazy, explicit commands, delayed binding, limits, matrices.

use nat_calc::{Environment, EvalError, EvalResult, eval};

fn run(src: &str, env: &mut Environment) -> EvalResult {
    eval(src, env).unwrap_or_else(|e| panic!("`{src}` failed: {e}"))
}

fn s(src: &str) -> String {
    let mut env = Environment::new();
    run(src, &mut env).to_string()
}

// --- Eager (numeric) path ------------------------------------------------

#[test]
fn eager_arithmetic_and_precedence() {
    assert_eq!(s("2 + 3 * 4"), "14");
    assert_eq!(s("(2 + 3) * 4"), "20");
    assert_eq!(s("2 ^ 3 ^ 2"), "512"); // right-associative
    assert_eq!(s("-2 ^ 2"), "-4"); // unary minus looser than ^
    assert_eq!(s("10 / 4"), "2.5");
}

#[test]
fn division_by_zero_is_an_error() {
    let mut env = Environment::new();
    assert_eq!(eval("1 / 0", &mut env), Err(EvalError::DivisionByZero));
}

// --- Lazy (symbolic) path ------------------------------------------------

#[test]
fn unbound_variable_promotes_to_symbolic() {
    assert_eq!(s("x + x"), "(2 * x)"); // term grouping
    assert_eq!(s("x * 1"), "x"); // identity
    assert_eq!(s("x + 0"), "x"); // identity
    assert_eq!(s("x * 0"), "0"); // annihilation
    assert_eq!(s("x - x"), "0");
}

#[test]
fn constant_folding_in_symbolic_context() {
    // y is unbound so the whole thing is symbolic, but constants still fold.
    assert_eq!(s("1 + 2 + y"), "(y + 3)");
}

// --- Implicit switching + delayed binding (Sub-Task 4) -------------------

#[test]
fn assignment_stays_symbolic_then_resolves_eagerly() {
    let mut env = Environment::new();
    // a, b unbound -> x stored symbolically.
    assert_eq!(run("x = a + b", &mut env).to_string(), "(a + b)");
    // Bind the dependencies; querying x re-triggers eager reduction.
    run("a = 2", &mut env);
    run("b = 3", &mut env);
    assert_eq!(run("x", &mut env), EvalResult::Numeric(5.into()));
}

// --- Explicit commands (Sub-Task 2) -------------------------------------

#[test]
fn expand_forces_lazy_even_when_bound() {
    let mut env = Environment::new();
    run("x = 5", &mut env);
    // expand ignores the binding and works structurally.
    assert_eq!(
        run("expand((x + 1) ^ 2)", &mut env).to_string(),
        "(((x ^ 2) + (2 * x)) + 1)"
    );
}

#[test]
fn simplify_command() {
    assert_eq!(s("simplify(x + x + x)"), "(3 * x)");
    assert_eq!(s("simplify(2 * x + 3 * x)"), "(5 * x)");
}

// --- Symbolic differentiation -------------------------------------------

#[test]
fn derivative_core_rules() {
    assert_eq!(s("derive(x, x ^ 2)"), "(2 * x)");
    assert_eq!(s("derive(x, x ^ 3)"), "(3 * (x ^ 2))");
    assert_eq!(s("derive(x, 5)"), "0");
    assert_eq!(s("derive(x, x * x)"), "(2 * x)");
}

#[test]
fn derivative_transcendental() {
    assert_eq!(s("derive(x, sin(x))"), "cos(x)");
    assert_eq!(s("derive(x, cos(x))"), "-(sin(x))");
    assert_eq!(s("derive(x, exp(x))"), "exp(x)");
    assert_eq!(s("derive(x, ln(x))"), "(1 / x)");
}

// --- Matrices (basic arithmetic) ----------------------------------------

#[test]
fn matrix_arithmetic() {
    assert_eq!(s("[1, 2; 3, 4] + [1, 1; 1, 1]"), "[2, 3; 4, 5]");
    assert_eq!(s("2 * [1, 2; 3, 4]"), "[2, 4; 6, 8]");
    // Matrix multiplication against the identity.
    assert_eq!(s("[1, 2; 3, 4] * [1, 0; 0, 1]"), "[1, 2; 3, 4]");
}

#[test]
fn matrix_dimension_mismatch_errors() {
    let mut env = Environment::new();
    assert!(matches!(
        eval("[1, 2] + [1, 2; 3, 4]", &mut env),
        Err(EvalError::TypeMismatch(_))
    ));
}

// --- Scalability guards (Section 3) -------------------------------------

#[test]
fn expansion_blowup_is_bounded() {
    let mut env = Environment::new();
    // (x+y)^14 = 16384 product terms > MAX_NODES (10_000).
    assert_eq!(
        eval("expand((x + y) ^ 14)", &mut env),
        Err(EvalError::ExpressionTooLarge)
    );
}

// --- Lambda calculus mode -----------------------------------------------

#[test]
fn lambda_basic_application() {
    assert_eq!(s("(\\x. x)(5)"), "5"); // identity
    assert_eq!(s("(\\x. x + 1)(4)"), "5"); // arithmetic in body
    assert_eq!(s("(\\x. \\y. x)(7)(9)"), "7"); // K combinator
}

#[test]
fn lambda_multi_param_sugar() {
    assert_eq!(s("(\\x y. x)(1, 2)"), "1");
    assert_eq!(s("(\\x y. x)(1)(2)"), "1"); // curried form is equivalent
}

#[test]
fn bare_lambda_is_lambda_mode() {
    let mut env = Environment::new();
    assert!(matches!(
        run("\\x. x", &mut env),
        EvalResult::Lambda(_)
    ));
    assert_eq!(s("\\x. x"), "(\\x. x)");
}

#[test]
fn user_defined_functions_via_binding() {
    // Sub-Task 4: a binding holding a lambda IS a user-defined function.
    let mut env = Environment::new();
    run("inc = \\x. x + 1", &mut env);
    assert_eq!(run("inc(9)", &mut env), EvalResult::Numeric(10.into()));
    run("compose = \\f. \\g. \\x. f(g(x))", &mut env);
    run("dbl = \\x. x * 2", &mut env);
    assert_eq!(
        run("compose(inc)(dbl)(10)", &mut env),
        EvalResult::Numeric(21.into())
    );
}

#[test]
fn lambda_closes_over_environment() {
    let mut env = Environment::new();
    run("a = 10", &mut env);
    run("addA = \\x. x + a", &mut env);
    assert_eq!(run("addA(5)", &mut env), EvalResult::Numeric(15.into()));
    // Delayed binding: rebinding `a` changes the captured value.
    run("a = 100", &mut env);
    assert_eq!(run("addA(5)", &mut env), EvalResult::Numeric(105.into()));
}

#[test]
fn higher_order_twice() {
    let mut env = Environment::new();
    run("twice = \\f. \\x. f(f(x))", &mut env);
    assert_eq!(
        run("twice(\\y. y + 1)(10)", &mut env),
        EvalResult::Numeric(12.into())
    );
}

#[test]
fn capture_avoiding_substitution() {
    // (\x. \y. x) y  must alpha-rename the inner binder, not capture `y`.
    assert_eq!(s("(\\x. \\y. x)(y)"), "(\\y'. y)");
}

#[test]
fn divergent_term_hits_step_cap() {
    // Omega combinator: reduces forever -> bounded by MAX_BETA_STEPS.
    let mut env = Environment::new();
    assert_eq!(
        eval("(\\x. x(x))(\\x. x(x))", &mut env),
        Err(EvalError::RewriteLimitExceeded)
    );
}

// --- Regression: self/mutually referential bindings (no stack overflow) --

#[test]
fn self_referential_binding_is_symbolic_not_a_crash() {
    let mut env = Environment::new();
    // `y + y` folds to `2*y` while y is unbound, so y is stored as `2*y`.
    // Looking up y must not recurse forever.
    run("y = y + y", &mut env);
    assert_eq!(run("y", &mut env).to_string(), "(2 * y)");
}

#[test]
fn pure_self_binding_resolves_to_free_symbol() {
    let mut env = Environment::new();
    run("z = z", &mut env);
    assert_eq!(run("z", &mut env).to_string(), "z");
}

#[test]
fn mutually_referential_bindings_do_not_recurse() {
    let mut env = Environment::new();
    run("a = b", &mut env);
    run("b = a", &mut env);
    // Either order of resolution terminates with a free symbol.
    assert!(matches!(
        eval("a", &mut env),
        Ok(EvalResult::Symbolic(_))
    ));
}
