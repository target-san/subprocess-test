//! This crate exposes single utility macro `subprocess_test`
//!
//! Macro generates test function code in such a way that first test code block
//! is executed in separate subprocess by re-invoking current test executable,
//! its output is captured, filtered a bit and then fed to verification function.
//! Test decides whether it's in normal or subprocess mode through marker environment variable
//!
//! Used when one needs to either run some test in isolation or validate test output
//! regardless of its proper completion, i.e. even if it aborts
//!
//! # Small examples
//!
//! ```rust
//! subprocess_test::subprocess_test! {
//!     #[test]
//!     fn just_success() {
//!         let value = 1;
//!         assert_eq!(value + 1, 2);
//!     }
//! }
//! ```
//!
//! ```rust
//! subprocess_test::subprocess_test! {
//!     #[test]
//!     fn one_plus_one() {
//!         println!("{}", 1 + 1);
//!     }
//!     verify |success, output| {
//!         assert!(success);
//!         assert_eq!(output, "2\n");
//!     }
//! }
//! ```
//!
//! # Usage
//!
//! ```rust
//! subprocess_test::subprocess_test! {
//!     // Mandatory test marker attribute; psrens are needed
//!     // only if some attribute parameters are specified.
//!     //
//!     // Please also note that this attribute must be first,
//!     // and its optional parameters must maintain order.
//!     // This is due to limitations of Rust's macro-by-example.
//!     #[test(     
//!         // Optionally specify name of environment variable used to mark subprocess mode.
//!         // Default name is "__TEST_RUN_SUBPROCESS__", so in very unprobable case case
//!         // you're getting name collision here, you can change it.
//!         env_var_name = "RUN_SUBPROCESS_ENV_VAR",
//!         // While subprocess is executed using `cargo test -q -- --nocapture`,
//!         // there's still some output from test harness.
//!         // To filter it out, test prints two boundary lines, in the beginning
//!         // and in the end of test's output, regardless if it succeeds or panics.
//!         // The default boundary line is "========================================",
//!         // so in rare case you expect conflict with actual test output, you can use
//!         // this parameter to set custom output boundary.
//!         output_boundary = "<><><><><><><><>",
//!     )]
//!     // Any other attributes are allowed, yet are optional
//!     #[ignore]
//!     // Test can have any valid name, same as normal test function
//!     fn dummy() {
//!         // This block is intended to generate test output,
//!         // although it can be used as normal test body
//!         println!("Foo");
//!         eprintln!("Bar");
//!     }
//!     // `verify` block is optional;
//!     // if absent, it's substituted with block which just asserts that subprocess succeeded
//!     // and prints test output in case of failure
//!     //
//!     // Parameters can be any names. Their meanings:
//!     // * `success` - boolean which is `true` if subprocess succeeded
//!     // * `output` - subprocess output collected into string, both `stdout` and `stderr`
//!     verify |success, output| {
//!         // This block is run as normal part of test and in general must succeed
//!         assert!(success);
//!         assert_eq!(output, "Foo\nBar\n");
//!     }
//! }
//! ```
//!
//! # Limitations
//!
//! Macro doesn't work well with `#[should_panic]` attribute because there's only one test function
//! which runs in two modes. If subprocess test panics as expected, subprocess succeeds, and
//! `verify` block must panic too. Just use `verify` block and do any checks you need there.
use std::borrow::Cow;
use std::env::{args_os, var_os};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::process::{Command, Stdio};

use defer::defer;
use tempfile::tempfile;
/// Implementation of `subprocess_test` macro. See crate-level documentation for details and usage examples
#[macro_export]
macro_rules! subprocess_test {
    (
        $(
            #[test $((
                $(env_var_name = $subp_var_name:literal $(,)?)?
                $(output_boundary = $subp_output_boundary:literal $(,)?)?
            ))?]
            $(#[$attrs:meta])*
            fn $test_name:ident () $test_block:block
            $(verify |$success_param:ident, $stdout_param:ident| $verify_block:block)?
        )*
    ) => {
        $(
            #[test]
            $(#[$attrs])*
            fn $test_name() {
                // NB: adjust full path to runner function whenever this code is moved to other module
                $crate::run_subprocess_test(
                    concat!(module_path!(), "::", stringify!($test_name)),
                    $crate::subprocess_test! {
                        @tokens_or_default { $($(Some($subp_var_name))?)? }
                        or { None }
                    },
                    $crate::subprocess_test! {
                        @tokens_or_default { $($(Some($subp_output_boundary))?)? }
                        or { None }
                    },
                    || $test_block,
                    $crate::subprocess_test! {
                        @tokens_or_default {
                            $(|$success_param, $stdout_param| $verify_block)?
                        } or {
                            // NB: we inject closure here, to make panic report its location
                            // at macro expansion
                            |success, output| {
                                if !success {
                                    eprintln!("{output}");
                                    // In case panic location will point to whole macro start,
                                    // you'll get at least test name
                                    panic!("Test {} subprocess failed", stringify!($test_name));
                                }
                            }
                        }
                    },
                );
            }
        )*
    };
    (
        @tokens_or_default { $($tokens:tt)+ } or { $($_:tt)* }
    ) => {
        $($tokens)+
    };
    (
        @tokens_or_default { } or { $($tokens:tt)* }
    ) => {
        $($tokens)*
    };
}

