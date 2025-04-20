subprocess_test::subprocess_test! {
    #[test]
    fn simple_success_2() {
        let value = 1;
        assert_eq!(value + 1, 2);
    }

    #[test]
    fn name_collision() {
        println!("Two");
    }
    verify |success, output| {
        assert!(success);
        assert_eq!(output, "Two\n");
    }
}

mod submodule_tests {
    subprocess_test::subprocess_test! {
        #[test]
        fn submodule_test_2() {
            let value = 1;
            assert_eq!(value + 1, 2);
        }
    }
}
