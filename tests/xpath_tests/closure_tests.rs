//! Regression tests for inline function closure capture.
//!
//! Inline functions (`function($x) { ... }`) are closures: they capture the
//! variable bindings in scope at the point of definition. Evaluating the
//! body must use those captured bindings — not the caller's variables — for
//! nested calls, functions returned from functions, functions passed as
//! values, and repeated evaluation.

use skyscraper::{
    html,
    xpath::{
        self,
        grammar::data_model::{AnyAtomicType, XpathItem},
    },
};

const LINKS_HTML: &str = r##"
<html>
    <body>
        <a href="/about">About</a>
        <a href="/contact">Contact</a>
    </body>
</html>"##;

fn int_item(n: i64) -> XpathItem<'static> {
    XpathItem::AnyAtomicType(AnyAtomicType::Integer(n))
}

fn str_item(s: &str) -> XpathItem<'static> {
    XpathItem::AnyAtomicType(AnyAtomicType::String(s.to_string()))
}

// ============================================================
// Closures: capturing definition-time variables
// ============================================================

/// Page-rule scenario: a `let`-bound site prefix is captured by an inline
/// function that builds absolute links from `href` attributes.
#[test]
fn closure_captures_let_bound_prefix_for_link_building() {
    let document = html::parse(LINKS_HTML).unwrap();
    let xpath = xpath::parse(
        r#"let $prefix := "https://site.example"
           return
             let $make-link := function($href) { $prefix || $href }
             return
               for $a in //a/@href return $make-link($a)"#,
    )
    .unwrap();

    let items = xpath.apply(&document).unwrap();
    assert_eq!(items.len(), 2, "should return 2 items: {items:?}");
    assert_eq!(items[0], str_item("https://site.example/about"));
    assert_eq!(items[1], str_item("https://site.example/contact"));
}

/// A variable shadowed after the function definition must not affect the
/// captured binding: the closure sees the value from its definition scope.
#[test]
fn closure_uses_definition_value_when_variable_is_shadowed() {
    let document = html::parse(LINKS_HTML).unwrap();
    let xpath = xpath::parse(
        r#"let $p := "definition"
           return
             let $f := function() { $p }
             return
               let $p := "caller"
               return $f()"#,
    )
    .unwrap();

    let items = xpath.apply(&document).unwrap();
    assert_eq!(items.len(), 1, "should return 1 item: {items:?}");
    assert_eq!(items[0], str_item("definition"));
}

/// The innermost binding at the definition site wins when several scopes
/// bind the same variable name.
#[test]
fn closure_captures_innermost_shadowing_binding() {
    let document = html::parse(LINKS_HTML).unwrap();
    let xpath = xpath::parse(
        r#"let $x := "outer"
           return
             let $x := "inner"
             return
               let $f := function() { $x }
               return $f()"#,
    )
    .unwrap();

    let items = xpath.apply(&document).unwrap();
    assert_eq!(items.len(), 1, "should return 1 item: {items:?}");
    assert_eq!(items[0], str_item("inner"));
}

/// Each iteration of a `for` loop produces a closure capturing that
/// iteration's value of the loop variable.
#[test]
fn closure_captures_for_loop_variable_per_iteration() {
    let document = html::parse(LINKS_HTML).unwrap();
    let xpath = xpath::parse(
        "let $fs := (for $x in (1, 2, 3) return function() { $x })
         return ($fs[1](), $fs[2](), $fs[3]())",
    )
    .unwrap();

    let items = xpath.apply(&document).unwrap();
    assert_eq!(items.len(), 3, "should return 3 items: {items:?}");
    assert_eq!(items[0], int_item(1));
    assert_eq!(items[1], int_item(2));
    assert_eq!(items[2], int_item(3));
}

/// Closures can capture node sequences, not just atomic values.
#[test]
fn closure_captures_node_sequence_variable() {
    let document = html::parse(LINKS_HTML).unwrap();
    let xpath = xpath::parse(
        "let $links := //a
         return
           let $f := function() { $links/@href }
           return $f()",
    )
    .unwrap();

    let items = xpath.apply(&document).unwrap();
    let hrefs: Vec<&str> = items
        .iter()
        .map(|item| {
            item.extract_as_node()
                .extract_as_attribute_node()
                .value
                .as_str()
        })
        .collect();
    assert_eq!(hrefs, vec!["/about", "/contact"]);
}

// ============================================================
// Nested calls and functions as values
// ============================================================

/// A function returned from a function captures the outer call's parameter.
#[test]
fn nested_closure_captures_outer_parameter() {
    let document = html::parse(LINKS_HTML).unwrap();
    let xpath =
        xpath::parse("let $adder := function($x) { function($y) { $x + $y } } return $adder(10)(32)")
            .unwrap();

    let items = xpath.apply(&document).unwrap();
    assert_eq!(items.len(), 1, "should return 1 item: {items:?}");
    assert_eq!(items[0], int_item(42));
}

/// A closure passed as an argument and invoked inside another function body
/// still sees its definition-time variables, which are not in scope at the
/// call site.
#[test]
fn closure_passed_as_value_keeps_definition_scope() {
    let document = html::parse(LINKS_HTML).unwrap();
    let xpath = xpath::parse(
        r#"let $apply := function($f, $x) { $f($x) }
           return
             let $suffix := "!"
             return $apply(function($s) { $s || $suffix }, "hi")"#,
    )
    .unwrap();

    let items = xpath.apply(&document).unwrap();
    assert_eq!(items.len(), 1, "should return 1 item: {items:?}");
    assert_eq!(items[0], str_item("hi!"));
}

