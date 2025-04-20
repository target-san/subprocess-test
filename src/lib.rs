use std::borrow::Cow;
use std::env::var_os;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::process::{Command, Stdio};

use defer::defer;
use tempfile::tempfile;
/// Launches piece of test code as separate subprocess, collects all its output
/// and then runs validation code against it
///
/// Used when one needs to either run some test in isolation or validate test output
/// regardless of its proper completion, i.e. even if it aborts
///
/// # Usage
/// ```rust,ignore
/// subprocess_test! {
///     #[test]     // Mandatory test attribute
///     #[ignore]   // Any other attributes are allowed, yet are optional
///     fn dummy() {    // Test can have any valid name
///         // This block is intended to generate test output,
///         // although it can be used as normal test body
///         println!("Foo");
///         eprintln!("Bar");
///     }
///     // `verify` block is optional;
///     // if absent, it's substituted with block which just asserts that exit code was 0
///     verify |code, output| { // Parameter names can be any valid identifiers
///         // This block is run as normal part of test and in general must succeed
///         assert_eq!(code, 0);
///         assert_eq!(output, "Foo\nBar\n");
///     }
/// }
/// ```
///
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
            $(verify |$status_param:ident, $stdout_param:ident| $verify_block:block)?
        )*
    ) => {
        $(
            #[test]
            $(#[$attrs])*
            fn $test_name() {
                // NB: adjust full path to runner function whenever this code is moved to other module
                $crate::run_subprocess_test(
                    env!("CARGO_PKG_NAME"),
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
                            $(|$status_param, $stdout_param| $verify_block)?
                        } or {
                            // NB: we inject closure here, to make panic report its location
                            // at macro expansion
                            |exit_code, output| {
                                if exit_code != 0 {
                                    eprintln!("{output}");
                                    panic!("Test process failed with {exit_code}");
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
    package_name: &str,
    full_test_name: &str,
    var_name: Option<&str>,
    boundary: Option<&str>,
    test_fn: impl FnOnce(),
    verify_fn: impl FnOnce(i32, String),
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
    let cargo = var_os("CARGO").unwrap_or("cargo".into());
    // If test phase is requested, execute it and bail immediately
    if var_os(var_name).is_some() {
        print!("{boundary}");
        // We expect that in case of panic we'll get test harness footer,
        // but in case of abort we won't get it, so finisher won't be needed
        defer! { print!("{boundary}") };
        test_fn();
        return;
    }
    // Otherwise, perform main runner phase
    // Note that we don't perform separate compilation phase,
    // as we always run this code as test
    let (tmpfile, stdout, stderr) = tmpfile_buffer();

    let exit_code = Command::new(&cargo)
        .args(["test", "-q", "-p"])
        .arg(package_name)
        .args(["--", "--include-ignored", "--nocapture", "--test"])
        .arg(full_test_name)
        .env(var_name, "")
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .status()
        .expect("Failed to execute test in binary output mode")
        .code()
        .expect("Test subprocess should've completed and got its status code");

    let mut output = read_file(tmpfile);
    let boundary_at = output
        .find(&*boundary)
        .expect("Test mode output should always include at least one boundary");

    output.replace_range(..(boundary_at + boundary.len()), "");

    if let Some(boundary_at) = output.find(&*boundary) {
        output.truncate(boundary_at);
    }

    verify_fn(exit_code, output);
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
    fn simple_success() {
        let value = 1;
        assert_eq!(value + 1, 2);
    }

    #[test]
    fn simple_verify() {
        println!("Simple verify test");
    }
    verify |code, output| {
        assert_eq!(code, 0);
        assert_eq!(output, "Simple verify test\n");
    }

    #[test]
    fn simple_failure() {
        panic!("Oopsie!");
    }
    verify |code, output| {
        assert_ne!(code, 0);
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
    verify |code, output| {
        assert_eq!(code, 0);
        assert_eq!(output, "One\nTwo\n");
    }

    #[test]
    #[should_panic]
    fn should_panic_test() {
        panic!("Oopsie!");
    }
    verify |exit_code, _output| {
        assert_ne!(exit_code, 0, "Correct result should cause panic");
    }
}
