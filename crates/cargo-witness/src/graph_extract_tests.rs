use super::*;

#[test]
fn extract_pub_fn() {
    let source = r#"
        pub fn hello(name: &str) -> String {
            format!("Hello, {name}")
        }

        fn private_helper() -> bool {
            true
        }
    "#;
    let items = extract_pub_items("test.rs", source);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].kind, "fn");
    assert_eq!(items[0].name, "hello");
    assert!(items[0].signature.contains("fn hello"));
}

#[test]
fn extract_pub_struct() {
    let source = r#"
        pub struct Config {
            pub name: String,
            port: u16,
        }

        struct Internal {
            data: Vec<u8>,
        }
    "#;
    let items = extract_pub_items("test.rs", source);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].kind, "struct");
    assert_eq!(items[0].name, "Config");
}

#[test]
fn extract_pub_enum() {
    let source = r#"
        pub enum Status {
            Active,
            Inactive,
            Pending,
        }
    "#;
    let items = extract_pub_items("test.rs", source);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].kind, "enum");
    assert_eq!(items[0].name, "Status");
    assert!(items[0].signature.contains("3 variants"));
}

#[test]
fn extract_pub_trait() {
    let source = r#"
        pub trait Handler {
            fn handle(&self, request: &Request) -> Response;
            fn name(&self) -> &str;
        }
    "#;
    let items = extract_pub_items("test.rs", source);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].kind, "trait");
    assert_eq!(items[0].name, "Handler");
    assert!(items[0].signature.contains("2 methods"));
}

#[test]
fn extract_impl_pub_methods() {
    let source = r#"
        pub struct Foo;

        impl Foo {
            pub fn bar(&self) -> i32 { 42 }
            fn private(&self) {}
        }
    "#;
    let items = extract_pub_items("test.rs", source);
    assert_eq!(items.len(), 2);
    let fn_items: Vec<_> = items.iter().filter(|i| i.kind == "fn").collect();
    assert_eq!(fn_items.len(), 1);
    assert_eq!(fn_items[0].name, "bar");
}

#[test]
fn private_items_excluded() {
    let source = r#"
        fn private_fn() -> bool { true }
        struct PrivateStruct { x: i32 }
        enum PrivateEnum { A, B }
    "#;
    let items = extract_pub_items("test.rs", source);
    assert!(items.is_empty());
}

#[test]
fn deterministic_ordering() {
    let source = r#"
        pub fn zebra() {}
        pub fn alpha() {}
        pub struct Middle;
    "#;
    let items = extract_pub_items("test.rs", source);
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].name, "alpha");
    assert_eq!(items[1].name, "zebra");
    assert_eq!(items[2].name, "Middle");
}
