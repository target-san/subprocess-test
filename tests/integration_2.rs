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
    verify |code, output| {
        assert_eq!(code, 0);
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
