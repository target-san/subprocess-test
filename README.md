# Subprocess test

Introduces macro and some infrastructure that allows running your test code in separate subprocess
and verify it, if needed, inside test invocation, like normal. See crate documentation for details.
Small example:

```rust
subprocess_test! {
    #[test]
    fn simple_test() {
        let value = 1;
        assert_eq!(value + 1, 2);
    }
    verify |code, output| {
        assert_eq!(code, 0);
        assert_eq!(output, "");
    }
}
```