#[doc(hidden)]
pub fn run_subprocess_test(
    full_test_name: &str,
    var_name: Option<&str>,
    boundary: Option<&str>,
    test_fn: impl FnOnce(),
    verify_fn: impl FnOnce(bool, String),
) {
    const DEFAULT_SUBPROCESS_ENV_VAR_NAME: &str = "__TEST_RUN_SUBPROCESS__";
    const DEFAULT_OUTPUT_BOUNDARY: &str = "\n========================================\n";

    let full_test_name = &full_test_name[full_test_name
        .find("::")
        .expect("Full test path is expected to include crate name")
        + 2..];
    let var_name = var_name.unwrap_or(DEFAULT_SUBPROCESS_ENV_VAR_NAME);
    let boundary: Cow<'static, str> = if let Some(boundary) = boundary {
        format!("\n{boundary}\n").into()
    } else {
        DEFAULT_OUTPUT_BOUNDARY.into()
    };
    // If test phase is requested, execute it and bail immediately
    if var_os(var_name).is_some() {
        print!("{boundary}");
        // We expect that in case of panic we'll get test harness footer,
        // but in case of abort we won't get it, so finisher won't be needed
        defer! { print!("{boundary}") };
        test_fn();
        return;
    }
    // Otherwise, perform main runner phase.
    // Just run same executable but with different options
    let (tmpfile, stdout, stderr) = tmpfile_buffer();
    let exe_path = args_os().next().expect("Test executable path not found");

    let success = Command::new(exe_path)
        .args([
            "--include-ignored",
            "--nocapture",
            "--quiet",
            "--exact",
            "--test",
        ])
        .arg(full_test_name)
        .env(var_name, "")
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .status()
        .expect("Failed to execute test as subprocess")
        .success();

    let mut output = read_file(tmpfile);
    let boundary_at = output
        .find(&*boundary)
        .expect("Subprocess output should always include at least one boundary");

    output.replace_range(..(boundary_at + boundary.len()), "");

    if let Some(boundary_at) = output.find(&*boundary) {
        output.truncate(boundary_at);
    }

    verify_fn(success, output);
}

fn tmpfile_buffer() -> (File, File, File) {
    let file = tempfile().expect("Failed to create temporary file for subprocess output");
    let stdout = file
        .try_clone()
        .expect("Failed to clone tmpfile descriptor");
    let stderr = file
        .try_clone()
        .expect("Failed to clone tmpfile descriptor");

    (file, stdout, stderr)
}

fn read_file(mut file: File) -> String {
    file.seek(SeekFrom::Start(0))
        .expect("Rewind to start failed");

    let mut buffer = String::new();
    file.read_to_string(&mut buffer)
        .expect("Failed to read file into buffer");

    buffer
}

subprocess_test! {
    #[test]
    fn name_collision() {
        println!("One");
    }
    verify |success, output| {
        assert!(success);
        assert_eq!(output, "One\n");
    }

    #[test]
    fn simple_success() {
        let value = 1;
        assert_eq!(value + 1, 2);
    }

    #[test]
    fn simple_verify() {
        println!("Simple verify test");
    }
    verify |success, output| {
        assert!(success);
        assert_eq!(output, "Simple verify test\n");
    }

    #[test]
    fn simple_failure() {
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
    fn custom_var() {
        assert!(var_os("__CUSTOM_SUBPROCESS_VAR__").is_some());
    }

    #[test(
        output_boundary = "!!!!!!!!!!!!!!!!"
    )]
    fn custom_boundary() {
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
    fn should_panic_test() {
        panic!("Oopsie!");
    }
    verify |success, _output| {
        assert!(!success, "Correct result should cause panic");
    }

    #[test]
    fn test_aborts() {
        println!("Banana");
        eprintln!("Mango");
        std::process::abort();
    }
    verify |success, output| {
        assert!(!success);
        assert_eq!(output, "Banana\nMango\n");
    }
}

#[cfg(test)]
mod submodule_tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    // Used to check that only single test is run per subprocess
    static COMMON_PREFIX_COUNTER: AtomicUsize = AtomicUsize::new(0);

    subprocess_test! {
        #[test]
        fn submodule_test() {
            let value = 1;
            assert_eq!(value + 1, 2);
        }

        #[test]
        fn common_prefix() {
            print!("One");
            COMMON_PREFIX_COUNTER.fetch_add(1, Ordering::Relaxed);
            assert_eq!(COMMON_PREFIX_COUNTER.load(Ordering::Relaxed), 1);
        }
        verify |success, output| {
            assert!(success);
            assert_eq!(output, "One");
        }

        #[test]
        fn common_prefix_2() {
            print!("Two");
            COMMON_PREFIX_COUNTER.fetch_add(1, Ordering::Relaxed);
            assert_eq!(COMMON_PREFIX_COUNTER.load(Ordering::Relaxed), 1);
        }
        verify |success, output| {
            assert!(success);
            assert_eq!(output, "Two");
        }
    }

    mod common_prefix {
        subprocess_test! {
            #[test]
            fn inner() {
                print!("Three");
                super::COMMON_PREFIX_COUNTER.fetch_add(1, super::Ordering::Relaxed);
                assert_eq!(super::COMMON_PREFIX_COUNTER.load(super::Ordering::Relaxed), 1);
            }
            verify |success, output| {
                assert!(success);
                assert_eq!(output, "Three");
            }
        }
    }
}
