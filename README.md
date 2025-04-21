# Subprocess test

Introduces macro and some infrastructure that allows running your test code in separate subprocess
and verify it, if needed, inside test invocation. See crate documentation for details.

Some small examples:

```rust
subprocess_test::subprocess_test! {
    #[test]
    fn just_success() {
        let value = 1;
        assert_eq!(value + 1, 2);
    }
}
```

```rust
subprocess_test::subprocess_test! {
    #[test]
    fn one_plus_one() {
        println!("{}", 1 + 1);
    }
    verify |success, output| {
        assert!(success);
        assert_eq!(output, "2\n");
    }
}
```