/// Closures passed to built-in higher-order functions (here `fn:for-each`)
/// capture their definition-time variables.
#[test]
fn closure_passed_to_builtin_higher_order_function() {
    let document = html::parse(LINKS_HTML).unwrap();
    let xpath = xpath::parse(
        "let $factor := 3
         return for-each((1, 2, 3), function($n) { $n * $factor })",
    )
    .unwrap();

    let items = xpath.apply(&document).unwrap();
    assert_eq!(items.len(), 3, "should return 3 items: {items:?}");
    assert_eq!(items[0], int_item(3));
    assert_eq!(items[1], int_item(6));
    assert_eq!(items[2], int_item(9));
}

// ============================================================
// Independent / repeated evaluation
// ============================================================

/// Repeated calls to the same closure each see the captured value.
#[test]
fn closure_evaluates_independently_on_repeated_calls() {
    let document = html::parse(LINKS_HTML).unwrap();
    let xpath = xpath::parse(
        "let $base := 100
         return
           let $f := function($x) { $x + $base }
           return ($f(1), $f(2), $f(3))",
    )
    .unwrap();

    let items = xpath.apply(&document).unwrap();
    assert_eq!(items.len(), 3, "should return 3 items: {items:?}");
    assert_eq!(items[0], int_item(101));
    assert_eq!(items[1], int_item(102));
    assert_eq!(items[2], int_item(103));
}

/// Rebinding a variable between two calls of the same closure does not
/// change what the closure sees.
#[test]
fn closure_is_stable_across_rebinding_between_calls() {
    let document = html::parse(LINKS_HTML).unwrap();
    let xpath = xpath::parse(
        r#"let $v := "first"
           return
             let $f := function() { $v }
             return
               let $a := $f()
               return
                 let $v := "second"
                 return
                   let $b := $f()
                   return ($a, $b)"#,
    )
    .unwrap();

    let items = xpath.apply(&document).unwrap();
    assert_eq!(items.len(), 2, "should return 2 items: {items:?}");
    assert_eq!(items[0], str_item("first"));
    assert_eq!(items[1], str_item("first"));
}

/// Two closures created from the same definition point with different
/// arguments capture independently of each other.
#[test]
fn closures_from_same_definition_capture_independently() {
    let document = html::parse(LINKS_HTML).unwrap();
    let xpath = xpath::parse(
        "let $make := function($x) { function() { $x } }
         return
           let $f1 := $make(1)
           return
             let $f2 := $make(2)
             return ($f1(), $f2(), $f1())",
    )
    .unwrap();

    let items = xpath.apply(&document).unwrap();
    assert_eq!(items.len(), 3, "should return 3 items: {items:?}");
    assert_eq!(items[0], int_item(1));
    assert_eq!(items[1], int_item(2));
    assert_eq!(items[2], int_item(1));
}

// ============================================================
// Error location
// ============================================================

/// An evaluation error inside a closure body identifies the function body
/// and the offending variable.
#[test]
fn closure_body_error_locates_function_body() {
    let document = html::parse(LINKS_HTML).unwrap();
    let xpath =
        xpath::parse("let $f := function() { $undefined-var } return $f()").unwrap();

    let err = xpath.apply(&document).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("inline function body"),
        "error should locate the inline function body: {msg}"
    );
    assert!(
        msg.contains("$undefined-var"),
        "error should name the undefined variable: {msg}"
    );
}

// ============================================================
// Legacy behavior (unchanged semantics)
// ============================================================

/// Parameters still shadow captured variables of the same name.
#[test]
fn parameter_shadows_captured_variable() {
    let document = html::parse(LINKS_HTML).unwrap();
    let xpath = xpath::parse(
        r#"let $x := "captured"
           return
             let $f := function($x) { $x }
             return $f("argument")"#,
    )
    .unwrap();

    let items = xpath.apply(&document).unwrap();
    assert_eq!(items.len(), 1, "should return 1 item: {items:?}");
    assert_eq!(items[0], str_item("argument"));
}

/// Functions that reference no variables behave as before.
#[test]
fn variable_free_functions_behave_as_before() {
    let document = html::parse(LINKS_HTML).unwrap();
    let xpath = xpath::parse(
        "let $add := function($a, $b) { $a + $b } return ($add(1, 2), $add(3, 4))",
    )
    .unwrap();

    let items = xpath.apply(&document).unwrap();
    assert_eq!(items.len(), 2, "should return 2 items: {items:?}");
    assert_eq!(items[0], int_item(3));
    assert_eq!(items[1], int_item(7));
}

/// Partial application captures variables referenced by concrete arguments.
#[test]
fn partial_application_captures_argument_variables() {
    let document = html::parse(LINKS_HTML).unwrap();
    let xpath = xpath::parse(
        r#"let $haystack := "hello world"
           return
             let $f := contains($haystack, ?)
             return
               let $haystack := "unrelated"
               return $f("world")"#,
    )
    .unwrap();

    let items = xpath.apply(&document).unwrap();
    assert_eq!(items.len(), 1, "should return 1 item: {items:?}");
    assert_eq!(items[0], XpathItem::AnyAtomicType(AnyAtomicType::Boolean(true)));
}

/// Wrong arity still errors for closures.
#[test]
fn closure_wrong_arity_still_errors() {
    let document = html::parse(LINKS_HTML).unwrap();
    let xpath =
        xpath::parse("let $x := 1 return let $f := function($a) { $a + $x } return $f(1, 2)")
            .unwrap();

    let result = xpath.apply(&document);
    assert!(result.is_err(), "should error on wrong arity");
}
