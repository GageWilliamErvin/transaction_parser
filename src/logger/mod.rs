use std::io::Write;

// Since this is &str, a::b::log and a::c::log would not cause duplication of the string.
//  That isn't necessarily true of other data types.
//  str cannot be static or const directly for now because it is unsized which is why it is an exception.
const WARNING_PREFIX: &'static str = "Warning! ";
const ERROR_PREFIX: &'static str = "ERROR! ";

pub fn warning(msg: &str) {
    if let Err(err) = std::io::stderr().write_all(format!( "\n{} {}\n", WARNING_PREFIX, msg).as_bytes()) {
        panic!("An error occured while trying to print a warning: {}", err);
    };
}

pub fn error(msg: &str) {
    if let Err(err) = std::io::stderr().write_all(format!( "\n{} {}\n", ERROR_PREFIX, msg).as_bytes()) {
        panic!("An error occured while trying to print an error: {}", err);
    };
}
