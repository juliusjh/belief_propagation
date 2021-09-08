#![macro_export]

#[cfg(feature = "debug_output")]
macro_rules! debug_print
{
    ($( $args:expr ),*) => { println!( $( $args ),* ); }
}

#[cfg(not(feature = "debug_output"))]
macro_rules! debug_print {
    ($( $args:expr ),*) => {};
}

#[cfg(feature = "info_output")]
macro_rules! info_print
{
    ($( $args:expr ),*) => { println!( $( $args ),* ); }
}

#[cfg(not(feature = "info_output"))]
macro_rules! info_print {
    ($( $args:expr ),*) => {};
}

#[cfg(feature = "thread_output")]
macro_rules! thread_print
{
    ($( $args:expr ),*) => { println!( $( $args ),* ); }
}

#[cfg(not(feature = "thread_output"))]
macro_rules! thread_print {
    ($( $args:expr ),*) => {};
}
