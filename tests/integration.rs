subprocess_test::subprocess_test! {
    #[test]
    fn integration_simple_success() {
        let value = 1;
        assert_eq!(value + 1, 2);
    }

    #[test]
    fn integration_simple_verify() {
        println!("Simple verify test");
    }
    verify |success, output| {
        assert!(success);
        assert_eq!(output, "Simple verify test\n");
    }

    #[test]
    fn integration_simple_failure() {
        panic!("Oopsie!");
    }
    verify |success, output| {
        assert!(!success);
        // Note that panic output contains stacktrace and other stuff
        assert!(output.contains("Oopsie!\n"));
    }

    #[test(
        env_var_name = "__CUSTOM_SUBPROCESS_VAR__"
    )]
    fn integration_custom_var() {
        assert!(std::env::var_os("__CUSTOM_SUBPROCESS_VAR__").is_some());
    }

    #[test(
        output_boundary = "!!!!!!!!!!!!!!!!"
    )]
    fn integration_custom_boundary() {
        println!("One");
        println!("Two");
        println!("\n!!!!!!!!!!!!!!!!\n");
        println!("Three");
    }
    verify |success, output| {
        assert!(success);
        assert_eq!(output, "One\nTwo\n");
    }

    #[test]
    #[should_panic]
    fn integration_should_panic_test() {
        panic!("Oopsie!");
    }
    verify |success, _output| {
        assert!(!success, "Correct result should cause panic");
    }
}

mod submodule_tests {
    subprocess_test::subprocess_test! {
        #[test]
        fn submodule_test() {
            let value = 1;
            assert_eq!(value + 1, 2);
        }
    }
}
